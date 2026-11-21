use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{api, engine::Engine, import, jobs, model::Job, store};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

// Empty and cube-only trees retain their source numbering, scores and rules.
const SOURCE: &str = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][game:0][ws:0][bs:0];W[31hefe];B[42qusu])\n(;GM[6]FF[4]MI[length:7][game:1][ws:1][bs:0];W[double];B[drop])\n(;GM[6]FF[4]MI[length:7][game:2][ws:6][bs:0]RU[CrawfordGame])\n(;GM[6]FF[4]MI[length:7][game:3][ws:6][bs:1];W[31hefe];B[42qusu])";

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

async fn setup(pool: &PgPool) -> (api::AppState, axum::Router, uuid::Uuid) {
    let imported = store::import_match(
        pool,
        "games.sgf",
        "sgf",
        SOURCE.as_bytes(),
        SOURCE,
        &import::parse_sgf(SOURCE).unwrap(),
    )
    .await
    .unwrap();
    let state = api::AppState::new(
        pool.clone(),
        Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())),
    );
    (state.clone(), api::router(state), imported.r#match.id)
}

#[sqlx::test(migrations = "./migrations")]
async fn game_catalog_preserves_all_games_and_queries_stay_inside_selected_game(pool: PgPool) {
    let (_, app, id) = setup(&pool).await;
    let catalog_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM match_games WHERE match_uuid=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        catalog_count, 4,
        "catalog and positions are persisted together during import"
    );
    let (status, games) = call(
        app.clone(),
        "GET",
        &format!("/api/matches/{id}/games"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{games}");
    assert_eq!(
        games,
        json!([
            {"game_no":1,"position_count":2,"white_score":0,"black_score":0,"crawford":false,"result":{"winner":0,"points":1,"cube":1,"multiplier":null,"reason":"recorded","scores_after":[1,0]}},
            {"game_no":2,"position_count":0,"white_score":1,"black_score":0,"crawford":false,"result":{"winner":0,"points":1,"cube":1,"multiplier":1,"reason":"drop","scores_after":[2,0]}},
            {"game_no":3,"position_count":0,"white_score":6,"black_score":0,"crawford":true,"result":{"winner":1,"points":1,"cube":1,"multiplier":null,"reason":"recorded","scores_after":[6,1]}},
            {"game_no":4,"position_count":2,"white_score":6,"black_score":1,"crawford":false}
        ])
    );
    let prefix = format!("/api/positions?match_id={id}");
    let (status, page) = call(
        app.clone(),
        "GET",
        &format!("{prefix}&game_no=4&limit=1"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(page["items"][0]["game_no"], 4);
    assert_eq!(page["items"][0]["ordinal"], 2);
    let p = &page["items"][0];
    let (status, position) = call(
        app.clone(),
        "GET",
        &format!("/api/positions/{}", p["id"].as_str().unwrap()),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(position, *p);
    let cursor = page["next_cursor"].as_str().unwrap();
    let (_, end) = call(
        app.clone(),
        "GET",
        &format!("{prefix}&game_no=4&after={cursor}"),
        Value::Null,
    )
    .await;
    assert_eq!(end["items"].as_array().unwrap().len(), 1);
    assert!(end["next_cursor"].is_null());
    for game in [2, 3] {
        let (_, empty) = call(
            app.clone(),
            "GET",
            &format!("{prefix}&game_no={game}"),
            Value::Null,
        )
        .await;
        assert_eq!(empty["items"], json!([]));
    }
    for uri in [
        format!("{prefix}&game_no=1&after={cursor}"),
        format!("{prefix}&game_no=0"),
        format!("{prefix}&game_no=5"),
        "/api/positions?game_no=4".into(),
    ] {
        assert_eq!(
            call(app.clone(), "GET", &uri, Value::Null).await.0,
            422,
            "{uri}"
        );
    }
    for (game, total) in [(1, 2), (2, 0), (3, 0), (4, 2)] {
        let (status, dice) = call(
            app.clone(),
            "GET",
            &format!("/api/matches/{id}/dice?game_no={game}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(dice["total"], total);
    }
    assert_eq!(
        call(
            app.clone(),
            "GET",
            &format!("/api/matches/{id}/dice?game_no=5"),
            Value::Null
        )
        .await
        .0,
        422
    );
    assert_eq!(
        call(
            app.clone(),
            "GET",
            &format!("/api/matches/{}/games", uuid::Uuid::new_v4()),
            Value::Null
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(
            app,
            "GET",
            &format!("/api/positions/{}", uuid::Uuid::new_v4()),
            Value::Null
        )
        .await
        .0,
        404
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn selected_game_analysis_is_isolated_deduplicated_and_recoverable(pool: PgPool) {
    let (state, app, id) = setup(&pool).await;
    let endpoint = format!("/api/matches/{id}/analysis");
    let (status, value) = call(
        app.clone(),
        "POST",
        &endpoint,
        json!({"plies":1,"game_no":4}),
    )
    .await;
    assert_eq!(status, 202, "{value}");
    assert_eq!(value["game_no"], 4);
    assert_eq!(value["total_games"], 1);
    let job: Job = serde_json::from_value(value).unwrap();
    let (_, duplicate) = call(
        app.clone(),
        "POST",
        &endpoint,
        json!({"plies":1,"game_no":4}),
    )
    .await;
    assert_eq!(duplicate["id"], job.id.to_string());
    for body in [json!({"game_no":0}), json!({"game_no":5})] {
        assert_eq!(call(app.clone(), "POST", &endpoint, body).await.0, 422);
    }
    assert!(
        call(app.clone(), "GET", &endpoint, Value::Null)
            .await
            .1
            .is_null()
    );
    assert!(
        call(
            app.clone(),
            "GET",
            &format!("{endpoint}?game_no=1"),
            Value::Null
        )
        .await
        .1
        .is_null()
    );
    // Recover an expired scoped lease without changing source numbering.
    sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()-interval '1 second' WHERE id=$1").bind(job.id).bind(uuid::Uuid::new_v4()).execute(&pool).await.unwrap();
    assert!(jobs::run_one(&state).await.unwrap());
    let (status, done) = call(
        app.clone(),
        "GET",
        &format!("{endpoint}?game_no=4"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(done["status"], "completed", "{done}");
    assert_eq!(done["attempts"], 2);
    assert_eq!(done["completed_games"], 1);
    assert_eq!(done["result"]["total_moves"], 2);
    assert_eq!(done["result"]["analyzed_moves"], 2);
    let checkpoints: Vec<i32> =
        sqlx::query_scalar("SELECT game_no FROM match_analysis_games WHERE job_id=$1")
            .bind(job.id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(checkpoints, [4]);
    let (_, rows) = call(
        app.clone(),
        "GET",
        &format!("/api/jobs/{}/moves?game_no=4&limit=1", job.id),
        Value::Null,
    )
    .await;
    assert!(
        rows["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["game_no"] == 4)
    );
    let cursor = rows["next_cursor"].as_i64().unwrap();
    for query in [
        "game_no=1".to_owned(),
        format!("game_no=1&after={cursor}"),
        "after=99999999".into(),
    ] {
        assert_eq!(
            call(
                app.clone(),
                "GET",
                &format!("/api/jobs/{}/moves?{query}", job.id),
                Value::Null
            )
            .await
            .0,
            422,
            "{query}"
        );
    }
    let (_, deck) = call(
        app.clone(),
        "POST",
        "/api/decks",
        json!({"name":"Selected game"}),
    )
    .await;
    let cards = format!("/api/decks/{}/cards", deck["id"].as_str().unwrap());
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &cards,
            json!({"job_id":job.id,"game_no":1})
        )
        .await
        .0,
        422
    );
    let (status, added) = call(
        app.clone(),
        "POST",
        &cards,
        json!({"job_id":job.id,"game_no":4}),
    )
    .await;
    assert_eq!(status, 200, "{added}");
    assert!(added["total"].as_i64().unwrap() >= 2);
    let payloads: Vec<Value> = sqlx::query_scalar("SELECT payload FROM learning_cards")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(payloads.iter().all(|p| p["source"]["game_no"] == 4));
    // Source state must retain late-game scores and original boards.
    let positions = store::positions(&pool, Some(id), None, None, 100)
        .await
        .unwrap()
        .items;
    for payload in &payloads {
        if let Some(position_id) = payload["source"]["position_id"].as_str() {
            let original = positions
                .iter()
                .find(|p| p.id.to_string() == position_id)
                .unwrap();
            assert_eq!(
                payload["position"]["gnubg_match_id"],
                original.gnubg_match_id
            );
            assert_eq!(payload["position"]["board"], json!(original.board));
        }
    }
    for game in [2, 3] {
        let (_, value) = call(app.clone(), "POST", &endpoint, json!({"game_no":game})).await;
        let id = value["id"].as_str().unwrap();
        assert!(jobs::run_one(&state).await.unwrap());
        let (_, done) = call(app.clone(), "GET", &format!("/api/jobs/{id}"), Value::Null).await;
        assert_eq!(done["status"], "completed", "{done}");
        assert_eq!(done["result"]["total_moves"], 0);
        assert_eq!(done["completed_games"], 1);
    }
    let (_, full) = call(app.clone(), "POST", &endpoint, json!({})).await;
    assert!(full["game_no"].is_null());
    assert_eq!(full["total_games"], 4);
    let (_, next) = call(app.clone(), "POST", &endpoint, json!({"game_no":4})).await;
    assert_ne!(next["id"], full["id"]);
    let (_, deeper) = call(
        app.clone(),
        "POST",
        &endpoint,
        json!({"game_no":4,"plies":1}),
    )
    .await;
    assert_ne!(next["id"], deeper["id"]);
    let (a, b) = tokio::join!(
        call(app.clone(), "POST", &endpoint, json!({"game_no":1})),
        call(app.clone(), "POST", &endpoint, json!({"game_no":1}))
    );
    assert_eq!(a.0, 202);
    assert_eq!(a.1["id"], b.1["id"]);
    assert_ne!(a.1["id"], next["id"]);
    assert!(jobs::run_one(&state).await.unwrap());
    let (_, full_done) = call(app.clone(), "GET", &endpoint, Value::Null).await;
    assert_eq!(full_done["id"], full["id"]);
    assert_eq!(full_done["status"], "completed", "{full_done}");
    assert_eq!(full_done["result"]["total_games"], 4);
    assert_eq!(full_done["result"]["analyzed_moves"], 4);
    let full_id = full["id"].as_str().unwrap();
    let (status, last_game) = call(
        app.clone(),
        "GET",
        &format!("/api/jobs/{full_id}/moves?game_no=4&limit=1"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(last_game["items"][0]["game_no"], 4);
    let last_cursor = last_game["next_cursor"].as_i64().unwrap();
    for query in [
        format!("game_no=1&after={last_cursor}"),
        format!("player=1&after={last_cursor}"),
        format!("kind=cube&after={last_cursor}"),
        format!("errors_only=true&after={last_cursor}"),
        format!("after={cursor}"),
    ] {
        assert_eq!(
            call(
                app.clone(),
                "GET",
                &format!("/api/jobs/{full_id}/moves?{query}"),
                Value::Null
            )
            .await
            .0,
            422,
            "{query}"
        );
    }
    let (_, filtered_deck) = call(
        app.clone(),
        "POST",
        "/api/decks",
        json!({"name":"Cube game only"}),
    )
    .await;
    let filtered_cards = format!("/api/decks/{}/cards", filtered_deck["id"].as_str().unwrap());
    let (status, filtered) = call(
        app.clone(),
        "POST",
        &filtered_cards,
        json!({"job_id":full_id,"game_no":2}),
    )
    .await;
    assert_eq!(status, 200, "{filtered}");
    assert_eq!(filtered["total"], 2);
    let (_, card_list) = call(app.clone(), "GET", &filtered_cards, Value::Null).await;
    assert!(
        card_list["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["source"]["game_no"] == 2)
    );
    assert_eq!(
        call(
            app.clone(),
            "GET",
            &format!("{endpoint}?game_no=9"),
            Value::Null
        )
        .await
        .0,
        422
    );
    let events = store::events(&pool, job.id, 0).await.unwrap();
    let progress = events
        .iter()
        .find(|e| e.event == "progress" && e.data["completed_games"] == 1)
        .unwrap();
    assert_eq!(progress.data["completed_games"], 1);
    assert_eq!(progress.data["total_games"], 1);
    assert_eq!(progress.data["progress"], 100);
}

#[sqlx::test(migrations = "./migrations")]
async fn legacy_catalog_backfills_once_under_concurrent_requests(pool: PgPool) {
    let (_, app, id) = setup(&pool).await;
    // Simulate a pre-migration match; checker rows alone cannot recover games 2/3.
    sqlx::query("DELETE FROM match_games WHERE match_uuid=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let endpoint = format!("/api/matches/{id}/games");
    let (a, b) = tokio::join!(
        call(app.clone(), "GET", &endpoint, Value::Null),
        call(app.clone(), "GET", &endpoint, Value::Null)
    );
    assert_eq!(a.0, 200, "{:?}", a);
    assert_eq!(a, b);
    assert_eq!(a.1.as_array().unwrap().len(), 4);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM match_games WHERE match_uuid=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 4);
    // A warm catalog remains readable if source result projection is unavailable.
    sqlx::query("UPDATE matches SET sgf='not SGF' WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, mut unavailable) = call(app.clone(), "GET", &endpoint, Value::Null).await;
    assert_eq!(status, 200);
    let mut expected = a.1.clone();
    for game in expected.as_array_mut().unwrap() {
        game.as_object_mut().unwrap().remove("result");
    }
    for game in unavailable.as_array_mut().unwrap() {
        assert!(game["result"].is_null());
    }
    assert_eq!(unavailable, expected);
    let (status, positions) = call(
        app,
        "GET",
        &format!("/api/positions?match_id={id}&game_no=4"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{positions}");
    assert_eq!(positions["items"].as_array().unwrap().len(), 2);
}
