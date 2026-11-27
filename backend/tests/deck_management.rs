use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{api, engine::Engine, import, model::Position, store};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

fn app(pool: &PgPool) -> axum::Router {
    api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ))
}

#[sqlx::test(migrations = "./migrations")]
async fn search_results_manage_card_memberships_and_delete_only_confirmed_cards(pool: PgPool) {
    let positions = positions(&pool).await;
    let app = app(&pool);
    let source = create(&app, "Source").await;
    let destination = create(&app, "Destination").await;
    add(&app, &source, &[positions[0].id, positions[1].id]).await;
    let (_, cards) = call(&app, "GET", "/api/cards?limit=2", Value::Null).await;
    let cards = cards["items"].as_array().unwrap();
    assert_eq!(cards.len(), 2);
    let (selected, other) = (&cards[0], &cards[1]);
    review(&app, selected).await;

    let (_, deckless) = call(
        &app,
        "GET",
        "/api/cards?without_deck=true&limit=1",
        Value::Null,
    )
    .await;
    assert_eq!(deckless["total"], 0, "{deckless}");
    let source_id = source["id"].as_str().unwrap();
    let target_id = destination["id"].as_str().unwrap();
    let remove_path = format!("/api/decks/{source_id}/remove-cards");
    let add_path = format!("/api/decks/{target_id}/add-cards");
    assert_eq!(
        call(
            &app,
            "POST",
            &remove_path,
            json!({"expected_version":1,"card_ids":[selected["id"]]})
        )
        .await
        .0,
        204
    );
    let (status, deckless) = call(
        &app,
        "GET",
        "/api/cards?without_deck=true&limit=1&status=reviewed",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{deckless}");
    assert_eq!(deckless["total"], 1);
    let card = &deckless["items"][0];
    assert_eq!(card["id"], selected["id"]);
    assert_eq!(card["review_count"], 1);

    let (status, result) = call(
        &app,
        "POST",
        &add_path,
        json!({"expected_version":0,"card_ids":[card["id"], Uuid::new_v4()]}),
    )
    .await;
    assert_eq!(status, 409, "{result}");
    assert_eq!(listed(&app, &destination["id"]).await["card_count"], 0);
    let (status, result) = call(
        &app,
        "POST",
        &add_path,
        json!({"expected_version":0,"card_ids":[card["id"]]}),
    )
    .await;
    assert_eq!(
        (status, result),
        (200, json!({"added":1,"already_in_target":0}))
    );
    let (status, result) = call(
        &app,
        "POST",
        &add_path,
        json!({"expected_version":1,"card_ids":[card["id"]]}),
    )
    .await;
    assert_eq!(
        (status, result),
        (200, json!({"added":0,"already_in_target":1}))
    );
    assert_eq!(listed(&app, &destination["id"]).await["version"], 1);
    let delete = "/api/cards/delete";
    let (status, result) = call(
        &app,
        "POST",
        delete,
        json!({"cards":[{"id":card["id"],"version":card["version"],"deck_ids":[]}]}),
    )
    .await;
    assert_eq!(status, 409, "{result}");
    let stale_batch = json!({"cards":[
        {"id":card["id"],"version":card["version"],"deck_ids":[target_id]},
        {"id":other["id"],"version":999,"deck_ids":[source_id]}
    ]});
    assert_eq!(call(&app, "POST", delete, stale_batch).await.0, 409);
    assert_eq!(listed(&app, &destination["id"]).await["card_count"], 1);
    assert_eq!(
        call(
            &app,
            "POST",
            delete,
            json!({"cards":[{"id":card["id"],"version":card["version"],"deck_ids":[target_id]}]})
        )
        .await
        .0,
        204
    );
    assert_eq!(listed(&app, &destination["id"]).await["version"], 2);
    let (_, remaining) = call(&app, "GET", "/api/cards", Value::Null).await;
    assert_eq!(remaining["total"], 1);
    assert_eq!(remaining["items"][0], *other);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM card_reviews")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

async fn raw(app: &axum::Router, method: &str, path: &str, body: &str) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    if status == 204 {
        assert!(bytes.is_empty(), "204 response has a body");
        (status, Value::Null)
    } else {
        let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            panic!(
                "{method} {path}: {status} non-JSON {}",
                String::from_utf8_lossy(&bytes)
            )
        });
        (status, body)
    }
}

