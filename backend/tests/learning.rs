use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{api, engine::Engine, import, jobs, store};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

static ANKI_CONNECT_TESTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static EXPORT_TESTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[sqlx::test(migrations = "./migrations")]
async fn anki_retry_open_errors_requires_open_history_for_this_deck(pool: PgPool) {
    let deck: Uuid = sqlx::query_scalar(
        "INSERT INTO decks(id,name) VALUES(gen_random_uuid(),'Linked') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO anki_auto_sync(deck_id,link_id,profile,collection_key)
                 VALUES($1,gen_random_uuid(),'Profile','key')",
    )
    .bind(deck)
    .execute(&pool)
    .await
    .unwrap();
    let app = api::router(api::AppState::new(pool, Engine::new("unused".into())));
    let _anki_guard = ANKI_CONNECT_TESTS.lock().await;
    let (status, _) = call(
        app,
        "POST",
        &format!("/api/decks/{deck}/anki-auto-sync/action"),
        json!({"action":"retry_open_errors"}),
    )
    .await;
    assert_eq!(status, 409);
}

#[sqlx::test(migrations = "./migrations")]
async fn anki_retry_rejects_unknown_or_foreign_history_entries(pool: PgPool) {
    let deck = Uuid::new_v4();
    let other = Uuid::new_v4();
    let foreign_run = Uuid::new_v4();
    sqlx::query("INSERT INTO decks(id,name) VALUES($1,'Linked'),($2,'Other')")
        .bind(deck)
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO anki_auto_sync(deck_id,link_id,profile,collection_key)
                 VALUES($1,gen_random_uuid(),'Profile','key')",
    )
    .bind(deck)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO anki_sync_history(id,deck_id,profile,received_reviews,sent_reviews,outcome,problem,error_detail)
                 VALUES($1,$2,'Profile',0,0,'failed','conflict','Profile changed')")
        .bind(foreign_run).bind(other).execute(&pool).await.unwrap();
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    let path = format!("/api/decks/{deck}/anki-auto-sync/action");
    let _anki_guard = ANKI_CONNECT_TESTS.lock().await;
    for id in [foreign_run, Uuid::new_v4()] {
        let (status, _) = call(
            app.clone(),
            "POST",
            &path,
            json!({"action":"sync_now","retry_of":id}),
        )
        .await;
        assert_eq!(status, 409);
    }
    assert_eq!(
        call(
            app,
            "GET",
            &format!("/api/decks/{deck}/anki-auto-sync/history"),
            Value::Null
        )
        .await,
        (200, json!([]))
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn anki_sync_history_is_scoped_and_bounded(pool: PgPool) {
    let deck = Uuid::new_v4();
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO decks(id,name) VALUES($1,'Linked'),($2,'Other')")
        .bind(deck)
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO anki_auto_sync(deck_id,link_id,profile,collection_key)
         VALUES($1,$2,'Test profile','test-key')",
    )
    .bind(deck)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    let path = |id| format!("/api/decks/{id}/anki-auto-sync/history");
    assert_eq!(
        call(app.clone(), "GET", &path(deck), Value::Null).await,
        (200, json!([]))
    );
    sqlx::query(
        "INSERT INTO anki_sync_history(deck_id,profile,finished_at,received_reviews,sent_reviews,outcome,problem,error_detail)
         SELECT $1,'Test profile',now()-make_interval(secs=>i),2,1,'failed','timeout','Connection timed out'
         FROM generate_series(0,100) i
         UNION ALL SELECT $2,'Other',now(),0,0,'synced',NULL,NULL",
    )
    .bind(deck)
    .bind(other)
    .execute(&pool)
    .await
    .unwrap();
    let (status, history) = call(app.clone(), "GET", &path(deck), Value::Null).await;
    assert_eq!(status, 200);
    let rows = history.as_array().unwrap();
    assert_eq!(rows.len(), 100);
    assert!(
        rows.windows(2)
            .all(|pair| pair[0]["finished_at"].as_str() > pair[1]["finished_at"].as_str())
    );
    assert!(rows.iter().all(|row| row["profile"] == "Test profile"));
    assert_eq!(rows[0]["received_reviews"], 2);
    assert_eq!(rows[0]["sent_reviews"], 1);
    assert_eq!(rows[0]["problem"], "timeout");
    assert_eq!(rows[0]["error_detail"], "Connection timed out");
    assert_eq!(call(app, "GET", &path(other), Value::Null).await.0, 404);
}

