use super::{super::*, apkg};

pub(in crate::learning) async fn build_deck_package(
    state: &AppState,
    id: Uuid,
    query: ExportQuery,
) -> ApiResult<Vec<u8>> {
    let locale = query.locale.unwrap_or(ConfigurationLocale::De);
    let permit = state
        .anki_limits
        .exports
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "Two exports are already in progress".into(),
            )
        })?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    // Hold a shared deck lock so concurrent moves or duplicate cleanup cannot
    // change its membership between reading IDs and locking the export cards.
    let name: String = sqlx::query_scalar("SELECT name FROM decks WHERE id=$1 FOR SHARE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    let anki_cards =
        || ApiError::invalid("Use the original Anki package to export imported Anki cards");
    if sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM learning_cards c JOIN deck_cards dc ON dc.card_id=c.id
                        WHERE dc.deck_id=$1 AND c.payload->>'kind'='anki')",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?
    {
        return Err(anki_cards());
    }
    let card_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT c.id FROM learning_cards c JOIN deck_cards dc ON dc.card_id=c.id
         WHERE dc.deck_id=$1 ORDER BY c.signature COLLATE \"C\" LIMIT 1001",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if card_ids.len() > MAX_CARDS as usize {
        return Err(ApiError::invalid("Export exceeds 1000 cards"));
    }
    // Read content only after taking all card locks, in the same order as add_cards.
    for card_id in &card_ids {
        sqlx::query("SELECT id FROM learning_cards WHERE id=$1 FOR UPDATE")
            .bind(card_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    }
    let mut cards = Vec::with_capacity(card_ids.len());
    for card_id in card_ids {
        let row: CardRow = sqlx::query_as("SELECT * FROM learning_cards WHERE id=$1")
            .bind(card_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
        let c = row.decision().ok_or_else(anki_cards)?.clone();
        let a = content::answer_on_connection(&mut tx, row.id, &c).await?;
        cards.push((row.id, c, a));
    }
    let (mut prepared, permit) = tokio::task::spawn_blocking(move || {
        apkg::prepare(cards, locale).map(|prepared| (prepared, permit))
    })
    .await
    .map_err(anyhow::Error::from)??;
    for card in &mut prepared {
        let fingerprint = card.fingerprint();
        card.modified = sqlx::query_scalar("INSERT INTO learning_card_exports(card_id,fingerprint,modified) VALUES($1,$2,$3) ON CONFLICT(card_id) DO UPDATE SET fingerprint=EXCLUDED.fingerprint,modified=CASE WHEN learning_card_exports.fingerprint=EXCLUDED.fingerprint THEN learning_card_exports.modified ELSE greatest(learning_card_exports.modified+1,EXCLUDED.modified) END RETURNING modified")
            .bind(card.id).bind(fingerprint).bind(Utc::now().timestamp()).fetch_one(&mut *tx).await.map_err(anyhow::Error::from)?;
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(tokio::task::spawn_blocking(move || {
        // Cancellation of the HTTP request must not release the CPU slot before ZIP work ends.
        let _permit = permit;
        apkg::build(id, &name, &prepared)
    })
    .await
    .map_err(anyhow::Error::from)??)
}
