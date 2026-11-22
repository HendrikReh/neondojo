use crate::{
    engine::{Analysis, Candidate, DiceEvaluation, Distribution, Engine, valid_id},
    import,
    model::*,
    store,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{collections::BTreeSet, convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::{Notify, Semaphore};
use tokio_util::sync::CancellationToken;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub engine: Engine,
    pub wake: Arc<Notify>,
    pub(crate) assessment_wake: Arc<Notify>,
    job_notifications: Arc<crate::job_events::JobNotifications>,
    pub shutdown: CancellationToken,
    imports: Arc<Semaphore>,
    pub(crate) backup: Arc<crate::database_backup::Access>,
    pub(crate) anki_connection: Arc<crate::learning::AnkiConnection>,
    /// How often a running analysis job renews its lease.
    pub(crate) job_heartbeat: Duration,
    pub(crate) running_jobs: Arc<crate::jobs::RunningJobs>,
}

impl AppState {
    pub fn new(pool: PgPool, engine: Engine) -> Self {
        Self {
            pool,
            engine,
            wake: Arc::new(Notify::new()),
            assessment_wake: Arc::new(Notify::new()),
            job_notifications: Arc::default(),
            shutdown: CancellationToken::new(),
            imports: Arc::new(Semaphore::new(2)),
            backup: Arc::default(),
            anki_connection: Arc::default(),
            job_heartbeat: crate::jobs::DEFAULT_HEARTBEAT,
            running_jobs: Arc::default(),
        }
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
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: String,
}
pub struct ApiError(pub(crate) StatusCode, pub(crate) String);
impl ApiError {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self(StatusCode::UNPROCESSABLE_ENTITY, message.into())
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
        (self.0, Json(ErrorBody { error: self.1 })).into_response()
    }
}
pub(crate) type ApiResult<T> = Result<T, ApiError>;

pub fn router(state: AppState) -> Router {
    Router::new()
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
        .route("/api/imports", post(upload))
        .route("/api/matches", get(matches))
        .route("/api/matches/{id}", get(get_match))
        .route("/api/matches/{id}/games", get(match_games))
        .route(
            "/api/matches/{id}/import-warnings",
            get(match_import_warnings),
        )
        .route("/api/matches/{id}/dice", get(dice))
        .route("/api/matches/{id}/source", get(source))
        .route(
            "/api/matches/{id}/analysis",
            get(latest_match_analysis).post(create_match_analysis),
        )
        .route("/api/matches/{id}/analysis/jobs", get(match_analysis_jobs))
        .route("/api/positions", get(positions))
        .route("/api/positions/{id}", get(get_position))
        .route("/api/jobs", post(create_job))
        .route("/api/jobs/status", post(job_status_batch))
        .route("/api/jobs/{id}", get(job))
        .route("/api/jobs/{id}/cancel", post(cancel_job))
        .merge(crate::analysis_timing::routes())
        .route("/api/jobs/{id}/events", get(events))
        .route("/api/jobs/{id}/moves", get(move_analyses))
        .route("/api/openapi.json", get(openapi))
        .route(
            "/api/database-backup/target-counts",
            get(crate::database_backup::target_counts),
        )
        .fallback(|| async { ApiError::missing() })
        .layer(middleware::from_fn_with_state(
            state.clone(),
            database_access,
        ))
        .merge(crate::database_backup::routes())
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
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
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

#[derive(Serialize, ToSchema)]
struct VersionInfo {
    /// NeonDojo release version, for example `1.1.0`.
    release: &'static str,
    /// Full build commit ID, or an empty string when it is unavailable.
    commit: &'static str,
    /// The build commit, as in `commit`; kept for earlier clients.
    #[schema(deprecated)]
    version: &'static str,
}

#[utoipa::path(get, path="/api/version", responses((status=200, body=VersionInfo)), tag="System")]
async fn version() -> Json<VersionInfo> {
    let commit = env!("NEONDOJO_GIT_COMMIT");
    Json(VersionInfo {
        release: env!("CARGO_PKG_VERSION"),
        commit,
        version: commit,
    })
}

#[derive(Serialize, ToSchema)]
pub struct Health {
    pub status: String,
    pub database: bool,
    pub gnubg: Option<String>,
}

#[utoipa::path(get, path="/api/health", responses((status=200, body=Health), (status=503, body=Health)), tag="System")]
async fn health(State(state): State<AppState>) -> (StatusCode, Json<Health>) {
    let database = sqlx::query("SELECT 1").execute(&state.pool).await.is_ok();
    let gnubg = match crate::settings::load(&state).await {
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
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(Health {
            status: if ready { "ok" } else { "degraded" }.into(),
            database,
            gnubg,
        }),
    )
}

#[derive(Deserialize, IntoParams)]
pub struct ImportQuery {
    /// Target logical database. Omitted: default database.
    pub database_id: Option<Uuid>,
}

#[derive(ToSchema)]
#[allow(dead_code)]
struct UploadForm {
    /// One MAT (also as a .txt match transcript), SGF, binary XG, BGBlitz BGF, or HedgeHog OGXM file, at most 16 MiB including multipart envelope.
    #[schema(value_type=String, format=Binary)]
    file: String,
}

#[utoipa::path(post, path="/api/imports", params(ImportQuery), request_body(content=UploadForm, content_type="multipart/form-data"), responses((status=201, body=ImportResponse), (status=200, body=ImportResponse, description="Previously imported file"), (status=409, body=ErrorBody, description="Target is reserved for paused sessions"), (status=422, body=ErrorBody), (status=413, description="Upload too large"), (status=429, body=ErrorBody)), tag="Library")]
async fn upload(
    State(state): State<AppState>,
    crate::json_errors::Query(query): crate::json_errors::Query<ImportQuery>,
    mut multipart: Multipart,
) -> ApiResult<(StatusCode, Json<ImportResponse>)> {
    let _permit = state.imports.try_acquire().map_err(|_| {
        ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Two imports are already in progress".into(),
        )
    })?;
    let field = multipart
        .next_field()
        .await
        .map_err(|e| ApiError(e.status(), e.body_text()))?
        .ok_or_else(|| ApiError::invalid("Missing file field"))?;
    if field.name() != Some("file") {
        return Err(ApiError::invalid("Expected multipart field 'file'"));
    }
    let filename = field
        .file_name()
        .ok_or_else(|| ApiError::invalid("Missing filename"))?
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_owned();
    if filename.len() > 255 || filename.is_empty() || filename.chars().any(char::is_control) {
        return Err(ApiError::invalid("Invalid filename"));
    }
    let extension = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    // A .txt upload is a plain-text match transcript in the Jellyfish MAT
    // layout, as published by tournament archives. It is stored as MAT.
    let format = match extension.as_str() {
        "txt" => "mat".to_owned(),
        _ => extension.clone(),
    };
    if !["mat", "sgf", "xg", "bgf", "ogxm"].contains(&format.as_str()) {
        return Err(ApiError::invalid(
            "Supported formats: .mat, .txt, .sgf, .xg, .bgf, .ogxm",
        ));
    }
    let bytes = field
        .bytes()
        .await
        .map_err(|e| ApiError(e.status(), e.body_text()))?;
    if bytes.is_empty() {
        return Err(ApiError::invalid("Empty file"));
    }
    if multipart
        .next_field()
        .await
        .map_err(|e| ApiError(e.status(), e.body_text()))?
        .is_some()
    {
        return Err(ApiError::invalid("Upload one file per request"));
    }
    let mut warnings = Vec::new();
    let sgf = match format.as_str() {
        "mat" => {
            let converted = Engine::new(crate::settings::load(&state).await?.gnubg_path)
                .import_mat(&bytes)
                .await
                .map_err(|e| {
                    ApiError::invalid(if extension == "txt" {
                        format!("TXT import failed; expected a MAT match transcript: {e:#}")
                    } else {
                        format!("MAT import failed: {e:#}")
                    })
                })?;
            warnings = converted.warnings;
            converted.sgf
        }
        "sgf" => {
            String::from_utf8(bytes.to_vec()).map_err(|_| ApiError::invalid("SGF must be UTF-8"))?
        }
        "bgf" => {
            let data = bytes.clone();
            tokio::task::spawn_blocking(move || import::bgf_to_sgf(&data))
                .await
                .map_err(anyhow::Error::from)?
                .map_err(|e| ApiError::invalid(format!("BGF import failed: {e:#}")))?
        }
        "ogxm" => {
            let data = bytes.clone();
            tokio::task::spawn_blocking(move || import::ogxm_to_sgf(&data))
                .await
                .map_err(anyhow::Error::from)?
                .map_err(|e| ApiError::invalid(format!("OGXM import failed: {e:#}")))?
        }
        "xg" => {
            let data = bytes.clone();
            let converted = tokio::task::spawn_blocking(move || import::convert_xg(&data))
                .await
                .map_err(anyhow::Error::from)?
                .map_err(|e| ApiError::invalid(format!("XG import failed: {e:#}")))?;
            warnings = converted.warnings;
            converted.sgf
        }
        _ => unreachable!(),
    };
    let parse_source = sgf.clone();
    let imported = tokio::task::spawn_blocking(move || import::parse_sgf(&parse_source))
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|e| ApiError::invalid(format!("Invalid game: {e:#}")))?;
    if imported.positions.is_empty() || imported.positions.len() > 100_000 {
        return Err(ApiError::invalid("Expected 1–100000 checker moves"));
    }
    let database_id = match query.database_id {
        Some(id) => id,
        None => crate::databases::default_database_id(&state.pool)
            .await
            .map_err(anyhow::Error::from)?,
    };
    crate::databases::require_library_destination(&state.pool, database_id).await?;
    let mut result = store::import_source_in_database(
        &state.pool,
        &filename,
        &format,
        &bytes,
        &sgf,
        &imported,
        warnings.clone(),
        database_id,
    )
    .await?;
    // Conversion warnings describe the source file, so a duplicate upload of
    // the same file reports them too.
    result.warnings = warnings;
    Ok((
        if result.duplicate {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        Json(result),
    ))
}

#[derive(Deserialize, IntoParams)]
struct MatchQuery {
    q: Option<String>,
    after: Option<Uuid>,
    limit: Option<i64>,
}
fn limit(value: Option<i64>) -> ApiResult<i64> {
    let limit = value.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::invalid("limit must be between 1 and 100"));
    }
    Ok(limit)
}

