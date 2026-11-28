//! Persisted preferences and immutable engine configuration for new work.
pub(crate) mod open_sage;
use crate::domain_error::DomainError;
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
pub use open_sage::Installation as OpenSageInstallation;
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

pub(crate) fn sage_default_analysis_threads() -> u32 {
    u32::try_from(EngineId::NeonGammon.default_threads("analysis"))
        .expect("valid embedded NeonGammon analysis default threads")
}

pub(crate) fn sage_default_play_threads() -> u32 {
    u32::try_from(EngineId::NeonGammon.default_threads("play"))
        .expect("valid embedded NeonGammon play default threads")
}

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
        sage_threads(self.open_sage_threads, sage_default_analysis_threads())
    }

    pub(crate) fn sage_play_threads(&self) -> u32 {
        sage_threads(self.open_sage_play_threads, sage_default_play_threads())
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
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EngineCheckResponse {
    pub version: String,
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
    let mut tx = crate::database_pool::begin_read_snapshot(&state.pool).await?;
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

/// The same verified NeonGammon profile with cubeless evaluation, after the
/// real capability check for that profile.
pub(crate) async fn cubeless_sage(mut snapshot: EngineSnapshot) -> anyhow::Result<EngineSnapshot> {
    let sage = snapshot
        .open_sage
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("Missing verified NeonGammon installation"))?;
    sage.cubeless = true;
    snapshot.engine_version = Some(SageEngine::new(sage.clone())?.verify().await?);
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
        // `verified_profile` has just checked this profile in its own process.
        Some(snapshot) => SageEngine::new(snapshot.clone())?
            .saved_version()
            .map(str::to_owned),
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
async fn validate_threads(engine: EngineId, threads: Option<i32>) -> ApiResult<()> {
    let Some(threads) = threads else {
        return Ok(());
    };
    let minimum = engine.min_threads();
    let engine = engine.id();
    let maximum = crate::engine::host_parallel_threads()
        .await
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
pub(crate) const PLAY_MATCH_LENGTH_RULE: &str =
    "Human player must be 0 or 1 and match length 0 (money game) or 1..25";
pub(crate) const TUTOR_THRESHOLD_RULE: &str = "Threshold must be greater than zero and at most one";

/// The one rule for a stored or per-match human player name.
pub(crate) fn valid_player_name(name: &str) -> bool {
    name.chars().count() <= 120 && !name.chars().any(char::is_control)
}

pub(crate) fn valid_human_player(player: i32) -> bool {
    (0..=1).contains(&player)
}

pub(crate) fn valid_play_match_length(match_length: i32) -> bool {
    (0..=25).contains(&match_length)
}

pub(crate) fn valid_tutor_threshold(threshold: f64) -> bool {
    threshold.is_finite() && 0.0 < threshold && threshold <= 1.0
}

pub(crate) fn valid_engine_depth(engine: &str, purpose: &str, plies: i32) -> bool {
    crate::engines::EngineId::parse(engine)
        .is_some_and(|provider| provider.depths(purpose).contains(&plies))
}

fn validate(settings: &AppSettings) -> ApiResult<()> {
    if !valid_tutor_threshold(settings.tutor_threshold) {
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
        if !valid_engine_depth(id, purpose, plies) {
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

pub(crate) async fn save_settings(
    state: &AppState,
    settings: AppSettings,
) -> Result<AppSettings, DomainError> {
    let update = SettingsUpdate {
        play_top_two: Some(settings.play_top_two),
        gnubg_threads: Some(settings.gnubg_threads),
        gnubg_play_threads: Some(settings.gnubg_play_threads),
        open_sage_threads: Some(settings.open_sage_threads),
        open_sage_play_threads: Some(settings.open_sage_play_threads),
        parallel_analysis_jobs: Some(settings.parallel_analysis_jobs),
        settings,
    };
    save_settings_update(state, update).await
}

async fn save_settings_update(
    state: &AppState,
    update: SettingsUpdate,
) -> Result<AppSettings, DomainError> {
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
    let current = load(state).await?;
    if current.version != settings.version {
        return Err(conflict().into());
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
            return Err(DomainError::invalid(format!(
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
            validate_threads(engine, proposed).await?;
        }
    }
    // Validate before the compare-and-swap. No partial changes are ever persisted,
    // and the final version predicate also fences concurrent checks/writes.
    let version = settings
        .gnubg()
        .check_installation()
        .await
        .map_err(|e| DomainError::invalid(format!("GNUbg installation check failed: {e:#}")))?;
    if settings.gnubg_play_threads != current.gnubg_play_threads
        && settings.gnubg_play_threads != settings.gnubg_threads
    {
        Engine::new(settings.gnubg_path.clone())
            .with_threads(settings.gnubg_play_threads)
            .check_installation()
            .await
            .map_err(|e| DomainError::invalid(format!("GNUbg installation check failed: {e:#}")))?;
    }
    open_sage::verify_settings(&state.pool, &current, &settings)
        .await
        .map_err(|e| {
            DomainError::invalid(format!("NeonGammon installation check failed: {e:#}"))
        })?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    crate::configuration::store::lock(&mut tx)
        .await
        .map_err(anyhow::Error::from)?;
    let latest: Option<i64> =
        sqlx::query_scalar("SELECT version FROM app_settings WHERE singleton=true FOR UPDATE")
            .fetch_optional(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    if latest != Some(settings.version) {
        return Err(conflict().into());
    }
    crate::configuration::store::save_app(&mut tx, &settings).await?;
    // The rest belongs to this installation, not to a profile.
    let saved: AppSettings = sqlx::query_as("UPDATE app_settings SET version=$1+1,gnubg_path=$2,gnubg_version=$3,gnubg_threads=$4,open_sage_threads=$5,gnubg_play_threads=$6,open_sage_play_threads=$7,parallel_analysis_jobs=$8 WHERE singleton=true RETURNING *")
        .bind(settings.version)
        .bind(settings.gnubg_path)
        .bind(version)
        .bind(settings.gnubg_threads)
        .bind(settings.open_sage_threads)
        .bind(settings.gnubg_play_threads)
        .bind(settings.open_sage_play_threads)
        .bind(settings.parallel_analysis_jobs)
        .fetch_one(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(saved)
}

#[utoipa::path(put, path="/api/settings", request_body=AppSettings, responses((status=200,body=AppSettings),(status=409,body=ErrorBody),(status=422,body=ErrorBody)), tag="Settings")]
async fn put_settings(
    State(state): State<AppState>,
    Json(update): Json<SettingsUpdate>,
) -> ApiResult<Json<AppSettings>> {
    Ok(Json(save_settings_update(&state, update).await?))
}
fn conflict() -> ApiError {
    ApiError(
        StatusCode::CONFLICT,
        "Settings changed; reload the latest settings before saving".into(),
    )
}

pub(crate) async fn list_engines(state: &AppState) -> Result<Vec<EngineInfo>, DomainError> {
    let saved = load_row(state).await?;
    let settings = saved.settings;
    let running = crate::benchmarks::has_live_run(&state.pool).await?;
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
    let cpu_threads = crate::engine::host_parallel_threads().await;
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
    Ok(vec![
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
    ])
}

#[utoipa::path(get, path="/api/engines", responses((status=200,body=Vec<EngineInfo>)), tag="Settings")]
async fn engines(State(state): State<AppState>) -> ApiResult<Json<Vec<EngineInfo>>> {
    Ok(Json(list_engines(&state).await?))
}

pub(crate) async fn check_gnubg_installation(
    path: String,
    threads: Option<i32>,
) -> Result<EngineCheckResponse, DomainError> {
    validate_path(&path)?;
    validate_threads(EngineId::Gnubg, threads).await?;
    let version = Engine::new(path)
        .with_threads(threads)
        .check_installation()
        .await
        .map_err(|e| DomainError::invalid(format!("GNUbg installation check failed: {e:#}")))?;
    Ok(EngineCheckResponse { version })
}

#[utoipa::path(post, path="/api/engines/gnubg/check", request_body=EngineCheckRequest, responses((status=200,body=EngineCheckResponse),(status=422,body=ErrorBody)), tag="Settings")]
async fn check_engine(
    Json(request): Json<EngineCheckRequest>,
) -> ApiResult<Json<EngineCheckResponse>> {
    Ok(Json(
        check_gnubg_installation(request.path, request.threads).await?,
    ))
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
