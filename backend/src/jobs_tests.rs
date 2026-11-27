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
    let position: Uuid = sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 LIMIT 1")
        .bind(imported.r#match.id)
        .fetch_one(pool)
        .await
        .unwrap();
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO jobs(id,position_uuid,kind,plies,status) VALUES($1,$2,'analysis',1,'queued')",
    )
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
async fn stopped_worker_leaves_queued_jobs_unclaimed(pool: PgPool) {
    let state = AppState::new(pool.clone(), crate::engine::Engine::new("/unused".into()));
    let id = queued_position_job(&pool).await;
    state.shutdown.cancel();
    worker(state.clone(), state.shutdown.clone()).await;

    let job = store::get_job(&pool, id).await.unwrap().unwrap();
    assert_eq!(job.status, "queued");
    assert_eq!(job.attempts, 0);
    assert!(event_names(&pool, id).await.is_empty());
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
    drop(jobs.register(id, expired));
    jobs.cancel(id);
    assert!(successor.stop.token.is_cancelled());
}

#[test]
fn a_job_registering_during_a_drain_is_stopped_until_the_drain_ends() {
    let jobs = RunningJobs::default();
    let drain = jobs.drain(RESTORE_INTERRUPTED_ERROR);
    assert_eq!(drain.interrupted, 0);
    let late = jobs.register(Uuid::new_v4(), Uuid::new_v4());
    assert!(late.stop.token.is_cancelled());
    assert_eq!(late.stop.reason.get(), Some(&RESTORE_INTERRUPTED_ERROR));
    drop(drain);
    let after = jobs.register(Uuid::new_v4(), Uuid::new_v4());
    assert!(!after.stop.token.is_cancelled());
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
    let position: Uuid = sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 LIMIT 1")
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
    sqlx::query("UPDATE jobs SET status='completed',lease_token=NULL,lease_until=NULL WHERE id=$1")
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
    sqlx::query("UPDATE jobs SET status='running',lease_token=$2,lease_until=now()+make_interval(secs=>$3) WHERE id=$1")
        .bind(job.id)
        .bind(token)
        .bind(LEASE.as_secs_f64())
        .execute(&pool)
        .await
        .unwrap();
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
    sqlx::query("UPDATE jobs SET status='running',lease_token=$2,lease_until=now()+make_interval(secs=>$3) WHERE id=$1")
        .bind(id)
        .bind(token)
        .bind(LEASE.as_secs_f64())
        .execute(&pool)
        .await
        .unwrap();
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