#[sqlx::test(migrations = "./migrations")]
async fn anki_overview_pause_and_resume_keep_the_link_and_counts(pool: PgPool) {
    let deck = Uuid::new_v4();
    let link = Uuid::new_v4();
    sqlx::query("INSERT INTO decks(id,name) VALUES($1,'Linked')")
        .bind(deck)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO anki_auto_sync(deck_id,link_id,profile,collection_key,received_reviews,sent_reviews)
         VALUES($1,$2,'Test profile','test-key',2,3)",
    )
    .bind(deck).bind(link).execute(&pool).await.unwrap();
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    let path = format!("/api/decks/{deck}/anki-auto-sync/action");
    for (action, expected) in [("pause", true), ("resume", false)] {
        assert_eq!(
            call(app.clone(), "POST", &path, json!({"action":action}))
                .await
                .0,
            204
        );
        let (_, overview) =
            call(app.clone(), "GET", "/api/study/anki-auto-sync", Value::Null).await;
        assert_eq!(overview[0]["paused"], expected);
        assert_eq!(overview[0]["received_reviews"], 2);
        assert_eq!(overview[0]["sent_reviews"], 3);
        if expected {
            assert_eq!(overview[0]["status"], "paused");
        }
    }
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT link_id FROM anki_auto_sync WHERE deck_id=$1")
            .bind(deck)
            .fetch_one(&pool)
            .await
            .unwrap(),
        link
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn anki_sync_overview_counts_open_errors_per_linked_deck(pool: PgPool) {
    let linked = Uuid::new_v4();
    let other = Uuid::new_v4();
    let unlinked = Uuid::new_v4();
    sqlx::query("INSERT INTO decks(id,name) VALUES($1,'Linked'),($2,'Other'),($3,'Unlinked')")
        .bind(linked)
        .bind(other)
        .bind(unlinked)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO anki_auto_sync(deck_id,link_id,profile,collection_key)
         VALUES($1,gen_random_uuid(),'Test profile','test-key'),
               ($2,gen_random_uuid(),'Test profile','test-key')",
    )
    .bind(linked)
    .bind(other)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO anki_sync_history(deck_id,profile,received_reviews,sent_reviews,outcome,problem,error_detail,resolved_at)
         VALUES ($1,'Test profile',0,0,'failed','timeout','Open error',NULL),
                ($1,'Test profile',0,0,'failed','timeout','Resolved error',now()),
                ($1,'Test profile',0,0,'synced',NULL,NULL,NULL),
                ($1,'Test profile',0,0,'no_matches',NULL,NULL,NULL),
                ($2,'Test profile',0,0,'failed','timeout','Unlinked error',NULL)",
    )
    .bind(linked)
    .bind(unlinked)
    .execute(&pool)
    .await
    .unwrap();
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    let (status, body) = call(app.clone(), "GET", "/api/study/anki-auto-sync", Value::Null).await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!([{
            "deck_id":linked,"deck_name":"Linked","profile":"Test profile",
            "last_synced_at":null,"received_reviews":0,"sent_reviews":0,
            "status":"checking","paused":false,"open_errors":1
        },{
            "deck_id":other,"deck_name":"Other","profile":"Test profile",
            "last_synced_at":null,"received_reviews":0,"sent_reviews":0,
            "status":"checking","paused":false,"open_errors":0
        }])
    );
    sqlx::query("UPDATE anki_sync_history SET resolved_at=now() WHERE deck_id=$1 AND error_detail='Open error'")
        .bind(linked)
        .execute(&pool)
        .await
        .unwrap();
    let (_, refreshed) = call(app, "GET", "/api/study/anki-auto-sync", Value::Null).await;
    assert_eq!(refreshed[0]["open_errors"], 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn anki_auto_sync_is_opt_in_and_can_be_disabled_offline(pool: PgPool) {
    let _anki_guard = ANKI_CONNECT_TESTS.lock().await;
    let app = api::router(api::AppState::new(pool, Engine::new("unused".into())));
    let (_, deck) = call(
        app.clone(),
        "POST",
        "/api/decks",
        json!({"name":"Optional sync"}),
    )
    .await;
    let path = format!("/api/decks/{}/anki-auto-sync", deck["id"].as_str().unwrap());
    let (status, settings) = call(app.clone(), "GET", &path, Value::Null).await;
    assert_eq!(status, 200);
    assert_eq!(settings["profile"], Value::Null);
    assert_eq!(
        call(app.clone(), "PUT", &path, json!({"enabled":false}))
            .await
            .0,
        200
    );
    assert_eq!(
        call(app, "PUT", &path, json!({"enabled":true})).await.0,
        422,
        "An empty deck cannot be linked"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn anki_review_sync_rejects_missing_decks_without_contacting_anki(pool: PgPool) {
    let _anki_guard = ANKI_CONNECT_TESTS.lock().await;
    let app = api::router(api::AppState::new(pool, Engine::new("unused".into())));
    let path = format!("/api/decks/{}/sync-anki-reviews", Uuid::new_v4());
    let (status, body) = call(app.clone(), "POST", &path, json!({})).await;
    assert_eq!((status, body), (404, json!({"error":"Not found"})));
    let (_, deck) = call(app.clone(), "POST", "/api/decks", json!({"name":"Empty"})).await;
    let path = format!(
        "/api/decks/{}/sync-anki-reviews",
        deck["id"].as_str().unwrap()
    );
    assert_eq!(call(app, "POST", &path, json!({})).await.0, 422);
}

#[sqlx::test(migrations = "./migrations")]
async fn send_to_anki_validates_the_selected_deck_before_connecting(pool: PgPool) {
    let _anki_guard = ANKI_CONNECT_TESTS.lock().await;
    let app = api::router(api::AppState::new(pool, Engine::new("unused".into())));
    let path = format!("/api/decks/{}/send-to-anki", Uuid::new_v4());
    assert_eq!(
        call(app.clone(), "POST", &path, json!({"locale":"unsupported"}))
            .await
            .0,
        422
    );
    assert_eq!(
        call(app, "POST", &path, json!({"locale":"en"})).await.0,
        404
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn external_anki_upload_rejects_a_non_package(pool: PgPool) {
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    let response = app.oneshot(Request::builder().method("POST").uri("/api/study/anki-imports")
        .header("content-type", "multipart/form-data; boundary=study-test")
        .body(Body::from("--study-test\r\nContent-Disposition: form-data; name=\"file\"; filename=\"bad.apkg\"\r\n\r\nNot a package\r\n--study-test--\r\n"))
        .unwrap()).await.unwrap();
    assert_eq!(response.status(), 422);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM decks")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn learning_summary_counts_unique_due_cards(pool: PgPool) {
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    let url = "/api/study/learning-summary";
    let empty = json!({"card_count":0,"due_count":0,"review_due_count":0,"new_count":0,
        "new_cards_per_day":10,"new_available_today":0,"today_count":0,
        "next_due_at":null,"next_deck":null});
    assert_eq!(
        call(app.clone(), "GET", url, Value::Null).await,
        (200, empty)
    );

    let first_deck = Uuid::from_u128(1);
    let second_deck = Uuid::from_u128(2);
    sqlx::query(
        "INSERT INTO decks(id,name,created_at) VALUES ($2,'Second',now()),($1,'First',now())",
    )
    .bind(first_deck)
    .bind(second_deck)
    .execute(&pool)
    .await
    .unwrap();
    let card = Uuid::from_u128(3);
    let tied = Uuid::from_u128(4);
    let future = Uuid::from_u128(5);
    let orphan = Uuid::from_u128(6);
    let parsed = import::parse_sgf("(;GM[6]MI[length:7];W[31hefe])").unwrap();
    let position = &parsed.positions[0];
    let payload = json!({
        "kind": "checker",
        "position": {
            "id": card, "match_uuid": Uuid::nil(), "ordinal": 1, "game_no": 1,
            "ply": 1, "player": position.player, "dice": position.dice, "board": position.board,
            "position_id": position.position_id, "gnubg_match_id": position.match_id,
            "notation": position.notation
        },
        "players": ["White", "Black"], "prompt": "Answer", "played": position.notation,
        "source": { "kind": "analysis", "job_id": Uuid::nil(), "match_id": Uuid::nil(), "game_no": 1 },
        "solution": { "engine": "Fixture", "plies": 1, "candidates": [] }
    });
    let future_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "WITH inserted AS (
            INSERT INTO learning_cards(id,signature,payload,due_at) VALUES
                ($1,'shared',$5,now()),($2,'tied',$5,now()),
                ($3,'future','{}',now()+interval '1 hour'),
                ($4,'orphan','{}',now()-interval '1 day') RETURNING id,due_at
         ) SELECT due_at FROM inserted WHERE id=$3",
    )
    .bind(card)
    .bind(tied)
    .bind(future)
    .bind(orphan)
    .bind(payload)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO deck_cards(deck_id,card_id) VALUES ($1,$3),($2,$3),($2,$4),($1,$5)")
        .bind(first_deck)
        .bind(second_deck)
        .bind(card)
        .bind(tied)
        .bind(future)
        .execute(&pool)
        .await
        .unwrap();
    let (status, summary) = call(app.clone(), "GET", url, Value::Null).await;
    assert_eq!(status, 200);
    assert_eq!(
        summary,
        json!({"card_count":3,"due_count":2,"review_due_count":0,"new_count":2,
        "new_cards_per_day":10,"new_available_today":2,"today_count":2,
        "next_due_at":future_at,"next_deck":{"id":first_deck,"name":"First"}})
    );
    // Pausing new cards leaves nothing to study today; reviews stay available.
    let preferences = "/api/study/learning-preferences";
    assert_eq!(
        call(
            app.clone(),
            "PUT",
            preferences,
            json!({"new_cards_per_day":1001})
        )
        .await
        .0,
        422
    );
    assert_eq!(
        call(
            app.clone(),
            "PUT",
            preferences,
            json!({"new_cards_per_day":0})
        )
        .await,
        (200, json!({"new_cards_per_day":0}))
    );
    let (_, paused) = call(app.clone(), "GET", url, Value::Null).await;
    assert_eq!(paused["today_count"], 0);
    assert_eq!(paused["new_available_today"], 0);
    assert_eq!(paused["next_deck"], Value::Null);
    let first_next = format!("/api/decks/{first_deck}/next");
    let (_, none) = call(app.clone(), "GET", &first_next, Value::Null).await;
    assert_eq!(none["card"], Value::Null);
    assert_eq!(
        none["counts"],
        json!({"total":2,"due":0,"new":1,"new_available":0})
    );
    sqlx::query("UPDATE learning_cards SET review_count=1 WHERE id=$1")
        .bind(tied)
        .execute(&pool)
        .await
        .unwrap();
    let (_, reviews) = call(app.clone(), "GET", url, Value::Null).await;
    assert_eq!(reviews["review_due_count"], 1);
    assert_eq!(reviews["today_count"], 1);
    assert_eq!(reviews["next_deck"]["id"], json!(second_deck));
    sqlx::query("UPDATE learning_cards SET review_count=0 WHERE id=$1")
        .bind(tied)
        .execute(&pool)
        .await
        .unwrap();
    // A new card reviewed for the first time today uses up the allowance.
    assert_eq!(
        call(
            app.clone(),
            "PUT",
            preferences,
            json!({"new_cards_per_day":1})
        )
        .await
        .0,
        200
    );
    sqlx::query("WITH session AS (INSERT INTO review_sessions(id,card_id,card_version,expires_at) VALUES(gen_random_uuid(),$1,1,now()) RETURNING id) INSERT INTO card_reviews(card_id,request_id,session_id,rating,result,reviewed_at) SELECT $1,gen_random_uuid(),id,'good','{}',now() FROM session")
        .bind(orphan)
        .execute(&pool)
        .await
        .unwrap();
    let (_, used) = call(app.clone(), "GET", url, Value::Null).await;
    assert_eq!(used["new_available_today"], 0);
    sqlx::query("WITH gone AS (DELETE FROM card_reviews WHERE card_id=$1 RETURNING session_id) DELETE FROM review_sessions WHERE card_id=$1")
        .bind(orphan)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(
            app.clone(),
            "PUT",
            preferences,
            json!({"new_cards_per_day":10})
        )
        .await
        .0,
        200
    );
    let next = format!(
        "/api/decks/{}/next",
        summary["next_deck"]["id"].as_str().unwrap()
    );
    let (status, deck_head) = call(app.clone(), "GET", &next, Value::Null).await;
    assert_eq!(status, 200);
    assert_eq!(deck_head["card"]["id"], json!(card));
    assert_eq!(deck_head["card"]["review_count"], 0);
    sqlx::query("DELETE FROM deck_cards WHERE card_id IN ($1,$2)")
        .bind(card)
        .bind(tied)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(app, "GET", url, Value::Null).await.1,
        json!({"card_count":1,"due_count":0,"review_due_count":0,"new_count":0,
            "new_cards_per_day":10,"new_available_today":0,"today_count":0,
            "next_due_at":future_at,"next_deck":null})
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn active_attempts_are_resumable_and_share_the_existing_review_transaction(pool: PgPool) {
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    let (_, deck) = call(app.clone(), "POST", "/api/decks", json!({"name":"Active"})).await;
    let deck_id = Uuid::parse_str(deck["id"].as_str().unwrap()).unwrap();
    let sgf = "(;GM[6]MI[length:7];W[31hefe])";
    let parsed = import::parse_sgf(sgf).unwrap();
    let game = &parsed.positions[0];
    let mut position = json!({
        "id": Uuid::nil(), "match_uuid":Uuid::new_v4(), "ordinal":1, "game_no":1,
        "ply":1,"player":game.player,"dice":game.dice,"board":game.board,
        "position_id":game.position_id,"gnubg_match_id":game.match_id,"notation":game.notation
    });
    for (kind, input, played, best, loss) in [
        (
            "checker",
            json!({"kind":"checker","steps":[{"from":8,"to":5},{"from":6,"to":5}]}),
            "8/5 6/5",
            "8/5 6/5",
            0.0,
        ),
        (
            "cube_offer",
            json!({"kind":"cube_offer","action":"double"}),
            "double",
            "no double",
            0.125,
        ),
        (
            "cube_response",
            json!({"kind":"cube_response","action":"drop"}),
            "take",
            "drop",
            0.0,
        ),
    ] {
        let card = Uuid::new_v4();
        if kind != "checker" {
            // This legacy response ID stores the responder in the position
            // context, with the pre-offer cube and no pending bit.
            let mut bytes = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD_NO_PAD,
                &game.match_id,
            )
            .unwrap();
            bytes[1] &= 0x7f;
            bytes[2] &= 0xe0;
            position["gnubg_match_id"] = json!(base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD_NO_PAD,
                bytes
            ));
            position["dice"] = json!([0, 0]);
        }
        let payload = json!({
            "kind":kind,"position":position,"players":["White","Black"],"prompt":"Answer",
            "source":{"kind":"match","job_id":Uuid::new_v4(),"match_id":position["match_uuid"],"position_id":null,"game_no":1,"decision_no":1},
            "solution":{"engine":"Fixture Engine","plies":2,"candidates":[],"decision":{
                "decision_no":1,"checker_no":null,"player":0,"kind":if kind=="checker" {"checker"} else {"cube"},
                "played":played,"best":best,"played_equity":0.0,"best_equity":if kind=="checker" {0.0} else {0.125},
                "equity_loss":if kind=="checker" {0.0} else {0.125},"severity":"none",
                "dice":if kind=="checker" {json!(game.dice)} else {Value::Null},"evaluation_status":"evaluated"
            }},"played":played
        });
        sqlx::query("INSERT INTO learning_cards(id,signature,payload) VALUES($1,$2,$3)")
            .bind(card)
            .bind(kind)
            .bind(payload)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO deck_cards(deck_id,card_id) VALUES($1,$2)")
            .bind(deck_id)
            .bind(card)
            .execute(&pool)
            .await
            .unwrap();
        let next = format!("/api/decks/{deck_id}/next");
        let attempt = format!("/api/cards/{card}/attempt");
        let reveal = format!("/api/cards/{card}/reveal");
        let review = format!("/api/cards/{card}/review");
        let (_, next_before) = call(app.clone(), "GET", &next, Value::Null).await;
        assert!(next_before["active_attempt"].is_null());
        assert_eq!(next_before["card"]["position"]["notation"], "");
        if kind == "checker" {
            let (illegal, _) = call(
                app.clone(),
                "POST",
                &attempt,
                json!({"expected_version":0,"request_id":Uuid::new_v4(),"answer":{"kind":"checker","steps":[]}}),
            ).await;
            assert_eq!(illegal, 422, "a playable roll cannot be passed");
            let (unavailable, detail) = call(
                app.clone(),
                "POST",
                &attempt,
                json!({"expected_version":0,"request_id":Uuid::new_v4(),"answer":{"kind":"checker","steps":[{"from":24,"to":21},{"from":24,"to":23}]}}),
            ).await;
            assert_eq!(unavailable, 503, "{detail}");
            assert!(
                call(app.clone(), "GET", &next, Value::Null).await.1["active_attempt"].is_null()
            );
        }
        let (_, old_reveal) =
            call(app.clone(), "POST", &reveal, json!({"expected_version":0})).await;
        let first = json!({"expected_version":0,"request_id":Uuid::new_v4(),"answer":input});
        let mut second = first.clone();
        second["request_id"] = json!(Uuid::new_v4());
        let (a, b) = tokio::join!(
            call(app.clone(), "POST", &attempt, first.clone()),
            call(app.clone(), "POST", &attempt, second.clone())
        );
        let (submission, (status, result), rejected) = if a.0 == 200 {
            (first, a, b)
        } else {
            (second, b, a)
        };
        assert_eq!(status, 200, "{kind}: {result}");
        assert_eq!(rejected.0, 409, "{kind}: {rejected:?}");
        assert_eq!(result["evaluation"]["equity_loss"], loss);
        let retry = call(app.clone(), "POST", &attempt, submission.clone()).await;
        assert_eq!(retry, (200, result.clone()));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM review_sessions WHERE card_id=$1 AND attempt_request_id IS NOT NULL")
                .bind(card).fetch_one(&pool).await.unwrap(),
            1
        );
        let (_, restored) = call(app.clone(), "GET", &next, Value::Null).await;
        assert_eq!(restored["active_attempt"], result);
        let (status, summary) = call(
            app.clone(),
            "GET",
            "/api/study/learning-summary",
            Value::Null,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(summary["due_count"], 1);
        assert_eq!(summary["next_deck"]["id"], deck["id"]);
        assert_eq!(
            call(app.clone(), "GET", &next, Value::Null).await,
            (200, restored)
        );
        let bypass = json!({"request_id":Uuid::new_v4(),"session_id":old_reveal["session_id"],"rating":"easy"});
        assert_eq!(call(app.clone(), "POST", &review, bypass).await.0, 409);
        let rating = json!({"request_id":Uuid::new_v4(),"session_id":result["revealed"]["session_id"],"rating":"good"});
        let (a, b) = tokio::join!(
            call(app.clone(), "POST", &review, rating.clone()),
            call(app.clone(), "POST", &review, rating.clone())
        );
        assert_eq!(a.0, 200, "{a:?}");
        assert_eq!(a, b);
        assert_eq!(a.1["review_count"], 1);
        assert_eq!(call(app.clone(), "POST", &attempt, submission).await.0, 409);
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn filtered_practice_retries_errors_and_marked_cards_without_reviews(pool: PgPool) {
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    let parsed = import::parse_sgf("(;GM[6]MI[length:7];W[31hefe])").unwrap();
    let game = &parsed.positions[0];
    let mut context = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &game.match_id,
    )
    .unwrap();
    context[1] &= 0x7f;
    context[2] &= 0xe0;
    let match_id =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD_NO_PAD, context);
    for (id, player) in [(1, "Ada"), (2, "Ada"), (3, "Other")] {
        let card = Uuid::from_u128(id);
        let payload = json!({
            "kind":"cube_offer", "position":{"id":card,"match_uuid":Uuid::nil(),"ordinal":1,
                "game_no":1,"ply":1,"player":game.player,"dice":[0,0],"board":game.board,
                "position_id":game.position_id,"gnubg_match_id":match_id,"notation":"hidden"},
            "players":[player,"Black"],"prompt":"Answer","played":"double",
            "source":{"kind":"match","job_id":Uuid::nil(),"match_id":Uuid::nil(),"game_no":1},
            "solution":{"engine":"Fixture Engine","plies":2,"candidates":[],"decision":{
                "decision_no":1,"player":0,"kind":"cube","played":"double","best":"no double",
                "played_equity":0.0,"best_equity":0.125,"equity_loss":0.125,"severity":"none",
                "evaluation_status":"evaluated"}}
        });
        sqlx::query(
            "INSERT INTO learning_cards(id,signature,payload,due_at) VALUES($1,$2,$3,'2099-01-01')",
        )
        .bind(card)
        .bind(card.to_string())
        .bind(payload)
        .execute(&pool)
        .await
        .unwrap();
    }
    let start = "/api/study/card-rounds?player=Ada&limit=10";
    let (status, mut round) = call(app.clone(), "POST", start, Value::Null).await;
    assert_eq!(status, 201, "{round}");
    let uri = format!("/api/study/card-rounds/{}", round["id"].as_str().unwrap());
    assert_eq!(round["total"], 2);
    let mut first = Vec::new();
    for (action, marked) in [("double", false), ("no_double", true)] {
        first.push(round["card"]["id"].clone());
        assert_eq!(round["card"]["position"]["notation"], "");
        assert!(round["result"].is_null());
        let body = json!({"version":round["version"],"card_version":round["card"]["version"],"request_id":Uuid::new_v4(),"answer":{"kind":"cube_offer","action":action}});
        let answered = call(app.clone(), "POST", &format!("{uri}/answer"), body.clone()).await;
        assert_eq!(answered.0, 200, "{answered:?}");
        assert_eq!(
            call(app.clone(), "POST", &format!("{uri}/answer"), body).await,
            answered
        );
        assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await, answered);
        round = call(
            app.clone(),
            "POST",
            &format!("{uri}/advance"),
            json!({"version":answered.1["version"],"marked":marked}),
        )
        .await
        .1;
    }
    assert_ne!(first[0], first[1]);
    assert_eq!(round["summary"]["retry_count"], 2);
    let retried = call(
        app.clone(),
        "POST",
        &format!("{uri}/retry"),
        json!({"version":round["version"]}),
    )
    .await;
    assert_eq!(retried.0, 200, "{retried:?}");
    assert_eq!(retried.1["total"], 2);
    assert_eq!(retried.1["card"]["id"], first[0]);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM learning_cards WHERE due_at='2099-01-01' AND review_count=0 AND version=0").fetch_one(&pool).await.unwrap(), 3);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM review_sessions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let tag_edit = call(
        app.clone(),
        "PUT",
        "/api/cards/tags",
        json!({
            "cards": first.iter().map(|id| json!({"id":id,"tags_version":0})).collect::<Vec<_>>(),
            "add":["Cube::Take Point", "Technik::Rechnen"], "remove":[]
        }),
    )
    .await;
    assert_eq!(tag_edit.0, 200, "{tag_edit:?}");
    let tagged = call(
        app.clone(),
        "GET",
        "/api/cards?topics=cube%2Ctechnik%3A%3Arechnen",
        Value::Null,
    )
    .await;
    assert_eq!(tagged.0, 200, "{tagged:?}");
    assert_eq!(tagged.1["total"], 2);
    assert_eq!(
        tagged.1["items"][0]["browse"]["card_tags"],
        json!(["cube::take point", "technik::rechnen"])
    );
    let (status, topic) = call(
        app.clone(),
        "POST",
        "/api/study/card-topics",
        json!({"name":"Cube","filter":{"topics":"cube","kind":"cube_offer"}}),
    )
    .await;
    assert_eq!(status, 201, "{topic}");
    let (_, topics) = call(app.clone(), "GET", "/api/study/card-topics", Value::Null).await;
    assert_eq!(topics[0]["filter"]["topics"], "cube");
    let (status, _) = call(
        app.clone(),
        "POST",
        "/api/study/transfer-rounds",
        json!({
            "player":"Ada", "game_plan":"pure_race", "kind":"cube_offer"
        }),
    )
    .await;
    assert_eq!(
        status, 422,
        "An empty comparable pool must not create a round"
    );
    let (status, pips) = call(
        app,
        "POST",
        "/api/study/card-pips",
        json!({"card_id":first[0],"counts":[167,167]}),
    )
    .await;
    assert_eq!(status, 200, "{pips}");
    assert_eq!(pips["counts"], json!([167, 167]));
    assert_eq!(pips["correct"], json!([true, true]));
}

