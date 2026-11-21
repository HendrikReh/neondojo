use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{
    api,
    engine::Engine,
    jobs,
    model::{ImportResponse, Job, PositionPage},
};
use sqlx::PgPool;
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

async fn json_call(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: serde_json::Value,
) -> (u16, serde_json::Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let data = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&data).unwrap())
}

async fn queued_job(pool: &PgPool) -> Job {
    let sgf = "(;GM[6]FF[4]MI[length:5][ws:0][bs:0];W[31hefe])";
    let imported = neondojo::store::import_match(
        pool,
        "events.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &neondojo::import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let position: Uuid = sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1")
        .bind(imported.r#match.id)
        .fetch_one(pool)
        .await
        .unwrap();
    neondojo::store::create_job(pool, position, "analysis", 1)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn batch_job_status_returns_full_available_jobs_and_identifies_missing_ids(pool: PgPool) {
    let job = queued_job(&pool).await;
    let missing = Uuid::new_v4();
    let app = api::router(api::AppState::new(pool, Engine::new("unused".into())));
    let (status, result) = json_call(
        app.clone(),
        "POST",
        "/api/jobs/status",
        serde_json::json!({"ids":[job.id,missing,job.id]}),
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(
        result,
        serde_json::json!({"jobs":[job],"missing":[missing],"restricted":[]})
    );
    for invalid in [
        serde_json::json!({"ids":vec![job.id;101]}),
        serde_json::json!({"ids":["invalid-uuid"]}),
    ] {
        assert_eq!(
            json_call(app.clone(), "POST", "/api/jobs/status", invalid)
                .await
                .0,
            422
        );
    }
}

async fn event_body(app: &axum::Router, job: Uuid, after: i64) -> Body {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{job}/events"))
                .header("last-event-id", after)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    response.into_body()
}

async fn event_frame(body: &mut Body) -> String {
    let frame = tokio::time::timeout(Duration::from_secs(5), body.frame())
        .await
        .expect("No SSE frame")
        .expect("SSE ended early")
        .unwrap();
    String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap()
}

fn event_ids(text: &str) -> Vec<i64> {
    text.lines()
        .filter_map(|line| line.strip_prefix("id: "))
        .map(|id| id.parse().unwrap())
        .collect()
}

#[sqlx::test(migrations = "./migrations")]
async fn job_events_replay_committed_rows_after_listener_disconnect(pool: PgPool) {
    let job = queued_job(&pool).await;
    let request_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with((*pool.connect_options()).clone())
        .await
        .unwrap();
    let state = api::AppState::new(request_pool, Engine::new("unused".into()));
    let app = api::router(state.clone());
    let mut body = event_body(&app, job.id, 0).await;
    assert!(event_frame(&mut body).await.contains("event: queued"));

    let mut tx = pool.begin().await.unwrap();
    let running: i64 = sqlx::query_scalar(
        "INSERT INTO job_events(job_id,event,data) VALUES($1,'running',
          '{\"status\":\"running\",\"progress\":0}') RETURNING id",
    )
    .bind(job.id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), body.frame())
            .await
            .is_err()
    );
    tx.commit().await.unwrap();
    assert_eq!(event_ids(&event_frame(&mut body).await), [running]);
    // Let the stream discard any remaining wake hint for the running event.
    assert!(
        tokio::time::timeout(Duration::from_millis(100), body.frame())
            .await
            .is_err()
    );
    let mut tx = pool.begin().await.unwrap();
    // Exercise durable replay even if reconnect is fast enough to see the commit.
    sqlx::query("ALTER TABLE job_events DISABLE TRIGGER job_events_notify")
        .execute(&mut *tx)
        .await
        .unwrap();
    let mut ids: Vec<i64> = sqlx::query_scalar(
        "INSERT INTO job_events(job_id,event,data)
         SELECT $1,'progress',jsonb_build_object('status','running','progress',n*100/105)
         FROM generate_series(1,105) AS n RETURNING id",
    )
    .bind(job.id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    sqlx::query("UPDATE jobs SET status='completed',progress=100 WHERE id=$1")
        .bind(job.id)
        .execute(&mut *tx)
        .await
        .unwrap();
    ids.push(
        sqlx::query_scalar(
            "INSERT INTO job_events(job_id,event,data) VALUES($1,'completed',
             '{\"status\":\"completed\",\"progress\":100}') RETURNING id",
        )
        .bind(job.id)
        .fetch_one(&mut *tx)
        .await
        .unwrap(),
    );
    let stopped: Vec<bool> = sqlx::query_scalar(
        "SELECT pg_terminate_backend(pid,5000) FROM pg_stat_activity
         WHERE datname=current_database() AND application_name='neondojo-job-events'",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(stopped, [true]);
    tx.commit().await.unwrap();
    ids.sort_unstable();
    let collect = async |body: Body| {
        let bytes = tokio::time::timeout(Duration::from_secs(5), body.collect())
            .await
            .expect("Stream did not terminate")
            .unwrap()
            .to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    };
    let events = collect(body).await;
    assert_eq!(event_ids(&events), ids);
    assert!(events.contains("event: completed"));
    let resumed = collect(event_body(&app, job.id, ids[100]).await).await;
    assert_eq!(event_ids(&resumed), ids[101..]);
    assert!(
        collect(event_body(&app, job.id, *ids.last().unwrap()).await)
            .await
            .is_empty()
    );
    state.shutdown.cancel();
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires pg_stat_statements preloaded on isolated PostgreSQL"]
async fn benchmark_idle_job_event_streams(pool: PgPool) {
    sqlx::query("CREATE EXTENSION pg_stat_statements")
        .execute(&pool)
        .await
        .unwrap();
    let job = queued_job(&pool).await;
    let state = api::AppState::new(pool.clone(), Engine::new("unused".into()));
    let app = api::router(state.clone());
    let mut readers = Vec::new();
    for _ in 0..12 {
        let mut body = event_body(&app, job.id, 0).await;
        assert!(event_frame(&mut body).await.contains("event: queued"));
        readers.push(tokio::spawn(body.collect()));
    }
    let stats = async || -> (i64, i64, f64) {
        sqlx::query_as(
            "SELECT coalesce(sum(calls) FILTER (WHERE query LIKE 'SELECT * FROM job_events WHERE%'),0)::bigint,
                    coalesce(sum(calls) FILTER (WHERE query = 'SELECT * FROM jobs WHERE id=$1'),0)::bigint,
                    coalesce(sum(total_exec_time),0)
             FROM pg_stat_statements
             WHERE dbid=(SELECT oid FROM pg_database WHERE datname=current_database())
               AND (query LIKE 'SELECT * FROM job_events WHERE%' OR query = 'SELECT * FROM jobs WHERE id=$1')",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    tokio::time::sleep(Duration::from_millis(500)).await;
    let before = stats().await;
    tokio::time::sleep(Duration::from_millis(2100)).await;
    let after = stats().await;
    state.shutdown.cancel();
    for reader in readers {
        tokio::time::timeout(Duration::from_secs(3), reader)
            .await
            .expect("Idle stream did not close on shutdown")
            .unwrap()
            .unwrap();
    }
    let calls = (after.0 - before.0, after.1 - before.1);
    eprintln!(
        "12 idle streams, 2100 ms: {} event reads, {} job reads, {:.3} ms PostgreSQL execution",
        calls.0,
        calls.1,
        after.2 - before.2
    );
    assert_eq!(calls, (0, 0));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg; see docs/development/testing.md"]
async fn imports_searches_analyzes_and_replays_sse(pool: PgPool) {
    let state = api::AppState::new(
        pool.clone(),
        Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())),
    );
    let app = api::router(state.clone());
    let mat = "7 point match\n\n Game 1\n Alice : 0                         Bob : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n 2) 65: 24/18 18/13                31: 8/5 6/5\n";
    let body = format!(
        "--bg\r\nContent-Disposition: form-data; name=\"file\"; filename=\"opening.mat\"\r\nContent-Type: application/octet-stream\r\n\r\n{mat}\r\n--bg--\r\n"
    );
    let upload = || {
        Request::builder()
            .method("POST")
            .uri("/api/imports")
            .header("content-type", "multipart/form-data; boundary=bg")
            .body(Body::from(body.clone()))
            .unwrap()
    };
    let response = app.clone().oneshot(upload()).await.unwrap();
    let status = response.status();
    let data = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(status, 201, "{}", String::from_utf8_lossy(&data));
    let imported: ImportResponse = serde_json::from_slice(&data).unwrap();
    assert_eq!(imported.r#match.position_count, 4);
    assert!(!imported.duplicate);
    let repeated = app.clone().oneshot(upload()).await.unwrap();
    assert_eq!(repeated.status(), 200);
    assert!(
        serde_json::from_slice::<ImportResponse>(
            &repeated.into_body().collect().await.unwrap().to_bytes()
        )
        .unwrap()
        .duplicate
    );
    let (status, result) = json_call(
        app.clone(),
        "GET",
        "/api/matches?q=Alice",
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    let (_, result) = json_call(
        app.clone(),
        "GET",
        &format!("/api/positions?match_id={}&limit=2", imported.r#match.id),
        serde_json::Value::Null,
    )
    .await;
    let first: PositionPage = serde_json::from_value(result).unwrap();
    assert_eq!(first.items.len(), 2);
    assert_eq!(first.items[0].ordinal, 0);
    let (_, result) = json_call(
        app.clone(),
        "GET",
        &format!(
            "/api/positions?match_id={}&limit=2&after={}",
            imported.r#match.id,
            first.next_cursor.unwrap()
        ),
        serde_json::Value::Null,
    )
    .await;
    let second: PositionPage = serde_json::from_value(result).unwrap();
    assert_eq!(second.items[0].ordinal, 2);
    assert!(second.next_cursor.is_none());
    let pos = &first.items[0];
    let (status, result) = json_call(
        app.clone(),
        "POST",
        "/api/jobs",
        serde_json::json!({"position_id": pos.id,"kind":"analysis","plies":1}),
    )
    .await;
    assert_eq!(status, 202);
    let job: Job = serde_json::from_value(result).unwrap();
    assert!(jobs::run_one(&state).await.unwrap());
    let (_, result) = json_call(
        app.clone(),
        "GET",
        &format!("/api/jobs/{}", job.id),
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(result["status"], "completed", "{result}");
    assert_eq!(result["result"]["candidates"][0]["notation"], "8/5 6/5");
    let event_id: i64 = sqlx::query_scalar("SELECT min(id) FROM job_events WHERE job_id=$1")
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let sse = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{}/events", job.id))
                .header("Last-Event-ID", event_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sse.headers()["content-type"], "text/event-stream");
    let events =
        String::from_utf8(sse.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(events.contains("event: completed"));
    assert!(!events.contains("event: queued"));
    let (status, _) = json_call(
        app.clone(),
        "POST",
        "/api/jobs",
        serde_json::json!({"position_id":pos.id,"kind":"rollout","plies":5}),
    )
    .await;
    assert_eq!(status, 422);
    let (_, stats) = json_call(
        app.clone(),
        "GET",
        &format!("/api/matches/{}/dice", imported.r#match.id),
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(stats["total"], 4);
    let (_, schema) = json_call(
        app.clone(),
        "GET",
        "/api/openapi.json",
        serde_json::Value::Null,
    )
    .await;
    assert!(schema["paths"]["/api/jobs/{id}/events"]["get"].is_object());
    let (status, _) = json_call(
        app.clone(),
        "GET",
        "/api/matches?limit=0",
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(status, 422);

    // Duplicate active jobs converge; a stale worker lease is reclaimed exactly once.
    let dice_job = neondojo::store::create_job(&pool, pos.id, "dice", 1)
        .await
        .unwrap();
    let duplicate = neondojo::store::create_job(&pool, pos.id, "dice", 1)
        .await
        .unwrap();
    assert_eq!(dice_job.id, duplicate.id);
    sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()-interval '1 second' WHERE id=$1").bind(dice_job.id).bind(uuid::Uuid::new_v4()).execute(&pool).await.unwrap();
    let (one, two) = tokio::join!(jobs::run_one(&state), jobs::run_one(&state));
    assert_ne!(one.unwrap(), two.unwrap());
    let done = neondojo::store::get_job(&pool, dice_job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(done.status, "completed", "{:?}", done.error);
    assert_eq!(done.attempts, 2);
    let result: neondojo::engine::Distribution =
        serde_json::from_value(done.result.unwrap()).unwrap();
    assert_eq!(result.rolls.len(), 21);
    assert_eq!(result.rolls.iter().map(|r| r.weight).sum::<i32>(), 36);
    assert!((result.rolls.iter().map(|r| r.probability).sum::<f64>() - 1.0).abs() < 1e-10);
    assert!(
        (result.mean_equity
            - result
                .rolls
                .iter()
                .map(|r| r.best.equity * r.probability)
                .sum::<f64>())
        .abs()
            < 1e-10
    );

    // Open SSE must end when the service stops, even while its job is queued.
    let queued = neondojo::store::create_job(&pool, pos.id, "dice", 1)
        .await
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{}/events", queued.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    state.shutdown.cancel();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        response.into_body().collect(),
    )
    .await
    .unwrap()
    .unwrap();
}
