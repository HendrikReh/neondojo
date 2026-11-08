use axum::{body::Body, http::Request};
use bgtutor::{api, engine::Engine, import, jobs, model::Job, store};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SOURCE: &str = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])\n(;GM[6]FF[4]MI[length:7][ws:1][bs:0];W[double];B[drop])\n(;GM[6]FF[4]MI[length:7][ws:6][bs:0]RU[CrawfordGame])";

async fn get(app: axum::Router, uri: &str) -> (u16, Value) {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn setup(pool: &PgPool) -> (api::AppState, axum::Router, Uuid) {
    let imported = store::import_match(
        pool,
        "statistics.sgf",
        "sgf",
        SOURCE.as_bytes(),
        SOURCE,
        &import::parse_sgf(SOURCE).unwrap(),
    )
    .await
    .unwrap();
    let state = api::AppState::new(
        pool.clone(),
        Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())),
    );
    (state.clone(), api::router(state), imported.r#match.id)
}

fn summary() -> Value {
    json!({"engine":"GNU Backgammon fixture", "plies":0,"total_games":3,"completed_games":1,"total_moves":2,"analyzed_moves":2,"players":[
        {"player":0,"checker_decisions":1,"cube_decisions":1,"forced_moves":0,"unavailable":0,"doubtful":1,"bad":1,"very_bad":0,"checker_equity_loss":0.04,"cube_equity_loss":0.08},
        {"player":1,"checker_decisions":1,"cube_decisions":0,"forced_moves":1,"unavailable":0,"doubtful":0,"bad":0,"very_bad":0,"checker_equity_loss":0.0,"cube_equity_loss":0.0}
    ],"performance_rating":null})
}

