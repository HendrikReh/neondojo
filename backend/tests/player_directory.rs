use axum::{body::Body, http::Request};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
    import, store,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

async fn call(app: &axum::Router, method: &str, uri: &str, data: Value, status: u16) -> Value {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(data.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let actual = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(actual, status, "{}", String::from_utf8_lossy(&body));
    serde_json::from_slice(&body).unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn source_aliases_stay_separate_until_assigned_and_apply_to_future_imports(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), Engine::new("gnubg".into())));
    for (file, site, player) in [
        ("one.sgf", "BackgammonGalaxy", "example"),
        ("two.sgf", "OpenGammon", "example"),
    ] {
        let sgf = format!("(;GM[6]PW[{player}]PB[Opponent]PC[{site}]MI[length:7];W[31hefe])");
        let parsed = import::parse_sgf(&sgf).unwrap();
        store::import_match(&pool, file, "sgf", sgf.as_bytes(), &sgf, &parsed)
            .await
            .unwrap();
    }
    let profiles = call(&app, "GET", "/api/players?q=example", Value::Null, 200).await;
    let items = profiles["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let id = items[0]["id"].as_str().unwrap();
    let version = items[0]["version"].as_i64().unwrap();
    let uri = format!("/api/players/{id}");
    let updated = call(
        &app,
        "PUT",
        &uri,
        json!({"first_name":"Anna","last_name":"Beispiel","version":version}),
        200,
    )
    .await;
    assert_eq!(updated["display_name"], "Anna Beispiel");
    call(
        &app,
        "PUT",
        &uri,
        json!({"first_name":"Stale","last_name":"","version":version}),
        409,
    )
    .await;
    let other = items[1]["id"].as_str().unwrap();
    let merged = call(&app, "POST", &format!("{uri}/merge"), json!({"source_id":other,"version":updated["version"],"source_version":items[1]["version"]}), 200).await;
    assert_eq!(merged["match_count"], 2);
    let sgf = "(;GM[6]PW[Someone]PB[example]PC[OpenGammon]MI[length:5];W[31hefe])";
    store::import_match(
        &pool,
        "three.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let profile = call(&app, "GET", &uri, Value::Null, 200).await;
    assert_eq!(profile["match_count"], 3);
    assert_eq!(profile["game_count"], 3);
    assert_eq!(profile["ratings"], json!([]));
    let matches = call(
        &app,
        "GET",
        &format!("/api/library/matches?player_id={id}&limit=2"),
        Value::Null,
        200,
    )
    .await;
    assert_eq!(matches["items"].as_array().unwrap().len(), 2);
    assert!(matches["next_cursor"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn ratings_weight_decisions_and_keep_analysis_profiles_separate(pool: PgPool) {
    use bgtutor::model::{
        MatchAnalysisSummary, PerformanceRatingSummary, PlayerAnalysisSummary,
        PlayerPerformanceRating,
    };
    let app = api::router(AppState::new(pool.clone(), Engine::new("gnubg".into())));
    for (length, plies, decisions, loss) in [(7, 0, 10, 0.02), (5, 0, 30, 0.14), (3, 2, 20, 0.12)] {
        let sgf = format!("(;GM[6]PW[Anna]PB[Bob]MI[length:{length}];W[31hefe])");
        let parsed = import::parse_sgf(&sgf).unwrap();
        let match_id =
            store::import_match(&pool, "match.sgf", "sgf", sgf.as_bytes(), &sgf, &parsed)
                .await
                .unwrap()
                .r#match
                .id;
        let result = MatchAnalysisSummary {
            engine: "GNUbg".into(),
            plies,
            total_games: 1,
            completed_games: 1,
            total_moves: 1,
            analyzed_moves: 1,
            players: [
                PlayerAnalysisSummary::default(),
                PlayerAnalysisSummary {
                    player: 1,
                    ..Default::default()
                },
            ],
            performance_rating: Some(PerformanceRatingSummary {
                method: "gnubg_emg_500_v1".into(),
                profile: format!("GNUbg; {plies}-ply cubeful deterministic"),
                games: 1,
                players: [
                    PlayerPerformanceRating {
                        value: Some(500.0 * loss / f64::from(decisions)),
                        normalized_equity_loss: loss,
                        unforced_checker_moves: decisions,
                        ..Default::default()
                    },
                    PlayerPerformanceRating {
                        player: 1,
                        ..Default::default()
                    },
                ],
            }),
        };
        sqlx::query(
            "INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,completed_games,result)
                     VALUES(gen_random_uuid(),$1,'match',$2,'completed',1,1,$3)",
        )
        .bind(match_id)
        .bind(plies)
        .bind(serde_json::to_value(result).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    }
    let page = call(&app, "GET", "/api/players?q=Anna", Value::Null, 200).await;
    let id = page["items"][0]["id"].as_str().unwrap();
    let profile = call(&app, "GET", &format!("/api/players/{id}"), Value::Null, 200).await;
    assert_eq!(
        profile["ratings"],
        json!([
            {"profile":"GNUbg; 0-ply cubeful deterministic","value":2.0,"match_count":2},
            {"profile":"GNUbg; 2-ply cubeful deterministic","value":3.0,"match_count":1}
        ])
    );
}
