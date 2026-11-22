use super::*;

pub(crate) async fn require_review_available(pool: &sqlx::PgPool, match_id: Uuid) -> ApiResult<()> {
    require_review_available_on(pool, match_id).await
}

pub(crate) const ACTIVE_COMPETITION: &str =
    "coalesce(state->>'play_mode','competition')='competition'
    AND state->>'phase'<>'match_over'
    AND NOT ((state->>'match_length')::int=0 AND state->>'phase'='game_over')";

pub(crate) async fn require_review_available_on(
    executor: impl sqlx::PgExecutor<'_>,
    match_id: Uuid,
) -> ApiResult<()> {
    let active: bool = sqlx::query_scalar(&format!(
        "SELECT EXISTS(SELECT 1 FROM play_sessions WHERE library_match_id=$1 AND {ACTIVE_COMPETITION})"
    ))
    .bind(match_id)
    .fetch_one(executor)
    .await
    .map_err(anyhow::Error::from)?;
    if active {
        return Err(ApiError::invalid(
            "Complete the competition match before requesting analysis",
        ));
    }
    Ok(())
}

pub(crate) async fn require_position_review_available(
    pool: &sqlx::PgPool,
    position_id: Uuid,
) -> ApiResult<()> {
    let match_id: Option<Uuid> = sqlx::query_scalar("SELECT match_uuid FROM positions WHERE id=$1")
        .bind(position_id)
        .fetch_optional(pool)
        .await
        .map_err(anyhow::Error::from)?;
    if let Some(match_id) = match_id {
        require_review_available(pool, match_id).await?;
    }
    Ok(())
}

pub(crate) async fn require_job_review_available(
    pool: &sqlx::PgPool,
    job: &crate::model::Job,
) -> ApiResult<()> {
    if let Some(match_id) = job.match_uuid {
        require_review_available(pool, match_id).await?;
    }
    if let Some(position_id) = job.position_uuid {
        require_position_review_available(pool, position_id).await?;
    }
    Ok(())
}
