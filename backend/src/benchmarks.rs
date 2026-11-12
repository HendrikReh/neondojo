//! Reproducible, append-only engine timing; independent of the match library.
use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use crate::engine::resolve_binary;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, types::Json as SqlJson};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

pub(crate) mod estimate;
mod machine;
mod play_latency;
mod runner;
use play_latency::PlayBenchmarkReference;
pub use runner::{recover_interrupted, run_one};
pub const REFERENCE_SGF: &str = include_str!("benchmarks/reference.sgf");

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Machine {
    pub id: Uuid,
    pub name: String,
    pub os: String,
    pub architecture: String,
    pub cpu_model: Option<String>,
    pub logical_cpus: Option<i32>,
    pub memory_bytes: Option<i64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BenchmarkProtocol {
    pub id: String,
    pub game_timeout_seconds: Option<i32>,
    pub cubeful: bool,
    pub deterministic: bool,
    pub noise: i32,
    pub luck_analysis: bool,
    pub process_strategy: String,
    pub timing_scope: String,
    pub move_filter: String,
}
fn protocol() -> BenchmarkProtocol {
    BenchmarkProtocol {
        id: "gnubg-match-v2-unlimited".into(),
        game_timeout_seconds: None,
        cubeful: true,
        deterministic: true,
        noise: 0,
        luck_analysis: false,
        process_strategy: "fresh-process-per-game".into(),
        timing_scope: "sum-of-full-game-pipeline-including-cube-startup-load-and-parse".into(),
        move_filter: "gnubg-startup-defaults".into(),
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BenchmarkReference {
    pub id: String,
    pub version: i32,
    pub name: String,
    pub sha256: String,
    pub match_length: i32,
    pub total_games: i32,
    pub total_moves: i32,
    pub protocol: BenchmarkProtocol,
}
pub fn reference() -> BenchmarkReference {
    BenchmarkReference {
        id: "wc2014-v1".into(),
        version: 1,
        name: "WC2014 – Yazawa / Mayfield".into(),
        sha256: format!("{:x}", Sha256::digest(REFERENCE_SGF.as_bytes())),
        match_length: 11,
        total_games: 10,
        total_moves: 460,
        protocol: protocol(),
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct BenchmarkRun {
    pub id: Uuid,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    #[sqlx(json)]
    pub machine: Machine,
    pub engine_id: String,
    pub engine_name: String,
    pub engine_version: String,
    pub engine_path: String,
    pub plies: i32,
    #[sqlx(json(nullable))]
    pub reference: Option<BenchmarkReference>,
    #[sqlx(json(nullable))]
    pub protocol: Option<BenchmarkProtocol>,
    #[sqlx(json(nullable))]
    pub play_reference: Option<PlayBenchmarkReference>,
    pub total_elapsed_ms: Option<f64>,
    pub average_move_ms: Option<f64>,
    pub average_turn_ms: Option<f64>,
    pub completed_games: i32,
    pub analyzed_moves: i32,
    pub completed_turns: i32,
    pub error: Option<String>,
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct BenchmarkRequest {
    pub engine_id: String,
    pub plies: Vec<i32>,
}
#[derive(Serialize, ToSchema)]
pub struct BenchmarkStatus {
    pub active: bool,
}
#[derive(Serialize, ToSchema)]
pub struct BenchmarkHistory {
    pub runs: Vec<BenchmarkRun>,
    pub active: bool,
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .merge(estimate::routes())
        .route("/api/benchmarks", get(history).post(create))
        .route("/api/benchmarks/play", post(create_play))
        .route("/api/benchmarks/reference", get(get_reference))
        .route("/api/benchmarks/status", get(status))
        .route("/api/benchmarks/{id}/cancel", post(cancel))
}
#[utoipa::path(get, path="/api/benchmarks/reference", responses((status=200,body=BenchmarkReference)),tag="Benchmark")]
async fn get_reference() -> Json<BenchmarkReference> {
    Json(reference())
}
#[utoipa::path(get, path="/api/benchmarks", responses((status=200,body=BenchmarkHistory)),tag="Benchmark")]
async fn history(State(state): State<AppState>) -> ApiResult<Json<BenchmarkHistory>> {
    let runs: Vec<BenchmarkRun> = sqlx::query_as(
        "SELECT * FROM benchmark_runs ORDER BY created_at DESC,queue_no DESC LIMIT 1000",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM benchmark_runs WHERE status IN ('queued','running'))",
    )
    .fetch_one(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    Ok(Json(BenchmarkHistory { runs, active }))
}
#[utoipa::path(get,path="/api/benchmarks/status",responses((status=200,body=BenchmarkStatus)),tag="Benchmark")]
async fn status(State(state): State<AppState>) -> ApiResult<Json<BenchmarkStatus>> {
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM benchmark_runs WHERE status IN ('queued','running'))",
    )
    .fetch_one(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    Ok(Json(BenchmarkStatus { active }))
}

/// During an exclusive measurement, availability is the already validated
/// installation snapshot. Starting a second probe would contaminate the run.
pub(crate) async fn running_engine_version(
    pool: &sqlx::PgPool,
    path: &str,
) -> anyhow::Result<Option<String>> {
    let run: Option<(String, String)> = sqlx::query_as(
        "SELECT engine_path,engine_version FROM benchmark_runs WHERE status='running' LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(run.and_then(|(saved, version)| {
        resolve_binary(path)
            .ok()
            .filter(|p| *p == saved)
            .map(|_| version)
    }))
}

#[utoipa::path(post, path="/api/benchmarks",request_body=BenchmarkRequest,responses((status=202,body=Vec<BenchmarkRun>),(status=422,body=ErrorBody),(status=409,body=ErrorBody)),tag="Benchmark")]
async fn create(
    State(state): State<AppState>,
    payload: Result<Json<BenchmarkRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<(StatusCode, Json<Vec<BenchmarkRun>>)> {
    let Json(request) = payload.map_err(|e| ApiError::invalid(e.body_text()))?;
    if request.engine_id != "gnubg"
        || request.plies.is_empty()
        || request.plies.len() > 5
        || request.plies.iter().any(|p| !(0..=4).contains(p))
        || request
            .plies
            .iter()
            .enumerate()
            .any(|(i, p)| request.plies[..i].contains(p))
    {
        return Err(ApiError::invalid(
            "Select an available engine and distinct analysis depths from 0 to 4",
        ));
    }
    let runs = enqueue(&state, Workload::Analysis(request.plies)).await?;
    Ok((StatusCode::ACCEPTED, Json(runs)))
}

#[utoipa::path(post, path="/api/benchmarks/play", responses((status=202,body=BenchmarkRun),(status=422,body=ErrorBody),(status=409,body=ErrorBody)),tag="Benchmark")]
async fn create_play(State(state): State<AppState>) -> ApiResult<(StatusCode, Json<BenchmarkRun>)> {
    let mut runs = enqueue(&state, Workload::Play).await?;
    let run = runs
        .pop()
        .ok_or_else(|| anyhow::anyhow!("Missing queued play benchmark"))?;
    Ok((StatusCode::ACCEPTED, Json(run)))
}

enum Workload {
    Analysis(Vec<i32>),
    Play,
}

async fn enqueue(state: &AppState, workload: Workload) -> ApiResult<Vec<BenchmarkRun>> {
    let running: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM benchmark_runs WHERE status='running')")
            .fetch_one(&state.pool)
            .await
            .map_err(anyhow::Error::from)?;
    if running {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "A benchmark is already running; wait for it to finish".into(),
        ));
    }
    let settings = crate::settings::load(state).await?;
    let (plies, analysis, play) = match workload {
        Workload::Analysis(plies) => (plies, Some(reference()), None),
        Workload::Play => (
            vec![settings.play_plies],
            None,
            Some(play_latency::reference()?),
        ),
    };
    let path = resolve_binary(&settings.gnubg_path)
        .map_err(|e| ApiError::invalid(format!("GNUbg unavailable: {e:#}")))?;
    let engine = crate::engine::Engine::new(path.clone());
    let version = engine
        .check_installation()
        .await
        .map_err(|e| ApiError::invalid(format!("GNUbg installation check failed: {e:#}")))?;
    let machine = machine::snapshot(&state.pool).await?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let mut runs = Vec::new();
    for plies in plies {
        let run: BenchmarkRun = sqlx::query_as("INSERT INTO benchmark_runs(id,machine,engine_id,engine_name,engine_version,engine_path,plies,reference,protocol,reference_sgf,play_reference) VALUES($1,$2,'gnubg','GNUbg',$3,$4,$5,$6,$7,$8,$9) RETURNING *")
            .bind(Uuid::new_v4()).bind(SqlJson(&machine)).bind(&version).bind(&path).bind(plies)
            .bind(analysis.as_ref().map(SqlJson)).bind(analysis.as_ref().map(|reference| SqlJson(&reference.protocol)))
            .bind(analysis.as_ref().map(|_| REFERENCE_SGF)).bind(play.as_ref().map(SqlJson))
            .fetch_one(&mut *tx).await.map_err(anyhow::Error::from)?;
        runs.push(run);
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    state.wake.notify_one();
    Ok(runs)
}
#[utoipa::path(post,path="/api/benchmarks/{id}/cancel",params(("id"=Uuid,Path)),responses((status=200,body=BenchmarkRun),(status=404,body=ErrorBody)),tag="Benchmark")]
async fn cancel(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BenchmarkRun>> {
    let run: Option<BenchmarkRun> = sqlx::query_as("UPDATE benchmark_runs SET status=CASE WHEN status IN ('queued','running') THEN 'cancelled' ELSE status END, finished_at=CASE WHEN status IN ('queued','running') THEN now() ELSE finished_at END WHERE id=$1 RETURNING *")
        .bind(id).fetch_optional(&state.pool).await.map_err(anyhow::Error::from)?;
    state.wake.notify_one();
    Ok(Json(run.ok_or_else(ApiError::missing)?))
}
#[derive(OpenApi)]
#[openapi(
    paths(get_reference, history, status, create, create_play, cancel),
    components(schemas(
        Machine,
        BenchmarkProtocol,
        BenchmarkReference,
        BenchmarkRun,
        PlayBenchmarkReference,
        BenchmarkRequest,
        BenchmarkHistory,
        BenchmarkStatus
    ))
)]
pub struct BenchmarkDoc;
