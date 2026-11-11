#![allow(dead_code)]
use axum::{body::Body, http::Request};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

pub async fn call(app: axum::Router, method: &str, path: &str, body: Value) -> (u16, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let code = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (code, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}
pub fn config() -> Value {
    let columns = vec![
        "id",
        "white",
        "white_pr",
        "black",
        "black_pr",
        "play_mode",
        "match_length",
        "score",
        "played_at",
        "created_at",
        "tournament",
        "position_count",
        "analysis_status",
        "tags",
        "comment",
    ];
    json!({"schema_version":1,
        "general":{"player_name":"Client value ignored","locale":"de","play_sounds":false},
        "analysis":{"engine":"gnubg","plies":0,"scope":"game","search_filter":"engine_default"},
        "play":{"engine":"gnubg","plies":0,"default_mode":"coach","tutor_threshold":0.04,"training_auto_analysis":true,"human_player":0,"match_length":7},
        "appearance":{"ui_theme":"graphite","mode":"system","board_theme":"walnut","bearoff_direction":"right","board_view":"arrows"},
        "opening_book":{"provider":"off","path":"/unused/client/book","xg_path":"/unused/client/xg"},
        "library":{"database_id":"00000000-0000-0000-0000-000000000001","filters":{"q":"","player":"","date_from":"","date_to":"","date_field":"played","tag":"","comment":"","position_id":"","tournament":"","round":"","match_name":"","match_type":"","result":"","match_points_min":"","match_points_max":""},"sort":"created_at","direction":"desc","visible_columns":columns,"column_order":columns,"column_widths":null},
        "positions":{"definition":{"schema_version":1,"sort":"match"},"show_played":false,"column_widths":null},
        "learning":{"deck_id":"__all__","filters":{"q":"","player":"","tag":"","comment":"","position_id":"","kind":"","status":""},"generation":{"min_loss":0.04,"player":""}},
        "annotations":{"filters":null},"review":{"player":"","kind":"","severity":"","errors_only":true}})
}
pub async fn initialized(pool: &PgPool, native: bool) -> (axum::Router, Value) {
    let app = api::router(AppState::new(
        pool.clone(),
        Engine::new(if native {
            std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())
        } else {
            "/missing/gnubg".into()
        }),
    ));
    let (code, settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    assert_eq!(code, 200);
    let (_, book) = call(app.clone(), "GET", "/api/opening-book", Value::Null).await;
    let mut book_config = book["config"].clone();
    book_config["provider"] = json!("off");
    let (code, book) = call(app.clone(), "PUT", "/api/opening-book", book_config).await;
    assert_eq!(code, 200, "{book}");
    let (code,state)=call(app.clone(),"POST","/api/configuration/initialize",json!({"expected_settings_version":settings["version"],"expected_book_version":book["config"]["version"],"configuration":config()})).await;
    assert_eq!(code, 200, "{state}");
    (app, state)
}
pub async fn create(app: &axum::Router, state: &Value, name: &str, config: Value) -> Value {
    let (code, result) = call(
        app.clone(),
        "POST",
        "/api/profiles",
        json!({"name":name,"expected_state_revision":state["revision"],"configuration":config}),
    )
    .await;
    assert_eq!(code, 200, "{result}");
    result
}
pub fn activate(state: &Value, target: &Value, action: &str) -> Value {
    json!({"expected_state_revision":state["revision"],"expected_target_revision":target["revision"],"expected_source_revision":state["active_profile"]["revision"],"source_action":action,"current_configuration":state["current_configuration"]})
}
