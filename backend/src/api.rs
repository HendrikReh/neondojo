use crate::{
    engine::{Analysis, Candidate, DiceEvaluation, Distribution, Engine, valid_id},
    import,
    model::*,
    store,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{convert::Infallible, sync::Arc, time::Duration};
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
    pub shutdown: CancellationToken,
    imports: Arc<Semaphore>,
}

impl AppState {
    pub fn new(pool: PgPool, engine: Engine) -> Self {
        Self {
            pool,
            engine,
            wake: Arc::new(Notify::new()),
            shutdown: CancellationToken::new(),
            imports: Arc::new(Semaphore::new(2)),
        }
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
        .merge(crate::play::routes())
        .merge(crate::study::routes())
        .merge(crate::recurring_mistakes::routes())
        .merge(crate::opening_book::routes())
        .merge(crate::imported_analysis::routes())
        .merge(crate::databases::routes())
        .merge(crate::move_catalog::routes())
        .merge(crate::annotations::routes())
        .merge(crate::learning::routes())
        .merge(crate::analysis_statistics::routes())
        .merge(crate::position_browser::routes())
        .merge(crate::similar_positions::routes())
        .merge(crate::position_bookmarks::routes())
        .merge(crate::move_assessments::routes())
        .merge(crate::game_plan::routes())
        .route("/api/health", get(health))
        .route("/api/imports", post(upload))
        .route("/api/matches", get(matches))
        .route("/api/matches/{id}", get(get_match))
        .route("/api/matches/{id}/games", get(match_games))
        .route("/api/matches/{id}/dice", get(dice))
        .route("/api/matches/{id}/source", get(source))
        .route(
            "/api/matches/{id}/analysis",
            get(latest_match_analysis).post(create_match_analysis),
        )
        .route("/api/positions", get(positions))
        .route("/api/positions/{id}", get(get_position))
        .route("/api/jobs", post(create_job))
        .route("/api/jobs/{id}", get(job))
        .merge(crate::analysis_timing::routes())
        .route("/api/jobs/{id}/events", get(events))
        .route("/api/jobs/{id}/moves", get(move_analyses))
        .route("/api/openapi.json", get(openapi))
        .fallback(|| async { ApiError::missing() })
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
                Ok(None) => Engine::new(settings.gnubg_path).version().await.ok(),
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
    /// One MAT, SGF, binary XG, or BGBlitz BGF file, at most 16 MiB including multipart envelope.
    #[schema(value_type=String, format=Binary)]
    file: String,
}

#[utoipa::path(post, path="/api/imports", params(ImportQuery), request_body(content=UploadForm, content_type="multipart/form-data"), responses((status=201, body=ImportResponse), (status=200, body=ImportResponse, description="Previously imported file"), (status=422, body=ErrorBody), (status=413, description="Upload too large"), (status=429, body=ErrorBody)), tag="Library")]
async fn upload(
    State(state): State<AppState>,
    Query(query): Query<ImportQuery>,
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
    let format = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if !["mat", "sgf", "xg", "bgf"].contains(&format.as_str()) {
        return Err(ApiError::invalid(
            "Supported formats: .mat, .sgf, .xg, .bgf",
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
    let sgf = match format.as_str() {
        "mat" => Engine::new(crate::settings::load(&state).await?.gnubg_path)
            .normalize_mat(&bytes)
            .await
            .map_err(|e| ApiError::invalid(format!("MAT import failed: {e:#}")))?,
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
        "xg" => {
            let data = bytes.clone();
            tokio::task::spawn_blocking(move || import::xg_to_sgf(&data))
                .await
                .map_err(anyhow::Error::from)?
                .map_err(|e| ApiError::invalid(format!("XG import failed: {e:#}")))?
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
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM databases WHERE id=$1 AND NOT collection_only)",
    )
    .bind(database_id)
    .fetch_one(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    if !exists {
        return Err(ApiError::missing());
    }
    let result = store::import_match_in_database(
        &state.pool,
        &filename,
        &format,
        &bytes,
        &sgf,
        &imported,
        database_id,
    )
    .await?;
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
    Query(query): Query<MatchQuery>,
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
    let games = store::match_games(&state.pool, match_id)
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
    Query(query): Query<PositionQuery>,
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
    Query(query): Query<GameScope>,
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
    /// analysis: five candidate moves; dice: 21 weighted rolls.
    kind: PositionJobKind,
    /// GNUbg search depth, 0 to 4. Omitted uses saved analysis depth.
    #[serde(default)]
    #[schema(minimum = 0, maximum = 4)]
    plies: Option<i32>,
}

#[utoipa::path(post, path="/api/jobs", request_body=JobRequest, responses((status=202, body=Job), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Analysis")]
async fn create_job(
    State(state): State<AppState>,
    payload: Result<Json<JobRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    let Json(request) = payload.map_err(|e| ApiError::invalid(e.body_text()))?;
    crate::play::require_position_review_available(&state.pool, request.position_id).await?;
    let (settings, snapshot) = crate::settings::snapshot(&state).await?;
    let plies = request.plies.unwrap_or(settings.analysis_plies);
    if !(0..=4).contains(&plies) {
        return Err(ApiError::invalid("plies must be 0..4"));
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
struct MatchAnalysisRequest {
    /// Analyze one source game in its original match context; omitted means the whole match.
    game_no: Option<i32>,
    /// Native GNUbg checker and cube search depth. Omitted uses saved analysis depth.
    #[serde(default)]
    #[schema(minimum = 0, maximum = 4)]
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
    let (settings, snapshot) = crate::settings::snapshot(&state).await?;
    let plies = request.plies.unwrap_or(settings.analysis_plies);
    if !(0..=4).contains(&plies) {
        return Err(ApiError::invalid("plies must be 0..4"));
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

#[utoipa::path(get, path="/api/matches/{id}/analysis", params(("id"=Uuid, Path), GameScope), responses((status=200, body=Option<Job>,description="Latest analysis job for this game or whole-match scope, or null if none has been submitted. Includes persisted partial summary during execution."),(status=404,body=ErrorBody),(status=422,body=ErrorBody)), tag="Match analysis")]
async fn latest_match_analysis(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<GameScope>,
) -> ApiResult<Json<Option<Job>>> {
    crate::play::require_review_available(&state.pool, id).await?;
    validate_game(&state, id, query.game_no).await?;
    Ok(Json(
        store::latest_match_job_scoped(&state.pool, id, query.game_no).await?,
    ))
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
    /// Exclude forced/unavailable decisions and require at least 0.04 normalized equity loss.
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
    if let Some(after) = query.after.filter(|n| *n > 0) {
        let row: Option<(Uuid, i32, i32, String, f64, String, String)> = sqlx::query_as(
            "SELECT job_id,game_no,player,kind,equity_loss,data->>'severity',COALESCE(data->>'evaluation_status','') FROM move_analyses WHERE id=$1",
        )
        .bind(after)
        .fetch_optional(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;
        let Some((job_id, game_no, player, kind, loss, severity, evaluation_status)) = row else {
            return Err(ApiError::invalid("Unknown cursor"));
        };
        if job_id != id
            || query.game_no.is_some_and(|n| n != game_no)
            || query.player.is_some_and(|n| n != player)
            || query.kind.as_ref().is_some_and(|k| *k != kind)
            || (query.errors_only
                && (loss < 0.04 || matches!(evaluation_status.as_str(), "forced" | "unavailable")))
            || query.severity.is_some_and(|s| s.as_str() != severity)
        {
            return Err(ApiError::invalid("Cursor does not match the filters"));
        }
    }
    Ok(Json(
        store::move_analyses_filtered(
            &state.pool,
            id,
            query.after.unwrap_or(0),
            limit(query.limit)?,
            query.errors_only,
            query.player,
            query.kind.as_deref(),
            query.game_no,
            query.severity.map(MoveSeverity::as_str),
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
    let stream = async_stream::stream! {
        loop {
            if state.shutdown.is_cancelled() { return; }
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
                _ = tokio::time::sleep(Duration::from_millis(350)) => {},
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
        title = "BG Tutor API",
        version = "0.1.0",
        description = "Local headless backgammon library and GNUbg analysis. No authentication; bind to loopback."
    ),
    paths(
        health,
        upload,
        matches,
        get_match,
        match_games,
        source,
        positions,
        get_position,
        dice,
        create_job,
        job,
        events,
        create_match_analysis,
        latest_match_analysis,
        move_analyses
    ),
    components(schemas(
        Health,
        ErrorBody,
        UploadForm,
        ImportResponse,
        MatchSummary,
        MatchGame,
        MatchPage,
        Position,
        PositionPage,
        JobRequest,
        Job,
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
        doc.merge(crate::configuration::ConfigurationDoc::openapi());
        doc.merge(crate::profiles::ProfilesDoc::openapi());
        doc.merge(crate::benchmarks::BenchmarkDoc::openapi());
        doc.merge(crate::benchmarks::estimate::EstimateDoc::openapi());
        doc.merge(crate::analysis_timing::TimingDoc::openapi());
        doc.merge(crate::play::PlayDoc::openapi());
        doc.merge(crate::study::StudyDoc::openapi());
        doc.merge(crate::recurring_mistakes::RecurringMistakesDoc::openapi());
        doc.merge(crate::opening_book::OpeningBookDoc::openapi());
        doc.merge(crate::imported_analysis::ImportedAnalysisDoc::openapi());
        doc.merge(crate::databases::DatabaseDoc::openapi());
        doc.merge(crate::move_catalog::MoveCatalogDoc::openapi());
        doc.merge(crate::annotations::AnnotationDoc::openapi());
        doc.merge(crate::learning::LearningDoc::openapi());
        doc.merge(crate::analysis_statistics::StatisticsDoc::openapi());
        doc.merge(crate::position_browser::PositionBrowserDoc::openapi());
        doc.merge(crate::similar_positions::SimilarPositionsDoc::openapi());
        doc.merge(crate::similar_positions::SimilarPracticeHistoryDoc::openapi());
        doc.merge(crate::position_bookmarks::PositionBookmarksDoc::openapi());
        doc.merge(crate::move_assessments::MoveAssessmentsDoc::openapi());
        doc.merge(crate::game_plan::GamePlanDoc::openapi());
        doc
    }
}

async fn openapi() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}
