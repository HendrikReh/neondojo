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
const SOURCE: &str = "(;GM[6]PW[Alice]PB[Bob]MI[length:7];W[31hefe])(;GM[6]PW[Alice]PB[Bob]MI[length:7][ws:1];W[31hefe])";
fn app(pool: &PgPool) -> axum::Router {
    api::router(AppState::new(
        pool.clone(),
        Engine::new("/unavailable".into()),
    ))
}
async fn call(app: &axum::Router, method: &str, uri: &str, body: Value) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
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
async fn imported(pool: &PgPool) -> (Uuid, Vec<Uuid>) {
    let id = store::import_match(
        pool,
        "foreign.sgf",
        "sgf",
        SOURCE.as_bytes(),
        SOURCE,
        &import::parse_sgf(SOURCE).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    sqlx::query("UPDATE matches SET format='xg',imported_analysis_loaded=true WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    let mut ids = vec![];
    for game in 1..=2 {
        let (position,position_id,gnubg_match_id):(Uuid,String,String)=sqlx::query_as("SELECT id,position_id,gnubg_match_id FROM positions WHERE match_uuid=$1 AND game_no=$2").bind(id).bind(game).fetch_one(pool).await.unwrap();
        let record = json!({"game_no":game,"source_move_no":1,"checker_ply":1,"position_id":position_id,"gnubg_match_id":gnubg_match_id,"dice":[3,1],"kind":"checker","player":0,"played":"8/5 6/5","engine":"eXtreme Gammon","engine_version":"2.19","method":"evaluation","depth":"XGRoller++","depth_plies":null,"candidates":[{"rank":1,"notation":"6/3 6/5","equity":0.2,"equity_kind":"cubeful","probabilities":{},"depth":"XGRoller++","raw":{}},{"rank":2,"notation":"8/5 6/5","equity":0.0,"equity_kind":"cubeful","probabilities":{},"raw":{}}],"cube":null,"raw":{}});
        let row = Uuid::new_v4();
        sqlx::query("INSERT INTO imported_analyses(id,match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data) VALUES($1,$2,$3,$4,$5,1,'checker',$6)").bind(row).bind(id).bind(position).bind(game-1).bind(game).bind(record).execute(pool).await.unwrap();
        ids.push(row);
    }
    (id, ids)
}
async fn checkpoint(pool: &PgPool, id: Uuid, game: i32) {
    let job = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,result,game_no,total_games,completed_games) VALUES($1,$2,'match',1,'completed','{}',$3,1,1)").bind(job).bind(id).bind(game).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,$2)")
        .bind(job)
        .bind(game)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO move_analyses(job_id,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,1,0,'checker',0,'{\"evaluation_status\":\"unavailable\"}')").bind(job).bind(game).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO analysis_selections(match_id,analysis_id) VALUES($1,$2) ON CONFLICT(match_id) DO UPDATE SET analysis_id=EXCLUDED.analysis_id").bind(id).bind(format!("job:{job}")).execute(pool).await.unwrap();
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn imports_are_eligible_private_and_reveal_preserves_metadata(pool: PgPool) {
    let (_, ids) = imported(&pool).await;
    let router = app(&pool);
    let (status, choices) = call(&router, "GET", "/api/study/mistakes", Value::Null).await;
    assert_eq!(status, 200, "{choices}");
    assert_eq!(choices["players"], json!(["Alice", "Bob"]));
    let (status, groups) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{groups}");
    assert_eq!(groups["analyzed_decisions"], 2, "{groups}");
    assert_eq!(groups["groups"][0]["count"], 2);
    let (_,practice)=call(&router,"POST","/api/study/mistakes/practice",json!({"player":"Alice","game_plan":groups["groups"][0]["game_plan"],"mistake_kind":"checker"})).await;
    assert_eq!(practice["items"].as_array().unwrap().len(), 2, "{practice}");
    let q = &practice["items"][0];
    assert!(q["reference"].get("job_id").is_none());
    assert!(
        ids.iter()
            .any(|id| q["reference"]["imported_analysis_id"] == id.to_string())
    );
    assert_eq!(q["source"], "xg");
    assert_eq!(q["plies"], Value::Null);
    assert_eq!(q["depth"], "XGRoller++");
    assert_eq!(q["method"], "evaluation");
    assert!(q["engine"].as_str().unwrap().contains("2.19"));
    for key in ["best", "played", "equity_loss"] {
        assert!(q.get(key).is_none());
    }
    let (status, a) = call(
        &router,
        "POST",
        "/api/study/mistakes/reveal",
        q["reference"].clone(),
    )
    .await;
    assert_eq!(status, 200, "{a}");
    assert_eq!(a["equity_loss"], 0.2);
    for key in ["engine", "plies", "depth", "method", "source"] {
        assert_eq!(a[key], q[key]);
    }
    let (status, checked) = call(
        &router,
        "POST",
        "/api/study/mistakes/attempt",
        json!({
            "reference":q["reference"],
            "answer":{"kind":"checker","steps":[{"from":6,"to":3},{"from":6,"to":5}]}
        }),
    )
    .await;
    assert_eq!(status, 200, "{checked}");
    assert_eq!(checked["evaluation"]["verdict"], "best");
    assert_eq!(checked["evaluation"]["provenance"]["kind"], "stored");
    assert_eq!(checked["evaluation"]["provenance"]["plies"], Value::Null);
    assert_eq!(checked["original"]["depth"], "XGRoller++");
    for table in ["learning_cards", "decks", "card_reviews", "jobs"] {
        let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn selected_native_checkpoint_stales_all_imported_references_without_cross_game_fallback(
    pool: PgPool,
) {
    let (id, ids) = imported(&pool).await;
    let router = app(&pool);
    checkpoint(&pool, id, 1).await;
    let (_, groups) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(groups["analyzed_decisions"], 0, "{groups}");
    assert_eq!(groups["groups"], json!([]));
    assert_eq!(
        call(
            &router,
            "POST",
            "/api/study/mistakes/reveal",
            json!({"imported_analysis_id":ids[0],"game_no":1,"decision_no":1})
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(
            &router,
            "POST",
            "/api/study/mistakes/reveal",
            json!({"imported_analysis_id":ids[1],"game_no":2,"decision_no":1})
        )
        .await
        .0,
        404
    );
    checkpoint(&pool, id, 2).await;
    assert_eq!(
        call(
            &router,
            "GET",
            "/api/study/mistakes?player=Alice",
            Value::Null
        )
        .await
        .1["groups"],
        json!([])
    );
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn imported_reference_validation_and_visibility(pool: PgPool) {
    let (id, ids) = imported(&pool).await;
    let router = app(&pool);
    for reference in [
        json!({"game_no":1,"decision_no":1}),
        json!({"job_id":Uuid::new_v4(),"imported_analysis_id":ids[0],"game_no":1,"decision_no":1}),
        json!({"imported_analysis_id":ids[0],"game_no":0,"decision_no":1}),
    ] {
        assert_eq!(
            call(&router, "POST", "/api/study/mistakes/reveal", reference)
                .await
                .0,
            422
        );
    }
    let reference = json!({"imported_analysis_id":ids[0],"game_no":1,"decision_no":1});
    assert_eq!(
        call(
            &router,
            "POST",
            "/api/study/mistakes/reveal",
            json!({"imported_analysis_id":ids[0],"game_no":2,"decision_no":1})
        )
        .await
        .0,
        404
    );
    sqlx::query("INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id,library_match_id) VALUES($1,$2,'/unavailable','','saved',$3)").bind(Uuid::new_v4()).bind(json!({"phase":"move","play_mode":"competition"})).bind(id).execute(&pool).await.unwrap();
    assert_eq!(
        call(&router, "GET", "/api/study/mistakes", Value::Null)
            .await
            .1["players"],
        json!([])
    );
    assert_eq!(
        call(
            &router,
            "POST",
            "/api/study/mistakes/reveal",
            reference.clone()
        )
        .await
        .0,
        404
    );
    sqlx::query("DELETE FROM play_sessions WHERE library_match_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM matches WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(&router, "POST", "/api/study/mistakes/reveal", reference)
            .await
            .0,
        404
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn real_binary_xg_legacy_catalog_is_loaded_without_engine_jobs(pool: PgPool) {
    let original = include_bytes!("fixtures/WC2014.xg");
    let sgf = import::xg_to_sgf(original).unwrap();
    let parsed = import::parse_sgf(&sgf).unwrap();
    let id = store::import_match(&pool, "WC2014.xg", "xg", original, &sgf, &parsed)
        .await
        .unwrap()
        .r#match
        .id;
    sqlx::query("DELETE FROM imported_analyses WHERE match_uuid=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM match_games WHERE match_uuid=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE matches SET imported_analysis_loaded=false,moves_cataloged=false WHERE id=$1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let router = app(&pool);
    let (status, choices) = call(&router, "GET", "/api/study/mistakes", Value::Null).await;
    assert_eq!(status, 200, "{choices}");
    assert_eq!(choices["players"].as_array().unwrap().len(), 2);
    let loaded: bool =
        sqlx::query_scalar("SELECT imported_analysis_loaded FROM matches WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!loaded, "choices must not parse legacy originals");
    let player = choices["players"][0].as_str().unwrap().replace(' ', "%20");
    let (status, groups) = call(
        &router,
        "GET",
        &format!("/api/study/mistakes?player={player}"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{groups}");
    assert!(
        groups["analyzed_decisions"].as_u64().unwrap() > 0,
        "{groups}"
    );
    assert!(!groups["groups"].as_array().unwrap().is_empty(), "{groups}");
    let loaded: bool =
        sqlx::query_scalar("SELECT imported_analysis_loaded FROM matches WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(loaded);
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(jobs, 0);
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn cataloged_imports_use_source_events_without_fetching_large_analysis_artifacts(
    pool: PgPool,
) {
    let (id, _) = imported(&pool).await;
    let router = app(&pool);
    assert_eq!(
        call(
            &router,
            "GET",
            "/api/study/mistakes?player=Alice",
            Value::Null
        )
        .await
        .0,
        200
    );
    sqlx::query("UPDATE matches SET sgf=repeat('x',67108865) WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, groups) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{groups}");
    assert_eq!(groups["analyzed_decisions"], 2, "{groups}");
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn unknown_units_and_source_identity_mismatches_are_not_counted(pool: PgPool) {
    let (_, ids) = imported(&pool).await;
    let router = app(&pool);
    sqlx::query("UPDATE imported_analyses SET data=jsonb_set(data,'{candidates,0,equity_kind}','\"mwc\"') WHERE id=$1").bind(ids[0]).execute(&pool).await.unwrap();
    sqlx::query(
        "UPDATE imported_analyses SET data=jsonb_set(data,'{source_move_no}','2') WHERE id=$1",
    )
    .bind(ids[1])
    .execute(&pool)
    .await
    .unwrap();
    let (status, groups) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{groups}");
    assert_eq!(groups["groups"], json!([]));
    assert_eq!(groups["analyzed_decisions"], 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn legacy_catalog_batches_make_progress_without_returning_partial_counts(pool: PgPool) {
    for n in 0..17 {
        let sgf = format!("(;GM[6]PW[Alice]PB[Bob]MI[length:100][ws:{n}];W[31hefe])");
        let id = store::import_match(
            &pool,
            &format!("legacy-{n}.sgf"),
            "sgf",
            sgf.as_bytes(),
            &sgf,
            &import::parse_sgf(&sgf).unwrap(),
        )
        .await
        .unwrap()
        .r#match
        .id;
        sqlx::query("UPDATE matches SET format='xg',imported_analysis_loaded=true,moves_cataloged=false WHERE id=$1").bind(id).execute(&pool).await.unwrap();
    }
    let router = app(&pool);
    let (status, error) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 422, "{error}");
    assert!(error["error"].as_str().unwrap().contains("retry"));
    let done: i64 = sqlx::query_scalar("SELECT count(*) FROM matches WHERE moves_cataloged")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(done, 16);
    let (status, result) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{result}");
    let done: i64 = sqlx::query_scalar("SELECT count(*) FROM matches WHERE moves_cataloged")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(done, 17);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn foreign_position_ownership_and_best_candidate_method_are_preserved(pool: PgPool) {
    let (_, ids) = imported(&pool).await;
    let router = app(&pool);
    sqlx::query("UPDATE imported_analyses SET position_uuid=(SELECT position_uuid FROM imported_analyses WHERE id=$2) WHERE id=$1").bind(ids[0]).bind(ids[1]).execute(&pool).await.unwrap();
    sqlx::query("UPDATE imported_analyses SET data=jsonb_set(data,'{candidates,0,method}','\"rollout\"') WHERE id=$1").bind(ids[1]).execute(&pool).await.unwrap();
    let (status, groups) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{groups}");
    assert_eq!(groups["analyzed_decisions"], 1, "{groups}");
    assert_eq!(
        call(
            &router,
            "POST",
            "/api/study/mistakes/reveal",
            json!({"imported_analysis_id":ids[0],"game_no":1,"decision_no":1})
        )
        .await
        .0,
        404
    );
    let (status, answer) = call(
        &router,
        "POST",
        "/api/study/mistakes/reveal",
        json!({"imported_analysis_id":ids[1],"game_no":2,"decision_no":1}),
    )
    .await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["method"], "rollout");
}
