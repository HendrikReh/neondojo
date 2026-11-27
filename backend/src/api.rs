mod analysis;
mod jobs;
mod matches;
mod openapi;

pub use analysis::{
    MatchAnalysisEngine, MatchAnalysisQuery, MatchAnalysisRequest, MoveAnalysisQuery,
};
pub(crate) use analysis::{
    get_latest_match_analysis, get_match_analysis_jobs, list_move_analyses, submit_match_analysis,
};
pub use jobs::{JobRequest, JobStatusBatch, PositionJobKind};
pub(crate) use jobs::{
    cancel_job_by_id, get_job_by_id, get_job_status_batch, submit_position_job,
    watch_job_events_checked,
};
pub use matches::{GameScope, ImportQuery, PositionQuery};
pub(crate) use matches::{
    get_match_by_id, get_match_dice, get_match_games, get_match_import_warnings,
    get_match_source_sgf, get_position_by_id, import_match_from_path, list_positions,
};
pub use openapi::{ApiDoc, BaseApiDoc};

use crate::json_errors::Json;
use crate::{configuration::ConfigurationIssue, engine::Engine};
use axum::{
    Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get, post},
};
use serde::Serialize;
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use tokio::sync::{Notify, Semaphore, broadcast};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub engine: Engine,
    pub wake: Arc<Notify>,
    pub(crate) browse_wake: Arc<Notify>,
    pub(crate) assessment_wake: Arc<Notify>,
    job_notifications: Arc<crate::job_events::JobNotifications>,
    pub(crate) benchmark_updates: broadcast::Sender<Uuid>,
    pub(crate) engine_match_updates: broadcast::Sender<Uuid>,
    pub(crate) assessment_updates: broadcast::Sender<Uuid>,
    pub(crate) anki_sync_updates: broadcast::Sender<Option<Uuid>>,
    pub shutdown: CancellationToken,
    pub(crate) background_tasks: TaskTracker,
    pub(crate) imports: Arc<Semaphore>,
    /// Serialize on-demand evaluations without holding a database lock.
    pub(crate) practice_evaluation: Arc<tokio::sync::Mutex<()>>,
    pub(crate) play_warmup: Arc<Semaphore>,
    pub(crate) opening_books: Arc<crate::opening_book::LookupCache>,
    pub(crate) backup: Arc<crate::database_backup::Access>,
    pub(crate) anki_limits: crate::learning::OperationLimits,
    pub(crate) anki_connection: Arc<crate::learning::AnkiConnection>,
    /// How often a running analysis job renews its lease.
    pub(crate) job_heartbeat: Duration,
    pub(crate) running_jobs: Arc<crate::jobs::RunningJobs>,
    pub(crate) openai: Arc<crate::match_assessment::OpenAi>,
    pub(crate) review_hit_queue: Arc<crate::review_decisions::HitRefreshQueue>,
}

impl AppState {
    pub fn new(pool: PgPool, engine: Engine) -> Self {
        Self {
            pool,
            engine,
            wake: Arc::new(Notify::new()),
            browse_wake: Arc::default(),
            assessment_wake: Arc::new(Notify::new()),
            job_notifications: Arc::default(),
            benchmark_updates: broadcast::channel(128).0,
            engine_match_updates: broadcast::channel(128).0,
            assessment_updates: broadcast::channel(128).0,
            anki_sync_updates: broadcast::channel(128).0,
            shutdown: CancellationToken::new(),
            background_tasks: TaskTracker::new(),
            imports: Arc::new(Semaphore::new(2)),
            practice_evaluation: Arc::default(),
            play_warmup: Arc::new(Semaphore::new(1)),
            opening_books: Arc::default(),
            backup: Arc::default(),
            anki_limits: Default::default(),
            anki_connection: Arc::default(),
            job_heartbeat: crate::jobs::DEFAULT_HEARTBEAT,
            running_jobs: Arc::default(),
            openai: Arc::new(crate::match_assessment::OpenAi::from_env()),
            review_hit_queue: Arc::default(),
        }
    }

    pub fn subscribe_benchmark_updates(&self) -> broadcast::Receiver<Uuid> {
        self.benchmark_updates.subscribe()
    }

    pub fn subscribe_engine_match_updates(&self) -> broadcast::Receiver<Uuid> {
        self.engine_match_updates.subscribe()
    }

    pub fn subscribe_assessment_updates(&self) -> broadcast::Receiver<Uuid> {
        self.assessment_updates.subscribe()
    }

    pub fn subscribe_anki_sync_updates(&self) -> broadcast::Receiver<Option<Uuid>> {
        self.anki_sync_updates.subscribe()
    }

