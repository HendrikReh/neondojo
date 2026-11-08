use axum::{body::Body, http::Request};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
    import,
    model::Position,
    store,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SOURCE: &str = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[31hefe];B[double];W[take];B[42qusu])\n(;GM[6]FF[4]MI[length:7][ws:6][bs:2]RU[CrawfordGame];W[31hefe])";

async fn call(app: axum::Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(
                    body.map(|v| Body::from(v.to_string()))
                        .unwrap_or_else(Body::empty),
                )
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&bytes)})),
    )
}

fn encoded(v: &Value) -> String {
    v.to_string().bytes().map(|b| format!("%{b:02X}")).collect()
}

async fn search(app: axum::Router, fields: Value) -> Value {
    let (status, page) = call(
        app,
        "GET",
        &format!(
            "/api/position-browser?filters={}",
            encoded(&definition(fields))
        ),
        None,
    )
    .await;
    assert_eq!(status, 200, "{page}");
    page
}

fn definition(mut fields: Value) -> Value {
    fields["schema_version"] = json!(1);
    fields
}

async fn setup(pool: &PgPool) -> (axum::Router, Uuid, Vec<Position>) {
    let imported = store::import_match(
        pool,
        "fixture_100%.sgf",
        "sgf",
        SOURCE.as_bytes(),
        SOURCE,
        &import::parse_sgf(SOURCE).unwrap(),
    )
    .await
    .unwrap();
    let id = imported.r#match.id;
    bgtutor::move_catalog::ensure_catalog(pool, id)
        .await
        .unwrap();
    let positions = sqlx::query_as("SELECT * FROM positions WHERE match_uuid=$1 ORDER BY ordinal")
        .bind(id)
        .fetch_all(pool)
        .await
        .unwrap();
    (
        api::router(AppState::new(pool.clone(), Engine::new("unused".into()))),
        id,
        positions,
    )
}

