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
    let data = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&data).unwrap_or(Value::Null))
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL"]
async fn installation_is_separate_revisioned_and_rejects_invalid_python(pool: PgPool) {
    let app = api::router(AppState::new(pool, Engine::new("gnubg".into())));
    let (status, initial) = call(app.clone(), "GET", "/api/engines/bgsage", Value::Null).await;
    assert_eq!(status, 200, "{initial}");
    assert_eq!(initial["version"], 0);
    assert!(initial["snapshot"].is_null());
    let (status, _) = call(
        app.clone(),
        "PUT",
        "/api/engines/bgsage",
        json!({"version":1,"path":"/missing/python"}),
    )
    .await;
    assert_eq!(status, 409, "revision conflict must precede probe");
    let (status, _) = call(
        app.clone(),
        "PUT",
        "/api/engines/bgsage",
        json!({"version":0,"path":"/missing/python"}),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(
        call(app, "GET", "/api/engines/bgsage", Value::Null).await.1,
        initial
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL and OPEN_SAGE_TEST_PYTHON with locked weights"]
async fn verified_installation_allows_independent_analysis_and_preserves_snapshot(pool: PgPool) {
    let python =
        std::env::var("OPEN_SAGE_TEST_PYTHON").expect("test requires explicit Open Sage Python");
    let app = api::router(AppState::new(pool.clone(), Engine::new("gnubg".into())));
    let (status, installation) = call(
        app.clone(),
        "PUT",
        "/api/engines/bgsage",
        json!({"version":0,"path":python}),
    )
    .await;
    assert_eq!(status, 200, "{installation}");
    assert_eq!(installation["version"], 1);
    assert_eq!(installation["snapshot"]["plies"], 2);
    assert!(
        installation["snapshot"]["artifact_fingerprint"]
            .as_str()
            .unwrap()
            .len()
            >= 64
    );
    let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    settings["analysis_engine"] = json!("bgsage");
    settings["analysis_plies"] = json!(2);
    let (status, changed) = call(app.clone(), "PUT", "/api/settings", settings).await;
    assert_eq!(status, 200, "{changed}");
    assert_eq!(changed["play_engine"], "gnubg");
    assert_eq!(changed["analysis_engine"], "bgsage");
    let (status, catalog) = call(app.clone(), "GET", "/api/engines", Value::Null).await;
    assert_eq!(status, 200);
    let sage = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "bgsage")
        .unwrap();
    assert_eq!(sage["available"], true);
    assert_eq!(sage["plies"], json!([1, 2]));
    let mut invalid = changed;
    invalid["analysis_plies"] = json!(3);
    assert_eq!(call(app, "PUT", "/api/settings", invalid).await.0, 422);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL"]
async fn selected_results_never_cross_provider_and_legacy_remains_gnubg(pool: PgPool) {
    use uuid::Uuid;
    let app = api::router(AppState::new(pool.clone(), Engine::new("gnubg".into())));
    assert_eq!(call(app, "GET", "/api/settings", Value::Null).await.0, 200);
    let sgf = "(;GM[6]FF[4]MI[length:5][ws:0][bs:0];W[31hefe])";
    let imported = bgtutor::import::parse_sgf(sgf).unwrap();
    let saved =
        bgtutor::store::import_match(&pool, "provider.sgf", "sgf", sgf.as_bytes(), sgf, &imported)
            .await
            .unwrap();
    let position: Uuid = sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 LIMIT 1")
        .bind(saved.r#match.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let legacy = Uuid::new_v4();
    let sage = Uuid::new_v4();
    for (id, snapshot) in [(legacy, Value::Null), (sage, json!({"engine_id":"bgsage"}))] {
        sqlx::query("INSERT INTO jobs(id,kind,plies,status,engine_snapshot,position_uuid) VALUES($1,'analysis',2,'completed',$2,$3)")
            .bind(id).bind(if snapshot.is_null() {None} else {Some(snapshot)}).bind(position).execute(&pool).await.unwrap();
    }
    let selected: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM selected_engine_jobs")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(selected, vec![legacy]);
    sqlx::query("UPDATE app_settings SET analysis_engine='bgsage',analysis_plies=2")
        .execute(&pool)
        .await
        .unwrap();
    let selected: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM selected_engine_jobs")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(selected, vec![sage]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    sqlx::query("DELETE FROM app_settings")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM selected_engine_jobs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        legacy
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL and locked Open Sage installation"]
async fn sage_benchmark_snapshots_are_independent_and_complete(pool: PgPool) {
    let python = std::env::var("OPEN_SAGE_TEST_PYTHON").expect("explicit native interpreter");
    let state = AppState::new(pool.clone(), Engine::new("gnubg".into()));
    let app = api::router(state.clone());
    assert_eq!(
        call(
            app.clone(),
            "PUT",
            "/api/engines/bgsage",
            json!({"version":0,"path":python})
        )
        .await
        .0,
        200
    );
    let (status, runs) = call(
        app.clone(),
        "POST",
        "/api/benchmarks",
        json!({"engine_id":"bgsage","plies":[2]}),
    )
    .await;
    assert_eq!(status, 202, "{runs}");
    assert_eq!(runs[0]["engine_id"], "bgsage");
    assert_eq!(runs[0]["protocol"]["id"], "bgsage-match-v3-2ply-4threads");
    assert_eq!(
        call(app.clone(), "GET", "/api/settings", Value::Null)
            .await
            .1["analysis_engine"],
        "gnubg"
    );
    assert_eq!(
        call(
            app.clone(),
            "POST",
            "/api/benchmarks",
            json!({"engine_id":"bgsage","plies":[3]})
        )
        .await
        .0,
        422
    );
    assert!(bgtutor::benchmarks::run_one(&state).await.unwrap());
    let (_, history) = call(app.clone(), "GET", "/api/benchmarks", Value::Null).await;
    let run = &history["runs"][0];
    assert_eq!(run["status"], "completed", "{run}");
    assert_eq!(run["analyzed_moves"], 460);
    assert_eq!(run["completed_games"], 10);
    let startup = run["startup_elapsed_ms"].as_f64().unwrap();
    assert!(startup > 0.0 && run["total_elapsed_ms"].as_f64().unwrap() > startup);
    let benchmark_id = run["id"].clone();
    // The venv symlink identity must match both pre-job and running-job ETA.
    let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    settings["analysis_engine"] = json!("bgsage");
    settings["analysis_plies"] = json!(2);
    assert_eq!(
        call(app.clone(), "PUT", "/api/settings", settings).await.0,
        200
    );
    let sgf = "(;GM[6]FF[4]MI[length:5][ws:0][bs:0];W[31hefe])";
    let source = bgtutor::store::import_match(
        &pool,
        "eta.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &bgtutor::import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let uri = format!("/api/matches/{}/analysis", source.r#match.id);
    let (status, estimate) =
        call(app.clone(), "GET", &format!("{uri}/estimate"), Value::Null).await;
    assert_eq!(status, 200, "{estimate}");
    assert_eq!(estimate["benchmark"]["id"], benchmark_id, "{estimate}");
    let (status, job) = call(app.clone(), "POST", &uri, json!({})).await;
    assert_eq!(status, 202, "{job}");
    assert!(bgtutor::jobs::run_one(&state).await.unwrap());
    let job_id = uuid::Uuid::parse_str(job["id"].as_str().unwrap()).unwrap();
    let saved_id: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT benchmark_id FROM match_analysis_timing WHERE job_id=$1")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(saved_id.map(|v| json!(v)), Some(benchmark_id));

    let (status, play) = call(
        app.clone(),
        "POST",
        "/api/benchmarks/play",
        json!({"engine_id":"bgsage","plies":2}),
    )
    .await;
    assert_eq!(status, 202, "{play}");
    assert!(bgtutor::benchmarks::run_one(&state).await.unwrap());
    let (_, history) = call(app, "GET", "/api/benchmarks", Value::Null).await;
    let run = &history["runs"][0];
    assert_eq!(run["status"], "completed", "{run}");
    assert_eq!(run["completed_turns"], 11);
    assert!(run["average_move_ms"].is_null());
    assert!(run["average_turn_ms"].as_f64().unwrap() > 0.0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL, GNUbg and locked Open Sage"]
async fn jobs_pin_provider_across_settings_changes_and_dice_keep_36_weight(pool: PgPool) {
    use bgtutor::{import, jobs, store};
    let python = std::env::var("OPEN_SAGE_TEST_PYTHON").expect("explicit native interpreter");
    let state = AppState::new(pool.clone(), Engine::new("gnubg".into()));
    let app = api::router(state.clone());
    assert_eq!(
        call(
            app.clone(),
            "PUT",
            "/api/engines/bgsage",
            json!({"version":0,"path":python})
        )
        .await
        .0,
        200
    );
    let sgf = "(;GM[6]FF[4]MI[length:5][ws:0][bs:0];W[31hefe])";
    let imported = store::import_match(
        &pool,
        "snapshot.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let position: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 LIMIT 1")
            .bind(imported.r#match.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    settings["analysis_engine"] = json!("bgsage");
    settings["analysis_plies"] = json!(2);
    let (status, saved) = call(app.clone(), "PUT", "/api/settings", settings).await;
    assert_eq!(status, 200, "{saved}");
    let (status, sage) = call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":position,"kind":"analysis"}),
    )
    .await;
    assert_eq!(status, 202, "{sage}");
    let (status, dice) = call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":position,"kind":"dice"}),
    )
    .await;
    assert_eq!(status, 202, "{dice}");
    let mut settings = saved;
    settings["analysis_engine"] = json!("gnubg");
    settings["analysis_plies"] = json!(0);
    assert_eq!(
        call(app.clone(), "PUT", "/api/settings", settings).await.0,
        200
    );
    let (status, gnu) = call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":position,"kind":"analysis"}),
    )
    .await;
    assert_eq!(status, 202, "{gnu}");
    assert_ne!(sage["id"], gnu["id"]);
    for _ in 0..3 {
        assert!(jobs::run_one(&state).await.unwrap());
    }
    for (job, provider) in [(&sage, "bgsage"), (&gnu, "gnubg"), (&dice, "bgsage")] {
        let (_, done) = call(
            app.clone(),
            "GET",
            &format!("/api/jobs/{}", job["id"].as_str().unwrap()),
            Value::Null,
        )
        .await;
        assert_eq!(done["status"], "completed", "{done}");
        assert_eq!(done["engine_snapshot"]["engine_id"], provider);
        assert_eq!(done["engine_snapshot"], job["engine_snapshot"]);
        if job["kind"] == "dice" {
            let rolls = done["result"]["rolls"].as_array().unwrap();
            assert_eq!(rolls.len(), 21);
            assert_eq!(
                rolls
                    .iter()
                    .map(|r| r["weight"].as_i64().unwrap())
                    .sum::<i64>(),
                36
            );
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL; builds synthetic provider-selectivity fixture"]
async fn provider_lookup_uses_index_with_many_other_engine_results(pool: PgPool) {
    let state = AppState::new(pool.clone(), Engine::new("gnubg".into()));
    assert_eq!(
        call(api::router(state), "GET", "/api/settings", Value::Null)
            .await
            .0,
        200
    );
    let sgf = "(;GM[6]FF[4]MI[length:5][ws:0][bs:0];W[31hefe])";
    let saved = bgtutor::store::import_match(
        &pool,
        "performance.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &bgtutor::import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let position: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 LIMIT 1")
            .bind(saved.r#match.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE app_settings SET analysis_engine='bgsage',analysis_plies=2")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result,engine_snapshot) SELECT md5('provider-'||n)::uuid,$1,'analysis',CASE WHEN n=10000 THEN 2 ELSE 4 END,'completed','{}'::jsonb,CASE WHEN n=10000 THEN '{\"engine_id\":\"bgsage\"}'::jsonb END FROM generate_series(1,10000) n")
        .bind(position).execute(&pool).await.unwrap();
    sqlx::query("ANALYZE jobs").execute(&pool).await.unwrap();
    let plan:Value=sqlx::query_scalar("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) SELECT id,result FROM selected_engine_jobs WHERE position_uuid=$1 AND kind='analysis' AND status='completed' AND result IS NOT NULL ORDER BY plies DESC,created_at DESC,id DESC LIMIT 1")
        .bind(position).fetch_one(&pool).await.unwrap();
    assert!(
        plan.to_string().contains("jobs_engine_position_ready"),
        "{plan}"
    );
    assert_eq!(plan[0]["Plan"]["Actual Rows"], 1);
    println!(
        "provider lookup: 10000 synthetic jobs, one matching Sage result; database execution_ms={}, shared_hit_blocks={}",
        plan[0]["Execution Time"], plan[0]["Plan"]["Shared Hit Blocks"]
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL, GNUbg, fork and original Sage interpreters"]
async fn fork_depths_are_pinned_and_old_installation_remains_usable(pool: PgPool) {
    let old = std::env::var("OPEN_SAGE_ORIGINAL_PYTHON").unwrap();
    let fork = std::env::var("OPEN_SAGE_TEST_PYTHON").unwrap();
    let state = AppState::new(pool.clone(), Engine::new("gnubg".into()));
    let app = api::router(state.clone());
    let (status, initial) = call(
        app.clone(),
        "PUT",
        "/api/engines/bgsage",
        json!({"version":0,"path":old}),
    )
    .await;
    assert_eq!(status, 200, "{initial}");
    let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    settings["analysis_engine"] = json!("bgsage");
    settings["analysis_plies"] = json!(1);
    assert_eq!(
        call(app.clone(), "PUT", "/api/settings", settings.clone())
            .await
            .0,
        422
    );
    let (status, changed) = call(
        app.clone(),
        "PUT",
        "/api/engines/bgsage",
        json!({"version":1,"path":fork}),
    )
    .await;
    assert_eq!(status, 200, "{changed}");
    settings["play_engine"] = json!("bgsage");
    settings["play_plies"] = json!(1);
    assert_eq!(
        call(app.clone(), "PUT", "/api/settings", settings).await.0,
        200
    );
    let (status, session) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    assert_eq!(status, 201, "{session}");
    for side in ["bot", "analysis"] {
        assert_eq!(session["engine_snapshots"][side]["open_sage"]["plies"], 1);
        assert_eq!(
            session["engine_snapshots"][side]["open_sage"]["python"],
            fork
        );
    }
    let (status, series) = call(app.clone(), "POST", "/api/engine-matches", json!({"a":{"engine_id":"bgsage","plies":1},"b":{"engine_id":"bgsage","plies":2},"match_length":1,"count":1})).await;
    assert_eq!(status, 201, "{series}");
    let id = uuid::Uuid::parse_str(series["id"].as_str().unwrap()).unwrap();
    let engines: Value = sqlx::query_scalar("SELECT engines FROM engine_match_runs WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(engines["a"]["open_sage"]["plies"], 1);
    assert_eq!(engines["b"]["open_sage"]["plies"], 2);
    // Previously saved snapshots continue to use the old, unchanged installation.
    let previous: bgtutor::engines::SageSnapshot =
        serde_json::from_value(initial["snapshot"].clone()).unwrap();
    assert!(
        bgtutor::engines::SageEngine::new(previous)
            .unwrap()
            .version()
            .await
            .is_ok()
    );
    let (status, runs) = call(
        app.clone(),
        "POST",
        "/api/benchmarks",
        json!({"engine_id":"bgsage","plies":[1,2]}),
    )
    .await;
    assert_eq!(status, 202, "{runs}");
    for (index, depth) in [1, 2].into_iter().enumerate() {
        let id = uuid::Uuid::parse_str(runs[index]["id"].as_str().unwrap()).unwrap();
        let saved: Value =
            sqlx::query_scalar("SELECT engine_snapshot FROM benchmark_runs WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(saved["engine"]["open_sage"]["plies"], depth);
        assert_eq!(saved["engine"]["open_sage"]["python"], fork);
        assert_eq!(
            runs[index]["protocol"]["id"],
            format!("bgsage-match-v3-{depth}ply-4threads")
        );
    }
    // Exercise both complete analysis depths through the durable benchmark worker.
    for _ in 0..2 {
        assert!(bgtutor::benchmarks::run_one(&state).await.unwrap());
    }
    let complete: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM benchmark_runs WHERE status='completed' AND analyzed_moves=460",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(complete, 2);
    let (status, play) = call(
        app.clone(),
        "POST",
        "/api/benchmarks/play",
        json!({"engine_id":"bgsage","plies":1}),
    )
    .await;
    assert_eq!(status, 202, "{play}");
    assert!(bgtutor::benchmarks::run_one(&state).await.unwrap());
    let (_, history) = call(app.clone(), "GET", "/api/benchmarks", Value::Null).await;
    assert_eq!(history["runs"][0]["status"], "completed", "{history}");
    assert_eq!(history["runs"][0]["completed_turns"], 11);
    let sgf = "(;GM[6]FF[4]MI[length:5][ws:0][bs:0];W[31hefe])";
    let source = bgtutor::store::import_match(
        &pool,
        "depth.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &bgtutor::import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let uri = format!("/api/matches/{}/analysis", source.r#match.id);
    let (status, job) = call(app.clone(), "POST", &uri, json!({"plies":2})).await;
    assert_eq!(status, 202, "{job}");
    let job_id = uuid::Uuid::parse_str(job["id"].as_str().unwrap()).unwrap();
    let saved: Value = sqlx::query_scalar("SELECT engine_snapshot FROM jobs WHERE id=$1")
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        saved["open_sage"]["plies"], 2,
        "request overrides the 1-ply default"
    );
    assert!(bgtutor::jobs::run_one(&state).await.unwrap());
    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id=$1")
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "completed");
}