    pub(crate) fn notify_benchmark_update(&self, id: Uuid) {
        let _ = self.benchmark_updates.send(id);
    }

    pub(crate) fn notify_engine_match_update(&self, id: Uuid) {
        let _ = self.engine_match_updates.send(id);
    }

    pub(crate) fn notify_assessment_update(&self, report_id: Uuid) {
        let _ = self.assessment_updates.send(report_id);
    }

    pub(crate) fn notify_anki_sync_update(&self, deck_id: Option<Uuid>) {
        let _ = self.anki_sync_updates.send(deck_id);
    }

    /// Sets how often running analysis jobs renew their lease. Each renewal
    /// extends the lease by [`crate::jobs::LEASE`], so the interval must stay
    /// at most half of it; at least one renewal then fits into every lease.
    ///
    /// # Panics
    ///
    /// Panics when the interval is zero or longer than half the lease.
    pub fn with_job_heartbeat(mut self, interval: Duration) -> Self {
        assert!(
            !interval.is_zero() && interval <= crate::jobs::LEASE / 2,
            "job heartbeat must be positive and at most half the {:?} lease",
            crate::jobs::LEASE
        );
        self.job_heartbeat = interval;
        self
    }

    /// Uses this OpenAI key instead of the one in the environment. An
    /// implausible key, such as an empty one, leaves AI assessments
    /// unconfigured. Only the assessment worker calls the provider.
    pub fn with_openai_key(mut self, key: &str) -> Self {
        self.openai = Arc::new(crate::match_assessment::OpenAi::with_key(key));
        self
    }

    /// Sets how long a database restore waits for background work it cannot
    /// stop (`preflight`) and then for running requests and stopped analysis
    /// jobs to release the database (`drain`). Call it before sharing the state.
    #[cfg(test)]
    pub(crate) fn with_restore_waits(mut self, preflight: Duration, drain: Duration) -> Self {
        self.backup = Arc::new(crate::database_backup::Access::with_waits(preflight, drain));
        self
    }
}

/// The body of every API error.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: String,
    /// A stable identifier of the problem, where the endpoint defines one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub code: Option<String>,
    /// The configuration values that caused the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub issues: Option<Vec<ConfigurationIssue>>,
}
impl<T: Into<String>> From<T> for ErrorBody {
    fn from(error: T) -> Self {
        Self {
            error: error.into(),
            code: None,
            issues: None,
        }
    }
}
#[derive(Debug)]
pub struct ApiError(pub(crate) StatusCode, pub(crate) ErrorBody);
impl ApiError {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self(StatusCode::UNPROCESSABLE_ENTITY, message.into().into())
    }
    /// The same error as HTTP 422, for a path that documents that status.
    pub(crate) fn into_invalid(self) -> Self {
        Self(StatusCode::UNPROCESSABLE_ENTITY, self.1)
    }
    pub(crate) fn missing() -> Self {
        Self(StatusCode::NOT_FOUND, "Not found".into())
    }
}
impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!(error=?error, "Request failed");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error; see backend log".into(),
        )
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(self.1)).into_response()
    }
}
pub(crate) type ApiResult<T> = Result<T, ApiError>;

pub fn router(state: AppState) -> Router {
    router_with_web_dir(state, None)
}

