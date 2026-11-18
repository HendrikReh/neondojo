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
    verified_profile(pool, 2, threads).await
}
pub(crate) async fn verified_profile(
    pool: &sqlx::PgPool,
    plies: i32,
    threads: u32,
) -> anyhow::Result<SageSnapshot> {
    let saved = load(pool)
        .await?
        .snapshot
        .ok_or_else(|| anyhow::anyhow!("Open Sage has not been installed and verified"))?;
    anyhow::ensure!(
        plies != 1 || saved.one_ply_verified,
        "Open Sage 1-ply capability has not been verified"
    );
    let mut current = SageEngine::probe_profile(saved.python.clone(), plies, threads).await?;
    anyhow::ensure!(
        current.artifact_fingerprint == saved.artifact_fingerprint
            && current.package_version == saved.package_version,
        "Open Sage installation changed; verify the installation again"
    );
    current.one_ply_verified = saved.one_ply_verified;
    Ok(current)
}
pub(super) async fn verify_settings(
    pool: &sqlx::PgPool,
    current: &AppSettings,
    proposed: &AppSettings,
) -> anyhow::Result<()> {
    let mut profiles = Vec::new();
    for (engine, plies, threads) in [
        (
            &proposed.analysis_engine,
            proposed.analysis_plies,
            proposed.sage_analysis_threads(),
        ),
        (
            &proposed.play_engine,
            proposed.play_plies,
            proposed.sage_play_threads(),
        ),
    ] {
        if engine == "bgsage" {
            profiles.push((plies, threads));
        }
    }
    // A host override also applies when Sage is selected later, including in duels.
    let changed: Vec<u32> = [
        (proposed.open_sage_threads, current.open_sage_threads),
        (
            proposed.open_sage_play_threads,
            current.open_sage_play_threads,
        ),
    ]
    .into_iter()
    .filter(|(proposed, saved)| proposed != saved)
    .filter_map(|(proposed, _)| proposed)
    .map(|threads| sage_threads(Some(threads), SAGE_ANALYSIS_THREADS))
    .collect();
    if !changed.is_empty()
        && let Some(installation) = load(pool).await?.snapshot
    {
        for threads in changed {
            profiles.push((2, threads));
            if installation.one_ply_verified {
                profiles.push((1, threads));
            }
        }
    }
    profiles.sort_unstable();
    profiles.dedup();
    for (plies, threads) in profiles {
        verified_profile(pool, plies, threads).await?;
    }
    Ok(())
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
    let snapshot = SageEngine::probe(request.path, SAGE_ANALYSIS_THREADS)
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
