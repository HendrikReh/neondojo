use super::*;

pub(super) async fn reveal_card(
    pool: &PgPool,
    id: Uuid,
    expected_version: i64,
) -> ApiResult<RevealedCard> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    let row: CardRow = sqlx::query_as("SELECT * FROM learning_cards WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    if row.version != expected_version {
        return Err(conflict("Card changed; reload it before revealing"));
    }
    let now = Utc::now();
    if let Some(attempt) = active_attempt(&mut *tx, &row).await? {
        return revealed_card(&mut tx, &row, attempt.session_id, now).await;
    }
    let session_id = Uuid::new_v4();
    let revealed = revealed_card(&mut tx, &row, session_id, now).await?;
    sqlx::query(
        "INSERT INTO review_sessions(id,card_id,card_version,expires_at) VALUES($1,$2,$3,$4)",
    )
    .bind(session_id)
    .bind(id)
    .bind(row.version)
    .bind(now + chrono::Duration::days(1))
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(revealed)
}

pub(super) async fn rate_card(
    pool: &PgPool,
    id: Uuid,
    body: ReviewCard,
) -> ApiResult<ReviewResult> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    let row: CardRow = sqlx::query_as("SELECT * FROM learning_cards WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    let previous: Option<(Uuid, String, serde_json::Value)> = sqlx::query_as(
        "SELECT session_id,rating,result FROM card_reviews WHERE card_id=$1 AND request_id=$2",
    )
    .bind(id)
    .bind(body.request_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if let Some((session, rating, result)) = previous {
        if session != body.session_id || rating != body.rating.as_str() {
            return Err(conflict(
                "request_id was already used for a different review",
            ));
        }
        return serde_json::from_value(result).map_err(|error| anyhow::Error::from(error).into());
    }
    let session: Option<(i64, DateTime<Utc>, bool)> = sqlx::query_as(
        "SELECT card_version,expires_at,attempt_request_id IS NOT NULL FROM review_sessions WHERE id=$1 AND card_id=$2",
    )
    .bind(body.session_id)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let (version, expires, is_attempt) =
        session.ok_or_else(|| conflict("Reveal this card before rating"))?;
    let now = Utc::now();
    if version != row.version || (!is_attempt && expires < now) {
        return Err(conflict(
            "Review session is stale; reload and reveal the card",
        ));
    }
    if !is_attempt && active_attempt(&mut *tx, &row).await?.is_some() {
        return Err(conflict("Rate the saved first attempt for this card"));
    }
    let mut result = scheduler::rate(&row, body.rating, now)?;
    sqlx::query(
        "UPDATE learning_cards SET stability=$2,difficulty=$3,last_review=$4,due_at=$5,
             version=$6,review_count=$7,updated_at=$4 WHERE id=$1",
    )
    .bind(id)
    .bind(result.stability)
    .bind(result.difficulty)
    .bind(now)
    .bind(result.due_at)
    .bind(result.version)
    .bind(result.review_count)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    result.review_count += row.anki_review_count;
    sqlx::query(
        "INSERT INTO card_reviews(card_id,request_id,session_id,rating,result,reviewed_at)
         VALUES($1,$2,$3,$4,$5,$6)",
    )
    .bind(id)
    .bind(body.request_id)
    .bind(body.session_id)
    .bind(body.rating.as_str())
    .bind(serde_json::to_value(&result).map_err(anyhow::Error::from)?)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(result)
}
