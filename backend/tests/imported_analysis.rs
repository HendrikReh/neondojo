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
async fn call(pool: &PgPool, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
    let app = api::router(AppState::new(
        pool.clone(),
        Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())),
    ));
    let r = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(
                    body.map(|v| Body::from(v.to_string()))
                        .unwrap_or_else(Body::empty),
                )
                .unwrap(),
        )
        .await
        .unwrap();
    let status = r.status().as_u16();
    let b = r.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&b).unwrap_or(Value::Null))
}
async fn imported(pool: &PgPool) -> Uuid {
    let sgf = include_str!("fixtures/analysis-game1.sgf");
    let parsed = import::parse_sgf(sgf).unwrap();
    store::import_match(
        pool,
        "public-analysis.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &parsed,
    )
    .await
    .unwrap()
    .r#match
    .id
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn source_analysis_is_persisted_exactly_and_copied_independently(pool: PgPool) {
    let id = imported(&pool).await;
    let (status, summary) = call(
        &pool,
        "GET",
        &format!("/api/matches/{id}/imported-analysis"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{summary}");
    assert_eq!(summary["status"], "available");
    assert_eq!(summary["engines"][0], "GNU Backgammon");
    let position: Uuid =
        sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 ORDER BY ordinal LIMIT 1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let (status, records) = call(
        &pool,
        "GET",
        &format!("/api/positions/{position}/imported-analysis"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{records}");
    assert_eq!(records[0]["candidates"][0]["equity"], 0.013766);
    assert_eq!(records[0]["depth_plies"], 0);
    let bytes: Vec<u8> = sqlx::query_scalar("SELECT original FROM matches WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(bytes, include_bytes!("fixtures/analysis-game1.sgf"));
    let computed: i64 = sqlx::query_scalar("SELECT count(*) FROM move_analyses")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        computed, 0,
        "imported analysis must not masquerade as a completed engine job"
    );
    let (status, db) = call(
        &pool,
        "POST",
        "/api/databases",
        Some(json!({"name":"Imported analysis copies","description":""})),
    )
    .await;
    assert_eq!(status, 201, "{db}");
    let (status, copy) = call(
        &pool,
        "POST",
        "/api/library/actions",
        Some(json!({"action":"copy","match_ids":[id],"target_database_id":db["id"]})),
    )
    .await;
    assert_eq!(status, 200, "{copy}");
    let target = Uuid::parse_str(copy["results"][0]["created_id"].as_str().unwrap()).unwrap();
    let target_pos: Uuid =
        sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 ORDER BY ordinal LIMIT 1")
            .bind(target)
            .fetch_one(&pool)
            .await
            .unwrap();
    let (status, copied) = call(
        &pool,
        "GET",
        &format!("/api/positions/{target_pos}/imported-analysis"),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(copied[0]["raw"], records[0]["raw"]);
    assert_ne!(copied[0]["id"], records[0]["id"]);
    sqlx::query("DELETE FROM matches WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(
            &pool,
            "GET",
            &format!("/api/positions/{target_pos}/imported-analysis"),
            None
        )
        .await
        .1,
        copied
    );
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn engine_reanalysis_does_not_overwrite_imported_analysis(pool: PgPool) {
    let id = imported(&pool).await;
    let before:Vec<Value>=sqlx::query_scalar("SELECT data FROM imported_analyses WHERE match_uuid=$1 ORDER BY game_no,source_move_no,kind").bind(id).fetch_all(&pool).await.unwrap();
    assert!(!before.is_empty());
    let job = store::create_match_job(&pool, id, 0, 1).await.unwrap();
    let state = AppState::new(
        pool.clone(),
        Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())),
    );
    assert!(bgtutor::jobs::run_one(&state).await.unwrap());
    let finished = store::get_job(&pool, job.id).await.unwrap().unwrap();
    assert_eq!(finished.status, "completed", "{:?}", finished.error);
    let after:Vec<Value>=sqlx::query_scalar("SELECT data FROM imported_analyses WHERE match_uuid=$1 ORDER BY game_no,source_move_no,kind").bind(id).fetch_all(&pool).await.unwrap();
    assert_eq!(before, after);
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn legacy_xg_recovers_analysis_from_exact_original_and_keeps_source_move_mapping(
    pool: PgPool,
) {
    let bytes = include_bytes!("fixtures/WC2014.xg");
    let sgf = import::xg_to_sgf(bytes).unwrap();
    let parsed = import::parse_sgf(&sgf).unwrap();
    let id = store::import_match(&pool, "public.xg", "xg", bytes, &sgf, &parsed)
        .await
        .unwrap()
        .r#match
        .id;
    sqlx::query("DELETE FROM imported_analyses WHERE match_uuid=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE matches SET imported_analysis_loaded=false,sgf=replace(replace(sgf,'XGA[','IGNOREDM['),'XGC[','IGNOREDC[') WHERE id=$1").bind(id).execute(&pool).await.unwrap();
    let (status, summary) = call(
        &pool,
        "GET",
        &format!("/api/matches/{id}/imported-analysis"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{summary}");
    assert_eq!(summary["checker_records"], 460);
    assert_eq!(summary["engines"][0], "eXtreme Gammon");
    let (_, decisions) = call(
        &pool,
        "GET",
        &format!("/api/matches/{id}/games/1/decisions"),
        None,
    )
    .await;
    let cube = decisions["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["kind"] == "cube")
        .unwrap();
    let (status, records) = call(
        &pool,
        "GET",
        &format!(
            "/api/moves/{}/imported-analysis",
            cube["id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(status, 200, "{records}");
    assert!(!records.as_array().unwrap().is_empty());
    assert_eq!(records[0]["source_move_no"], cube["move_no"]);
    assert_eq!(records[0]["played"], cube["notation"]);
    assert_eq!(records[0]["position_uuid"], Value::Null);
    assert_eq!(records[0]["cube"]["equity_kind"], "cubeful");
    let (_, again) = call(
        &pool,
        "GET",
        &format!("/api/matches/{id}/imported-analysis"),
        None,
    )
    .await;
    assert_eq!(summary, again);
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn source_statistics_do_not_claim_decision_analysis(pool: PgPool) {
    let sgf = "(;FF[4]GM[6]AP[GNU Backgammon:1.05.002]GS[M:0 0 0 0];W[31hefe])";
    let parsed = import::parse_sgf(sgf).unwrap();
    let id = store::import_match(&pool, "unanalysed.sgf", "sgf", sgf.as_bytes(), sgf, &parsed)
        .await
        .unwrap()
        .r#match
        .id;
    let (status, summary) = call(
        &pool,
        "GET",
        &format!("/api/matches/{id}/imported-analysis"),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(summary["status"], "missing");
    assert_eq!(summary["checker_records"], 0);
    assert_eq!(summary["cube_records"], 0);
    assert_eq!(summary["record_count"], 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn bgf_native_numeric_strings_are_numbers_in_the_api_and_strings_in_raw(pool: PgPool) {
    let fixture = include_bytes!("fixtures/tutorial.bgf");
    let newline = fixture.iter().position(|b| *b == b'\n').unwrap();
    let mut value: Value = serde_json::from_slice(&fixture[newline + 1..]).unwrap();
    value["games"][0]["moves"][0]["type"] = json!("amove");
    value["games"][0]["moves"][0]["ply"] = json!(3);
    value["games"][0]["moves"][0]["moveAnalysis"] = json!([{"ply":4,"move":{"from":[8,6,-1,-1],"to":[5,5,-1,-1]},"eq":{"useMatchEquity":true,"matchEquity":"0.50011218","equity":"0.00270277","myWins":"0.49812975"}}]);
    let mut source = fixture[..newline + 1].to_vec();
    source.extend(serde_json::to_vec(&value).unwrap());
    let sgf = import::bgf_to_sgf(&source).unwrap();
    let parsed = import::parse_sgf(&sgf).unwrap();
    let id = store::import_match(
        &pool,
        "synthetic-analysis.bgf",
        "bgf",
        &source,
        &sgf,
        &parsed,
    )
    .await
    .unwrap()
    .r#match
    .id;
    let position: Uuid =
        sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 ORDER BY ordinal LIMIT 1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let (status, records) = call(
        &pool,
        "GET",
        &format!("/api/positions/{position}/imported-analysis"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{records}");
    assert_eq!(records[0]["candidates"][0]["equity"], 0.50011218);
    assert_eq!(
        records[0]["candidates"][0]["probabilities"]["win"],
        0.49812975
    );
    assert_eq!(records[0]["candidates"][0]["depth_plies"], 4);
    assert_eq!(
        records[0]["raw"]["moveAnalysis"][0]["eq"]["matchEquity"],
        "0.50011218"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn library_distinguishes_imported_analysis_from_native_job_status_and_pr(pool: PgPool) {
    async fn item(pool: &PgPool, id: Uuid) -> Value {
        let (status, page) = call(pool, "GET", "/api/library/matches", None).await;
        assert_eq!(status, 200, "{page}");
        page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == id.to_string())
            .unwrap()
            .clone()
    }
    fn no_native_pr(value: &Value) {
        for field in ["white_pr", "black_pr", "pr_method", "pr_profile"] {
            assert!(
                value[field].is_null(),
                "source analysis cannot supply native {field}: {value}"
            );
        }
    }
    let id = imported(&pool).await;
    let source = item(&pool, id).await;
    assert_eq!(source["analysis_status"], "imported");
    no_native_pr(&source);

    for (filename, sgf) in [
        ("plain.sgf", "(;FF[4]GM[6];W[31hefe])"),
        (
            "statistics.sgf",
            "(;FF[4]GM[6]AP[GNU Backgammon:1.05.002]GS[M:0 0 0 0];W[31hefe])",
        ),
    ] {
        let parsed = import::parse_sgf(sgf).unwrap();
        let plain = store::import_match(&pool, filename, "sgf", sgf.as_bytes(), sgf, &parsed)
            .await
            .unwrap()
            .r#match
            .id;
        let source = item(&pool, plain).await;
        assert!(source["analysis_status"].is_null(), "{filename}: {source}");
        no_native_pr(&source);
    }

    let job = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games) VALUES($1,$2,'match',0,'queued',1)").bind(job).bind(id).execute(&pool).await.unwrap();
    for native_status in ["queued", "running", "failed", "completed"] {
        sqlx::query("UPDATE jobs SET status=$2 WHERE id=$1")
            .bind(job)
            .bind(native_status)
            .execute(&pool)
            .await
            .unwrap();
        let source = item(&pool, id).await;
        assert_eq!(
            source["analysis_status"], "imported",
            "{native_status}: {source}"
        );
        no_native_pr(&source);
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM jobs WHERE id=$1")
                .bind(job)
                .fetch_one(&pool)
                .await
                .unwrap(),
            native_status,
            "showing a saved result must not change the calculation status"
        );
    }
    // Only durable, complete results outrank an imported analysis in automatic
    // mode. A completed scoped result remains available for explicit selection.
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no,summary) VALUES($1,1,'{\"completed_games\":1,\"total_games\":1}')")
        .bind(job).execute(&pool).await.unwrap();
    for (game_no, expected) in [(Some(1), "imported"), (None, "completed")] {
        sqlx::query("UPDATE jobs SET completed_games=1,game_no=$2 WHERE id=$1")
            .bind(job)
            .bind(game_no)
            .execute(&pool)
            .await
            .unwrap();
        let source = item(&pool, id).await;
        assert_eq!(source["analysis_status"], expected, "{source}");
        no_native_pr(&source);
    }
    let (status, selected) = call(
        &pool,
        "PUT",
        &format!("/api/matches/{id}/analysis-selection"),
        Some(json!({"analysis_id":format!("imported:{id}")})),
    )
    .await;
    assert_eq!(status, 200, "{selected}");
    let source = item(&pool, id).await;
    assert_eq!(source["analysis_status"], "imported");
    no_native_pr(&source);
    sqlx::query("DELETE FROM jobs WHERE id=$1")
        .bind(job)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(item(&pool, id).await["analysis_status"], "imported");
}
