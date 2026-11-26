use super::{BenchmarkRun, REFERENCE_CHECKER_COUNTS, REFERENCE_SGF};
use crate::{
    api::AppState,
    engine::{Engine, benchmark_exclusive},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::Instant;
use uuid::Uuid;

use crate::worker_lease::{self, LEASE, Owner};

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
    let owns: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
        .bind(crate::advisory_locks::BENCHMARK_CLAIM)
        .fetch_one(&mut *tx)
        .await?;
    if !owns {
        return Ok(false);
    }
    sqlx::query("UPDATE benchmark_runs SET status='failed',finished_at=now(),error='Benchmark worker lease expired; start a new measurement',worker_token=NULL,lease_until=NULL WHERE status='running' AND (lease_until IS NULL OR lease_until<=now())")
        .execute(&mut *tx).await?;
    let claimed_at = Instant::now();
    let run: Option<BenchmarkRun> = sqlx::query_as("UPDATE benchmark_runs SET status='running',worker_token=$1,lease_until=statement_timestamp()+make_interval(secs=>$2) WHERE id=(SELECT id FROM benchmark_runs WHERE status='queued' AND NOT EXISTS(SELECT 1 FROM benchmark_runs WHERE status='running') ORDER BY created_at,queue_no FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *")
        .bind(token).bind(LEASE.as_secs_f64()).fetch_optional(&mut *tx).await?;
    tx.commit().await?;
    let Some(run) = run else {
        return Ok(false);
    };
    let result = tokio::select! {
        result = benchmark_exclusive(measure(state, &run, token)) => result,
        watched = worker_lease::watch(Owner::Benchmark, &state.pool, run.id, token, claimed_at) => match watched {
            Ok(()) => return Ok(true),
            Err(error) => Err(error),
        },
    };
    finish(&state.pool, run.id, token, result).await?;
    Ok(true)
}

