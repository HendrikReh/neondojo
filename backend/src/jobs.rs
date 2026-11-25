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
pub(crate) struct RunningJobs(std::sync::Mutex<Running>);

#[derive(Default)]
struct Running {
    jobs: std::collections::HashMap<(Uuid, Uuid), Stop>,
    /// Set while a database restore drains running work. A job that a worker
    /// claimed before the restore began but registers only now is stopped at
    /// once with this reason instead of slipping past the drain.
    draining: Option<&'static str>,
}

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

/// Keeps [`RunningJobs`] draining until it is dropped, when the restore that
/// created it has finished or failed.
pub(crate) struct Drain<'a> {
    jobs: &'a RunningJobs,
    /// How many running jobs the drain stopped when it began.
    pub(crate) interrupted: usize,
}

impl Drop for Drain<'_> {
    fn drop(&mut self) {
        self.jobs.lock().draining = None;
    }
}

impl RunningJobs {
    fn lock(&self) -> std::sync::MutexGuard<'_, Running> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn register(&self, id: Uuid, lease: Uuid) -> Stop {
        let stop = Stop::default();
        let mut running = self.lock();
        if let Some(reason) = running.draining {
            stop.stop(reason);
        }
        running.jobs.insert((id, lease), stop.clone());
        stop
    }

    fn remove(&self, id: Uuid, lease: Uuid) {
        self.lock().jobs.remove(&(id, lease));
    }

    fn cancel(&self, id: Uuid) {
        for ((job, _), stop) in &self.lock().jobs {
            if *job == id {
                stop.stop(CANCELLED_ERROR);
            }
        }
    }

    /// Stop every running job, for example before a database restore, and
    /// keep stopping jobs that register until the returned guard is dropped.
    pub(crate) fn drain(&self, reason: &'static str) -> Drain<'_> {
        let mut running = self.lock();
        running.draining = Some(reason);
        for stop in running.jobs.values() {
            stop.stop(reason);
        }
        Drain {
            jobs: self,
            interrupted: running.jobs.len(),
        }
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
            && let Some(access) = state
                .backup
                .try_background(crate::database_backup::BackgroundWork::Benchmark)
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
        // A restore drains these jobs: one claimed while it starts is stopped
        // when it registers.
        let Some(access) = state.backup.try_job() else {
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

/// Requeue or fail the jobs whose worker lease expired.
const RECOVER_EXPIRED: &str = "UPDATE jobs SET status=CASE WHEN attempts>=3 THEN 'failed' ELSE 'queued' END, error=CASE WHEN attempts>=3 THEN 'Worker lease expired after 3 attempts' ELSE NULL END, lease_token=NULL, lease_until=NULL, progress=CASE WHEN kind='match' THEN progress ELSE 0 END, updated_at=now() WHERE status='running' AND lease_until<now() RETURNING id";
/// Claim the oldest queued job with lease token `$1`. NeonGammon analyses
/// share one sidecar process and run one at a time.
const CLAIM_NEXT: &str = "UPDATE jobs SET status='running',lease_token=$1,lease_until=now()+interval '120 seconds',attempts=attempts+1,updated_at=now() WHERE id=(SELECT id FROM jobs WHERE status='queued' AND (coalesce(engine_snapshot->>'engine_id','gnubg')<>'bgsage' OR NOT EXISTS(SELECT 1 FROM jobs WHERE status='running' AND engine_snapshot->>'engine_id'='bgsage')) ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *";

/// Support for the ignored large-library performance test: the plans of lease
/// recovery and of the claim, executed in a transaction that is rolled back.
#[doc(hidden)]
pub async fn explain_claim(pool: &PgPool) -> Result<Vec<(&'static str, Value)>> {
    const EXPLAIN: &str = "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) ";
    let mut tx = pool.begin().await?;
    let recover = sqlx::query_scalar(&format!("{EXPLAIN}{RECOVER_EXPIRED}"))
        .fetch_one(&mut *tx)
        .await?;
    let claim = sqlx::query_scalar(&format!("{EXPLAIN}{CLAIM_NEXT}"))
        .bind(Uuid::new_v4())
        .fetch_one(&mut *tx)
        .await?;
    tx.rollback().await?;
    Ok(vec![("recover", recover), ("claim", claim)])
}

async fn claim(state: &AppState) -> Result<Option<Claimed>> {
    let token = Uuid::new_v4();
    let mut tx = state.pool.begin().await?;
    // Serialize claims so concurrent claimers observe each other's running rows.
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(crate::advisory_locks::ANALYSIS_JOB_CLAIM)
        .execute(&mut *tx)
        .await?;
    let expired: Vec<Uuid> = sqlx::query_scalar(RECOVER_EXPIRED)
        .fetch_all(&mut *tx)
        .await?;
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
    let job = sqlx::query_as::<_, Job>(CLAIM_NEXT)
        .bind(token)
        .fetch_optional(&mut *tx)
        .await?;
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
    let cancelled = state.running_jobs.register(job.id, token);
    // The job future is large; keep it on the heap rather than in this frame.
    let work = Box::pin(with_heartbeat(
        &state.pool,
        job.id,
        token,
        execute(state, job, token),
        state.job_heartbeat,
    ));
    // Dropping the work on cancellation terminates its engine processes. A
    // job stopped before it registered never starts its work.
    let result = tokio::select! {
        biased;
        () = cancelled.token.cancelled() => Err(anyhow::anyhow!(
            cancelled.reason.get().copied().unwrap_or(CANCELLED_ERROR)
        )),
        result = work => result,
    };
    state.running_jobs.remove(job.id, token);
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
        // The claim granted the lease just before; each renewal extends it by
        // LEASE from a time no earlier than when it was sent.
        let mut renewed = std::time::Instant::now();
        loop {
            tokio::time::sleep(interval).await;
            let attempt = std::time::Instant::now();
            match sqlx::query("UPDATE jobs SET lease_until=now()+interval '120 seconds' WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now()")
                .bind(id).bind(token).execute(pool).await
            {
                Ok(updated) => {
                    ensure!(updated.rows_affected() == 1, "Job lease was lost");
                    renewed = attempt;
                }
                // A busy pool or a dropped connection says nothing about the
                // lease. The database still fences every write by its token.
                Err(error)
                    if transient(&error) && may_retry_renewal(renewed.elapsed(), interval) =>
                {
                    tracing::warn!(job = %id, %error, "Retrying the job lease renewal");
                }
                Err(error) => return Err(error.into()),
            }
        }
    };
    tokio::select! {
        result = execution => result,
        result = heartbeat => result,
    }
}

/// Database errors after which the same query may succeed again.
fn transient(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::PoolTimedOut | sqlx::Error::Io(_))
}

/// Whether a failed renewal may wait for another attempt: that attempt, one
/// interval later, must still fall within the lease of the last renewal.
fn may_retry_renewal(since_renewal: Duration, interval: Duration) -> bool {
    since_renewal + interval < LEASE
}

async fn execute(state: &AppState, job: &Job, token: Uuid) -> Result<Value> {
    let configured_engine =
        store::claimed_job_engine(&state.pool, job.id, token, &state.engine).await?;
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
    let cube_enabled = store::game_cube_enabled(&state.pool, &position).await?;
    let configured_engine =
        configured_engine.for_checker_rules(cube_enabled, &position.gnubg_match_id)?;
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
    async fn cancelling_between_claim_and_registration_never_starts_the_engine(pool: PgPool) {
        let engine = SleepingEngine::new();
        let state = AppState::new(pool.clone(), engine.engine());
        let id = queued_position_job(&pool).await;
        let claimed = claim(&state).await.unwrap().unwrap();
        assert_eq!(claimed.job.id, id);

        let (release, gate) = tokio::sync::oneshot::channel();
        let worker = tokio::spawn({
            let state = state.clone();
            async move {
                gate.await.unwrap();
                run_claimed(&state, claimed).await
            }
        });
        assert!(cancel(&state, id).await.unwrap().unwrap().0);
        release.send(()).unwrap();
        tokio::select! {
            _ = engine.pid() => panic!("the engine started after cancellation"),
            result = worker => assert!(result.unwrap().unwrap()),
        }
        assert!(!engine.directory.path().join("engine.pid").exists());
        assert_eq!(event_names(&pool, id).await, ["running", "failed"]);
        assert_eq!(
            status(&pool, id).await,
            ("failed".into(), Some(CANCELLED_ERROR.into()), None)
        );
        assert!(state.running_jobs.lock().jobs.is_empty());
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
        assert!(state.running_jobs.lock().jobs.is_empty());
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
            state
                .running_jobs
                .drain(RESTORE_INTERRUPTED_ERROR)
                .interrupted,
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
            state
                .running_jobs
                .drain(RESTORE_INTERRUPTED_ERROR)
                .interrupted,
            0
        );
    }

    #[test]
    fn an_expired_claim_cannot_unregister_its_successor() {
        let jobs = RunningJobs::default();
        let id = Uuid::new_v4();
        let expired = Uuid::new_v4();
        let successor = jobs.register(id, Uuid::new_v4());
        // The earlier worker starts late, discovers its lease has expired and exits.
        jobs.register(id, expired);
        jobs.remove(id, expired);
        jobs.cancel(id);
        assert!(successor.token.is_cancelled());
    }

    #[test]
    fn a_job_registering_during_a_drain_is_stopped_until_the_drain_ends() {
        let jobs = RunningJobs::default();
        let drain = jobs.drain(RESTORE_INTERRUPTED_ERROR);
        assert_eq!(drain.interrupted, 0);
        let late = jobs.register(Uuid::new_v4(), Uuid::new_v4());
        assert!(late.token.is_cancelled());
        assert_eq!(late.reason.get(), Some(&RESTORE_INTERRUPTED_ERROR));
        drop(drain);
        let after = jobs.register(Uuid::new_v4(), Uuid::new_v4());
        assert!(!after.token.is_cancelled());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn restore_stops_a_job_claimed_while_it_starts_draining(pool: PgPool) {
        use axum::{
            body::{Body, to_bytes},
            http::{Request, StatusCode, header},
        };
        use tower::ServiceExt;
        let engine = SleepingEngine::new();
        let state = AppState::new(pool.clone(), engine.engine())
            .with_restore_waits(Duration::from_millis(200), Duration::from_secs(10));
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
        // As in `fill`: the worker holds database access but has not claimed
        // or registered its job when the restore stops the running jobs.
        let access = state.backup.try_job().expect("database access");
        let restore = tokio::spawn({
            let app = app.clone();
            async move {
                app.oneshot(
                    Request::post("/api/database-backup?replace=true")
                        .header(header::CONTENT_TYPE, "application/vnd.neondojo.backup")
                        .body(Body::from(archive))
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
            }
        });
        tokio::time::timeout(Duration::from_secs(10), async {
            while state.running_jobs.lock().draining.is_none() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the restore must start draining");
        let claimed = claim(&state).await.unwrap().expect("queued job");
        assert_eq!(claimed.job.id, id);
        let started = std::time::Instant::now();
        let job = tokio::spawn({
            let state = state.clone();
            async move {
                let _access = access;
                run_claimed(&state, claimed).await
            }
        });
        assert!(
            tokio::time::timeout(Duration::from_secs(5), job)
                .await
                .expect("a job registering during the drain must stop at once")
                .unwrap()
                .unwrap()
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(10), restore)
                .await
                .expect("the restore must proceed once the job stopped")
                .unwrap(),
            StatusCode::NO_CONTENT
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(
            status(&pool, id).await,
            (
                "failed".into(),
                Some(RESTORE_INTERRUPTED_ERROR.into()),
                None
            )
        );
        // The stopped job never started its engine.
        assert!(!engine.directory.path().join("engine.pid").exists());
        assert!(state.running_jobs.lock().draining.is_none());
        assert!(state.backup.try_job().is_some());
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

    #[test]
    fn renewals_are_retried_only_while_the_next_attempt_fits_into_the_lease() {
        let interval = DEFAULT_HEARTBEAT;
        assert!(may_retry_renewal(Duration::ZERO, interval));
        assert!(may_retry_renewal(
            LEASE - interval - Duration::from_millis(1),
            interval
        ));
        assert!(!may_retry_renewal(LEASE - interval, interval));
        assert!(!may_retry_renewal(LEASE, interval));
        assert!(transient(&sqlx::Error::PoolTimedOut));
        assert!(transient(&sqlx::Error::Io(
            std::io::ErrorKind::BrokenPipe.into()
        )));
        assert!(!transient(&sqlx::Error::RowNotFound));
        assert!(!transient(&sqlx::Error::PoolClosed));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn heartbeat_retries_a_busy_pool_but_stops_after_lease_loss(pool: PgPool) {
        let id = queued_position_job(&pool).await;
        let token = Uuid::new_v4();
        sqlx::query("UPDATE jobs SET status='running',lease_token=$2,lease_until=now()+interval '120 seconds' WHERE id=$1").bind(id).bind(token).execute(&pool).await.unwrap();
        // Every renewal times out waiting for the only connection.
        let busy = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_millis(250))
            .test_before_acquire(false)
            .connect_with((*pool.connect_options()).clone())
            .await
            .unwrap();
        let held = busy.acquire().await.unwrap();
        let execution = async {
            tokio::time::sleep(Duration::from_millis(700)).await;
            Ok(json!({"engine":"finished"}))
        };
        let result = with_heartbeat(&busy, id, token, execution, Duration::from_millis(10))
            .await
            .expect("a busy pool must not end healthy engine work");
        assert_eq!(result["engine"], "finished");
        drop(held);
        // A replaced worker owns the job; the heartbeat stops this one.
        sqlx::query("UPDATE jobs SET lease_token=$2 WHERE id=$1")
            .bind(id)
            .bind(Uuid::new_v4())
            .execute(&pool)
            .await
            .unwrap();
        let error = tokio::time::timeout(
            Duration::from_secs(5),
            with_heartbeat(
                &busy,
                id,
                token,
                std::future::pending(),
                Duration::from_millis(10),
            ),
        )
        .await
        .expect("lease loss must stop the job")
        .unwrap_err();
        assert_eq!(error.to_string(), "Job lease was lost");
        busy.close().await;
    }

    async fn event_names(pool: &PgPool, id: Uuid) -> Vec<String> {
        sqlx::query_scalar("SELECT event FROM job_events WHERE job_id=$1 ORDER BY id")
            .bind(id)
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn finishing_a_job_prunes_only_its_progress_events(pool: PgPool) {
        let state = AppState::new(pool.clone(), crate::engine::Engine::new("/unused".into()));
        let cancelled = queued_position_job(&pool).await;
        let expiring = Uuid::new_v4();
        sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,created_at) SELECT $1,position_uuid,'analysis',2,'queued',created_at+interval '1 second' FROM jobs WHERE id=$2")
            .bind(expiring)
            .bind(cancelled)
            .execute(&pool)
            .await
            .unwrap();
        let first = claim(&state).await.unwrap().unwrap();
        let second = claim(&state).await.unwrap().unwrap();
        assert_eq!((first.job.id, second.job.id), (cancelled, expiring));
        for claimed in [&first, &second] {
            for percent in [25, 50] {
                progress(&pool, claimed.job.id, claimed.token, percent)
                    .await
                    .unwrap();
            }
        }
        // A finished job keeps its lifecycle events; a running job keeps everything.
        cancel(&state, cancelled).await.unwrap().unwrap();
        assert_eq!(event_names(&pool, cancelled).await, ["running", "failed"]);
        assert_eq!(
            event_names(&pool, expiring).await,
            ["running", "progress", "progress"]
        );
        // A lease that expires before the last attempt requeues the job with
        // its progress; the worker claims it again at once.
        sqlx::query("UPDATE jobs SET lease_until=now()-interval '1 second' WHERE id=$1")
            .bind(expiring)
            .execute(&pool)
            .await
            .unwrap();
        let again = claim(&state).await.unwrap().unwrap();
        assert_eq!(again.job.id, expiring);
        assert_eq!(
            event_names(&pool, expiring).await,
            ["running", "progress", "progress", "queued", "running"]
        );
        // The third expired lease fails the job and prunes its progress.
        sqlx::query("UPDATE jobs SET attempts=3,lease_until=now()-interval '1 second' WHERE id=$1")
            .bind(expiring)
            .execute(&pool)
            .await
            .unwrap();
        assert!(claim(&state).await.unwrap().is_none());
        assert_eq!(status(&pool, expiring).await.0, "failed");
        assert_eq!(
            event_names(&pool, expiring).await,
            ["running", "queued", "running", "failed"]
        );
    }
}