async fn call(app: &axum::Router, method: &str, path: &str, body: Value) -> (u16, Value) {
    raw(app, method, path, &body.to_string()).await
}

async fn create(app: &axum::Router, name: &str) -> Value {
    let (status, deck) = call(app, "POST", "/api/decks", json!({"name":name})).await;
    assert_eq!(status, 201, "{deck}");
    deck
}

async fn listed(app: &axum::Router, id: &Value) -> Value {
    let (status, decks) = call(app, "GET", "/api/decks", Value::Null).await;
    assert_eq!(status, 200, "{decks}");
    decks
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == *id)
        .unwrap()
        .clone()
}

async fn positions(pool: &PgPool) -> Vec<Position> {
    let source = "(;GM[6]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])";
    let imported = store::import_match(
        pool,
        "decks.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &import::parse_sgf(source).unwrap(),
    )
    .await
    .unwrap();
    let positions = store::positions(pool, Some(imported.r#match.id), None, None, 100)
        .await
        .unwrap()
        .items;
    // All questions in this fixture share one completed match analysis, as the
    // saved catalog selection is per match rather than an implicit per-position fallback.
    let job = store::create_match_job(pool, imported.r#match.id, 1, 1)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET status='completed',completed_games=1,result=$2 WHERE id=$1")
        .bind(job.id)
        .bind(json!({"engine":"Native fixture"}))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,1)")
        .bind(job.id)
        .execute(pool)
        .await
        .unwrap();
    for position in &positions {
        sqlx::query("INSERT INTO saved_positions(position_uuid) VALUES($1)")
            .bind(position.id)
            .execute(pool)
            .await
            .unwrap();
        let decision = json!({"decision_no":position.ply,"checker_no":position.ply,"kind":"checker","player":position.player,"dice":position.dice,"played":position.notation,"best":position.notation,"played_equity":0.123,"best_equity":0.123,"equity_loss":0.0,"severity":"none","evaluation_status":"evaluated"});
        sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,1,$3,$4,'checker',0,$5)").bind(job.id).bind(position.id).bind(position.ply).bind(position.player).bind(decision).execute(pool).await.unwrap();
    }

    positions
}

async fn add(app: &axum::Router, deck: &Value, ids: &[Uuid]) -> Value {
    let (status, body) = call(
        app,
        "POST",
        &format!("/api/decks/{}/positions", deck["id"].as_str().unwrap()),
        json!({"position_ids":ids}),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    body
}

async fn review(app: &axum::Router, card: &Value) {
    let card_id = card["id"].as_str().unwrap();
    let (status, revealed) = call(
        app,
        "POST",
        &format!("/api/cards/{card_id}/reveal"),
        json!({"expected_version":card["version"]}),
    )
    .await;
    assert_eq!(status, 200, "{revealed}");
    let (status, reviewed) = call(
        app,
        "POST",
        &format!("/api/cards/{card_id}/review"),
        json!({"session_id":revealed["session_id"],"request_id":Uuid::new_v4(),"rating":"good"}),
    )
    .await;
    assert_eq!(status, 200, "{reviewed}");
}

#[sqlx::test(migrations = "./migrations")]
async fn rename_is_versioned_and_preserves_identity_memberships_and_reviews(pool: PgPool) {
    let positions = positions(&pool).await;
    let app = app(&pool);
    let deck = create(&app, "  Original  ").await;
    assert_eq!(deck["name"], "Original");
    assert_eq!(deck["version"], 0);
    assert_eq!(listed(&app, &deck["id"]).await, deck);
    add(&app, &deck, &[positions[0].id]).await;
    let path = format!("/api/decks/{}", deck["id"].as_str().unwrap());
    let (_, cards) = call(&app, "GET", &format!("{path}/cards"), Value::Null).await;
    review(&app, &cards["items"][0]).await;
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM learning_cards c")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (status, renamed) = call(
        &app,
        "PUT",
        &path,
        json!({"name":"  Renamed ✓  ","expected_version":1}),
    )
    .await;
    assert_eq!(status, 200, "{renamed}");
    assert_eq!(renamed["id"], deck["id"]);
    assert_eq!(renamed["created_at"], deck["created_at"]);
    assert_eq!(renamed["name"], "Renamed ✓");
    assert_eq!(renamed["version"], 2);
    assert_eq!(renamed["card_count"], 1);
    assert_eq!(renamed["due_count"], 0);
    assert_eq!(listed(&app, &deck["id"]).await, renamed);
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM learning_cards c")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM card_reviews")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reviews, 1);
    for method in ["PUT", "DELETE"] {
        let uri = if method == "DELETE" {
            format!("{path}?expected_version=1")
        } else {
            path.clone()
        };
        let (status, error) = call(
            &app,
            method,
            &uri,
            json!({"name":"Stale","expected_version":1}),
        )
        .await;
        assert_eq!(status, 409, "{error}");
        assert!(error["error"].is_string());
    }
    for method in ["PUT", "DELETE"] {
        let path = if method == "DELETE" {
            "/api/decks/not-a-uuid?expected_version=0"
        } else {
            "/api/decks/not-a-uuid"
        };
        let (status, error) = call(
            &app,
            method,
            path,
            json!({"name":"Invalid identifier","expected_version":0}),
        )
        .await;
        assert_eq!(status, 422, "{error}");
        assert!(error["error"].is_string());
    }
    assert_eq!(listed(&app, &deck["id"]).await, renamed);
}

