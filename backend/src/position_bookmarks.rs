//! Explicit collection membership for an exact imported position occurrence.
use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use crate::json_errors::{Json, Path};
use axum::{Router, extract::State, http::StatusCode, routing::get};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PositionBookmark {
    /// UUID of the exact imported occurrence, retaining its player and source context.
    pub position_id: Uuid,
    pub saved: bool,
    pub saved_at: Option<DateTime<Utc>>,
}

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/api/positions/{id}/bookmark",
        get(get_bookmark).put(save_bookmark).delete(remove_bookmark),
    )
}

fn position_id(path: Result<Path<Uuid>, ApiError>) -> ApiResult<Uuid> {
    path.map(|Path(id)| id).map_err(ApiError::into_invalid)
}

async fn lock_position(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> ApiResult<()> {
    // Library deletion locks the owning match before cascading to positions.
    // Take the same order, including reads, to give a coherent existence check.
    let owner = crate::position_retention::lock_owner(tx, id).await?;
    owner.ok_or_else(ApiError::missing)?;
    let position: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM positions WHERE id=$1 FOR KEY SHARE")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(anyhow::Error::from)?;
    position.ok_or_else(ApiError::missing)?;
    Ok(())
}

async fn load_bookmark(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> ApiResult<PositionBookmark> {
    let saved_at =
        sqlx::query_scalar("SELECT saved_at FROM saved_positions WHERE position_uuid=$1")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(anyhow::Error::from)?;
    Ok(PositionBookmark {
        position_id: id,
        saved: saved_at.is_some(),
        saved_at,
    })
}

#[utoipa::path(get, path="/api/positions/{id}/bookmark", params(("id"=Uuid, Path)), responses((status=200, body=PositionBookmark),(status=404, body=ErrorBody),(status=422, body=ErrorBody)), tag="Positions")]
async fn get_bookmark(
    State(state): State<AppState>,
    path: Result<Path<Uuid>, ApiError>,
) -> ApiResult<Json<PositionBookmark>> {
    let id = position_id(path)?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    lock_position(&mut tx, id).await?;
    let bookmark = load_bookmark(&mut tx, id).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(bookmark))
}

#[utoipa::path(put, path="/api/positions/{id}/bookmark", params(("id"=Uuid, Path)), responses((status=200, body=PositionBookmark, description="Explicitly save this occurrence. Repeated saves retain its original saved_at."),(status=404, body=ErrorBody),(status=422, body=ErrorBody)), tag="Positions")]
async fn save_bookmark(
    State(state): State<AppState>,
    path: Result<Path<Uuid>, ApiError>,
) -> ApiResult<Json<PositionBookmark>> {
    let id = position_id(path)?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    lock_position(&mut tx, id).await?;
    sqlx::query("INSERT INTO saved_positions(position_uuid) VALUES($1) ON CONFLICT(position_uuid) DO NOTHING")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let bookmark = load_bookmark(&mut tx, id).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(bookmark))
}

#[utoipa::path(delete, path="/api/positions/{id}/bookmark", params(("id"=Uuid, Path)), responses((status=204, description="Remove collection membership while retaining the source position, notes, analysis and learning cards. Repeated removal succeeds for an existing position."),(status=404, body=ErrorBody),(status=422, body=ErrorBody)), tag="Positions")]
async fn remove_bookmark(
    State(state): State<AppState>,
    path: Result<Path<Uuid>, ApiError>,
) -> ApiResult<StatusCode> {
    let id = position_id(path)?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    lock_position(&mut tx, id).await?;
    sqlx::query("DELETE FROM saved_positions WHERE position_uuid=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(OpenApi)]
#[openapi(
    paths(get_bookmark, save_bookmark, remove_bookmark),
    components(schemas(PositionBookmark))
)]
pub struct PositionBookmarksDoc;
