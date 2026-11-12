//! Moves the existing library entry together with the versioned play checkpoint.
use super::{PlayPhase, PlaySession, Saved, library, present_with_results};
use crate::api::{ApiError, ApiResult, AppState};
use axum::{Json, http::StatusCode};
use uuid::Uuid;

fn conflict(message: &str) -> ApiError {
    ApiError(StatusCode::CONFLICT, message.into())
}

pub(super) fn require_active(session: &PlaySession) -> ApiResult<()> {
    if session.paused {
        return Err(conflict("Resume the paused match before playing"));
    }
    Ok(())
}

pub(super) async fn transition(
    state: &AppState,
    saved: Saved,
    pausing: bool,
) -> ApiResult<Json<PlaySession>> {
    let mut session = saved.state.0;
    if pausing {
        require_active(&session)?;
        if session.money_session_ended
            || (matches!(session.phase, PlayPhase::MatchOver) && session.match_length != 0)
        {
            return Err(ApiError::invalid("This match has already finished"));
        }
    } else if !session.paused {
        return Err(conflict("This match is not paused"));
    }
    let paused_db = library::PAUSED_MATCHES_DATABASE_ID;
    let bot_db = library::BOT_DATABASE_ID;
    let destinations = if let Some(origin) = saved.paused_from_database_id {
        let unsorted = crate::databases::default_database_id(&state.pool)
            .await
            .map_err(anyhow::Error::from)?;
        vec![origin, bot_db, unsorted]
    } else {
        Vec::new()
    };
    let mut databases = destinations.clone();
    databases.push(paused_db);
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let linked = library::lock_match(&mut tx, session.id, &databases).await?;
    let (destination, origin) = if pausing {
        (paused_db, Some(linked.map_or(bot_db, |(_, db)| db)))
    } else {
        let (id, _) = linked.ok_or_else(|| conflict("The paused library match is missing"))?;
        // All eligible destinations are locked before checking for independent copies.
        let destination: Uuid = sqlx::query_scalar(
            "SELECT db.id
             FROM unnest($2::uuid[]) WITH ORDINALITY AS candidate(id, priority)
             JOIN databases db ON db.id=candidate.id AND NOT db.collection_only
             JOIN matches source ON source.id=$1
             WHERE NOT EXISTS (
                 SELECT 1 FROM matches copy
                 WHERE copy.database_id=db.id AND copy.sha256=source.sha256
             )
             ORDER BY candidate.priority LIMIT 1",
        )
        .bind(id)
        .bind(&destinations)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(|| conflict("Move an independent copy out of the previous database, Bot Matches or Unsorted before resuming"))?;
        (destination, None)
    };

    let updated = sqlx::query(
        "UPDATE play_sessions SET paused_from_database_id=$3,
             state=jsonb_set(state,'{version}',to_jsonb(version+1)),
             version=version+1, updated_at=now() WHERE id=$1 AND version=$2",
    )
    .bind(session.id)
    .bind(session.version)
    .bind(origin)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if updated.rows_affected() != 1 {
        return Err(conflict(
            "Session changed; reload before pausing or resuming",
        ));
    }

    session.library_match_id = Some(if let Some((id, _)) = linked {
        sqlx::query("UPDATE matches SET database_id=$2 WHERE id=$1")
            .bind(id)
            .bind(destination)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                if error
                    .as_database_error()
                    .is_some_and(|error| error.is_unique_violation())
                {
                    conflict("The destination database already contains a copy of this match")
                } else {
                    anyhow::Error::from(error).into()
                }
            })?;
        id
    } else if pausing {
        library::sync(
            &mut tx,
            &session,
            &saved.sgf,
            library::ArchiveTarget::NewIn(paused_db),
        )
        .await?
    } else {
        return Err(conflict("The paused library match is missing"));
    });
    tx.commit().await.map_err(anyhow::Error::from)?;
    session.version += 1;
    session.paused = pausing;
    let session = present_with_results(session, saved.sgf).await?;
    super::open_sage::schedule_warmup(&session, &state.shutdown);
    Ok(Json(session))
}
