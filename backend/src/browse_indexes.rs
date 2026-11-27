//! Maintain derived Saved Positions and Study indexes outside browse requests.
use crate::{
    api::{ApiError, ApiResult, AppState},
    imported_analysis::projections,
    position_game_plans,
};
use axum::http::StatusCode;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

pub(crate) fn preparing(state: &AppState, name: &str) -> ApiError {
    state.browse_wake.notify_one();
    let mut error = crate::api::ErrorBody::from(format!(
        "{name} is being prepared in the background. Retry after indexing finishes."
    ));
    error.code = Some("index_preparing".into());
    ApiError(StatusCode::SERVICE_UNAVAILABLE, error)
}

#[derive(Default)]
pub struct BackfillCursor {
    catalogs: Option<Uuid>,
    legacy: Option<Uuid>,
    decisions: Option<Uuid>,
    catalogs_complete: bool,
    saved_complete: bool,
    decisions_complete: bool,
}

async fn backfill_match_catalogs(pool: &PgPool, after: &mut Option<Uuid>) -> anyhow::Result<bool> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT m.id FROM matches m
         WHERE NOT m.collection_only
           AND (NOT m.moves_cataloged
                OR NOT m.imported_analysis_loaded
                OR NOT EXISTS(SELECT 1 FROM match_games g WHERE g.match_uuid = m.id))
           AND ($1::uuid IS NULL OR m.id > $1)
         ORDER BY m.id LIMIT 16",
    )
    .bind(*after)
    .fetch_all(pool)
    .await?;
    let full = ids.len() == 16;
    *after = if full { ids.last().copied() } else { None };
    for id in ids {
        let catalog = async {
            crate::store::game_catalog(pool, id).await?;
            crate::move_catalog::ensure_catalog(pool, id).await?;
            crate::imported_analysis::ensure_catalog(pool, id).await?;
            anyhow::Ok(())
        };
        if let Err(error) = catalog.await {
            tracing::warn!(%id, %error, "Failed to backfill legacy match catalogs");
        }
    }
    Ok(full)
}

/// Run one bounded page, including after an upgrade or a database restore.
/// A full page asks the worker to continue from the cursor immediately.
pub async fn backfill_batch(pool: &PgPool, cursor: &mut BackfillCursor) -> ApiResult<bool> {
    if !cursor.catalogs_complete {
        cursor.catalogs_complete = !backfill_match_catalogs(pool, &mut cursor.catalogs).await?;
    }
    if !cursor.saved_complete {
        let legacy = projections::backfill_legacy(pool, &mut cursor.legacy).await?;
        let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
        let plans = position_game_plans::refresh(&mut tx, None)
            .await
            .map_err(anyhow::Error::from)?;
        let projections = projections::refresh(&mut tx, None).await?;
        tx.commit().await.map_err(anyhow::Error::from)?;
        cursor.saved_complete = !(legacy || plans || projections);
    }
    if !cursor.decisions_complete {
        cursor.decisions_complete =
            !crate::recurring_mistakes::backfill_batch(pool, &mut cursor.decisions).await?;
    }
    if cursor.catalogs_complete && cursor.saved_complete && cursor.decisions_complete {
        *cursor = BackfillCursor::default();
        Ok(false)
    } else {
        Ok(true)
    }
}

/// Run one maintenance pass including queued review hit-set refreshes.
pub async fn backfill_state_batch(
    state: &AppState,
    cursor: &mut BackfillCursor,
) -> ApiResult<bool> {
    let more_hits = crate::review_decisions::drain_hit_queue(state).await?;
    let more_indexes = backfill_batch(&state.pool, cursor).await?;
    Ok(more_hits || more_indexes)
}

pub async fn worker(state: AppState) {
    let mut cursor = BackfillCursor::default();
    while !state.shutdown.is_cancelled() {
        let delay = if let Some(_access) = state
            .backup
            .try_background(crate::database_backup::BackgroundWork::BrowseIndexes)
        {
            let next = tokio::select! {
                _ = state.shutdown.cancelled() => break,
                next = backfill_state_batch(&state, &mut cursor) => next,
            };
            match next {
                // Preserve notifications arriving during a pass: a later pass must
                // revisit changes that fell behind either cursor.
                Ok(true) => {
                    tokio::task::yield_now().await;
                    continue;
                }
                // Also discovers external changes and missed notifications.
                Ok(false) => Duration::from_secs(60),
                Err(error) => {
                    tracing::warn!(error=%error.1.error, "Browse index maintenance failed");
                    Duration::from_secs(5)
                }
            }
        } else {
            Duration::from_secs(1)
        };
        tokio::select! {
            _ = state.shutdown.cancelled() => break,
            _ = state.browse_wake.notified() => {},
            _ = tokio::time::sleep(delay) => {},
        }
    }
}
