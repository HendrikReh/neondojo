use crate::{
    api::AppState,
    engine::{DiceEvaluation, Distribution},
    model::Job,
    store,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Lease that a claim, a progress update and each heartbeat grant a running
/// job. The job SQL here and in `match_jobs.rs` spells it as
/// `interval '120 seconds'`; change them together.
pub const LEASE: Duration = Duration::from_secs(120);
/// Lease renewal interval of running analysis jobs unless the application
/// state sets another one with [`AppState::with_job_heartbeat`].
pub const DEFAULT_HEARTBEAT: Duration = Duration::from_secs(20);

/// Error of a job the user cancelled. Cancelling uses the existing `failed`
/// state, as a database restore or an advancing match already do when they
/// stop a job; the frontend recognizes this text and labels it as cancelled.
pub const CANCELLED_ERROR: &str = "Cancelled by user";

/// Error of a job that a database restore stopped or found unfinished.
pub const RESTORE_INTERRUPTED_ERROR: &str = "Interrupted by database restore";

/// Cancellation tokens of the jobs this process is running, so that cancelling
/// a running job stops its engine work at once instead of at the next lease
/// renewal. The database state stays authoritative: once a job is cancelled,
/// its lease token no longer matches and any later write is rejected.
#[derive(Default)]
pub(crate) struct RunningJobs(std::sync::Mutex<std::collections::HashMap<Uuid, Stop>>);

/// Stops one running job and records why; the first reason wins.
#[derive(Clone, Default)]
struct Stop {
    token: CancellationToken,
    reason: std::sync::Arc<std::sync::OnceLock<&'static str>>,
}

impl Stop {
    fn stop(&self, reason: &'static str) {
        let _ = self.reason.set(reason);
        self.token.cancel();
    }
}

impl RunningJobs {
    fn register(&self, id: Uuid) -> Stop {
        let stop = Stop::default();
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id, stop.clone());
        stop
    }

    fn remove(&self, id: Uuid) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&id);
    }

    fn cancel(&self, id: Uuid) {
        if let Some(token) = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&id)
        {
            token.stop(CANCELLED_ERROR);
        }
    }

    /// Stop every running job, for example before a database restore, and
    /// return how many were running.
    pub(crate) fn interrupt_all(&self, reason: &'static str) -> usize {
        let running = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for stop in running.values() {
            stop.stop(reason);
        }
        running.len()
    }
}

/// Cancel a queued or running job. Returns `None` when the job does not exist
/// and `Some(false)` when it has already finished. Results a match job saved
/// for completed games stay with the job, as after any other failure.
pub(crate) async fn cancel(state: &AppState, id: Uuid) -> Result<Option<(bool, Job)>> {
    let mut tx = state.pool.begin().await?;
    let cancelled = sqlx::query_as::<_, Job>("UPDATE jobs SET status='failed',error=$2,lease_token=NULL,lease_until=NULL,updated_at=now() WHERE id=$1 AND status IN ('queued','running') RETURNING *")
        .bind(id)
        .bind(CANCELLED_ERROR)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(job) = cancelled else {
        tx.rollback().await?;
        return Ok(store::get_job(&state.pool, id)
            .await?
            .map(|job| (false, job)));
    };
    sqlx::query("UPDATE match_analysis_timing SET active_started_at=NULL,active_elapsed_ms=0 WHERE job_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,'failed',$2)")
        .bind(id)
        .bind(json!({"status":"failed","progress":job.progress,"error":CANCELLED_ERROR,"completed_games":job.completed_games,"total_games":job.total_games}))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    state.running_jobs.cancel(id);
    state.wake.notify_one();
    Ok(Some((true, job)))
}