#[sqlx::test(migrations = "./migrations")]
async fn card_game_plan_filter_is_applied_before_practice_selection(pool: PgPool) {
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    let parsed = import::parse_sgf("(;GM[6]AE[a:y]AW[e][e]AB[s]PL[W];W[31ebed])").unwrap();
    let p = &parsed.positions[0];
    let id = Uuid::new_v4();
    let payload = json!({
        "kind":"checker", "position":{"id":id,"match_uuid":Uuid::nil(),"ordinal":1,"game_no":1,"ply":1,
        "player":p.player,"dice":p.dice,"board":p.board,"position_id":p.position_id,"gnubg_match_id":p.match_id,"notation":p.notation},
        "players":["White","Black"],"prompt":"Answer","played":p.notation,
        "source":{"kind":"match","job_id":Uuid::nil(),"match_id":Uuid::nil(),"game_no":1},
        "solution":{"engine":"Fixture","plies":1,"candidates":[]}
    });
    sqlx::query("INSERT INTO learning_cards(id,signature,payload) VALUES($1,$2,$3)")
        .bind(id)
        .bind(id.to_string())
        .bind(payload)
        .execute(&pool)
        .await
        .unwrap();
    let (status, result) = call(
        app.clone(),
        "GET",
        "/api/cards?game_plan=pure_race",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["total"], 1);
    let (status, round) = call(
        app.clone(),
        "POST",
        "/api/study/card-rounds?game_plan=pure_race",
        Value::Null,
    )
    .await;
    assert_eq!(status, 201, "{round}");
    assert_eq!(round["card"]["id"], json!(id));
    assert_eq!(
        call(
            app,
            "POST",
            "/api/study/card-rounds?game_plan=priming",
            Value::Null
        )
        .await
        .0,
        422
    );
}

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
    (
        status,
        if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        },
    )
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn cards_share_identity_and_preserve_atomic_fsrs_reviews(pool: PgPool) {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let sgf = "(;GM[6]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[31hefe])";
    let imported = store::import_match(
        &pool,
        "learning.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let position = store::positions(&pool, Some(imported.r#match.id), None, None, 100)
        .await
        .unwrap()
        .items
        .remove(0);
    let state = api::AppState::new(pool.clone(), engine);
    let app = api::router(state.clone());
    let (status, deck) = call(
        app.clone(),
        "POST",
        "/api/decks",
        json!({"name":"Practice"}),
    )
    .await;
    assert_eq!(status, 201, "{deck}");
    let deck_id = deck["id"].as_str().unwrap();
    let path = format!("/api/decks/{deck_id}/cards");
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &path,
            json!({"position_id":position.id})
        )
        .await
        .0,
        422
    );
    let (_, job) = call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":position.id,"kind":"analysis","plies":1}),
    )
    .await;
    assert!(jobs::run_one(&state).await.unwrap());
    assert_eq!(
        call(
            app.clone(),
            "GET",
            &format!("/api/jobs/{}", job["id"].as_str().unwrap()),
            Value::Null
        )
        .await
        .1["status"],
        "completed"
    );
    let (status, added) = call(
        app.clone(),
        "POST",
        &path,
        json!({"position_id":position.id}),
    )
    .await;
    assert_eq!(status, 200, "{added}");
    assert_eq!(added["added"], 1);
    let (_, next) = call(
        app.clone(),
        "GET",
        &format!("/api/decks/{deck_id}/next"),
        Value::Null,
    )
    .await;
    let card = &next["card"];
    let id = card["id"].as_str().unwrap();
    assert_eq!(card["position"]["notation"], "");
    assert!(card.get("answer").is_none());
    for (kind, target, tags, comment) in [
        (
            "matches",
            imported.r#match.id,
            json!(["shared", "race"]),
            "Match lesson",
        ),
        (
            "positions",
            position.id,
            json!(["shared", "tactics"]),
            "Position lesson",
        ),
    ] {
        let (status, result) = call(
            app.clone(),
            "PUT",
            &format!("/api/{kind}/{target}/annotation"),
            json!({"tags": tags, "comment": comment, "expected_version": 0}),
        )
        .await;
        assert_eq!(status, 200, "{result}");
    }
    assert!(!serde_json::to_string(card).unwrap().contains("lesson"));
    let reveal_path = format!("/api/cards/{id}/reveal");
    let review_path = format!("/api/cards/{id}/review");
    let (status, reveal) = call(
        app.clone(),
        "POST",
        &reveal_path,
        json!({"expected_version":0}),
    )
    .await;
    assert_eq!(status, 200, "{reveal}");
    assert!(
        !reveal["answer"]["solution"]["candidates"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        reveal["answer"]["tags"],
        json!(["race", "shared", "tactics"])
    );
    assert_eq!(reveal["answer"]["notes"], "Match lesson\n\nPosition lesson");
    assert_eq!(reveal["intervals"]["again"], 60);
    assert_eq!(reveal["intervals"]["hard"], 600);
    assert!(reveal["intervals"]["good"].as_i64().unwrap() >= 86400);
    assert!(
        reveal["intervals"]["easy"].as_i64().unwrap()
            > reveal["intervals"]["good"].as_i64().unwrap()
    );
    let request =
        json!({"session_id":reveal["session_id"],"request_id":Uuid::new_v4(),"rating":"good"});
    let (a, b) = tokio::join!(
        call(app.clone(), "POST", &review_path, request.clone()),
        call(app.clone(), "POST", &review_path, request.clone())
    );
    assert_eq!(a.0, 200, "{:?}", a);
    assert_eq!(b.0, 200, "{:?}", b);
    assert_eq!(a.1, b.1);
    assert_eq!(a.1["version"], 1);
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &reveal_path,
            json!({"expected_version":0})
        )
        .await
        .0,
        409
    );
    let mut conflicting = request.clone();
    conflicting["rating"] = json!("easy");
    assert_eq!(
        call(app.clone(), "POST", &review_path, conflicting).await.0,
        409
    );
    let (status, stale) = call(
        app.clone(),
        "POST",
        &review_path,
        json!({"session_id":reveal["session_id"],"request_id":Uuid::new_v4(),"rating":"easy"}),
    )
    .await;
    assert_eq!(status, 409, "{stale}");
    assert!(
        call(
            app.clone(),
            "GET",
            &format!("/api/decks/{deck_id}/next"),
            Value::Null
        )
        .await
        .1["card"]
            .is_null()
    );
    let (_, second) = call(app.clone(), "POST", "/api/decks", json!({"name":"Other"})).await;
    let second_path = format!("/api/decks/{}/cards", second["id"].as_str().unwrap());
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &second_path,
            json!({"position_id":position.id})
        )
        .await
        .1["added"],
        1
    );
    let (_, cards) = call(app.clone(), "GET", &second_path, Value::Null).await;
    assert_eq!(cards["items"][0]["id"], id);
    assert_eq!(cards["items"][0]["version"], 1);
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &path,
            json!({"position_id":position.id})
        )
        .await
        .1["added"],
        0
    );
    let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM card_reviews")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reviews, 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn match_cards_include_cube_offers_responses_and_recover_original_boards(pool: PgPool) {
    let sgf = "(;GM[6]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])\n(;GM[6]MI[length:7][ws:1][bs:0]AE[a:y]AW[e]AB[s]PL[B]CV[2]CO[b];B[double];W[take];B[31svvw])\n(;GM[6]MI[length:7][ws:1][bs:0]AE[a:y]AW[e]AB[s]PL[B]CV[2]CO[b];B[double];W[drop])";
    let parsed = import::parse_sgf(sgf).unwrap();
    let imported = store::import_match(
        &pool,
        "cube-learning.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &parsed,
    )
    .await
    .unwrap();
    let state = api::AppState::new(
        pool.clone(),
        Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())),
    );
    let app = api::router(state.clone());
    let (_, job) = call(
        app.clone(),
        "POST",
        &format!("/api/matches/{}/analysis", imported.r#match.id),
        json!({"plies":1}),
    )
    .await;
    assert!(jobs::run_one(&state).await.unwrap());
    let (_, done) = call(
        app.clone(),
        "GET",
        &format!("/api/jobs/{}", job["id"].as_str().unwrap()),
        Value::Null,
    )
    .await;
    assert_eq!(done["status"], "completed", "{done}");
    let (_, deck) = call(
        app.clone(),
        "POST",
        "/api/decks",
        json!({"name":"All decisions"}),
    )
    .await;
    let path = format!("/api/decks/{}/cards", deck["id"].as_str().unwrap());
    let (status, added) = call(app.clone(), "POST", &path, json!({"job_id":job["id"]})).await;
    assert_eq!(status, 200, "{added}");
    let (_, page) = call(app.clone(), "GET", &path, Value::Null).await;
    let cards = page["items"].as_array().unwrap();
    assert!(cards.iter().any(|c| c["kind"] == "checker"));
    assert!(cards.iter().any(|c| c["kind"] == "cube_offer"));
    let cube = cards
        .iter()
        .find(|card| card["kind"] == "cube_response")
        .unwrap();
    let score_path = format!("/api/cards/{}/score-variants", cube["id"].as_str().unwrap());
    let (status, variants) = call(app.clone(), "POST", &score_path, Value::Null).await;
    assert_eq!(status, 200, "{variants}");
    assert_eq!(variants["questions"].as_array().unwrap().len(), 3);
    let (status, evaluation) = call(app.clone(), "PUT", &score_path, json!({
        "index":0, "practice_id":variants["id"], "answer":{"kind":"cube_response","action":"drop"}
    })).await;
    assert_eq!(status, 200, "{evaluation}");
    assert_eq!(evaluation["provenance"]["kind"], "recomputed");
    assert!(cards.iter().any(|c| c["kind"] == "cube_response"));
    let decisions: Vec<(i32, Value)> = sqlx::query_as(
        "SELECT game_no,data FROM move_analyses WHERE job_id=$1 ORDER BY game_no,decision_no",
    )
    .bind(Uuid::parse_str(job["id"].as_str().unwrap()).unwrap())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(decisions.iter().any(|(_, d)| d["played"] == "no double"));
    // The identical offer/response state in games 2 and 3 is one library card.
    assert_eq!(cards.len(), decisions.len() - 2);
    for card in cards {
        let game = card["source"]["game_no"].as_i64().unwrap();
        if game >= 2 {
            let board = card["position"]["board"].as_array().unwrap();
            assert_eq!(board[5], 1);
            assert_eq!(board[19], -1);
            if card["kind"] != "checker" {
                assert_eq!(card["position"]["dice"], json!([0, 0]));
            }
            assert_eq!(
                card["position"]["player"],
                if card["kind"] == "cube_response" {
                    0
                } else {
                    1
                }
            );
        }
    }
    let (_, filtered) = call(
        app.clone(),
        "POST",
        "/api/decks",
        json!({"name":"Black errors"}),
    )
    .await;
    let filter_path = format!("/api/decks/{}/cards", filtered["id"].as_str().unwrap());
    let (status, added) = call(
        app.clone(),
        "POST",
        &filter_path,
        json!({"job_id":job["id"],"player":1,"min_loss":0.04}),
    )
    .await;
    assert_eq!(status, 200, "{added}");
    let expected = decisions
        .iter()
        .filter(|(_, d)| d["player"] == 1 && d["equity_loss"].as_f64().unwrap() >= 0.04)
        .count();
    assert!(added["total"].as_u64().unwrap() <= expected as u64);
    let (_, filtered_page) = call(app.clone(), "GET", &filter_path, Value::Null).await;
    for c in filtered_page["items"].as_array().unwrap() {
        assert_eq!(c["position"]["player"], 1);
        let game = c["source"]["game_no"].as_i64().unwrap() as i32;
        let number = &c["source"]["decision_no"];
        assert!(decisions.iter().any(|(g, d)| *g == game
            && &d["decision_no"] == number
            && d["equity_loss"].as_f64().unwrap() >= 0.04));
    }
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &path,
            json!({"job_id":job["id"],"position_id":Uuid::new_v4()})
        )
        .await
        .0,
        422
    );
    assert_eq!(
        call(
            app.clone(),
            "GET",
            &format!(
                "/api/decks/{}/export.apkg?locale=xx",
                deck["id"].as_str().unwrap()
            ),
            Value::Null
        )
        .await
        .0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn malformed_learning_payloads_use_json_error_contract(pool: PgPool) {
    let app = api::router(api::AppState::new(pool, Engine::new("gnubg".into())));
    for (path, body) in [
        ("/api/decks".to_owned(), json!({"name":42})),
        (
            format!("/api/cards/{}/review", Uuid::new_v4()),
            json!({"session_id":Uuid::new_v4(),"request_id":Uuid::new_v4(),"rating":"unknown"}),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 422);
        assert_eq!(response.headers()["content-type"], "application/json");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(serde_json::from_slice::<Value>(&bytes).unwrap()["error"].is_string());
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn deck_size_limit_rolls_back_addition_and_bounds_exports(pool: PgPool) {
    let sgf = "(;GM[6];W[31hefe])";
    let imported = store::import_match(
        &pool,
        "bound.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let p = store::positions(&pool, Some(imported.r#match.id), None, None, 1)
        .await
        .unwrap()
        .items
        .remove(0);
    let state = api::AppState::new(pool.clone(), Engine::new("gnubg".into()));
    let app = api::router(state.clone());
    call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":p.id,"kind":"analysis","plies":1}),
    )
    .await;
    assert!(jobs::run_one(&state).await.unwrap());
    let (_, deck) = call(
        app.clone(),
        "POST",
        "/api/decks",
        json!({"name":"At limit"}),
    )
    .await;
    let id = Uuid::parse_str(deck["id"].as_str().unwrap()).unwrap();
    sqlx::query("WITH inserted AS (INSERT INTO learning_cards(id,signature,payload) SELECT gen_random_uuid(),'bound-'||n,'{}'::jsonb FROM generate_series(1,1000) n RETURNING id) INSERT INTO deck_cards(deck_id,card_id) SELECT $1,id FROM inserted").bind(id).execute(&pool).await.unwrap();
    let (status, error) = call(
        app.clone(),
        "POST",
        &format!("/api/decks/{id}/cards"),
        json!({"position_id":p.id}),
    )
    .await;
    assert_eq!(status, 422, "{error}");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM learning_cards")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1000
    );
    sqlx::query("WITH inserted AS (INSERT INTO learning_cards(id,signature,payload) VALUES($2,'over-bound','{}'::jsonb) RETURNING id) INSERT INTO deck_cards(deck_id,card_id) SELECT $1,id FROM inserted").bind(id).bind(Uuid::new_v4()).execute(&pool).await.unwrap();
    let (status, error) = call(
        app.clone(),
        "GET",
        &format!("/api/decks/{id}/export.apkg"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 422, "{error}");
}

async fn export_bytes(app: axum::Router, id: Uuid, locale: &str) -> Vec<u8> {
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/decks/{id}/export.apkg?locale={locale}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&bytes));
    bytes.to_vec()
}
fn package_note(bytes: &[u8]) -> (String, i64, String, String) {
    package_notes(bytes).remove(0)
}
fn package_notes(bytes: &[u8]) -> Vec<(String, i64, String, String)> {
    use std::io::{Cursor, Read};
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut db = Vec::new();
    zip.by_name("collection.anki2")
        .unwrap()
        .read_to_end(&mut db)
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("collection.anki2");
    std::fs::write(&path, db).unwrap();
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut statement = conn
        .prepare("SELECT guid,mod,flds,tags FROM notes ORDER BY id")
        .unwrap();
    statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn anki_exports_update_monotonically_for_annotations_and_locales(pool: PgPool) {
    let _export_guard = EXPORT_TESTS.lock().await;
    let sgf = "(;GM[6]PW[Alice]PB[Bob];W[31hefe])";
    let imported = store::import_match(
        &pool,
        "export-update.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let p = store::positions(&pool, Some(imported.r#match.id), None, None, 1)
        .await
        .unwrap()
        .items
        .remove(0);
    let state = api::AppState::new(pool.clone(), Engine::new("gnubg".into()));
    let app = api::router(state.clone());
    call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":p.id,"kind":"analysis","plies":1}),
    )
    .await;
    assert!(jobs::run_one(&state).await.unwrap());
    let (_, deck) = call(
        app.clone(),
        "POST",
        "/api/decks",
        json!({"name":"Export updates"}),
    )
    .await;
    let id = Uuid::parse_str(deck["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &format!("/api/decks/{id}/cards"),
            json!({"position_id":p.id})
        )
        .await
        .0,
        200
    );
    let before = export_bytes(app.clone(), id, "de").await;
    let (status, target) = call(
        app.clone(),
        "POST",
        "/api/decks",
        json!({"name":"Portable copy"}),
    )
    .await;
    assert_eq!(status, 201);
    let target_id = target["id"].as_str().unwrap();
    let boundary = "neondojo-package-test";
    let mut multipart = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"study.apkg\"\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();
    multipart.extend_from_slice(&before);
    multipart.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let uploaded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/decks/{target_id}/import.apkg"))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(multipart))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(uploaded.status(), 200);
    let imported: Value =
        serde_json::from_slice(&uploaded.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(imported, json!({"added":0,"updated":1,"total":1}));
    let first = package_note(&before);
    let (a, b) = tokio::join!(
        export_bytes(app.clone(), id, "de"),
        export_bytes(app.clone(), id, "de")
    );
    assert_eq!(package_note(&a), first);
    assert_eq!(package_note(&b), first);
    let (status,result)=call(app.clone(),"PUT",&format!("/api/positions/{}/annotation",p.id),json!({"tags":["changed"],"comment":"Updated <script>lesson</script>","expected_version":0})).await;
    assert_eq!(status, 200, "{result}");
    let changed = export_bytes(app.clone(), id, "de").await;
    let second = package_note(&changed);
    assert_eq!(second.0, first.0);
    assert!(second.1 > first.1);
    assert!(
        second
            .2
            .contains("Updated &lt;script&gt;lesson&lt;/script&gt;")
    );
    assert!(second.3.contains("changed"));
    let after = export_bytes(app.clone(), id, "en").await;
    let third = package_note(&after);
    assert_eq!(third.0, first.0);
    assert!(third.1 > second.1);
    assert!(third.2.contains("Best play"));
    assert_eq!(
        package_note(&export_bytes(app.clone(), id, "en").await),
        third
    );
    let (_, cards) = call(
        app.clone(),
        "GET",
        &format!("/api/decks/{id}/cards"),
        Value::Null,
    )
    .await;
    assert_eq!(cards["items"][0]["version"], 0);
    assert_eq!(cards["items"][0]["review_count"], 0);
    let entries: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_card_exports")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(entries, 1);
    if let Ok(dir) = std::env::var("NEONDOJO_EXPORT_FIXTURES") {
        std::fs::write(
            std::path::Path::new(&dir).join("neondojo-export-before.apkg"),
            before,
        )
        .unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join("neondojo-export-after.apkg"),
            after,
        )
        .unwrap();
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn blocked_export_reads_current_content_after_acquiring_all_card_locks(pool: PgPool) {
    let _export_guard = EXPORT_TESTS.lock().await;
    let sgf = "(;GM[6]PW[Alice]PB[Bob];W[31hefe];B[42qusu])";
    let imported = store::import_match(
        &pool,
        "gated-export.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let positions = store::positions(&pool, Some(imported.r#match.id), None, None, 100)
        .await
        .unwrap()
        .items;
    let state = api::AppState::new(pool.clone(), Engine::new("gnubg".into()));
    let app = api::router(state.clone());
    let (_, deck_a) = call(
        app.clone(),
        "POST",
        "/api/decks",
        json!({"name":"Overlapping A"}),
    )
    .await;
    let (_, deck_b) = call(
        app.clone(),
        "POST",
        "/api/decks",
        json!({"name":"Overlapping B"}),
    )
    .await;
    let a = Uuid::parse_str(deck_a["id"].as_str().unwrap()).unwrap();
    let b = Uuid::parse_str(deck_b["id"].as_str().unwrap()).unwrap();
    for position in positions {
        call(
            app.clone(),
            "POST",
            "/api/jobs",
            json!({"position_id":position.id,"kind":"analysis","plies":1}),
        )
        .await;
        assert!(jobs::run_one(&state).await.unwrap());
        let (status, result) = call(
            app.clone(),
            "POST",
            &format!("/api/decks/{a}/cards"),
            json!({"position_id":position.id}),
        )
        .await;
        assert_eq!(status, 200, "{result}");
    }
    let cards: Vec<(Uuid, Value)> =
        sqlx::query_as("SELECT id,payload FROM learning_cards ORDER BY signature COLLATE \"C\"")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(cards.len(), 2);
    let early = cards[0].0;
    let common = cards[1].0;
    let common_position =
        Uuid::parse_str(cards[1].1["source"]["position_id"].as_str().unwrap()).unwrap();
    sqlx::query("INSERT INTO deck_cards(deck_id,card_id) VALUES($1,$2)")
        .bind(b)
        .bind(common)
        .execute(&pool)
        .await
        .unwrap();
    let baseline = package_note(&export_bytes(app.clone(), b, "en").await);
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM learning_cards WHERE id=$1 FOR UPDATE")
        .bind(early)
        .fetch_one(&mut *gate)
        .await
        .unwrap();
    let app_a = app.clone();
    let pending_a = tokio::spawn(async move { export_bytes(app_a, a, "en").await });
    // Observe the database lock wait, rather than relying on renderer timing or an arbitrary sleep.
    tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%learning_cards%' AND query LIKE '%FOR UPDATE%')").fetch_one(&pool).await.unwrap();
            if waiting {break;}
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.expect("export A must reach its first card lock");
    let (status,result)=call(app.clone(),"PUT",&format!("/api/positions/{common_position}/annotation"),json!({"tags":["latest"],"comment":"Latest concurrently edited notes","expected_version":0})).await;
    assert_eq!(status, 200, "{result}");
    let newer_b = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        export_bytes(app.clone(), b, "en"),
    )
    .await
    .expect("B must export X while A waits for unrelated Y");
    let newer = package_note(&newer_b);
    assert_eq!(newer.0, baseline.0);
    assert!(newer.1 > baseline.1);
    assert!(newer.2.contains("Latest concurrently edited notes"));
    gate.commit().await.unwrap();
    let later_a = tokio::time::timeout(std::time::Duration::from_secs(5), pending_a)
        .await
        .unwrap()
        .unwrap();
    let from_a = package_notes(&later_a)
        .into_iter()
        .find(|n| n.0 == newer.0)
        .unwrap();
    assert_eq!(
        from_a.2, newer.2,
        "a blocked export must reread the latest shared-card content"
    );
    assert_eq!(
        from_a.1, newer.1,
        "the same content must retain its existing Anki modification time"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn imported_anki_cards_can_be_revealed_and_rated_without_a_board(pool: PgPool) {
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    let (_, deck) = call(app.clone(), "POST", "/api/decks", json!({"name":"Anki"})).await;
    let deck = deck["id"].as_str().unwrap();
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO learning_cards(id,signature,payload) VALUES($1,$2,$3)")
        .bind(id)
        .bind(format!("anki:{id}"))
        .bind(json!({"kind":"anki","prompt":"Take Point"}))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO deck_cards(deck_id,card_id) VALUES($1,$2)")
        .bind(Uuid::parse_str(deck).unwrap())
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    let (status, next) = call(
        app.clone(),
        "GET",
        &format!("/api/decks/{deck}/next"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(next["card"]["kind"], "anki");
    assert!(next["card"]["position"].is_null());
    assert_eq!(next["card"]["prompt"], "Take Point");

    // Every practice board and the duplicate check reject the card by name.
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO learning_cards(id,signature,payload) SELECT $1,'anki:other',payload FROM learning_cards WHERE id=$2")
        .bind(other)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    for (path, body, error) in [
        (
            "/api/study/card-pips".to_owned(),
            json!({"card_id":id,"counts":[0,0]}),
            "This Anki card has no practice board",
        ),
        (
            format!("/api/cards/{id}/score-variants"),
            Value::Null,
            "This Anki card has no cube practice board",
        ),
        (
            "/api/cards/duplicates/resolve".to_owned(),
            json!({"keep_id":id,"keep_version":0,"delete_id":other,"delete_version":0}),
            "Anki cards are not compared for duplicates",
        ),
    ] {
        let (status, body) = call(app.clone(), "POST", &path, body).await;
        assert_eq!((status, &body["error"]), (422, &json!(error)), "{path}");
    }

    let (status, revealed) = call(
        app.clone(),
        "POST",
        &format!("/api/cards/{id}/reveal"),
        json!({"expected_version":next["card"]["version"]}),
    )
    .await;
    assert_eq!(status, 200);
    assert!(revealed["answer"]["solution"].is_null());
    let (status, result) = call(
        app,
        "POST",
        &format!("/api/cards/{id}/review"),
        json!({"session_id":revealed["session_id"],"request_id":Uuid::new_v4(),"rating":"good"}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(result["review_count"], 1);
}

async fn seed_card_topics(pool: &PgPool, count: i32) {
    sqlx::query(
        "INSERT INTO card_topics(id,name,filter)
         SELECT gen_random_uuid(),'Seed '||g,'{}'::jsonb FROM generate_series(1,$1) g",
    )
    .bind(count)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn card_topic_limit_holds_for_concurrent_saves(pool: PgPool) {
    seed_card_topics(&pool, 199).await;
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    // Hold both saves before they insert, so that both count 199 topics
    // unless they serialize.
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE card_topics IN SHARE MODE")
        .execute(&mut *gate)
        .await
        .unwrap();
    let saves: Vec<_> = ["Race A", "Race B"]
        .into_iter()
        .map(|name| {
            let app = app.clone();
            tokio::spawn(async move {
                call(
                    app,
                    "POST",
                    "/api/study/card-topics",
                    json!({"name":name,"filter":{"topics":"cube"}}),
                )
                .await
            })
        })
        .collect();
    crate::support::wait_for_lock_waiters(&pool, "%", 2).await;
    gate.commit().await.unwrap();
    let mut statuses = Vec::new();
    for save in saves {
        let (status, body) = save.await.unwrap();
        assert!(status == 201 || status == 422, "{status}: {body}");
        statuses.push(status);
    }
    statuses.sort();
    assert_eq!(statuses, [201, 422]);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM card_topics")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 200);
}

#[sqlx::test(migrations = "./migrations")]
async fn card_topic_delete_does_not_wait_for_a_pending_save(pool: PgPool) {
    seed_card_topics(&pool, 1).await;
    let seeded: Uuid = sqlx::query_scalar("SELECT id FROM card_topics")
        .fetch_one(&pool)
        .await
        .unwrap();
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    // An uncommitted topic of the same name pauses the save at its insert.
    let mut gate = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO card_topics(id,name,filter) VALUES(gen_random_uuid(),'Pending','{}')")
        .execute(&mut *gate)
        .await
        .unwrap();
    let save = tokio::spawn({
        let app = app.clone();
        async move {
            call(
                app,
                "POST",
                "/api/study/card-topics",
                json!({"name":"pending","filter":{"topics":"cube"}}),
            )
            .await
        }
    });
    crate::support::wait_for_lock_waiters(&pool, "%card_topics%", 1).await;
    let (status, body) = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        call(
            app.clone(),
            "DELETE",
            &format!("/api/study/card-topics/{seeded}"),
            Value::Null,
        ),
    )
    .await
    .expect("deleting a topic must not wait for a pending save");
    assert_eq!(status, 204, "{body}");
    gate.rollback().await.unwrap();
    let (status, body) = save.await.unwrap();
    assert_eq!(status, 201, "{body}");
}