#[utoipa::path(get, path="/api/matches", params(MatchQuery), responses((status=200, body=MatchPage), (status=422, body=ErrorBody)), tag="Library")]
async fn matches(
    State(state): State<AppState>,
    crate::json_errors::Query(query): crate::json_errors::Query<MatchQuery>,
) -> ApiResult<Json<MatchPage>> {
    let q = query.q.as_deref().unwrap_or("").trim();
    if !q.is_empty() && !(3..=200).contains(&q.chars().count()) {
        return Err(ApiError::invalid("Search needs 3–200 characters"));
    }
    Ok(Json(
        store::matches(&state.pool, q, query.after, limit(query.limit)?).await?,
    ))
}

#[utoipa::path(get, path="/api/matches/{id}", params(("id"=Uuid, Path)), responses((status=200, body=MatchSummary), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
async fn get_match(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<MatchSummary>> {
    crate::play::require_review_available(&state.pool, id).await?;
    Ok(Json(
        store::get_match(&state.pool, id)
            .await?
            .ok_or_else(ApiError::missing)?,
    ))
}

#[utoipa::path(get, path="/api/matches/{id}/games", params(("id"=Uuid, Path)), responses((status=200, body=Vec<MatchGame>), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
async fn match_games(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<MatchGame>>> {
    crate::play::require_review_available(&state.pool, id).await?;
    Ok(Json(
        store::match_games(&state.pool, id)
            .await?
            .ok_or_else(ApiError::missing)?,
    ))
}

/// Irregularities in the source file that the import resolved, in game order.
#[utoipa::path(get, path="/api/matches/{id}/import-warnings", params(("id"=Uuid, Path)), responses((status=200, body=Vec<ImportWarning>), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
async fn match_import_warnings(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<ImportWarning>>> {
    crate::play::require_review_available(&state.pool, id).await?;
    Ok(Json(
        store::import_warnings(&state.pool, id)
            .await?
            .ok_or_else(ApiError::missing)?,
    ))
}

#[utoipa::path(get, path="/api/positions/{id}", params(("id"=Uuid, Path)), responses((status=200, body=Position), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
async fn get_position(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Position>> {
    crate::play::require_position_review_available(&state.pool, id).await?;
    Ok(Json(
        store::get_position(&state.pool, id)
            .await?
            .ok_or_else(ApiError::missing)?,
    ))
}

#[derive(Deserialize, IntoParams)]
struct GameScope {
    /// One-based source game number; omitted means the whole match.
    game_no: Option<i32>,
}

async fn validate_game(
    state: &AppState,
    match_id: Uuid,
    game_no: Option<i32>,
) -> ApiResult<Vec<MatchGame>> {
    // Existence and scope come from the catalog; results are not needed here.
    let games = store::game_catalog(&state.pool, match_id)
        .await?
        .ok_or_else(ApiError::missing)?;
    if game_no.is_some_and(|n| !games.iter().any(|g| g.game_no == n)) {
        return Err(ApiError::invalid("game_no is outside this match"));
    }
    Ok(games)
}

#[utoipa::path(get, path="/api/matches/{id}/source", params(("id"=Uuid, Path)), responses((status=200, body=String, content_type="application/x-go-sgf"), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
async fn source(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Response> {
    crate::play::require_review_available(&state.pool, id).await?;
    let sgf: Option<String> =
        sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1 AND NOT collection_only")
            .bind(id)
            .fetch_optional(&state.pool)
            .await
            .map_err(anyhow::Error::from)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-go-sgf"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=match.sgf",
            ),
        ],
        sgf.ok_or_else(ApiError::missing)?,
    )
        .into_response())
}

#[derive(Deserialize, IntoParams)]
struct PositionQuery {
    database_id: Option<Uuid>,
    match_id: Option<Uuid>,
    game_no: Option<i32>,
    position_id: Option<String>,
    after: Option<Uuid>,
    limit: Option<i64>,
}

#[utoipa::path(get, path="/api/positions", params(PositionQuery), responses((status=200, body=PositionPage), (status=422, body=ErrorBody)), tag="Library")]
async fn positions(
    State(state): State<AppState>,
    crate::json_errors::Query(query): crate::json_errors::Query<PositionQuery>,
) -> ApiResult<Json<PositionPage>> {
    if let Some(id) = query.match_id {
        crate::play::require_review_available(&state.pool, id).await?;
    }
    if query.game_no.is_some() {
        let match_id = query
            .match_id
            .ok_or_else(|| ApiError::invalid("game_no requires match_id"))?;
        validate_game(&state, match_id, query.game_no).await?;
    }
    if let Some(ref id) = query.position_id
        && !valid_id(id, 14)
    {
        return Err(ApiError::invalid(
            "Expected a 14-character GNUbg Position ID",
        ));
    }
    if query.position_id.is_none() && query.match_id.is_none() {
        return Err(ApiError::invalid("Supply match_id or position_id"));
    }
    if let Some(cursor) = query.after {
        let p = store::get_position(&state.pool, cursor)
            .await?
            .ok_or_else(|| ApiError::invalid("Unknown cursor"))?;
        if let Some(database_id) = query.database_id {
            let belongs: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM matches WHERE id=$1 AND database_id=$2)",
            )
            .bind(p.match_uuid)
            .bind(database_id)
            .fetch_one(&state.pool)
            .await
            .map_err(anyhow::Error::from)?;
            if !belongs {
                return Err(ApiError::invalid(
                    "Cursor does not match the database filter",
                ));
            }
        }
        if query.match_id.is_some_and(|id| id != p.match_uuid)
            || query.game_no.is_some_and(|n| n != p.game_no)
            || query
                .position_id
                .as_ref()
                .is_some_and(|id| *id != p.position_id)
        {
            return Err(ApiError::invalid("Cursor does not match the filters"));
        }
    }
    Ok(Json(
        store::positions_scoped_in_database(
            &state.pool,
            query.match_id,
            query.position_id.as_deref(),
            query.after,
            limit(query.limit)?,
            query.game_no,
            query.database_id,
        )
        .await?,
    ))
}

