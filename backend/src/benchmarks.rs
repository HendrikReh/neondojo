//! Reproducible, append-only engine timing; independent of the match library.
use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use crate::engine::{GnubgMoveFilter, resolve_binary};
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
fn reference_for(
    engine: &str,
    plies: i32,
    move_filter: Option<GnubgMoveFilter>,
) -> BenchmarkReference {
    let mut reference = reference();
    let value = &mut reference.protocol;
    if engine == "bgsage" {
        value.id = format!("bgsage-match-v3-{plies}ply-4threads");
        value.process_strategy = "persistent-worker-atomic-game-verified-at-both-boundaries".into();
        value.timing_scope = "cold-worker-startup-once-plus-full-game-pipeline".into();
        value.move_filter = format!("all-cubeful-{plies}ply-candidates-played-board-included");
    } else if let Some(GnubgMoveFilter::Top14) = move_filter {
        value.id = format!("gnubg-match-v3-{plies}ply-top14");
        value.move_filter = format!("gnubg-{plies}ply-top14");
    }
    reference
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct PlayBenchmarkRequest {
    pub engine_id: String,
    pub plies: i32,
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BenchmarkEngines {
    pub engine: crate::settings::EngineSnapshot,
    pub rules_version: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct BenchmarkRun {
    #[sqlx(json(nullable))]
    pub engine_snapshot: Option<BenchmarkEngines>,
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
    /// Cold Open Sage worker initialization, outside the warm pipeline timer.
    pub startup_elapsed_ms: Option<f64>,
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
pub(crate) async fn has_queued(pool: &sqlx::PgPool) -> anyhow::Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM benchmark_runs WHERE status='queued')")
            .fetch_one(pool)
            .await?,
    )
}

pub(crate) async fn running_engine_version(
    pool: &sqlx::PgPool,
    path: &str,
) -> anyhow::Result<Option<String>> {
    let run: Option<(String, String, Option<serde_json::Value>)> = sqlx::query_as(
        "SELECT engine_path,engine_version,engine_snapshot FROM benchmark_runs WHERE status='running' LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(run.and_then(|(saved, version, snapshot)| {
        let same_path =
            |saved: &str| path == saved || resolve_binary(path).ok().as_deref() == Some(saved);
        if same_path(&saved) {
            return Some(version);
        }
        let snapshot = snapshot?;
        let rules_path = snapshot["engine"]["gnubg_path"].as_str()?;
        if same_path(rules_path)
            || resolve_binary(rules_path).ok().as_deref() == resolve_binary(path).ok().as_deref()
        {
            snapshot["rules_version"].as_str().map(str::to_owned)
        } else {
            None
        }
    }))
}

