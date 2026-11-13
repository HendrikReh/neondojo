//! Persisted preferences and immutable engine configuration for new work.
pub(crate) mod open_sage;
use crate::engines::{AnalysisEngine, EngineFork, SageEngine, SageSnapshot};
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
    #[serde(default = "crate::play::default_play_mode")]
    pub default_play_mode: crate::play::PlayMode,
    #[serde(default = "crate::play::default_threshold")]
    pub tutor_threshold: f64,
    #[serde(default = "crate::play::default_auto_analysis")]
    pub training_auto_analysis: bool,
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
    /// Host-local override. Null uses one GNUbg calculation thread. Omit on PUT to keep the saved value.
    #[schema(minimum = 1)]
    pub gnubg_threads: Option<i32>,
    /// Host-local override. Null uses four threads for analysis and two for play. Omit on PUT to keep the saved value.
    #[schema(minimum = 2)]
    pub open_sage_threads: Option<i32>,
    #[serde(default = "default_bearoff_direction")]
    pub bearoff_direction: String,
}

#[derive(Deserialize)]
struct SettingsUpdate {
    #[serde(flatten)]
    settings: AppSettings,
    #[serde(default, deserialize_with = "thread_override")]
    gnubg_threads: Option<Option<i32>>,
    #[serde(default, deserialize_with = "thread_override")]
    open_sage_threads: Option<Option<i32>>,
}

// Missing fields keep their saved values. Explicit null restores the default.
fn thread_override<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<i32>>, D::Error> {
    Option::deserialize(deserializer).map(Some)
}

pub(crate) const SAGE_ANALYSIS_THREADS: u32 = 4;
pub(crate) const SAGE_PLAY_THREADS: u32 = 2;

pub(crate) fn sage_threads(configured: Option<i32>, default: u32) -> u32 {
    configured
        .and_then(|threads| u32::try_from(threads).ok())
        .unwrap_or(default)
}

impl AppSettings {
    pub(crate) fn gnubg(&self) -> Engine {
        Engine::new(self.gnubg_path.clone()).with_threads(self.gnubg_threads)
    }
}

fn default_bearoff_direction() -> String {
    "right".into()
}

#[derive(FromRow)]
struct SavedSettings {
    #[sqlx(flatten)]
    settings: AppSettings,
    #[sqlx(rename = "gnubg_version")]
    _gnubg_version: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EngineInfo {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork: Option<EngineFork>,
    pub plies: Vec<i32>,
    /// Upper bound on simultaneous CPU execution for this engine on the backend host.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 1)]
    pub max_parallel_threads: Option<i32>,
}

#[derive(Deserialize, ToSchema)]
struct EngineCheckRequest {
    path: String,
    #[schema(minimum = 1)]
    threads: Option<i32>,
}
#[derive(Serialize, ToSchema)]
struct EngineCheckResponse {
    version: String,
}

/// Internal job payload, never derived again from mutable application settings.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EngineSnapshot {
    pub engine_id: String,
    pub gnubg_path: String,
    pub engine_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 1)]
    pub gnubg_threads: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_sage: Option<SageSnapshot>,
}
impl EngineSnapshot {
    pub fn fingerprint(&self) -> String {
        let value = serde_json::to_vec(self).expect("engine snapshot is serializable");
        format!("{:x}", Sha256::digest(value))
    }
    pub fn engine(&self) -> anyhow::Result<AnalysisEngine> {
        match self.engine_id.as_str() {
            "gnubg" => Ok(AnalysisEngine::Gnubg(
                Engine::new(self.gnubg_path.clone())
                    .with_threads(self.gnubg_threads)
                    .with_expected_version(self.engine_version.clone()),
            )),
            "bgsage" => Ok(AnalysisEngine::OpenSage(SageEngine::new(
                self.open_sage
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("Missing saved Open Sage installation"))?,
            )?)),
            _ => anyhow::bail!("Unsupported saved job engine"),
        }
    }
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .merge(open_sage::routes())
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
pub(crate) async fn snapshot(
    state: &AppState,
    requested_plies: Option<i32>,
) -> anyhow::Result<(AppSettings, EngineSnapshot)> {
    snapshot_for_analysis(state, requested_plies, None).await
}

