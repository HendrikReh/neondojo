use axum::{body::Body, http::Request};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
    import, jobs, store,
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
    (status, serde_json::from_slice(&bytes).unwrap())
}
fn binary() -> String {
    std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())
}
#[cfg(unix)]
fn absolute_binary() -> std::path::PathBuf {
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};
    let path = PathBuf::from(binary());
    let path = if path.components().count() == 1 {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|directory| directory.join(&path))
            .find(|candidate| {
                candidate.is_file()
                    && candidate
                        .metadata()
                        .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
            })
            .expect("GNUbg must be available on PATH")
    } else {
        path
    };
    std::fs::canonicalize(path).expect("GNUbg executable must exist")
}

async fn imported(pool: &PgPool) -> bgtutor::model::ImportResponse {
    let sgf = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][game:0][ws:0][bs:0];W[31hefe])";
    store::import_match(
        pool,
        "settings.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn defaults_use_injected_engine_and_survive_new_app_states(pool: PgPool) {
    let app = api::router(AppState::new(
        pool.clone(),
        Engine::new("/missing/injected-gnubg".into()),
    ));
    let (status, settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    assert_eq!(status, 200, "{settings}");
    assert_eq!(
        settings,
        json!({"version":0,"player_name":"","analysis_engine":"gnubg","analysis_plies":2,"play_engine":"gnubg","play_plies":0,"gnubg_path":"/missing/injected-gnubg","bearoff_direction":"right","default_play_mode":"coach","tutor_threshold":0.04,"training_auto_analysis":true})
    );
    let second = api::router(AppState::new(pool.clone(), Engine::new("different".into())));
    assert_eq!(
        call(second, "GET", "/api/settings", Value::Null).await.1,
        settings
    );
    let (_, engines) = call(app.clone(), "GET", "/api/engines", Value::Null).await;
    assert_eq!(engines[0]["id"], "gnubg");
    assert_eq!(engines[0]["available"], false);
    assert_eq!(engines[0]["version"], Value::Null);
    assert_eq!(engines[0]["plies"], json!([0, 1, 2, 3, 4]));
    assert_eq!(
        engines[1],
        json!({"id":"bgsage","name":"BGSage","available":false,"version":null,"plies":[]})
    );
    let (_, schema) = call(app, "GET", "/api/openapi.json", Value::Null).await;
    for route in ["/api/settings", "/api/engines", "/api/engines/gnubg/check"] {
        assert!(schema["paths"][route].is_object(), "{route}");
    }
    assert!(schema["components"]["schemas"]["AppSettings"].is_object());
    for name in ["JobRequest", "MatchAnalysisRequest", "Job"] {
        assert_eq!(
            schema["components"]["schemas"][name]["properties"]["plies"]["maximum"], 4,
            "{name}"
        );
    }
    assert_eq!(
        schema["components"]["schemas"]["AppSettings"]["properties"]["analysis_plies"]["maximum"],
        4
    );
    assert_eq!(
        schema["components"]["schemas"]["AppSettings"]["properties"]["play_plies"]["maximum"],
        2
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn analysis_depth_applies_to_position_dice_game_and_match_jobs(pool: PgPool) {
    let imported = imported(&pool).await;
    let position = store::positions(&pool, Some(imported.r#match.id), None, None, 10)
        .await
        .unwrap()
        .items
        .remove(0);
    let app = api::router(AppState::new(pool, Engine::new("/missing/gnubg".into())));
    let endpoint = format!("/api/matches/{}/analysis", imported.r#match.id);
    for (uri, body) in [
        (
            "/api/jobs",
            json!({"position_id":position.id,"kind":"analysis"}),
        ),
        (
            "/api/jobs",
            json!({"position_id":position.id,"kind":"dice"}),
        ),
        (endpoint.as_str(), json!({})),
        (endpoint.as_str(), json!({"game_no":1})),
    ] {
        let (status, default) = call(app.clone(), "POST", uri, body.clone()).await;
        assert_eq!(status, 202, "{default}");
        assert_eq!(default["plies"], 2);
        for plies in 0..=4 {
            let mut explicit = body.clone();
            explicit["plies"] = json!(plies);
            let (status, job) = call(app.clone(), "POST", uri, explicit).await;
            assert_eq!(status, 202, "{job}");
            assert_eq!(job["plies"], plies);
        }
        for plies in [-1, 5] {
            let mut invalid = body.clone();
            invalid["plies"] = json!(plies);
            assert_eq!(call(app.clone(), "POST", uri, invalid).await.0, 422);
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn depth_migration_changes_only_untouched_defaults_and_preserves_old_jobs(pool: PgPool) {
    let imported = imported(&pool).await;
    let position = store::positions(&pool, Some(imported.r#match.id), None, None, 10)
        .await
        .unwrap()
        .items
        .remove(0);
    let old = store::create_job(&pool, position.id, "analysis", 0)
        .await
        .unwrap();
    let snapshot = json!({"engine_id":"gnubg","gnubg_path":"/legacy/gnubg","engine_version":"GNU Backgammon legacy"});
    let result = json!({"engine":"GNU Backgammon legacy","plies":0,"candidates":[]});
    sqlx::query("UPDATE jobs SET status='completed',engine_snapshot=$2,result=$3 WHERE id=$1")
        .bind(old.id)
        .bind(&snapshot)
        .bind(&result)
        .execute(&pool)
        .await
        .unwrap();
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(jobs) FROM jobs WHERE id=$1")
        .bind(old.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    // Reproduce the pre-upgrade schema inside this isolated test database.
    sqlx::raw_sql("ALTER TABLE jobs DROP CONSTRAINT jobs_plies_check; ALTER TABLE jobs ADD CONSTRAINT jobs_plies_check CHECK (plies BETWEEN 0 AND 2); ALTER TABLE app_settings DROP CONSTRAINT app_settings_analysis_plies_check; ALTER TABLE app_settings ADD CONSTRAINT app_settings_analysis_plies_check CHECK (analysis_plies BETWEEN 0 AND 2); ALTER TABLE app_settings ALTER COLUMN analysis_plies SET DEFAULT 0;")
        .execute(&pool).await.unwrap();
    let migration = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/0019_analysis_depth.sql"
    ))
    .expect("analysis-depth migration must exist");
    for (version, plies, expected_plies, expected_version) in [
        (0i64, 0, 2, 1i64),
        (1, 0, 0, 1),
        (0, 1, 1, 0),
        (0, 2, 2, 0),
        (4, 2, 2, 4),
    ] {
        sqlx::query("DELETE FROM app_settings")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO app_settings(singleton,version,analysis_plies,play_plies,player_name,gnubg_path,gnubg_version) VALUES(true,$1,$2,1,'Saved name','/saved/gnubg','GNU Backgammon saved')")
            .bind(version).bind(plies).execute(&pool).await.unwrap();
        sqlx::raw_sql(&migration).execute(&pool).await.unwrap();
        let settings: Value = sqlx::query_scalar("SELECT to_jsonb(app_settings) FROM app_settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(settings["analysis_plies"], expected_plies);
        assert_eq!(settings["version"], expected_version);
        assert_eq!(settings["play_plies"], 1);
        assert_eq!(settings["player_name"], "Saved name");
        assert_eq!(settings["gnubg_path"], "/saved/gnubg");
        assert_eq!(settings["gnubg_version"], "GNU Backgammon saved");
    }
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(jobs) FROM jobs WHERE id=$1")
        .bind(old.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        before, after,
        "migration must not rewrite old jobs or their provenance"
    );
    sqlx::query("DELETE FROM app_settings")
        .execute(&pool)
        .await
        .unwrap();
    let default: i32 = sqlx::query_scalar(
        "INSERT INTO app_settings(gnubg_path) VALUES('/new/gnubg') RETURNING analysis_plies",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(default, 2);
    assert!(
        store::create_job(&pool, position.id, "analysis", 3)
            .await
            .is_ok()
    );
    assert!(
        store::create_job(&pool, position.id, "analysis", 4)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE app_settings SET analysis_plies=3")
            .execute(&pool)
            .await
            .is_ok()
    );
    assert!(
        sqlx::query("UPDATE app_settings SET analysis_plies=4")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE app_settings SET play_plies=3")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn four_ply_migration_preserves_saved_profiles_and_completed_jobs(pool: PgPool) {
    let imported = imported(&pool).await;
    let position = store::positions(&pool, Some(imported.r#match.id), None, None, 10)
        .await
        .unwrap()
        .items
        .remove(0);
    for plies in 0..=3 {
        let old = store::create_job(&pool, position.id, "analysis", plies)
            .await
            .unwrap();
        sqlx::query("UPDATE jobs SET status='completed',engine_snapshot=$2,result=$3 WHERE id=$1")
            .bind(old.id)
            .bind(json!({"engine_id":"gnubg","gnubg_path":"/saved/gnubg","engine_version":"GNU Backgammon saved"}))
            .bind(json!({"engine":"GNU Backgammon saved","plies":plies,"candidates":[]}))
            .execute(&pool).await.unwrap();
    }
    let jobs_before: Value =
        sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(jobs) ORDER BY id) FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::raw_sql("ALTER TABLE jobs DROP CONSTRAINT jobs_plies_check; ALTER TABLE jobs ADD CONSTRAINT jobs_plies_check CHECK (plies BETWEEN 0 AND 3); ALTER TABLE app_settings DROP CONSTRAINT app_settings_analysis_plies_check; ALTER TABLE app_settings ADD CONSTRAINT app_settings_analysis_plies_check CHECK (analysis_plies BETWEEN 0 AND 3);")
        .execute(&pool).await.unwrap();
    let migration = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/0020_four_ply.sql"
    ))
    .unwrap();
    for (version, plies) in [(0i64, 0), (3, 0), (5, 1), (7, 2), (9, 3)] {
        sqlx::query("DELETE FROM app_settings")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO app_settings(version,analysis_plies,play_plies,player_name,gnubg_path,gnubg_version) VALUES($1,$2,2,'Saved name','/saved/gnubg','GNU Backgammon saved')")
            .bind(version).bind(plies).execute(&pool).await.unwrap();
        let before: Value = sqlx::query_scalar("SELECT to_jsonb(app_settings) FROM app_settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(&migration).execute(&pool).await.unwrap();
        let after: Value = sqlx::query_scalar("SELECT to_jsonb(app_settings) FROM app_settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            before, after,
            "4-ply must not change a saved profile or its revision"
        );
    }
    let jobs_after: Value =
        sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(jobs) ORDER BY id) FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(jobs_before, jobs_after);
    sqlx::query("DELETE FROM app_settings")
        .execute(&pool)
        .await
        .unwrap();
    let default: i32 = sqlx::query_scalar(
        "INSERT INTO app_settings(gnubg_path) VALUES('/new/gnubg') RETURNING analysis_plies",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(default, 2);
    assert!(
        store::create_job(&pool, position.id, "analysis", 4)
            .await
            .is_ok()
    );
    assert!(
        store::create_job(&pool, position.id, "analysis", 5)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE app_settings SET analysis_plies=4")
            .execute(&pool)
            .await
            .is_ok()
    );
    assert!(
        sqlx::query("UPDATE app_settings SET analysis_plies=5")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE app_settings SET play_plies=3")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn validates_installation_updates_atomically_and_rejects_stale_writes(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), Engine::new(binary())));
    let (_, initial) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    let (status, checked) = call(
        app.clone(),
        "POST",
        "/api/engines/gnubg/check",
        json!({"path":binary()}),
    )
    .await;
    assert_eq!(status, 200, "{checked}");
    assert!(
        checked["version"]
            .as_str()
            .unwrap()
            .contains("GNU Backgammon")
    );
    let (_, available) = call(app.clone(), "GET", "/api/engines", Value::Null).await;
    assert_eq!(available[0]["available"], true);
    assert!(available[0]["version"].is_string());
    let mut update = initial.clone();
    update["player_name"] = json!("Hendrik");
    update["analysis_plies"] = json!(4);
    update["play_plies"] = json!(2);
    let (status, saved) = call(app.clone(), "PUT", "/api/settings", update.clone()).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["version"], 1);
    assert_eq!(saved["player_name"], "Hendrik");
    assert_eq!(
        call(app.clone(), "PUT", "/api/settings", update).await.0,
        409
    );
    for (field, value) in [
        ("analysis_engine", json!("bgsage")),
        ("play_engine", json!("unknown")),
        ("analysis_plies", json!(5)),
        ("analysis_plies", json!(-1)),
        ("play_plies", json!(3)),
        ("play_plies", json!(-1)),
        ("player_name", json!("a\nname")),
        ("player_name", json!("x".repeat(121))),
        ("gnubg_path", json!("")),
        ("gnubg_path", json!("/missing/gnubg")),
        ("gnubg_path", json!("/bin/true")),
    ] {
        let mut invalid = saved.clone();
        invalid[field] = value;
        let (status, error) = call(app.clone(), "PUT", "/api/settings", invalid).await;
        assert_eq!(status, 422, "{field}: {error}");
        assert_eq!(
            call(app.clone(), "GET", "/api/settings", Value::Null)
                .await
                .1,
            saved
        );
    }
    let mut first = saved.clone();
    first["player_name"] = json!("First");
    let mut second = saved.clone();
    second["player_name"] = json!("Second");
    let (a, b) = tokio::join!(
        call(app.clone(), "PUT", "/api/settings", first),
        call(app.clone(), "PUT", "/api/settings", second)
    );
    assert!(
        (a.0 == 200 && b.0 == 409) || (a.0 == 409 && b.0 == 200),
        "{a:?} {b:?}"
    );
    let reopened = api::router(AppState::new(
        pool,
        Engine::new("/missing/new-default".into()),
    ));
    assert_eq!(
        call(reopened, "GET", "/api/settings", Value::Null).await.1["version"],
        2
    );
}

#[cfg(unix)]
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn configured_relative_gnubg_path_is_shared_by_analysis_play_and_benchmark(pool: PgPool) {
    let app = api::router(AppState::new(
        pool.clone(),
        Engine::new("/missing/default".into()),
    ));
    let directory = tempfile::tempdir_in(".").unwrap();
    let executable = absolute_binary();
    let alias = directory.path().join("GNUbg für Tests");
    std::os::unix::fs::symlink(&executable, &alias).unwrap();
    let relative_path = std::path::Path::new(directory.path().file_name().unwrap())
        .join(alias.file_name().unwrap());
    let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    settings["gnubg_path"] = json!(relative_path.to_str().unwrap());
    let (status, saved) = call(app.clone(), "PUT", "/api/settings", settings).await;
    assert_eq!(status, 200, "{saved}");

    let analysis = Engine::new(saved["gnubg_path"].as_str().unwrap().into())
        .analyze("4HPwATDgc/ABMA", "cInlAAAAAAAE", 0, None)
        .await
        .unwrap();
    assert!(!analysis.candidates.is_empty());

    let (status, session) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"human_player":0,"match_length":5}),
    )
    .await;
    assert_eq!(status, 201, "{session}");
    let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
    let play_path: String = sqlx::query_scalar("SELECT gnubg_path FROM play_sessions WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();

    let (status, benchmark) = call(
        app,
        "POST",
        "/api/benchmarks",
        json!({"engine_id":"gnubg","plies":[0]}),
    )
    .await;
    assert_eq!(status, 202, "{benchmark}");
    assert_eq!(play_path, executable.to_str().unwrap());
    assert_eq!(benchmark[0]["engine_path"], play_path);
    assert_eq!(benchmark[0]["engine_version"], session["engine"]["version"]);
}

#[cfg(unix)]
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg with embedded Python"]
async fn play_latency_uses_the_saved_play_depth_and_installation(pool: PgPool) {
    let state = AppState::new(pool.clone(), Engine::new("/missing/backend-default".into()));
    let app = api::router(state.clone());
    call(app.clone(), "GET", "/api/settings", Value::Null).await;
    sqlx::query("UPDATE app_settings SET play_plies=1,analysis_plies=4,gnubg_path=$1")
        .bind(binary())
        .execute(&pool)
        .await
        .unwrap();
    let (status, queued) = call(app.clone(), "POST", "/api/benchmarks/play", Value::Null).await;
    assert_eq!(status, 202, "{queued}");
    assert_eq!(queued["plies"], 1);
    assert!(queued["reference"].is_null());
    assert!(queued["play_reference"]["total_turns"].as_i64().unwrap() > 1);
    assert!(queued["average_turn_ms"].is_null());
    let executable = absolute_binary();
    assert_eq!(queued["engine_path"], executable.to_str().unwrap());

    sqlx::query("UPDATE app_settings SET play_plies=0,gnubg_path='/missing/after-queueing'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(bgtutor::benchmarks::run_one(&state).await.unwrap());
    let (_, history) = call(app, "GET", "/api/benchmarks", Value::Null).await;
    let run = &history["runs"][0];
    assert_eq!(run["id"], queued["id"]);
    assert_eq!(run["status"], "completed", "{run}");
    assert_eq!(run["plies"], 1);
    assert_eq!(run["engine_path"], queued["engine_path"]);
    assert_eq!(
        run["completed_turns"],
        queued["play_reference"]["total_turns"]
    );
    let total = run["total_elapsed_ms"].as_f64().unwrap();
    let mean = run["average_turn_ms"].as_f64().unwrap();
    assert!(mean > 0.0 && (mean - total / run["completed_turns"].as_f64().unwrap()).abs() < 1e-9);
    assert!(run["average_move_ms"].is_null());
    assert_eq!(run["analyzed_moves"], 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM play_sessions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn jobs_use_saved_defaults_snapshot_installations_and_deduplicate_by_configuration(
    pool: PgPool,
) {
    let imported = imported(&pool).await;
    let position = store::positions(&pool, Some(imported.r#match.id), None, None, 10)
        .await
        .unwrap()
        .items
        .remove(0);
    let state = AppState::new(pool.clone(), Engine::new(binary()));
    let app = api::router(state.clone());
    let (_, old) = call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":position.id,"kind":"analysis"}),
    )
    .await;
    assert_eq!(old["plies"], 2);
    let endpoint = format!("/api/matches/{}/analysis", imported.r#match.id);
    let (_, old_full) = call(app.clone(), "POST", &endpoint, json!({"plies":0})).await;
    let (_, old_game) = call(
        app.clone(),
        "POST",
        &endpoint,
        json!({"plies":0,"game_no":1}),
    )
    .await;
    let temp = tempfile::tempdir().unwrap();
    let alternative = temp.path().join("gnubg-alternative");
    std::os::unix::fs::symlink(absolute_binary(), &alternative).unwrap();
    let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    settings["gnubg_path"] = json!(alternative.to_str().unwrap());
    settings["analysis_plies"] = json!(1);
    let (status, saved) = call(app.clone(), "PUT", "/api/settings", settings).await;
    assert_eq!(status, 200, "{saved}");
    let (_, explicit) = call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":position.id,"kind":"analysis","plies":0}),
    )
    .await;
    assert_eq!(explicit["plies"], 0);
    assert_ne!(
        old["id"], explicit["id"],
        "old installation must not deduplicate a new job"
    );
    let (_, duplicate) = call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":position.id,"kind":"analysis","plies":0}),
    )
    .await;
    assert_eq!(explicit["id"], duplicate["id"]);
    let (_, default) = call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":position.id,"kind":"analysis"}),
    )
    .await;
    assert_eq!(default["plies"], 1);
    let (_, full) = call(app.clone(), "POST", &endpoint, json!({})).await;
    assert_eq!(full["plies"], 1);
    let (_, full_explicit) = call(app.clone(), "POST", &endpoint, json!({"plies":0})).await;
    let (_, game_explicit) = call(
        app.clone(),
        "POST",
        &endpoint,
        json!({"plies":0,"game_no":1}),
    )
    .await;
    assert_ne!(old_full["id"], full_explicit["id"]);
    assert_ne!(old_game["id"], game_explicit["id"]);
    let (_, full_duplicate) = call(app.clone(), "POST", &endpoint, json!({"plies":0})).await;
    let (_, game_duplicate) = call(
        app.clone(),
        "POST",
        &endpoint,
        json!({"plies":0,"game_no":1}),
    )
    .await;
    assert_eq!(full_explicit["id"], full_duplicate["id"]);
    assert_eq!(game_explicit["id"], game_duplicate["id"]);
    // Non-engine preferences do not change the active job identity.
    let mut renamed = saved;
    renamed["player_name"] = json!("Changed name");
    assert_eq!(
        call(app.clone(), "PUT", "/api/settings", renamed).await.0,
        200
    );
    let (_, unchanged_config) = call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":position.id,"kind":"analysis","plies":0}),
    )
    .await;
    assert_eq!(unchanged_config["id"], explicit["id"]);
    let old_id: uuid::Uuid = serde_json::from_value(old["id"].clone()).unwrap();
    sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()-interval '1 second' WHERE id=$1").bind(old_id).bind(uuid::Uuid::new_v4()).execute(&pool).await.unwrap();
    let changed_process = AppState::new(
        pool.clone(),
        Engine::new("/missing/fallback-must-not-be-used".into()),
    );
    let old_match_id: uuid::Uuid = serde_json::from_value(old_full["id"].clone()).unwrap();
    sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()-interval '1 second' WHERE id=$1").bind(old_match_id).bind(uuid::Uuid::new_v4()).execute(&pool).await.unwrap();
    for _ in 0..8 {
        assert!(jobs::run_one(&changed_process).await.unwrap());
    }
    for submitted in [
        &old,
        &old_full,
        &old_game,
        &explicit,
        &default,
        &full,
        &full_explicit,
        &game_explicit,
    ] {
        let (_, done) = call(
            app.clone(),
            "GET",
            &format!("/api/jobs/{}", submitted["id"].as_str().unwrap()),
            Value::Null,
        )
        .await;
        assert_eq!(done["status"], "completed", "{done}");
    }
    let (_, done) = call(app, "GET", &format!("/api/jobs/{old_id}"), Value::Null).await;
    assert_eq!(done["attempts"], 2);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn mat_import_uses_current_saved_installation(pool: PgPool) {
    let app = api::router(AppState::new(pool, Engine::new("/missing/initial".into())));
    let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    settings["gnubg_path"] = json!(binary());
    let (status, saved) = call(app.clone(), "PUT", "/api/settings", settings).await;
    assert_eq!(status, 200, "{saved}");
    let mat = "7 point match\n\n Game 1\n Alice : 0                         Bob : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n";
    let body = format!(
        "--bg\r\nContent-Disposition: form-data; name=\"file\"; filename=\"current.mat\"\r\n\r\n{mat}\r\n--bg--\r\n"
    );
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/imports")
                .header("content-type", "multipart/form-data; boundary=bg")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let data = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(status, 201, "{}", String::from_utf8_lossy(&data));
}

#[cfg(unix)]
fn fixture_engine(script: &str) -> (tempfile::TempDir, Engine) {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("fixture-gnubg");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let engine = Engine::new(path.to_string_lossy().into_owned());
    (temp, engine)
}

#[cfg(unix)]
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg with weights"]
async fn three_ply_native_jobs_preserve_real_search_depth(pool: PgPool) {
    native_jobs_preserve_real_search_depth(pool, 3).await;
}

#[cfg(unix)]
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg with weights"]
async fn four_ply_native_jobs_preserve_real_search_depth(pool: PgPool) {
    native_jobs_preserve_real_search_depth(pool, 4).await;
}

#[cfg(unix)]
async fn native_jobs_preserve_real_search_depth(pool: PgPool, plies: i32) {
    let evidence = tempfile::tempdir().unwrap();
    let quote = |value: &str| format!("'{}'", value.replace('\'', "'\\''"));
    let native = quote(absolute_binary().to_str().unwrap());
    let output_log = evidence.path().join("native-output.txt");
    let export_log = evidence.path().join("native-game-export.txt");
    // Capture the real native output and exported depth, rather than trusting
    // the requested depth echoed into our result metadata.
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then exec {native} \"$@\"; fi\n{native} \"$@\" >native-output.txt\nstatus=$?\ncat native-output.txt\ncat native-output.txt >>{}\nif [ -f analyzed.txt ]; then cat analyzed.txt >>{}; fi\nexit \"$status\"\n",
        quote(output_log.to_str().unwrap()),
        quote(export_log.to_str().unwrap())
    );
    let (_wrapper, engine) = fixture_engine(&script);
    // Opening 51 has close candidate moves; GNUbg's normal pruning can stop
    // obvious choices such as opening 31 below the requested maximum depth.
    let sgf = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][game:0][ws:0][bs:0];W[51xwmh])";
    let parsed = import::parse_sgf(sgf).unwrap();
    let imported = store::import_match(
        &pool,
        &format!("{plies}-ply.sgf"),
        "sgf",
        sgf.as_bytes(),
        sgf,
        &parsed,
    )
    .await
    .unwrap();
    let position = store::positions(&pool, Some(imported.r#match.id), None, None, 10)
        .await
        .unwrap()
        .items
        .remove(0);
    let analysis = engine
        .analyze(&position.position_id, &position.gnubg_match_id, plies, None)
        .await
        .unwrap();
    assert_eq!(analysis.plies, plies);
    assert!(!analysis.candidates.is_empty());
    assert!(
        std::fs::read_to_string(&output_log)
            .unwrap()
            .contains(&format!("Cubeful {plies}-ply"))
    );
    let game = engine.analyze_game(sgf, 1, plies).await.unwrap();
    assert_eq!(game.decisions.len(), 1);
    assert!(
        std::fs::read_to_string(&export_log)
            .unwrap()
            .contains(&format!("{plies}-ply"))
    );

    let state = AppState::new(pool.clone(), engine);
    let app = api::router(state.clone());
    let endpoint = format!("/api/matches/{}/analysis", imported.r#match.id);
    for (uri, body, kind) in [
        (
            "/api/jobs",
            json!({"position_id":position.id,"kind":"analysis","plies":plies}),
            "analysis",
        ),
        (
            "/api/jobs",
            json!({"position_id":position.id,"kind":"dice","plies":plies}),
            "dice",
        ),
        (endpoint.as_str(), json!({"plies":plies}), "match"),
        (
            endpoint.as_str(),
            json!({"game_no":1,"plies":plies}),
            "match",
        ),
    ] {
        std::fs::write(&output_log, "").unwrap();
        std::fs::write(&export_log, "").unwrap();
        let (status, submitted) = call(app.clone(), "POST", uri, body).await;
        assert_eq!(status, 202, "{submitted}");
        assert!(jobs::run_one(&state).await.unwrap());
        let (status, done) = call(
            app.clone(),
            "GET",
            &format!("/api/jobs/{}", submitted["id"].as_str().unwrap()),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(done["status"], "completed", "{done}");
        assert_eq!(done["plies"], plies);
        assert_eq!(done["result"]["plies"], plies);
        if kind == "match" {
            assert_eq!(done["completed_games"], 1);
            assert!(
                std::fs::read_to_string(&export_log)
                    .unwrap()
                    .contains(&format!("{plies}-ply"))
            );
        } else {
            let native_output = std::fs::read_to_string(&output_log).unwrap();
            assert!(native_output.contains(&format!("Cubeful {plies}-ply")));
            if kind == "dice" {
                assert_eq!(done["result"]["rolls"].as_array().unwrap().len(), 21);
                assert_eq!(native_output.matches("Setting GNUbg ID").count(), 21);
                let weight: i64 = done["result"]["rolls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| r["weight"].as_i64().unwrap())
                    .sum();
                assert_eq!(weight, 36);
            }
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn installation_check_rejects_an_executable_without_evaluation_data() {
    let (_temp, engine) = fixture_engine(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'GNU Backgammon fixture'; exit 0; fi\necho 'No evaluation weights found'\n",
    );
    assert_eq!(engine.version().await.unwrap(), "GNU Backgammon fixture");
    assert!(engine.check_installation().await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn installation_check_bounds_a_hanging_evaluation() {
    use std::time::{Duration, Instant};
    let (_temp, engine) = fixture_engine(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'GNU Backgammon fixture'; exit 0; fi\nexec sleep 60\n",
    );
    let started = Instant::now();
    let error = engine.check_installation().await.unwrap_err();
    assert!(error.to_string().contains("timed out"), "{error:#}");
    assert!(started.elapsed() < Duration::from_secs(12));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn initial_jobs_capture_version_without_settings_reads_validating_the_engine(pool: PgPool) {
    let imported = imported(&pool).await;
    let position = store::positions(&pool, Some(imported.r#match.id), None, None, 10)
        .await
        .unwrap()
        .items
        .remove(0);
    let app = api::router(AppState::new(pool.clone(), Engine::new(binary())));
    let (_, first) = call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":position.id,"kind":"analysis"}),
    )
    .await;
    let id: uuid::Uuid = serde_json::from_value(first["id"].clone()).unwrap();
    let snapshot: Value = sqlx::query_scalar("SELECT engine_snapshot FROM jobs WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        snapshot["engine_version"],
        Engine::new(binary()).version().await.unwrap()
    );
    let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    settings["player_name"] = json!("Name only");
    assert_eq!(
        call(app.clone(), "PUT", "/api/settings", settings).await.0,
        200
    );
    let (_, second) = call(
        app,
        "POST",
        "/api/jobs",
        json!({"position_id":position.id,"kind":"analysis"}),
    )
    .await;
    assert_eq!(
        first["id"], second["id"],
        "saving only a player name must not change engine identity"
    );
}

#[cfg(unix)]
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn jobs_detect_an_engine_version_change_at_the_same_path(pool: PgPool) {
    let (temp, engine) = fixture_engine("#!/bin/sh\necho 'GNU Backgammon fixture-A'\n");
    let imported = imported(&pool).await;
    let position = store::positions(&pool, Some(imported.r#match.id), None, None, 10)
        .await
        .unwrap()
        .items
        .remove(0);
    let app = api::router(AppState::new(pool, engine));
    let (_, first) = call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":position.id,"kind":"analysis"}),
    )
    .await;
    std::fs::write(
        temp.path().join("fixture-gnubg"),
        "#!/bin/sh\necho 'GNU Backgammon fixture-B'\n",
    )
    .unwrap();
    let (_, second) = call(
        app,
        "POST",
        "/api/jobs",
        json!({"position_id":position.id,"kind":"analysis"}),
    )
    .await;
    assert_ne!(
        first["id"], second["id"],
        "another GNUbg version must not reuse an active old-version job"
    );
}

#[cfg(unix)]
fn versioned_fixture_engine(change_during_evaluation: bool) -> (tempfile::TempDir, Engine) {
    let change = if change_during_evaluation {
        "printf 'GNU Backgammon fixture-B\\n' >\"$(dirname \"$0\")/version\"\n"
    } else {
        ""
    };
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then cat \"$(dirname \"$0\")/version\"; exit 0; fi\n{change}cat <<'HINTS'\nGNU Backgammon fixture-A\n    1. Cubeful 0-ply    8/5 6/5                      Eq.: +0.219\n       0.551 0.174 0.013 - 0.449 0.124 0.005\nHINTS\n"
    );
    let (temp, engine) = fixture_engine(&script);
    std::fs::write(temp.path().join("version"), "GNU Backgammon fixture-A\n").unwrap();
    (temp, engine)
}

#[cfg(unix)]
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn queued_and_recovered_jobs_refuse_a_replaced_engine_version(pool: PgPool) {
    let (temp, engine) = versioned_fixture_engine(false);
    let imported = imported(&pool).await;
    let position = store::positions(&pool, Some(imported.r#match.id), None, None, 10)
        .await
        .unwrap()
        .items
        .remove(0);
    let state = AppState::new(pool.clone(), engine);
    let app = api::router(state.clone());
    let mut submitted = Vec::new();
    for kind in ["analysis", "dice"] {
        let (status, job) = call(
            app.clone(),
            "POST",
            "/api/jobs",
            json!({"position_id":position.id,"kind":kind}),
        )
        .await;
        assert_eq!(status, 202, "{job}");
        submitted.push(job);
    }
    let (status, recovered) = call(
        app.clone(),
        "POST",
        &format!("/api/matches/{}/analysis", imported.r#match.id),
        json!({}),
    )
    .await;
    assert_eq!(status, 202, "{recovered}");
    let id: uuid::Uuid = serde_json::from_value(recovered["id"].clone()).unwrap();
    sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()-interval '1 second' WHERE id=$1").bind(id).bind(uuid::Uuid::new_v4()).execute(&pool).await.unwrap();
    submitted.push(recovered);
    std::fs::write(temp.path().join("version"), "GNU Backgammon fixture-B\n").unwrap();
    for job in submitted {
        assert!(jobs::run_one(&state).await.unwrap());
        let (_, done) = call(
            app.clone(),
            "GET",
            &format!("/api/jobs/{}", job["id"].as_str().unwrap()),
            Value::Null,
        )
        .await;
        assert_eq!(done["status"], "failed", "{done}");
        let error = done["error"].as_str().unwrap();
        assert!(
            error.contains("version changed")
                && error.contains("fixture-A")
                && error.contains("fixture-B"),
            "{done}"
        );
        assert!(done["result"].is_null(), "{done}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn snapshot_rejects_output_if_the_version_changes_during_evaluation() {
    let (_temp, engine) = versioned_fixture_engine(true);
    let snapshot = bgtutor::settings::EngineSnapshot {
        engine_id: "gnubg".into(),
        gnubg_path: engine.binary().into(),
        engine_version: Some(engine.version().await.unwrap()),
    };
    let result = snapshot
        .engine()
        .unwrap()
        .analyze("4HPwATDgc/ABMA", "cInlAAAAAAAE", 0, None)
        .await;
    assert!(
        result.is_err(),
        "an output spanning a version change must be discarded"
    );
    let error = format!("{:#}", result.unwrap_err());
    assert!(error.contains("version changed"), "{error}");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and local GNUbg"]
async fn bearoff_direction_is_persisted_validated_and_defaulted(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), Engine::new(binary())));
    let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    assert_eq!(settings["bearoff_direction"], "right");
    for direction in ["left", "right"] {
        settings["bearoff_direction"] = json!(direction);
        let (status, saved) = call(app.clone(), "PUT", "/api/settings", settings.clone()).await;
        assert_eq!(status, 200, "{saved}");
        assert_eq!(saved["bearoff_direction"], direction);
        settings = saved;
    }
    let mut invalid = settings.clone();
    invalid["bearoff_direction"] = json!("sideways");
    assert_eq!(
        call(app.clone(), "PUT", "/api/settings", invalid).await.0,
        422
    );
    assert_eq!(
        call(app, "GET", "/api/settings", Value::Null).await.1,
        settings
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn playing_mode_defaults_roundtrip_and_invalid_thresholds_are_atomic(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), Engine::new(binary())));
    let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    for mode in ["competition", "training", "coach", "tutor"] {
        settings["default_play_mode"] = json!(mode);
        settings["tutor_threshold"] = json!(0.123);
        settings["training_auto_analysis"] = json!(false);
        let (status, saved) = call(app.clone(), "PUT", "/api/settings", settings).await;
        assert_eq!(status, 200, "{saved}");
        settings = saved;
        let fresh = api::router(AppState::new(
            pool.clone(),
            Engine::new("ignored-new-default".into()),
        ));
        assert_eq!(
            call(fresh, "GET", "/api/settings", Value::Null).await.1,
            settings
        );
        let (status, session) = call(
            app.clone(),
            "POST",
            "/api/play/sessions",
            json!({"human_player":0,"match_length":5}),
        )
        .await;
        assert_eq!(status, 201, "{session}");
        assert_eq!(session["play_mode"], mode);
        assert_eq!(session["tutor_threshold"], 0.123);
        assert_eq!(session["training_auto_analysis"], false);
    }
    for threshold in [0.0, -0.1, 1.001] {
        let mut invalid = settings.clone();
        invalid["tutor_threshold"] = json!(threshold);
        assert_eq!(
            call(app.clone(), "PUT", "/api/settings", invalid).await.0,
            422
        );
        assert_eq!(
            call(app.clone(), "GET", "/api/settings", Value::Null)
                .await
                .1,
            settings
        );
    }
    let mut invalid = settings.clone();
    invalid["default_play_mode"] = json!("unexpected");
    assert_eq!(
        call(app.clone(), "PUT", "/api/settings", invalid).await.0,
        422
    );
    assert_eq!(
        call(app, "GET", "/api/settings", Value::Null).await.1,
        settings
    );
}
