use axum::{body::Body, http::Request};
use bgtutor::{api, engine::Engine, import, jobs, model::Job, store};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

async fn call(app: axum::Router, method: &str, uri: &str, body: Value) -> (u16, Value) {
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
#[ignore = "requires PostgreSQL and GNUbg"]
async fn full_match_analysis_is_persisted_paginated_and_replayable(pool: PgPool) {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    // Deliberately weak opening 31, then a second game with independent context.
    let mat = b"7 point match\n\n Game 1\n Alice : 0                         Bob : 0\n 1) 31: 24/21 6/5                  42: 8/4 6/4\n\n Game 2\n Alice : 1                         Bob : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n";
    let sgf = engine.normalize_mat(mat).await.unwrap();
    let parsed = import::parse_sgf(&sgf).unwrap();
    let imported = store::import_match(&pool, "two-games.mat", "mat", mat, &sgf, &parsed)
        .await
        .unwrap();
    let state = api::AppState::new(pool.clone(), engine);
    let app = api::router(state.clone());
    let endpoint = format!("/api/matches/{}/analysis", imported.r#match.id);
    let (status, empty) = call(app.clone(), "GET", &endpoint, Value::Null).await;
    assert_eq!(status, 200);
    assert!(empty.is_null());
    let (status, value) = call(app.clone(), "POST", &endpoint, json!({"plies": 0})).await;
    assert_eq!(status, 202, "{value}");
    let job: Job = serde_json::from_value(value).unwrap();
    let (_, duplicate) = call(app.clone(), "POST", &endpoint, json!({"plies": 0})).await;
    assert_eq!(duplicate["id"], job.id.to_string());
    assert!(jobs::run_one(&state).await.unwrap());
    let (_, done) = call(app.clone(), "GET", &endpoint, Value::Null).await;
    assert_eq!(done["status"], "completed", "{done}");
    assert_eq!(done["completed_games"], 2);
    assert_eq!(done["total_games"], 2);
    assert_eq!(done["result"]["analyzed_moves"], 4);
    let (_, first) = call(
        app.clone(),
        "GET",
        &format!("/api/jobs/{}/moves?limit=1", job.id),
        Value::Null,
    )
    .await;
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    let row = &first["items"][0];
    assert_eq!(row["kind"], "checker");
    assert!(row["equity_loss"].as_f64().unwrap() > 0.04, "{row}");
    assert!(row["position_uuid"].is_string());
    assert_eq!(row["best"], "8/5 6/5");
    let cursor = first["next_cursor"].as_i64().unwrap();
    let (_, second) = call(
        app.clone(),
        "GET",
        &format!("/api/jobs/{}/moves?limit=100&after={cursor}", job.id),
        Value::Null,
    )
    .await;
    assert!(
        second["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["id"].as_i64().unwrap() > cursor)
    );
    let (_, errors) = call(
        app.clone(),
        "GET",
        &format!("/api/jobs/{}/moves?errors_only=true", job.id),
        Value::Null,
    )
    .await;
    assert!(!errors["items"].as_array().unwrap().is_empty());
    assert!(
        errors["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["equity_loss"].as_f64().unwrap() >= 0.04)
    );
    let events = store::events(&pool, job.id, 0).await.unwrap();
    let checkpoints: Vec<_> = events.iter().filter(|e| e.event == "progress").collect();
    assert_eq!(checkpoints.len(), 2);
    assert_eq!(checkpoints[0].data["completed_games"], 1);
    assert_eq!(checkpoints[0].data["progress"], 50);
    assert_eq!(checkpoints[1].data["completed_games"], 2);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{}/events", job.id))
                .header("Last-Event-ID", checkpoints[0].id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let text = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(text.contains("event: completed"));
    assert!(!text.contains(&format!("id: {}\n", checkpoints[0].id)));
    for query in ["limit=0", "player=2", "after=-1", "kind=unknown"] {
        assert_eq!(
            call(
                app.clone(),
                "GET",
                &format!("/api/jobs/{}/moves?{query}", job.id),
                Value::Null
            )
            .await
            .0,
            422
        );
    }
    assert_eq!(
        call(app, "POST", &endpoint, json!({"plies":4})).await.0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn engine_failure_is_visible_and_empty_job_results_remain_readable(pool: PgPool) {
    let sgf = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][game:0][ws:0][bs:0];W[31hefe])";
    let parsed = import::parse_sgf(sgf).unwrap();
    let imported = store::import_match(&pool, "failure.sgf", "sgf", sgf.as_bytes(), sgf, &parsed)
        .await
        .unwrap();
    let state = api::AppState::new(pool.clone(), Engine::new("/no/such/bgtutor-gnubg".into()));
    let app = api::router(state.clone());
    let endpoint = format!("/api/matches/{}/analysis", imported.r#match.id);
    let (_, value) = call(app.clone(), "POST", &endpoint, json!({})).await;
    let job: Job = serde_json::from_value(value).unwrap();
    assert!(jobs::run_one(&state).await.unwrap());
    let (_, done) = call(app.clone(), "GET", &endpoint, Value::Null).await;
    assert_eq!(done["status"], "failed");
    assert!(done["error"].as_str().unwrap().len() > 5);
    assert_eq!(done["completed_games"], 0);
    let (status, page) = call(
        app,
        "GET",
        &format!("/api/jobs/{}/moves", job.id),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(page["items"], json!([]));
}

#[cfg(unix)]
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL; runs a 22-second engine-process heartbeat check"]
async fn heartbeat_renews_lease_during_a_long_game(pool: PgPool) {
    use std::{os::unix::fs::PermissionsExt, time::Duration};
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("slow-engine");
    // Exercise a process that is silent for longer than one heartbeat interval.
    // Native GNUbg correctness is covered separately by the real-engine tests.
    std::fs::write(&binary, "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'GNU Backgammon heartbeat fixture'; exit 0; fi\nsleep 22\nexit 1\n").unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    let sgf = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][game:0][ws:0][bs:0];W[31hefe])";
    let imported = store::import_match(
        &pool,
        "heartbeat.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let job = store::create_match_job(&pool, imported.r#match.id, 0, 1)
        .await
        .unwrap();
    let state = api::AppState::new(
        pool.clone(),
        Engine::new(binary.to_string_lossy().into_owned()),
    );
    let worker = tokio::spawn(async move { jobs::run_one(&state).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if store::get_job(&pool, job.id).await.unwrap().unwrap().status == "running" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let initial: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "UPDATE jobs SET lease_until=now()+interval '25 seconds' WHERE id=$1 RETURNING lease_until",
    )
    .bind(job.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(24), async {
        loop {
            let lease: Option<chrono::DateTime<chrono::Utc>> =
                sqlx::query_scalar("SELECT lease_until FROM jobs WHERE id=$1")
                    .bind(job.id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            if lease.is_some_and(|value| value > initial + chrono::Duration::seconds(60)) {
                break;
            }
            assert!(!worker.is_finished(), "engine finished without a heartbeat");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    assert!(worker.await.unwrap().unwrap());
    let done = store::get_job(&pool, job.id).await.unwrap().unwrap();
    assert_eq!(done.status, "failed");
    assert!(done.error.unwrap().contains("process failed"));
    assert_eq!(done.attempts, 1);
}