pub(crate) async fn snapshot_for_analysis(
    state: &AppState,
    requested_plies: Option<i32>,
    requested_engine: Option<&str>,
) -> anyhow::Result<(AppSettings, EngineSnapshot)> {
    let saved = load_row(state).await?;
    let plies = requested_plies.unwrap_or(saved.settings.analysis_plies);
    let snapshot = analysis_snapshot(state, &saved.settings, requested_engine, plies).await?;
    Ok((saved.settings, snapshot))
}

pub(crate) async fn snapshot_for_practice(
    state: &AppState,
) -> anyhow::Result<(i32, EngineSnapshot)> {
    let settings = load(state).await?;
    let plies = settings.analysis_plies.min(2);
    Ok((
        plies,
        analysis_snapshot(state, &settings, None, plies).await?,
    ))
}

async fn analysis_snapshot(
    state: &AppState,
    settings: &AppSettings,
    requested_engine: Option<&str>,
    plies: i32,
) -> anyhow::Result<EngineSnapshot> {
    // Record the installed version even before the first settings save, and
    // distinguish an executable upgraded in place from an existing active job.
    // Unavailable initial/fake engines still produce a job whose worker reports
    // its normal durable failure; reading settings never probes the executable.
    snapshot_for(
        state,
        requested_engine.unwrap_or(&settings.analysis_engine),
        &settings.gnubg_path,
        settings.gnubg_threads,
        sage_threads(settings.open_sage_threads, SAGE_ANALYSIS_THREADS),
        plies,
    )
    .await
}

pub(crate) async fn snapshot_for(
    state: &AppState,
    id: &str,
    gnubg_path: &str,
    gnubg_threads: Option<i32>,
    sage_threads: u32,
    plies: i32,
) -> anyhow::Result<EngineSnapshot> {
    let open_sage = if id == "bgsage" {
        Some(open_sage::verified_profile(&state.pool, plies, sage_threads).await?)
    } else {
        None
    };
    anyhow::ensure!(matches!(id, "gnubg" | "bgsage"), "Unsupported engine");
    let engine_version = match &open_sage {
        Some(snapshot) => Some(SageEngine::new(snapshot.clone())?.version().await?),
        None => {
            let current = Engine::new(gnubg_path.to_owned()).version().await.ok();
            let saved = load_row(state).await?;
            current.or_else(|| {
                (saved.settings.gnubg_path == gnubg_path)
                    .then_some(saved._gnubg_version)
                    .flatten()
            })
        }
    };
    Ok(EngineSnapshot {
        engine_id: id.into(),
        gnubg_path: gnubg_path.into(),
        engine_version,
        gnubg_threads: gnubg_threads.filter(|&threads| id == "gnubg" && threads != 1),
        open_sage,
    })
}

fn validate_path(path: &str) -> ApiResult<()> {
    if path.trim().is_empty() || path.len() > 4096 || path.chars().any(char::is_control) {
        return Err(ApiError::invalid(
            "GNUbg path must be nonempty, at most 4096 bytes and contain no control characters",
        ));
    }
    Ok(())
}
fn validate_threads(engine: &str, threads: Option<i32>) -> ApiResult<()> {
    let Some(threads) = threads else {
        return Ok(());
    };
    let minimum = crate::configuration::engine_min_threads(engine);
    let maximum = crate::configuration::available_cpu_threads()
        .filter(|&maximum| maximum >= minimum)
        .ok_or_else(|| {
            ApiError::invalid(format!(
                "No configurable thread range is available for {engine}"
            ))
        })?;
    if !(minimum..=maximum).contains(&threads) {
        return Err(ApiError::invalid(format!(
            "{engine} threads must be between {minimum} and {maximum}"
        )));
    }
    Ok(())
}