/// Runs queued benchmarks and up to the configured number of analysis jobs at
/// the same time. Each job runs in its own task and keeps its own lease,
/// heartbeat and checkpoints. Stopping the worker aborts running tasks; their
/// leases expire and the jobs are recovered like after a restart.
pub async fn worker(state: AppState, stop: CancellationToken) {
    let mut running = JoinSet::new();
    let mut benchmark = None;
    loop {
        if let Err(error) = fill(&state, &mut running, &mut benchmark).await {
            tracing::error!(%error, "Job worker error");
        }
        tokio::select! {
            _ = stop.cancelled() => break,
            Some(finished) = running.join_next_with_id(), if !running.is_empty() => {
                let id = match &finished {
                    Ok((id, _)) => *id,
                    Err(error) => error.id(),
                };
                if benchmark == Some(id) {
                    benchmark = None;
                }
                match finished {
                    Ok((_, Ok(true))) => continue,
                    Ok((_, Ok(false))) => {},
                    Ok((_, Err(error))) => tracing::error!(%error, "Job worker error"),
                    // A panicking job releases its slot; its lease expires.
                    Err(error) => tracing::error!(%error, "Job task failed"),
                }
            }
            _ = state.wake.notified() => {},
            _ = tokio::time::sleep(Duration::from_secs(1)) => {},
        }
    }
    running.shutdown().await;
}

/// Starts work until all slots are busy. A benchmark takes one slot and has
/// priority: while it is queued or running, no new analysis job starts.
async fn fill(
    state: &AppState,
    running: &mut JoinSet<Result<bool>>,
    benchmark: &mut Option<tokio::task::Id>,
) -> Result<()> {
    if benchmark.is_some() {
        return Ok(());
    }
    let slots = crate::settings::load_analysis_job_slots(&state.pool).await?;
    if crate::benchmarks::has_queued(&state.pool).await? {
        if running.len() < slots
            && let Ok(access) = state.backup.requests.clone().try_read_owned()
        {
            let state = state.clone();
            let task = running.spawn(async move {
                let _access = access;
                crate::benchmarks::run_one(&state).await
            });
            *benchmark = Some(task.id());
        }
        return Ok(());
    }
    while running.len() < slots {
        let Ok(access) = state.backup.requests.clone().try_read_owned() else {
            break;
        };
        let Some(claimed) = claim(state).await? else {
            break;
        };
        let state = state.clone();
        running.spawn(async move {
            let _access = access;
            run_claimed(&state, claimed).await
        });
    }
    Ok(())
}

/// Runs one queued benchmark or analysis job to completion.
pub async fn run_one(state: &AppState) -> Result<bool> {
    if crate::benchmarks::run_one(state).await? {
        return Ok(true);
    }
    let Some(claimed) = claim(state).await? else {
        return Ok(false);
    };
    run_claimed(state, claimed).await
}

struct Claimed {
    job: Job,
    token: Uuid,
}

