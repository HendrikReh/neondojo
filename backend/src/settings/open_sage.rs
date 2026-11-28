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
        .ok_or_else(|| anyhow::anyhow!("NeonGammon has not been installed and verified"))?;
    anyhow::ensure!(
        plies != 1 || saved.one_ply_verified,
        "NeonGammon 1-ply capability has not been verified"
    );
    let mut current = SageEngine::probe_profile(saved.python.clone(), plies, threads).await?;
    anyhow::ensure!(
        current.artifact_fingerprint == saved.artifact_fingerprint
            && current.package_version == saved.package_version,
        "NeonGammon installation changed; verify the installation again"
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
        if crate::engines::EngineId::parse(engine)
            .is_some_and(crate::engines::EngineId::requires_verified_profile)
        {
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
    .map(|threads| sage_threads(Some(threads), sage_default_analysis_threads()))
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

pub(crate) async fn verify_installation(
    state: &AppState,
    version: i64,
    path: String,
) -> Result<Installation, DomainError> {
    if version < 0 || version == i64::MAX {
        return Err(DomainError::invalid("Invalid installation revision"));
    }
    if load(&state.pool).await?.version != version {
        return Err(conflict().into());
    }
    validate_path(&path)?;
    let snapshot = SageEngine::probe(path, sage_default_analysis_threads())
        .await
        .map_err(|e| {
            DomainError::invalid(format!("NeonGammon installation check failed: {e:#}"))
        })?;
    let saved = sqlx::query_as("UPDATE engine_installations SET version=version+1,python_path=$2,snapshot=$3 WHERE engine_id='bgsage' AND version=$1 RETURNING version,python_path,snapshot")
        .bind(version).bind(&snapshot.python).bind(sqlx::types::Json(&snapshot)).fetch_optional(&state.pool).await.map_err(anyhow::Error::from)?;
    saved.ok_or_else(|| conflict().into())
}

#[utoipa::path(get,path="/api/engines/bgsage",responses((status=200,body=Installation)),tag="Settings")]
async fn get_installation(State(state): State<AppState>) -> ApiResult<Json<Installation>> {
    Ok(Json(load(&state.pool).await?))
}
#[utoipa::path(put,path="/api/engines/bgsage",request_body=InstallationRequest,responses((status=200,body=Installation),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Settings")]
async fn put_installation(
    State(state): State<AppState>,
    Json(request): Json<InstallationRequest>,
) -> ApiResult<Json<Installation>> {
    Ok(Json(
        verify_installation(&state, request.version, request.path).await?,
    ))
}
#[derive(OpenApi)]
#[openapi(
    paths(get_installation, put_installation),
    components(schemas(Installation, InstallationRequest, SageSnapshot))
)]
pub struct OpenSageDoc;
