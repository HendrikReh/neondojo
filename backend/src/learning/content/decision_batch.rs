use super::*;
use std::collections::HashMap;

type DecisionCards = HashMap<MistakeReference, Result<CardContent, String>>;
type ImportedRefs<'a> = HashMap<Uuid, Vec<&'a MistakeReference>>;

/// Missing sources stay absent. An invalid source marks just the supplied
/// references as unavailable. Transport and database failures abort the batch.
fn record_error<'a>(
    output: &mut DecisionCards,
    references: impl IntoIterator<Item = &'a MistakeReference>,
    error: ApiError,
) -> ApiResult<()> {
    match error.0 {
        StatusCode::UNPROCESSABLE_ENTITY => {
            for reference in references {
                output.insert(reference.clone(), Err(error.1.error.clone()));
            }
            Ok(())
        }
        StatusCode::NOT_FOUND => Ok(()),
        _ => Err(error),
    }
}

/// Resolves the distinct references. The route emits the items in request order,
/// including duplicates, after it has written the corresponding cards.
pub(in crate::learning) async fn decision_cards(
    pool: &PgPool,
    references: &[MistakeReference],
) -> ApiResult<DecisionCards> {
    let mut native: BTreeMap<Uuid, BTreeMap<i32, Vec<&MistakeReference>>> = BTreeMap::new();
    let mut imported = ImportedRefs::new();
    for reference in references {
        if let Some(job) = reference.job_id {
            native
                .entry(job)
                .or_default()
                .entry(reference.game_no)
                .or_default()
                .push(reference);
        } else if let Some(id) = reference.imported_analysis_id {
            imported.entry(id).or_default().push(reference);
        }
    }
    let mut output = DecisionCards::new();
    for (job, games) in native {
        let analysis = match MatchCardSource::load(pool, job).await {
            Ok(analysis) => analysis,
            Err(error) => {
                record_error(&mut output, games.values().flatten().copied(), error)?;
                continue;
            }
        };
        for (game_no, requested) in games {
            let decisions: BTreeSet<i32> = requested.iter().map(|r| r.decision_no).collect();
            match analysis
                .cards(pool, Some(game_no), move |d| {
                    decisions.contains(&d.decision_no)
                })
                .await
            {
                Ok(cards) => {
                    for (decision_no, card) in cards {
                        output.insert(
                            MistakeReference {
                                job_id: Some(job),
                                imported_analysis_id: None,
                                game_no,
                                decision_no,
                            },
                            card,
                        );
                    }
                }
                Err(error) => record_error(&mut output, requested, error)?,
            }
        }
    }
    if imported.is_empty() {
        return Ok(output);
    }

    let owners: Vec<(Uuid, Uuid, i32)> =
        sqlx::query_as("SELECT id, match_uuid, game_no FROM imported_analyses WHERE id = ANY($1)")
            .bind(imported.keys().copied().collect::<Vec<_>>())
            .fetch_all(pool)
            .await
            .map_err(anyhow::Error::from)?;
    for ((match_id, game_no), ids) in imported_groups(owners) {
        let result = imported_decision_cards(pool, match_id, Some(game_no), None, Some(&ids)).await;
        collect_imported_group(&mut output, &imported, &ids, game_no, result)?;
    }
    Ok(output)
}

fn imported_groups(owners: Vec<(Uuid, Uuid, i32)>) -> BTreeMap<(Uuid, i32), Vec<Uuid>> {
    let mut groups: BTreeMap<(Uuid, i32), Vec<Uuid>> = BTreeMap::new();
    for (id, match_id, game_no) in owners {
        groups.entry((match_id, game_no)).or_default().push(id);
    }
    groups
}

fn collect_imported_group(
    output: &mut DecisionCards,
    wanted: &ImportedRefs<'_>,
    ids: &[Uuid],
    game_no: i32,
    result: ApiResult<Vec<(Uuid, Result<CardContent, String>)>>,
) -> ApiResult<()> {
    let cards = match result {
        Ok(cards) => cards,
        Err(error) => {
            let owned = ids
                .iter()
                .filter_map(|id| wanted.get(id))
                .flatten()
                .copied()
                .filter(|r| r.game_no == game_no);
            return record_error(output, owned, error);
        }
    };
    for (id, card) in cards {
        let decision = card
            .as_ref()
            .ok()
            .and_then(|content| content.source.decision_no);
        for &reference in wanted.get(&id).into_iter().flatten() {
            if reference.game_no == game_no && decision.is_none_or(|n| n == reference.decision_no) {
                output.insert(reference.clone(), card.clone());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn check_imported_failure(good_match: Uuid, bad_match: Uuid) {
        let good_id = Uuid::new_v4();
        let bad_id = Uuid::new_v4();
        let good = MistakeReference {
            job_id: None,
            imported_analysis_id: Some(good_id),
            game_no: 1,
            decision_no: 1,
        };
        let bad = MistakeReference {
            imported_analysis_id: Some(bad_id),
            ..good.clone()
        };
        let absent = MistakeReference {
            imported_analysis_id: Some(Uuid::new_v4()),
            ..good.clone()
        };
        let wanted = HashMap::from([
            (good_id, vec![&good]),
            (bad_id, vec![&bad]),
            (absent.imported_analysis_id.unwrap(), vec![&absent]),
        ]);

        let card: CardContent = serde_json::from_value(json!({
            "kind":"checker","players":["A","B"],"prompt":"","played":"8/5",
            "position":{"id":Uuid::nil(),"match_uuid":good_match,"ordinal":1,"game_no":1,"ply":1,"player":0,"dice":[3,1],"board":[],"position_id":"","gnubg_match_id":"","notation":""},
            "source":{"kind":"imported","match_id":good_match,"game_no":1,"decision_no":1,"imported_analysis_id":good_id},
            "solution":{"engine":"XG","candidates":[]}
        })).unwrap();
        let mut output = HashMap::new();
        let owners = vec![(good_id, good_match, 1), (bad_id, bad_match, 1)];
        for ((match_id, game), ids) in imported_groups(owners) {
            let result = if match_id == good_match {
                Ok(vec![(good_id, Ok(card.clone()))])
            } else {
                Err(ApiError::invalid("failed imported game"))
            };
            assert!(collect_imported_group(&mut output, &wanted, &ids, game, result).is_ok());
        }
        assert!(output[&good].is_ok());
        assert_eq!(output[&bad].as_ref().unwrap_err(), "failed imported game");
        assert!(!output.contains_key(&absent));
    }

    #[test]
    fn imported_group_failure_before_success() {
        check_imported_failure(Uuid::from_u128(2), Uuid::from_u128(1));
    }

    #[test]
    fn imported_group_failure_after_success() {
        check_imported_failure(Uuid::from_u128(1), Uuid::from_u128(2));
    }
}
