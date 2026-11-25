use crate::support;
use neondojo::{
    api::{self, AppState},
    engine::Engine,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::call;

async fn setup(pool: &PgPool) -> axum::Router {
    let python =
        std::env::var("OPEN_SAGE_TEST_PYTHON").expect("Explicit locked Sage interpreter required");
    let app = api::router(AppState::new(
        pool.clone(),
        Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())),
    ));
    let (status, installed) = call(
        app.clone(),
        "PUT",
        "/api/engines/bgsage",
        json!({"version":0,"path":python}),
    )
    .await;
    assert_eq!(status, 200, "{installed}");
    let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    settings["play_engine"] = json!("bgsage");
    settings["play_plies"] = json!(2);
    settings["analysis_engine"] = json!("bgsage");
    settings["analysis_plies"] = json!(2);
    let (status, changed) = call(app.clone(), "PUT", "/api/settings", settings).await;
    assert_eq!(status, 200, "{changed}");
    app
}

async fn action(app: &axum::Router, session: &Value, name: &str, extra: Value) -> (u16, Value) {
    let mut request = json!({"version":session["version"],"action":name});
    for (key, value) in extra.as_object().unwrap() {
        request[key] = value.clone();
    }
    call(
        app.clone(),
        "POST",
        &format!(
            "/api/play/sessions/{}/actions",
            session["id"].as_str().unwrap()
        ),
        request,
    )
    .await
}

