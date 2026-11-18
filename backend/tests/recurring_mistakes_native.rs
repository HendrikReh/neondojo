//! End-to-end oracle: a real GNUbg result remains usable through grouping and practice.
use axum::{body::Body, http::Request};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use bgtutor::{api, engine::Engine, import, jobs, store};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

async fn call(app: &axum::Router, method: &str, uri: &str, body: Value) -> (u16, Value) {
    let response = app
        .clone()
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
async fn real_native_analysis_flows_into_answer_hidden_checker_and_cube_practice(pool: PgPool) {
    // In the cube-only game, GNUbg recommends Take from the responder's position.
    let source = "(;GM[6]PW[Alice]PB[Bob]MI[length:7];W[31xufe];B[42qusu])\n(;GM[6]PW[Alice]PB[Bob]MI[length:7]AE[a:y]AW[i]AB[s]PL[W]RE[W+1];W[double];B[drop])";
    let imported = store::import_match(
        &pool,
        "recurring-native.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &import::parse_sgf(source).unwrap(),
    )
    .await
    .unwrap();
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let state = api::AppState::new(pool.clone(), engine);
    let oracle = state.engine.analyze_game(source, 1, 1).await.unwrap();
    let opening_loss = oracle
        .decisions
        .iter()
        .find(|d| d.kind == "checker" && d.player == 0)
        .unwrap()
        .equity_loss;
    assert!(
        opening_loss > 0.2,
        "the opening split is a substantial blunder"
    );
    let app = api::router(state.clone());
    let job = store::create_match_job(&pool, imported.r#match.id, 1, 2)
        .await
        .unwrap();
    assert!(jobs::run_one(&state).await.unwrap());
    let completed = store::get_job(&pool, job.id).await.unwrap().unwrap();
    assert_eq!(completed.status, "completed", "{:?}", completed.error);

    let mut checker_seen = false;
    let mut response_seen = false;
    for (name, color) in [("Alice", 0), ("Bob", 1)] {
        let (status, groups) = call(
            &app,
            "GET",
            &format!("/api/study/mistakes?player={name}&min_loss=0.000001"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{groups}");
        for group in groups["groups"].as_array().unwrap() {
            let (status, questions) = call(
                &app,
                "POST",
                "/api/study/mistakes/practice",
                json!({"player":name,"min_loss":0.000001,"game_plan":group["game_plan"],"mistake_kind":group["mistake_kind"]}),
            )
            .await;
            assert_eq!(status, 200, "{questions}");
            assert!(!questions["items"].as_array().unwrap().is_empty());
            for question in questions["items"].as_array().unwrap() {
                assert_eq!(question["position"]["player"], color);
                assert_eq!(question["position"]["notation"], "");
                for field in ["played", "best", "equity_loss", "answer", "solution"] {
                    assert!(question.get(field).is_none(), "{question}");
                }
                let (status, answer) = call(
                    &app,
                    "POST",
                    "/api/study/mistakes/reveal",
                    question["reference"].clone(),
                )
                .await;
                assert_eq!(status, 200, "{answer}");
                assert!(answer["equity_loss"].as_f64().unwrap() > 0.0);
                assert!(answer["engine"].as_str().unwrap().contains("GNU"));
                assert_eq!(answer["plies"], 1);
                if question["kind"] == "checker" && name == "Alice" {
                    checker_seen = true;
                    assert_eq!(question["position"]["dice"], json!([3, 1]));
                    assert_eq!(answer["best"], "8/5 6/5");
                    assert_eq!(answer["played"], "24/21 6/5");
                    assert!(
                        (answer["equity_loss"].as_f64().unwrap() - opening_loss).abs() < 0.000002,
                        "{answer}"
                    );
                    let (status, checked) = call(&app, "POST", "/api/study/mistakes/attempt", json!({
                        "reference":question["reference"], "answer":{"kind":"checker","steps":[{"from":8,"to":7},{"from":8,"to":5}]}
                    })).await;
                    assert_eq!(status, 200, "{checked}");
                    assert_eq!(checked["evaluation"]["provenance"]["kind"], "recomputed");
                    assert_eq!(checked["evaluation"]["best"], "8/5 6/5");
                    assert!(checked["evaluation"]["equity_loss"].as_f64().unwrap() > 0.25);
                }
                if question["kind"] == "cube_response" {
                    response_seen = true;
                    assert_eq!(name, "Bob");
                    assert_eq!(question["position"]["board"][9], 1);
                    assert_eq!(question["position"]["board"][19], -1);
                    assert_eq!(question["position"]["dice"], json!([0, 0]));
                    let encoded = question["position"]["gnubg_match_id"].as_str().unwrap();
                    let bytes = STANDARD_NO_PAD.decode(encoded).unwrap();
                    assert_eq!((bytes[0] >> 6) & 1, 1, "response belongs to Black");
                    assert_eq!(bytes[1] & 0x10, 0x10, "the offered cube is pending");
                    assert_eq!(bytes[0] & 0x0f, 0, "cube exponent stays at pre-offer value");
                    assert_eq!(answer["played"], "drop");
                    assert_eq!(answer["best"], "take");
                    let (status, checked) = call(&app, "POST", "/api/study/mistakes/attempt", json!({
                        "reference":question["reference"], "answer":{"kind":"cube_response","action":"take"}
                    })).await;
                    assert_eq!(status, 200, "{checked}");
                    assert_eq!(checked["evaluation"]["verdict"], "best");
                    assert_eq!(checked["evaluation"]["equity_loss"], 0.0);
                }
            }
        }
    }
    assert!(
        checker_seen,
        "the independently known opening blunder must be present"
    );
    assert!(
        response_seen,
        "a cube-only game must preserve the responder's question"
    );
    let cards: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_cards")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cards, 0, "practice must not create or schedule flashcards");
}
