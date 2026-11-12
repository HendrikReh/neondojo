use axum::{body::Body, http::Request};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
    import, store,
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn setup(pool: &PgPool) -> (axum::Router, Uuid) {
    let sgf = "(;GM[6]FF[4]MI[length:7][ws:0][bs:0];W[31hefe])";
    let imported = store::import_match(
        pool,
        "timing.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let job = store::create_match_job(pool, imported.r#match.id, 0, 1)
        .await
        .unwrap();
    (
        api::router(AppState::new(
            pool.clone(),
            Engine::new("/engine-must-not-be-launched".into()),
        )),
        job.id,
    )
}
async fn read(app: &axum::Router, id: Uuid) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{id}/remaining-time"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn legacy_job_read_is_unknown_and_read_only(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    sqlx::query("UPDATE jobs SET status='running' WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, timing) = read(&app, id).await;
    assert_eq!(status, 200, "{timing}");
    assert_eq!(timing["state"], "unavailable");
    assert_eq!(timing["total_moves"], 1);
    assert!(timing["remaining_ms"].is_null());
    assert_eq!(timing["current_game_elapsed_ms"], 0.0);
    for table in [
        "app_settings",
        "benchmark_machine",
        "benchmark_runs",
        "match_analysis_timing",
    ] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "GET must not mutate {table}");
    }
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn remaining_uses_move_weighted_observed_rate_and_active_elapsed(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    let token = Uuid::new_v4();
    sqlx::query("UPDATE jobs SET status='running',attempts=2,lease_token=$2,lease_until=now()+interval '120 seconds',total_games=3,completed_games=1 WHERE id=$1").bind(id).bind(token).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO match_analysis_timing(job_id,total_moves,completed_moves,timed_moves,measured_elapsed_ms,benchmark_average_move_ms,lease_token,attempt,active_elapsed_ms,active_started_at) VALUES($1,100,20,20,20000,500,$2,2,1000,clock_timestamp()-interval '1 second')").bind(id).bind(token).execute(&pool).await.unwrap();
    let (status, timing) = read(&app, id).await;
    assert_eq!(status, 200, "{timing}");
    assert_eq!(timing["basis"], "observed");
    assert_eq!(timing["state"], "estimated");
    let remaining = timing["remaining_ms"].as_f64().unwrap();
    assert!((78_000.0..=79_100.0).contains(&remaining), "{timing}");
    assert_eq!(timing["attempt"], 2);
    assert_eq!(timing["timed_moves"], 20);
    // Legacy completed work belongs to C, never to the observed-rate divisor.
    sqlx::query("UPDATE match_analysis_timing SET completed_moves=60,timed_moves=2,measured_elapsed_ms=2000,active_started_at=NULL WHERE job_id=$1").bind(id).execute(&pool).await.unwrap();
    let (_, waiting) = read(&app, id).await;
    assert_eq!(waiting["state"], "waiting");
    assert_eq!(waiting["remaining_ms"], 40000.0);
    // Reload reads the same saved confirmation data; no caller clock is used.
    assert_eq!(
        read(&app, id).await.1["remaining_ms"],
        waiting["remaining_ms"]
    );
    sqlx::query("UPDATE match_analysis_timing SET active_started_at=clock_timestamp()-interval '1 hour',active_elapsed_ms=3600000 WHERE job_id=$1").bind(id).execute(&pool).await.unwrap();
    let (_, overdue) = read(&app, id).await;
    assert_eq!(overdue["state"], "reestimating");
    assert!(overdue["remaining_ms"].is_null());
    sqlx::query("UPDATE jobs SET completed_games=total_games WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, finishing) = read(&app, id).await;
    assert_eq!(finishing["state"], "finishing");
    assert!(finishing["remaining_ms"].is_null());
    for terminal in ["completed", "failed", "queued"] {
        sqlx::query("UPDATE jobs SET status=$2 WHERE id=$1")
            .bind(id)
            .bind(terminal)
            .execute(&pool)
            .await
            .unwrap();
        let (_, value) = read(&app, id).await;
        assert_eq!(value["state"], terminal);
        assert!(value["remaining_ms"].is_null());
        assert_eq!(value["current_game_elapsed_ms"], 0.0);
    }
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn benchmark_zero_move_sections_and_stale_attempt_are_safe(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    let token = Uuid::new_v4();
    sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()+interval '120 seconds' WHERE id=$1").bind(id).bind(token).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO match_analysis_timing(job_id,total_moves,completed_moves,timed_moves,measured_elapsed_ms,benchmark_average_move_ms,lease_token,attempt,active_started_at) VALUES($1,100,0,0,2000,500,$2,1,clock_timestamp())").bind(id).bind(token).execute(&pool).await.unwrap();
    let (_, benchmark) = read(&app, id).await;
    assert_eq!(benchmark["state"], "estimated");
    assert_eq!(benchmark["basis"], "benchmark");
    assert!(benchmark["remaining_ms"].as_f64().unwrap() > 49000.0);
    sqlx::query("UPDATE match_analysis_timing SET benchmark_average_move_ms=NULL WHERE job_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, unknown) = read(&app, id).await;
    assert_eq!(unknown["state"], "unavailable");
    assert!(unknown["basis"].is_null());
    // An old worker's active timestamp must not count during a replacement attempt.
    sqlx::query("UPDATE jobs SET attempts=2,lease_token=$2 WHERE id=$1")
        .bind(id)
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .unwrap();
    let (_, resumed) = read(&app, id).await;
    assert_eq!(resumed["current_game_elapsed_ms"], 0.0);
    assert_eq!(resumed["measured_elapsed_ms"], 2000.0);
    assert_eq!(resumed["attempt"], 2);
    let (_, missing) = read(&app, Uuid::new_v4()).await;
    assert!(missing["error"].is_object() || missing["error"].is_string());
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn native_game_commits_timing_with_checkpoint(pool: PgPool) {
    let (_, id) = setup(&pool).await;
    let state = AppState::new(
        pool.clone(),
        Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())),
    );
    assert!(bgtutor::jobs::run_one(&state).await.unwrap());
    let job = store::get_job(&pool, id).await.unwrap().unwrap();
    assert_eq!(job.status, "completed", "{:?}", job.error);
    let app = api::router(state);
    let (_, timing) = read(&app, id).await;
    assert_eq!(timing["state"], "completed");
    assert_eq!(timing["completed_moves"], 1);
    assert_eq!(timing["timed_moves"], 1);
    assert!(
        timing["measured_elapsed_ms"].as_f64().unwrap() > 0.0,
        "{timing}"
    );
    let active: bool = sqlx::query_scalar(
        "SELECT active_started_at IS NOT NULL FROM match_analysis_timing WHERE job_id=$1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!active);
    assert_eq!(job.result.unwrap()["analyzed_moves"], 1);
    let events = store::events(&pool, id, 0).await.unwrap();
    assert!(events.iter().filter(|e| e.event == "progress").count() >= 3);
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn timing_endpoint_honors_source_and_competition_guards(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    let job = store::get_job(&pool, id).await.unwrap().unwrap();
    let match_id = job.match_uuid.unwrap();
    sqlx::query("INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id,library_match_id) VALUES($1,$2,'/missing','source','native',$3)")
        .bind(Uuid::new_v4()).bind(serde_json::json!({"play_mode":"competition","phase":"move","match_length":7})).bind(match_id).execute(&pool).await.unwrap();
    let (status, body) = read(&app, id).await;
    assert_eq!(status, 422, "{body}");
    assert!(body["error"].as_str().unwrap().contains("competition"));
    sqlx::query("DELETE FROM play_sessions")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE matches SET collection_only=true,database_id='00000000-0000-0000-0000-000000000002',source_match_id=id,source_match_no=match_no,source_database_id=database_id,source_database_name='Original database',original=''::bytea,sgf='',moves_cataloged=true,imported_analysis_loaded=true,source_metadata_loaded=true WHERE id=$1").bind(match_id).execute(&pool).await.unwrap();
    assert_eq!(read(&app, id).await.0, 422);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn timing_calibration_uses_saved_job_installation_and_plies(pool: PgPool) {
    let (_, old_id) = setup(&pool).await;
    sqlx::query("DELETE FROM jobs WHERE id=$1")
        .bind(old_id)
        .execute(&pool)
        .await
        .unwrap();
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let state = AppState::new(pool.clone(), engine);
    let app = api::router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/benchmarks")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"engine_id":"gnubg","plies":[0]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 202);
    let runs: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let run = &runs[0];
    let run_id = Uuid::parse_str(run["id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE benchmark_runs SET status='completed',started_at=now(),finished_at=now(),completed_games=10,analyzed_moves=460,total_elapsed_ms=9200,average_move_ms=20 WHERE id=$1").bind(run_id).execute(&pool).await.unwrap();
    let match_id: Uuid = sqlx::query_scalar("SELECT id FROM matches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let snapshot = bgtutor::settings::EngineSnapshot {
        open_sage: None,
        engine_id: "gnubg".into(),
        gnubg_path: run["engine_path"].as_str().unwrap().into(),
        engine_version: Some(run["engine_version"].as_str().unwrap().into()),
    };
    let job =
        store::create_match_job_scoped_with_snapshot(&pool, match_id, 0, 1, None, Some(&snapshot))
            .await
            .unwrap();
    // The worker and GET must never derive a running job's basis from current settings.
    sqlx::query(
        "UPDATE app_settings SET gnubg_path='/wrong-engine',analysis_plies=4 WHERE singleton=true",
    )
    .execute(&pool)
    .await
    .unwrap();
    let state = AppState::new(pool.clone(), Engine::new("/wrong-fallback".into()));
    assert!(bgtutor::jobs::run_one(&state).await.unwrap());
    let done = store::get_job(&pool, job.id).await.unwrap().unwrap();
    assert_eq!(done.status, "completed", "{:?}", done.error);
    let basis: (Option<Uuid>, Option<f64>) = sqlx::query_as(
        "SELECT benchmark_id,benchmark_average_move_ms FROM match_analysis_timing WHERE job_id=$1",
    )
    .bind(job.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(basis, (Some(run_id), Some(20.0)));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM benchmark_runs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1,
        "job must not launch calibration"
    );
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn wall_clock_corrections_cannot_change_persisted_active_elapsed(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    let token = Uuid::new_v4();
    sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()+interval '120 seconds' WHERE id=$1").bind(id).bind(token).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO match_analysis_timing(job_id,total_moves,completed_moves,benchmark_average_move_ms,lease_token,attempt,active_elapsed_ms,active_started_at) VALUES($1,100,0,500,$2,1,1000,clock_timestamp())").bind(id).bind(token).execute(&pool).await.unwrap();
    let (_, before) = read(&app, id).await;
    assert_eq!(before["current_game_elapsed_ms"], 1000.0);
    sqlx::query("UPDATE match_analysis_timing SET active_started_at=clock_timestamp()-interval '1 hour' WHERE job_id=$1").bind(id).execute(&pool).await.unwrap();
    let (_, after) = read(&app, id).await;
    assert_eq!(
        before["current_game_elapsed_ms"], after["current_game_elapsed_ms"],
        "wall clock corrections must not change a monotonic duration"
    );
    assert_eq!(before["remaining_ms"], after["remaining_ms"]);
    sqlx::query("UPDATE match_analysis_timing SET active_started_at=clock_timestamp()+interval '1 hour' WHERE job_id=$1").bind(id).execute(&pool).await.unwrap();
    assert_eq!(read(&app, id).await.1["current_game_elapsed_ms"], 1000.0);
}
