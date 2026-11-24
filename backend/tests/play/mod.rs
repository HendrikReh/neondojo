//! Playing against a bot, by topic. The shared fixtures are in this file.

mod feedback;
mod library;
mod modes;
mod money;
mod opening;
mod resign;
mod review;
mod session;
mod setup;

use crate::profile_support;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{
    api::{self, AppState},
    engine::Engine,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

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
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned())),
    )
}

async fn ready(app: axum::Router, mut session: Value) -> Value {
    while session["phase"] == "opening_roll" || session["phase"] == "opening_result" {
        let action = if session["phase"] == "opening_roll" {
            "opening_roll"
        } else {
            "start_opening_move"
        };
        let uri = format!(
            "/api/play/sessions/{}/actions",
            session["id"].as_str().unwrap()
        );
        let (status, next) = call(
            app.clone(),
            "POST",
            &uri,
            json!({"version":session["version"],"action":action}),
        )
        .await;
        assert_eq!(status, 200, "{next}");
        session = next;
    }
    session
}

/// Point a session's pinned engines at another binary. Sessions pin their
/// rules, bot and analysis engines in the saved state; the column only
/// serves sessions saved before those snapshots existed.
/// Returns the path that was pinned before, to restore it.
async fn pin_engine_path(pool: &PgPool, id: uuid::Uuid, path: &str) -> String {
    let previous: String = sqlx::query_scalar(
        "SELECT coalesce(state#>>'{engine_snapshots,rules,gnubg_path}',gnubg_path)
         FROM play_sessions WHERE id=$1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE play_sessions SET gnubg_path=$2,
             state=CASE WHEN state ? 'engine_snapshots' THEN
                 jsonb_set(jsonb_set(jsonb_set(state,
                     '{engine_snapshots,rules,gnubg_path}',to_jsonb($2::text)),
                     '{engine_snapshots,bot,gnubg_path}',to_jsonb($2::text)),
                     '{engine_snapshots,analysis,gnubg_path}',to_jsonb($2::text))
             ELSE state END
         WHERE id=$1",
    )
    .bind(id)
    .bind(path)
    .execute(pool)
    .await
    .unwrap();
    previous
}

fn engine() -> Engine {
    Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()))
}

async fn assert_library_checkpoint(pool: &PgPool, app: &axum::Router, session: &Value) {
    let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
    let match_id: uuid::Uuid =
        sqlx::query_scalar("SELECT library_match_id FROM play_sessions WHERE id=$1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
    let (status, row) = call(
        app.clone(),
        "GET",
        &format!("/api/matches/{match_id}/metadata"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{row}");
    assert_eq!(row["play_session_id"], session["id"]);
    assert_eq!(row["play_phase"], session["phase"]);
    assert_eq!(row["white_score"], session["scores"][0]);
    assert_eq!(row["black_score"], session["scores"][1]);
    assert_eq!(
        row["result_state"],
        if session["phase"] == "match_over"
            || (session["match_length"] == 0 && session["phase"] == "game_over")
        {
            "completed"
        } else {
            "ongoing"
        }
    );
    assert_eq!(
        row[if session["human_player"] == 0 {
            "white"
        } else {
            "black"
        }],
        session["human_name"]
    );
    assert_eq!(
        row[if session["human_player"] == 0 {
            "black"
        } else {
            "white"
        }],
        "GNUbg"
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM positions WHERE match_uuid=$1")
        .bind(match_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(row["position_count"].as_i64().unwrap(), count);
    assert!(row["played_on"].as_str().is_some());
    neondojo::move_catalog::ensure_catalog(pool, match_id)
        .await
        .unwrap();
}

#[allow(clippy::too_many_arguments)] // Explicit native fixture parameters keep test scenarios readable.
async fn seed(
    pool: &PgPool,
    app: &axum::Router,
    score: i32,
    black_points: &str,
    dice: &str,
    id: &str,
    phase: &str,
    legal: Value,
) -> Value {
    let (status, mut s) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    assert_eq!(status, 201, "{s}");
    s["phase"] = json!(phase);
    s["legal_moves"] = legal;
    s["can_double"] = json!(phase == "roll");
    s["scores"] = json!([score, 0]);
    let sgf = format!(
        "(;FF[4]GM[6]MI[length:5][game:0][ws:{score}][bs:0]PW[Human]PB[GNUbg]RU[Crawford];PL[W]DI[{dice}];PL[W]AE[a:y]AW[a]AB{black_points})"
    );
    sqlx::query("UPDATE play_sessions SET state=$1,sgf=$2,native_id=$3 WHERE id=$4")
        .bind(&s)
        .bind(sgf)
        .bind(id)
        .bind(uuid::Uuid::parse_str(s["id"].as_str().unwrap()).unwrap())
        .execute(pool)
        .await
        .unwrap();
    s
}

async fn replace_native(pool: &PgPool, s: &Value, sgf: &str, id: &str) {
    sqlx::query("UPDATE play_sessions SET state=$1,sgf=$2,native_id=$3 WHERE id=$4")
        .bind(s)
        .bind(sgf)
        .bind(id)
        .bind(uuid::Uuid::parse_str(s["id"].as_str().unwrap()).unwrap())
        .execute(pool)
        .await
        .unwrap();
}
