//! Read-only scope estimates calibrated by complete local native measurements.
use super::{
    BenchmarkRun, Machine, known_protocol_id, machine, reference, reference_for,
    running_engine_version,
};
use crate::domain_error::DomainError;
use crate::json_errors::{Json, Path, Query};
use crate::{
    api::{ApiResult, AppState, ErrorBody},
    engine::{Engine, GnubgMoveFilter, resolve_binary_async},
    engines::EngineId,
    store,
};
use axum::{Router, extract::State, routing::get};
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Debug, Default, Deserialize, IntoParams)]
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
    fn unavailable(mut self, reason: EstimateUnavailableReason) -> Self {
        self.unavailable_reason = Some(reason);
        self
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
fn compatible_estimate(
    run: BenchmarkRun,
    machine: &Machine,
    move_filter: Option<GnubgMoveFilter>,
) -> Option<EstimateBenchmark> {
    let expected = reference_for(&run.engine_id, run.plies, move_filter);
    let saved_reference = run.reference?;
    let mut normalized = run.protocol?;
    let finished_at = run.finished_at?;
    if !known_protocol_id(&run.engine_id, run.plies, move_filter, &normalized.id) {
        return None;
    }
    let protocol_id = std::mem::replace(&mut normalized.id, expected.protocol.id.clone());
    normalized.game_timeout_seconds = expected.protocol.game_timeout_seconds;
    let total = run.total_elapsed_ms?;
    let average = run.average_move_ms?;
    let expected_average = total / f64::from(expected.total_moves);
    let matches = same_hardware(&run.machine, machine)
        && normalized == expected.protocol
        && saved_reference.id == expected.id
        && saved_reference.version == expected.version
        && saved_reference.sha256 == expected.sha256
        && saved_reference.total_games == expected.total_games
        && saved_reference.total_moves == expected.total_moves
        && saved_reference.match_length == expected.match_length
        && run.completed_games == expected.total_games
        && run.analyzed_moves == expected.total_moves
        && total.is_finite()
        && total > 0.0
        && average.is_finite()
        && average > 0.0
        && (average - expected_average).abs() <= expected_average.abs() * 1e-9;
    matches.then_some(EstimateBenchmark {
        id: run.id,
        finished_at,
        reference_name: saved_reference.name,
        average_move_ms: average,
        protocol_id,
    })
}

async fn legacy_game_in_scope(sgf: String, game_no: i32) -> anyhow::Result<bool> {
    let count =
        tokio::task::spawn_blocking(move || crate::match_analysis::game_count(&sgf)).await??;
    Ok((1..=count).contains(&game_no))
}

pub(crate) async fn estimate_match_analysis(
    state: &AppState,
    id: Uuid,
    query: EstimateQuery,
) -> Result<AnalysisTimeEstimate, DomainError> {
    crate::play::require_review_available(&state.pool, id).await?;
    let source = store::get_match(&state.pool, id)
        .await?
        .ok_or_else(DomainError::missing)?;
    if !source.source_available {
        return Err(DomainError::invalid(
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
    .unwrap_or_else(|| {
        (EngineId::Gnubg.id().into(), 2, state.engine.binary().to_owned(), None, None)
    });
    let engine_id = query.engine_id.unwrap_or(default_engine);
    let plies = query.plies.unwrap_or(default_plies);
    let Some(provider) =
        EngineId::parse(&engine_id).filter(|provider| provider.depths("analysis").contains(&plies))
    else {
        return Err(DomainError::invalid(
            "Depth is not verified for the selected engine",
        ));
    };
    if let Some(game_no) = query.game_no {
        let games: Vec<i32> =
            sqlx::query_scalar("SELECT game_no FROM match_games WHERE match_uuid=$1")
                .bind(id)
                .fetch_all(&state.pool)
                .await
                .map_err(anyhow::Error::from)?;
        let in_scope = if games.is_empty() {
            // Legacy matches may not have a catalog yet. Read their SGF tree
            // count without invoking the metadata-backfilling store helper.
            let sgf: Option<String> = sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1")
                .bind(id)
                .fetch_optional(&state.pool)
                .await
                .map_err(anyhow::Error::from)?;
            let sgf = sgf.ok_or_else(DomainError::missing)?;
            legacy_game_in_scope(sgf, game_no).await?
        } else {
            games.contains(&game_no)
        };
        if !in_scope {
            return Err(DomainError::invalid("game_no is outside this match"));
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
    let identity = match provider {
        EngineId::NeonGammon => {
            if let Some(engine) = crate::settings::open_sage::load(&state.pool)
                .await
                .ok()
                .and_then(|i| i.snapshot)
                .and_then(|mut snapshot| {
                    snapshot.plies = plies;
                    snapshot.threads = crate::settings::sage_threads(
                        open_sage_threads,
                        crate::settings::sage_default_analysis_threads(),
                    );
                    crate::engines::SageEngine::new(snapshot).ok()
                })
            {
                // The verified snapshot fixes the version string, as for jobs.
                // Starting a worker here would replace the cached one and, at
                // 4-ply, run an evaluation longer than this read may take; the
                // worker rechecks the installation when the analysis starts.
                tokio::fs::metadata(engine.binary())
                    .await
                    .is_ok_and(|metadata| metadata.is_file())
                    .then(|| engine.saved_version().map(str::to_owned))
                    .flatten()
                    .map(|v| (engine.binary().to_owned(), v))
            } else {
                None
            }
        }
        EngineId::Gnubg => {
            if let Ok(path) = resolve_binary_async(&binary).await {
                let version = match running_engine_version(&state.pool, &path).await? {
                    Some(version) => Some(version),
                    None => tokio::time::timeout(
                        Duration::from_secs(2),
                        Engine::new(path.clone()).version(),
                    )
                    .await
                    .ok()
                    .and_then(Result::ok),
                };
                version
                    .filter(|v| v.starts_with("GNU Backgammon "))
                    .map(|v| (path, v))
            } else {
                None
            }
        }
    };
    let Some((path, version)) = identity else {
        return Ok(result.unavailable(EstimateUnavailableReason::EngineUnavailable));
    };
    result.engine_version = Some(version.clone());
    // Benchmarks measure GNUbg single-threaded.
    if provider.uses_gnubg_threads() && gnubg_threads.unwrap_or(1) != 1 {
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
            return Ok(result);
        }
    }
    Ok(result.unavailable(EstimateUnavailableReason::NoMatchingBenchmark))
}

#[utoipa::path(get,path="/api/matches/{id}/analysis/estimate",params(("id"=Uuid,Path),EstimateQuery),responses((status=200,body=AnalysisTimeEstimate),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Match analysis")]
async fn estimate(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<EstimateQuery>,
) -> ApiResult<Json<AnalysisTimeEstimate>> {
    Ok(Json(estimate_match_analysis(&state, id, query).await?))
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
        if let Some(estimate) = compatible_estimate(run, &machine, move_filter) {
            return Ok(Some(estimate));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::{SageEngine, SageSnapshot};
    use http_body_util::BodyExt;
    use std::os::unix::fs::PermissionsExt;
    use tower::ServiceExt;

    #[test]
    fn legacy_game_validation_yields_while_sgf_parsing_is_queued() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(1)
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (release, wait) = tokio::sync::oneshot::channel::<()>();
            let blocker = tokio::task::spawn_blocking(move || wait.blocking_recv());
            let scope = legacy_game_in_scope("(;GM[6])(;GM[6])".into(), 2);
            tokio::pin!(scope);
            assert!(futures_util::poll!(&mut scope).is_pending());
            release.send(()).unwrap();
            blocker.await.unwrap().unwrap();
            assert!(scope.await.unwrap());
            assert!(!legacy_game_in_scope("(;GM[6])".into(), 2).await.unwrap());
        });
    }

    async fn get_estimate(state: &AppState, id: Uuid) -> serde_json::Value {
        let response = crate::api::router(state.clone())
            .oneshot(
                axum::http::Request::get(format!(
                    "/api/matches/{id}/analysis/estimate?engine_id=bgsage&plies=4"
                ))
                .body(axum::body::Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Opening a 4-ply worker runs a real evaluation that outlasts any
    /// interactive timeout, so the estimate must not start one.
    #[sqlx::test(migrations = "./migrations")]
    async fn neongammon_estimate_uses_the_verified_identity_without_starting_a_worker(
        pool: sqlx::PgPool,
    ) {
        let dir = std::env::temp_dir().join(format!("neondojo-estimate-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let python = dir.join("python");
        let started = dir.join("started");
        std::fs::write(
            &python,
            format!("#!/bin/sh\ntouch '{}'\nexit 1\n", started.display()),
        )
        .unwrap();
        std::fs::set_permissions(&python, std::fs::Permissions::from_mode(0o755)).unwrap();
        let installed = SageSnapshot {
            python: python.to_str().unwrap().into(),
            package_version: "3.1.0".into(),
            artifact_fingerprint:
                "26e7701c27c6c5e0cd3cc864768e27675f0d72149c2ad545ffb0c5293ff705d5".into(),
            threads: 2,
            plies: 2,
            adapter_version: 1,
            one_ply_verified: true,
            cubeless: false,
        };
        sqlx::query(
            "UPDATE engine_installations SET python_path=$1,snapshot=$2 WHERE engine_id='bgsage'",
        )
        .bind(&installed.python)
        .bind(sqlx::types::Json(&installed))
        .execute(&pool)
        .await
        .unwrap();
        let measured = SageEngine::new(SageSnapshot {
            plies: 4,
            threads: crate::settings::sage_default_analysis_threads(),
            ..installed.clone()
        })
        .unwrap();
        let reference = reference_for("bgsage", 4, None);
        sqlx::query("INSERT INTO benchmark_runs(id,status,started_at,finished_at,machine,engine_id,engine_name,engine_version,engine_path,plies,reference,protocol,reference_sgf,completed_games,analyzed_moves,total_elapsed_ms,average_move_ms) VALUES($1,'completed',now(),now(),$2,'bgsage','NeonGammon',$3,$4,4,$5,$6,$7,10,460,$8,$9)")
            .bind(Uuid::new_v4())
            .bind(sqlx::types::Json(machine::snapshot(&pool).await.unwrap()))
            .bind(measured.saved_version().unwrap())
            .bind(&installed.python)
            .bind(sqlx::types::Json(&reference))
            .bind(sqlx::types::Json(&reference.protocol))
            .bind(super::super::REFERENCE_SGF)
            .bind(1260.0 * 460.0)
            .bind(1260.0)
            .execute(&pool)
            .await
            .unwrap();
        let source = super::super::REFERENCE_SGF;
        let id = store::import_match(
            &pool,
            "estimate.sgf",
            "sgf",
            source.as_bytes(),
            source,
            &crate::import::parse_sgf(source).unwrap(),
        )
        .await
        .unwrap()
        .r#match
        .id;
        let state = AppState::new(pool.clone(), Engine::new("/unused".into()));

        let estimate = get_estimate(&state, id).await;
        assert_eq!(
            estimate["unavailable_reason"],
            serde_json::Value::Null,
            "{estimate}"
        );
        assert_eq!(estimate["estimated_ms"], 1260.0 * 460.0, "{estimate}");
        assert_eq!(
            estimate["engine_version"],
            measured.saved_version().unwrap()
        );
        assert!(
            !started.exists(),
            "the estimate started a NeonGammon worker"
        );

        // A removed interpreter still makes the engine unavailable.
        std::fs::remove_file(&python).unwrap();
        let estimate = get_estimate(&state, id).await;
        assert_eq!(
            estimate["unavailable_reason"], "engine_unavailable",
            "{estimate}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
