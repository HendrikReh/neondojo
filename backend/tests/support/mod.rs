//! Shared helpers for the integration test targets. Each target that uses them
//! declares `mod support;`; not every target needs every helper.
#![allow(dead_code)]
use axum::{Router, body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{
    api::{self, AppState},
    engine::Engine,
    import, store,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

/// The API router with an engine path. Tests pass a path that cannot be
/// launched when the behavior under test must not start GNUbg.
pub fn app(pool: &PgPool, engine: &str) -> Router {
    api::router(AppState::new(pool.clone(), Engine::new(engine.into())))
}

/// Sends `body` as JSON, including a literal `null`. A response body that is
/// not JSON becomes `Value::Null`.
pub async fn call(app: Router, method: &str, uri: &str, body: Value) -> (u16, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    respond(app, request).await
}

/// Like [`call`], but `Value::Null` sends an empty body.
pub async fn send(app: &Router, method: &str, uri: &str, body: Value) -> (u16, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(if body.is_null() {
            Body::empty()
        } else {
            Body::from(body.to_string())
        })
        .unwrap();
    respond(app.clone(), request).await
}

async fn respond(app: Router, request: Request<Body>) -> (u16, Value) {
    let response = app.oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Two seven-point-match games between Alice and Bob with one opening move
/// each, imported as SGF and then marked as an XG import.
pub const FOREIGN_SOURCE: &str = "(;GM[6]PW[Alice]PB[Bob]MI[length:7];W[31hefe])(;GM[6]PW[Alice]PB[Bob]MI[length:7][ws:1];W[31hefe])";

/// Imports [`FOREIGN_SOURCE`] with one eXtreme Gammon checker analysis on the
/// first decision of each game. Returns the match and the analysis row IDs.
pub async fn foreign_analysis(pool: &PgPool) -> (Uuid, Vec<Uuid>) {
    let id = store::import_match(
        pool,
        "foreign.sgf",
        "sgf",
        FOREIGN_SOURCE.as_bytes(),
        FOREIGN_SOURCE,
        &import::parse_sgf(FOREIGN_SOURCE).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    sqlx::query("UPDATE matches SET format='xg',imported_analysis_loaded=true WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    let mut ids = vec![];
    for game in 1..=2 {
        let (position,position_id,gnubg_match_id):(Uuid,String,String)=sqlx::query_as("SELECT id,position_id,gnubg_match_id FROM positions WHERE match_uuid=$1 AND game_no=$2").bind(id).bind(game).fetch_one(pool).await.unwrap();
        let record = json!({"game_no":game,"source_move_no":1,"checker_ply":1,"position_id":position_id,"gnubg_match_id":gnubg_match_id,"dice":[3,1],"kind":"checker","player":0,"played":"8/5 6/5","engine":"eXtreme Gammon","engine_version":"2.19","method":"evaluation","depth":"XGRoller++","depth_plies":null,"candidates":[{"rank":1,"notation":"6/3 6/5","equity":0.2,"equity_kind":"cubeful","probabilities":{},"depth":"XGRoller++","raw":{}},{"rank":2,"notation":"8/5 6/5","equity":0.0,"equity_kind":"cubeful","probabilities":{},"raw":{}}],"cube":null,"raw":{}});
        let row = Uuid::new_v4();
        sqlx::query("INSERT INTO imported_analyses(id,match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data) VALUES($1,$2,$3,$4,$5,1,'checker',$6)").bind(row).bind(id).bind(position).bind(game-1).bind(game).bind(record).execute(pool).await.unwrap();
        ids.push(row);
    }
    (id, ids)
}
