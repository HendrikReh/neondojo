use axum::{body::Body, http::Request};
use bgtutor::{api, engine::Engine, import, model::Position, store};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn call(app: &axum::Router, method: &str, uri: &str, data: Value) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(status.is_success(), "{status}: {body}");
    body
}

async fn position(pool: &PgPool, name: &str, dice_move: &str, score: i32) -> Position {
    let source =
        format!("(;GM[6]PW[{name}]PB[Opponent]MI[length:7][ws:{score}][bs:0];W[{dice_move}])");
    let imported = store::import_match(
        pool,
        &format!("{name}.sgf"),
        "sgf",
        source.as_bytes(),
        &source,
        &import::parse_sgf(&source).unwrap(),
    )
    .await
    .unwrap();
    let position = store::positions(pool, Some(imported.r#match.id), None, None, 1)
        .await
        .unwrap()
        .items
        .remove(0);
    sqlx::query("INSERT INTO saved_positions(position_uuid) VALUES($1)")
        .bind(position.id)
        .execute(pool)
        .await
        .unwrap();
    position
}

async fn analyze(pool: &PgPool, position: Uuid) {
    let result = json!({"engine":"Native test analysis","plies":0,"candidates":[{"rank":1,"notation":"13/10 6/5","equity":0.123,"probabilities":[0.6,0.1,0.01,0.4,0.05,0.005]}]});
    sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result) VALUES($1,$2,'analysis',0,'completed',$3)")
        .bind(Uuid::new_v4()).bind(position).bind(result).execute(pool).await.unwrap();
}

async fn deck(app: &axum::Router, name: &str, position: Option<Uuid>) -> Value {
    let deck = call(app, "POST", "/api/decks", json!({"name":name})).await;
    if let Some(position) = position {
        let path = format!("/api/decks/{}/positions", deck["id"].as_str().unwrap());
        let added = call(app, "POST", &path, json!({"position_ids":[position]})).await;
        assert_eq!(added["added"], 1);
        let again = call(app, "POST", &path, json!({"position_ids":[position]})).await;
        assert_eq!(again["added"], 0);
    }
    json!({"id":deck["id"],"name":deck["name"]})
}

async fn page(app: &axum::Router, filters: Value, after: Option<&str>, limit: usize) -> Value {
    let encoded: String = filters
        .to_string()
        .bytes()
        .map(|b| format!("%{b:02X}"))
        .collect();
    call(
        app,
        "GET",
        &format!(
            "/api/position-browser?filters={encoded}&limit={limit}{}",
            after.map(|c| format!("&after={c}")).unwrap_or_default()
        ),
        Value::Null,
    )
    .await
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn memberships_follow_the_learning_identity_and_refresh_without_duplicate_rows(pool: PgPool) {
    let first = position(&pool, "Original", "31hefe", 0).await;
    let equivalent = position(&pool, "Equivalent", "31hefe", 0).await;
    let dice = position(&pool, "Different dice", "42hdfd", 0).await;
    let score = position(&pool, "Different score", "31hefe", 2).await;
    assert_eq!(first.position_id, dice.position_id);
    assert_eq!(first.position_id, score.position_id);
    assert_ne!(first.gnubg_match_id, dice.gnubg_match_id);
    assert_ne!(first.gnubg_match_id, score.gnubg_match_id);
    analyze(&pool, first.id).await;
    analyze(&pool, dice.id).await;
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    let zulu = deck(&app, "Zulu review", Some(first.id)).await;
    let alpha = deck(&app, "Alpha plans", Some(first.id)).await;
    let other = deck(&app, "Other dice", Some(dice.id)).await;
    deck(&app, "Empty deck", None).await;
    let result = page(&app, json!({"schema_version":1}), None, 100).await;
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    for (id, decks) in [
        (first.id, json!([alpha, zulu])),
        (equivalent.id, json!([alpha, zulu])),
        (dice.id, json!([other])),
        (score.id, json!([])),
    ] {
        let hit = items
            .iter()
            .find(|hit| hit["position"]["id"] == id.to_string())
            .unwrap();
        assert_eq!(hit["decks"], decks, "position {id}");
    }
    // Membership reads stay current and do not depend on the original payload's source UUID.
    sqlx::query("UPDATE decks SET name='Renamed plans' WHERE id=$1")
        .bind(Uuid::parse_str(alpha["id"].as_str().unwrap()).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM decks WHERE id=$1")
        .bind(Uuid::parse_str(zulu["id"].as_str().unwrap()).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let updated = page(
        &app,
        json!({"schema_version":1,"player":"Equivalent"}),
        None,
        1,
    )
    .await;
    assert_eq!(
        updated["items"][0]["decks"],
        json!([{"id":alpha["id"],"name":"Renamed plans"}])
    );
    call(
        &app,
        "POST",
        "/api/library/actions",
        json!({"action":"delete","match_ids":[first.match_uuid]}),
    )
    .await;
    let retained = page(
        &app,
        json!({"schema_version":1,"player":"Original"}),
        None,
        1,
    )
    .await;
    assert_eq!(retained["items"][0]["position"]["id"], first.id.to_string());
    assert_eq!(retained["items"][0]["source_available"], false);
    assert_eq!(
        retained["items"][0]["decks"],
        json!([{"id":alpha["id"],"name":"Renamed plans"}])
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn deck_name_sorting_keeps_empty_memberships_last_and_pagination_unique(pool: PgPool) {
    let a = position(&pool, "Last match name", "31hefe", 0).await;
    let b = position(&pool, "First match name", "42hdfd", 0).await;
    let empty = position(&pool, "Empty", "31hefe", 2).await;
    analyze(&pool, a.id).await;
    analyze(&pool, b.id).await;
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    deck(&app, "zulu", Some(b.id)).await;
    deck(&app, "Alpha", Some(a.id)).await;
    for (direction, expected) in [
        ("asc", vec![a.id, b.id, empty.id]),
        ("desc", vec![b.id, a.id, empty.id]),
    ] {
        let mut after = None;
        let mut actual = Vec::new();
        loop {
            let result = page(
                &app,
                json!({"schema_version":1,"sort":"decks","direction":direction}),
                after.as_deref(),
                1,
            )
            .await;
            actual.extend(
                result["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|item| Uuid::parse_str(item["position"]["id"].as_str().unwrap()).unwrap()),
            );
            after = result["next_cursor"].as_str().map(str::to_owned);
            if after.is_none() {
                break;
            }
            assert!(actual.len() <= expected.len());
        }
        assert_eq!(actual, expected);
    }
}
