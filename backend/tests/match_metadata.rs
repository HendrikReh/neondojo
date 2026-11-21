use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{
    api::{self, AppState},
    engine::Engine,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SOURCE: &str = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][game:0][ws:0][bs:0]DT[2025-04-03]EV[Source Open];W[31hefe];B[42qusu])\n(;GM[6]FF[4]MI[length:7][game:1][ws:6][bs:3]RE[W+1];W[31hefe])";

async fn request(pool: &PgPool, request: Request<Body>) -> (u16, Value) {
    let response = api::router(AppState::new(pool.clone(), Engine::new("unused".into())))
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn call(pool: &PgPool, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
    request(
        pool,
        Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
            .unwrap(),
    )
    .await
}

async fn import(pool: &PgPool, source: &str) -> Uuid {
    let body = format!(
        "--bg\r\nContent-Disposition: form-data; name=\"file\"; filename=\"metadata.sgf\"\r\nContent-Type: application/octet-stream\r\n\r\n{source}\r\n--bg--\r\n"
    );
    let (status, imported) = request(
        pool,
        Request::builder()
            .method("POST")
            .uri("/api/imports")
            .header("content-type", "multipart/form-data; boundary=bg")
            .body(Body::from(body))
            .unwrap(),
    )
    .await;
    assert_eq!(status, 201, "{imported}");
    Uuid::parse_str(imported["match"]["id"].as_str().unwrap()).unwrap()
}

async fn metadata(pool: &PgPool, id: Uuid) -> Value {
    let (status, value) = call(pool, "GET", &format!("/api/matches/{id}/metadata"), None).await;
    assert_eq!(status, 200, "{value}");
    value
}

async fn stored_match(pool: &PgPool, id: Uuid) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(m) FROM matches m WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn assert_library_projection(pool: &PgPool, expected: &Value) {
    let (status, page) = call(pool, "GET", "/api/library/matches", None).await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["items"][0], *expected);
}

