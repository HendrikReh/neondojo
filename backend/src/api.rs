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
pub struct ApiError(StatusCode, String);
impl ApiError {
    fn invalid(message: impl Into<String>) -> Self {
        Self(StatusCode::UNPROCESSABLE_ENTITY, message.into())
    }
    fn missing() -> Self {
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
type ApiResult<T> = Result<T, ApiError>;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/imports", post(upload))
        .route("/api/matches", get(matches))
        .route("/api/matches/{id}", get(get_match))
        .route("/api/matches/{id}/dice", get(dice))
        .route("/api/matches/{id}/source", get(source))
        .route("/api/positions", get(positions))
        .route("/api/jobs", post(create_job))
        .route("/api/jobs/{id}", get(job))
        .route("/api/jobs/{id}/events", get(events))
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
                .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
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
    let gnubg = state.engine.version().await.ok();
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

#[derive(ToSchema)]
#[allow(dead_code)]
struct UploadForm {
    /// One MAT, SGF, or binary XG file, at most 16 MiB including multipart envelope.
    #[schema(value_type=String, format=Binary)]
    file: String,
}

#[utoipa::path(post, path="/api/imports", request_body(content=UploadForm, content_type="multipart/form-data"), responses((status=201, body=ImportResponse), (status=200, body=ImportResponse, description="Previously imported file"), (status=422, body=ErrorBody), (status=413, description="Upload too large"), (status=429, body=ErrorBody)), tag="Library")]
async fn upload(
    State(state): State<AppState>,
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
    if !["mat", "sgf", "xg"].contains(&format.as_str()) {
        return Err(ApiError::invalid("Supported formats: .mat, .sgf, .xg"));
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
        "mat" => state
            .engine
            .normalize_mat(&bytes)
            .await
            .map_err(|e| ApiError::invalid(format!("MAT import failed: {e:#}")))?,
        "sgf" => {
            String::from_utf8(bytes.to_vec()).map_err(|_| ApiError::invalid("SGF must be UTF-8"))?
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
    let result =
        store::import_match(&state.pool, &filename, &format, &bytes, &sgf, &imported).await?;
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

#[utoipa::path(get, path="/api/matches/{id}", params(("id"=Uuid, Path)), responses((status=200, body=MatchSummary), (status=404, body=ErrorBody)), tag="Library")]
async fn get_match(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<MatchSummary>> {
    Ok(Json(
        store::get_match(&state.pool, id)
            .await?
            .ok_or_else(ApiError::missing)?,
    ))
}

#[utoipa::path(get, path="/api/matches/{id}/source", params(("id"=Uuid, Path)), responses((status=200, body=String, content_type="application/x-go-sgf"), (status=404, body=ErrorBody)), tag="Library")]
async fn source(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Response> {
    let sgf: Option<String> = sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1")
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
    match_id: Option<Uuid>,
    position_id: Option<String>,
    after: Option<Uuid>,
    limit: Option<i64>,
}

#[utoipa::path(get, path="/api/positions", params(PositionQuery), responses((status=200, body=PositionPage), (status=422, body=ErrorBody)), tag="Library")]
async fn positions(
    State(state): State<AppState>,
    Query(query): Query<PositionQuery>,
) -> ApiResult<Json<PositionPage>> {
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
        if query.match_id.is_some_and(|id| id != p.match_uuid)
            || query
                .position_id
                .as_ref()
                .is_some_and(|id| *id != p.position_id)
        {
            return Err(ApiError::invalid("Cursor does not match the filters"));
        }
    }
    Ok(Json(
        store::positions(
            &state.pool,
            query.match_id,
            query.position_id.as_deref(),
            query.after,
            limit(query.limit)?,
        )
        .await?,
    ))
}

#[utoipa::path(get, path="/api/matches/{id}/dice", params(("id"=Uuid, Path)), responses((status=200, body=DiceStats), (status=404, body=ErrorBody)), tag="Library")]
async fn dice(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Json<DiceStats>> {
    store::get_match(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    Ok(Json(store::dice_stats(&state.pool, id).await?))
}

#[derive(Deserialize, ToSchema)]
struct JobRequest {
    /// Database UUID of the imported position, not its GNUbg ID.
    position_id: Uuid,
    /// analysis: five candidate moves; dice: 21 weighted rolls.
    kind: JobKind,
    /// GNUbg search depth, 0 to 2. Defaults to 0.
    #[serde(default)]
    #[schema(minimum = 0, maximum = 2)]
    plies: i32,
}

#[utoipa::path(post, path="/api/jobs", request_body=JobRequest, responses((status=202, body=Job), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Analysis")]
async fn create_job(
    State(state): State<AppState>,
    payload: Result<Json<JobRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    let Json(request) = payload.map_err(|e| ApiError::invalid(e.body_text()))?;
    if !(0..=2).contains(&request.plies) {
        return Err(ApiError::invalid("plies must be 0..2"));
    }
    store::get_position(&state.pool, request.position_id)
        .await?
        .ok_or_else(ApiError::missing)?;
    let job = store::create_job(
        &state.pool,
        request.position_id,
        request.kind.as_str(),
        request.plies,
    )
    .await?;
    state.wake.notify_one();
    Ok((StatusCode::ACCEPTED, Json(job)))
}

#[utoipa::path(get, path="/api/jobs/{id}", params(("id"=Uuid, Path)), responses((status=200, body=Job), (status=404, body=ErrorBody)), tag="Analysis")]
async fn job(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Json<Job>> {
    Ok(Json(
        store::get_job(&state.pool, id)
            .await?
            .ok_or_else(ApiError::missing)?,
    ))
}

#[utoipa::path(get, path="/api/jobs/{id}/events", params(("id"=Uuid, Path),("Last-Event-ID"=Option<i64>, Header, description="Resume after this persisted event ID")), responses((status=200, body=String, content_type="text/event-stream", description="SSE wire format: id: <persisted integer>\\nevent: <queued|running|progress|completed|failed>\\ndata: <JobProgress JSON>\\n\\n. Each data field conforms to components.schemas.JobProgress. Stream ends after terminal event or service shutdown; reconnect resumes via Last-Event-ID."),(status=404, body=ErrorBody),(status=422,body=ErrorBody)), tag="Analysis")]
async fn events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    store::get_job(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
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
        health, upload, matches, get_match, source, positions, dice, create_job, job, events
    ),
    components(schemas(
        Health,
        ErrorBody,
        UploadForm,
        ImportResponse,
        MatchSummary,
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
        DiceEvaluation
    ))
)]
pub struct ApiDoc;

async fn openapi() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}
