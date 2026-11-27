use super::*;

pub(crate) const NOCUBE_SIGNATURE_SUFFIX: &str = ":nocube";

/// SQL expression matching [`card_signature`], appending `:nocube` when
/// `$nocube` holds.
macro_rules! card_signature_sql {
    ($position_id:expr, $match_id:expr, $kind:expr, $nocube:expr) => {
        concat!(
            "(",
            $position_id,
            " || ':' || ",
            $match_id,
            " || ':' || ",
            $kind,
            " || CASE WHEN ",
            $nocube,
            " THEN ':nocube' ELSE '' END)"
        )
    };
}
pub(crate) use card_signature_sql;

pub(super) fn card_signature(content: &CardContent) -> String {
    let mut signature = format!(
        "{}:{}:{}",
        content.position.position_id, content.position.gnubg_match_id, content.kind
    );
    if content.position.cube_enabled == Some(false) {
        signature.push_str(NOCUBE_SIGNATURE_SUFFIX);
    }
    signature
}

// Bulk selection reports unavailable historical native results explicitly, while
// retaining the existing single-source endpoint and its source priority unchanged.
pub(super) fn has_usable_checker_solution(solution: &CardSolution) -> bool {
    if let Some(decision) = &solution.decision {
        !decision.best.trim().is_empty()
            && match decision.evaluation_status.as_str() {
                "forced" => decision.equity_loss == 0.0,
                "evaluated" => {
                    decision.played_equity.is_some_and(f64::is_finite)
                        && decision.best_equity.is_some_and(f64::is_finite)
                }
                _ => false,
            }
    } else {
        solution
            .candidates
            .first()
            .is_some_and(|candidate| !candidate.notation.trim().is_empty())
    }
}

pub(super) async fn write_match_cards(
    pool: &PgPool,
    deck_id: Uuid,
    contents: BTreeMap<String, CardContent>,
    tags: &BTreeSet<String>,
) -> ApiResult<AddCardsResult> {
    Ok(write_cards(pool, deck_id, contents, tags).await?.summary())
}

pub(super) struct CardWrite {
    pub(super) id: Uuid,
    pub(super) added: bool,
    pub(super) changed: bool,
}

pub(super) struct WrittenCards {
    pub(super) cards: BTreeMap<String, CardWrite>,
    pub(super) total: i64,
}

impl WrittenCards {
    pub(super) fn summary(&self) -> AddCardsResult {
        let added = self.cards.values().filter(|card| card.added).count() as i64;
        AddCardsResult {
            added,
            updated: self.cards.len() as i64 - added,
            total: self.total,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum CardConflictPolicy {
    UpdatePayload,
    PreserveExistingPayload,
}

pub(super) async fn write_cards(
    pool: &PgPool,
    deck_id: Uuid,
    unique: BTreeMap<String, CardContent>,
    tags: &BTreeSet<String>,
) -> ApiResult<WrittenCards> {
    let entries = unique
        .into_iter()
        .map(|(signature, content)| (signature, (content, tags.clone())))
        .collect();
    write_cards_with_policy(pool, deck_id, entries, CardConflictPolicy::UpdatePayload).await
}

pub(super) async fn write_cards_with_policy(
    pool: &PgPool,
    deck_id: Uuid,
    unique: BTreeMap<String, (CardContent, BTreeSet<String>)>,
    policy: CardConflictPolicy,
) -> ApiResult<WrittenCards> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    // Serializing deck mutations keeps the size bound valid under concurrent requests.
    sqlx::query("SELECT id FROM decks WHERE id=$1 FOR UPDATE")
        .bind(deck_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    let mut cards = BTreeMap::new();
    // Byte-sorted signatures match export's C collation and prevent opposite source
    // orders in concurrent imports from acquiring shared-card locks in reverse order.
    for (signature, (c, tags)) in unique {
        let payload = serde_json::to_value(&c).map_err(anyhow::Error::from)?;
        let new_id = Uuid::new_v4();
        let (card_id, changed) = match policy {
            CardConflictPolicy::UpdatePayload => {
                // ON CONFLICT locks even unchanged rows, so a later lookup observes the exact
                // shared card protected by this transaction. No FSRS fields are overwritten.
                let changed_id: Option<Uuid> = sqlx::query_scalar("INSERT INTO learning_cards(id,signature,payload) VALUES($1,$2,$3) ON CONFLICT(signature) DO UPDATE SET payload=EXCLUDED.payload,version=learning_cards.version+1,updated_at=now() WHERE learning_cards.payload IS DISTINCT FROM EXCLUDED.payload RETURNING id")
                    .bind(new_id).bind(&signature).bind(payload).fetch_optional(&mut *tx).await.map_err(anyhow::Error::from)?;
                let card_id = if let Some(card_id) = changed_id {
                    card_id
                } else {
                    sqlx::query_scalar("SELECT id FROM learning_cards WHERE signature=$1")
                        .bind(&signature)
                        .fetch_one(&mut *tx)
                        .await
                        .map_err(anyhow::Error::from)?
                };
                (card_id, changed_id.is_some_and(|id| id != new_id))
            }
            CardConflictPolicy::PreserveExistingPayload => {
                // Portable imports retain any richer local source provenance on existing cards.
                let inserted_id: Option<Uuid> = sqlx::query_scalar("INSERT INTO learning_cards(id,signature,payload) VALUES($1,$2,$3) ON CONFLICT(signature) DO NOTHING RETURNING id")
                    .bind(new_id).bind(&signature).bind(payload).fetch_optional(&mut *tx).await.map_err(anyhow::Error::from)?;
                let card_id = if let Some(card_id) = inserted_id {
                    card_id
                } else {
                    sqlx::query_scalar(
                        "SELECT id FROM learning_cards WHERE signature=$1 FOR UPDATE",
                    )
                    .bind(&signature)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(anyhow::Error::from)?
                };
                (card_id, false)
            }
        };
        if !tags.is_empty() {
            let mut card: topics::CardTags =
                sqlx::query_as("SELECT id,tags,tags_version FROM learning_cards WHERE id=$1")
                    .bind(card_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(anyhow::Error::from)?;
            topics::update_card(&mut tx, &mut card, &tags, &BTreeSet::new()).await?;
        }
        let result = sqlx::query(
            "INSERT INTO deck_cards(deck_id,card_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
        )
        .bind(deck_id)
        .bind(card_id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
        cards.insert(
            signature,
            CardWrite {
                id: card_id,
                added: result.rows_affected() == 1,
                changed,
            },
        );
    }
    let total = check_deck_capacity(&mut tx, deck_id).await?;
    if cards.values().any(|card| card.added) {
        sqlx::query("UPDATE decks SET version=version+1 WHERE id=$1")
            .bind(deck_id)
            .execute(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(WrittenCards { cards, total })
}
