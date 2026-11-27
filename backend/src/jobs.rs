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

/// Lease that a claim, a progress update, a match checkpoint, and each
/// heartbeat grant a running job.
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

/// A reclaimed job may have two tasks briefly. Dropping either task removes
/// only its own registration, including when the worker aborts it.
struct RegisteredClaim<'a> {
    registry: &'a RunningJobs,
    claim: (Uuid, Uuid),
    stop: Stop,
}

impl Drop for RegisteredClaim<'_> {
    fn drop(&mut self) {
        self.registry.lock().jobs.remove(&self.claim);
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

    fn register(&self, id: Uuid, lease: Uuid) -> RegisteredClaim<'_> {
        let stop = Stop::default();
        let mut running = self.lock();
        if let Some(reason) = running.draining {
            stop.stop(reason);
        }
        let claim = (id, lease);
        running.jobs.insert(claim, stop.clone());
        RegisteredClaim {
            registry: self,
            claim,
            stop,
        }
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
    state.browse_wake.notify_one();
    Ok(Some((true, job)))
}

/// Runs queued benchmarks and up to the configured number of analysis jobs at
/// the same time. Each job runs in its own task and keeps its own lease,
/// heartbeat and checkpoints. Stopping the worker aborts running tasks; their
/// leases expire and the jobs are recovered like after a restart.
pub async fn worker(state: AppState, stop: CancellationToken) {
    let mut running = JoinSet::new();
    let mut benchmark = None;
    while !stop.is_cancelled() {
        if let Err(error) = fill(&state, &stop, &mut running, &mut benchmark).await {
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
    stop: &CancellationToken,
    running: &mut JoinSet<Result<bool>>,
    benchmark: &mut Option<tokio::task::Id>,
) -> Result<()> {
    if benchmark.is_some() {
        return Ok(());
    }
    let slots = crate::settings::load_analysis_job_slots(&state.pool).await?;
    if crate::benchmarks::needs_worker(&state.pool).await? {
        if !stop.is_cancelled()
            && running.len() < slots
            && let Some(access) = state
                .backup
                .try_background(crate::database_backup::BackgroundWork::Benchmark)
        {
            let state = state.clone();
            let stop = stop.clone();
            let task = running.spawn(async move {
                let _access = access;
                if stop.is_cancelled() {
                    return Ok(false);
                }
                crate::benchmarks::run_one(&state).await
            });
            *benchmark = Some(task.id());
        }
        return Ok(());
    }
    while !stop.is_cancelled() && running.len() < slots {
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
/// Claim the oldest queued job with lease token `$1` and lease seconds `$2`.
/// NeonGammon analyses share one sidecar process and run one at a time.
const CLAIM_NEXT: &str = "UPDATE jobs SET status='running',lease_token=$1,lease_until=now()+make_interval(secs=>$2),attempts=attempts+1,updated_at=now() WHERE id=(SELECT id FROM jobs WHERE status='queued' AND (coalesce(engine_snapshot->>'engine_id','gnubg')<>'bgsage' OR NOT EXISTS(SELECT 1 FROM jobs WHERE status='running' AND engine_snapshot->>'engine_id'='bgsage')) ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *";

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
        .bind(LEASE.as_secs_f64())
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
    let recovered = !expired.is_empty();
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
        .bind(LEASE.as_secs_f64())
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
    if recovered {
        state.browse_wake.notify_one();
    }
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
    if job.kind == "match" && changed.rows_affected() == 1 {
        state.browse_wake.notify_one();
    }
    Ok(true)
}

async fn execute_with_heartbeat(state: &AppState, job: &Job, token: Uuid) -> Result<Value> {
    let claim = state.running_jobs.register(job.id, token);
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
    tokio::select! {
        biased;
        () = claim.stop.token.cancelled() => Err(anyhow::anyhow!(
            claim.stop.reason.get().copied().unwrap_or(CANCELLED_ERROR)
        )),
        result = work => result,
    }
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
            match sqlx::query("UPDATE jobs SET lease_until=now()+make_interval(secs=>$3) WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now()")
                .bind(id)
                .bind(token)
                .bind(LEASE.as_secs_f64())
                .execute(pool)
                .await
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
pub(crate) fn transient(error: &sqlx::Error) -> bool {
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
    let result = sqlx::query("UPDATE jobs SET progress=$3,updated_at=now(),lease_until=now()+make_interval(secs=>$4) WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now()")
        .bind(id)
        .bind(token)
        .bind(progress)
        .bind(LEASE.as_secs_f64())
        .execute(&mut *tx)
        .await?;
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
#[path = "jobs_tests.rs"]
mod tests;
