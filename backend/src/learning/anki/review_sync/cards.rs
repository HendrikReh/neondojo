use super::*;

pub(in crate::learning::anki) async fn deck_cards(
    pool: &PgPool,
    id: Uuid,
) -> ApiResult<HashMap<String, Uuid>> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    lock_deck(&mut tx, id).await?;
    let rows: Vec<CardRow> = sqlx::query_as(
        "SELECT c.* FROM learning_cards c JOIN deck_cards dc ON dc.card_id=c.id
         WHERE dc.deck_id=$1 LIMIT 1001",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    let unsupported =
        || ApiError::invalid("Select a non-empty native NeonDojo deck with at most 1000 cards");
    if rows.is_empty()
        || rows.len() > MAX_CARDS as usize
        || rows.iter().any(|row| row.decision().is_none())
    {
        return Err(unsupported());
    }
    let mut cards = HashMap::new();
    for row in rows {
        let identity = card_signature(row.decision().ok_or_else(unsupported)?);
        // GNUbg's base64 IDs and the decision kind are literal Anki search terms.
        if !identity.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'/' | b'+' | b'=' | b'_')
        }) {
            return Err(ApiError::invalid("Invalid NeonDojo card identity"));
        }
        if cards
            .insert(apkg::identity_field(&identity), row.id)
            .is_some()
        {
            return Err(conflict(
                "Resolve duplicate cards in the deck before synchronizing",
            ));
        }
    }
    Ok(cards)
}
