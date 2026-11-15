use axum::{body::Body, http::Request};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
    import, store,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;
async fn call(app: &axum::Router, method: &str, url: &str, body: Value) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(url)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let code = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (code, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}
async fn setup(pool: &PgPool) -> (axum::Router, Uuid) {
    let app = api::router(AppState::new(
        pool.clone(),
        Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())),
    ));
    let source = bgtutor::benchmarks::REFERENCE_SGF;
    let imported = store::import_match(
        pool,
        "estimate.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &import::parse_sgf(source).unwrap(),
    )
    .await
    .unwrap();
    (app, imported.r#match.id)
}
async fn seed(pool: &PgPool, app: &axum::Router, plies: i32, average: f64) -> Value {
    let (status, created) = call(
        app,
        "POST",
        "/api/benchmarks",
        json!({"engine_id":"gnubg","plies":[plies]}),
    )
    .await;
    assert_eq!(status, 202, "{created}");
    let id = Uuid::parse_str(created[0]["id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE benchmark_runs SET status='completed',started_at=now(),finished_at=now(),completed_games=10,analyzed_moves=460,total_elapsed_ms=$2*460,average_move_ms=$2 WHERE id=$1").bind(id).bind(average).execute(pool).await.unwrap();
    created[0].clone()
}
fn endpoint(id: Uuid, query: &str) -> String {
    format!("/api/matches/{id}/analysis/estimate?{query}")
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn estimates_whole_match_and_single_game_without_starting_work(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    let basis = seed(&pool, &app, 2, 40.0).await;
    for (query, count) in [
        ("plies=2", 460),
        ("plies=2&game_no=1", 8),
        ("plies=2&game_no=2", 11),
    ] {
        let (status, estimate) = call(&app, "GET", &endpoint(id, query), Value::Null).await;
        assert_eq!(status, 200, "{estimate}");
        assert_eq!(estimate["position_count"], count);
        assert_eq!(estimate["estimated_ms"], 40.0 * f64::from(count));
        assert_eq!(estimate["benchmark"]["id"], basis["id"]);
        assert!(estimate["unavailable_reason"].is_null());
        assert_eq!(estimate["engine_version"], basis["engine_version"]);
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM benchmark_runs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    let (status, default) = call(
        &app,
        "GET",
        &format!("/api/matches/{id}/analysis/estimate"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(default["plies"], 2);
    sqlx::query("UPDATE app_settings SET gnubg_threads=3 WHERE singleton=true")
        .execute(&pool)
        .await
        .unwrap();
    let (status, custom) = call(&app, "GET", &endpoint(id, "plies=2"), Value::Null).await;
    assert_eq!(status, 200, "{custom}");
    assert_eq!(custom["unavailable_reason"], "no_matching_benchmark");
    assert!(custom["estimated_ms"].is_null());
    let (_, schema) = call(&app, "GET", "/api/openapi.json", Value::Null).await;
    assert!(schema["paths"]["/api/matches/{id}/analysis/estimate"].is_object());
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn missing_calibration_is_optional_and_does_not_create_metadata(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    let (status, estimate) = call(&app, "GET", &endpoint(id, "plies=4"), Value::Null).await;
    assert_eq!(status, 200, "{estimate}");
    assert_eq!(estimate["unavailable_reason"], "no_matching_benchmark");
    assert!(estimate["estimated_ms"].is_null());
    assert!(estimate["benchmark"].is_null());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app_settings")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0,
        "estimate GET must not create default settings"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM benchmark_machine")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn estimates_explicit_gnubg_depth_with_open_sage_as_default(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    let basis = seed(&pool, &app, 4, 40.0).await;
    assert_eq!(basis["protocol"]["id"], "gnubg-match-v3-4ply-top14");
    sqlx::query("UPDATE app_settings SET analysis_engine='bgsage',analysis_plies=2")
        .execute(&pool)
        .await
        .unwrap();

    let (status, estimate) = call(
        &app,
        "GET",
        &endpoint(id, "plies=4&game_no=1&engine_id=gnubg"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{estimate}");
    assert_eq!(estimate["engine_id"], "gnubg");
    assert_eq!(estimate["plies"], 4);
    assert_eq!(estimate["benchmark"]["id"], basis["id"]);

    // A completed measurement with the old 4-ply filter cannot calibrate this job.
    sqlx::query("UPDATE benchmark_runs SET protocol=jsonb_set(jsonb_set(protocol,'{id}','\"gnubg-match-v2-unlimited\"'),'{move_filter}','\"gnubg-startup-defaults\"') WHERE id=$1")
        .bind(Uuid::parse_str(basis["id"].as_str().unwrap()).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let (_, estimate) = call(
        &app,
        "GET",
        &endpoint(id, "plies=4&game_no=1&engine_id=gnubg"),
        Value::Null,
    )
    .await;
    assert_eq!(estimate["unavailable_reason"], "no_matching_benchmark");
    let (_, settings) = call(&app, "GET", "/api/settings", Value::Null).await;
    assert_eq!(settings["analysis_engine"], "bgsage");
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn newest_finished_matching_measurement_wins_and_legacy_timeout_is_compatible(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    let old = seed(&pool, &app, 3, 20.0).await;
    let latest = seed(&pool, &app, 3, 30.0).await;
    sqlx::query("UPDATE benchmark_runs SET protocol=jsonb_set(jsonb_set(protocol,'{id}','\"gnubg-match-v1\"'),'{game_timeout_seconds}','600'),reference=jsonb_set(reference,'{protocol}',jsonb_set(jsonb_set(protocol,'{id}','\"gnubg-match-v1\"'),'{game_timeout_seconds}','600')) WHERE id=$1").bind(Uuid::parse_str(latest["id"].as_str().unwrap()).unwrap()).execute(&pool).await.unwrap();
    // Completion order, not submission order, determines the freshest observation.
    sqlx::query("UPDATE benchmark_runs SET finished_at=now()-interval '1 hour' WHERE id=$1")
        .bind(Uuid::parse_str(old["id"].as_str().unwrap()).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let (_, estimate) = call(&app, "GET", &endpoint(id, "plies=3"), Value::Null).await;
    assert_eq!(estimate["benchmark"]["id"], latest["id"]);
    assert_eq!(estimate["estimated_ms"], 13800.0);
    let (_, other) = call(&app, "GET", &endpoint(id, "plies=4"), Value::Null).await;
    assert_eq!(other["unavailable_reason"], "no_matching_benchmark");
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn incompatible_snapshots_never_replace_a_valid_older_basis(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    let valid = seed(&pool, &app, 2, 10.0).await;
    assert_eq!(valid["protocol"]["id"], "gnubg-match-v3-2ply-top14");
    let mutations = [
        "engine_version='GNU Backgammon different'",
        "engine_path='/another/gnubg'",
        "engine_id='other'",
        "plies=3",
        "machine=jsonb_set(machine,'{id}','\"00000000-0000-0000-0000-000000000002\"')",
        "machine=jsonb_set(machine,'{cpu_model}','\"Different CPU\"')",
        "machine=jsonb_set(machine,'{memory_bytes}','1')",
        "reference=jsonb_set(reference,'{sha256}','\"different\"')",
        "protocol=jsonb_set(protocol,'{noise}','1')",
        "protocol=jsonb_set(protocol,'{move_filter}','\"custom\"')",
        "protocol=jsonb_set(jsonb_set(protocol,'{id}','\"gnubg-match-v2-unlimited\"'),'{move_filter}','\"gnubg-startup-defaults\"')",
        "average_move_ms=1000",
        "status='failed',total_elapsed_ms=NULL,average_move_ms=NULL",
        "status='cancelled',total_elapsed_ms=NULL,average_move_ms=NULL",
        "status='queued',total_elapsed_ms=NULL,average_move_ms=NULL",
    ];
    for mutation in mutations {
        let bad = seed(&pool, &app, 2, 25.0).await;
        sqlx::query(&format!("UPDATE benchmark_runs SET {mutation} WHERE id=$1"))
            .bind(Uuid::parse_str(bad["id"].as_str().unwrap()).unwrap())
            .execute(&pool)
            .await
            .unwrap();
        let (status, estimate) = call(&app, "GET", &endpoint(id, "plies=2"), Value::Null).await;
        assert_eq!(status, 200, "{mutation}: {estimate}");
        assert_eq!(
            estimate["benchmark"]["id"], valid["id"],
            "{mutation}: {estimate}"
        );
    }
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn unavailable_engine_and_zero_moves_do_not_return_zero_seconds(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    sqlx::query("INSERT INTO app_settings(singleton,gnubg_path) VALUES(true,'/missing/gnubg') ON CONFLICT(singleton) DO UPDATE SET gnubg_path='/missing/gnubg'").execute(&pool).await.unwrap();
    let (status, estimate) = call(&app, "GET", &endpoint(id, "plies=2"), Value::Null).await;
    assert_eq!(status, 200, "{estimate}");
    assert_eq!(estimate["unavailable_reason"], "engine_unavailable");
    assert!(estimate["estimated_ms"].is_null());
    let source = "(;GM[6]MI[length:7][game:0][ws:0][bs:0];W[31hefe])\n(;GM[6]MI[length:7][game:1][ws:0][bs:0]AE[a:y]AW[f]AB[s]PL[W]RE[W+1];W[double];B[drop])\n(;GM[6]MI[length:7][game:2][ws:1][bs:0])";
    let empty = store::import_match(
        &pool,
        "empty.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &import::parse_sgf(source).unwrap(),
    )
    .await
    .unwrap();
    let (status, estimate) = call(
        &app,
        "GET",
        &endpoint(empty.r#match.id, "plies=2&game_no=2"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{estimate}");
    assert_eq!(estimate["unavailable_reason"], "no_checker_moves");
    assert!(estimate["estimated_ms"].is_null());
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn validates_scope_and_honors_removed_sources(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    for query in [
        "plies=-1",
        "plies=5",
        "plies=two",
        "game_no=999",
        "game_no=0",
        "plies=5&engine_id=bgsage",
        "plies=2&engine_id=unknown",
    ] {
        assert_eq!(
            call(&app, "GET", &endpoint(id, query), Value::Null).await.0,
            422,
            "{query}"
        );
    }
    assert_eq!(
        call(
            &app,
            "GET",
            &endpoint(Uuid::new_v4(), "plies=2"),
            Value::Null
        )
        .await
        .0,
        404
    );
    sqlx::query("UPDATE matches SET collection_only=true,database_id='00000000-0000-0000-0000-000000000002',source_match_id=id,source_match_no=match_no,source_database_id=database_id,source_database_name='Original database',original=''::bytea,sgf='',moves_cataloged=true,imported_analysis_loaded=true,source_metadata_loaded=true WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(&app, "GET", &endpoint(id, "plies=2"), Value::Null)
            .await
            .0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn active_competition_source_cannot_be_used_for_an_estimate(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    sqlx::query("INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id,library_match_id) VALUES($1,$2,'/missing','source','native',$3)")
  .bind(Uuid::new_v4()).bind(json!({"play_mode":"competition","phase":"move","match_length":7})).bind(id).execute(&pool).await.unwrap();
    let (status, body) = call(&app, "GET", &endpoint(id, "plies=2"), Value::Null).await;
    assert_eq!(status, 422, "{body}");
    assert!(body["error"].as_str().unwrap().contains("competition"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn legacy_scope_validation_does_not_backfill_game_metadata(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    sqlx::query("DELETE FROM match_games WHERE match_uuid=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, estimate) =
        call(&app, "GET", &endpoint(id, "plies=2&game_no=1"), Value::Null).await;
    assert_eq!(status, 200, "{estimate}");
    assert_eq!(estimate["position_count"], 8);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM match_games WHERE match_uuid=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0,
        "estimate GET must not backfill legacy game catalog"
    );
}