#[sqlx::test(migrations = "./migrations")]
async fn metadata_get_returns_stored_context_final_scores_and_persistent_numeric_id(pool: PgPool) {
    let id = import(&pool, SOURCE).await;
    let initial = metadata(&pool, id).await;
    assert_eq!(initial["white"], "Alice");
    assert_eq!(initial["black"], "Bob");
    assert_eq!(initial["match_length"], 7);
    assert_eq!(initial["white_score"], 7);
    assert_eq!(initial["black_score"], 3);
    assert_eq!(initial["played_on"], "2025-04-03");
    assert_eq!(initial["tournament"], "Source Open");
    assert_eq!(initial["game_count"], 2);
    let match_no = initial["match_no"].as_i64().expect("numeric library ID");
    assert!(match_no > 0);
    let (status, saved) = call(
        &pool,
        "PATCH",
        &format!("/api/matches/{id}/metadata"),
        Some(json!({"version":0,"display_name":"Final","played_on":"2026-11-08","tournament":"Edited Open","round":"Final","place":"Berlin","result_state":"completed","white_score":7,"black_score":5})),
    )
    .await;
    assert_eq!(status, 200, "{saved}");
    let before = stored_match(&pool, id).await;
    let value = metadata(&pool, id).await;
    assert_eq!(value, saved);
    assert_eq!(value["id"], id.to_string());
    assert_eq!(value["match_no"], match_no);
    assert_eq!(value["white_score"], 7);
    assert_eq!(value["black_score"], 5);
    assert_eq!(value["played_on"], "2026-11-08");
    assert_eq!(value["tournament"], "Edited Open");
    assert_eq!(value["version"], 1);
    for excluded in ["original", "sgf", "positions", "source"] {
        assert!(value.get(excluded).is_none(), "source payload {excluded}");
    }
    assert!(!value.to_string().contains("(;GM[6]"));
    assert_eq!(stored_match(&pool, id).await, before, "GET must not write");
    assert_library_projection(&pool, &value).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn metadata_get_preserves_unknown_values_and_money_game_without_backfill(pool: PgPool) {
    let id = import(&pool, &SOURCE.replace("length:7", "length:0")).await;
    // A legacy row may still need startup backfill. Reading must neither parse
    // its source date/event nor mark that backfill as completed.
    sqlx::query("UPDATE matches SET played_on=NULL,tournament=NULL,white_score=NULL,black_score=NULL,result_state='unknown',source_metadata_loaded=false WHERE id=$1")
        .bind(id).execute(&pool).await.unwrap();
    let before = stored_match(&pool, id).await;
    let value = metadata(&pool, id).await;
    assert_eq!(value["match_length"], 0);
    assert_eq!(value["result_state"], "unknown");
    for key in [
        "played_on",
        "tournament",
        "white_score",
        "black_score",
        "white_pr",
        "black_pr",
        "pr_method",
        "pr_profile",
    ] {
        assert!(value[key].is_null(), "unknown {key}: {value}");
    }
    assert_eq!(
        stored_match(&pool, id).await,
        before,
        "GET must not backfill"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn metadata_get_missing_match_returns_json_404(pool: PgPool) {
    let (status, value) = call(
        &pool,
        "GET",
        &format!("/api/matches/{}/metadata", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(status, 404, "{value}");
    assert_eq!(value["error"], "Not found");
}

fn native_result() -> Value {
    use neondojo::model::PlayerAnalysisSummary;
    let players = [
        PlayerAnalysisSummary {
            player: 0,
            ..Default::default()
        },
        PlayerAnalysisSummary {
            player: 1,
            ..Default::default()
        },
    ];
    let value = json!({
        "engine":"GNUbg", "plies":2, "total_games":2, "completed_games":2,
        "total_moves":3, "analyzed_moves":3,
        "players":players,
        "performance_rating":{
            "method":"gnubg_emg_500_v1", "profile":"GNUbg; 2-ply cubeful deterministic", "games":2,
            "players":[
                {"player":0,"value":2.5,"normalized_equity_loss":0.005,"unforced_checker_moves":1,"close_cube_decisions":0},
                {"player":1,"value":4.0,"normalized_equity_loss":0.008,"unforced_checker_moves":1,"close_cube_decisions":0}
            ]
        }
    });
    assert!(neondojo::match_analysis::match_performance_rating(&value).is_some());
    value
}

async fn analysis(pool: &PgPool, id: Uuid, game_no: Option<i32>, result: Value, order: i32) {
    let games = if game_no.is_some() { 1 } else { 2 };
    let job = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,completed_games,game_no,result,created_at) VALUES($1,$2,'match',2,'completed',$6,$6,$3,$4,now()+$5*interval '1 second')")
        .bind(job).bind(id).bind(game_no).bind(result).bind(order).bind(games).execute(pool).await.unwrap();
    // The analysis catalog offers a job once its games have checkpoints.
    for game in game_no.map_or(1..=2, |game| game..=game) {
        sqlx::query(
            "INSERT INTO match_analysis_games(job_id,game_no,summary)
             VALUES($1,$2,'{\"completed_games\":1,\"total_games\":1}')",
        )
        .bind(job)
        .bind(game)
        .execute(pool)
        .await
        .unwrap();
    }
}

fn assert_unknown_pr(value: &Value) {
    for key in ["white_pr", "black_pr", "pr_method", "pr_profile"] {
        assert!(
            value[key].is_null(),
            "unavailable whole-match {key}: {value}"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn metadata_get_uses_only_authoritative_whole_match_native_pr(pool: PgPool) {
    let id = import(&pool, SOURCE).await;
    let mut game_result = native_result();
    game_result["total_games"] = json!(1);
    game_result["completed_games"] = json!(1);
    game_result["performance_rating"]["games"] = json!(1);
    assert!(neondojo::match_analysis::match_performance_rating(&game_result).is_some());
    analysis(&pool, id, Some(1), game_result, 2).await;
    let scoped = metadata(&pool, id).await;
    assert_eq!(scoped["analysis_status"], "partial");
    assert_unknown_pr(&scoped);
    assert_library_projection(&pool, &scoped).await;

    analysis(&pool, id, None, native_result(), 1).await;
    let whole = metadata(&pool, id).await;
    assert_eq!(whole["white_pr"], 2.5);
    assert_eq!(whole["black_pr"], 4.0);
    assert_eq!(whole["pr_method"], "gnubg_emg_500_v1");
    assert_eq!(whole["pr_profile"], "GNUbg; 2-ply cubeful deterministic");
    assert_eq!(whole["analysis_status"], "completed");
    assert_library_projection(&pool, &whole).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn metadata_get_does_not_revive_older_pr_when_latest_native_result_is_partial(pool: PgPool) {
    let id = import(&pool, SOURCE).await;
    analysis(&pool, id, None, native_result(), 0).await;
    let mut incomplete_games = native_result();
    incomplete_games["completed_games"] = json!(1);
    let mut incomplete_moves = native_result();
    incomplete_moves["analyzed_moves"] = json!(2);
    let mut unavailable = native_result();
    unavailable["players"][1]["unavailable"] = json!(1);
    let mut mismatched_profile = native_result();
    mismatched_profile["performance_rating"]["profile"] =
        json!("GNUbg; 0-ply cubeful deterministic");
    let mut no_native_statistics = native_result();
    no_native_statistics["performance_rating"] = Value::Null;
    for (order, result) in [
        incomplete_games,
        incomplete_moves,
        unavailable,
        mismatched_profile,
        no_native_statistics,
    ]
    .into_iter()
    .enumerate()
    {
        analysis(&pool, id, None, result, order as i32 + 1).await;
        let value = metadata(&pool, id).await;
        assert_unknown_pr(&value);
        assert_library_projection(&pool, &value).await;
    }
}

#[test]
fn openapi_documents_metadata_get_and_existing_update() {
    use utoipa::OpenApi;
    let doc = serde_json::to_value(api::ApiDoc::openapi()).unwrap();
    let path = &doc["paths"]["/api/matches/{id}/metadata"];
    assert_eq!(
        path["get"]["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/DatabaseMatch"
    );
    assert!(path["get"]["responses"].get("404").is_some());
    assert!(path.get("put").is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn learning_history_never_exposes_corrected_native_pr_as_independent_competition(
    pool: PgPool,
) {
    let id = import(&pool, SOURCE).await;
    analysis(&pool, id, None, native_result(), 1).await;
    for mode in [Some("coach"), Some("competition"), None] {
        sqlx::query("UPDATE matches SET play_mode=$2,play_assisted=true WHERE id=$1")
            .bind(id)
            .bind(mode)
            .execute(&pool)
            .await
            .unwrap();
        let value = metadata(&pool, id).await;
        assert!(
            value["white_pr"].is_null() && value["black_pr"].is_null(),
            "{value}"
        );
    }
    sqlx::query("UPDATE matches SET play_mode='competition',play_assisted=false WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let value = metadata(&pool, id).await;
    assert_eq!(value["white_pr"], 2.5);
    assert_eq!(value["black_pr"], 4.0);
}
