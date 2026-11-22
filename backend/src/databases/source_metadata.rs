use super::*;

#[derive(Debug)]
pub struct SourceMetadata {
    pub played_on: Option<NaiveDate>,
    pub tournament: Option<String>,
    pub round: Option<String>,
    pub place: Option<String>,
    pub result_state: String,
    pub white_score: Option<i32>,
    pub black_score: Option<i32>,
}

/// Only explicit SGF properties are trusted. A game RE alone is not a match result.
pub fn source_metadata(sgf: &str) -> SourceMetadata {
    let mut out = SourceMetadata {
        played_on: None,
        tournament: None,
        round: None,
        place: None,
        result_state: "unknown".into(),
        white_score: None,
        black_score: None,
    };
    let Ok(trees) = crate::import::sgf::parse_trees(sgf) else {
        return out;
    };
    let Some(first) = trees.first().and_then(|t| t.first()) else {
        return out;
    };
    let prop = |key: &str, limit: usize| {
        first
            .get(key)
            .and_then(|v| v.first())
            .filter(|v| !v.trim().is_empty() && v.chars().count() <= limit)
            .cloned()
    };
    out.played_on = prop("DT", 10)
        .filter(|v| v.len() == 10)
        .and_then(|v| NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok());
    out.tournament = prop("EV", 240);
    out.round = prop("RO", 120);
    out.place = prop("PC", 240);
    let Some(last) = trees.last().and_then(|t| t.first()) else {
        return out;
    };
    let mi = |key: &str| {
        last.get("MI")
            .and_then(|v| v.iter().find_map(|v| v.strip_prefix(key)))
            .and_then(|v| v.parse::<i32>().ok())
            .filter(|v| *v >= 0)
    };
    let (Some(length), Some(mut white), Some(mut black)) = (mi("length:"), mi("ws:"), mi("bs:"))
    else {
        return out;
    };
    if length == 0 {
        return out;
    }
    if let Some(result) = last.get("RE").and_then(|v| v.first()) {
        let Some((side, points)) = result.split_once('+') else {
            return out;
        };
        let points = points.strip_suffix('R').unwrap_or(points);
        let Ok(points) = points.parse::<i32>() else {
            return out;
        };
        if points <= 0 {
            return out;
        }
        match side {
            "W" => {
                let Some(n) = white.checked_add(points) else {
                    return out;
                };
                white = n
            }
            "B" => {
                let Some(n) = black.checked_add(points) else {
                    return out;
                };
                black = n
            }
            _ => return out,
        }
    }
    if prop("AP", 32).as_deref() == Some(crate::import::ogxm::SGF_APPLICATION) {
        // OGXM §5.4.6 caps the match score at the target, even if the
        // final game's award was larger than the remaining points.
        white = white.min(length);
        black = black.min(length);
    }
    if white < length && black < length {
        // A NeonDojo forfeit awards the remaining match points without a game result.
        match prop(crate::import::sgf::MATCH_FORFEIT_PROPERTY, 1).as_deref() {
            Some("W") => black = length,
            Some("B") => white = length,
            _ => {}
        }
    }
    out.white_score = Some(white);
    out.black_score = Some(black);
    out.result_state = if white >= length || black >= length {
        "completed"
    } else {
        "ongoing"
    }
    .into();
    out
}

/// Bounded startup backfill; repeat until it returns zero. Manual metadata is preserved.
pub async fn backfill_source_metadata(pool: &PgPool) -> anyhow::Result<usize> {
    let mut tx = pool.begin().await?;
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM matches WHERE NOT source_metadata_loaded OR native_match_context_pending
         ORDER BY id LIMIT 100 FOR UPDATE SKIP LOCKED",
    )
    .fetch_all(&mut *tx)
    .await?;

    for id in &ids {
        // Fetch sources one at a time: a batch must not retain 100 large originals.
        let (sgf, format, original, version, source_loaded, context_pending):
            (String, String, Vec<u8>, i64, bool, bool) = sqlx::query_as(
                "SELECT CASE WHEN NOT source_metadata_loaded AND metadata_version=0 THEN sgf ELSE '' END,
                        format, CASE WHEN metadata_version=0 AND format IN ('xg','bgf')
                                     THEN original ELSE ''::bytea END,
                        metadata_version, source_metadata_loaded, native_match_context_pending
                 FROM matches WHERE id=$1",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

        if version == 0 {
            let match_id = *id;
            let (legacy, context) = tokio::task::spawn_blocking(move || {
                let legacy = (!source_loaded).then(|| {
                    let mut metadata = source_metadata(&sgf);
                    // Recover outcomes missing from older canonical SGF.
                    let canonical = match format.as_str() {
                        "xg" => crate::import::xg_to_sgf(&original).ok(),
                        "bgf" => crate::import::bgf_to_sgf(&original).ok(),
                        _ => None,
                    };
                    if let Some(canonical) = canonical {
                        let source = source_metadata(&canonical);
                        metadata.result_state = source.result_state;
                        metadata.white_score = source.white_score;
                        metadata.black_score = source.black_score;
                    }
                    metadata
                });
                let mut context = if context_pending {
                    crate::import::match_context::from_original(&format, &original)
                        .unwrap_or_else(|error| {
                            tracing::warn!(%match_id, %format, %error, "Could not recover match context from original");
                            Default::default()
                        })
                } else {
                    Default::default()
                };
                if let Some(metadata) = &legacy {
                    context.played_on = context.played_on.or(metadata.played_on);
                    context.event = context.event.or_else(|| metadata.tournament.clone());
                    context.round = context.round.or_else(|| metadata.round.clone());
                    context.place = context.place.or_else(|| metadata.place.clone());
                }
                (legacy, context)
            })
            .await?;

            sqlx::query(
                "UPDATE matches
                 SET played_on=COALESCE(played_on,$2),tournament=COALESCE(tournament,$3),
                     round=COALESCE(round,$4),place=COALESCE(place,$5),
                     result_state=COALESCE($6,result_state),
                     white_score=CASE WHEN $6 IS NULL THEN white_score ELSE $7 END,
                     black_score=CASE WHEN $6 IS NULL THEN black_score ELSE $8 END,
                     metadata_version=metadata_version+CASE WHEN $9 AND (
                         (played_on IS NULL AND $2 IS NOT NULL)
                         OR (tournament IS NULL AND $3 IS NOT NULL)
                         OR (round IS NULL AND $4 IS NOT NULL)
                         OR (place IS NULL AND $5 IS NOT NULL)) THEN 1 ELSE 0 END,
                     source_metadata_loaded=true,native_match_context_pending=false
                 WHERE id=$1",
            )
            .bind(id)
            .bind(context.played_on)
            .bind(context.event)
            .bind(context.round)
            .bind(context.place)
            .bind(legacy.as_ref().map(|m| m.result_state.as_str()))
            .bind(legacy.as_ref().and_then(|m| m.white_score))
            .bind(legacy.as_ref().and_then(|m| m.black_score))
            .bind(context_pending)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query("UPDATE matches SET source_metadata_loaded=true,native_match_context_pending=false WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(ids.len())
}
