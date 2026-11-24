//! Persisted preferences and immutable engine configuration for new work.
pub(crate) mod open_sage;
use crate::engines::{AnalysisEngine, EngineFork, EngineId, SageEngine, SageSnapshot};
use crate::json_errors::Json;
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    engine::Engine,
};
use axum::{
    Router,
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
    /// Depth in NeonDojo's ply convention; 1-ply is the raw evaluation.
    #[schema(minimum = 1, maximum = 5, default = 3)]
    pub analysis_plies: i32,
    pub play_engine: String,
    /// Depth in NeonDojo's ply convention; 1-ply is the raw evaluation.
    #[schema(minimum = 1, maximum = 3, default = 1)]
    pub play_plies: i32,
    /// Rescore the two closest NeonGammon 2-ply checker plays at 3-ply.
    #[serde(default)]
    pub play_top_two: bool,
    pub gnubg_path: String,
    /// Host-local override for analyses and engine duels. Null uses one GNUbg calculation thread. Omit on PUT to keep the saved value.
    #[schema(minimum = 1)]
    pub gnubg_threads: Option<i32>,
    /// Host-local override for playing. Null uses one GNUbg calculation thread. Omit on PUT to keep the saved value.
    #[schema(minimum = 1)]
    pub gnubg_play_threads: Option<i32>,
    /// Host-local override for analyses and engine duels. Null uses four threads. Omit on PUT to keep the saved value.
    #[schema(minimum = 2)]
    pub open_sage_threads: Option<i32>,
    /// Host-local override for playing. Null uses two threads. Omit on PUT to keep the saved value.
    #[schema(minimum = 2)]
    pub open_sage_play_threads: Option<i32>,
    /// Host-local limit for simultaneously running analysis jobs. Null runs one job at a time. Omit on PUT to keep the saved value.
    #[schema(minimum = 1, maximum = 8)]
    pub parallel_analysis_jobs: Option<i32>,
    #[serde(default = "default_bearoff_direction")]
    pub bearoff_direction: String,
}

#[derive(Deserialize)]
struct SettingsUpdate {
    #[serde(flatten)]
    settings: AppSettings,
    play_top_two: Option<bool>,
    #[serde(default, deserialize_with = "thread_override")]
    gnubg_threads: Option<Option<i32>>,
    #[serde(default, deserialize_with = "thread_override")]
    gnubg_play_threads: Option<Option<i32>>,
    #[serde(default, deserialize_with = "thread_override")]
    open_sage_threads: Option<Option<i32>>,
    #[serde(default, deserialize_with = "thread_override")]
    open_sage_play_threads: Option<Option<i32>>,
    #[serde(default, deserialize_with = "thread_override")]
    parallel_analysis_jobs: Option<Option<i32>>,
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
    /// GNUbg with the analysis thread count; playing uses `gnubg_play_threads`.
    pub(crate) fn gnubg(&self) -> Engine {
        Engine::new(self.gnubg_path.clone()).with_threads(self.gnubg_threads)
    }

    pub(crate) fn sage_analysis_threads(&self) -> u32 {
        sage_threads(self.open_sage_threads, SAGE_ANALYSIS_THREADS)
    }

    pub(crate) fn sage_play_threads(&self) -> u32 {
        sage_threads(self.open_sage_play_threads, SAGE_PLAY_THREADS)
    }
}

/// Number of analysis jobs the worker may run at the same time.
pub(crate) fn analysis_job_slots(configured: Option<i32>) -> usize {
    let (minimum, maximum, default) = crate::configuration::parallel_analysis_jobs();
    let slots = configured.unwrap_or(default).clamp(minimum, maximum);
    usize::try_from(slots).unwrap_or(1)
}