fn validate(settings: &AppSettings) -> ApiResult<()> {
    if !(settings.tutor_threshold.is_finite()
        && 0.0 < settings.tutor_threshold
        && settings.tutor_threshold <= 1.0)
    {
        return Err(ApiError::invalid(
            "tutor_threshold must be greater than 0 and at most 1",
        ));
    }
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
    for (id, purpose, plies) in [
        (
            &settings.analysis_engine,
            "analysis",
            settings.analysis_plies,
        ),
        (&settings.play_engine, "play", settings.play_plies),
    ] {
        if !crate::configuration::engine_depths(id, purpose).contains(&plies) {
            return Err(ApiError::invalid(
                "Engine or depth is not verified for this purpose",
            ));
        }
    }
    if !matches!(settings.bearoff_direction.as_str(), "right" | "left") {
        return Err(ApiError::invalid("bearoff direction must be right or left"));
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
    payload: Result<Json<SettingsUpdate>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<AppSettings>> {
    let Json(SettingsUpdate {
        mut settings,
        gnubg_threads,
        open_sage_threads,
    }) = payload.map_err(|e| ApiError::invalid(e.body_text()))?;
    settings.gnubg_threads = gnubg_threads.flatten();
    settings.open_sage_threads = open_sage_threads.flatten();
    validate(&settings)?;
    let current = load(&state).await?;
    if current.version != settings.version {
        return Err(conflict());
    }
    settings.gnubg_threads = gnubg_threads.unwrap_or(current.gnubg_threads);
    settings.open_sage_threads = open_sage_threads.unwrap_or(current.open_sage_threads);
    // Existing settings and submitted jobs can outlive a change in CPU availability.
    if settings.gnubg_threads != current.gnubg_threads {
        validate_threads("gnubg", settings.gnubg_threads)?;
    }
    if settings.open_sage_threads != current.open_sage_threads {
        validate_threads("bgsage", settings.open_sage_threads)?;
    }
    // Validate before the compare-and-swap. No partial changes are ever persisted,
    // and the final version predicate also fences concurrent checks/writes.
    let version = settings
        .gnubg()
        .check_installation()
        .await
        .map_err(|e| ApiError::invalid(format!("GNUbg installation check failed: {e:#}")))?;
    open_sage::verify_settings(&state.pool, &current, &settings)
        .await
        .map_err(|e| ApiError::invalid(format!("Open Sage installation check failed: {e:#}")))?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    crate::configuration::store::lock(&mut tx)
        .await
        .map_err(anyhow::Error::from)?;
    let saved: Option<AppSettings> = sqlx::query_as("UPDATE app_settings SET version=version+1,player_name=$2,analysis_engine=$3,analysis_plies=$4,play_engine=$5,play_plies=$6,gnubg_path=$7,gnubg_version=$8,bearoff_direction=$9,default_play_mode=$10,tutor_threshold=$11,training_auto_analysis=$12,gnubg_threads=$13,open_sage_threads=$14 WHERE singleton=true AND version=$1 RETURNING *")
        .bind(settings.version).bind(settings.player_name).bind(settings.analysis_engine).bind(settings.analysis_plies)
        .bind(settings.play_engine).bind(settings.play_plies).bind(settings.gnubg_path).bind(version).bind(settings.bearoff_direction).bind(settings.default_play_mode).bind(settings.tutor_threshold).bind(settings.training_auto_analysis)
        .bind(settings.gnubg_threads).bind(settings.open_sage_threads)
        .fetch_optional(&mut *tx).await.map_err(anyhow::Error::from)?;
    let saved = saved.ok_or_else(conflict)?;
    crate::configuration::store::legacy_app(&mut tx, &saved)
        .await
        .map_err(crate::configuration::store::legacy_error)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(saved))
}
fn conflict() -> ApiError {
    ApiError(
        StatusCode::CONFLICT,
        "Settings changed; reload the latest settings before saving".into(),
    )
}

#[utoipa::path(get, path="/api/engines", responses((status=200,body=Vec<EngineInfo>)), tag="Settings")]
async fn engines(State(state): State<AppState>) -> ApiResult<Json<Vec<EngineInfo>>> {
    let saved = load_row(&state).await?;
    let settings = saved.settings;
    let running: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM benchmark_runs WHERE status='running')")
            .fetch_one(&state.pool)
            .await
            .map_err(anyhow::Error::from)?;
    let version = if running {
        saved
            ._gnubg_version
            .or(
                crate::benchmarks::running_engine_version(&state.pool, &settings.gnubg_path)
                    .await?,
            )
    } else {
        match crate::benchmarks::running_engine_version(&state.pool, &settings.gnubg_path).await? {
            Some(version) => Some(version),
            None => settings.gnubg().check_installation().await.ok(),
        }
    };
    let threads = sage_threads(settings.open_sage_threads, SAGE_ANALYSIS_THREADS);
    let sage = if running {
        open_sage::load(&state.pool)
            .await?
            .snapshot
            .map(|mut snapshot| {
                snapshot.threads = threads;
                snapshot
            })
    } else {
        open_sage::verified(&state.pool, threads).await.ok()
    };
    let sage_version = match &sage {
        Some(s) => SageEngine::new(s.clone())?
            .saved_version()
            .map(str::to_owned),
        None => None,
    };
    let sage_plies = if sage
        .as_ref()
        .is_some_and(|snapshot| snapshot.one_ply_verified)
    {
        crate::configuration::engine_depths("bgsage", "analysis")
    } else {
        crate::configuration::engine_depths("bgsage", "analysis")
            .into_iter()
            .filter(|&depth| depth != 1)
            .collect()
    };
    let cpu_threads = crate::configuration::available_cpu_threads();
    let gnubg_parallel_limit = if !running
        && version.is_some()
        && let Some(cpu_threads) = cpu_threads
    {
        Engine::new(settings.gnubg_path.clone())
            .max_parallel_threads(cpu_threads)
            .await
            .ok()
    } else {
        None
    };
    Ok(Json(vec![
        EngineInfo {
            id: "gnubg".into(),
            name: "GNUbg".into(),
            available: version.is_some(),
            version,
            fork: None,
            plies: vec![0, 1, 2, 3, 4],
            max_parallel_threads: gnubg_parallel_limit,
        },
        EngineInfo {
            id: "bgsage".into(),
            name: "Open Sage".into(),
            available: sage_version.is_some(),
            version: sage_version,
            fork: sage.as_ref().and_then(SageSnapshot::known_fork),
            plies: sage_plies,
            max_parallel_threads: cpu_threads,
        },
    ]))
}
#[utoipa::path(post, path="/api/engines/gnubg/check", request_body=EngineCheckRequest, responses((status=200,body=EngineCheckResponse),(status=422,body=ErrorBody)), tag="Settings")]
async fn check_engine(
    payload: Result<Json<EngineCheckRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<EngineCheckResponse>> {
    let Json(request) = payload.map_err(|e| ApiError::invalid(e.body_text()))?;
    validate_path(&request.path)?;
    validate_threads("gnubg", request.threads)?;
    let version = Engine::new(request.path)
        .with_threads(request.threads)
        .check_installation()
        .await
        .map_err(|e| ApiError::invalid(format!("GNUbg installation check failed: {e:#}")))?;
    Ok(Json(EngineCheckResponse { version }))
}

#[derive(OpenApi)]
#[openapi(
    paths(get_settings, put_settings, engines, check_engine),
    components(schemas(
        AppSettings,
        EngineInfo,
        EngineFork,
        EngineCheckRequest,
        EngineCheckResponse
    ))
)]
pub struct SettingsDoc;
