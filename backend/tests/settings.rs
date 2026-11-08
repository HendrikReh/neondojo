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
        json!({"version":0,"player_name":"","analysis_engine":"gnubg","analysis_plies":0,"play_engine":"gnubg","play_plies":0,"gnubg_path":"/missing/injected-gnubg"})
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
    assert_eq!(engines[0]["plies"], json!([0, 1, 2]));
    assert_eq!(
        engines[1],
        json!({"id":"bgsage","name":"BGSage","available":false,"version":null,"plies":[]})
    );
    let (_, schema) = call(app, "GET", "/api/openapi.json", Value::Null).await;
    for route in ["/api/settings", "/api/engines", "/api/engines/gnubg/check"] {
        assert!(schema["paths"][route].is_object(), "{route}");
    }
    assert!(schema["components"]["schemas"]["AppSettings"].is_object());
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
    update["analysis_plies"] = json!(1);
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
        ("analysis_plies", json!(3)),
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
    assert_eq!(old["plies"], 0);
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
