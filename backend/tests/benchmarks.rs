use axum::{body::Body, http::Request};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
};
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
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
fn engine() -> Engine {
    Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()))
}

#[cfg(unix)]
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn health_allows_gnubg_version_startup_past_300_ms(pool: PgPool) {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("gnubg");
    std::fs::write(
        &executable,
        "#!/bin/sh\nsleep 0.6\nprintf '%s\\n' 'GNU Backgammon test version'\n",
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let app = api::router(AppState::new(
        pool,
        Engine::new(executable.to_str().unwrap().to_owned()),
    ));

    let (status, health) = call(app, "GET", "/api/health", Value::Null).await;
    assert_eq!(status, 200, "{health}");
    assert_eq!(health["gnubg"], "GNU Backgammon test version");
}

#[sqlx::test(migrations = false)]
#[ignore = "requires PostgreSQL"]
async fn play_latency_migration_preserves_existing_analysis_history(pool: PgPool) {
    let all = sqlx::migrate!("./migrations");
    let previous = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            all.iter().filter(|m| m.version < 39).cloned().collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    previous.run(&pool).await.unwrap();
    let reference = serde_json::to_value(bgtutor::benchmarks::reference()).unwrap();
    let machine = json!({
        "id": uuid::Uuid::new_v4(), "name": "Historical backend",
        "os": "macOS", "architecture": "aarch64",
        "cpu_model": null, "logical_cpus": null, "memory_bytes": null
    });
    for status in ["queued", "running", "completed", "failed", "cancelled"] {
        sqlx::query("INSERT INTO benchmark_runs(id,status,machine,engine_id,engine_name,engine_version,engine_path,plies,reference,protocol,reference_sgf,completed_games,analyzed_moves,total_elapsed_ms,average_move_ms,finished_at) VALUES($1,$2,$3,'gnubg','GNUbg','historical version','/historical/gnubg',2,$4,$5,$6,CASE WHEN $2='completed' THEN 10 ELSE 0 END,CASE WHEN $2='completed' THEN 460 ELSE 0 END,CASE WHEN $2='completed' THEN 4600 END,CASE WHEN $2='completed' THEN 10 END,CASE WHEN $2 IN ('completed','failed','cancelled') THEN now() END)")
            .bind(uuid::Uuid::new_v4()).bind(status).bind(&machine).bind(&reference)
            .bind(&reference["protocol"]).bind(bgtutor::benchmarks::REFERENCE_SGF)
            .execute(&pool).await.unwrap();
    }
    let before: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(r) FROM benchmark_runs r ORDER BY queue_no")
            .fetch_all(&pool)
            .await
            .unwrap();
    all.run(&pool).await.unwrap();
    let after: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(r) - 'play_reference' - 'completed_turns' - 'average_turn_ms' - 'engine_snapshot' - 'startup_elapsed_ms' FROM benchmark_runs r ORDER BY queue_no")
        .fetch_all(&pool).await.unwrap();
    // Migration 0079 moves GNUbg's native 2-ply to the shared 3-ply.
    let expected: Vec<Value> = before
        .into_iter()
        .map(|mut run| {
            run["plies"] = json!(run["plies"].as_i64().unwrap() + 1);
            run
        })
        .collect();
    assert_eq!(after, expected);
    let app = api::router(AppState::new(pool, engine()));
    let (status, history) = call(app, "GET", "/api/benchmarks", Value::Null).await;
    assert_eq!(status, 200, "{history}");
    assert_eq!(history["runs"].as_array().unwrap().len(), 5);
    for run in history["runs"].as_array().unwrap() {
        assert!(run["play_reference"].is_null());
        assert!(run["average_turn_ms"].is_null());
        assert_eq!(run["completed_turns"], 0);
        assert_eq!(run["reference"], reference);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn interrupted_play_measurements_never_publish_partial_times(pool: PgPool) {
    let state = AppState::new(pool.clone(), engine());
    let app = api::router(state.clone());
    for interruption in ["restart", "lease expired", "cancel"] {
        let (status, run) = call(app.clone(), "POST", "/api/benchmarks/play", Value::Null).await;
        assert_eq!(status, 202, "{run}");
        assert_eq!(run["play_reference"]["total_turns"], 11);
        let id = uuid::Uuid::parse_str(run["id"].as_str().unwrap()).unwrap();
        sqlx::query("UPDATE benchmark_runs SET status='running',started_at=now(),completed_turns=1,worker_token=$2,lease_until=now()-interval '1 second' WHERE id=$1")
            .bind(id).bind(uuid::Uuid::new_v4()).execute(&pool).await.unwrap();
        match interruption {
            "restart" => bgtutor::benchmarks::recover_interrupted(&pool)
                .await
                .unwrap(),
            "lease expired" => assert!(!bgtutor::benchmarks::run_one(&state).await.unwrap()),
            _ => {
                let (status, _) = call(
                    app.clone(),
                    "POST",
                    &format!("/api/benchmarks/{id}/cancel"),
                    Value::Null,
                )
                .await;
                assert_eq!(status, 200);
            }
        }
        let (_, history) = call(app.clone(), "GET", "/api/benchmarks", Value::Null).await;
        let saved = &history["runs"][0];
        assert_eq!(saved["id"], run["id"]);
        assert_eq!(
            saved["status"],
            if interruption == "cancel" {
                "cancelled"
            } else {
                "failed"
            }
        );
        assert_eq!(saved["completed_turns"], 1);
        assert!(saved["total_elapsed_ms"].is_null());
        assert!(saved["average_turn_ms"].is_null());
        assert!(saved["average_move_ms"].is_null());
        if interruption != "cancel" {
            assert!(saved["error"].as_str().unwrap().contains(interruption));
        }
        assert_eq!(history["active"], false);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn reference_and_openapi_describe_the_fixed_workload(pool: PgPool) {
    let app = api::router(AppState::new(pool, engine()));
    let (status, reference) =
        call(app.clone(), "GET", "/api/benchmarks/reference", Value::Null).await;
    assert_eq!(status, 200, "{reference}");
    assert_eq!(reference["total_games"], 10);
    assert_eq!(reference["total_moves"], 460);
    assert_eq!(reference["sha256"].as_str().unwrap().len(), 64);
    assert!(reference["protocol"]["game_timeout_seconds"].is_null());
    assert_eq!(reference["protocol"]["id"], "gnubg-match-v2-unlimited");
    let (_, schema) = call(app, "GET", "/api/openapi.json", Value::Null).await;
    for path in [
        "/api/benchmarks",
        "/api/benchmarks/play",
        "/api/benchmarks/reference",
        "/api/benchmarks/{id}/cancel",
    ] {
        assert!(schema["paths"][path].is_object(), "{path}");
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn runs_are_append_only_snapshots_and_cancellation_survives_reload(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let body = json!({"engine_id":"gnubg","plies":[1,3]});
    let (status, first) = call(app.clone(), "POST", "/api/benchmarks", body.clone()).await;
    assert_eq!(status, 202, "{first}");
    assert_eq!(first.as_array().unwrap().len(), 2);
    assert_eq!(first[0]["status"], "queued");
    assert!(first[0]["engine_path"].as_str().unwrap().starts_with('/'));
    assert!(
        first[0]["engine_version"]
            .as_str()
            .unwrap()
            .starts_with("GNU Backgammon")
    );
    assert!(first[0]["total_elapsed_ms"].is_null());
    assert!(!first[0]["machine"]["id"].as_str().unwrap().is_empty());
    assert_eq!(first[0]["machine"], first[1]["machine"]);
    let (_, second) = call(app.clone(), "POST", "/api/benchmarks", body).await;
    assert_ne!(first[0]["id"], second[0]["id"]);
    let uri = format!(
        "/api/benchmarks/{}/cancel",
        first[0]["id"].as_str().unwrap()
    );
    let (status, cancelled) = call(app.clone(), "POST", &uri, Value::Null).await;
    assert_eq!(status, 200);
    assert_eq!(cancelled["status"], "cancelled");
    assert!(cancelled["average_move_ms"].is_null());
    assert_eq!(
        call(app, "POST", &uri, Value::Null).await.1["status"],
        "cancelled"
    );
    let reloaded = api::router(AppState::new(pool, Engine::new("/different-engine".into())));
    let (_, list) = call(reloaded, "GET", "/api/benchmarks", Value::Null).await;
    assert_eq!(list["runs"].as_array().unwrap().len(), 4);
    let saved = list["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == first[0]["id"])
        .unwrap();
    assert_eq!(saved["machine"], first[0]["machine"]);
    assert_eq!(saved["reference"], first[0]["reference"]);
    assert_eq!(saved["engine_path"], first[0]["engine_path"]);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn rejects_invalid_depths_and_unavailable_engines_without_partial_runs(pool: PgPool) {
    let app = api::router(AppState::new(pool, Engine::new("/missing/gnubg".into())));
    for body in [
        json!({"engine_id":"gnubg","plies":[]}),
        json!({"engine_id":"gnubg","plies":[1,1]}),
        json!({"engine_id":"gnubg","plies":[-1]}),
        json!({"engine_id":"gnubg","plies":[6]}),
        json!({"engine_id":"bgsage","plies":[0]}),
        json!({"engine_id":"gnubg","plies":[1]}),
        json!({}),
    ] {
        let (status, result) = call(app.clone(), "POST", "/api/benchmarks", body).await;
        assert_eq!(status, 422, "{result}");
    }
    let (_, list) = call(app, "GET", "/api/benchmarks", Value::Null).await;
    assert_eq!(list["runs"], json!([]));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn native_zero_ply_run_completes_all_games_and_moves(pool: PgPool) {
    let state = AppState::new(pool.clone(), engine());
    let app = api::router(state.clone());
    let (status, runs) = call(
        app.clone(),
        "POST",
        "/api/benchmarks",
        json!({"engine_id":"gnubg","plies":[1]}),
    )
    .await;
    assert_eq!(status, 202, "{runs}");
    assert!(bgtutor::jobs::run_one(&state).await.unwrap());
    let (_, list) = call(app, "GET", "/api/benchmarks", Value::Null).await;
    let run = &list["runs"][0];
    assert_eq!(run["status"], "completed", "{run}");
    assert_eq!(run["completed_games"], 10);
    assert_eq!(run["analyzed_moves"], 460);
    let total = run["total_elapsed_ms"].as_f64().unwrap();
    let avg = run["average_move_ms"].as_f64().unwrap();
    assert!(total > 0.0);
    assert!((avg - total / 460.0).abs() < 1e-6);
    assert_eq!(list["active"], false);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM matches")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn interrupted_runs_do_not_resume_and_queued_runs_keep_their_snapshots(pool: PgPool) {
    let state = AppState::new(pool.clone(), engine());
    let app = api::router(state);
    let (_, runs) = call(
        app.clone(),
        "POST",
        "/api/benchmarks",
        json!({"engine_id":"gnubg","plies":[1,2]}),
    )
    .await;
    let id = uuid::Uuid::parse_str(runs[0]["id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE benchmark_runs SET status='running',started_at=now(),completed_games=1,analyzed_moves=10,worker_token=$2 WHERE id=$1")
        .bind(id).bind(uuid::Uuid::new_v4()).execute(&pool).await.unwrap();
    bgtutor::benchmarks::recover_interrupted(&pool)
        .await
        .unwrap();
    let (_, list) = call(app, "GET", "/api/benchmarks", Value::Null).await;
    let failed = list["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == runs[0]["id"])
        .unwrap();
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["completed_games"], 1);
    assert!(failed["total_elapsed_ms"].is_null());
    assert!(failed["average_move_ms"].is_null());
    assert!(failed["error"].as_str().unwrap().contains("restart"));
    let queued = list["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == runs[1]["id"])
        .unwrap();
    assert_eq!(queued["status"], "queued");
    assert_eq!(queued["reference"], runs[1]["reference"]);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn changed_engine_fails_without_a_partial_timing(pool: PgPool) {
    let state = AppState::new(pool.clone(), engine());
    let app = api::router(state.clone());
    let (status, runs) = call(
        app.clone(),
        "POST",
        "/api/benchmarks",
        json!({"engine_id":"gnubg","plies":[1]}),
    )
    .await;
    assert_eq!(status, 202);
    let id = uuid::Uuid::parse_str(runs[0]["id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE benchmark_runs SET engine_version='GNU Backgammon incompatible-version' WHERE id=$1").bind(id).execute(&pool).await.unwrap();
    assert!(bgtutor::jobs::run_one(&state).await.unwrap());
    let (_, list) = call(app, "GET", "/api/benchmarks", Value::Null).await;
    assert_eq!(list["runs"][0]["status"], "failed");
    assert!(
        list["runs"][0]["error"]
            .as_str()
            .unwrap()
            .contains("version changed")
    );
    assert!(list["runs"][0]["total_elapsed_ms"].is_null());
}

#[cfg(unix)]
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn cancelling_running_measurement_kills_native_process_and_never_publishes_times(
    pool: PgPool,
) {
    assert_cancellation_kills_native_process(pool, false).await;
}

#[cfg(unix)]
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn cancelling_play_measurement_kills_native_process_and_never_publishes_times(pool: PgPool) {
    assert_cancellation_kills_native_process(pool, true).await;
}

#[cfg(unix)]
async fn assert_cancellation_kills_native_process(pool: PgPool, play: bool) {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("gnubg-benchmark-test");
    let pidfile = directory.path().join("native.pid");
    let real = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|d| d.join("gnubg"))
        .find(|p| p.is_file())
        .unwrap();
    std::fs::write(
        &script,
        format!(
            r#"#!/usr/bin/env python3
import os,sys
real={real:?}
if '--version' not in sys.argv and {command:?} in open('commands.txt').read():
    with open({pidfile:?},'w') as f: f.write(str(os.getpid()))
    os.execv(sys.executable,[sys.executable,'-c','import time; time.sleep(30)'])
os.execv(real,[real]+sys.argv[1:])
"#,
            command = if play {
                "load python adapter.py"
            } else {
                "analyse game"
            },
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let state = AppState::new(pool, Engine::new(script.to_string_lossy().into_owned()));
    let app = api::router(state.clone());
    let (status, response) = call(
        app.clone(),
        "POST",
        if play {
            "/api/benchmarks/play"
        } else {
            "/api/benchmarks"
        },
        if play {
            json!({"engine_id":"gnubg","plies":1})
        } else {
            json!({"engine_id":"gnubg","plies":[1]})
        },
    )
    .await;
    let runs = if play { json!([response]) } else { response };
    assert_eq!(status, 202, "{runs}");
    let worker = tokio::spawn(async move { bgtutor::jobs::run_one(&state).await.unwrap() });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !pidfile.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let pid = std::fs::read_to_string(pidfile).unwrap();
    let health = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        call(app.clone(), "GET", "/api/health", Value::Null),
    )
    .await
    .expect("health must stay responsive during a benchmark");
    assert_eq!(health.0, 200);
    assert_eq!(health.1["gnubg"], runs[0]["engine_version"]);
    let catalog = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        call(app.clone(), "GET", "/api/engines", Value::Null),
    )
    .await
    .expect("catalog must not run a competing probe or wait behind a benchmark");
    assert_eq!(catalog.1[0]["available"], true);
    assert_eq!(catalog.1[0]["version"], runs[0]["engine_version"]);
    let (busy, _) = call(
        app.clone(),
        "POST",
        "/api/benchmarks",
        json!({"engine_id":"gnubg","plies":[1]}),
    )
    .await;
    assert_eq!(busy, 409);

    let uri = format!("/api/benchmarks/{}/cancel", runs[0]["id"].as_str().unwrap());
    assert_eq!(
        call(app.clone(), "POST", &uri, Value::Null).await.1["status"],
        "cancelled"
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(3), worker)
            .await
            .unwrap()
            .unwrap()
    );
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let alive = std::process::Command::new("kill")
                .args(["-0", &pid])
                .output()
                .unwrap()
                .status
                .success();
            if !alive {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("cancel must terminate the actual engine process");
    let (_, list) = call(app, "GET", "/api/benchmarks", Value::Null).await;
    assert_eq!(list["runs"][0]["status"], "cancelled");
    assert!(list["runs"][0]["total_elapsed_ms"].is_null());
    assert!(list["runs"][0]["average_move_ms"].is_null());
    assert!(list["runs"][0]["average_turn_ms"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn expired_worker_is_failed_without_resuming_or_publishing_partial_times(pool: PgPool) {
    let state = AppState::new(pool.clone(), engine());
    let app = api::router(state.clone());
    let (status, runs) = call(
        app.clone(),
        "POST",
        "/api/benchmarks",
        json!({"engine_id":"gnubg","plies":[1]}),
    )
    .await;
    assert_eq!(status, 202);
    let id = uuid::Uuid::parse_str(runs[0]["id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE benchmark_runs SET status='running', started_at=now(), completed_games=1,analyzed_moves=10,worker_token=$2,lease_until=now()-interval '1 second' WHERE id=$1")
        .bind(id).bind(uuid::Uuid::new_v4()).execute(&pool).await.unwrap();
    assert!(!bgtutor::jobs::run_one(&state).await.unwrap());
    let (_, list) = call(app, "GET", "/api/benchmarks", Value::Null).await;
    assert_eq!(list["runs"][0]["status"], "failed");
    assert!(
        list["runs"][0]["error"]
            .as_str()
            .unwrap()
            .contains("lease expired")
    );
    assert_eq!(list["active"], false);
    assert!(list["runs"][0]["total_elapsed_ms"].is_null());
    assert!(list["runs"][0]["average_move_ms"].is_null());
}