async fn finish(pool: &PgPool, id: Uuid, token: Uuid, result: Result<f64>) -> Result<()> {
    // Serialize with cancellation and wait out any row lock before checking
    // the database clock. A delayed worker must never publish a stale result.
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT id FROM benchmark_runs WHERE id=$1 FOR UPDATE")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    match result {
        Ok(elapsed) => {
            sqlx::query("UPDATE benchmark_runs SET status='completed',finished_at=clock_timestamp(),total_elapsed_ms=$3,average_move_ms=CASE WHEN play_reference IS NULL THEN $3/analyzed_moves END,average_turn_ms=CASE WHEN play_reference IS NOT NULL THEN $3/completed_turns END,worker_token=NULL,lease_until=NULL WHERE id=$1 AND worker_token=$2 AND status='running' AND lease_until>clock_timestamp()")
                .bind(id).bind(token).bind(elapsed).execute(&mut *tx).await?;
        }
        Err(error) => {
            sqlx::query("UPDATE benchmark_runs SET status='failed',finished_at=clock_timestamp(),error=$3,worker_token=NULL,lease_until=NULL WHERE id=$1 AND worker_token=$2 AND status='running' AND lease_until>clock_timestamp()")
                .bind(id).bind(token).bind(format!("{error:#}")).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
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
    let checksum = reference.sha256.clone();
    let source = tokio::task::spawn_blocking(move || {
        ensure!(
            format!("{:x}", Sha256::digest(source.as_bytes())) == checksum,
            "Reference checksum mismatch"
        );
        ensure!(
            source == REFERENCE_SGF,
            "Unsupported benchmark protocol or reference version"
        );
        anyhow::Ok(source)
    })
    .await
    .context("Benchmark reference validation worker failed")??;
    let workload = super::reference_for(
        &run.engine_id,
        run.plies,
        crate::engine::GnubgMoveFilter::for_analysis(&run.engine_id, run.plies),
    );
    ensure!(
        protocol.id == workload.protocol.id,
        "Unsupported benchmark protocol or reference version"
    );
    ensure!(
        reference.total_moves == workload.total_moves
            && reference.total_games == workload.total_games,
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
    let preparing = Instant::now();
    let evaluator = crate::engines::replay::MatchEvaluator::prepare(&engine, source).await?;
    let mut elapsed_ms = startup;
    if matches!(
        evaluator,
        crate::engines::replay::MatchEvaluator::NeonGammon(..)
    ) {
        elapsed_ms += preparing.elapsed().as_secs_f64() * 1000.0;
    }
    let mut moves = 0;
    for (game_no, expected) in (1..=reference.total_games).zip(REFERENCE_CHECKER_COUNTS) {
        guard_engine
            .version()
            .await
            .context("Benchmark engine changed before game")?;
        let start = Instant::now();
        let game = evaluator
            .analyze_game(
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
        ensure!(
            count == expected as usize,
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
        // Only GNUbg runs predate saved engine snapshots.
        ensure!(
            crate::engines::EngineId::parse(&run.engine_id)
                == Some(crate::engines::EngineId::Gnubg),
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
    let start = Instant::now();
    if engine.restart_persistent_worker().await? {
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn running(pool: &PgPool, lifetime: Duration) -> (Uuid, Uuid) {
        let id = Uuid::new_v4();
        let token = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO benchmark_runs (id,worker_token,status,lease_until,machine,engine_id,
                engine_name,engine_version,engine_path,plies,reference,protocol,reference_sgf,
                completed_games,analyzed_moves)
             VALUES ($1,$2,'running',now()+make_interval(secs=>$3),'{}','gnubg',
                'GNUbg','test','unused',1,'{}','{}','',10,460)",
        )
        .bind(id)
        .bind(token)
        .bind(lifetime.as_secs_f64())
        .execute(pool)
        .await
        .unwrap();
        (id, token)
    }

    async fn single_connection(pool: &PgPool, acquire_timeout: Duration) -> PgPool {
        PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(acquire_timeout)
            .test_before_acquire(false)
            .connect_with((*pool.connect_options()).clone())
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn watch_retries_pool_timeouts_then_renews_and_stops_on_token_loss(pool: PgPool) {
        let worker = single_connection(&pool, Duration::from_millis(200)).await;
        let held = worker.acquire().await.unwrap();
        let (id, token) = running(&pool, LEASE).await;
        let watch = worker_lease::watch(Owner::Benchmark, &worker, id, token, Instant::now());
        tokio::pin!(watch);
        let original: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT lease_until FROM benchmark_runs WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let scenario = async {
            // The first 500 ms poll exhausts the worker pool's 200 ms timeout.
            tokio::time::sleep(Duration::from_millis(900)).await;
            drop(held);
            loop {
                let renewed: bool =
                    sqlx::query_scalar("SELECT lease_until>$2 FROM benchmark_runs WHERE id=$1")
                        .bind(id)
                        .bind(original)
                        .fetch_one(&pool)
                        .await
                        .unwrap();
                if renewed {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        tokio::select! {
            result = &mut watch => panic!("watch stopped before a successful renewal: {result:?}"),
            result = tokio::time::timeout(Duration::from_secs(5), scenario) => result.unwrap(),
        }
        sqlx::query("UPDATE benchmark_runs SET worker_token=$2 WHERE id=$1")
            .bind(id)
            .bind(Uuid::new_v4())
            .execute(&pool)
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), watch)
            .await
            .unwrap()
            .unwrap();

        // Even if measurement finishes before the watcher notices reassignment,
        // neither success nor failure belongs to the old worker.
        finish(&pool, id, token, Ok(100.0)).await.unwrap();
        finish(&pool, id, token, Err(anyhow::anyhow!("stale failure")))
            .await
            .unwrap();
        assert_still_running(&pool, id).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn watch_stops_at_expiry_even_while_waiting_for_a_connection(pool: PgPool) {
        let worker = single_connection(&pool, Duration::from_secs(10)).await;
        let held = worker.acquire().await.unwrap();
        let remaining = Duration::from_millis(800);
        let (id, token) = running(&pool, remaining).await;
        let renewed = Instant::now() - (LEASE - remaining);
        tokio::time::timeout(
            Duration::from_secs(3),
            worker_lease::watch(Owner::Benchmark, &worker, id, token, renewed),
        )
        .await
        .expect("waiting for a connection must not outlive the lease")
        .unwrap();
        assert!(renewed.elapsed() >= LEASE);
        drop(held);
        finish(&pool, id, token, Ok(100.0)).await.unwrap();
        finish(&pool, id, token, Err(anyhow::anyhow!("stale failure")))
            .await
            .unwrap();
        assert_still_running(&pool, id).await;
    }

    async fn assert_still_running(pool: &PgPool, id: Uuid) {
        let untouched: bool = sqlx::query_scalar(
            "SELECT status='running' AND finished_at IS NULL AND error IS NULL
                    AND total_elapsed_ms IS NULL AND average_move_ms IS NULL AND average_turn_ms IS NULL
             FROM benchmark_runs WHERE id=$1",
        )
        .bind(id).fetch_one(pool).await.unwrap();
        assert!(untouched, "a stale worker finalized its benchmark");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn renewal_and_finalization_behind_a_row_lock_recheck_expiry(pool: PgPool) {
        let (id, token) = running(&pool, Duration::from_secs(1)).await;
        let mut blocker = pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM benchmark_runs WHERE id=$1 FOR UPDATE")
            .bind(id)
            .execute(&mut *blocker)
            .await
            .unwrap();
        // Both statements reach PostgreSQL with a live lease but wait on the
        // same row until after the database deadline.
        let (finished, watched, ()) = tokio::join!(
            finish(&pool, id, token, Ok(100.0)),
            tokio::time::timeout(
                Duration::from_secs(4),
                worker_lease::watch(
                    Owner::Benchmark,
                    &pool,
                    id,
                    token,
                    Instant::now() - Duration::from_secs(1)
                ),
            ),
            async {
                tokio::time::sleep(Duration::from_millis(1100)).await;
                blocker.commit().await.unwrap();
            },
        );
        finished.unwrap();
        watched.expect("renewal revived an expired lease").unwrap();
        assert_still_running(&pool, id).await;
    }
}