async fn claim(state: &AppState) -> Result<Option<Claimed>> {
    let token = Uuid::new_v4();
    let mut tx = state.pool.begin().await?;
    // Serialize claims so concurrent claimers observe each other's running rows.
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(crate::advisory_locks::ANALYSIS_JOB_CLAIM)
        .execute(&mut *tx)
        .await?;
    let expired: Vec<Uuid> = sqlx::query_scalar("UPDATE jobs SET status=CASE WHEN attempts>=3 THEN 'failed' ELSE 'queued' END, error=CASE WHEN attempts>=3 THEN 'Worker lease expired after 3 attempts' ELSE NULL END, lease_token=NULL, lease_until=NULL, progress=CASE WHEN kind='match' THEN progress ELSE 0 END, updated_at=now() WHERE status='running' AND lease_until<now() RETURNING id")
        .fetch_all(&mut *tx).await?;
    for id in expired {
        sqlx::query("UPDATE match_analysis_timing SET active_started_at=NULL,active_elapsed_ms=0 WHERE job_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO job_events(job_id,event,data) SELECT id,status,jsonb_build_object('status',status,'progress',progress,'error',error,'completed_games',completed_games,'total_games',total_games) FROM jobs WHERE id=$1")
            .bind(id).execute(&mut *tx).await?;
    }
    // NeonGammon analyses share one sidecar process; run them one at a time
    // and let GNUbg jobs use the remaining slots.
    let job = sqlx::query_as::<_, Job>("UPDATE jobs SET status='running',lease_token=$1,lease_until=now()+interval '120 seconds',attempts=attempts+1,updated_at=now() WHERE id=(SELECT id FROM jobs WHERE status='queued' AND (coalesce(engine_snapshot->>'engine_id','gnubg')<>'bgsage' OR NOT EXISTS(SELECT 1 FROM jobs WHERE status='running' AND engine_snapshot->>'engine_id'='bgsage')) ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *")
        .bind(token).fetch_optional(&mut *tx).await?;
    if let Some(ref job) = job {
        sqlx::query("UPDATE match_analysis_timing SET active_started_at=NULL,active_elapsed_ms=0 WHERE job_id=$1")
            .bind(job.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,'running',$2)")
            .bind(job.id)
            .bind(json!({"status":"running","progress":job.progress,"attempt":job.attempts,"completed_games":job.completed_games,"total_games":job.total_games}))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(job.map(|job| Claimed { job, token }))
}

async fn run_claimed(state: &AppState, claimed: Claimed) -> Result<bool> {
    let Claimed { job, token } = claimed;
    let result = execute_with_heartbeat(state, &job, token).await;
    let (status, data, error) = match result {
        Ok(data) => ("completed", Some(data), None),
        Err(error) => ("failed", None, Some(format!("{error:#}"))),
    };
    let mut tx = state.pool.begin().await?;
    let changed = sqlx::query("UPDATE jobs SET status=$3,result=COALESCE($4,result),error=$5,progress=CASE WHEN $3='completed' THEN 100 ELSE progress END,lease_token=NULL,lease_until=NULL,updated_at=now() WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now()")
        .bind(job.id).bind(token).bind(status).bind(&data).bind(&error).execute(&mut *tx).await?;
    if changed.rows_affected() == 1 {
        sqlx::query("UPDATE match_analysis_timing SET active_started_at=NULL,active_elapsed_ms=0 WHERE job_id=$1 AND lease_token=$2").bind(job.id).bind(token).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO job_events(job_id,event,data) SELECT id,status,jsonb_build_object('status',status,'progress',progress,'error',error,'completed_games',completed_games,'total_games',total_games) FROM jobs WHERE id=$1")
            .bind(job.id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(true)
}

async fn execute_with_heartbeat(state: &AppState, job: &Job, token: Uuid) -> Result<Value> {
    let cancelled = state.running_jobs.register(job.id);
    // The job future is large; keep it on the heap rather than in this frame.
    let work = Box::pin(with_heartbeat(
        &state.pool,
        job.id,
        token,
        execute(state, job, token),
        state.job_heartbeat,
    ));
    // Dropping the work on cancellation terminates its engine processes.
    let result = tokio::select! {
        result = work => result,
        () = cancelled.token.cancelled() => Err(anyhow::anyhow!(
            cancelled.reason.get().copied().unwrap_or(CANCELLED_ERROR)
        )),
    };
    state.running_jobs.remove(job.id);
    result
}

async fn with_heartbeat(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    execution: impl std::future::Future<Output = Result<Value>>,
    interval: Duration,
) -> Result<Value> {
    // Keep polling the work while renewal waits for its row lock. A checkpoint
    // can own that lock until its transaction commits; awaiting renewal inside
    // an execution-select branch would stop polling the commit and deadlock.
    let heartbeat = async {
        loop {
            tokio::time::sleep(interval).await;
            let updated = sqlx::query("UPDATE jobs SET lease_until=now()+interval '120 seconds' WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now()")
                .bind(id).bind(token).execute(pool).await?;
            ensure!(updated.rows_affected() == 1, "Job lease was lost");
        }
    };
    tokio::select! {
        result = execution => result,
        result = heartbeat => result,
    }
}

async fn execute(state: &AppState, job: &Job, token: Uuid) -> Result<Value> {
    let configured_engine = store::job_engine(&state.pool, job.id, &state.engine).await?;
    if job.kind == "match" {
        return crate::match_jobs::execute(state, job, token, &configured_engine).await;
    }
    let position = store::get_position(
        &state.pool,
        job.position_uuid
            .context("Position job has no position target")?,
    )
    .await?
    .context("Position no longer exists")?;
    if job.kind == "analysis" {
        return Ok(serde_json::to_value(
            configured_engine
                .analyze(
                    &position.position_id,
                    &position.gnubg_match_id,
                    job.plies,
                    None,
                )
                .await?,
        )?);
    }
    // 21 unordered pairs; doubles have weight 1 and other pairs weight 2.
    let dice: Vec<[i32; 2]> = (1..=6).flat_map(|a| (a..=6).map(move |b| [a, b])).collect();
    let total = dice.len();
    let analyses = configured_engine
        .analyze_rolls(
            &position.position_id,
            &position.gnubg_match_id,
            job.plies,
            &dice,
            |evaluated| progress(&state.pool, job.id, token, (evaluated * 100 / total) as i32),
        )
        .await?;
    let mut engine = String::new();
    let mut rolls = Vec::with_capacity(dice.len());
    for ([a, b], analysis) in dice.iter().copied().zip(analyses) {
        engine = analysis.engine;
        let best = analysis
            .candidates
            .into_iter()
            .next()
            .context("No evaluated candidate")?;
        let weight = if a == b { 1 } else { 2 };
        rolls.push(DiceEvaluation {
            dice: [a, b],
            weight,
            probability: f64::from(weight) / 36.0,
            best,
        });
    }
    ensure!(rolls.len() == 21, "Dice equity needs all 21 rolls");
    let mean_equity = rolls.iter().map(|r| r.best.equity * r.probability).sum();
    Ok(serde_json::to_value(Distribution {
        engine,
        plies: job.plies,
        mean_equity,
        rolls,
    })?)
}

async fn progress(pool: &PgPool, id: Uuid, token: Uuid, progress: i32) -> Result<()> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query("UPDATE jobs SET progress=$3,updated_at=now(),lease_until=now()+interval '120 seconds' WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now()")
        .bind(id).bind(token).bind(progress).execute(&mut *tx).await?;
    ensure!(result.rows_affected() == 1, "Job lease was lost");
    sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,'progress',$2)")
        .bind(id)
        .bind(json!({"status":"running","progress":progress}))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn heartbeat_interval_defaults_and_must_fit_into_half_the_lease() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .unwrap();
        let state = || AppState::new(pool.clone(), crate::engine::Engine::new("/unused".into()));
        assert_eq!(state().job_heartbeat, DEFAULT_HEARTBEAT);
        assert_eq!(
            state().with_job_heartbeat(LEASE / 2).job_heartbeat,
            LEASE / 2
        );
        for invalid in [Duration::ZERO, LEASE / 2 + Duration::from_millis(1)] {
            let state = state();
            let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                state.with_job_heartbeat(invalid)
            }));
            assert!(rejected.is_err(), "{invalid:?}");
        }
    }

    async fn queued_position_job(pool: &PgPool) -> Uuid {
        let sgf = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][game:0][ws:0][bs:0];W[31hefe])";
        let imported = store::import_match(
            pool,
            "cancel.sgf",
            "sgf",
            sgf.as_bytes(),
            sgf,
            &crate::import::parse_sgf(sgf).unwrap(),
        )
        .await
        .unwrap();
        let position: Uuid =
            sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 LIMIT 1")
                .bind(imported.r#match.id)
                .fetch_one(pool)
                .await
                .unwrap();
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status) VALUES($1,$2,'analysis',1,'queued')")
            .bind(id)
            .bind(position)
            .execute(pool)
            .await
            .unwrap();
        id
    }

    async fn status(pool: &PgPool, id: Uuid) -> (String, Option<String>, Option<Uuid>) {
        sqlx::query_as("SELECT status,error,lease_token FROM jobs WHERE id=$1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn cancelling_a_queued_job_records_it_once(pool: PgPool) {
        let state = AppState::new(pool.clone(), crate::engine::Engine::new("/unused".into()));
        let id = queued_position_job(&pool).await;
        let (changed, job) = cancel(&state, id).await.unwrap().unwrap();
        assert!(changed);
        assert_eq!(job.status, "failed");
        assert_eq!(
            status(&pool, id).await,
            ("failed".into(), Some(CANCELLED_ERROR.into()), None)
        );
        let events: Vec<(String, Value)> =
            sqlx::query_as("SELECT event,data FROM job_events WHERE job_id=$1")
                .bind(id)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].0, "failed");
        assert_eq!(events[0].1["error"], CANCELLED_ERROR);
        // A finished job is reported, not changed; an unknown job is missing.
        let (changed, _) = cancel(&state, id).await.unwrap().unwrap();
        assert!(!changed);
        assert!(cancel(&state, Uuid::new_v4()).await.unwrap().is_none());
        // The worker no longer finds it.
        assert!(claim(&state).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn cancelling_a_running_job_stops_its_engine_process(pool: PgPool) {
        let engine = SleepingEngine::new();
        let state = AppState::new(pool.clone(), engine.engine());
        let id = queued_position_job(&pool).await;
        let worker = tokio::spawn({
            let state = state.clone();
            async move { run_one(&state).await }
        });
        let pid = engine.pid().await;
        assert_eq!(status(&pool, id).await.0, "running");
        let (changed, _) = cancel(&state, id).await.unwrap().unwrap();
        assert!(changed);
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .expect("a cancelled job must stop without waiting for its lease")
            .unwrap()
            .unwrap();
        assert_eq!(
            status(&pool, id).await,
            ("failed".into(), Some(CANCELLED_ERROR.into()), None)
        );
        // kill_on_drop terminates the engine process with the cancelled work.
        terminated(pid).await;
        assert!(state.running_jobs.0.lock().unwrap().is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn interrupting_running_jobs_records_the_restore_and_stops_the_engine(pool: PgPool) {
        let engine = SleepingEngine::new();
        let state = AppState::new(pool.clone(), engine.engine());
        let id = queued_position_job(&pool).await;
        let worker = tokio::spawn({
            let state = state.clone();
            async move { run_one(&state).await }
        });
        let pid = engine.pid().await;
        assert_eq!(
            state.running_jobs.interrupt_all(RESTORE_INTERRUPTED_ERROR),
            1
        );
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .expect("an interrupted job must stop at once")
            .unwrap()
            .unwrap();
        // The worker records the interruption itself while its lease is valid.
        assert_eq!(
            status(&pool, id).await,
            (
                "failed".into(),
                Some(RESTORE_INTERRUPTED_ERROR.into()),
                None
            )
        );
        terminated(pid).await;
        assert_eq!(
            state.running_jobs.interrupt_all(RESTORE_INTERRUPTED_ERROR),
            0
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn restore_stops_a_running_analysis_instead_of_reporting_busy(pool: PgPool) {
        use axum::{
            body::{Body, to_bytes},
            http::{Request, StatusCode, header},
        };
        use tower::ServiceExt;
        let engine = SleepingEngine::new();
        let state = AppState::new(pool.clone(), engine.engine());
        let app = crate::api::router(state.clone());
        let id = queued_position_job(&pool).await;
        let archive = app
            .clone()
            .oneshot(
                Request::get("/api/database-backup")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(archive.status(), StatusCode::OK);
        let archive = to_bytes(archive.into_body(), 16 * 1024 * 1024)
            .await
            .unwrap();
        // The worker holds the database for the whole job, as in production.
        let stop = CancellationToken::new();
        let jobs = tokio::spawn(worker(state.clone(), stop.clone()));
        let pid = engine.pid().await;
        assert_eq!(status(&pool, id).await.0, "running");
        let started = std::time::Instant::now();
        let response = app
            .oneshot(
                Request::post("/api/database-backup?replace=true")
                    .header(header::CONTENT_TYPE, "application/vnd.neondojo.backup")
                    .body(Body::from(archive))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(started.elapsed() < Duration::from_secs(15));
        terminated(pid).await;
        // The archive held the job as queued; the restore ends it as interrupted.
        assert_eq!(
            status(&pool, id).await,
            (
                "failed".into(),
                Some(RESTORE_INTERRUPTED_ERROR.into()),
                None
            )
        );
        stop.cancel();
        jobs.await.unwrap();
    }

    /// A stand-in GNUbg that records its process id and never answers.
    struct SleepingEngine {
        directory: tempfile::TempDir,
    }

    impl SleepingEngine {
        fn new() -> Self {
            use std::os::unix::fs::PermissionsExt;
            let directory = tempfile::tempdir().unwrap();
            let engine = directory.path().join("gnubg");
            std::fs::write(
                &engine,
                format!(
                    "#!/bin/sh\necho $$ > '{}'\nexec sleep 60\n",
                    directory.path().join("engine.pid").display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o755)).unwrap();
            Self { directory }
        }

        fn engine(&self) -> crate::engine::Engine {
            crate::engine::Engine::new(self.directory.path().join("gnubg").display().to_string())
        }

        async fn pid(&self) -> i32 {
            let file = self.directory.path().join("engine.pid");
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if let Ok(pid) = std::fs::read_to_string(&file)
                        && let Ok(pid) = pid.trim().parse::<i32>()
                    {
                        return pid;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("the stand-in engine must start")
        }
    }

    async fn terminated(pid: i32) {
        let alive = || {
            std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .status()
                .unwrap()
                .success()
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            while alive() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the engine process must be terminated");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn claims_run_one_open_sage_job_at_a_time(pool: PgPool) {
        let sgf = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][game:0][ws:0][bs:0];W[31hefe])";
        let imported = store::import_match(
            &pool,
            "claims.sgf",
            "sgf",
            sgf.as_bytes(),
            sgf,
            &crate::import::parse_sgf(sgf).unwrap(),
        )
        .await
        .unwrap();
        let position: Uuid =
            sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 LIMIT 1")
                .bind(imported.r#match.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        // Two NeonGammon jobs are queued before one GNUbg job.
        for (offset, plies, engine) in [(3, 1, "bgsage"), (2, 2, "bgsage"), (1, 3, "gnubg")] {
            sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,engine_snapshot,created_at) VALUES($1,$2,'analysis',$3,'queued',jsonb_build_object('engine_id',$4::text,'gnubg_path','gnubg'),now()-make_interval(secs=>$5))")
                .bind(Uuid::new_v4())
                .bind(position)
                .bind(plies)
                .bind(engine)
                .bind(f64::from(offset))
                .execute(&pool)
                .await
                .unwrap();
        }
        let state = AppState::new(pool.clone(), crate::engine::Engine::new("gnubg".into()));
        let first = claim(&state).await.unwrap().expect("first NeonGammon job");
        assert_eq!(first.job.plies, 1);
        let second = claim(&state)
            .await
            .unwrap()
            .expect("GNUbg job passes the waiting Sage job");
        assert_eq!(second.job.plies, 3);
        assert!(claim(&state).await.unwrap().is_none());
        sqlx::query(
            "UPDATE jobs SET status='completed',lease_token=NULL,lease_until=NULL WHERE id=$1",
        )
        .bind(first.job.id)
        .execute(&pool)
        .await
        .unwrap();
        let third = claim(&state).await.unwrap().expect("second NeonGammon job");
        assert_eq!(third.job.plies, 2);
    }
    #[sqlx::test(migrations = "./migrations")]
    async fn heartbeat_does_not_deadlock_a_checkpoint_holding_the_job_lock(pool: PgPool) {
        let sgf = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][game:0][ws:0][bs:0];W[31hefe])";
        let imported = store::import_match(
            &pool,
            "lock.sgf",
            "sgf",
            sgf.as_bytes(),
            sgf,
            &crate::import::parse_sgf(sgf).unwrap(),
        )
        .await
        .unwrap();
        let job = store::create_match_job(&pool, imported.r#match.id, 1, 1)
            .await
            .unwrap();
        let token = Uuid::new_v4();
        sqlx::query("UPDATE jobs SET status='running',lease_token=$2,lease_until=now()+interval '120 seconds' WHERE id=$1").bind(job.id).bind(token).execute(&pool).await.unwrap();
        let mut checkpoint = pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM jobs WHERE id=$1 FOR UPDATE")
            .bind(job.id)
            .execute(&mut *checkpoint)
            .await
            .unwrap();
        let execution = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            checkpoint.commit().await?;
            Ok(json!({"checkpoint":"committed"}))
        };
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            with_heartbeat(&pool, job.id, token, execution, Duration::from_millis(10)),
        )
        .await
        .expect("heartbeat must not stop polling the checkpoint while waiting for its row lock")
        .unwrap();
        assert_eq!(result["checkpoint"], "committed");
    }
}