#[sqlx::test(migrations = "./migrations")]
async fn mutations_validate_strict_names_versions_and_json_errors(pool: PgPool) {
    let app = app(&pool);
    let deck = create(&app, "Practice").await;
    let path = format!("/api/decks/{}", deck["id"].as_str().unwrap());
    for name in [
        "".to_owned(),
        " \n\t ".to_owned(),
        "a".repeat(121),
        "é".repeat(61),
        "bad\nname".to_owned(),
        "bad\0name".to_owned(),
    ] {
        for method in ["POST", "PUT"] {
            let uri = if method == "POST" {
                "/api/decks"
            } else {
                &path
            };
            let body = if method == "POST" {
                json!({"name":name})
            } else {
                json!({"name":name,"expected_version":0})
            };
            let (status, error) = call(&app, method, uri, body).await;
            assert_eq!(status, 422, "{method} {name:?}: {error}");
            assert!(error["error"].is_string());
        }
    }
    for body in [
        json!({}),
        json!({"name":"Valid"}),
        json!({"name":"Valid","expected_version":-1}),
        json!({"name":"Valid","expected_version":1.5}),
        json!({"name":"Valid","expected_version":"0"}),
        json!({"name":"Valid","expected_version":0,"extra":true}),
        json!({"name":null,"expected_version":0}),
    ] {
        let (status, error) = call(&app, "PUT", &path, body.clone()).await;
        assert_eq!(status, 422, "{body}: {error}");
        assert!(error["error"].is_string());
    }
    for body in [
        "{",
        "null",
        "[]",
        "{\"name\":\"a\",\"expected_version\":9223372036854775808}",
    ] {
        let (status, error) = raw(&app, "PUT", &path, body).await;
        assert_eq!(status, 422, "{body}: {error}");
        assert!(error["error"].is_string());
    }
    let (status, error) = call(
        &app,
        "POST",
        "/api/decks",
        json!({"name":"Valid","extra":true}),
    )
    .await;
    assert_eq!(status, 422, "{error}");
    for query in [
        "",
        "?expected_version=-1",
        "?expected_version=abc",
        "?expected_version=1.5",
        "?expected_version=9223372036854775808",
        "?expected_version=0&unexpected=1",
        "?expected_version=0&expected_version=1",
    ] {
        let (status, error) = raw(&app, "DELETE", &format!("{path}{query}"), "").await;
        assert_eq!(status, 422, "{query}: {error}");
        assert!(error["error"].is_string());
    }
    for method in ["PUT", "DELETE"] {
        let missing = format!(
            "/api/decks/{}{}",
            Uuid::new_v4(),
            if method == "DELETE" {
                "?expected_version=0"
            } else {
                ""
            }
        );
        let (status, error) = call(
            &app,
            method,
            &missing,
            json!({"name":"Missing","expected_version":0}),
        )
        .await;
        assert_eq!(status, 404, "{error}");
        assert!(error["error"].is_string());
    }
    let (status, valid) = call(
        &app,
        "PUT",
        &path,
        json!({"name":"é".repeat(60),"expected_version":0}),
    )
    .await;
    assert_eq!(status, 200, "{valid}");
    assert_eq!(valid["version"], 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn deletion_retains_shared_and_sole_cards_reviews_saved_positions_and_readd_history(
    pool: PgPool,
) {
    let positions = positions(&pool).await;
    let app = app(&pool);
    let target = create(&app, "Delete me").await;
    let other = create(&app, "Other deck").await;
    let target_path = format!("/api/decks/{}", target["id"].as_str().unwrap());
    let other_path = format!("/api/decks/{}", other["id"].as_str().unwrap());
    add(&app, &target, &[positions[0].id, positions[1].id]).await;
    add(&app, &other, &[positions[0].id]).await;
    let (_, cards) = call(&app, "GET", &format!("{target_path}/cards"), Value::Null).await;
    for card in cards["items"].as_array().unwrap() {
        review(&app, card).await;
    }
    sqlx::query("INSERT INTO learning_card_exports(card_id,fingerprint,modified) SELECT id,repeat('a',64),1 FROM learning_cards")
        .execute(&pool).await.unwrap();
    let before: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM learning_cards c ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    let other_before = listed(&app, &other["id"]).await;
    let (status, body) = raw(
        &app,
        "DELETE",
        &format!("{target_path}?expected_version=1"),
        "",
    )
    .await;
    assert_eq!(status, 204, "{body}");
    let after: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM learning_cards c ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    for table in [
        "card_reviews",
        "review_sessions",
        "saved_positions",
        "learning_card_exports",
    ] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2, "{table}");
    }
    assert_eq!(listed(&app, &other["id"]).await, other_before);
    let (_, remaining) = call(&app, "GET", &format!("{other_path}/cards"), Value::Null).await;
    assert_eq!(remaining["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        raw(
            &app,
            "DELETE",
            &format!("{target_path}?expected_version=1"),
            ""
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(&app, "GET", &format!("{target_path}/cards"), Value::Null)
            .await
            .0,
        404
    );
    let new_deck = create(&app, "Re-added").await;
    add(&app, &new_deck, &[positions[0].id, positions[1].id]).await;
    let restored: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM learning_cards c ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(before, restored);
    let memberships: i64 = sqlx::query_scalar("SELECT count(*) FROM deck_cards")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(memberships, 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn membership_batches_advance_versions_only_for_new_memberships(pool: PgPool) {
    let positions = positions(&pool).await;
    let app = app(&pool);
    let deck = create(&app, "Versions").await;
    let other = create(&app, "Other").await;
    assert_eq!(deck["version"], 0);
    add(&app, &deck, &[positions[0].id, positions[0].id]).await;
    assert_eq!(listed(&app, &deck["id"]).await["version"], 1);
    add(&app, &deck, &[positions[0].id]).await;
    assert_eq!(listed(&app, &deck["id"]).await["version"], 1);
    add(&app, &deck, &[Uuid::new_v4()]).await;
    assert_eq!(listed(&app, &deck["id"]).await["version"], 1);
    sqlx::query("UPDATE move_analyses SET equity_loss=0.077,data=data || '{\"best\":\"13/10 6/5\",\"best_equity\":0.2,\"equity_loss\":0.077,\"severity\":\"bad\"}'::jsonb WHERE position_uuid=$1").bind(positions[0].id).execute(&pool).await.unwrap();
    let changed = add(&app, &deck, &[positions[0].id]).await;
    assert_eq!(changed["updated"], 1);
    assert_eq!(listed(&app, &deck["id"]).await["version"], 1);
    add(&app, &other, &[positions[0].id]).await;
    assert_eq!(listed(&app, &other["id"]).await["version"], 1);
    let path = format!("/api/decks/{}", deck["id"].as_str().unwrap());
    let (status, result) = call(
        &app,
        "POST",
        &format!("{path}/cards"),
        json!({"position_id":positions[1].id}),
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(listed(&app, &deck["id"]).await["version"], 2);
    let (status, error) = call(
        &app,
        "PUT",
        &path,
        json!({"name":"Old dialog","expected_version":1}),
    )
    .await;
    assert_eq!(status, 409, "{error}");
    assert_eq!(
        raw(&app, "DELETE", &format!("{path}?expected_version=1"), "")
            .await
            .0,
        409
    );
    let negative = sqlx::query("UPDATE decks SET version=-1")
        .execute(&pool)
        .await;
    assert!(negative.is_err(), "database must reject negative versions");
}

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_mutations_have_one_winner_and_memberships_serialize(pool: PgPool) {
    let positions = positions(&pool).await;
    let app = app(&pool);
    let deck = create(&app, "Concurrent").await;
    let path = format!("/api/decks/{}", deck["id"].as_str().unwrap());
    let delete_path = format!("{path}?expected_version=0");
    let (rename, delete) = tokio::join!(
        call(
            &app,
            "PUT",
            &path,
            json!({"name":"Winner","expected_version":0})
        ),
        raw(&app, "DELETE", &delete_path, "")
    );
    assert!(
        matches!((rename.0, delete.0), (200, 409) | (404, 204)),
        "rename={rename:?}, delete={delete:?}"
    );
    let deck = create(&app, "Membership locks").await;
    let first_ids = [positions[0].id];
    let second_ids = [positions[1].id];
    let (first, second) = tokio::join!(add(&app, &deck, &first_ids), add(&app, &deck, &second_ids));
    assert_eq!(first["added"], 1);
    assert_eq!(second["added"], 1);
    let current = listed(&app, &deck["id"]).await;
    assert_eq!(current["version"], 2);
    assert_eq!(current["card_count"], 2);
    let (first, second) = tokio::join!(add(&app, &deck, &first_ids), add(&app, &deck, &first_ids));
    assert_eq!(first["added"], 0);
    assert_eq!(second["added"], 0);
    assert_eq!(listed(&app, &deck["id"]).await["version"], 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn dialogs_wait_for_in_flight_memberships_and_then_reject_their_stale_version(pool: PgPool) {
    use std::time::Duration;

    let positions = positions(&pool).await;
    let app = app(&pool);
    let original = create(&app, "Original membership").await;
    add(&app, &original, &[positions[0].id]).await;
    let target = create(&app, "In flight").await;
    let target_id = target["id"].as_str().unwrap();
    let card_id: Uuid = sqlx::query_scalar("SELECT id FROM learning_cards")
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut card_lock = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM learning_cards WHERE id=$1 FOR UPDATE")
        .bind(card_id)
        .execute(&mut *card_lock)
        .await
        .unwrap();
    let membership_app = app.clone();
    let membership_path = format!("/api/decks/{target_id}/positions");
    let position_id = positions[0].id;
    let membership = tokio::spawn(async move {
        call(
            &membership_app,
            "POST",
            &membership_path,
            json!({"position_ids":[position_id]}),
        )
        .await
    });
    // The membership now owns the deck row and waits on the shared card. This makes
    // the dialog race deterministic without changing the application's lock order.
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'INSERT INTO learning_cards%')")
                .fetch_one(&pool).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("membership did not reach its shared-card lock");
    let rename_app = app.clone();
    let rename_path = format!("/api/decks/{target_id}");
    let mut rename = tokio::spawn(async move {
        call(
            &rename_app,
            "PUT",
            &rename_path,
            json!({"name":"Stale dialog","expected_version":0}),
        )
        .await
    });
    let delete_app = app.clone();
    let delete_path = format!("/api/decks/{target_id}?expected_version=0");
    let mut delete =
        tokio::spawn(async move { raw(&delete_app, "DELETE", &delete_path, "").await });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut rename)
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut delete)
            .await
            .is_err()
    );
    card_lock.commit().await.unwrap();
    let membership = membership.await.unwrap();
    assert_eq!(membership.0, 200, "{membership:?}");
    let rename = rename.await.unwrap();
    let delete = delete.await.unwrap();
    assert_eq!(rename.0, 409, "{rename:?}");
    assert_eq!(delete.0, 409, "{delete:?}");
    let current = listed(&app, &target["id"]).await;
    assert_eq!(current["version"], 1);
    assert_eq!(current["name"], "In flight");
    assert_eq!(current["card_count"], 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn rejected_membership_batches_rollback_cards_memberships_and_versions(pool: PgPool) {
    let positions = positions(&pool).await;
    let app = app(&pool);
    let deck = create(&app, "Full deck").await;
    let deck_id = Uuid::parse_str(deck["id"].as_str().unwrap()).unwrap();
    sqlx::query("WITH seeded AS (INSERT INTO learning_cards(id,signature,payload) SELECT gen_random_uuid(),'seed-' || n,'{}'::jsonb FROM generate_series(1,1000) n RETURNING id) INSERT INTO deck_cards(deck_id,card_id) SELECT $1,id FROM seeded")
        .bind(deck_id).execute(&pool).await.unwrap();
    let before = listed(&app, &deck["id"]).await;
    let (status, error) = call(
        &app,
        "POST",
        &format!("/api/decks/{deck_id}/positions"),
        json!({"position_ids":[positions[0].id]}),
    )
    .await;
    assert_eq!(status, 422, "{error}");
    assert_eq!(listed(&app, &deck["id"]).await, before);
    let cards: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_cards")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cards, 1000);
}

#[test]
fn openapi_describes_required_versions_and_mutation_errors() {
    use utoipa::OpenApi;
    let doc = serde_json::to_value(neondojo::api::ApiDoc::openapi()).unwrap();
    let schema = &doc["components"]["schemas"];
    assert_eq!(schema["Deck"]["properties"]["version"]["type"], "integer");
    assert!(
        schema["Deck"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("version"))
    );
    assert!(
        schema["RenameDeck"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("expected_version"))
    );
    for method in ["put", "delete"] {
        let operation = &doc["paths"]["/api/decks/{id}"][method];
        for status in ["404", "409", "422"] {
            assert!(
                operation["responses"][status].is_object(),
                "{method} {status}"
            );
        }
    }
    assert!(doc["paths"]["/api/decks/{id}"]["put"]["responses"]["200"].is_object());
    let delete = &doc["paths"]["/api/decks/{id}"]["delete"];
    assert!(delete["responses"]["204"].get("content").is_none());
    let expected = delete["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "expected_version")
        .unwrap();
    assert_eq!(expected["in"], "query");
    assert_eq!(expected["required"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn cards_move_and_duplicates_can_be_resolved_without_changing_keeper_progress(pool: PgPool) {
    let pos = positions(&pool).await;
    let app = app(&pool);
    let source = create(&app, "Source").await;
    let target = create(&app, "Target").await;
    let source_id = source["id"].as_str().unwrap();
    let target_id = target["id"].as_str().unwrap();
    for (deck, indices) in [(source_id, &[0, 1][..]), (target_id, &[0][..])] {
        for index in indices {
            let (status, result) = call(
                &app,
                "POST",
                &format!("/api/decks/{deck}/cards"),
                json!({"position_id":pos[*index].id}),
            )
            .await;
            assert_eq!(status, 200, "{result}");
        }
    }
    let (_, source_cards) = call(
        &app,
        "GET",
        &format!("/api/decks/{source_id}/cards"),
        Value::Null,
    )
    .await;
    let cards = source_cards["items"].as_array().unwrap();
    let shared = cards
        .iter()
        .find(|c| {
            c["browse"]["decks"]
                .as_array()
                .is_some_and(|d| d.len() == 2)
        })
        .unwrap_or_else(|| panic!("{source_cards}"));
    review(&app, shared).await;
    let shared_id = shared["id"].as_str().unwrap();
    let duplicate_path = "/api/cards/duplicates";
    let (status, duplicates) = call(&app, "GET", duplicate_path, Value::Null).await;
    assert_eq!(status, 200, "{duplicates}");
    assert_eq!(duplicates["items"][0]["card"]["id"], shared_id);
    assert_eq!(duplicates["items"][0]["decks"].as_array().unwrap().len(), 2);

    let move_path = format!("/api/decks/{source_id}/move-cards");
    let body = json!({
        "source_version": 2, "target_version": 1, "target_id": target_id,
        "card_ids": cards.iter().map(|c| &c["id"]).collect::<Vec<_>>()
    });
    let (status, moved) = call(&app, "POST", &move_path, body.clone()).await;
    assert_eq!(
        (status, moved),
        (200, json!({"moved":2,"already_in_target":1}))
    );
    assert_eq!(call(&app, "POST", &move_path, body).await.0, 409);
    assert_eq!(listed(&app, &source["id"]).await["card_count"], 0);
    assert_eq!(listed(&app, &target["id"]).await["card_count"], 2);

    let duplicate_id = Uuid::new_v4();
    sqlx::query("INSERT INTO learning_cards(id,signature,payload) SELECT $1,'legacy duplicate',payload FROM learning_cards WHERE id=$2")
        .bind(duplicate_id).bind(Uuid::parse_str(shared_id).unwrap()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO deck_cards(deck_id,card_id) VALUES($1,$2)")
        .bind(Uuid::parse_str(source_id).unwrap())
        .bind(duplicate_id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, found) = call(&app, "GET", duplicate_path, Value::Null).await;
    let entries = found["items"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    let keeper = entries
        .iter()
        .find(|c| c["card"]["id"] == shared_id)
        .unwrap();
    assert_eq!(keeper["copies"], 2);
    assert_eq!(keeper["card"]["review_count"], 1);
    let duplicate = entries
        .iter()
        .find(|c| c["card"]["id"] == json!(duplicate_id))
        .unwrap();
    let (status, numbered) = call(&app, "GET", "/api/cards/duplicates?page=1", Value::Null).await;
    assert_eq!(status, 200, "{numbered}");
    assert_eq!(numbered["total"], 2);
    let key = keeper["duplicate_key"].as_str().unwrap();
    let peers = numbered["peers"][key].as_array().unwrap();
    assert!(peers.iter().any(|peer| peer["id"] == json!(duplicate_id)));
    assert!(peers.iter().any(|peer| peer["id"] == json!(shared_id)));
    review(&app, &duplicate["card"]).await;
    let mut merge = json!({"keep_id":shared_id,"keep_version":keeper["card"]["version"],"delete_id":duplicate_id,"delete_version":0});
    assert_eq!(
        call(&app, "POST", "/api/cards/duplicates/resolve", merge.clone())
            .await
            .0,
        409
    );
    merge["delete_version"] = json!(1);
    assert_eq!(
        call(&app, "POST", "/api/cards/duplicates/resolve", merge)
            .await
            .0,
        204
    );
    let removed_reviews: i64 =
        sqlx::query_scalar("SELECT count(*) FROM card_reviews WHERE card_id=$1")
            .bind(duplicate_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(removed_reviews, 0);
    assert_eq!(listed(&app, &source["id"]).await["card_count"], 1);
    let (status, removed) = call(
        &app,
        "POST",
        &format!("/api/decks/{source_id}/remove-cards"),
        json!({"expected_version":4,"card_ids":[shared_id]}),
    )
    .await;
    assert_eq!((status, removed), (204, Value::Null));
    let (_, result) = call(
        &app,
        "GET",
        &format!("/api/decks/{target_id}/cards"),
        Value::Null,
    )
    .await;
    assert!(
        result["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == shared_id && c["review_count"] == 1)
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn cubeful_and_nocube_cards_are_not_grouped_as_duplicates_and_nocube_duplicates_resolve(
    pool: PgPool,
) {
    let positions = positions(&pool).await;
    let app = app(&pool);
    let deck = create(&app, "Cube rules").await;
    let deck_id = Uuid::parse_str(deck["id"].as_str().unwrap()).unwrap();
    add(&app, &deck, &[positions[0].id]).await;

    let (_, listed_cards) = call(
        &app,
        "GET",
        &format!("/api/decks/{deck_id}/cards"),
        Value::Null,
    )
    .await;
    let cubeful = &listed_cards["items"][0];
    let cubeful_id = Uuid::parse_str(cubeful["id"].as_str().unwrap()).unwrap();

    // Add a NoCube card for the exact same position_id, gnubg_match_id and kind.
    let nocube_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO learning_cards(id,signature,payload)
         SELECT $1, signature || ':nocube', jsonb_set(payload, '{position,cube_enabled}', 'false'::jsonb, true)
         FROM learning_cards WHERE id=$2",
    )
    .bind(nocube_id)
    .bind(cubeful_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO deck_cards(deck_id,card_id) VALUES($1,$2)")
        .bind(deck_id)
        .bind(nocube_id)
        .execute(&pool)
        .await
        .unwrap();

    // Cubeful and NoCube cards have distinct identities and must not be grouped as duplicates.
    let (status, duplicates) = call(&app, "GET", "/api/cards/duplicates?page=1", Value::Null).await;
    assert_eq!(status, 200, "{duplicates}");
    assert_eq!(duplicates["total"], 0, "{duplicates}");
    assert!(duplicates["items"].as_array().unwrap().is_empty());

    // Attempting to resolve a cubeful card against a NoCube card is rejected with 422.
    let (status, error) = call(
        &app,
        "POST",
        "/api/cards/duplicates/resolve",
        json!({"keep_id": cubeful_id, "keep_version": 0, "delete_id": nocube_id, "delete_version": 0}),
    )
    .await;
    assert_eq!(status, 422, "{error}");

    // Insert a real duplicate of the NoCube card with a noncanonical legacy signature.
    let nocube_dup_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO learning_cards(id,signature,payload)
         SELECT $1, 'legacy nocube duplicate', payload FROM learning_cards WHERE id=$2",
    )
    .bind(nocube_dup_id)
    .bind(nocube_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO deck_cards(deck_id,card_id) VALUES($1,$2)")
        .bind(deck_id)
        .bind(nocube_dup_id)
        .execute(&pool)
        .await
        .unwrap();

    let (status, duplicates) = call(&app, "GET", "/api/cards/duplicates?page=1", Value::Null).await;
    assert_eq!(status, 200, "{duplicates}");
    assert_eq!(duplicates["total"], 2, "{duplicates}");
    let entries = duplicates["items"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .all(|entry| entry["card"]["id"] != json!(cubeful_id))
    );
    let key = entries[0]["duplicate_key"].as_str().unwrap();
    assert!(key.ends_with(":nocube"), "expected :nocube key, got {key}");
    assert_eq!(duplicates["peers"][key].as_array().unwrap().len(), 2);

    // Resolving the two NoCube duplicates succeeds and preserves the canonical :nocube signature.
    let (status, resolved) = call(
        &app,
        "POST",
        "/api/cards/duplicates/resolve",
        json!({"keep_id": nocube_dup_id, "keep_version": 0, "delete_id": nocube_id, "delete_version": 0}),
    )
    .await;
    assert_eq!((status, resolved), (204, Value::Null));
    let kept_signature: String =
        sqlx::query_scalar("SELECT signature FROM learning_cards WHERE id=$1")
            .bind(nocube_dup_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kept_signature, key);

    let (status, after) = call(&app, "GET", "/api/cards/duplicates?page=1", Value::Null).await;
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["total"], 0, "{after}");
    assert!(after["items"].as_array().unwrap().is_empty());
}
