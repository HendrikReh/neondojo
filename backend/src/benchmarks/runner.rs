use super::{BenchmarkRun, REFERENCE_SGF};
use crate::{
    api::AppState,
    engine::{Engine, benchmark_exclusive},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// A timing run is indivisible: a crashed process must never resume partial times.
pub async fn recover_interrupted(pool: &PgPool) -> Result<()> {
    sqlx::query("UPDATE benchmark_runs SET status='failed',finished_at=now(),error='Benchmark interrupted by backend restart; start a new measurement',worker_token=NULL,lease_until=NULL WHERE status='running'")
        .execute(pool).await?;
    Ok(())
}

pub async fn run_one(state: &AppState) -> Result<bool> {
    let token = Uuid::new_v4();
    let mut tx = state.pool.begin().await?;
    // The transaction lock also makes concurrent workers observe the unique
    // running row before claiming another run. It is never held during analysis.
    let owns: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(91432301)")
        .fetch_one(&mut *tx)
        .await?;
    if !owns {
        return Ok(false);
    }
    sqlx::query("UPDATE benchmark_runs SET status='failed',finished_at=now(),error='Benchmark worker lease expired; start a new measurement',worker_token=NULL,lease_until=NULL WHERE status='running' AND (lease_until IS NULL OR lease_until<now())")
        .execute(&mut *tx).await?;
    let run: Option<BenchmarkRun> = sqlx::query_as("UPDATE benchmark_runs SET status='running',worker_token=$1,lease_until=now()+interval '30 seconds' WHERE id=(SELECT id FROM benchmark_runs WHERE status='queued' AND NOT EXISTS(SELECT 1 FROM benchmark_runs WHERE status='running') ORDER BY created_at,queue_no FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *")
        .bind(token).fetch_optional(&mut *tx).await?;
    tx.commit().await?;
    let Some(run) = run else {
        return Ok(false);
    };
    let watch = async {
        let mut renewed = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM benchmark_runs WHERE id=$1 AND status='running' AND worker_token=$2 AND lease_until>now())")
                .bind(run.id).bind(token).fetch_one(&state.pool).await?;
            ensure!(active, "Benchmark cancelled or worker ownership lost");
            if renewed.elapsed() >= Duration::from_secs(1) {
                let updated = sqlx::query("UPDATE benchmark_runs SET lease_until=now()+interval '30 seconds' WHERE id=$1 AND worker_token=$2 AND status='running' AND lease_until>now()")
                    .bind(run.id).bind(token).execute(&state.pool).await?;
                ensure!(
                    updated.rows_affected() == 1,
                    "Benchmark worker lease was lost"
                );
                renewed = Instant::now();
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };
    let result = tokio::select! {
        result=benchmark_exclusive(measure(state, &run, token)) => result,
        result=watch => result.map(|_|0.0),
    };
    match result {
        Ok(elapsed) => {
            sqlx::query("UPDATE benchmark_runs SET status='completed',finished_at=now(),total_elapsed_ms=$3,average_move_ms=CASE WHEN play_reference IS NULL THEN $3/analyzed_moves END,average_turn_ms=CASE WHEN play_reference IS NOT NULL THEN $3/completed_turns END,worker_token=NULL,lease_until=NULL WHERE id=$1 AND worker_token=$2 AND status='running' AND lease_until>now()")
                .bind(run.id).bind(token).bind(elapsed).execute(&state.pool).await?;
        }
        Err(error) => {
            sqlx::query("UPDATE benchmark_runs SET status='failed',finished_at=now(),error=$3,worker_token=NULL,lease_until=NULL WHERE id=$1 AND worker_token=$2 AND status='running' AND lease_until>now()")
                .bind(run.id).bind(token).bind(format!("{error:#}")).execute(&state.pool).await?;
        }
    }
    Ok(true)
}

pub(super) async fn mark_started(pool: &PgPool, id: Uuid, token: Uuid) -> Result<()> {
    let started = sqlx::query("UPDATE benchmark_runs SET started_at=now() WHERE id=$1 AND worker_token=$2 AND status='running' AND lease_until>now()").bind(id).bind(token).execute(pool).await?;
    ensure!(started.rows_affected() == 1, "Benchmark cancelled");
    Ok(())
}

async fn measure(state: &AppState, run: &BenchmarkRun, token: Uuid) -> Result<f64> {
    if run.play_reference.is_some() {
        return super::play_latency::measure(&state.pool, run, token).await;
    }
    let reference = run
        .reference
        .as_ref()
        .context("Missing analysis reference")?;
    let protocol = run.protocol.as_ref().context("Missing analysis protocol")?;
    let source: String = sqlx::query_scalar("SELECT reference_sgf FROM benchmark_runs WHERE id=$1")
        .bind(run.id)
        .fetch_one(&state.pool)
        .await?;
    ensure!(
        format!("{:x}", Sha256::digest(source.as_bytes())) == reference.sha256,
        "Reference checksum mismatch"
    );
    ensure!(
        source == REFERENCE_SGF
            && matches!(
                protocol.id.as_str(),
                "gnubg-match-v1" | "gnubg-match-v2-unlimited" | "bgsage-match-v3-2ply-4threads"
            ),
        "Unsupported benchmark protocol or reference version"
    );
    let imported = crate::import::parse_sgf(&source)?;
    ensure!(
        imported.positions.len() == reference.total_moves as usize
            && crate::match_analysis::game_count(&source)? == reference.total_games,
        "Incomplete reference workload"
    );
    let engine = saved_engine(&state.pool, run).await?;
    let startup = prepare(&state.pool, run, token, &engine).await?;
    let guard_engine = engine.clone();
    guard_engine
        .version()
        .await
        .context("Benchmark engine changed or is unavailable")?;
    mark_started(&state.pool, run.id, token).await?;
    let mut elapsed_ms = startup;
    let mut moves = 0;
    for game_no in 1..=reference.total_games {
        guard_engine
            .version()
            .await
            .context("Benchmark engine changed before game")?;
        let start = Instant::now();
        let game = engine
            .analyze_game_with_limit(
                &source,
                game_no,
                run.plies,
                protocol
                    .game_timeout_seconds
                    .map(|seconds| Duration::from_secs(seconds as u64)),
            )
            .await
            .with_context(|| {
                let limit = protocol
                    .game_timeout_seconds
                    .map(|seconds| format!("{seconds} second timeout per game"))
                    .unwrap_or_else(|| "no time limit".into());
                format!(
                    "Benchmark game {game_no}/{} failed ({limit})",
                    reference.total_games
                )
            })?;
        elapsed_ms += start.elapsed().as_secs_f64() * 1000.0;
        guard_engine
            .version()
            .await
            .context("Benchmark engine changed during game")?;
        ensure!(
            game.engine == run.engine_version,
            "Engine version changed during benchmark"
        );
        let count = game
            .decisions
            .iter()
            .filter(|d| d.kind == "checker")
            .count();
        let expected = imported
            .positions
            .iter()
            .filter(|p| p.game_no == game_no)
            .count();
        ensure!(
            count == expected,
            "Incomplete checker coverage in game {game_no}: {count}/{expected}"
        );
        moves += count as i32;
        let changed = sqlx::query("UPDATE benchmark_runs SET completed_games=$3,analyzed_moves=$4 WHERE id=$1 AND worker_token=$2 AND status='running' AND lease_until>now()")
            .bind(run.id).bind(token).bind(game_no).bind(moves).execute(&state.pool).await?;
        ensure!(changed.rows_affected() == 1, "Benchmark cancelled");
    }
    ensure!(
        moves == reference.total_moves && elapsed_ms > 0.0 && elapsed_ms.is_finite(),
        "Incomplete benchmark or invalid elapsed time"
    );
    guard_engine
        .version()
        .await
        .context("Engine changed at benchmark completion")?;
    Ok(elapsed_ms)
}

pub(super) async fn saved_snapshot(pool: &PgPool, id: Uuid) -> Result<Option<serde_json::Value>> {
    Ok(
        sqlx::query_scalar("SELECT engine_snapshot FROM benchmark_runs WHERE id=$1")
            .bind(id)
            .fetch_one(pool)
            .await?,
    )
}
pub(super) async fn saved_engine(
    pool: &PgPool,
    run: &BenchmarkRun,
) -> Result<crate::engines::AnalysisEngine> {
    if let Some(value) = saved_snapshot(pool, run.id).await? {
        let snapshot: crate::settings::EngineSnapshot =
            serde_json::from_value(value["engine"].clone())?;
        ensure!(
            snapshot.engine_id == run.engine_id,
            "Benchmark engine identity mismatch"
        );
        snapshot.engine()
    } else {
        ensure!(
            run.engine_id == "gnubg",
            "Missing benchmark engine snapshot"
        );
        Ok(Engine::new(run.engine_path.clone())
            .with_expected_version(Some(run.engine_version.clone()))
            .into())
    }
}

pub(super) async fn prepare(
    pool: &PgPool,
    run: &BenchmarkRun,
    token: Uuid,
    engine: &crate::engines::AnalysisEngine,
) -> Result<f64> {
    if let crate::engines::AnalysisEngine::OpenSage(engine) = engine {
        let start = Instant::now();
        engine.restart().await?;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let updated = sqlx::query("UPDATE benchmark_runs SET startup_elapsed_ms=$3 WHERE id=$1 AND worker_token=$2 AND status='running' AND lease_until>now()")
            .bind(run.id).bind(token).bind(elapsed).execute(pool).await?;
        ensure!(
            updated.rows_affected() == 1,
            "Benchmark cancelled during initialization"
        );
        return Ok(elapsed);
    }
    Ok(0.0)
}
