use axum::{body::Body, http::Request};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
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

const SOURCE: &str = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][game:0][ws:0][bs:0];W[31hefe];B[double];W[take];B[42qusu];W[double];B[drop])\n(;GM[6]FF[4]MI[length:7][game:1][ws:6][bs:0]RU[CrawfordGame])";

async fn setup(pool: &PgPool) -> (axum::Router, Uuid) {
    let response = store::import_match(
        pool,
        "notes.sgf",
        "sgf",
        SOURCE.as_bytes(),
        SOURCE,
        &import::parse_sgf(SOURCE).unwrap(),
    )
    .await
    .unwrap();
    (
        api::router(AppState::new(pool.clone(), Engine::new("unused".into()))),
        response.r#match.id,
    )
}

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

async fn catalog(app: axum::Router, id: Uuid, game: i32) -> Value {
    let (status, page) = call(
        app,
        "GET",
        &format!("/api/matches/{id}/games/{game}/decisions"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{page}");
    page
}

async fn save(
    app: axum::Router,
    scope: &str,
    id: &str,
    comment: &str,
    version: i64,
) -> (u16, Value) {
    call(
        app,
        "PUT",
        &format!("/api/{scope}/{id}/annotation"),
        Some(json!({"tags":[" CUBE "],"comment":comment,"expected_version":version})),
    )
    .await
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn played_move_catalog_is_stable_and_snapshots_precede_actions(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    let page = catalog(app.clone(), id, 1).await;
    assert_eq!(page, catalog(app.clone(), id, 1).await);
    assert_eq!(page["items"].as_array().unwrap().len(), 6);
    let moves = page["items"].as_array().unwrap();
    assert_eq!(
        moves
            .iter()
            .map(|m| m["move_no"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6]
    );
    assert_eq!(
        moves
            .iter()
            .map(|m| m["notation"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["8/5 6/5", "Double", "Take", "8/4 6/4", "Double", "Drop"]
    );
    let (_, positions) = call(
        app.clone(),
        "GET",
        &format!("/api/positions?match_id={id}"),
        None,
    )
    .await;
    for (move_index, pos_index) in [(0, 0), (3, 1)] {
        let position = &positions["items"][pos_index];
        assert_eq!(moves[move_index]["position_uuid"], position["id"]);
        assert_eq!(moves[move_index]["board"], position["board"]);
        assert_eq!(
            moves[move_index]["gnubg_match_id"],
            position["gnubg_match_id"]
        );
    }
    for index in [1, 2, 4, 5] {
        assert_eq!(moves[index]["kind"], "cube");
        assert!(moves[index]["position_uuid"].is_null());
        assert_eq!(moves[index]["dice"], json!([0, 0]));
    }
    assert_eq!(moves[1]["board"], moves[2]["board"]);
    assert_eq!(
        moves[1]["position_id"], moves[2]["position_id"],
        "cube responses retain the board orientation of the player on roll"
    );
    assert_eq!(moves[4]["position_id"], moves[5]["position_id"]);
    assert_eq!(moves[2]["board"], moves[3]["board"]);
    assert_ne!(moves[0]["board"], moves[1]["board"]);
    let take_id = STANDARD_NO_PAD
        .decode(moves[2]["gnubg_match_id"].as_str().unwrap())
        .unwrap();
    assert_ne!(
        take_id[1] & 0x10,
        0,
        "cube response snapshot must retain the pending double"
    );
    let after_take = STANDARD_NO_PAD
        .decode(moves[3]["gnubg_match_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(after_take[0] & 15, 1, "accepted cube is two");
    assert_eq!((after_take[0] >> 4) & 3, 0, "White owns the accepted cube");
    let (_, single) = call(
        app.clone(),
        "GET",
        &format!("/api/moves/{}", moves[1]["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(single, moves[1]);
    let empty = catalog(app.clone(), id, 2).await;
    assert_eq!(empty["items"], json!([]));
    assert_eq!(empty, catalog(app.clone(), id, 2).await);
    assert_ne!(page["game_id"], empty["game_id"]);
    assert_eq!(
        call(
            app.clone(),
            "GET",
            &format!("/api/matches/{id}/games/99/decisions"),
            None
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(app, "GET", &format!("/api/moves/{}", Uuid::new_v4()), None)
            .await
            .0,
        404
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn game_and_cube_notes_are_independent_searchable_and_versioned(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    let page = catalog(app.clone(), id, 1).await;
    let empty = catalog(app.clone(), id, 2).await;
    let game_id = empty["game_id"].as_str().unwrap();
    let move_id = page["items"][1]["id"].as_str().unwrap();
    for (scope, target, kind, text) in [
        ("games", game_id, "game", "needle empty game"),
        ("moves", move_id, "move", "needle cube event"),
    ] {
        let (status, initial) = call(
            app.clone(),
            "GET",
            &format!("/api/{scope}/{target}/annotation"),
            None,
        )
        .await;
        assert_eq!(status, 200, "{initial}");
        assert_eq!(initial["target_kind"], kind);
        assert_eq!(initial["version"], 0);
        let (status, saved) = save(app.clone(), scope, target, text, 0).await;
        assert_eq!(status, 200, "{saved}");
        assert_eq!(saved["comment"], text);
        assert_eq!(saved["tags"], json!(["cube"]));
        assert_eq!(saved["version"], 1);
        assert_eq!(
            save(app.clone(), scope, target, "overwrite", 0).await.0,
            409
        );
        assert_eq!(
            save(app.clone(), scope, target, text, 1).await.1["version"],
            2
        );
        let (status, found) = call(
            app.clone(),
            "GET",
            &format!("/api/annotations/search?q=needle&kind={kind}"),
            None,
        )
        .await;
        assert_eq!(status, 200, "{found}");
        assert_eq!(found["items"].as_array().unwrap().len(), 1);
        assert_eq!(found["items"][0]["target_id"], target);
        assert_eq!(found["items"][0]["match_uuid"], id.to_string());
        assert_eq!(
            found["items"][0]["game_no"],
            if kind == "game" { 2 } else { 1 }
        );
        assert_eq!(
            call(
                app.clone(),
                "GET",
                &format!("/api/{scope}/{}/annotation", Uuid::new_v4()),
                None
            )
            .await
            .0,
            404
        );
    }
    let (_, tags) = call(app, "GET", "/api/tags", None).await;
    assert_eq!(tags, json!(["cube"]));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn checker_move_notes_use_the_existing_position_annotation(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    let page = catalog(app.clone(), id, 1).await;
    let checker = &page["items"][0];
    let move_id = checker["id"].as_str().unwrap();
    let position_id = checker["position_uuid"].as_str().unwrap();
    let (status, saved) = save(app.clone(), "moves", move_id, "canonical checker note", 0).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["target_kind"], "position");
    assert_eq!(saved["target_id"], position_id);
    assert_eq!(
        call(
            app.clone(),
            "GET",
            &format!("/api/positions/{position_id}/annotation"),
            None
        )
        .await
        .1,
        saved
    );
    assert_eq!(
        save(
            app.clone(),
            "positions",
            position_id,
            "revised checker note",
            1
        )
        .await
        .0,
        200
    );
    let (_, via_move) = call(
        app.clone(),
        "GET",
        &format!("/api/moves/{move_id}/annotation"),
        None,
    )
    .await;
    assert_eq!(via_move["comment"], "revised checker note");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM annotations WHERE move_uuid=$1")
        .bind(Uuid::parse_str(move_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let (_, match_note) = call(app, "GET", &format!("/api/matches/{id}/annotation"), None).await;
    assert_eq!(match_note["version"], 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn legacy_catalog_backfills_once_under_concurrent_requests(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    sqlx::query("DELETE FROM match_games WHERE match_uuid=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (first, second) = tokio::join!(catalog(app.clone(), id, 1), catalog(app.clone(), id, 1));
    assert_eq!(first, second);
    let complete: bool = sqlx::query_scalar("SELECT moves_cataloged FROM matches WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(complete);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM match_moves WHERE match_uuid=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 6);
    let empty = catalog(app, id, 2).await;
    assert_eq!(empty["items"], json!([]));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn all_note_scopes_respect_database_search_and_tag_filters(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    let (status, database) = call(
        app.clone(),
        "POST",
        "/api/databases",
        Some(json!({"name":"Private notes"})),
    )
    .await;
    assert_eq!(status, 201, "{database}");
    let db_id = Uuid::parse_str(database["id"].as_str().unwrap()).unwrap();
    let other = store::import_match_in_database(
        &pool,
        "other.sgf",
        "sgf",
        SOURCE.as_bytes(),
        SOURCE,
        &import::parse_sgf(SOURCE).unwrap(),
        db_id,
    )
    .await
    .unwrap()
    .r#match
    .id;
    let page = catalog(app.clone(), id, 1).await;
    let other_page = catalog(app.clone(), other, 1).await;
    for (scope, target) in [
        ("matches", id.to_string()),
        ("games", page["game_id"].as_str().unwrap().into()),
        ("moves", page["items"][1]["id"].as_str().unwrap().into()),
        (
            "positions",
            page["items"][0]["position_uuid"].as_str().unwrap().into(),
        ),
    ] {
        assert_eq!(
            save(app.clone(), scope, &target, "needle shared", 0)
                .await
                .0,
            200
        );
    }
    assert_eq!(
        save(
            app.clone(),
            "moves",
            other_page["items"][1]["id"].as_str().unwrap(),
            "needle private",
            0
        )
        .await
        .0,
        200
    );
    let other_move = other_page["items"][1]["id"].as_str().unwrap();
    assert_eq!(
        call(
            app.clone(),
            "PUT",
            &format!("/api/moves/{other_move}/annotation"),
            Some(json!({"tags":["private"],"comment":"needle private","expected_version":1}))
        )
        .await
        .0,
        200
    );
    let (_, hits) = call(
        app.clone(),
        "GET",
        &format!("/api/annotations/search?q=needle&database_id={db_id}"),
        None,
    )
    .await;
    assert_eq!(hits["items"].as_array().unwrap().len(), 1, "{hits}");
    assert_eq!(hits["items"][0]["match_uuid"], other.to_string());
    let (_, hits) = call(
        app.clone(),
        "GET",
        "/api/annotations/search?q=needle&database_id=00000000-0000-0000-0000-000000000001",
        None,
    )
    .await;
    assert_eq!(hits["items"].as_array().unwrap().len(), 4, "{hits}");
    assert_eq!(
        call(
            app.clone(),
            "GET",
            &format!("/api/tags?database_id={db_id}"),
            None
        )
        .await
        .1,
        json!(["private"])
    );
    assert_eq!(
        call(
            app,
            "GET",
            "/api/tags?database_id=00000000-0000-0000-0000-000000000001",
            None
        )
        .await
        .1,
        json!(["cube"])
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn deleting_match_cascades_game_and_move_notes(pool: PgPool) {
    let (app, id) = setup(&pool).await;
    let page = catalog(app.clone(), id, 1).await;
    let game_id = page["game_id"].as_str().unwrap();
    let move_id = page["items"][1]["id"].as_str().unwrap();
    assert_eq!(save(app.clone(), "games", game_id, "game", 0).await.0, 200);
    assert_eq!(save(app.clone(), "moves", move_id, "move", 0).await.0, 200);
    sqlx::query("DELETE FROM matches WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    for table in ["annotations", "match_moves", "match_games"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
    assert_eq!(
        call(
            app.clone(),
            "GET",
            &format!("/api/games/{game_id}/annotation"),
            None
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(
            app,
            "GET",
            &format!("/api/moves/{move_id}/annotation"),
            None
        )
        .await
        .0,
        404
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn native_result_resignation_is_a_commentable_terminal_source_event(pool: PgPool) {
    // GNUbg stores accepted resignations in RE, and synthesizes MOVE_RESIGN
    // after restoring the played checker/cube nodes.
    let source =
        "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0]RE[B+2R];W[31hefe];B[42qusu])";
    let id = store::import_match(
        &pool,
        "resigned.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &import::parse_sgf(source).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    let app = api::router(AppState::new(pool.clone(), Engine::new("unused".into())));
    let page = catalog(app.clone(), id, 1).await;
    let moves = page["items"].as_array().unwrap();
    assert_eq!(moves.len(), 3);
    assert_eq!(moves[2]["move_no"], 3);
    assert_eq!(moves[2]["kind"], "resign");
    assert_eq!(moves[2]["player"], 0);
    assert_eq!(moves[2]["notation"], "Resign");
    assert_eq!(moves[2]["dice"], json!([0, 0]));
    assert!(moves[2]["position_uuid"].is_null());
    assert_ne!(moves[2]["board"], moves[1]["board"]);
    let (status, note) = save(
        app,
        "moves",
        moves[2]["id"].as_str().unwrap(),
        "resignation note",
        0,
    )
    .await;
    assert_eq!(status, 200, "{note}");
    assert_eq!(note["target_kind"], "move");
}
