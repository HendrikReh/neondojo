use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{
    api,
    engine::Engine,
    jobs,
    model::{ImportResponse, Job, PositionPage},
};
use sqlx::PgPool;
use tower::ServiceExt;

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