async fn note(pool: &PgPool, column: &str, target: Uuid, text: &str) {
    sqlx::query(&format!(
        "INSERT INTO annotations(id,{column},tags,comment,version) VALUES($1,$2,ARRAY['test'],$3,1)"
    ))
    .bind(Uuid::new_v4())
    .bind(target)
    .bind(text)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn notes_belong_to_the_exact_game_position_and_checker_move(pool: PgPool) {
    let (app, id, positions) = setup(&pool).await;
    let game: Uuid =
        sqlx::query_scalar("SELECT id FROM match_games WHERE match_uuid=$1 AND game_no=2")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let cube: Uuid = sqlx::query_scalar(
        "SELECT id FROM match_moves WHERE match_uuid=$1 AND kind='cube' ORDER BY move_no LIMIT 1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let checker: Uuid = sqlx::query_scalar("SELECT id FROM match_moves WHERE position_uuid=$1")
        .bind(positions[1].id)
        .fetch_one(&pool)
        .await
        .unwrap();
    note(&pool, "game_uuid", game, "gameonly").await;
    note(&pool, "move_uuid", cube, "cubeonly").await;
    note(&pool, "move_uuid", checker, "checkeronly").await;
    note(&pool, "position_uuid", positions[0].id, "positiononly %_").await;
    note(&pool, "match_uuid", id, "matchwide").await;
    assert_eq!(
        search(app.clone(), json!({"comment":"gameonly"})).await["items"][0]["position"]["id"],
        positions[2].id.to_string()
    );
    assert_eq!(
        search(app.clone(), json!({"q":"cubeonly"})).await["items"],
        json!([])
    );
    let found = search(app.clone(), json!({"q":"checkeronly"})).await;
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        found["items"][0]["position"]["id"],
        positions[1].id.to_string()
    );
    assert_eq!(
        search(app.clone(), json!({"comment":"%_"})).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        search(app.clone(), json!({"q":"matchwide"})).await["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let first = search(app, json!({"comment":"positiononly"})).await;
    assert_eq!(first["items"][0]["tags"], json!(["test"]));
    assert_eq!(first["items"][0]["notes"].as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn source_context_dice_player_and_duplicate_boards_stay_distinct(pool: PgPool) {
    let (app, _, positions) = setup(&pool).await;
    let all = search(app.clone(), json!({})).await;
    assert_eq!(all["items"].as_array().unwrap().len(), 3);
    assert_eq!(positions[0].position_id, positions[2].position_id);
    let first = search(app.clone(), json!({"die1":1,"die2":3,"white_score":0,"black_score":0,"cube_value":1,"crawford":false,"player":"ALICE"})).await;
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        first["items"][0]["position"]["id"],
        positions[0].id.to_string()
    );
    let second = search(app.clone(), json!({"player":"Bob","die1":2,"cube_value":2})).await;
    assert_eq!(second["items"][0]["cube_owner"], 0);
    assert_eq!(
        second["items"][0]["position"]["id"],
        positions[1].id.to_string()
    );
    let crawford = search(
        app.clone(),
        json!({"white_score":6,"black_score":2,"crawford":true}),
    )
    .await;
    assert_eq!(
        crawford["items"][0]["position"]["id"],
        positions[2].id.to_string()
    );
    assert_eq!(
        search(app.clone(), json!({"match_name":"100%"})).await["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        search(app, json!({"match_points_min":8})).await["items"],
        json!([])
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn cursor_is_stable_and_bound_to_every_filter_and_sort(pool: PgPool) {
    let (app, _, _) = setup(&pool).await;
    let filters = encoded(&definition(json!({})));
    let (status, first) = call(
        app.clone(),
        "GET",
        &format!("/api/position-browser?filters={filters}&limit=1"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{first}");
    let cursor = first["next_cursor"].as_str().unwrap();
    let (_, second) = call(
        app.clone(),
        "GET",
        &format!("/api/position-browser?filters={filters}&limit=1&after={cursor}"),
        None,
    )
    .await;
    assert_ne!(
        first["items"][0]["position"]["id"],
        second["items"][0]["position"]["id"]
    );
    for fields in [
        json!({"player":"Alice"}),
        json!({"sort":"loss"}),
        json!({"die1":1}),
    ] {
        let (status, body) = call(
            app.clone(),
            "GET",
            &format!(
                "/api/position-browser?filters={}&after={cursor}",
                encoded(&definition(fields))
            ),
            None,
        )
        .await;
        assert_eq!(status, 422, "{body}");
    }
    for uri in [
        "/api/position-browser?limit=101",
        "/api/position-browser?limit=0",
        "/api/position-browser?after=broken",
    ] {
        assert_eq!(call(app.clone(), "GET", uri, None).await.0, 422);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn saved_filters_validate_names_schema_and_optimistic_versions(pool: PgPool) {
    let (app, _, _) = setup(&pool).await;
    let body = json!({"name":"  My Search  ","definition":definition(json!({"player":"Alice"}))});
    let (status, saved) = call(
        app.clone(),
        "POST",
        "/api/position-filters",
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, 201, "{saved}");
    assert_eq!(saved["name"], "My Search");
    assert_eq!(saved["version"], 1);
    assert_eq!(
        call(
            app.clone(),
            "POST",
            "/api/position-filters",
            Some(json!({"name":"my search","definition":definition(json!({}))}))
        )
        .await
        .0,
        409
    );
    let path = format!("/api/position-filters/{}", saved["id"].as_str().unwrap());
    let update =
        json!({"name":"Renamed","definition":definition(json!({"die1":2})),"expected_version":1});
    assert_eq!(
        call(app.clone(), "PUT", &path, Some(update.clone()))
            .await
            .1["version"],
        2
    );
    assert_eq!(call(app.clone(), "PUT", &path, Some(update)).await.0, 409);
    assert_eq!(
        call(
            app.clone(),
            "DELETE",
            &format!("{path}?expected_version=1"),
            None
        )
        .await
        .0,
        409
    );
    assert_eq!(
        call(
            app.clone(),
            "DELETE",
            &format!("{path}?expected_version=2"),
            None
        )
        .await
        .0,
        204
    );
    assert_eq!(
        call(
            app.clone(),
            "DELETE",
            &format!("{path}?expected_version=2"),
            None
        )
        .await
        .0,
        404
    );
    for bad in [
        json!({"schema_version":2}),
        json!({"schema_version":1,"typo":1}),
        json!({}),
        definition(json!({"die1":7})),
        definition(json!({"cube_value":3})),
        definition(json!({"min_loss":-1})),
        definition(json!({"analysis_state":"imported"})),
        definition(json!({"sort":"unknown"})),
        definition(json!({"white_score":-1})),
        definition(json!({"match_points_min":10,"match_points_max":2})),
    ] {
        let (status, error) = call(
            app.clone(),
            "POST",
            "/api/position-filters",
            Some(json!({"name":"bad","definition":bad})),
        )
        .await;
        assert_eq!(status, 422, "{error}");
    }
    assert_eq!(
        call(
            app,
            "POST",
            "/api/position-filters",
            Some(json!({"name":" ","definition":definition(json!({}))}))
        )
        .await
        .0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn deleted_database_reference_is_preserved_and_explicitly_rejected(pool: PgPool) {
    let (app, _, _) = setup(&pool).await;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO databases(id,name) VALUES($1,'Temporary')")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, saved) = call(
        app.clone(),
        "POST",
        "/api/position-filters",
        Some(json!({"name":"scoped","definition":definition(json!({"database_id":id}))})),
    )
    .await;
    sqlx::query("DELETE FROM databases WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, list) = call(app.clone(), "GET", "/api/position-filters", None).await;
    assert_eq!(list[0]["id"], saved["id"]);
    assert_eq!(list[0]["database_missing"], true);
    assert_eq!(
        call(
            app,
            "GET",
            &format!(
                "/api/position-browser?filters={}",
                encoded(&definition(json!({"database_id":id})))
            ),
            None
        )
        .await
        .0,
        422
    );
}

async fn native(
    pool: &PgPool,
    match_id: Uuid,
    position: &Position,
    plies: i32,
    loss: f64,
    status: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO jobs(id,match_uuid,kind,plies,status,result) VALUES($1,$2,'match',$3,$4,$5)",
    )
    .bind(id)
    .bind(match_id)
    .bind(plies)
    .bind(status)
    .bind(json!({"engine":format!("GNU Backgammon {plies}" )}))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,$3,1,$4,'checker',$5,$6)").bind(id).bind(position.id).bind(position.game_no).bind(position.player).bind(loss).bind(json!({"best":"8/5 6/5","severity":"bad","evaluation_status":"evaluated","played_equity":0.0,"best_equity":loss,"equity_loss":loss})).execute(pool).await.unwrap();
    id
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn native_loss_uses_highest_completed_depth_and_hints_only_mark_ready(pool: PgPool) {
    let (app, id, positions) = setup(&pool).await;
    native(&pool, id, &positions[0], 0, 0.5, "completed").await;
    let authoritative = native(&pool, id, &positions[0], 1, 0.09, "completed").await;
    native(&pool, id, &positions[0], 2, 0.9, "running").await;
    sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result) VALUES($1,$2,'analysis',2,'completed',$3)")
        .bind(Uuid::new_v4()).bind(positions[1].id).bind(json!({"engine":"GNU Backgammon hint","plies":2,"candidates":[{"rank":1,"notation":"best","equity":0.2,"probabilities":[]}]})).execute(&pool).await.unwrap();
    let ready = search(app.clone(), json!({"analysis_state":"ready","sort":"loss"})).await;
    assert_eq!(ready["items"].as_array().unwrap().len(), 2);
    assert_eq!(
        ready["items"][0]["native_error"]["job_id"],
        authoritative.to_string()
    );
    assert_eq!(ready["items"][0]["native_error"]["equity_loss"], 0.09);
    assert_eq!(ready["items"][1]["native_error"], Value::Null);
    assert_eq!(
        search(app.clone(), json!({"min_loss":0.1})).await["items"],
        json!([])
    );
    assert_eq!(
        search(app, json!({"analysis_state":"missing"})).await["items"][0]["position"]["id"],
        positions[2].id.to_string()
    );
}

#[test]
fn openapi_covers_browser_and_strict_persisted_filter_contract() {
    use utoipa::OpenApi;
    let doc = serde_json::to_value(api::ApiDoc::openapi()).unwrap();
    assert!(doc["paths"]["/api/position-browser"]["get"].is_object());
    assert!(doc["paths"]["/api/position-filters/{id}"]["delete"].is_object());
    assert!(doc["components"]["schemas"]["PositionFilterDefinition"].is_object());
    let parameters = doc["paths"]["/api/position-browser"]["get"]["parameters"]
        .as_array()
        .unwrap();
    for name in ["filters", "after", "limit"] {
        let parameter = parameters.iter().find(|p| p["name"] == name).unwrap();
        assert_eq!(parameter["in"], "query");
        assert_eq!(parameter["required"], false);
    }
    let filters = parameters.iter().find(|p| p["name"] == "filters").unwrap();
    assert!(
        filters["description"]
            .as_str()
            .unwrap()
            .contains("PositionFilterDefinition")
    );
    assert!(filters["example"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn filters_are_database_scoped_and_do_not_merge_copied_positions(pool: PgPool) {
    let (app, original, _) = setup(&pool).await;
    let database = Uuid::new_v4();
    sqlx::query("INSERT INTO databases(id,name) VALUES($1,'Second')")
        .bind(database)
        .execute(&pool)
        .await
        .unwrap();
    let copy = store::import_match_in_database(
        &pool,
        "copy.sgf",
        "sgf",
        SOURCE.as_bytes(),
        SOURCE,
        &import::parse_sgf(SOURCE).unwrap(),
        database,
    )
    .await
    .unwrap()
    .r#match
    .id;
    note(&pool, "match_uuid", original, "private-to-original").await;
    assert_eq!(
        search(app.clone(), json!({})).await["items"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    let found = search(app.clone(), json!({"database_id":database})).await;
    assert_eq!(found["items"].as_array().unwrap().len(), 3);
    assert!(
        found["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|hit| hit["position"]["match_uuid"] == copy.to_string())
    );
    assert_eq!(
        search(
            app,
            json!({"database_id":database,"q":"private-to-original"})
        )
        .await["items"],
        json!([])
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn loss_pagination_covers_known_losses_and_unknowns_once(pool: PgPool) {
    let (app, id, positions) = setup(&pool).await;
    native(&pool, id, &positions[0], 1, 0.04, "completed").await;
    native(&pool, id, &positions[1], 1, 0.16, "completed").await;
    let filters = encoded(&definition(json!({"sort":"loss"})));
    let mut after = String::new();
    let mut ids = vec![];
    loop {
        let (status, page) = call(
            app.clone(),
            "GET",
            &format!("/api/position-browser?filters={filters}&limit=1{after}"),
            None,
        )
        .await;
        assert_eq!(status, 200, "{page}");
        ids.push(
            page["items"][0]["position"]["id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
        let Some(cursor) = page["next_cursor"].as_str() else {
            break;
        };
        after = format!("&after={cursor}");
        assert!(ids.len() < 4);
    }
    assert_eq!(
        ids,
        vec![
            positions[1].id.to_string(),
            positions[0].id.to_string(),
            positions[2].id.to_string()
        ]
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn malformed_legacy_ids_remain_unknown_and_saved_filter_count_is_bounded(pool: PgPool) {
    let (app, _, positions) = setup(&pool).await;
    sqlx::query("UPDATE positions SET gnubg_match_id='legacy-dummy' WHERE id=$1")
        .bind(positions[0].id)
        .execute(&pool)
        .await
        .unwrap();
    let found = search(app.clone(), json!({})).await;
    assert_eq!(found["items"][0]["cube_value"], Value::Null);
    assert_eq!(found["items"][0]["white_score"], Value::Null);
    let context: Option<Vec<i32>> =
        sqlx::query_scalar("SELECT position_browser_context('AAAAAAAAAAAA')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(context.is_none());
    sqlx::query("INSERT INTO position_filters(id,name,definition) SELECT gen_random_uuid(),'saved '||n,'{\"schema_version\":1}'::jsonb FROM generate_series(1,200) n").execute(&pool).await.unwrap();
    let (status, error) = call(
        app,
        "POST",
        "/api/position-filters",
        Some(json!({"name":"one too many","definition":definition(json!({}))})),
    )
    .await;
    assert_eq!(status, 422, "{error}");
    assert!(error["error"].as_str().unwrap().contains("200"));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn readiness_obeys_the_highest_individual_hint_even_when_an_older_hint_is_valid(
    pool: PgPool,
) {
    let (app, _, positions) = setup(&pool).await;
    for (plies, candidates) in [
        (
            0,
            json!([{"rank":1,"notation":"best","equity":0.1,"probabilities":[]}]),
        ),
        (2, json!([])),
    ] {
        sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result) VALUES($1,$2,'analysis',$3,'completed',$4)")
            .bind(Uuid::new_v4()).bind(positions[0].id).bind(plies)
            .bind(json!({"engine":"GNU Backgammon","plies":plies,"candidates":candidates}))
            .execute(&pool).await.unwrap();
    }
    let found = search(app.clone(), json!({})).await;
    assert_eq!(found["items"][0]["card_ready"], false);
    assert_eq!(
        search(app.clone(), json!({"analysis_state":"ready"})).await["items"],
        json!([])
    );
    assert_eq!(
        search(app, json!({"analysis_state":"missing"})).await["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn an_empty_selected_individual_hint_blocks_match_solution_fallback(pool: PgPool) {
    let (app, id, positions) = setup(&pool).await;
    let native = native(&pool, id, &positions[0], 1, 0.08, "completed").await;
    sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result) VALUES($1,$2,'analysis',2,'completed',$3)")
        .bind(Uuid::new_v4()).bind(positions[0].id)
        .bind(json!({"engine":"GNU Backgammon","plies":2,"candidates":[]}))
        .execute(&pool).await.unwrap();
    let found = search(app.clone(), json!({})).await;
    assert_eq!(
        found["items"][0]["native_error"]["job_id"],
        native.to_string()
    );
    assert_eq!(found["items"][0]["card_ready"], false);
    assert_eq!(
        search(app, json!({"analysis_state":"ready"})).await["items"],
        json!([])
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn unavailable_selected_native_decisions_do_not_become_zero_loss_or_fall_back_to_old_errors(
    pool: PgPool,
) {
    let (app, id, positions) = setup(&pool).await;
    native(&pool, id, &positions[0], 0, 0.08, "completed").await;
    let unknown = native(&pool, id, &positions[0], 2, 0.0, "completed").await;
    sqlx::query("UPDATE move_analyses SET data=$2 WHERE job_id=$1").bind(unknown)
        .bind(json!({"best":"","evaluation_status":"unavailable","played_equity":null,"best_equity":null,"equity_loss":0.0}))
        .execute(&pool).await.unwrap();
    let found = search(app.clone(), json!({})).await;
    assert_eq!(found["items"][0]["native_error"], Value::Null);
    assert_eq!(found["items"][0]["card_ready"], false);
    assert_eq!(
        search(app.clone(), json!({"analysis_state":"ready"})).await["items"],
        json!([])
    );
    assert_eq!(
        search(app, json!({"max_loss":0.01})).await["items"],
        json!([])
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn forced_zero_loss_is_ready_but_missing_native_equities_or_blank_answers_are_not(
    pool: PgPool,
) {
    let (app, id, positions) = setup(&pool).await;
    let job = native(&pool, id, &positions[0], 2, 0.0, "completed").await;
    let data = json!({"best":"Cannot move","evaluation_status":"forced","played_equity":null,"best_equity":null,"equity_loss":0.0});
    sqlx::query("UPDATE move_analyses SET data=$2 WHERE job_id=$1")
        .bind(job)
        .bind(data.clone())
        .execute(&pool)
        .await
        .unwrap();
    let found = search(app.clone(), json!({"analysis_state":"ready"})).await;
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    assert_eq!(found["items"][0]["native_error"]["equity_loss"], 0.0);
    for invalid in [
        json!({"best":"best","evaluation_status":"evaluated","played_equity":null,"best_equity":null,"equity_loss":0.0}),
        json!({"best":"   ","evaluation_status":"evaluated","played_equity":0.0,"best_equity":0.0,"equity_loss":0.0}),
        json!({"best":"Cannot move","evaluation_status":"forced","played_equity":null,"best_equity":null,"equity_loss":0.2}),
    ] {
        sqlx::query("UPDATE move_analyses SET data=$2 WHERE job_id=$1")
            .bind(job)
            .bind(invalid)
            .execute(&pool)
            .await
            .unwrap();
        let found = search(app.clone(), json!({})).await;
        assert_eq!(found["items"][0]["native_error"], Value::Null);
        assert_eq!(found["items"][0]["card_ready"], false);
    }
    sqlx::query("UPDATE move_analyses SET data=$2 WHERE job_id=$1")
        .bind(job)
        .bind(data)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result) VALUES($1,$2,'analysis',2,'completed',$3)")
        .bind(Uuid::new_v4()).bind(positions[0].id)
        .bind(json!({"engine":"GNU Backgammon","plies":2,"candidates":[{"notation":"   ","equity":0.0,"rank":1,"probabilities":[]}]}))
        .execute(&pool).await.unwrap();
    assert_eq!(
        search(app, json!({"analysis_state":"ready"})).await["items"],
        json!([])
    );
}
