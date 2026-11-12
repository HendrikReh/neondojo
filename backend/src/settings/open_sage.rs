//! Host-local installation, kept separate from portable preference profiles.
use super::*;
use crate::engines::{SageEngine, SageSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Installation {
    pub version: i64,
    pub python_path: String,
    #[sqlx(json(nullable))]
    pub snapshot: Option<SageSnapshot>,
}
#[derive(Deserialize, ToSchema)]
struct InstallationRequest {
    version: i64,
    path: String,
}
pub(super) fn routes() -> Router<AppState> {
    Router::new().route(
        "/api/engines/bgsage",
        get(get_installation).put(put_installation),
    )
}
pub(crate) async fn load(pool: &sqlx::PgPool) -> anyhow::Result<Installation> {
    Ok(sqlx::query_as(
        "SELECT version,python_path,snapshot FROM engine_installations WHERE engine_id='bgsage'",
    )
    .fetch_one(pool)
    .await?)
}
pub(crate) async fn verified(pool: &sqlx::PgPool, threads: u32) -> anyhow::Result<SageSnapshot> {
    let saved = load(pool)
        .await?
        .snapshot
        .ok_or_else(|| anyhow::anyhow!("Open Sage has not been installed and verified"))?;
    let current = SageEngine::probe(saved.python.clone(), threads).await?;
    anyhow::ensure!(
        current.artifact_fingerprint == saved.artifact_fingerprint
            && current.package_version == saved.package_version,
        "Open Sage installation changed; verify the installation again"
    );
    Ok(current)
}
#[utoipa::path(get,path="/api/engines/bgsage",responses((status=200,body=Installation)),tag="Settings")]
async fn get_installation(State(state): State<AppState>) -> ApiResult<Json<Installation>> {
    Ok(Json(load(&state.pool).await?))
}
#[utoipa::path(put,path="/api/engines/bgsage",request_body=InstallationRequest,responses((status=200,body=Installation),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Settings")]
async fn put_installation(
    State(state): State<AppState>,
    payload: Result<Json<InstallationRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<Installation>> {
    let Json(request) = payload.map_err(|e| ApiError::invalid(e.body_text()))?;
    if request.version < 0 || request.version == i64::MAX {
        return Err(ApiError::invalid("Invalid installation revision"));
    }
    if load(&state.pool).await?.version != request.version {
        return Err(conflict());
    }
    validate_path(&request.path)?;
    let snapshot = SageEngine::probe(request.path, 4)
        .await
        .map_err(|e| ApiError::invalid(format!("Open Sage installation check failed: {e:#}")))?;
    let saved = sqlx::query_as("UPDATE engine_installations SET version=version+1,python_path=$2,snapshot=$3 WHERE engine_id='bgsage' AND version=$1 RETURNING version,python_path,snapshot")
        .bind(request.version).bind(&snapshot.python).bind(sqlx::types::Json(&snapshot)).fetch_optional(&state.pool).await.map_err(anyhow::Error::from)?;
    Ok(Json(saved.ok_or_else(conflict)?))
}
#[derive(OpenApi)]
#[openapi(
    paths(get_installation, put_installation),
    components(schemas(Installation, InstallationRequest, SageSnapshot))
)]
pub struct OpenSageDoc;