#[utoipa::path(get, path="/api/matches/{id}/dice", params(("id"=Uuid, Path), GameScope), responses((status=200, body=DiceStats), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
async fn dice(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    crate::json_errors::Query(query): crate::json_errors::Query<GameScope>,
) -> ApiResult<Json<DiceStats>> {
    crate::play::require_review_available(&state.pool, id).await?;
    validate_game(&state, id, query.game_no).await?;
    Ok(Json(
        store::dice_stats_scoped(&state.pool, id, query.game_no).await?,
    ))
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum PositionJobKind {
    Analysis,
    Dice,
}

#[derive(Deserialize, ToSchema)]
struct JobRequest {
    /// Database UUID of the imported position, not its GNUbg ID.
    position_id: Uuid,
    /// A ranked candidate analysis or a distribution over 21 weighted rolls.
    kind: PositionJobKind,
    /// Provider for this job; omitted uses the saved analysis engine.
    engine_id: Option<MatchAnalysisEngine>,
    /// Verified search depth of the selected engine. Omitted uses saved analysis depth.
    #[serde(default)]
    #[schema(minimum = 1, maximum = 5)]
    plies: Option<i32>,
}

#[utoipa::path(post, path="/api/jobs", request_body=JobRequest, responses((status=202, body=Job), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Analysis")]
async fn create_job(
    State(state): State<AppState>,
    payload: Result<Json<JobRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    let Json(request) = payload.map_err(|e| ApiError::invalid(e.body_text()))?;
    crate::play::require_position_review_available(&state.pool, request.position_id).await?;
    let engine_id = request.engine_id.as_ref().map(MatchAnalysisEngine::id);
    let (settings, snapshot) =
        crate::settings::snapshot_for_analysis(&state, request.plies, engine_id)
            .await
            .map_err(|e| ApiError::invalid(format!("Engine unavailable: {e:#}")))?;
    let plies = request.plies.unwrap_or(settings.analysis_plies);
    if !crate::configuration::engine_depths(&snapshot.engine_id, "analysis").contains(&plies) {
        return Err(ApiError::invalid(
            "Depth is not verified for the selected engine",
        ));
    }
    store::get_position(&state.pool, request.position_id)
        .await?
        .ok_or_else(ApiError::missing)?;
    let job = store::create_job_with_snapshot(
        &state.pool,
        request.position_id,
        match request.kind {
            PositionJobKind::Analysis => "analysis",
            PositionJobKind::Dice => "dice",
        },
        plies,
        Some(&snapshot),
    )
    .await?;
    state.wake.notify_one();
    Ok((StatusCode::ACCEPTED, Json(job)))
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MatchAnalysisEngine {
    Gnubg,
    Bgsage,
}

impl MatchAnalysisEngine {
    pub(crate) fn id(&self) -> &'static str {
        crate::engines::EngineId::from(self).id()
    }
}

impl From<&MatchAnalysisEngine> for crate::engines::EngineId {
    fn from(engine: &MatchAnalysisEngine) -> Self {
        match engine {
            MatchAnalysisEngine::Gnubg => Self::Gnubg,
            MatchAnalysisEngine::Bgsage => Self::NeonGammon,
        }
    }
}

#[derive(Deserialize, ToSchema)]
struct MatchAnalysisRequest {
    /// Analyze one source game in its original match context; omitted means the whole match.
    game_no: Option<i32>,
    /// Provider for this job; omitted uses the saved analysis engine.
    engine_id: Option<MatchAnalysisEngine>,
    /// Verified checker and cube depth of the selected engine. Omitted uses saved analysis depth.
    #[serde(default)]
    #[schema(minimum = 1, maximum = 5)]
    plies: Option<i32>,
}

#[utoipa::path(post, path="/api/matches/{id}/analysis", params(("id"=Uuid, Path)), request_body=MatchAnalysisRequest, responses((status=202, body=Job),(status=404,body=ErrorBody),(status=422,body=ErrorBody)), tag="Match analysis")]
async fn create_match_analysis(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    payload: Result<Json<MatchAnalysisRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    crate::play::require_review_available(&state.pool, id).await?;
    let Json(request) = payload.map_err(|e| ApiError::invalid(e.body_text()))?;
    let source = store::get_match(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    if !source.source_available {
        return Err(ApiError::invalid(
            "The source match was removed; analyze the saved position instead",
        ));
    }
    let engine_id = request.engine_id.as_ref().map(MatchAnalysisEngine::id);
    let (settings, snapshot) =
        crate::settings::snapshot_for_analysis(&state, request.plies, engine_id)
            .await
            .map_err(|e| ApiError::invalid(format!("Engine unavailable: {e:#}")))?;
    let plies = request.plies.unwrap_or(settings.analysis_plies);
    if !crate::configuration::engine_depths(&snapshot.engine_id, "analysis").contains(&plies) {
        return Err(ApiError::invalid(
            "Depth is not verified for the selected engine",
        ));
    }
    let games = validate_game(&state, id, request.game_no).await?;
    let total = if request.game_no.is_some() {
        1
    } else {
        games.len() as i32
    };
    let job = store::create_match_job_scoped_with_snapshot(
        &state.pool,
        id,
        plies,
        total,
        request.game_no,
        Some(&snapshot),
    )
    .await?;
    state.wake.notify_one();
    Ok((StatusCode::ACCEPTED, Json(job)))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct MatchAnalysisQuery {
    /// One-based source game number; omitted means the whole match.
    game_no: Option<i32>,
    /// Provider whose status to show; omitted uses the saved analysis engine.
    engine_id: Option<MatchAnalysisEngine>,
}

#[utoipa::path(get, path="/api/matches/{id}/analysis", params(("id"=Uuid, Path), MatchAnalysisQuery), responses((status=200, body=Option<Job>,description="Latest analysis job for the selected engine and scope, or null if none has been submitted. Includes persisted partial summary during execution."),(status=404,body=ErrorBody),(status=422,body=ErrorBody)), tag="Match analysis")]
async fn latest_match_analysis(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    query: Result<Query<MatchAnalysisQuery>, axum::extract::rejection::QueryRejection>,
) -> ApiResult<Json<Option<Job>>> {
    crate::play::require_review_available(&state.pool, id).await?;
    let Query(query) = query.map_err(|error| ApiError::invalid(error.body_text()))?;
    let game_no = query.game_no;
    validate_game(&state, id, game_no).await?;
    Ok(Json(
        store::latest_match_job_scoped(
            &state.pool,
            id,
            game_no,
            query.engine_id.as_ref().map(MatchAnalysisEngine::id),
        )
        .await?,
    ))
}

#[utoipa::path(get, path="/api/matches/{id}/analysis/jobs", params(("id"=Uuid, Path), ("game_no"=Option<i32>, Query, description="One-based source game number; omitted means the whole match.")), responses((status=200, body=Vec<AnalysisComparisonRun>,description="Current analysis jobs for the selected game or whole match, newest first. For a selected game, includes its checkpoint from completed whole-match analyses. Returns the latest job for each provider, depth, status and source scope."),(status=404,body=ErrorBody),(status=422,body=ErrorBody)), tag="Match analysis")]
async fn match_analysis_jobs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    query: Result<Query<GameScope>, axum::extract::rejection::QueryRejection>,
) -> ApiResult<Json<Vec<AnalysisComparisonRun>>> {
    crate::play::require_review_available(&state.pool, id).await?;
    let Query(query) = query.map_err(|error| ApiError::invalid(error.body_text()))?;
    if query.game_no.is_some() {
        validate_game(&state, id, query.game_no).await?;
    } else {
        store::get_match(&state.pool, id)
            .await?
            .ok_or_else(ApiError::missing)?;
    }
    let mut runs = store::match_analysis_jobs_for_scope(&state.pool, id, query.game_no).await?;
    if let Some(game_no) = query.game_no {
        for run in runs
            .iter_mut()
            .filter(|run| run.job.game_no.is_none() && run.game_summary.is_none())
        {
            let statistics =
                crate::analysis_statistics::load_statistics(&state, run.job.id).await?;
            run.game_summary = statistics
                .games
                .into_iter()
                .find(|game| {
                    game.game_no == game_no && game.analyzed && game.error_breakdown.is_some()
                })
                .and_then(|game| game.summary);
        }
    }
    Ok(Json(runs))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct MoveAnalysisQuery {
    /// Restrict decisions to one source game.
    game_no: Option<i32>,
    /// Continue after a move-analysis row ID, keeping the same filters.
    after: Option<i64>,
    /// Page size 1–100, defaults to 50.
    limit: Option<i64>,
    /// Exclude forced/unavailable decisions and apply the configured Questionable threshold.
    #[serde(default)]
    errors_only: bool,
    /// 0 = White, 1 = Black.
    player: Option<i32>,
    /// checker or cube; omitted means both.
    kind: Option<String>,
    /// Exact error severity; combined with all other filters.
    severity: Option<MoveSeverity>,
}

#[utoipa::path(get, path="/api/jobs/{id}/moves", params(("id"=Uuid, Path),MoveAnalysisQuery), responses((status=200,body=MoveAnalysisPage),(status=404,body=ErrorBody),(status=422,body=ErrorBody)), tag="Match analysis")]
async fn move_analyses(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    query: Result<Query<MoveAnalysisQuery>, axum::extract::rejection::QueryRejection>,
) -> ApiResult<Json<MoveAnalysisPage>> {
    let Query(query) = query.map_err(|e| ApiError::invalid(e.body_text()))?;
    let job = store::get_job(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    crate::play::require_job_review_available(&state.pool, &job).await?;
    if job.kind != "match" {
        return Err(ApiError::invalid("This job is not a match analysis"));
    }
    if query.after.is_some_and(|n| n < 0)
        || query.player.is_some_and(|n| !(0..=1).contains(&n))
        || query
            .kind
            .as_deref()
            .is_some_and(|k| k != "checker" && k != "cube")
    {
        return Err(ApiError::invalid("Invalid analysis cursor, player or kind"));
    }
    if let Some(game_no) = query.game_no {
        if job.game_no.is_some_and(|n| n != game_no) {
            return Err(ApiError::invalid("game_no does not match this job's scope"));
        }
        validate_game(
            &state,
            job.match_uuid.ok_or_else(ApiError::missing)?,
            Some(game_no),
        )
        .await?;
    }
    let thresholds = crate::configuration::store::error_thresholds(&state.pool).await?;
    if let Some(after) = query.after.filter(|n| *n > 0) {
        let row: Option<(Uuid, i32, i32, String, f64, String)> = sqlx::query_as(
            "SELECT job_id,game_no,player,kind,equity_loss,COALESCE(data->>'evaluation_status','') FROM move_analyses WHERE id=$1",
        )
        .bind(after)
        .fetch_optional(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;
        let Some((job_id, game_no, player, kind, loss, evaluation_status)) = row else {
            return Err(ApiError::invalid("Unknown cursor"));
        };
        let severity = if matches!(evaluation_status.as_str(), "forced" | "unavailable") {
            MoveSeverity::None
        } else {
            thresholds.classify(loss)
        };
        if job_id != id
            || query.game_no.is_some_and(|n| n != game_no)
            || query.player.is_some_and(|n| n != player)
            || query.kind.as_ref().is_some_and(|k| *k != kind)
            || (query.errors_only && severity == MoveSeverity::None)
            || query.severity.is_some_and(|grade| grade != severity)
        {
            return Err(ApiError::invalid("Cursor does not match the filters"));
        }
    }
    Ok(Json(
        store::move_analyses_with_thresholds(
            &state.pool,
            id,
            query.after.unwrap_or(0),
            limit(query.limit)?,
            query.errors_only,
            query.player,
            query.kind.as_deref(),
            query.game_no,
            query.severity.map(MoveSeverity::as_str),
            thresholds,
        )
        .await?,
    ))
}

#[utoipa::path(get, path="/api/jobs/{id}", params(("id"=Uuid, Path)), responses((status=200, body=Job), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Analysis")]
async fn job(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Json<Job>> {
    let job = store::get_job(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    crate::play::require_job_review_available(&state.pool, &job).await?;
    Ok(Json(job))
}

/// Cancel a queued or running analysis, dice or match job. The job ends as
/// `failed` with the error "Cancelled by user"; a running engine process is
/// stopped, and games a match analysis completed before stay saved.
#[utoipa::path(post, path="/api/jobs/{id}/cancel", params(("id"=Uuid, Path)), responses((status=200, body=Job, description="The cancelled job"), (status=404, body=ErrorBody), (status=409, body=ErrorBody, description="The job has already finished"), (status=422, body=ErrorBody)), tag="Analysis")]
async fn cancel_job(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Json<Job>> {
    let job = store::get_job(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    crate::play::require_job_review_available(&state.pool, &job).await?;
    match crate::jobs::cancel(&state, id).await? {
        Some((true, job)) => Ok(Json(job)),
        Some((false, _)) => Err(ApiError(
            StatusCode::CONFLICT,
            "The job has already finished".into(),
        )),
        None => Err(ApiError::missing()),
    }
}

#[derive(Deserialize, ToSchema)]
struct JobStatusRequest {
    #[schema(max_items = 100)]
    ids: Vec<Uuid>,
}

#[derive(Serialize, ToSchema)]
struct JobStatusBatch {
    jobs: Vec<Job>,
    missing: Vec<Uuid>,
    restricted: Vec<Uuid>,
}

#[utoipa::path(post, path="/api/jobs/status", request_body=JobStatusRequest,
    responses((status=200, body=JobStatusBatch, description="Each requested ID is represented once as an available job, a missing ID, or an ID restricted by competition play."), (status=422, body=ErrorBody)), tag="Analysis")]
async fn job_status_batch(
    State(state): State<AppState>,
    payload: Result<Json<JobStatusRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<JobStatusBatch>> {
    let Json(request) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    if request.ids.len() > 100 {
        return Err(ApiError::invalid("At most 100 job IDs can be read at once"));
    }
    let mut missing: BTreeSet<Uuid> = request.ids.iter().copied().collect();
    let mut jobs = Vec::new();
    let mut restricted = Vec::new();
    for row in store::jobs_with_review_status(&state.pool, &request.ids).await? {
        missing.remove(&row.job.id);
        if row.restricted {
            restricted.push(row.job.id);
        } else {
            jobs.push(row.job);
        }
    }
    Ok(Json(JobStatusBatch {
        jobs,
        missing: missing.into_iter().collect(),
        restricted,
    }))
}

#[utoipa::path(get, path="/api/jobs/{id}/events", params(("id"=Uuid, Path),("Last-Event-ID"=Option<i64>, Header, description="Resume after this persisted event ID")), responses((status=200, body=String, content_type="text/event-stream", description="SSE wire format: id: <persisted integer>\\nevent: <queued|running|progress|completed|failed>\\ndata: <JobProgress JSON>\\n\\n. Each data field conforms to components.schemas.JobProgress. Stream ends after terminal event or service shutdown; reconnect resumes via Last-Event-ID."),(status=404, body=ErrorBody),(status=422,body=ErrorBody)), tag="Analysis")]
async fn events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let job = store::get_job(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    crate::play::require_job_review_available(&state.pool, &job).await?;
    let mut after = headers
        .get("last-event-id")
        .map(|h| h.to_str().ok().and_then(|s| s.parse::<i64>().ok()))
        .unwrap_or(Some(0))
        .filter(|n| *n >= 0)
        .ok_or_else(|| ApiError::invalid("Invalid Last-Event-ID"))?;
    let mut notifications = state
        .job_notifications
        .subscribe(&state.pool, &state.shutdown);
    let stream = async_stream::stream! {
        loop {
            if state.shutdown.is_cancelled() { return; }
            // The replay sees commits covered by queued hints. Keep listening
            // for any commits made while the replay queries run.
            notifications = notifications.resubscribe();
            match store::events(&state.pool,id,after).await {
                Ok(events) => {
                    let count = events.len();
                    for event in events {
                        after = event.id;
                        let terminal = event.event=="completed" || event.event=="failed";
                        yield Ok(Event::default().id(event.id.to_string()).event(event.event).data(event.data.to_string()));
                        if terminal { return; }
                    }
                    if count==100 { continue; }
                    // Check the committed terminal event ID as well as status: a completion
                    // may have committed between the replay query and this status query.
                    if let Ok(Some(job))=store::get_job(&state.pool,id).await
                        && (job.status=="completed" || job.status=="failed") {
                            let last: Result<Option<i64>,_> = sqlx::query_scalar("SELECT max(id) FROM job_events WHERE job_id=$1").bind(id).fetch_one(&state.pool).await;
                            if let Ok(Some(last)) = last && after>=last { return; }
                        }
                },
                Err(error) => { tracing::warn!(%error,"SSE database read failed"); return; }
            }
            tokio::select! {
                _ = state.shutdown.cancelled() => return,
                _ = crate::job_events::wait(&mut notifications, id) => {},
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keepalive"),
    ))
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "NeonDojo API",
        description = "Local headless backgammon library with GNUbg and NeonGammon analysis. No authentication; bind to loopback."
    ),
    paths(
        health,
        version,
        upload,
        matches,
        get_match,
        match_games,
        match_import_warnings,
        source,
        positions,
        get_position,
        dice,
        create_job,
        job,
        cancel_job,
        job_status_batch,
        events,
        create_match_analysis,
        latest_match_analysis,
        match_analysis_jobs,
        move_analyses
    ),
    components(schemas(
        Health,
        VersionInfo,
        ErrorBody,
        UploadForm,
        ImportResponse,
        ImportWarning,
        ImportWarningKind,
        MatchSummary,
        MatchGame,
        MatchPage,
        Position,
        PositionPage,
        JobRequest,
        Job,
        JobStatusRequest,
        JobStatusBatch,
        JobEvent,
        JobProgress,
        DiceStats,
        DiceCount,
        Analysis,
        Candidate,
        Distribution,
        DiceEvaluation,
        MatchAnalysisRequest,
        MatchAnalysisSummary,
        PlayerAnalysisSummary,
        MoveAnalysis,
        MoveAnalysisPage,
        MoveSeverity,
        crate::match_analysis::Decision
    ))
)]
pub struct BaseApiDoc;

pub struct ApiDoc;
impl OpenApi for ApiDoc {
    fn openapi() -> utoipa::openapi::OpenApi {
        let mut doc = BaseApiDoc::openapi();
        doc.merge(crate::settings::SettingsDoc::openapi());
        doc.merge(crate::settings::open_sage::OpenSageDoc::openapi());
        doc.merge(crate::configuration::ConfigurationDoc::openapi());
        doc.merge(crate::profiles::ProfilesDoc::openapi());
        doc.merge(crate::benchmarks::BenchmarkDoc::openapi());
        doc.merge(crate::engine_matches::EngineMatchDoc::openapi());
        doc.merge(crate::benchmarks::estimate::EstimateDoc::openapi());
        doc.merge(crate::analysis_timing::TimingDoc::openapi());
        doc.merge(crate::play::PlayDoc::openapi());
        doc.merge(crate::study::StudyDoc::openapi());
        doc.merge(crate::match_assessment::AssessmentDoc::openapi());
        doc.merge(crate::recurring_mistakes::RecurringMistakesDoc::openapi());
        doc.merge(crate::opening_book::OpeningBookDoc::openapi());
        doc.merge(crate::imported_analysis::ImportedAnalysisDoc::openapi());
        doc.merge(crate::analysis_catalog::AnalysisCatalogDoc::openapi());
        doc.merge(crate::databases::DatabaseDoc::openapi());
        doc.merge(crate::duplicate_matches::DuplicateMatchesDoc::openapi());
        doc.merge(crate::database_backup::BackupDoc::openapi());
        doc.merge(crate::player_directory::openapi());
        doc.merge(crate::move_catalog::MoveCatalogDoc::openapi());
        doc.merge(crate::annotations::AnnotationDoc::openapi());
        doc.merge(crate::learning::LearningDoc::openapi());
        doc.merge(crate::learning::daily_doc());
        doc.merge(crate::analysis_statistics::StatisticsDoc::openapi());
        doc.merge(crate::analysis_statistics::catalog::CatalogStatisticsDoc::openapi());
        doc.merge(crate::position_browser::PositionBrowserDoc::openapi());
        doc.merge(crate::similar_positions::SimilarPositionsDoc::openapi());
        doc.merge(crate::similar_positions::SimilarPracticeHistoryDoc::openapi());
        doc.merge(crate::position_bookmarks::PositionBookmarksDoc::openapi());
        doc.merge(crate::move_assessments::MoveAssessmentsDoc::openapi());
        doc.merge(crate::review_decisions::ReviewDoc::openapi());
        doc.merge(crate::game_plan::GamePlanDoc::openapi());
        doc.merge(crate::cube_context::CubeContextDoc::openapi());
        doc.merge(crate::move_explanation::MoveExplanationDoc::openapi());
        doc
    }
}

async fn openapi() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}
