use axum::{body::Body, http::Request};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
    jobs,
    model::ImportResponse,
    store,
};
use http_body_util::BodyExt;
use sqlx::PgPool;
use tower::ServiceExt;

fn upload(filename: &str, bytes: &[u8]) -> Request<Body> {
    let mut body=format!("--bg\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n--bg--\r\n");
    Request::builder()
        .method("POST")
        .uri("/api/imports")
        .header("content-type", "multipart/form-data; boundary=bg")
        .body(Body::from(body))
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn import_uses_unsorted_unless_a_database_is_selected(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), Engine::new("gnubg".into())));
    let original = b"(;GM[6]PW[Alice]PB[Bob]MI[length:7];W[31hefe])";
    let selected: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO databases(id,name) VALUES(gen_random_uuid(),'Demo BGF') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    for (uri, expected_name) in [
        ("/api/imports".to_owned(), "Unsorted"),
        (format!("/api/imports?database_id={selected}"), "Demo BGF"),
    ] {
        let mut request = upload("match.sgf", original);
        *request.uri_mut() = uri.parse().unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), 201);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let result: ImportResponse = serde_json::from_slice(&body).unwrap();
        let name: String = sqlx::query_scalar(
            "SELECT d.name FROM matches m JOIN databases d ON d.id=m.database_id WHERE m.id=$1",
        )
        .bind(result.r#match.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(name, expected_name);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and local GNUbg"]
async fn all_four_formats_are_atomic_and_engine_failures_are_persisted(pool: PgPool) {
    let state = AppState::new(
        pool.clone(),
        Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())),
    );
    let app = api::router(state.clone());
    let bad=b"7 point match\n\n Game 1\n Alice : 0                         Bob : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n 2) 65: 24/22 22/13\n";
    assert_eq!(
        app.clone()
            .oneshot(upload("broken.mat", bad))
            .await
            .unwrap()
            .status(),
        422
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM matches")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        app.clone()
            .oneshot(upload("broken.xg", b"RGMH"))
            .await
            .unwrap()
            .status(),
        422
    );
    let mut imported = Vec::new();
    for (filename, bytes, expected) in [
        (
            "WC2014.xg",
            include_bytes!("fixtures/WC2014.xg").as_slice(),
            460,
        ),
        (
            "WC2014.sgf",
            include_bytes!("fixtures/WC2014.gnubg.sgf").as_slice(),
            460,
        ),
        (
            "WC2014.mat",
            include_bytes!("fixtures/WC2014.mat").as_slice(),
            460,
        ),
        (
            "tutorial.bgf",
            include_bytes!("fixtures/tutorial.bgf").as_slice(),
            3,
        ),
    ] {
        let response = app.clone().oneshot(upload(filename, bytes)).await.unwrap();
        let status = response.status();
        let data = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(status, 201, "{}", String::from_utf8_lossy(&data));
        let result: ImportResponse = serde_json::from_slice(&data).unwrap();
        assert_eq!(result.r#match.position_count, expected);
        imported.push(result.r#match.id);
    }
    // A BGF source reaches the real native checker/cube analysis worker, too.
    let bgf_job = store::create_match_job(&pool, imported[3], 0, 1)
        .await
        .unwrap();
    assert!(jobs::run_one(&state).await.unwrap());
    let analyzed = store::get_job(&pool, bgf_job.id).await.unwrap().unwrap();
    assert_eq!(analyzed.status, "completed", "{:?}", analyzed.error);
    assert_eq!((analyzed.total_games, analyzed.completed_games), (1, 1));
    let checker_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM move_analyses WHERE job_id=$1 AND kind='checker'")
            .bind(bgf_job.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(checker_count, 3);

    // An actual spawn failure must be durable and visible as a terminal SSE event.
    let position = store::positions(&pool, Some(imported[0]), None, None, 1)
        .await
        .unwrap()
        .items
        .remove(0);
    let job = store::create_job(&pool, position.id, "analysis", 0)
        .await
        .unwrap();
    let broken_state = AppState::new(
        pool.clone(),
        Engine::new("/nonexistent/bgtutor/gnubg".into()),
    );
    jobs::run_one(&broken_state).await.unwrap();
    let failed = store::get_job(&pool, job.id).await.unwrap().unwrap();
    assert_eq!(failed.status, "failed");
    assert!(failed.error.unwrap().contains("Could not start GNUbg"));
    assert_eq!(
        store::events(&pool, job.id, 0)
            .await
            .unwrap()
            .last()
            .unwrap()
            .event,
        "failed"
    );
    // Expired leases stop after the third attempt instead of looping forever.
    let retried = store::create_job(&pool, position.id, "analysis", 0)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET status='running',attempts=3,lease_until=now()-interval '1 second' WHERE id=$1").bind(retried.id).execute(&pool).await.unwrap();
    assert!(!jobs::run_one(&broken_state).await.unwrap());
    let failed = store::get_job(&pool, retried.id).await.unwrap().unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.attempts, 3);

    // Deterministically simulate the worker completing the conflicting row
    // after INSERT ON CONFLICT, before create_job's follow-up SELECT.
    let existing = store::create_job(&pool, position.id, "analysis", 0)
        .await
        .unwrap();
    sqlx::raw_sql(&format!("CREATE FUNCTION complete_conflicting_job() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE jobs SET status='completed' WHERE id='{}'::uuid; RETURN NULL; END $$; CREATE TRIGGER complete_after_insert AFTER INSERT ON jobs FOR EACH STATEMENT EXECUTE FUNCTION complete_conflicting_job();",existing.id)).execute(&pool).await.unwrap();
    let replacement = store::create_job(&pool, position.id, "analysis", 0)
        .await
        .unwrap();
    assert_ne!(replacement.id, existing.id);
    assert_eq!(replacement.status, "queued");
}