#[allow(clippy::too_many_arguments)]
async fn decision(
    pool: &PgPool,
    job: &Job,
    game: i32,
    number: i32,
    player: i32,
    kind: &str,
    severity: &str,
    loss: f64,
) -> i64 {
    let data = json!({"decision_no":number,"checker_no":null,"player":player,"kind":kind,"played":"played","best":"best","played_equity":0.0,"best_equity":loss,"equity_loss":loss,"severity":severity,"dice":null,"evaluation_status":if loss == 0.0 {"forced"} else {"evaluated"}});
    sqlx::query_scalar("INSERT INTO move_analyses(job_id,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id")
        .bind(job.id).bind(game).bind(number).bind(player).bind(kind).bind(loss).bind(data).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn statistics_use_checkpoint_coverage_and_preserve_authoritative_summary(pool: PgPool) {
    let (_, app, match_id) = setup(&pool).await;
    let job = store::create_match_job(&pool, match_id, 0, 3)
        .await
        .unwrap();
    let uri = format!("/api/jobs/{}/statistics", job.id);
    let (status, queued) = get(app.clone(), &uri).await;
    assert_eq!(status, 200, "{queued}");
    assert_eq!(queued["job_id"], job.id.to_string());
    assert!(queued["summary"].is_null());
    assert_eq!(queued["games"].as_array().unwrap().len(), 3);
    assert!(
        queued["games"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["analyzed"] == false && g["summary"].is_null())
    );
    assert_eq!(queued["games"][1]["white_score"], 1);
    assert_eq!(queued["games"][2]["crawford"], true);
    // An uncheckpointed row must never prove completed coverage.
    decision(&pool, &job, 2, 1, 0, "cube", "very_bad", 0.2).await;
    decision(&pool, &job, 1, 1, 0, "checker", "doubtful", 0.04).await;
    decision(&pool, &job, 1, 2, 0, "cube", "bad", 0.08).await;
    decision(&pool, &job, 1, 3, 1, "checker", "none", 0.0).await;
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,1)")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let saved = summary();
    sqlx::query("UPDATE jobs SET status='running',completed_games=1,result=$2 WHERE id=$1")
        .bind(job.id)
        .bind(&saved)
        .execute(&pool)
        .await
        .unwrap();
    for status in ["running", "failed", "completed"] {
        sqlx::query("UPDATE jobs SET status=$2 WHERE id=$1")
            .bind(job.id)
            .bind(status)
            .execute(&pool)
            .await
            .unwrap();
        let (code, data) = get(app.clone(), &uri).await;
        assert_eq!(code, 200, "{data}");
        assert_eq!(data["summary"], saved);
        let game = &data["games"][0];
        assert_eq!(game["analyzed"], true);
        assert_eq!(game["summary"]["total_games"], 1);
        assert_eq!(game["summary"]["completed_games"], 1);
        assert_eq!(game["summary"]["analyzed_moves"], 2);
        assert_eq!(game["summary"]["players"], saved["players"]);
        assert!(
            game["summary"]["performance_rating"].is_null(),
            "legacy checkpoint cannot prove PR"
        );
        assert_eq!(data["games"][1]["analyzed"], false);
        assert!(data["games"][1]["summary"].is_null());
        assert_eq!(
            data["games"][2]["analyzed"], false,
            "an empty source game still requires a checkpoint"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn statistics_scope_empty_checkpoint_missing_and_wrong_kind(pool: PgPool) {
    let (_, app, match_id) = setup(&pool).await;
    let job = store::create_match_job_scoped(&pool, match_id, 0, 1, Some(3))
        .await
        .unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,3)")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, data) = get(app.clone(), &format!("/api/jobs/{}/statistics", job.id)).await;
    assert_eq!(status, 200, "{data}");
    assert!(data["summary"].is_null());
    assert_eq!(data["games"].as_array().unwrap().len(), 1);
    assert_eq!(data["games"][0]["game_no"], 3);
    assert_eq!(data["games"][0]["analyzed"], true);
    assert_eq!(data["games"][0]["summary"]["analyzed_moves"], 0);
    assert!(data["games"][0]["summary"]["performance_rating"].is_null());
    assert_eq!(
        get(
            app.clone(),
            &format!("/api/jobs/{}/statistics", Uuid::new_v4())
        )
        .await
        .0,
        404
    );
    let position: Uuid = sqlx::query_scalar("SELECT id FROM positions LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let other = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO jobs(id,position_uuid,kind,plies,status) VALUES($1,$2,'analysis',0,'queued')",
    )
    .bind(other)
    .bind(position)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        get(app, &format!("/api/jobs/{other}/statistics")).await.0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn native_statistics_are_persisted_per_game_without_averaging_pr(pool: PgPool) {
    let (state, app, match_id) = setup(&pool).await;
    let job = store::create_match_job(&pool, match_id, 0, 3)
        .await
        .unwrap();
    assert!(jobs::run_one(&state).await.unwrap());
    let (status, data) = get(app, &format!("/api/jobs/{}/statistics", job.id)).await;
    assert_eq!(status, 200, "{data}");
    assert!(
        data["games"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["analyzed"] == true)
    );
    let persisted: Vec<Value> = sqlx::query_scalar(
        "SELECT summary FROM match_analysis_games WHERE job_id=$1 ORDER BY game_no",
    )
    .bind(job.id)
    .fetch_all(&pool)
    .await
    .unwrap();
    for (game, saved) in data["games"].as_array().unwrap().iter().zip(&persisted) {
        assert_eq!(&game["summary"], saved);
        assert_eq!(saved["performance_rating"]["games"], 1);
        assert_eq!(saved["performance_rating"]["method"], "gnubg_emg_500_v1");
        for player in saved["performance_rating"]["players"].as_array().unwrap() {
            let denominator = player["unforced_checker_moves"].as_i64().unwrap()
                + player["close_cube_decisions"].as_i64().unwrap();
            if denominator == 0 {
                assert!(player["value"].is_null());
            } else {
                assert!(
                    (player["value"].as_f64().unwrap()
                        - 500.0 * player["normalized_equity_loss"].as_f64().unwrap()
                            / denominator as f64)
                        .abs()
                        < 1e-9
                );
            }
        }
    }
    for p in 0..2 {
        let sum: f64 = persisted
            .iter()
            .map(|g| {
                g["performance_rating"]["players"][p]["normalized_equity_loss"]
                    .as_f64()
                    .unwrap()
            })
            .sum();
        assert!(
            (data["summary"]["performance_rating"]["players"][p]["normalized_equity_loss"]
                .as_f64()
                .unwrap()
                - sum)
                .abs()
                < 1e-9
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn severity_filters_intersect_and_cursor_is_bound_to_severity(pool: PgPool) {
    let (_, app, match_id) = setup(&pool).await;
    let job = store::create_match_job(&pool, match_id, 0, 3)
        .await
        .unwrap();
    let none = decision(&pool, &job, 1, 1, 0, "checker", "none", 0.0).await;
    let doubtful = decision(&pool, &job, 1, 2, 0, "checker", "doubtful", 0.04).await;
    let bad = decision(&pool, &job, 1, 3, 0, "checker", "bad", 0.08).await;
    let very_bad = decision(&pool, &job, 1, 4, 0, "checker", "very_bad", 0.16).await;
    decision(&pool, &job, 1, 5, 1, "checker", "bad", 0.08).await;
    decision(&pool, &job, 1, 6, 0, "cube", "bad", 0.08).await;
    decision(&pool, &job, 2, 1, 0, "checker", "bad", 0.08).await;
    let second_bad = decision(&pool, &job, 1, 7, 0, "checker", "bad", 0.09).await;
    let base = format!("/api/jobs/{}/moves", job.id);
    for (severity, id) in [
        ("none", none),
        ("doubtful", doubtful),
        ("bad", bad),
        ("very_bad", very_bad),
    ] {
        let (status, data) = get(
            app.clone(),
            &format!("{base}?severity={severity}&game_no=1&player=0&kind=checker&limit=1"),
        )
        .await;
        assert_eq!(status, 200, "{data}");
        assert_eq!(data["items"][0]["id"], id);
        assert!(
            data["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["severity"] == severity)
        );
    }
    let filters = "severity=bad&errors_only=true&game_no=1&player=0&kind=checker&limit=1";
    let (_, first) = get(app.clone(), &format!("{base}?{filters}")).await;
    assert_eq!(first["next_cursor"], bad);
    let (status, next) = get(app.clone(), &format!("{base}?{filters}&after={bad}")).await;
    assert_eq!(status, 200);
    assert_eq!(next["items"][0]["id"], second_bad);
    assert!(next["next_cursor"].is_null());
    let (_, empty) = get(
        app.clone(),
        &format!("{base}?severity=none&errors_only=true"),
    )
    .await;
    assert_eq!(empty["items"], json!([]));
    assert_eq!(
        get(
            app.clone(),
            &format!("{base}?severity=bad&after={doubtful}")
        )
        .await
        .0,
        422
    );
    assert_eq!(
        get(app.clone(), &format!("{base}?severity=unknown"))
            .await
            .0,
        422
    );
    assert_eq!(
        get(app, &format!("{base}?severity=bad&after={none}"))
            .await
            .0,
        422
    );
}

#[test]
fn openapi_documents_statistics_and_severity_values() {
    use utoipa::OpenApi;
    let doc = serde_json::to_value(api::ApiDoc::openapi()).unwrap();
    assert_eq!(
        doc["paths"]["/api/jobs/{id}/statistics"]["get"]["responses"]["200"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/JobAnalysisStatistics"
    );
    let parameters = doc["paths"]["/api/jobs/{id}/moves"]["get"]["parameters"]
        .as_array()
        .unwrap();
    let severity = parameters.iter().find(|p| p["name"] == "severity").unwrap();
    assert_eq!(severity["required"], false);
    assert_eq!(severity["in"], "query");
    assert_eq!(
        severity["schema"]["$ref"],
        "#/components/schemas/MoveSeverity"
    );
    assert!(
        parameters
            .iter()
            .filter(|p| p["name"] != "id")
            .all(|p| p["in"] == "query" && p["required"] == false)
    );
    assert_eq!(
        doc["components"]["schemas"]["MoveSeverity"]["enum"],
        json!(["none", "doubtful", "bad", "very_bad"])
    );
    assert!(
        doc["components"]["schemas"]["GameAnalysisStatistics"]["properties"]["analyzed"]
            .is_object()
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn statistics_read_legacy_source_catalog_without_writes(pool: PgPool) {
    let (_, app, match_id) = setup(&pool).await;
    let job = store::create_match_job_scoped(&pool, match_id, 0, 1, Some(3))
        .await
        .unwrap();
    sqlx::query("DELETE FROM match_games WHERE match_uuid=$1")
        .bind(match_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,3)")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, data) = get(app, &format!("/api/jobs/{}/statistics", job.id)).await;
    assert_eq!(status, 200, "{data}");
    assert_eq!(data["games"].as_array().unwrap().len(), 1);
    assert_eq!(data["games"][0]["game_no"], 3);
    assert_eq!(data["games"][0]["white_score"], 6);
    assert_eq!(data["games"][0]["crawford"], true);
    assert_eq!(data["games"][0]["analyzed"], true);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM match_games WHERE match_uuid=$1")
        .bind(match_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "statistics reads do not mutate legacy catalogs");
}