#[utoipa::path(post, path="/api/benchmarks",request_body=BenchmarkRequest,responses((status=202,body=Vec<BenchmarkRun>),(status=422,body=ErrorBody),(status=409,body=ErrorBody)),tag="Benchmark")]
async fn create(
    State(state): State<AppState>,
    payload: Result<Json<BenchmarkRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<(StatusCode, Json<Vec<BenchmarkRun>>)> {
    let Json(request) = payload.map_err(|e| ApiError::invalid(e.body_text()))?;
    if request.plies.is_empty()
        || request.plies.len() > 5
        || request.plies.iter().any(|p| {
            !crate::configuration::engine_depths(&request.engine_id, "analysis").contains(p)
        })
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
    let runs = enqueue(
        &state,
        &request.engine_id,
        Workload::Analysis(request.plies),
    )
    .await?;
    Ok((StatusCode::ACCEPTED, Json(runs)))
}

#[utoipa::path(post, path="/api/benchmarks/play", request_body=Option<PlayBenchmarkRequest>, responses((status=202,body=BenchmarkRun),(status=422,body=ErrorBody),(status=409,body=ErrorBody)),tag="Benchmark")]
async fn create_play(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> ApiResult<(StatusCode, Json<BenchmarkRun>)> {
    let settings = crate::settings::load(&state).await?;
    let payload: Option<PlayBenchmarkRequest> = if body.is_empty() {
        None
    } else {
        serde_json::from_slice(&body).map_err(|e| ApiError::invalid(e.to_string()))?
    };
    let request = payload.unwrap_or(PlayBenchmarkRequest {
        engine_id: settings.play_engine,
        plies: settings.play_plies,
    });
    if !crate::configuration::engine_depths(&request.engine_id, "play").contains(&request.plies) {
        return Err(ApiError::invalid("Unsupported play benchmark profile"));
    }
    let mut runs = enqueue(&state, &request.engine_id, Workload::Play(request.plies)).await?;
    let run = runs
        .pop()
        .ok_or_else(|| anyhow::anyhow!("Missing queued play benchmark"))?;
    Ok((StatusCode::ACCEPTED, Json(run)))
}

enum Workload {
    Analysis(Vec<i32>),
    Play(i32),
}

async fn enqueue(
    state: &AppState,
    engine_id: &str,
    workload: Workload,
) -> ApiResult<Vec<BenchmarkRun>> {
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
    let (depths, is_play) = match workload {
        Workload::Analysis(plies) => (plies, false),
        Workload::Play(plies) => (vec![plies], true),
    };
    let mut prepared = Vec::new();
    for depth in depths {
        let move_filter = if is_play {
            None
        } else {
            GnubgMoveFilter::for_analysis(engine_id, depth)
        };
        let analysis = (!is_play).then(|| reference_for(engine_id, depth, move_filter));
        let play = if is_play {
            Some(play_latency::reference_for(engine_id, depth)?)
        } else {
            None
        };
        let mut snapshot = crate::settings::snapshot_for(
            state,
            engine_id,
            &settings.gnubg_path,
            None,
            if play.is_some() {
                crate::settings::SAGE_PLAY_THREADS
            } else {
                crate::settings::SAGE_ANALYSIS_THREADS
            },
            depth,
        )
        .await
        .map_err(|e| ApiError::invalid(format!("Engine unavailable: {e:#}")))?;
        snapshot.gnubg_move_filter = move_filter;
        let engine = snapshot.engine()?;
        let path = if engine_id == "bgsage" {
            engine.binary().to_owned()
        } else {
            resolve_binary(engine.binary())
                .map_err(|e| ApiError::invalid(format!("Engine unavailable: {e:#}")))?
        };
        let version = match &engine {
            crate::engines::AnalysisEngine::Gnubg(engine) => engine.check_installation().await,
            crate::engines::AnalysisEngine::OpenSage(engine) => engine.version().await,
        }
        .map_err(|e| ApiError::invalid(format!("Engine check failed: {e:#}")))?;
        let rules_version = if engine_id == "gnubg" {
            Some(version.clone())
        } else if play.is_some() {
            Some(
                crate::engine::Engine::new(settings.gnubg_path.clone())
                    .check_installation()
                    .await
                    .map_err(|e| ApiError::invalid(format!("GNUbg rules unavailable: {e:#}")))?,
            )
        } else {
            crate::engine::Engine::new(settings.gnubg_path.clone())
                .version()
                .await
                .ok()
        };
        let saved_snapshot = serde_json::json!({"engine":snapshot,"rules_version":rules_version});
        prepared.push((depth, analysis, play, version, path, saved_snapshot));
    }
    let machine = machine::snapshot(&state.pool).await?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let mut runs = Vec::new();
    for (plies, analysis, play, version, path, saved_snapshot) in prepared {
        let run: BenchmarkRun = sqlx::query_as("INSERT INTO benchmark_runs(id,machine,engine_id,engine_name,engine_version,engine_path,plies,reference,protocol,reference_sgf,play_reference,engine_snapshot) VALUES($1,$2,$10,$11,$3,$4,$5,$6,$7,$8,$9,$12) RETURNING *")
            .bind(Uuid::new_v4()).bind(SqlJson(&machine)).bind(&version).bind(&path).bind(plies)
            .bind(analysis.as_ref().map(SqlJson)).bind(analysis.as_ref().map(|reference| SqlJson(&reference.protocol)))
            .bind(analysis.as_ref().map(|_| REFERENCE_SGF)).bind(play.as_ref().map(SqlJson))
            .bind(engine_id).bind(if engine_id == "bgsage" {"Open Sage"} else {"GNUbg"}).bind(&saved_snapshot)
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
        BenchmarkEngines,
        PlayBenchmarkReference,
        BenchmarkRequest,
        PlayBenchmarkRequest,
        BenchmarkHistory,
        BenchmarkStatus
    ))
)]
pub struct BenchmarkDoc;