pub(crate) async fn load_analysis_job_slots(pool: &sqlx::PgPool) -> anyhow::Result<usize> {
    let configured: Option<Option<i32>> =
        sqlx::query_scalar("SELECT parallel_analysis_jobs FROM app_settings WHERE singleton=true")
            .fetch_optional(pool)
            .await?;
    Ok(analysis_job_slots(configured.flatten()))
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
    /// Whether the verified NeonGammon wheel provides named opponent presets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difficulty_presets: Option<bool>,
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
    /// Saved checker move filter. Older snapshots use GNUbg startup defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gnubg_move_filter: Option<crate::engine::GnubgMoveFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_sage: Option<SageSnapshot>,
}
impl EngineSnapshot {
    pub fn fingerprint(&self) -> String {
        let value = serde_json::to_vec(self).expect("engine snapshot is serializable");
        format!("{:x}", Sha256::digest(value))
    }
    pub(crate) fn provider(&self) -> Option<EngineId> {
        EngineId::parse(&self.engine_id)
    }
    pub fn engine(&self) -> anyhow::Result<AnalysisEngine> {
        match self.provider() {
            Some(EngineId::Gnubg) => Ok(AnalysisEngine::Gnubg(
                Engine::new(self.gnubg_path.clone())
                    .with_threads(self.gnubg_threads)
                    .with_move_filter(self.gnubg_move_filter)
                    .with_expected_version(self.engine_version.clone()),
            )),
            Some(EngineId::NeonGammon) => Ok(AnalysisEngine::OpenSage(SageEngine::new(
                self.open_sage
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("Missing saved NeonGammon installation"))?,
            )?)),
            None => anyhow::bail!("Unsupported saved job engine"),
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

async fn ensure_settings(state: &AppState) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO app_settings(singleton,gnubg_path) VALUES(true,$1) ON CONFLICT(singleton) DO NOTHING")
        .bind(state.engine.binary()).execute(&state.pool).await?;
    Ok(())
}

async fn load_row(state: &AppState) -> anyhow::Result<SavedSettings> {
    ensure_settings(state).await?;
    Ok(
        sqlx::query_as("SELECT * FROM app_settings WHERE singleton=true")
            .fetch_one(&state.pool)
            .await?,
    )
}
pub(crate) async fn load(state: &AppState) -> anyhow::Result<AppSettings> {
    Ok(load_row(state).await?.settings)
}

pub(crate) async fn load_play_settings(
    state: &AppState,
) -> anyhow::Result<(AppSettings, crate::configuration::ErrorThresholds)> {
    ensure_settings(state).await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let settings = sqlx::query_as("SELECT * FROM app_settings WHERE singleton=true")
        .fetch_one(&mut *tx)
        .await?;
    let thresholds = crate::configuration::store::error_thresholds(&mut *tx).await?;
    tx.commit().await?;
    Ok((settings, thresholds))
}

pub(crate) async fn snapshot_for_analysis(
    state: &AppState,
    requested_plies: Option<i32>,
    requested_engine: Option<&str>,
) -> anyhow::Result<(AppSettings, EngineSnapshot)> {
    let saved = load_row(state).await?;
    let plies = requested_plies.unwrap_or(saved.settings.analysis_plies);
    let mut snapshot = analysis_snapshot(state, &saved.settings, requested_engine, plies).await?;
    snapshot.gnubg_move_filter =
        crate::engine::GnubgMoveFilter::for_analysis(&snapshot.engine_id, plies);
    Ok((saved.settings, snapshot))
}

pub(crate) async fn snapshot_for_practice(
    state: &AppState,
) -> anyhow::Result<(i32, EngineSnapshot)> {
    let settings = load(state).await?;
    let plies = settings
        .analysis_plies
        .min(EngineId::require(&settings.analysis_engine)?.practice_max_plies());
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
        settings.sage_analysis_threads(),
        plies,
    )
    .await
}

/// The same verified NeonGammon profile with cubeless evaluation. Starting the
/// worker for its version runs the real capability check for that profile.
pub(crate) async fn cubeless_sage(mut snapshot: EngineSnapshot) -> anyhow::Result<EngineSnapshot> {
    let sage = snapshot
        .open_sage
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("Missing verified NeonGammon installation"))?;
    sage.cubeless = true;
    snapshot.engine_version = Some(SageEngine::new(sage.clone())?.version().await?);
    Ok(snapshot)
}

