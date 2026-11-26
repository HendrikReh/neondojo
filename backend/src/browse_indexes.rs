//! Maintain derived Saved Positions and Study indexes outside browse requests.
use crate::{
    api::{ApiError, ApiResult, AppState},
    imported_analysis::projections,
    position_game_plans,
};
use axum::http::StatusCode;
use sqlx::{PgPool, Postgres, Transaction};
use std::time::Duration;
use uuid::Uuid;

pub(crate) async fn read_snapshot(pool: &PgPool) -> ApiResult<Transaction<'_, Postgres>> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(tx)
}

pub(crate) fn preparing(name: &str) -> ApiError {
    let mut error = crate::api::ErrorBody::from(format!(
        "{name} is being prepared in the background. Retry after indexing finishes."
    ));
    error.code = Some("index_preparing".into());
    ApiError(StatusCode::SERVICE_UNAVAILABLE, error)
}

#[derive(Default)]
pub struct BackfillCursor {
    legacy: Option<Uuid>,
    decisions: Option<Uuid>,
    saved_complete: bool,
    decisions_complete: bool,
}

/// Run one bounded page, including after an upgrade or a database restore.
/// A full page asks the worker to continue from the cursor immediately.
pub async fn backfill_batch(pool: &PgPool, cursor: &mut BackfillCursor) -> ApiResult<bool> {
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
    if cursor.saved_complete && cursor.decisions_complete {
        *cursor = BackfillCursor::default();
        Ok(false)
    } else {
        Ok(true)
    }
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
                next = backfill_batch(&state.pool, &mut cursor) => next,
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
