use super::*;

pub(super) async fn require_deck(executor: impl sqlx::PgExecutor<'_>, id: Uuid) -> ApiResult<()> {
    if !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM decks WHERE id=$1)")
        .bind(id)
        .fetch_one(executor)
        .await
        .map_err(anyhow::Error::from)?
    {
        return Err(ApiError::missing());
    }
    Ok(())
}

pub(super) fn deck_name(name: &str) -> ApiResult<&str> {
    let name = name.trim();
    if name.is_empty() || name.len() > 120 || name.chars().any(char::is_control) {
        return Err(ApiError::invalid(
            "Deck name must be 1–120 bytes without control characters",
        ));
    }
    Ok(name)
}

pub(super) async fn lock_deck_version(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    expected_version: i64,
) -> ApiResult<()> {
    if expected_version < 0 {
        return Err(ApiError::invalid("expected_version must be nonnegative"));
    }
    // Match membership writes' deck-first lock order. Cards and FSRS rows remain untouched.
    let version: i64 = sqlx::query_scalar("SELECT version FROM decks WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    if version != expected_version {
        return Err(conflict("Deck changed; reload it before trying again"));
    }
    Ok(())
}

pub(super) async fn check_deck_capacity(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    deck_id: Uuid,
) -> ApiResult<i64> {
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM deck_cards WHERE deck_id=$1")
        .bind(deck_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(anyhow::Error::from)?;
    if total > MAX_CARDS {
        return Err(ApiError::invalid(
            "Deck exceeds 1000 cards; use another deck or narrow the filter",
        ));
    }
    Ok(total)
}