pub fn router_with_web_dir(state: AppState, web_dir: Option<&std::path::Path>) -> Router {
    let mut app = Router::new()
        .merge(crate::settings::routes())
        .merge(crate::configuration::routes())
        .merge(crate::profiles::routes())
        .merge(crate::benchmarks::routes())
        .merge(crate::engine_matches::routes())
        .merge(crate::play::routes())
        .merge(crate::study::routes())
        .merge(crate::match_assessment::routes())
        .merge(crate::recurring_mistakes::routes())
        .merge(crate::opening_book::routes())
        .merge(crate::imported_analysis::routes())
        .merge(crate::analysis_catalog::routes())
        .merge(crate::databases::routes())
        .merge(crate::duplicate_matches::routes())
        .merge(crate::player_directory::routes())
        .merge(crate::move_catalog::routes())
        .merge(crate::annotations::routes())
        .merge(crate::learning::routes())
        .merge(crate::analysis_statistics::routes())
        .merge(crate::position_browser::routes())
        .merge(crate::similar_positions::routes())
        .merge(crate::position_bookmarks::routes())
        .merge(crate::move_assessments::routes())
        .merge(crate::review_decisions::routes())
        .merge(crate::game_plan::routes())
        .merge(crate::cube_context::routes())
        .merge(crate::move_explanation::routes())
        .route("/api/health", get(health))
        .route("/api/version", get(version))
        .route("/api/imports", post(matches::upload))
        .route("/api/matches", get(matches::matches))
        .route("/api/matches/{id}", get(matches::get_match))
        .route("/api/matches/{id}/games", get(matches::match_games))
        .route(
            "/api/matches/{id}/import-warnings",
            get(matches::match_import_warnings),
        )
        .route("/api/matches/{id}/dice", get(matches::dice))
        .route("/api/matches/{id}/source", get(matches::source))
        .route(
            "/api/matches/{id}/analysis",
            get(analysis::latest_match_analysis).post(analysis::create_match_analysis),
        )
        .route(
            "/api/matches/{id}/analysis/jobs",
            get(analysis::match_analysis_jobs),
        )
        .route("/api/positions", get(matches::positions))
        .route("/api/positions/{id}", get(matches::get_position))
        .route("/api/jobs", post(jobs::create_job))
        .route("/api/jobs/status", post(jobs::job_status_batch))
        .route("/api/jobs/{id}", get(jobs::job))
        .route("/api/jobs/{id}/cancel", post(jobs::cancel_job))
        .merge(crate::analysis_timing::routes())
        .route("/api/jobs/{id}/events", get(jobs::events))
        .route("/api/jobs/{id}/moves", get(analysis::move_analyses))
        .route("/api/openapi.json", get(openapi::openapi))
        .route(
            "/api/database-backup/target-counts",
            get(crate::database_backup::target_counts),
        )
        // Not only the fallback: the web client's fallback replaces it.
        .route("/api", any(not_found))
        .route("/api/", any(not_found))
        .route("/api/{*path}", any(not_found))
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            database_access,
        ))
        .merge(crate::database_backup::routes().method_not_allowed_fallback(method_not_allowed))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .layer(
            CorsLayer::new()
                .allow_origin([
                    "http://localhost:5173".parse().unwrap(),
                    "http://127.0.0.1:5173".parse().unwrap(),
                    "tauri://localhost".parse().unwrap(),
                ])
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::PUT,
                    axum::http::Method::PATCH,
                    axum::http::Method::DELETE,
                ])
                .allow_headers([
                    header::CONTENT_TYPE,
                    header::HeaderName::from_static("last-event-id"),
                ]),
        );
    if let Some(dir) = web_dir {
        app = app.fallback_service(
            tower_http::services::ServeDir::new(dir)
                .not_found_service(tower_http::services::ServeFile::new(dir.join("index.html"))),
        );
    }
    // Trace and guard every route, including the frontend and database backups.
    app.layer(middleware::from_fn(crate::local_origin::guard))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn not_found() -> ApiError {
    ApiError::missing()
}

async fn method_not_allowed() -> ApiError {
    ApiError(StatusCode::METHOD_NOT_ALLOWED, "Method not allowed".into())
}

async fn database_access(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> ApiResult<Response> {
    let _access = state.backup.requests.try_read().map_err(|_| {
        ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "Database restoration is in progress".into(),
        )
    })?;
    Ok(next.run(request).await)
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VersionInfo {
    /// NeonDojo release version, for example `1.1.0`.
    pub release: &'static str,
    /// Full build commit ID, or an empty string when it is unavailable.
    pub commit: &'static str,
    /// The build commit, as in `commit`; kept for earlier clients.
    #[schema(deprecated)]
    pub version: &'static str,
}

pub fn version_info() -> VersionInfo {
    let commit = env!("NEONDOJO_GIT_COMMIT");
    VersionInfo {
        release: env!("CARGO_PKG_VERSION"),
        commit,
        version: commit,
    }
}

#[utoipa::path(get, path="/api/version", responses((status=200, body=VersionInfo)), tag="System")]
async fn version() -> Json<VersionInfo> {
    Json(version_info())
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Health {
    pub status: String,
    pub database: bool,
    pub gnubg: Option<String>,
}

pub async fn check_health(state: &AppState) -> (bool, Health) {
    let database = sqlx::query("SELECT 1").execute(&state.pool).await.is_ok();
    let gnubg = match crate::settings::load(state).await {
        Ok(settings) => {
            match crate::benchmarks::running_engine_version(&state.pool, &settings.gnubg_path).await
            {
                Ok(Some(version)) => Some(version),
                Ok(None) => settings.gnubg().health_version().await.ok(),
                Err(_) => None,
            }
        }
        Err(_) => None,
    };
    let ready = database && gnubg.is_some();
    (
        ready,
        Health {
            status: if ready { "ok" } else { "degraded" }.into(),
            database,
            gnubg,
        },
    )
}

#[utoipa::path(get, path="/api/health", responses((status=200, body=Health), (status=503, body=Health)), tag="System")]
async fn health(State(state): State<AppState>) -> (StatusCode, Json<Health>) {
    let (ready, body) = check_health(&state).await;
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(body),
    )
}
