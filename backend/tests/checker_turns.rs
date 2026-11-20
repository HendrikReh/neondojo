use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use neondojo::{
    api::{self, AppState},
    engine::Engine,
};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

async fn turns(board: [i32; 26], player: i32, dice: [i32; 2]) -> (StatusCode, Value) {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    let response = api::router(AppState::new(pool, Engine::new("/missing/gnubg".into())))
        .oneshot(
            Request::post("/api/play/checker-turns")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "board": board, "player": player, "dice": dice }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(
        !body.is_empty(),
        "Expected checker-turn response, got {status}"
    );
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn returns_absolute_black_bar_entry_with_the_only_legal_die() {
    let mut board = [0; 26];
    board[0] = -1;
    board[5] = 2;
    board[6] = 2;
    let (status, paths) = turns(board, 1, [5, 6]).await;
    assert_eq!(status, StatusCode::OK, "{paths}");
    assert_eq!(paths, json!([]));

    board[6] = 1;
    board[11] = 2;
    let (status, paths) = turns(board, 1, [5, 6]).await;
    assert_eq!(status, StatusCode::OK, "{paths}");
    assert_eq!(
        paths,
        json!([{ "steps": [{ "from": 0, "to": 6 }], "dice": [6] }])
    );
}

#[tokio::test]
async fn preserves_alternate_paths_to_the_same_end_board() {
    let mut board = [0; 26];
    board[24] = 1;
    board[1] = -1;
    let (status, paths) = turns(board, 0, [3, 2]).await;
    assert_eq!(status, StatusCode::OK, "{paths}");
    assert_eq!(
        paths,
        json!([
            { "steps": [{ "from": 24, "to": 21 }, { "from": 21, "to": 19 }], "dice": [3, 2] },
            { "steps": [{ "from": 24, "to": 22 }, { "from": 22, "to": 19 }], "dice": [2, 3] }
        ])
    );
}

#[tokio::test]
async fn invalid_counts_are_errors_and_never_a_pass() {
    let mut board = [0; 26];
    board[24] = i32::MAX;
    board[1] = i32::MIN;
    assert_eq!(
        turns(board, 0, [3, 2]).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn returns_individual_double_steps_and_preserves_immediate_bearoff() {
    let mut board = [0; 26];
    board[13] = 1;
    board[24] = -1;
    let (status, paths) = turns(board, 0, [3, 3]).await;
    assert_eq!(status, StatusCode::OK, "{paths}");
    assert_eq!(
        paths,
        json!([{
            "steps": [{"from":13,"to":10},{"from":10,"to":7},{"from":7,"to":4},{"from":4,"to":1}],
            "dice": [3, 3, 3, 3],
        }])
    );

    board[13] = 0;
    board[2] = 1;
    let (status, paths) = turns(board, 0, [1, 3]).await;
    assert_eq!(status, StatusCode::OK, "{paths}");
    assert_eq!(
        paths,
        json!([
            {"steps": [{"from":2,"to":1},{"from":1,"to":-1}], "dice": [1, 3]},
            {"steps": [{"from":2,"to":-1}], "dice": [3]},
        ])
    );
}
