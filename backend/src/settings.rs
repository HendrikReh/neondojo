//! Persisted preferences and immutable engine configuration for new work.
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    engine::Engine,
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use utoipa::{OpenApi, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct AppSettings {
    /// Expected revision on PUT; a successful write increments it by one.
    pub version: i64,
    pub player_name: String,
    pub analysis_engine: String,
    #[schema(minimum = 0, maximum = 4, default = 2)]
    pub analysis_plies: i32,
    pub play_engine: String,
    #[schema(minimum = 0, maximum = 2)]
    pub play_plies: i32,
    pub gnubg_path: String,
}

#[derive(FromRow)]
struct SavedSettings {
    #[sqlx(flatten)]
    settings: AppSettings,
    gnubg_version: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EngineInfo {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub version: Option<String>,
    pub plies: Vec<i32>,
}

#[derive(Deserialize, ToSchema)]
struct EngineCheckRequest {
    path: String,
}
#[derive(Serialize, ToSchema)]
struct EngineCheckResponse {
    version: String,
}

/// Internal job payload, never derived again from mutable application settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSnapshot {
    pub engine_id: String,
    pub gnubg_path: String,
    pub engine_version: Option<String>,
}
impl EngineSnapshot {
    pub fn fingerprint(&self) -> String {
        let value = serde_json::to_vec(self).expect("engine snapshot is serializable");
        format!("{:x}", Sha256::digest(value))
    }
    pub fn engine(&self) -> anyhow::Result<Engine> {
        anyhow::ensure!(self.engine_id == "gnubg", "Unsupported saved job engine");
        Ok(Engine::new(self.gnubg_path.clone()).with_expected_version(self.engine_version.clone()))
    }
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/engines", get(engines))
        .route("/api/engines/gnubg/check", post(check_engine))
}

async fn load_row(state: &AppState) -> anyhow::Result<SavedSettings> {
    sqlx::query("INSERT INTO app_settings(singleton,gnubg_path) VALUES(true,$1) ON CONFLICT(singleton) DO NOTHING")
        .bind(state.engine.binary()).execute(&state.pool).await?;
    Ok(
        sqlx::query_as("SELECT * FROM app_settings WHERE singleton=true")
            .fetch_one(&state.pool)
            .await?,
    )
}
pub(crate) async fn load(state: &AppState) -> anyhow::Result<AppSettings> {
    Ok(load_row(state).await?.settings)
}
pub(crate) async fn snapshot(state: &AppState) -> anyhow::Result<(AppSettings, EngineSnapshot)> {
    let saved = load_row(state).await?;
    // Record the installed version even before the first settings save, and
    // distinguish an executable upgraded in place from an existing active job.
    // Unavailable initial/fake engines still produce a job whose worker reports
    // its normal durable failure; reading settings never probes the executable.
    let version = Engine::new(saved.settings.gnubg_path.clone())
        .version()
        .await
        .ok()
        .or(saved.gnubg_version);
    let snapshot = EngineSnapshot {
        engine_id: saved.settings.analysis_engine.clone(),
        gnubg_path: saved.settings.gnubg_path.clone(),
        engine_version: version,
    };
    Ok((saved.settings, snapshot))
}

fn validate_path(path: &str) -> ApiResult<()> {
    if path.trim().is_empty() || path.len() > 4096 || path.chars().any(char::is_control) {
        return Err(ApiError::invalid(
            "GNUbg path must be nonempty, at most 4096 bytes and contain no control characters",
        ));
    }
    Ok(())
}
fn validate(settings: &AppSettings) -> ApiResult<()> {
    if settings.version < 0 || settings.version == i64::MAX {
        return Err(ApiError::invalid("Invalid settings version"));
    }
    if settings.player_name.chars().count() > 120
        || settings.player_name.chars().any(char::is_control)
    {
        return Err(ApiError::invalid(
            "Player name must be at most 120 characters and contain no control characters",
        ));
    }
    if settings.analysis_engine != "gnubg" || settings.play_engine != "gnubg" {
        return Err(ApiError::invalid(
            "Only the available GNUbg engine can be selected",
        ));
    }
    if !(0..=4).contains(&settings.analysis_plies) {
        return Err(ApiError::invalid("analysis plies must be 0..4"));
    }
    if !(0..=2).contains(&settings.play_plies) {
        return Err(ApiError::invalid("play plies must be 0..2"));
    }
    validate_path(&settings.gnubg_path)
}

#[utoipa::path(get, path="/api/settings", responses((status=200,body=AppSettings)), tag="Settings")]
async fn get_settings(State(state): State<AppState>) -> ApiResult<Json<AppSettings>> {
    Ok(Json(load(&state).await?))
}

#[utoipa::path(put, path="/api/settings", request_body=AppSettings, responses((status=200,body=AppSettings),(status=409,body=ErrorBody),(status=422,body=ErrorBody)), tag="Settings")]
async fn put_settings(
    State(state): State<AppState>,
    payload: Result<Json<AppSettings>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<AppSettings>> {
    let Json(settings) = payload.map_err(|e| ApiError::invalid(e.body_text()))?;
    validate(&settings)?;
    let current = load(&state).await?;
    if current.version != settings.version {
        return Err(conflict());
    }
    // Validate before the compare-and-swap. No partial changes are ever persisted,
    // and the final version predicate also fences concurrent checks/writes.
    let version = Engine::new(settings.gnubg_path.clone())
        .check_installation()
        .await
        .map_err(|e| ApiError::invalid(format!("GNUbg installation check failed: {e:#}")))?;
    let saved: Option<AppSettings> = sqlx::query_as("UPDATE app_settings SET version=version+1,player_name=$2,analysis_engine=$3,analysis_plies=$4,play_engine=$5,play_plies=$6,gnubg_path=$7,gnubg_version=$8 WHERE singleton=true AND version=$1 RETURNING *")
        .bind(settings.version).bind(settings.player_name).bind(settings.analysis_engine).bind(settings.analysis_plies)
        .bind(settings.play_engine).bind(settings.play_plies).bind(settings.gnubg_path).bind(version)
        .fetch_optional(&state.pool).await.map_err(anyhow::Error::from)?;
    Ok(Json(saved.ok_or_else(conflict)?))
}
fn conflict() -> ApiError {
    ApiError(
        StatusCode::CONFLICT,
        "Settings changed; reload the latest settings before saving".into(),
    )
}

#[utoipa::path(get, path="/api/engines", responses((status=200,body=Vec<EngineInfo>)), tag="Settings")]
async fn engines(State(state): State<AppState>) -> ApiResult<Json<Vec<EngineInfo>>> {
    let settings = load(&state).await?;
    let version = Engine::new(settings.gnubg_path)
        .check_installation()
        .await
        .ok();
    Ok(Json(vec![
        EngineInfo {
            id: "gnubg".into(),
            name: "GNUbg".into(),
            available: version.is_some(),
            version,
            plies: vec![0, 1, 2, 3, 4],
        },
        EngineInfo {
            id: "bgsage".into(),
            name: "BGSage".into(),
            available: false,
            version: None,
            plies: vec![],
        },
    ]))
}
#[utoipa::path(post, path="/api/engines/gnubg/check", request_body=EngineCheckRequest, responses((status=200,body=EngineCheckResponse),(status=422,body=ErrorBody)), tag="Settings")]
async fn check_engine(
    payload: Result<Json<EngineCheckRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<EngineCheckResponse>> {
    let Json(request) = payload.map_err(|e| ApiError::invalid(e.body_text()))?;
    validate_path(&request.path)?;
    let version = Engine::new(request.path)
        .check_installation()
        .await
        .map_err(|e| ApiError::invalid(format!("GNUbg installation check failed: {e:#}")))?;
    Ok(Json(EngineCheckResponse { version }))
}

#[derive(OpenApi)]
#[openapi(
    paths(get_settings, put_settings, engines, check_engine),
    components(schemas(AppSettings, EngineInfo, EngineCheckRequest, EngineCheckResponse))
)]
pub struct SettingsDoc;
