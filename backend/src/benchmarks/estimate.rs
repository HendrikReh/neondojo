//! Read-only scope estimates calibrated by complete local native measurements.
use super::{
    BenchmarkRun, Machine, known_protocol_id, machine, reference, reference_for,
    running_engine_version,
};
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    engine::{Engine, GnubgMoveFilter, resolve_binary},
    store,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State, rejection::QueryRejection},
    routing::get,
};
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Deserialize, IntoParams)]
pub struct EstimateQuery {
    #[param(minimum = 1, maximum = 5)]
    pub plies: Option<i32>,
    pub game_no: Option<i32>,
    /// Provider for this estimate; omitted uses the saved analysis engine.
    pub engine_id: Option<String>,
}
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EstimateUnavailableReason {
    NoMatchingBenchmark,
    EngineUnavailable,
    NoCheckerMoves,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct EstimateBenchmark {
    pub id: Uuid,
    pub finished_at: DateTime<Utc>,
    pub reference_name: String,
    pub average_move_ms: f64,
    pub protocol_id: String,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct AnalysisTimeEstimate {
    pub match_id: Uuid,
    pub game_no: Option<i32>,
    pub engine_id: String,
    pub engine_version: Option<String>,
    pub plies: i32,
    pub position_count: i64,
    pub estimated_ms: Option<f64>,
    pub unavailable_reason: Option<EstimateUnavailableReason>,
    pub benchmark: Option<EstimateBenchmark>,
}
impl AnalysisTimeEstimate {
    fn unavailable(mut self, reason: EstimateUnavailableReason) -> Json<Self> {
        self.unavailable_reason = Some(reason);
        Json(self)
    }
}
pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/api/matches/{id}/analysis/estimate", get(estimate))
}
fn same_hardware(a: &Machine, b: &Machine) -> bool {
    a.id == b.id
        && a.os == b.os
        && a.architecture == b.architecture
        && a.cpu_model == b.cpu_model
        && a.logical_cpus == b.logical_cpus
        && a.memory_bytes == b.memory_bytes
}
fn compatible(run: &BenchmarkRun, machine: &Machine, move_filter: Option<GnubgMoveFilter>) -> bool {
    let expected = reference_for(&run.engine_id, run.plies, move_filter);
    let (Some(saved_reference), Some(protocol_snapshot)) = (&run.reference, &run.protocol) else {
        return false;
    };
    let mut normalized = protocol_snapshot.clone();
    if !known_protocol_id(&run.engine_id, run.plies, move_filter, &normalized.id) {
        return false;
    }
    normalized.id = expected.protocol.id.clone();
    normalized.game_timeout_seconds = expected.protocol.game_timeout_seconds;
    let total = run.total_elapsed_ms.unwrap_or(f64::NAN);
    let average = run.average_move_ms.unwrap_or(f64::NAN);
    let expected_average = total / f64::from(expected.total_moves);
    same_hardware(&run.machine, machine)
        && normalized == expected.protocol
        && saved_reference.id == expected.id
        && saved_reference.version == expected.version
        && saved_reference.sha256 == expected.sha256
        && saved_reference.total_games == expected.total_games
        && saved_reference.total_moves == expected.total_moves
        && saved_reference.match_length == expected.match_length
        && run.completed_games == expected.total_games
        && run.analyzed_moves == expected.total_moves
        && run.finished_at.is_some()
        && total.is_finite()
        && total > 0.0
        && average.is_finite()
        && average > 0.0
        && (average - expected_average).abs() <= expected_average.abs() * 1e-9
}

#[utoipa::path(get,path="/api/matches/{id}/analysis/estimate",params(("id"=Uuid,Path),EstimateQuery),responses((status=200,body=AnalysisTimeEstimate),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Match analysis")]
async fn estimate(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    query: Result<Query<EstimateQuery>, QueryRejection>,
) -> ApiResult<Json<AnalysisTimeEstimate>> {
    let Query(query) = query.map_err(|e| ApiError::invalid(e.body_text()))?;
    crate::play::require_review_available(&state.pool, id).await?;
    let source = store::get_match(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    if !source.source_available {
        return Err(ApiError::invalid(
            "The source match was removed; analyze the saved position instead",
        ));
    }
    // The existing settings loader initializes missing rows. An estimate must
    // stay read-only, including on a fresh installation.
    let (default_engine, default_plies, binary, gnubg_threads, open_sage_threads):
        (String, i32, String, Option<i32>, Option<i32>) = sqlx::query_as(
        "SELECT analysis_engine,analysis_plies,gnubg_path,gnubg_threads,open_sage_threads FROM app_settings WHERE singleton=true",
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(anyhow::Error::from)?
    .unwrap_or_else(|| ("gnubg".into(), 2, state.engine.binary().to_owned(), None, None));
    let engine_id = query.engine_id.unwrap_or(default_engine);
    let plies = query.plies.unwrap_or(default_plies);
    if !crate::configuration::engine_depths(&engine_id, "analysis").contains(&plies) {
        return Err(ApiError::invalid(
            "Depth is not verified for the selected engine",
        ));
    }
    if let Some(game_no) = query.game_no {
        let mut games: Vec<i32> =
            sqlx::query_scalar("SELECT game_no FROM match_games WHERE match_uuid=$1")
                .bind(id)
                .fetch_all(&state.pool)
                .await
                .map_err(anyhow::Error::from)?;
        if games.is_empty() {
            // Legacy matches may not have a catalog yet. Read their SGF tree
            // count without invoking the metadata-backfilling store helper.
            let sgf: Option<String> = sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1")
                .bind(id)
                .fetch_optional(&state.pool)
                .await
                .map_err(anyhow::Error::from)?;
            let sgf = sgf.ok_or_else(ApiError::missing)?;
            games = (1..=crate::match_analysis::game_count(&sgf)?).collect();
        }
        if !games.contains(&game_no) {
            return Err(ApiError::invalid("game_no is outside this match"));
        }
    }
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM positions WHERE match_uuid=$1 AND ($2::integer IS NULL OR game_no=$2)")
        .bind(id).bind(query.game_no).fetch_one(&state.pool).await.map_err(anyhow::Error::from)?;
    let mut result = AnalysisTimeEstimate {
        match_id: id,
        game_no: query.game_no,
        engine_id,
        engine_version: None,
        plies,
        position_count: count,
        estimated_ms: None,
        unavailable_reason: None,
        benchmark: None,
    };
    if count == 0 {
        return Ok(result.unavailable(EstimateUnavailableReason::NoCheckerMoves));
    }
    let identity = if result.engine_id == "bgsage" {
        if let Some(engine) = crate::settings::open_sage::load(&state.pool)
            .await
            .ok()
            .and_then(|i| i.snapshot)
            .and_then(|mut snapshot| {
                snapshot.plies = plies;
                snapshot.threads = crate::settings::sage_threads(
                    open_sage_threads,
                    crate::settings::SAGE_ANALYSIS_THREADS,
                );
                crate::engines::SageEngine::new(snapshot).ok()
            })
        {
            let version = match running_engine_version(&state.pool, engine.binary()).await? {
                Some(_) => engine.saved_version().map(str::to_owned),
                None => tokio::time::timeout(Duration::from_secs(2), engine.version())
                    .await
                    .ok()
                    .and_then(Result::ok),
            };
            version.map(|v| (engine.binary().to_owned(), v))
        } else {
            None
        }
    } else if let Ok(path) = resolve_binary(&binary) {
        let version = match running_engine_version(&state.pool, &path).await? {
            Some(version) => Some(version),
            None => {
                tokio::time::timeout(Duration::from_secs(2), Engine::new(path.clone()).version())
                    .await
                    .ok()
                    .and_then(Result::ok)
            }
        };
        version
            .filter(|v| v.starts_with("GNU Backgammon "))
            .map(|v| (path, v))
    } else {
        None
    };
    let Some((path, version)) = identity else {
        return Ok(result.unavailable(EstimateUnavailableReason::EngineUnavailable));
    };
    result.engine_version = Some(version.clone());
    if result.engine_id == "gnubg" && gnubg_threads.unwrap_or(1) != 1 {
        return Ok(result.unavailable(EstimateUnavailableReason::NoMatchingBenchmark));
    }
    let move_filter = GnubgMoveFilter::for_analysis(&result.engine_id, plies);
    if let Some(benchmark) = matching_benchmark(
        &state.pool,
        &result.engine_id,
        &path,
        &version,
        plies,
        move_filter,
    )
    .await?
    {
        let elapsed = benchmark.average_move_ms * count as f64;
        if elapsed.is_finite() && elapsed > 0.0 {
            result.estimated_ms = Some(elapsed);
            result.benchmark = Some(benchmark);
            return Ok(Json(result));
        }
    }
    Ok(result.unavailable(EstimateUnavailableReason::NoMatchingBenchmark))
}

/// Shared read-only compatibility lookup. Callers supply their saved engine basis.
pub(crate) async fn matching_benchmark(
    pool: &sqlx::PgPool,
    engine_id: &str,
    path: &str,
    version: &str,
    plies: i32,
    move_filter: Option<GnubgMoveFilter>,
) -> anyhow::Result<Option<EstimateBenchmark>> {
    let Some(machine) = machine::current(pool).await? else {
        return Ok(None);
    };
    let expected = reference();
    let mut rows=sqlx::query_as::<_,BenchmarkRun>("SELECT * FROM benchmark_runs WHERE status='completed' AND play_reference IS NULL AND engine_id=$1 AND engine_path=$2 AND engine_version=$3 AND plies=$4 AND machine->>'id'=$5 AND reference->>'sha256'=$6 ORDER BY finished_at DESC,created_at DESC,queue_no DESC")
        .bind(engine_id).bind(path).bind(version).bind(plies).bind(machine.id.to_string()).bind(expected.sha256).fetch(pool);
    while let Some(run) = rows.try_next().await? {
        if compatible(&run, &machine, move_filter) {
            return Ok(Some(EstimateBenchmark {
                id: run.id,
                finished_at: run.finished_at.expect("compatible measurement is finished"),
                reference_name: run
                    .reference
                    .expect("compatible measurement has a reference")
                    .name,
                average_move_ms: run
                    .average_move_ms
                    .expect("compatible measurement has average"),
                protocol_id: run
                    .protocol
                    .expect("compatible measurement has a protocol")
                    .id,
            }));
        }
    }
    Ok(None)
}

#[derive(OpenApi)]
#[openapi(
    paths(estimate),
    components(schemas(AnalysisTimeEstimate, EstimateBenchmark, EstimateUnavailableReason))
)]
pub struct EstimateDoc;
