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

pub(super) async fn update_deck(
    pool: &PgPool,
    id: Uuid,
    expected_version: i64,
    name: &str,
) -> ApiResult<Deck> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    lock_deck_version(&mut tx, id, expected_version).await?;
    let deck = sqlx::query_as(
        "UPDATE decks d SET name=$2,version=version+1 WHERE id=$1
         RETURNING d.id,d.name,d.version,d.created_at,
                   (SELECT count(*) FROM deck_cards WHERE deck_id=d.id) AS card_count,
                   (SELECT count(*) FROM deck_cards dc
                    JOIN learning_cards c ON c.id=dc.card_id
                    WHERE dc.deck_id=d.id AND c.due_at<=now()) AS due_count",
    )
    .bind(id)
    .bind(name)
    .fetch_one(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(deck)
}

pub(super) async fn remove_deck(pool: &PgPool, id: Uuid, expected_version: i64) -> ApiResult<()> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    lock_deck_version(&mut tx, id, expected_version).await?;
    // deck_cards cascades from decks; learning_cards, reviews and saved positions do not.
    sqlx::query("DELETE FROM decks WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(())
}

pub(super) async fn list_decks(pool: &PgPool) -> ApiResult<Vec<Deck>> {
    Ok(sqlx::query_as(
        "SELECT d.id,d.name,d.version,d.created_at,count(c.id) AS card_count,
                count(c.id) FILTER (WHERE c.due_at<=now()) AS due_count
         FROM decks d LEFT JOIN deck_cards dc ON dc.deck_id=d.id
         LEFT JOIN learning_cards c ON c.id=dc.card_id
         GROUP BY d.id ORDER BY d.created_at,d.id",
    )
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?)
}

pub(super) async fn insert_deck(pool: &PgPool, name: &str) -> ApiResult<Deck> {
    Ok(sqlx::query_as(
        "INSERT INTO decks(id,name) VALUES($1,$2)
         RETURNING id,name,version,created_at,0::bigint AS card_count,0::bigint AS due_count",
    )
    .bind(Uuid::new_v4())
    .bind(name)
    .fetch_one(pool)
    .await
    .map_err(anyhow::Error::from)?)
}
