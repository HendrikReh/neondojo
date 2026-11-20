use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{api, engine::Engine, import, jobs, model::Job, store};
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
#[ignore = "requires isolated PostgreSQL"]
async fn explicit_match_analysis_engine_leaves_the_saved_provider_intact(pool: PgPool) {
    let sgf = "(;GM[6]FF[4]MI[length:5][ws:0][bs:0];W[31hefe])";
    let imported = store::import_match(
        &pool,
        "engine-override.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("/no/such/neondojo-gnubg".into()),
    ));
    call(app.clone(), "GET", "/api/settings", Value::Null).await;
    sqlx::query("UPDATE app_settings SET analysis_engine='bgsage',analysis_plies=2")
        .execute(&pool)
        .await
        .unwrap();

    let uri = format!("/api/matches/{}/analysis", imported.r#match.id);
    let (status, job) = call(
        app.clone(),
        "POST",
        &uri,
        json!({"engine_id":"gnubg","plies":3}),
    )
    .await;
    assert_eq!(status, 202, "{job}");
    assert_eq!(job["engine_snapshot"]["engine_id"], "gnubg");
    assert_eq!(job["plies"], 3);
    let (_, settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    assert_eq!(settings["analysis_engine"], "bgsage");
    assert_eq!(settings["analysis_plies"], 2);
    assert_eq!(
        call(app, "POST", &uri, json!({"engine_id":"unknown","plies":3}))
            .await
            .0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL"]
async fn match_analysis_history_includes_both_providers_for_the_current_full_match(pool: PgPool) {
    let sgf = "(;GM[6]FF[4]MI[length:5][ws:0][bs:0];W[31hefe])";
    let imported = store::import_match(
        &pool,
        "engine-history.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let match_id = imported.r#match.id;
    sqlx::query("UPDATE matches SET source_revision=1 WHERE id=$1")
        .bind(match_id)
        .execute(&pool)
        .await
        .unwrap();
    let gnu = store::create_match_job(&pool, match_id, 3, 1)
        .await
        .unwrap();
    let sage = uuid::Uuid::new_v4();
    let game = store::create_match_job_scoped(&pool, match_id, 3, 1, Some(1))
        .await
        .unwrap();
    let stale = store::create_match_job(&pool, match_id, 2, 1)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET source_revision=0 WHERE id=$1")
        .bind(stale.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,completed_games,engine_snapshot) VALUES($1,$2,'match',3,'completed',1,1,$3)")
        .bind(sage)
        .bind(match_id)
        .bind(json!({"engine_id":"bgsage","gnubg_path":"gnubg"}))
        .execute(&pool)
        .await
        .unwrap();
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("gnubg".into()),
    ));
    let uri = format!("/api/matches/{match_id}/analysis/jobs");
    let (status, history) = call(app.clone(), "GET", &uri, Value::Null).await;
    assert_eq!(status, 200, "{history}");
    let ids: Vec<_> = history
        .as_array()
        .unwrap()
        .iter()
        .map(|job| job["id"].clone())
        .collect();
    assert_eq!(ids, [json!(sage), json!(gnu.id)]);
    assert!(!ids.contains(&json!(game.id)));
    assert!(!ids.contains(&json!(stale.id)));

    let (status, history) =
        call(app.clone(), "GET", &format!("{uri}?game_no=1"), Value::Null).await;
    assert_eq!(status, 200, "{history}");
    assert_eq!(history.as_array().unwrap().len(), 1);
    assert_eq!(history[0]["id"], json!(game.id));
    assert_eq!(
        call(app.clone(), "GET", &format!("{uri}?game_no=2"), Value::Null)
            .await
            .0,
        422
    );

    call(app.clone(), "GET", "/api/settings", Value::Null).await;
    sqlx::query("UPDATE app_settings SET analysis_engine='bgsage', analysis_plies=2")
        .execute(&pool)
        .await
        .unwrap();
    let latest = format!("/api/matches/{match_id}/analysis");
    for (query, expected) in [
        ("", json!(sage)),
        ("?engine_id=bgsage", json!(sage)),
        ("?engine_id=gnubg", json!(gnu.id)),
        ("?engine_id=gnubg&game_no=1", json!(game.id)),
        ("?engine_id=bgsage&game_no=1", Value::Null),
    ] {
        let (status, job) =
            call(app.clone(), "GET", &format!("{latest}{query}"), Value::Null).await;
        assert_eq!(status, 200, "{query}: {job}");
        assert_eq!(job["id"], expected, "{query}");
    }
    assert_eq!(
        call(
            app,
            "GET",
            &format!("{latest}?engine_id=unknown"),
            Value::Null
        )
        .await
        .0,
        422,
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL and native GNUbg"]
async fn gnubg_game_jobs_evaluate_all_fourteen_opening_candidates_at_depth(pool: PgPool) {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let sgf = "(;GM[6]FF[4]MI[length:21][ws:0][bs:0];B[63lolr])";
    let imported = store::import_match(
        &pool,
        "opening.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let state = api::AppState::new(pool.clone(), engine);
    let app = api::router(state.clone());
    for plies in [3, 5] {
        let (status, submitted) = call(
            app.clone(),
            "POST",
            &format!("/api/matches/{}/analysis", imported.r#match.id),
            json!({"engine_id":"gnubg","plies":plies,"game_no":1}),
        )
        .await;
        assert_eq!(status, 202, "{submitted}");
        let job_id = submitted["id"].as_str().unwrap();
        assert!(jobs::run_one(&state).await.unwrap());
        let (_, done) = call(
            app.clone(),
            "GET",
            &format!("/api/jobs/{job_id}"),
            Value::Null,
        )
        .await;
        assert_eq!(done["status"], "completed", "{done}");
        let stored: Value = sqlx::query_scalar(
            "SELECT data FROM move_analyses WHERE job_id=$1::uuid AND kind='checker'",
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(stored["checker_candidates"].is_array(), "{stored}");
        let (status, result) = call(
            app.clone(),
            "GET",
            &format!("/api/jobs/{job_id}/moves?kind=checker&limit=1"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{result}");
        let position_id = result["items"][0]["position_uuid"].as_str().unwrap();
        let (status, position_result) = call(
            app.clone(),
            "GET",
            &format!("/api/positions/{position_id}/analysis-result?analysis_id=job:{job_id}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{position_result}");
        let candidates = position_result["decision"]["checker_candidates"]
            .as_array()
            .unwrap_or_else(|| panic!("Missing checker candidates: {position_result}"));
        assert_eq!(candidates.len(), 14);
        let running_play = candidates
            .iter()
            .find(|row| row["notation"] == "24/15")
            .unwrap();
        assert_eq!(running_play["path"], "24/18 18/15");
        let depths: Vec<_> = candidates
            .iter()
            .map(|candidate| candidate["plies"].as_i64().unwrap())
            .collect();
        assert_eq!(depths, vec![plies; 14]);
        assert_eq!(submitted["engine_snapshot"]["gnubg_move_filter"], "top14");
    }
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
    let (status, value) = call(app.clone(), "POST", &endpoint, json!({"plies": 1})).await;
    assert_eq!(status, 202, "{value}");
    let job: Job = serde_json::from_value(value).unwrap();
    let (_, duplicate) = call(app.clone(), "POST", &endpoint, json!({"plies": 1})).await;
    assert_eq!(duplicate["id"], job.id.to_string());
    assert!(jobs::run_one(&state).await.unwrap());
    let (_, done) = call(app.clone(), "GET", &endpoint, Value::Null).await;
    assert_eq!(done["status"], "completed", "{done}");
    assert_eq!(done["completed_games"], 2);
    assert_eq!(done["total_games"], 2);
    assert_eq!(done["result"]["analyzed_moves"], 4);
    let (_, statistics) = call(
        app.clone(),
        "GET",
        &format!("/api/jobs/{}/statistics", job.id),
        Value::Null,
    )
    .await;
    let (status, comparison) = call(
        app.clone(),
        "GET",
        &format!("{endpoint}/jobs?game_no=2"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{comparison}");
    assert_eq!(comparison.as_array().unwrap().len(), 1, "{comparison}");
    assert_eq!(comparison[0]["id"], json!(job.id));
    assert!(comparison[0]["game_no"].is_null());
    assert_eq!(
        comparison[0]["game_summary"],
        statistics["games"][1]["summary"]
    );
    assert_eq!(comparison[0]["game_summary"]["analyzed_moves"], 2);
    sqlx::query("UPDATE match_analysis_games SET summary=NULL WHERE job_id=$1 AND game_no=2")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, legacy) = call(
        app.clone(),
        "GET",
        &format!("{endpoint}/jobs?game_no=2"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{legacy}");
    assert_eq!(
        legacy[0]["game_summary"]["players"],
        comparison[0]["game_summary"]["players"]
    );
    assert!(legacy[0]["game_summary"]["performance_rating"].is_null());
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
    // Timing-ready/start updates are ordinary progress events too. Select the
    // first confirmation of each completed game for the durable SSE assertions.
    let checkpoints: Vec<_> = (1..=2)
        .map(|completed| {
            events
                .iter()
                .find(|e| e.event == "progress" && e.data["completed_games"] == completed)
                .unwrap()
        })
        .collect();
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
        call(app, "POST", &endpoint, json!({"plies":6})).await.0,
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
    let state = api::AppState::new(pool.clone(), Engine::new("/no/such/neondojo-gnubg".into()));
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
    let job = store::create_match_job(&pool, imported.r#match.id, 1, 1)
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
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let elapsed: Option<f64> = sqlx::query_scalar(
                "SELECT active_elapsed_ms FROM match_analysis_timing WHERE job_id=$1",
            )
            .bind(job.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
            if elapsed.is_some_and(|ms| ms >= 1000.0) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("monotonic samples must persist while a silent native process is running");
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
    let timing: (i64,f64,f64)=sqlx::query_as("SELECT timed_moves,measured_elapsed_ms,active_elapsed_ms FROM match_analysis_timing WHERE job_id=$1").bind(job.id).fetch_one(&pool).await.unwrap();
    assert_eq!(
        timing,
        (0, 0.0, 0.0),
        "interrupted sections never become confirmed time and active samples reset at failure"
    );
}
