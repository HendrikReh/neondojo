use axum::{body::Body, http::Request};
use bgtutor::{api, engine::Engine, import, jobs, store};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

static EXPORT_TESTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
        json!({"position_id":position.id,"kind":"analysis","plies":0}),
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
        json!({"plies":0}),
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
#[ignore = "requires PostgreSQL"]
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
        json!({"position_id":p.id,"kind":"analysis","plies":0}),
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
        json!({"position_id":p.id,"kind":"analysis","plies":0}),
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
    if let Ok(dir) = std::env::var("BGTUTOR_EXPORT_FIXTURES") {
        std::fs::write(
            std::path::Path::new(&dir).join("bgtutor-export-before.apkg"),
            before,
        )
        .unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join("bgtutor-export-after.apkg"),
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
            json!({"position_id":position.id,"kind":"analysis","plies":0}),
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