pub(crate) async fn snapshot_for(
    state: &AppState,
    id: &str,
    gnubg_path: &str,
    gnubg_threads: Option<i32>,
    sage_threads: u32,
    plies: i32,
) -> anyhow::Result<EngineSnapshot> {
    let provider = EngineId::parse(id).ok_or_else(|| anyhow::anyhow!("Unsupported engine"))?;
    let open_sage = if provider.requires_verified_profile() {
        Some(open_sage::verified_profile(&state.pool, plies, sage_threads).await?)
    } else {
        None
    };
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
        gnubg_threads: gnubg_threads
            .filter(|&threads| provider.uses_gnubg_threads() && threads != 1),
        gnubg_move_filter: None,
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

pub(crate) const TOP_TWO_PROFILE_ERROR: &str = "Top-2 play requires NeonGammon 2-ply";

pub(crate) fn supports_top_two(engine: &str, plies: i32) -> bool {
    EngineId::parse(engine).is_some_and(|engine| engine.supports_top_two(plies))
}
fn validate_threads(engine: EngineId, threads: Option<i32>) -> ApiResult<()> {
    let Some(threads) = threads else {
        return Ok(());
    };
    let minimum = engine.min_threads();
    let engine = engine.id();
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

pub(crate) const PLAYER_NAME_RULE: &str =
    "Player name must be at most 120 characters and contain no control characters";

/// The one rule for a stored or per-match human player name.
pub(crate) fn valid_player_name(name: &str) -> bool {
    name.chars().count() <= 120 && !name.chars().any(char::is_control)
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
    if !valid_player_name(&settings.player_name) {
        return Err(ApiError::invalid(PLAYER_NAME_RULE));
    }
    if settings.play_top_two && !supports_top_two(&settings.play_engine, settings.play_plies) {
        return Err(ApiError::invalid(TOP_TWO_PROFILE_ERROR));
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
    Json(update): Json<SettingsUpdate>,
) -> ApiResult<Json<AppSettings>> {
    let SettingsUpdate {
        mut settings,
        play_top_two,
        gnubg_threads,
        gnubg_play_threads,
        open_sage_threads,
        open_sage_play_threads,
        parallel_analysis_jobs,
    } = update;
    settings.play_top_two = play_top_two.unwrap_or(false);
    validate(&settings)?;
    let current = load(&state).await?;
    if current.version != settings.version {
        return Err(conflict());
    }
    if play_top_two.is_none() && supports_top_two(&settings.play_engine, settings.play_plies) {
        settings.play_top_two = current.play_top_two;
    }
    settings.gnubg_threads = gnubg_threads.unwrap_or(current.gnubg_threads);
    settings.gnubg_play_threads = gnubg_play_threads.unwrap_or(current.gnubg_play_threads);
    settings.open_sage_threads = open_sage_threads.unwrap_or(current.open_sage_threads);
    settings.open_sage_play_threads =
        open_sage_play_threads.unwrap_or(current.open_sage_play_threads);
    settings.parallel_analysis_jobs =
        parallel_analysis_jobs.unwrap_or(current.parallel_analysis_jobs);
    if settings.parallel_analysis_jobs != current.parallel_analysis_jobs
        && let Some(jobs) = settings.parallel_analysis_jobs
    {
        let (minimum, maximum, _) = crate::configuration::parallel_analysis_jobs();
        if !(minimum..=maximum).contains(&jobs) {
            return Err(ApiError::invalid(format!(
                "parallel_analysis_jobs must be between {minimum} and {maximum}"
            )));
        }
    }
    // Existing settings and submitted jobs can outlive a change in CPU availability.
    for (engine, proposed, saved) in [
        (
            EngineId::Gnubg,
            settings.gnubg_threads,
            current.gnubg_threads,
        ),
        (
            EngineId::Gnubg,
            settings.gnubg_play_threads,
            current.gnubg_play_threads,
        ),
        (
            EngineId::NeonGammon,
            settings.open_sage_threads,
            current.open_sage_threads,
        ),
        (
            EngineId::NeonGammon,
            settings.open_sage_play_threads,
            current.open_sage_play_threads,
        ),
    ] {
        if proposed != saved {
            validate_threads(engine, proposed)?;
        }
    }
    // Validate before the compare-and-swap. No partial changes are ever persisted,
    // and the final version predicate also fences concurrent checks/writes.
    let version = settings
        .gnubg()
        .check_installation()
        .await
        .map_err(|e| ApiError::invalid(format!("GNUbg installation check failed: {e:#}")))?;
    if settings.gnubg_play_threads != current.gnubg_play_threads
        && settings.gnubg_play_threads != settings.gnubg_threads
    {
        Engine::new(settings.gnubg_path.clone())
            .with_threads(settings.gnubg_play_threads)
            .check_installation()
            .await
            .map_err(|e| ApiError::invalid(format!("GNUbg installation check failed: {e:#}")))?;
    }
    open_sage::verify_settings(&state.pool, &current, &settings)
        .await
        .map_err(|e| ApiError::invalid(format!("NeonGammon installation check failed: {e:#}")))?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    crate::configuration::store::lock(&mut tx)
        .await
        .map_err(anyhow::Error::from)?;
    let saved: Option<AppSettings> = sqlx::query_as("UPDATE app_settings SET version=version+1,player_name=$2,analysis_engine=$3,analysis_plies=$4,play_engine=$5,play_plies=$6,gnubg_path=$7,gnubg_version=$8,bearoff_direction=$9,default_play_mode=$10,tutor_threshold=$11,training_auto_analysis=$12,gnubg_threads=$13,open_sage_threads=$14,gnubg_play_threads=$15,open_sage_play_threads=$16,parallel_analysis_jobs=$17,play_top_two=$18 WHERE singleton=true AND version=$1 RETURNING *")
        .bind(settings.version).bind(settings.player_name).bind(settings.analysis_engine).bind(settings.analysis_plies)
        .bind(settings.play_engine).bind(settings.play_plies).bind(settings.gnubg_path).bind(version).bind(settings.bearoff_direction).bind(settings.default_play_mode).bind(settings.tutor_threshold).bind(settings.training_auto_analysis)
        .bind(settings.gnubg_threads).bind(settings.open_sage_threads)
        .bind(settings.gnubg_play_threads).bind(settings.open_sage_play_threads)
        .bind(settings.parallel_analysis_jobs)
        .bind(settings.play_top_two)
        .fetch_optional(&mut *tx).await.map_err(anyhow::Error::from)?;
    let saved = saved.ok_or_else(conflict)?;
    crate::configuration::store::legacy_app(&mut tx, &saved).await?;
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
    let threads = settings.sage_analysis_threads();
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
        EngineId::NeonGammon.depths("analysis")
    } else {
        EngineId::NeonGammon
            .depths("analysis")
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
            id: EngineId::Gnubg.id().into(),
            name: EngineId::Gnubg.display_name().into(),
            available: version.is_some(),
            version,
            fork: None,
            plies: EngineId::Gnubg.depths("analysis"),
            difficulty_presets: None,
            max_parallel_threads: gnubg_parallel_limit,
        },
        EngineInfo {
            id: EngineId::NeonGammon.id().into(),
            name: EngineId::NeonGammon.display_name().into(),
            available: sage_version.is_some(),
            version: sage_version,
            fork: sage.as_ref().and_then(SageSnapshot::known_fork),
            plies: sage_plies,
            difficulty_presets: Some(
                sage.as_ref()
                    .is_some_and(SageSnapshot::supports_difficulty_presets),
            ),
            max_parallel_threads: cpu_threads,
        },
    ]))
}
#[utoipa::path(post, path="/api/engines/gnubg/check", request_body=EngineCheckRequest, responses((status=200,body=EngineCheckResponse),(status=422,body=ErrorBody)), tag="Settings")]
async fn check_engine(
    Json(request): Json<EngineCheckRequest>,
) -> ApiResult<Json<EngineCheckResponse>> {
    validate_path(&request.path)?;
    validate_threads(EngineId::Gnubg, request.threads)?;
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