async fn first_checker_decision(app: &axum::Router, mut session: Value) -> Value {
    for _ in 0..5 {
        let next = match session["phase"].as_str().unwrap() {
            "opening_roll" => "opening_roll",
            "opening_result" => "start_opening_move",
            "roll" => "roll",
            "move" => break,
            phase => panic!("Unexpected setup phase {phase}"),
        };
        let (status, result) = action(app, &session, next, json!({})).await;
        assert_eq!(status, 200, "{result}");
        session = result;
    }
    assert_eq!(session["phase"], "move");
    session
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL, GNUbg Python, locked OPEN_SAGE_TEST_PYTHON"]
async fn sage_full_game_both_colors_restores_pinned_engines_and_has_no_gnubg_pr(pool: PgPool) {
    let app = setup(&pool).await;
    for human in [0, 1] {
        let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
        settings["play_engine"] = json!("bgsage");
        settings["play_plies"] = json!(2);
        settings["analysis_engine"] = json!("bgsage");
        settings["analysis_plies"] = json!(2);
        assert_eq!(
            call(app.clone(), "PUT", "/api/settings", settings).await.0,
            200
        );
        let (status, mut session) = call(
            app.clone(),
            "POST",
            "/api/play/sessions",
            json!({"human_player":human,"match_length":1,"play_mode":"competition"}),
        )
        .await;
        assert_eq!(status, 201, "{session}");
        assert_eq!(session["engine_snapshots"]["rules"]["engine_id"], "gnubg");
        assert_eq!(
            session["engine_snapshots"]["bot"]["open_sage"]["threads"],
            2
        );
        assert_eq!(
            session["engine_snapshots"]["analysis"]["open_sage"]["threads"],
            4
        );
        let pinned = session["engine_snapshots"].clone();
        // Settings changes affect future sessions only, including review engine.
        let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
        settings["play_engine"] = json!("gnubg");
        settings["play_plies"] = json!(1);
        settings["analysis_engine"] = json!("gnubg");
        settings["analysis_plies"] = json!(1);
        assert_eq!(
            call(app.clone(), "PUT", "/api/settings", settings).await.0,
            200
        );
        let mut finished = false;
        for turn in 0..500 {
            if turn == 3 {
                let id: uuid::Uuid = session["id"].as_str().unwrap().parse().unwrap();
                let checkpoint = sqlx::query_as::<_, (String, String)>(
                    "SELECT sgf,native_id FROM play_sessions WHERE id=$1",
                )
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
                let (status, paused) = action(&app, &session, "pause", json!({})).await;
                assert_eq!(status, 200, "{paused}");
                assert_eq!(paused["paused"], true);
                assert_eq!(paused["engine_snapshots"], pinned);
                let (status, resumed) = action(&app, &paused, "resume", json!({})).await;
                assert_eq!(status, 200, "{resumed}");
                assert_eq!(resumed["paused"], false);
                for field in [
                    "engine_snapshots",
                    "position",
                    "history",
                    "library_match_id",
                ] {
                    assert_eq!(resumed[field], session[field], "pause changed {field}");
                }
                assert_eq!(
                    sqlx::query_as::<_, (String, String)>(
                        "SELECT sgf,native_id FROM play_sessions WHERE id=$1",
                    )
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                    checkpoint
                );
                session = resumed;
            }
            let next = match session["phase"].as_str().unwrap() {
                "opening_roll" => "opening_roll",
                "opening_result" => "start_opening_move",
                "roll" => "roll",
                "double_response" => "take",
                "move" => {
                    if session["legal_moves"].as_array().unwrap().is_empty() {
                        "pass"
                    } else {
                        "move"
                    }
                }
                "match_over" => {
                    finished = true;
                    break;
                }
                phase => panic!("Unexpected phase {phase}"),
            };
            let extra = if next == "move" {
                assert!(
                    session["legal_turns"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|turn| { turn["move_index"] == 0 }),
                    "The top GNUbg move must have a selectable path: {session}"
                );
                json!({"move": 0})
            } else {
                json!({})
            };
            let (status, result) = action(&app, &session, next, extra).await;
            assert_eq!(status, 200, "{result}");
            session = result;
            assert_eq!(session["engine_snapshots"], pinned);
            if session["phase"] != "opening_result" && session["phase"] != "match_over" {
                assert_eq!(session["position"]["player"], human);
            }
            let (status, restored) = call(
                app.clone(),
                "GET",
                &format!("/api/play/sessions/{}", session["id"].as_str().unwrap()),
                Value::Null,
            )
            .await;
            assert_eq!(status, 200);
            assert_eq!(restored["engine_snapshots"], pinned);
        }
        assert!(finished, "Bounded game did not finish");
        let (status, report) = call(
            app.clone(),
            "POST",
            &format!(
                "/api/play/sessions/{}/review",
                session["id"].as_str().unwrap()
            ),
            json!({"version":session["version"]}),
        )
        .await;
        assert_eq!(status, 200, "{report}");
        assert!(report["review"]["first_attempt_rating"].is_null());
        let games = report["review"]["games"].as_array().unwrap();
        assert!(!games.is_empty());
        assert!(games.iter().all(|game| game["statistics"].is_null()
            && game["engine"].as_str().unwrap().starts_with("NeonGammon")));
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL, GNUbg Python, locked OPEN_SAGE_TEST_PYTHON"]
async fn sage_explicit_cube_drop_and_resignation_preserve_native_checkpoint(pool: PgPool) {
    let app = setup(&pool).await;
    for (action_name, level, expected_score) in [
        ("double", None, 1),
        ("resign", Some(1), 1),
        ("resign", Some(2), 2),
        ("resign", Some(3), 3),
    ] {
        let (_, mut session) = call(
            app.clone(),
            "POST",
            "/api/play/sessions",
            json!({"human_player":0,"match_length":5,"play_mode":"competition"}),
        )
        .await;
        let sgf = format!(
            "(;FF[4]GM[6]MI[length:5][game:0][ws:0][bs:0]PW[White]PB[Black]RU[Crawford];PL[W];PL[W]AE[a:y]AW[a]AB{})",
            "[s]".repeat(15)
        );
        let gid = "4P8PAAABAAAAAA:MAGgAAAAAAAE";
        session["phase"] = json!("roll");
        session["can_double"] = json!(true);
        session["position"]["position_id"] = json!("4P8PAAABAAAAAA");
        session["position"]["gnubg_match_id"] = json!("MAGgAAAAAAAE");
        let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
        sqlx::query("UPDATE play_sessions SET state=$1,sgf=$2,native_id=$3 WHERE id=$4")
            .bind(&session)
            .bind(&sgf)
            .bind(gid)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        let extra = level.map_or(json!({}), |level| json!({"resign_level":level}));
        let (status, result) = action(&app, &session, action_name, extra).await;
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["phase"], "game_over");
        assert_eq!(
            result["scores"][if level.is_some() { 1 } else { 0 }],
            expected_score
        );
        let (status, next) = action(&app, &result, "next_game", json!({})).await;
        assert_eq!(status, 200, "{next}");
        let (status, rolled) = action(&app, &next, "opening_roll", json!({})).await;
        assert_eq!(status, 200, "{rolled}");
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL, GNUbg Python, locked OPEN_SAGE_TEST_PYTHON"]
async fn sage_declined_resignation_leaves_exact_saved_state_unchanged(pool: PgPool) {
    let app = setup(&pool).await;
    let (_, mut session) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"human_player":0,"match_length":5,"play_mode":"competition"}),
    )
    .await;
    let sgf = format!(
        "(;FF[4]GM[6]MI[length:5][game:0][ws:0][bs:0]PW[White]PB[Black]RU[Crawford];PL[W]DI[21];PL[W]AE[a:y]AW{}AB[x])",
        "[g]".repeat(15)
    );
    let gid = "AQAAAP9/AAAAAA:MAGlAAAAAAAE";
    session["phase"] = json!("move");
    session["legal_moves"] = json!([]);
    session["position"]["position_id"] = json!("AQAAAP9/AAAAAA");
    session["position"]["gnubg_match_id"] = json!("MAGlAAAAAAAE");
    let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE play_sessions SET state=$1,sgf=$2,native_id=$3 WHERE id=$4")
        .bind(&session)
        .bind(&sgf)
        .bind(gid)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(s) FROM play_sessions s WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let (status, rejected) = action(&app, &session, "resign", json!({"resign_level":1})).await;
    assert_eq!(status, 422, "{rejected}");
    assert!(rejected["error"].as_str().unwrap().contains("declined"));
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(s) FROM play_sessions s WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, before);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL, GNUbg Python, locked OPEN_SAGE_TEST_PYTHON"]
async fn bot_and_hint_engines_are_independent_in_both_directions(pool: PgPool) {
    let app = setup(&pool).await;
    for (bot, analysis) in [("bgsage", "gnubg"), ("gnubg", "bgsage")] {
        let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
        settings["play_engine"] = json!(bot);
        settings["play_plies"] = json!(if bot == "bgsage" { 2 } else { 1 });
        settings["analysis_engine"] = json!(analysis);
        settings["analysis_plies"] = json!(if analysis == "bgsage" { 2 } else { 1 });
        assert_eq!(
            call(app.clone(), "PUT", "/api/settings", settings).await.0,
            200
        );
        let (status, mut session) = call(app.clone(), "POST", "/api/play/sessions", json!({"human_player":0,"match_length":1,"play_mode":"training","training_auto_analysis":false})).await;
        assert_eq!(status, 201, "{session}");
        assert_eq!(session["engine_snapshots"]["bot"]["engine_id"], bot);
        assert_eq!(
            session["engine_snapshots"]["analysis"]["engine_id"],
            analysis
        );
        session = first_checker_decision(&app, session).await;
        let (status, hinted) = call(
            app.clone(),
            "POST",
            &format!(
                "/api/play/sessions/{}/feedback",
                session["id"].as_str().unwrap()
            ),
            json!({"version":session["version"],"operation":"hint"}),
        )
        .await;
        assert_eq!(status, 200, "{hinted}");
        let alternatives = hinted["pending_decision"]["alternatives"]
            .as_array()
            .unwrap();
        assert_eq!(
            alternatives.len(),
            session["legal_moves"].as_array().unwrap().len()
        );
        assert!(
            alternatives
                .iter()
                .all(|candidate| candidate["equity"].is_number())
        );
        if analysis == "bgsage" {
            assert!(
                alternatives
                    .iter()
                    .all(|candidate| candidate["rating_decisions"] == 0)
            );
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires GNUbg Python and locked OPEN_SAGE_TEST_PYTHON, run serially"]
async fn benchmark_hint_and_practice_finish_while_a_verified_batch_owns_general(pool: PgPool) {
    use neondojo::{engines::SageEngine, import, store};
    use tokio::{
        sync::Notify,
        time::{Duration, timeout},
    };

    let app = setup(&pool).await;
    let (status, session) = call(app.clone(), "POST", "/api/play/sessions",
        json!({"human_player":0,"match_length":1,"play_mode":"training","training_auto_analysis":false})).await;
    assert_eq!(status, 201, "{session}");
    let session = first_checker_decision(&app, session).await;
    let engine = SageEngine::new(
        serde_json::from_value(session["engine_snapshots"]["analysis"]["open_sage"].clone())
            .unwrap(),
    )
    .unwrap();
    let sgf = "(;GM[6]MI[length:7];W[31hefe])";
    let match_id = store::import_match(
        &pool,
        "admission.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    let position = store::positions(&pool, Some(match_id), None, None, 1)
        .await
        .unwrap()
        .items
        .pop()
        .unwrap();

    // The progress callback runs after a real evaluation, while the batch still
    // owns the General worker. Hold it until all three HTTP responses arrive.
    let held = Notify::new();
    let release = Notify::new();
    let batch = engine.analyze_rolls(
        &position.position_id,
        &position.gnubg_match_id,
        2,
        &[[3, 1]],
        |_| async {
            held.notify_one();
            release.notified().await;
            Ok(())
        },
    );
    let requests = async {
        held.notified().await;
        let hint = format!(
            "/api/play/sessions/{}/feedback",
            session["id"].as_str().unwrap()
        );
        let practice = format!("/api/positions/{}/practice-score", position.id);
        let post = async |uri: &str, body: Value, expected: u16| {
            let (status, result) =
                timeout(Duration::from_secs(1), call(app.clone(), "POST", uri, body))
                    .await
                    .unwrap_or_else(|_| {
                        panic!("POST {uri} exceeded one second with General occupied")
                    });
            assert_eq!(status, expected, "POST {uri} returned {result}");
            result
        };
        let feedback = post(
            &hint,
            json!({"version":session["version"],"operation":"hint"}),
            200,
        )
        .await;
        assert!(feedback["pending_decision"]["alternatives"][0]["equity"].is_number());
        let score = post(
            &practice,
            json!({"steps":[{"from":8,"to":5},{"from":6,"to":5}]}),
            200,
        )
        .await;
        assert!(score["best_equity"].is_number());
        let runs = post(
            "/api/benchmarks",
            json!({"engine_id":"bgsage","plies":[2]}),
            202,
        )
        .await;
        assert_eq!(runs[0]["engine_id"], "bgsage");
        release.notify_one();
        Ok::<(), anyhow::Error>(())
    };
    let (rolls, ()) = tokio::try_join!(batch, requests).unwrap();
    assert_eq!(rolls.len(), 1);
}
