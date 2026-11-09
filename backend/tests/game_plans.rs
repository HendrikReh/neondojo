use axum::{body::Body, http::Request};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

const SOURCE_REVISION: &str = "139427d826dd67798226e7d23c003c90315306ba";

fn app() -> axum::Router {
    // Neither dependency exists: classification must remain pure and local.
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    api::router(AppState::new(pool, Engine::new("/missing/gnubg".into())))
}

async fn call(method: &str, uri: &str, body: &str) -> (u16, Value) {
    let response = app()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn classifies_both_absolute_colors_without_database_or_engine() {
    let mut board = [0; 26];
    board[6] = 15;
    board[19] = -15;
    let (status, body) = call(
        "POST",
        "/api/game-plans",
        &json!({"board":board}).to_string(),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body,
        json!({
            "classifier":"opensage_rules_v1",
            "source_revision":SOURCE_REVISION,
            "game_over":false,
            "white":"pure_race",
            "black":"pure_race"
        })
    );
}

#[tokio::test]
async fn preserves_absolute_color_identity_for_different_plans() {
    let board = [
        0, -1, 1, 0, 1, 0, 5, -1, 1, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -4, 0, 0, 0, -1, 2, 0,
    ];
    let (status, body) = call(
        "POST",
        "/api/game-plans",
        &json!({"board":board}).to_string(),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["white"], "attacking");
    assert_eq!(body["black"], "priming");
    assert_eq!(body["game_over"], false);
}

#[tokio::test]
async fn rejects_invalid_board_lengths_signs_and_checker_counts() {
    let mut invalid = vec![json!([]), json!(vec![0; 25]), json!(vec![0; 27])];
    for (point, count) in [
        (0, 1),
        (25, -1),
        (1, 16),
        (1, -16),
        (1, i32::MIN),
        (1, i32::MAX),
    ] {
        let mut board = [0; 26];
        board[point] = count;
        invalid.push(json!(board));
    }
    for sign in [-1, 1] {
        let mut board = [0; 26];
        board[6] = 8 * sign;
        board[8] = 8 * sign;
        invalid.push(json!(board));
    }
    for (bar, sign) in [(0, -1), (25, 1)] {
        let mut board = [0; 26];
        board[bar] = sign;
        board[6] = 15 * sign;
        invalid.push(json!(board));
    }
    for board in invalid {
        let (status, body) = call(
            "POST",
            "/api/game-plans",
            &json!({"board":board}).to_string(),
        )
        .await;
        assert_eq!(status, 422, "{board}: {body}");
        assert!(body["error"].is_string(), "{body}");
    }
}

#[tokio::test]
async fn rejects_malformed_unknown_and_noninteger_json_with_422() {
    let empty = [0; 26];
    let mut noninteger = json!(empty);
    noninteger[6] = json!(1.5);
    let mut extreme = json!(empty);
    extreme[6] = json!(9223372036854775807_i64);
    for body in [
        "{".to_owned(),
        "null".to_owned(),
        "{}".to_owned(),
        json!({"board":empty,"player":"white"}).to_string(),
        json!({"board":null}).to_string(),
        json!({"board":noninteger}).to_string(),
        json!({"board":extreme}).to_string(),
    ] {
        let (status, response) = call("POST", "/api/game-plans", &body).await;
        assert_eq!(status, 422, "{body}: {response}");
        assert!(response["error"].is_string(), "{response}");
    }
}

#[tokio::test]
async fn terminal_positions_keep_the_reference_class_and_report_game_over() {
    for point_and_count in [Some((6, 15)), Some((19, -15)), None] {
        let mut board = [0; 26];
        if let Some((point, count)) = point_and_count {
            board[point] = count;
        }
        let (status, body) = call(
            "POST",
            "/api/game-plans",
            &json!({"board":board}).to_string(),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["game_over"], true);
        assert_eq!(body["white"], "pure_race");
        assert_eq!(body["black"], "pure_race");
    }
    let mut board = [0; 26];
    board[0] = -1;
    let (status, body) = call(
        "POST",
        "/api/game-plans",
        &json!({"board":board}).to_string(),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["game_over"], true);
    assert_eq!(body["white"], "racing");
    assert_eq!(body["black"], "attacking");
}

#[tokio::test]
async fn publishes_game_plan_openapi_contract() {
    let (status, schema) = call("GET", "/api/openapi.json", "").await;
    assert_eq!(status, 200);
    let operation = &schema["paths"]["/api/game-plans"]["post"];
    assert!(operation.is_object(), "{operation}");
    assert!(operation["responses"]["200"].is_object());
    assert!(operation["responses"]["422"].is_object());
    assert_eq!(
        operation["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/GamePlanRequest"
    );
    assert_eq!(
        operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/GamePlans"
    );
    let schemas = &schema["components"]["schemas"];
    assert_eq!(
        schemas["GamePlan"]["enum"],
        json!(["pure_race", "racing", "attacking", "priming", "anchoring"])
    );
    assert_eq!(schemas["GamePlanRequest"]["additionalProperties"], false);
    assert_eq!(
        schemas["GamePlanRequest"]["properties"]["board"]["minItems"],
        26
    );
    assert_eq!(
        schemas["GamePlanRequest"]["properties"]["board"]["maxItems"],
        26
    );
    for field in [
        "classifier",
        "source_revision",
        "game_over",
        "white",
        "black",
    ] {
        assert!(
            schemas["GamePlans"]["properties"][field].is_object(),
            "{field}"
        );
    }
}
