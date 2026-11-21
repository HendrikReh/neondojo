use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{
    api::{self, AppState},
    engine::Engine,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
mod profile_support;

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

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn play_create_resume_actions_and_atomic_failures(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (status, bad) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":2,"match_length":0}),
    )
    .await;
    assert_eq!(status, 422, "{bad}");
    for human in [0, 1] {
        let (status, mut session) = call(
            app.clone(),
            "POST",
            "/api/play/sessions",
            json!({"play_mode":"competition","human_player":human,"match_length":5}),
        )
        .await;
        assert_eq!(status, 201, "{session}");
        session = ready(app.clone(), session).await;
        assert_eq!(session["human_player"], human);
        assert_eq!(session["engine"]["plies"], 1);
        assert_eq!(
            session["engine"]["version"],
            engine().version().await.unwrap()
        );
        let uri = format!("/api/play/sessions/{}", session["id"].as_str().unwrap());
        assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, session);
        for _ in 0..4 {
            let version = session["version"].as_i64().unwrap();
            let action = if session["phase"] == "roll" {
                "roll"
            } else if session["phase"] == "double_response" {
                "take"
            } else if session["legal_moves"].as_array().unwrap().is_empty() {
                "pass"
            } else {
                "move"
            };
            let body = if action == "move" {
                json!({"version":version,"action":action,"move":0})
            } else {
                json!({"version":version,"action":action})
            };
            let (status, next) = call(app.clone(), "POST", &format!("{uri}/actions"), body).await;
            assert_eq!(status, 200, "{next}");
            assert_eq!(next["version"], version + 1);
            assert_eq!(next["position"]["player"], human);
            session = next;
            let (status, _) = call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":version,"action":"roll"}),
            )
            .await;
            assert_eq!(status, 409);
            assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, session);
        }
        let (status, _) = call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":session["version"],"action":"move","move":999999}),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, session);
    }
    let (_, list) = call(app.clone(), "GET", "/api/play/sessions", Value::Null).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    let (_, schema) = call(app, "GET", "/api/openapi.json", Value::Null).await;
    assert!(schema["paths"]["/api/play/sessions"].is_object());
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
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn native_gammon_backgammon_crawford_and_match_completion(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    for (score, point, gid, points, phase) in [
        (0, "s", "4P8PAAABAAAAAA:MAGnAAAAAAAE", 2, "game_over"),
        (0, "b", "AADA/x8BAAAAAA:MAGnAAAAAAAE", 3, "game_over"),
        (2, "s", "4P8PAAABAAAAAA:MAGnACAAAAAE", 2, "game_over"),
        (3, "s", "4P8PAAABAAAAAA:MAGnADAAAAAE", 2, "match_over"),
    ] {
        let s = seed(
            &pool,
            &app,
            score,
            &format!("[{point}]").repeat(15),
            "61",
            gid,
            "move",
            json!([{"notation":"1/off","steps":[{"from":1,"to":-1}]}]),
        )
        .await;
        let uri = format!("/api/play/sessions/{}", s["id"].as_str().unwrap());
        let (status, finished) = call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":0,"action":"move","move":0}),
        )
        .await;
        assert_eq!(status, 200, "{finished}");
        assert_library_checkpoint(&pool, &app, &finished).await;
        assert_eq!(finished["phase"], phase);
        assert_eq!(finished["winner"], 0);
        assert_eq!(finished["scores"], json!([score + points, 0]));
        assert_eq!(
            call(app.clone(), "GET", &uri, Value::Null).await.1,
            finished
        );
        if score == 2 {
            let old_checkpoint: (String, String) =
                sqlx::query_as("SELECT sgf,native_id FROM play_sessions WHERE id=$1")
                    .bind(uuid::Uuid::parse_str(s["id"].as_str().unwrap()).unwrap())
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let (status, next) = call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":1,"action":"next_game"}),
            )
            .await;
            assert_eq!(status, 200, "{next}");
            assert_library_checkpoint(&pool, &app, &next).await;
            assert_eq!(next["game_no"], 2);
            assert_eq!(next["scores"], json!([4, 0]));
            assert_eq!(next["can_double"], false);
            assert_eq!(next["phase"], "opening_roll");
            assert_eq!(next["position"]["dice"], json!([]));
            assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, next);
            let held_checkpoint: (String, String) =
                sqlx::query_as("SELECT sgf,native_id FROM play_sessions WHERE id=$1")
                    .bind(uuid::Uuid::parse_str(s["id"].as_str().unwrap()).unwrap())
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                held_checkpoint, old_checkpoint,
                "NextGame must not roll or create the next native game yet"
            );
            use base64::Engine as _;
            let key = base64::engine::general_purpose::STANDARD_NO_PAD
                .decode(next["position"]["gnubg_match_id"].as_str().unwrap())
                .unwrap();
            assert_ne!(key[0] & 0x80, 0, "The staged next game is Crawford");
            let (status, result) = call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":next["version"],"action":"opening_roll"}),
            )
            .await;
            assert_eq!(status, 200, "{result}");
            assert_library_checkpoint(&pool, &app, &result).await;
            assert_eq!(result["phase"], "opening_result");
            assert_eq!(result["game_no"], 2);
            assert_eq!(result["history"], finished["history"]);
            assert_eq!(result["legal_moves"], json!([]));
            assert_eq!(result["scores"], json!([4, 0]));
            assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, result);
            let next = ready(app.clone(), result).await;
            // A native Crawford marker must survive export and a subsequent action.
            let sgf: String = sqlx::query_scalar("SELECT sgf FROM play_sessions WHERE id=$1")
                .bind(uuid::Uuid::parse_str(s["id"].as_str().unwrap()).unwrap())
                .fetch_one(&pool)
                .await
                .unwrap();
            assert!(sgf.contains("CrawfordGame"), "{sgf}");
            let action = if next["phase"] == "roll" {
                json!({"version":next["version"],"action":"roll"})
            } else {
                json!({"version":next["version"],"action":"move","move":0})
            };
            assert_eq!(
                call(app.clone(), "POST", &format!("{uri}/actions"), action)
                    .await
                    .0,
                200
            );
        }
    }
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
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn native_cube_take_drop_bot_response_and_forced_pass(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    for action in ["take", "drop"] {
        let (_, mut s) = call(
            app.clone(),
            "POST",
            "/api/play/sessions",
            json!({"play_mode":"competition","human_player":0,"match_length":5}),
        )
        .await;
        s["phase"] = json!("double_response");
        s["legal_moves"] = json!([]);
        s["can_double"] = json!(false);
        replace_native(&pool,&s,"(;FF[4]GM[6]MI[length:5][game:0][ws:0][bs:0]PW[Human]PB[GNUbg]RU[Crawford];PL[B];B[double])","4HPwATDgc/ABMA:cBGgAAAAAAAE").await;
        let uri = format!("/api/play/sessions/{}/actions", s["id"].as_str().unwrap());
        let (status, next) = call(
            app.clone(),
            "POST",
            &uri,
            json!({"version":0,"action":action}),
        )
        .await;
        assert_eq!(status, 200, "{next}");
        assert_library_checkpoint(&pool, &app, &next).await;
        if action == "drop" {
            assert_eq!(next["phase"], "game_over");
            assert_eq!(next["scores"], json!([0, 1]));
            assert_eq!(next["winner"], 1);
        } else {
            assert_eq!(next["phase"], "roll");
            assert!(
                next["history"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|h| h["notation"] == "take")
            );
        }
    }
    // Native bot drops a hopeless cube offer, producing a single-point game.
    let s = seed(
        &pool,
        &app,
        0,
        &"[s]".repeat(15),
        "00",
        "4P8PAAABAAAAAA:MAGgAAAAAAAE",
        "roll",
        json!([]),
    )
    .await;
    let id = s["id"].as_str().unwrap();
    let sgf: String = sqlx::query_scalar("SELECT sgf FROM play_sessions WHERE id=$1")
        .bind(uuid::Uuid::parse_str(id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    replace_native(
        &pool,
        &s,
        &sgf.replace("DI[00]", ""),
        "4P8PAAABAAAAAA:MAGgAAAAAAAE",
    )
    .await;
    let (status, next) = call(
        app.clone(),
        "POST",
        &format!("/api/play/sessions/{id}/actions"),
        json!({"version":0,"action":"double"}),
    )
    .await;
    assert_eq!(status, 200, "{next}");
    assert_eq!(next["phase"], "game_over");
    assert_library_checkpoint(&pool, &app, &next).await;
    assert_eq!(next["scores"], json!([1, 0]));
    assert_eq!(next["history"][1]["notation"], "drop");
    // Bar entry on either die is blocked. Passing is native and bot advances.
    let (_, mut s) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    s["phase"] = json!("move");
    s["legal_moves"] = json!([]);
    replace_native(&pool,&s,"(;FF[4]GM[6]MI[length:5][game:0][ws:0][bs:0]PW[Human]PB[GNUbg]RU[Crawford];PL[W]DI[21];PL[W]AE[a:y]AW[y]AB[x][x][w][w][s][s][s][s][s][s][s][s][s][s][s])","G/4PAAAAAAABAA:MAGlAAAAAAAE").await;
    let (status, next) = call(
        app.clone(),
        "POST",
        &format!("/api/play/sessions/{}/actions", s["id"].as_str().unwrap()),
        json!({"version":0,"action":"pass"}),
    )
    .await;
    assert_eq!(status, 200, "{next}");
    assert_eq!(next["phase"], "roll");
    assert_eq!(next["history"][0]["notation"], "pass");
    assert_eq!(next["position"]["board"][25], 1);
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn roll_where_both_players_dance_returns_to_the_same_native_state(pool: PgPool) {
    // Both players have a checker on the bar and enter only with a 1: White
    // (human) on 24, Black (bot) on White's 1 point. A roll without a 1 for
    // both players is accepted natively but restores the identical GNUbg ID.
    // The bot recorded B[65] as a dance after White's partial entry W[31yx].
    let sgf = "(;FF[4]GM[6]MI[length:1][game:0][ws:0][bs:0]PW[Human]PB[GNUbg]RU[Crawford];PL[W]AE[a:y]AW[y][y][b][b][c][c][d][d][e][e][f][f][m][m][m]AB[x][s][s][t][t][u][u][v][v][w][w][l][l][l][l];W[31yx];B[65])";
    let native_id = "tm3AA0C2bcABUA:MAEgAAAAAAAE";
    let board = json!([
        -1, 0, 2, 2, 2, 2, 2, 0, 0, 0, 0, 0, -4, 3, 0, 0, 0, 0, 0, -2, -2, -2, -2, -2, 1, 1
    ]);
    let app = api::router(AppState::new(pool.clone(), engine()));
    // Each roll uses GNUbg's own random dice; both dance with (25/36)² ≈ 48 %.
    // Every attempt starts a new session, so its library match has no
    // positions from an earlier attempt's different dice.
    let mut double_dances = 0;
    for _ in 0..40 {
        let (status, mut s) = call(
            app.clone(),
            "POST",
            "/api/play/sessions",
            json!({"play_mode":"competition","human_player":0,"match_length":1}),
        )
        .await;
        assert_eq!(status, 201, "{s}");
        s["phase"] = json!("roll");
        s["legal_moves"] = json!([]);
        s["can_double"] = json!(false);
        s["position"]["board"] = board.clone();
        s["history"] = json!([
            {"player":0,"dice":[3,1],"notation":"bar/24"},
            {"player":1,"dice":[6,5],"notation":"pass"}
        ]);
        replace_native(&pool, &s, sgf, native_id).await;
        let uri = format!("/api/play/sessions/{}/actions", s["id"].as_str().unwrap());
        let (status, next) = call(
            app.clone(),
            "POST",
            &uri,
            json!({"version":0,"action":"roll"}),
        )
        .await;
        assert_eq!(status, 200, "{next}");
        let history = next["history"].as_array().unwrap();
        let both_danced =
            history.len() == 4 && history[2..].iter().all(|turn| turn["notation"] == "pass");
        if both_danced {
            assert_eq!(next["phase"], "roll");
            assert_eq!(next["position"]["player"], 0);
            assert_eq!(next["position"]["board"], board);
            assert_eq!(
                format!(
                    "{}:{}",
                    next["position"]["position_id"].as_str().unwrap(),
                    next["position"]["gnubg_match_id"].as_str().unwrap()
                ),
                native_id
            );
            assert_eq!(history[2]["player"], 0);
            assert_eq!(history[3]["player"], 1);
            assert_library_checkpoint(&pool, &app, &next).await;
            double_dances += 1;
            if double_dances == 2 {
                break;
            }
        }
    }
    assert_eq!(double_dances, 2, "no two rolls made both players dance");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn saved_configuration_concurrent_actions_and_native_failure_are_atomic(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let s = seed(
        &pool,
        &app,
        0,
        &"[s]".repeat(15),
        "00",
        "4P8PAAABAAAAAA:MAGgAAAAAAAE",
        "roll",
        json!([]),
    )
    .await;
    let id = uuid::Uuid::parse_str(s["id"].as_str().unwrap()).unwrap();
    let sgf: String = sqlx::query_scalar("SELECT sgf FROM play_sessions WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    replace_native(
        &pool,
        &s,
        &sgf.replace("DI[00]", ""),
        "4P8PAAABAAAAAA:MAGgAAAAAAAE",
    )
    .await;
    sqlx::query("UPDATE app_settings SET gnubg_path='/missing/changed-settings',play_plies=2,player_name='Changed',bearoff_direction='left'").execute(&pool).await.unwrap();
    let resumed = api::router(AppState::new(
        pool.clone(),
        Engine::new("/missing/new-app-default".into()),
    ));
    let uri = format!("/api/play/sessions/{id}/actions");
    let (a, b) = tokio::join!(
        call(
            resumed.clone(),
            "POST",
            &uri,
            json!({"version":0,"action":"roll"})
        ),
        call(
            resumed.clone(),
            "POST",
            &uri,
            json!({"version":0,"action":"roll"})
        )
    );
    let mut statuses = vec![a.0, b.0];
    statuses.sort();
    assert_eq!(statuses, vec![200, 409], "{a:?} {b:?}");
    let (_, current) = call(
        resumed.clone(),
        "GET",
        &format!("/api/play/sessions/{id}"),
        Value::Null,
    )
    .await;
    assert_eq!(current["version"], 1);
    assert_eq!(current["engine"], s["engine"]);
    assert_eq!(current["human_name"], s["human_name"]);
    assert_eq!(current["bearoff_direction"], s["bearoff_direction"]);
    let pinned = pin_engine_path(&pool, id, "/missing/pinned-engine").await;
    let (status, _) = call(
        resumed.clone(),
        "POST",
        &uri,
        json!({"version":1,"action":"move","move":0}),
    )
    .await;
    assert_eq!(status, 503);
    // The saved session exposes its pinned engines; compare it unchanged.
    pin_engine_path(&pool, id, &pinned).await;
    assert_eq!(
        call(
            resumed,
            "GET",
            &format!("/api/play/sessions/{id}"),
            Value::Null
        )
        .await
        .1,
        current
    );
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn whole_native_match_reaches_scored_terminal_and_exports_no_pending_move(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (status, mut s) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":1,"match_length":1}),
    )
    .await;
    assert_eq!(status, 201, "{s}");
    s = ready(app.clone(), s).await;
    let id = s["id"].as_str().unwrap().to_owned();
    for _ in 0..800 {
        if s["phase"] == "match_over" {
            break;
        }
        let action = match s["phase"].as_str().unwrap() {
            "roll" => "roll",
            "move" if s["legal_moves"].as_array().unwrap().is_empty() => "pass",
            "move" => "move",
            "double_response" => "take",
            _ => panic!("unexpected phase {s}"),
        };
        let mut body = json!({"version":s["version"],"action":action});
        if action == "move" {
            body["move"] = json!(0);
        }
        let (status, next) = call(
            app.clone(),
            "POST",
            &format!("/api/play/sessions/{id}/actions"),
            body,
        )
        .await;
        assert_eq!(status, 200, "{next}");
        s = next;
    }
    assert_eq!(s["phase"], "match_over");
    assert!(
        s["scores"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_i64().unwrap() >= 1)
    );
    assert!(s["history"].as_array().unwrap().len() > 30);
    let results = s["game_results"]
        .as_array()
        .expect("completed action response explains its game");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["game_no"], 1);
    assert_eq!(
        results[0]["history_index"],
        s["history"].as_array().unwrap().len() - 1
    );
    assert_eq!(results[0]["result"]["scores_after"], s["scores"]);
    assert!(results[0]["result"]["multiplier"].is_i64());

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/play/sessions/{id}/sgf"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(
        response.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .contains(".sgf")
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let sgf = String::from_utf8(bytes.to_vec()).unwrap();
    let imported = neondojo::import::parse_sgf(&sgf).unwrap();
    assert!(!imported.positions.is_empty());
    assert!(sgf.contains("RE["));
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn match_choices_override_name_and_strength_without_changing_settings(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    call(app.clone(), "GET", "/api/settings", Value::Null).await;
    sqlx::query("UPDATE app_settings SET play_engine='gnubg',play_plies=1,player_name='Saved'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, s) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"human_player":1,"match_length":3,"human_name":"  Guest  ",
               "play_engine":"gnubg","play_plies":2}),
    )
    .await;
    assert_eq!(status, 201, "{s}");
    assert_eq!(s["human_name"], "Guest");
    assert_eq!(s["engine"]["id"], "gnubg");
    assert_eq!(s["engine"]["plies"], 2);
    assert_eq!(s["engine_snapshots"]["bot"]["engine_id"], "gnubg");
    let saved: (String, String, i32) =
        sqlx::query_as("SELECT player_name,play_engine,play_plies FROM app_settings")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(saved, ("Saved".into(), "gnubg".into(), 1));
    // A blank name and an omitted strength fall back to the saved settings.
    let (status, s) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"human_player":0,"match_length":1,"human_name":"   "}),
    )
    .await;
    assert_eq!(status, 201, "{s}");
    assert_eq!(s["human_name"], "Saved");
    assert_eq!(s["engine"]["plies"], 1);
    for body in [
        json!({"human_player":0,"match_length":1,"play_plies":4}),
        json!({"human_player":0,"match_length":1,"play_engine":"xg"}),
        json!({"human_player":0,"match_length":1,"human_name":"a\u{7}b"}),
        json!({"human_player":0,"match_length":1,"human_name":"x".repeat(121)}),
    ] {
        let (status, error) = call(app.clone(), "POST", "/api/play/sessions", body).await;
        assert_eq!(status, 422, "{error}");
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn creation_pins_actual_executable_profile_name_direction_and_sgf_names(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    call(app.clone(), "GET", "/api/settings", Value::Null).await;
    // Seed only GNUbg's native RNG. Seed 1 produces an opening bot decision
    // with several close choices, so its native SGF must record a real 2C
    // evaluation rather than a legitimate early-pruned 0C decision.
    use std::os::unix::fs::PermissionsExt;
    let probe = tempfile::tempdir().unwrap();
    let wrapper = probe.path().join("seeded-gnubg");
    let quoted = format!("'{}'", engine().binary().replace('\'', "'\"'\"'"));
    std::fs::write(&wrapper, format!("#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then exec {quoted} \"$@\"; fi\nprintf 'set seed 1\\n' > seeded.txt\ncat commands.txt >> seeded.txt\nexec {quoted} -t -q -r -c seeded.txt\n")).unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    sqlx::query("UPDATE app_settings SET gnubg_path=$1")
        .bind(wrapper.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE app_settings SET play_plies=3,player_name=$1,bearoff_direction='left'")
        .bind("Test ] \\ name")
        .execute(&pool)
        .await
        .unwrap();
    let (status, mut s) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    assert_eq!(status, 201, "{s}");
    s = ready(app.clone(), s).await;
    assert_eq!(s["engine"]["plies"], 3);
    assert_eq!(s["human_name"], "Test ] \\ name");
    assert_eq!(s["bearoff_direction"], "left");
    let id = uuid::Uuid::parse_str(s["id"].as_str().unwrap()).unwrap();
    let path: String = sqlx::query_scalar("SELECT gnubg_path FROM play_sessions WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        std::fs::canonicalize(&wrapper).unwrap().to_str().unwrap(),
        path
    );
    // Execute enough to guarantee at least one actual bot checker decision.
    for _ in 0..3 {
        let action = if s["phase"] == "roll" { "roll" } else { "move" };
        let mut body = json!({"version":s["version"],"action":action});
        if action == "move" {
            body["move"] = json!(0);
        }
        let (status, next) = call(
            app.clone(),
            "POST",
            &format!("/api/play/sessions/{id}/actions"),
            body,
        )
        .await;
        assert_eq!(status, 200, "{next}");
        s = next;
    }
    let sgf: String = sqlx::query_scalar("SELECT sgf FROM play_sessions WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        sgf.contains("2C 0 1 0.000000"),
        "Bot must use its captured 3-ply (GNUbg-native 2-ply) profile: {sgf}"
    );
    // Seed the finishing scenario in a fresh session: the previous session's
    // played decisions are already archived and must never be rewritten by a
    // synthetic fixture. Both sessions capture the same configured profile.
    let (status, fresh) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    assert_eq!(status, 201, "{fresh}");
    assert_eq!(fresh["engine"], s["engine"]);
    assert_eq!(fresh["history"], json!([]));
    s = fresh;
    let id = uuid::Uuid::parse_str(s["id"].as_str().unwrap()).unwrap();
    // Finish this native game and verify that the next game's deterministic
    // non-trivial opening retains its captured 2-ply profile.
    s["phase"] = json!("move");
    s["legal_moves"] = json!([{"notation":"1/off","steps":[{"from":1,"to":-1}]}]);
    s["scores"] = json!([2, 0]);
    replace_native(&pool,&s,&format!("(;FF[4]GM[6]MI[length:5][game:0][ws:2][bs:0]PW[Human]PB[GNUbg]RU[Crawford];PL[W]DI[61];PL[W]AE[a:y]AW[a]AB{})","[s]".repeat(15)),"4P8PAAABAAAAAA:MAGnACAAAAAE").await;
    let uri = format!("/api/play/sessions/{id}/actions");
    let (status, finished) = call(
        app.clone(),
        "POST",
        &uri,
        json!({"version":s["version"],"action":"move","move":0}),
    )
    .await;
    assert_eq!(status, 200, "{finished}");
    let (status, next) = call(
        app.clone(),
        "POST",
        &uri,
        json!({"version":finished["version"],"action":"next_game"}),
    )
    .await;
    assert_eq!(status, 200, "{next}");
    let next = ready(app.clone(), next).await;
    assert_eq!(next["engine"]["plies"], 3);
    let sgf: String = sqlx::query_scalar("SELECT sgf FROM play_sessions WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        sgf.rsplit("(;").next().unwrap().contains("2C 0 1 0.000000"),
        "Next game must retain the captured bot profile: {sgf}"
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/play/sessions/{id}/sgf"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let exported = String::from_utf8(bytes.to_vec()).unwrap();
    let imported = neondojo::import::parse_sgf(&exported).unwrap();
    assert_eq!(imported.players[0], "Test ] \\ name");
    assert_eq!(imported.players[1], "GNUbg");
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn native_bot_offers_cube_and_post_crawford_restores_cube_access(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (_, mut s) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    s["phase"] = json!("move");
    s["legal_moves"] = json!([{"notation":"6/2","steps":[{"from":6,"to":2}]}]);
    replace_native(&pool,&s,"(;FF[4]GM[6]MI[length:5][game:0][ws:0][bs:0]PW[Human]PB[GNUbg]RU[Crawford];PL[W]DI[11];PL[W]AE[a:y]AW[f]AB[x][x])","AwAAAAEAAAAAAA:MIGkAAAAAAAE").await;
    let uri = format!("/api/play/sessions/{}/actions", s["id"].as_str().unwrap());
    let (status, offered) = call(
        app.clone(),
        "POST",
        &uri,
        json!({"version":0,"action":"move","move":0}),
    )
    .await;
    assert_eq!(status, 200, "{offered}");
    assert_eq!(offered["phase"], "double_response");
    assert_eq!(offered["history"][1]["notation"], "double");
    assert_eq!(offered["history"][1]["player"], 1);
    let (status, finished) = call(
        app.clone(),
        "POST",
        &uri,
        json!({"version":1,"action":"take"}),
    )
    .await;
    assert_eq!(status, 200, "{finished}");
    assert_eq!(finished["phase"], "game_over");
    assert_eq!(finished["scores"], json!([0, 2]));
    let (_, mut s) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":1,"match_length":5}),
    )
    .await;
    s["phase"] = json!("move");
    s["legal_moves"] = json!([{"notation":"1/off","steps":[{"from":24,"to":26}]}]);
    s["scores"] = json!([4, 0]);
    replace_native(&pool,&s,"(;FF[4]GM[6]MI[length:5][game:0][ws:4][bs:0]PW[Human]PB[GNUbg]RU[Crawford:CrawfordGame];PL[B]DI[61];PL[W]AE[a:y]AW[f][f][f][f][f][f][f][f][f][f][f][f][f][f][f]AB[x];PL[B])","4P8PAAABAAAAAA:8AmnAEAAAAAE").await;
    let uri = format!("/api/play/sessions/{}/actions", s["id"].as_str().unwrap());
    let (status, finished) = call(
        app.clone(),
        "POST",
        &uri,
        json!({"version":0,"action":"move","move":0}),
    )
    .await;
    assert_eq!(status, 200, "{finished}");
    assert_eq!(finished["scores"], json!([4, 2]));
    let (status, mut next) = call(
        app.clone(),
        "POST",
        &uri,
        json!({"version":1,"action":"next_game"}),
    )
    .await;
    assert_eq!(status, 200, "{next}");
    assert_eq!(next["game_no"], 2);
    assert_eq!(next["phase"], "opening_roll");
    next = ready(app.clone(), next).await;
    if next["phase"] == "move" {
        let (status, played) = call(
            app.clone(),
            "POST",
            &uri,
            json!({"version":next["version"],"action":"move","move":0}),
        )
        .await;
        assert_eq!(status, 200, "{played}");
        next = played;
    }
    assert_eq!(next["phase"], "roll");
    assert_eq!(
        next["can_double"], true,
        "Cube must return after the Crawford game: {next}"
    );
    let (status, doubled) = call(
        app,
        "POST",
        &uri,
        json!({"version":next["version"],"action":"double"}),
    )
    .await;
    assert_eq!(status, 200, "{doubled}");
    assert!(
        doubled["history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["notation"] == "double" && h["player"] == 1)
    );
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn bot_resignation_is_declined_before_checkpoint_and_match_can_finish(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (_, mut s) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":1,"match_length":1}),
    )
    .await;
    s["phase"] = json!("move");
    s["legal_moves"] = json!([{"notation":"5/1","steps":[{"from":20,"to":24}]}]);
    replace_native(&pool,&s,"(;FF[4]GM[6]MI[length:1][game:0][ws:0][bs:0]PW[Human]PB[GNUbg];PL[B]DI[11];PL[W]AE[a:y]AW[f][f][f][f][f][f][f][f][f][f][f][f][f][f][f]AB[t];PL[B])","4P8PAAAQAAAAAA:cIkkAAAAAAAE").await;
    let uri = format!("/api/play/sessions/{}/actions", s["id"].as_str().unwrap());
    let (status, next) = call(
        app.clone(),
        "POST",
        &uri,
        json!({"version":0,"action":"move","move":0}),
    )
    .await;
    assert_eq!(status, 200, "{next}");
    assert_eq!(next["phase"], "roll");
    assert_eq!(
        next["history"].as_array().unwrap().len(),
        2,
        "Bot must play through its resignation offer: {next}"
    );
    let (status, rolled) = call(
        app.clone(),
        "POST",
        &uri,
        json!({"version":1,"action":"roll"}),
    )
    .await;
    assert_eq!(status, 200, "{rolled}");
    let (status, finished) = call(
        app,
        "POST",
        &uri,
        json!({"version":2,"action":"move","move":0}),
    )
    .await;
    assert_eq!(status, 200, "{finished}");
    assert_eq!(finished["phase"], "match_over");
    assert_eq!(finished["winner"], 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn explicit_opening_stages_pause_before_roll_and_before_checker_play(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    call(app.clone(), "GET", "/api/settings", Value::Null).await;
    for (seed, expected) in [
        (3, vec![[1, 1], [4, 5]]),
        (8, vec![[6, 4]]),
        (12, vec![[3, 3], [6, 6], [2, 1]]),
    ] {
        let probe = tempfile::tempdir().unwrap();
        let wrapper = probe.path().join("seeded-opening-gnubg");
        use std::os::unix::fs::PermissionsExt;
        let quoted = format!("'{}'", engine().binary().replace('\'', "'\"'\"'"));
        std::fs::write(&wrapper,format!("#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then exec {quoted} \"$@\"; fi\nprintf 'set seed {seed}\\n' > seeded.txt\ncat commands.txt >> seeded.txt\nexec {quoted} -t -q -r -c seeded.txt\n")).unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        sqlx::query("UPDATE app_settings SET gnubg_path=$1")
            .bind(wrapper.to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        for human in [0, 1] {
            let (status, pending) = call(
                app.clone(),
                "POST",
                "/api/play/sessions",
                json!({"play_mode":"competition","human_player":human,"match_length":5}),
            )
            .await;
            assert_eq!(status, 201, "{pending}");
            assert_library_checkpoint(&pool, &app, &pending).await;
            assert_eq!(pending["phase"], "opening_roll");
            let checkpoint: (String, String) =
                sqlx::query_as("SELECT sgf,native_id FROM play_sessions WHERE id=$1")
                    .bind(uuid::Uuid::parse_str(pending["id"].as_str().unwrap()).unwrap())
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(checkpoint, (String::new(), String::new()));
            assert_eq!(pending["position"]["dice"], json!([]));
            assert_eq!(pending["history"], json!([]));
            let exported = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!(
                            "/api/play/sessions/{}/sgf",
                            pending["id"].as_str().unwrap()
                        ))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(exported.status(), 200);
            let bytes = exported.into_body().collect().await.unwrap().to_bytes();
            let sgf = std::str::from_utf8(&bytes).unwrap();
            assert!(sgf.contains("GM[6]"));
            assert!(
                neondojo::import::parse_sgf(sgf)
                    .unwrap_err()
                    .to_string()
                    .contains("no checker-play decisions"),
                "A pregame export is syntactically valid but has no played decisions yet"
            );
            assert_eq!(pending["legal_moves"], json!([]));
            assert_eq!(pending["can_double"], false);
            let uri = format!("/api/play/sessions/{}", pending["id"].as_str().unwrap());
            assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, pending);
            for action in ["roll", "double", "move", "start_opening_move"] {
                assert_eq!(
                    call(
                        app.clone(),
                        "POST",
                        &format!("{uri}/actions"),
                        json!({"version":0,"action":action})
                    )
                    .await
                    .0,
                    422
                );
            }
            let (status, result) = call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":0,"action":"opening_roll"}),
            )
            .await;
            assert_eq!(status, 200, "{result}");
            assert_library_checkpoint(&pool, &app, &result).await;
            assert_eq!(result["phase"], "opening_result");
            assert_eq!(result["version"], 1);
            assert_eq!(result["history"], json!([]));
            assert_eq!(result["legal_moves"], json!([]));
            assert_eq!(result["can_double"], false);
            let opening = &result["opening"];
            let white = opening["white_die"].as_i64().unwrap();
            let black = opening["black_die"].as_i64().unwrap();
            assert_ne!(white, black);
            assert_eq!(opening["winner"], if white > black { 0 } else { 1 });
            assert_eq!(result["position"]["player"], opening["winner"]);
            assert_eq!(
                result["position"]["dice"],
                json!([white.max(black), white.min(black)])
            );
            assert_eq!(
                opening["rolls"],
                json!(expected),
                "Each native tie must reroll both sides"
            );
            let rolls = opening["rolls"].as_array().unwrap();
            assert_eq!(rolls.last().unwrap(), &json!([white, black]));
            for tied in &rolls[..rolls.len() - 1] {
                assert_eq!(tied[0], tied[1]);
            }
            assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, result);
            for action in ["roll", "double", "move", "opening_roll"] {
                assert_eq!(
                    call(
                        app.clone(),
                        "POST",
                        &format!("{uri}/actions"),
                        json!({"version":1,"action":action})
                    )
                    .await
                    .0,
                    422
                );
            }
            let (status, started) = call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":1,"action":"start_opening_move"}),
            )
            .await;
            assert_eq!(status, 200, "{started}");
            assert_eq!(started["version"], 2);
            if opening["winner"] == human {
                assert_eq!(started["phase"], "move");
                assert_eq!(started["position"]["dice"], result["position"]["dice"]);
                assert_eq!(started["history"], json!([]));
                assert!(!started["legal_moves"].as_array().unwrap().is_empty());
                for action in ["roll", "double"] {
                    assert_eq!(
                        call(
                            app.clone(),
                            "POST",
                            &format!("{uri}/actions"),
                            json!({"version":2,"action":action})
                        )
                        .await
                        .0,
                        422
                    );
                }
                let (status, moved) = call(
                    app.clone(),
                    "POST",
                    &format!("{uri}/actions"),
                    json!({"version":2,"action":"move","move":0}),
                )
                .await;
                assert_eq!(status, 200, "{moved}");
                assert_eq!(moved["history"][0]["dice"], result["position"]["dice"]);
                assert_eq!(moved["history"][0]["player"], opening["winner"]);
            } else {
                assert_eq!(started["phase"], "roll");
                assert_eq!(started["history"][0]["player"], opening["winner"]);
                assert_eq!(started["history"][0]["dice"], result["position"]["dice"]);
            }
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn concurrent_opening_actions_are_fenced_and_native_failure_preserves_pending(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (status, pending) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    assert_eq!(status, 201, "{pending}");
    let id = uuid::Uuid::parse_str(pending["id"].as_str().unwrap()).unwrap();
    let uri = format!("/api/play/sessions/{id}");
    let actions = format!("{uri}/actions");
    let row: (String, String, String) =
        sqlx::query_as("SELECT gnubg_path,sgf,native_id FROM play_sessions WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let pinned = pin_engine_path(&pool, id, "/missing/pinned-opening-engine").await;
    let (status, _) = call(
        app.clone(),
        "POST",
        &actions,
        json!({"version":0,"action":"opening_roll"}),
    )
    .await;
    assert_eq!(status, 503);
    // The saved session exposes its pinned engines; compare it unchanged.
    pin_engine_path(&pool, id, &pinned).await;
    assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, pending);
    let checkpoint: (String, String) =
        sqlx::query_as("SELECT sgf,native_id FROM play_sessions WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(checkpoint, (row.1, row.2));
    pin_engine_path(&pool, id, &row.0).await;
    let (a, b) = tokio::join!(
        call(
            app.clone(),
            "POST",
            &actions,
            json!({"version":0,"action":"opening_roll"})
        ),
        call(
            app.clone(),
            "POST",
            &actions,
            json!({"version":0,"action":"opening_roll"})
        )
    );
    let mut codes = vec![a.0, b.0];
    codes.sort();
    assert_eq!(codes, vec![200, 409], "{a:?} {b:?}");
    let result = call(app.clone(), "GET", &uri, Value::Null).await.1;
    assert_eq!(result["phase"], "opening_result");
    assert_eq!(result["version"], 1);
    assert_eq!(result["history"], json!([]));
    let (a, b) = tokio::join!(
        call(
            app.clone(),
            "POST",
            &actions,
            json!({"version":1,"action":"start_opening_move"})
        ),
        call(
            app.clone(),
            "POST",
            &actions,
            json!({"version":1,"action":"start_opening_move"})
        )
    );
    let mut codes = vec![a.0, b.0];
    codes.sort();
    assert_eq!(codes, vec![200, 409], "{a:?} {b:?}");
    let started = call(app, "GET", &uri, Value::Null).await.1;
    assert_eq!(started["version"], 2);
    assert_eq!(started["opening"], result["opening"]);
    assert!(
        started["history"].as_array().unwrap().len() <= 1,
        "Only one opening checker turn may commit"
    );
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn legacy_sessions_without_opening_metadata_keep_their_active_native_turn(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let mut session = seed(
        &pool,
        &app,
        0,
        &"[s]".repeat(15),
        "61",
        "4P8PAAABAAAAAA:MAGnAAAAAAAE",
        "move",
        json!([{"notation":"1/off","steps":[{"from":1,"to":-1}]}]),
    )
    .await;
    let mut board = [0; 26];
    board[1] = 1;
    board[19] = -15;
    session["position"]["board"] = json!(board);
    session["position"]["dice"] = json!([6, 1]);
    session.as_object_mut().unwrap().remove("opening");
    sqlx::query("UPDATE play_sessions SET state=$1 WHERE id=$2")
        .bind(&session)
        .bind(uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let uri = format!("/api/play/sessions/{}", session["id"].as_str().unwrap());
    let loaded = call(app.clone(), "GET", &uri, Value::Null).await.1;
    assert_eq!(loaded["phase"], "move");
    assert!(loaded["opening"].is_null());
    assert!(
        loaded["legal_turns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|turn| {
                turn["steps"] == json!([{"from":1,"to":-1}])
                    && turn["dice"] == json!([6])
                    && turn["move_index"] == 0
            })
    );
    let (status, finished) = call(
        app,
        "POST",
        &format!("{uri}/actions"),
        json!({"version":0,"action":"move","move":0}),
    )
    .await;
    assert_eq!(status, 200, "{finished}");
    assert_eq!(finished["phase"], "game_over");
    assert_eq!(finished["scores"], json!([2, 0]));
    assert!(finished["opening"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn pausing_and_resuming_preserves_the_checkpoint_and_library_identity(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (status, session) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    assert_eq!(status, 201, "{session}");
    let session = ready(app.clone(), session).await;
    let id = session["id"].as_str().unwrap();
    let session_id: uuid::Uuid = id.parse().unwrap();
    let library_id = session["library_match_id"].as_str().unwrap();
    let uri = format!("/api/play/sessions/{id}");
    let action_uri = format!("{uri}/actions");
    let (_, databases) = call(app.clone(), "GET", "/api/databases", Value::Null).await;
    let default = &databases.as_array().unwrap()[0]["id"];
    let (status, copied) = call(
        app.clone(),
        "POST",
        "/api/library/actions",
        json!({"action":"copy","match_ids":[library_id],"target_database_id":default}),
    )
    .await;
    assert_eq!(status, 200, "{copied}");
    let copy_id = copied["results"][0]["created_id"].as_str().unwrap();
    // Pause and resume must work with only the saved checkpoint.
    pin_engine_path(&pool, session_id, "/missing/gnubg").await;
    let checkpoint = || {
        sqlx::query_as::<_, (String, String)>("SELECT sgf,native_id FROM play_sessions WHERE id=$1")
            .bind(session_id)
            .fetch_one(&pool)
    };
    let original_checkpoint = checkpoint().await.unwrap();
    let pause = json!({"version":session["version"],"action":"pause"});
    let (first, second) = tokio::join!(
        call(app.clone(), "POST", &action_uri, pause.clone()),
        call(app.clone(), "POST", &action_uri, pause)
    );
    let ((status, paused), (competing_status, competing)) = if first.0 == 200 {
        (first, second)
    } else {
        (second, first)
    };
    assert_eq!(status, 200, "{paused}");
    assert_eq!(competing_status, 409, "{competing}");
    let database = databases
        .as_array()
        .unwrap()
        .iter()
        .find(|db| db["name"] == "Paused Matches")
        .unwrap();
    assert_eq!(database["is_system"], true);
    assert_eq!(paused["paused"], true);
    for field in [
        "phase",
        "position",
        "history",
        "scores",
        "engine",
        "library_match_id",
    ] {
        assert_eq!(paused[field], session[field], "changed {field}");
    }
    let (_, restored) = call(app.clone(), "GET", &uri, Value::Null).await;
    assert_eq!(restored, paused);
    let database_id = database["id"].as_str().unwrap();
    let (_, paused_library) = call(
        app.clone(),
        "GET",
        &format!("/api/library/matches?database_id={database_id}"),
        Value::Null,
    )
    .await;
    assert_eq!(paused_library["items"][0]["id"], library_id);
    assert_eq!(paused_library["items"][0]["play_paused"], true);
    let (status, error) = call(
        app.clone(),
        "DELETE",
        &format!(
            "/api/databases/{database_id}/matches?version={}",
            database["version"]
        ),
        Value::Null,
    )
    .await;
    assert_eq!(status, 409, "{error}");
    for (action, version) in [
        ("roll", &paused["version"]),
        ("resume", &session["version"]),
    ] {
        let (status, body) = call(
            app.clone(),
            "POST",
            &action_uri,
            json!({"action":action,"version":version}),
        )
        .await;
        assert_eq!(status, 409, "{body}");
    }
    let (status, moved) = call(
        app.clone(), "POST", "/api/library/actions",
        json!({"action":"move","match_ids":[copy_id],"target_database_id":neondojo::play::BOT_DATABASE_ID}),
    ).await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(moved["results"][0]["status"], "moved");
    let (status, resumed) = call(
        app.clone(),
        "POST",
        &action_uri,
        json!({"version":paused["version"],"action":"resume"}),
    )
    .await;
    assert_eq!(status, 200, "{resumed}");
    assert_eq!(resumed["paused"], false);
    assert_eq!(resumed["position"], session["position"]);
    assert_eq!(resumed["library_match_id"], library_id);
    assert_eq!(checkpoint().await.unwrap(), original_checkpoint);
    let (_, row) = call(
        app,
        "GET",
        &format!("/api/matches/{library_id}/metadata"),
        Value::Null,
    )
    .await;
    assert_eq!(&row["database_id"], default);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn bot_sessions_are_library_matches_from_creation(pool: PgPool) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("neondojo=debug")
        .try_init();
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (status, session) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    assert_eq!(status, 201, "{session}");
    let (_, library) = call(
        app.clone(),
        "GET",
        "/api/library/matches?database_id=00000000-0000-0000-0000-000000000003",
        Value::Null,
    )
    .await;
    assert_eq!(library["items"].as_array().unwrap().len(), 1, "{library}");
    let row = &library["items"][0];
    assert_eq!(row["play_session_id"], session["id"]);
    assert_eq!(row["play_phase"], "opening_roll");
    assert_eq!(row["result_state"], "ongoing");
    assert_eq!(row["position_count"], 0);
    // The human side is a NeonDojo player from the start; the bot is none.
    let aliases: Vec<(String, String)> =
        sqlx::query_as("SELECT platform,name FROM player_aliases ORDER BY platform,name")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        aliases,
        [(
            "neondojo".to_owned(),
            session["human_name"].as_str().unwrap().to_owned()
        )]
    );
    let catalogued: i64 = sqlx::query_scalar("SELECT count(*) FROM match_players")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(catalogued, 1);
    let match_id = row["id"].as_str().unwrap();
    for method in ["GET", "POST"] {
        let (status, body) = call(
            app.clone(),
            method,
            &format!("/api/matches/{match_id}/analysis"),
            json!({"plies":1}),
        )
        .await;
        assert_eq!(status, 422, "{body}");
        assert!(
            body["error"].as_str().unwrap().contains("competition"),
            "{body}"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn bot_library_preserves_identity_management_and_detaches_deleted_matches(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (status, mut session) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    assert_eq!(status, 201, "{session}");
    let session_id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
    let match_id: uuid::Uuid =
        sqlx::query_scalar("SELECT library_match_id FROM play_sessions WHERE id=$1")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let (_, target) = call(
        app.clone(),
        "POST",
        "/api/databases",
        json!({"name":"My bot selection"}),
    )
    .await;
    let target_id = target["id"].as_str().unwrap();
    let (status, copied) = call(app.clone(), "POST", "/api/library/actions", json!({"action":"copy","match_ids":[match_id],"target_database_id":"00000000-0000-0000-0000-000000000001"})).await;
    assert_eq!(status, 200, "{copied}");
    let copy_id = copied["results"][0]["created_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, copy) = call(
        app.clone(),
        "GET",
        &format!("/api/matches/{copy_id}/metadata"),
        Value::Null,
    )
    .await;
    assert!(copy["play_session_id"].is_null());
    let (status, moved) = call(
        app.clone(),
        "POST",
        "/api/library/actions",
        json!({"action":"move","match_ids":[match_id],"target_database_id":target_id}),
    )
    .await;
    assert_eq!(status, 200, "{moved}");
    sqlx::query("UPDATE matches SET display_name='My named match',metadata_version=5 WHERE id=$1")
        .bind(match_id)
        .execute(&pool)
        .await
        .unwrap();
    session = ready(app.clone(), session).await;
    for _ in 0..3 {
        let action = match session["phase"].as_str().unwrap() {
            "roll" => "roll",
            "move" if session["legal_moves"].as_array().unwrap().is_empty() => "pass",
            "move" => "move",
            "double_response" => "take",
            phase => panic!("Unexpected {phase}"),
        };
        let mut body = json!({"version":session["version"],"action":action});
        if action == "move" {
            body["move"] = json!(0);
        }
        let (status, next) = call(
            app.clone(),
            "POST",
            &format!("/api/play/sessions/{session_id}/actions"),
            body,
        )
        .await;
        assert_eq!(status, 200, "{next}");
        session = next;
    }
    let positions: Vec<(uuid::Uuid, i32)> =
        sqlx::query_as("SELECT id,ordinal FROM positions WHERE match_uuid=$1 ORDER BY ordinal")
            .bind(match_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(!positions.is_empty());
    neondojo::move_catalog::ensure_catalog(&pool, match_id)
        .await
        .unwrap();
    let moves: Vec<(uuid::Uuid, i32)> =
        sqlx::query_as("SELECT id,move_no FROM match_moves WHERE match_uuid=$1 ORDER BY move_no")
            .bind(match_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(!moves.is_empty());
    let first = positions[0].0;
    sqlx::query("INSERT INTO annotations(id,position_uuid,tags,comment,version) VALUES($1,$2,ARRAY['kept'],'my note',1)").bind(uuid::Uuid::new_v4()).bind(first).execute(&pool).await.unwrap();
    let job = neondojo::store::create_match_job(&pool, match_id, 1, 1)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET status='completed' WHERE id=$1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let queued = neondojo::store::create_match_job(&pool, match_id, 1, 1)
        .await
        .unwrap();
    // Advance through a checker action so the source really grows.
    for _ in 0..3 {
        let action = if session["phase"] == "roll" {
            "roll"
        } else if session["phase"] == "double_response" {
            "take"
        } else if session["legal_moves"].as_array().unwrap().is_empty() {
            "pass"
        } else {
            "move"
        };
        let mut body = json!({"version":session["version"],"action":action});
        if action == "move" {
            body["move"] = json!(0);
        }
        let (status, next) = call(
            app.clone(),
            "POST",
            &format!("/api/play/sessions/{session_id}/actions"),
            body,
        )
        .await;
        assert_eq!(status, 200, "{next}");
        session = next;
        if action == "move" || action == "pass" {
            break;
        }
    }
    neondojo::move_catalog::ensure_catalog(&pool, match_id)
        .await
        .unwrap();
    let now: Vec<(uuid::Uuid, i32)> =
        sqlx::query_as("SELECT id,ordinal FROM positions WHERE match_uuid=$1 ORDER BY ordinal")
            .bind(match_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(&now[..positions.len()], positions.as_slice());
    let now_moves: Vec<(uuid::Uuid, i32)> =
        sqlx::query_as("SELECT id,move_no FROM match_moves WHERE match_uuid=$1 ORDER BY move_no")
            .bind(match_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(&now_moves[..moves.len()], moves.as_slice());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT comment FROM annotations WHERE position_uuid=$1")
            .bind(first)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "my note"
    );
    let (_, metadata) = call(
        app.clone(),
        "GET",
        &format!("/api/matches/{match_id}/metadata"),
        Value::Null,
    )
    .await;
    assert_eq!(metadata["database_id"], target_id);
    assert_eq!(metadata["display_name"], "My named match");
    assert!(metadata["analysis_status"].is_null(), "{metadata}");
    assert!(
        neondojo::store::latest_match_job(&pool, match_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        neondojo::store::get_job(&pool, job.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "completed"
    );
    let (_, stale_copy) = call(app.clone(), "POST", "/api/library/actions", json!({"action":"copy","match_ids":[match_id],"target_database_id":"00000000-0000-0000-0000-000000000001"})).await;
    let stale_copy_id = stale_copy["results"][0]["created_id"].as_str().unwrap();
    let (_, stale_metadata) = call(
        app.clone(),
        "GET",
        &format!("/api/matches/{stale_copy_id}/metadata"),
        Value::Null,
    )
    .await;
    assert!(
        stale_metadata["analysis_status"].is_null(),
        "Copied historical analysis must stay stale: {stale_metadata}"
    );
    assert_eq!(
        neondojo::store::get_job(&pool, queued.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "failed"
    );
    let events = neondojo::store::events(&pool, queued.id, 0).await.unwrap();
    assert_eq!(events.last().unwrap().event, "failed");
    let (status, _) = call(
        app.clone(),
        "GET",
        &format!("/api/jobs/{}/statistics", job.id),
        Value::Null,
    )
    .await;
    assert_eq!(
        status, 422,
        "Historical statistics must not claim current source coverage"
    );
    let before: Value = sqlx::query_scalar("SELECT state FROM play_sessions WHERE id=$1")
        .bind(session_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        neondojo::play::backfill_library_matches(&pool)
            .await
            .unwrap(),
        0
    );
    let (status, deleted) = call(
        app.clone(),
        "POST",
        "/api/library/actions",
        json!({"action":"delete","match_ids":[match_id]}),
    )
    .await;
    assert_eq!(status, 200, "{deleted}");
    assert_eq!(deleted["results"][0]["status"], "deleted");
    assert_eq!(
        neondojo::play::backfill_library_matches(&pool)
            .await
            .unwrap(),
        0
    );
    let after: Value = sqlx::query_scalar("SELECT state FROM play_sessions WHERE id=$1")
        .bind(session_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let (link, archived): (Option<uuid::Uuid>, bool) =
        sqlx::query_as("SELECT library_match_id,library_archived FROM play_sessions WHERE id=$1")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(link, None);
    assert!(archived);
    let action = if session["phase"] == "roll" {
        "roll"
    } else if session["phase"] == "double_response" {
        "take"
    } else if session["legal_moves"].as_array().unwrap().is_empty() {
        "pass"
    } else {
        "move"
    };
    let mut body = json!({"version":session["version"],"action":action});
    if action == "move" {
        body["move"] = json!(0);
    }
    let (status, next) = call(
        app,
        "POST",
        &format!("/api/play/sessions/{session_id}/actions"),
        body,
    )
    .await;
    assert_eq!(status, 200, "{next}");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM matches WHERE id=$1")
            .bind(match_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn bot_library_backfill_keeps_native_checkpoint_and_revisions(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (status, session) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":1,"match_length":5}),
    )
    .await;
    assert_eq!(status, 201, "{session}");
    let session = ready(app, session).await;
    let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
    sqlx::query(
        "DELETE FROM matches WHERE id=(SELECT library_match_id FROM play_sessions WHERE id=$1)",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE play_sessions SET library_archived=false WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let before: Value = sqlx::query_scalar(
        "SELECT to_jsonb(s)-'library_match_id'-'library_archived' FROM play_sessions s WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        neondojo::play::backfill_library_matches(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        neondojo::play::backfill_library_matches(&pool)
            .await
            .unwrap(),
        0
    );
    let after: Value = sqlx::query_scalar(
        "SELECT to_jsonb(s)-'library_match_id'-'library_archived' FROM play_sessions s WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before, after);
    let count:i32=sqlx::query_scalar("SELECT position_count FROM matches WHERE id=(SELECT library_match_id FROM play_sessions WHERE id=$1)").bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(
        count as usize,
        session["history"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["dice"].as_array().unwrap().len() == 2)
            .count()
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn bot_library_rejects_rewriting_existing_decisions_atomically(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (_, session) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    let mut session = ready(app.clone(), session).await;
    let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
    if session["phase"] == "move" {
        let (status, next) = call(
            app.clone(),
            "POST",
            &format!("/api/play/sessions/{id}/actions"),
            json!({"version":session["version"],"action":"move","move":0}),
        )
        .await;
        assert_eq!(status, 200, "{next}");
        session = next;
    }
    let changed=sqlx::query("UPDATE positions SET notation='tampered' WHERE match_uuid=(SELECT library_match_id FROM play_sessions WHERE id=$1)").bind(id).execute(&pool).await.unwrap().rows_affected();
    assert!(changed > 0);
    let (status, _) = call(
        app.clone(),
        "POST",
        &format!("/api/play/sessions/{id}/actions"),
        json!({"version":session["version"],"action":"roll"}),
    )
    .await;
    assert_eq!(status, 500);
    assert_eq!(
        call(app, "GET", &format!("/api/play/sessions/{id}"), Value::Null)
            .await
            .1,
        session
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn bot_analysis_submission_uses_game_catalog_after_waiting_for_source_lock(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (status, session) = call(
        app,
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    assert_eq!(status, 201, "{session}");
    let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
    let match_id: uuid::Uuid =
        sqlx::query_scalar("SELECT library_match_id FROM play_sessions WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut growth = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM matches WHERE id=$1 FOR UPDATE")
        .bind(match_id)
        .execute(&mut *growth)
        .await
        .unwrap();
    sqlx::query("INSERT INTO match_games(match_uuid,game_no,position_count,white_score,black_score,crawford) VALUES($1,2,0,1,0,false)").bind(match_id).execute(&mut *growth).await.unwrap();
    sqlx::query("UPDATE matches SET sgf=sgf||'(;GM[6]FF[4]PW[Human]PB[GNUbg]MI[length:5][game:1][ws:1][bs:0])',source_revision=source_revision+1 WHERE id=$1").bind(match_id).execute(&mut *growth).await.unwrap();
    let task_pool = pool.clone();
    let submission =
        tokio::spawn(
            async move { neondojo::store::create_match_job(&task_pool, match_id, 1, 1).await },
        );
    tokio::task::yield_now().await;
    growth.commit().await.unwrap();
    let job = submission.await.unwrap().unwrap();
    assert_eq!(
        job.total_games, 2,
        "Preflight count 1 must not be stamped with revision of a two-game source"
    );
    let revision: i64 = sqlx::query_scalar("SELECT source_revision FROM jobs WHERE id=$1")
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(revision, 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn native_play_history_snapshots_follow_checkpoint_and_survive_library_deletion(
    pool: PgPool,
) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (status, initial) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":1,"match_length":5}),
    )
    .await;
    assert_eq!(status, 201, "{initial}");
    let mut session = ready(app.clone(), initial).await;
    let uri = format!("/api/play/sessions/{}", session["id"].as_str().unwrap());
    for _ in 0..8 {
        let (status, snapshots) = call(
            app.clone(),
            "GET",
            &format!("{uri}/history?version={}", session["version"]),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{snapshots}");
        let items = snapshots["items"].as_array().unwrap();
        let records = session["history"].as_array().unwrap();
        assert_eq!(items.len(), records.len());
        for (item, record) in items.iter().zip(records) {
            assert_eq!(item["before"]["player"], record["player"]);
            assert_eq!(item["before"]["dice"], record["dice"]);
            assert_eq!(item["after"]["dice"], json!([]));
        }
        if let Some(last) = items.last() {
            assert_eq!(last["after"]["board"], session["position"]["board"]);
            assert_eq!(
                last["after"]["position_id"],
                session["position"]["position_id"]
            );
        }
        let action = match session["phase"].as_str().unwrap() {
            "roll" => "roll",
            "move" if session["legal_moves"].as_array().unwrap().is_empty() => "pass",
            "move" => "move",
            "double_response" => "take",
            _ => break,
        };
        let mut body = json!({"version":session["version"],"action":action});
        if action == "move" {
            body["move"] = json!(0);
        }
        let (status, next) = call(app.clone(), "POST", &format!("{uri}/actions"), body).await;
        assert_eq!(status, 200, "{next}");
        session = next;
    }
    let history_uri = format!("{uri}/history?version={}", session["version"]);
    let (status, before) = call(app.clone(), "GET", &history_uri, Value::Null).await;
    assert_eq!(status, 200, "{before}");
    let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
    sqlx::query(
        "DELETE FROM matches WHERE id=(SELECT library_match_id FROM play_sessions WHERE id=$1)",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let checkpoint: Value =
        sqlx::query_scalar("SELECT to_jsonb(s) FROM play_sessions s WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let (status, after) = call(app, "GET", &history_uri, Value::Null).await;
    assert_eq!(status, 200, "{after}");
    assert_eq!(before, after);
    let unchanged: Value =
        sqlx::query_scalar("SELECT to_jsonb(s) FROM play_sessions s WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(checkpoint, unchanged);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn coach_commits_checker_feedback_and_restores_it_after_reload(pool: PgPool) {
    let (app, profile) = profile_support::initialized(&pool, true).await;
    let mut config = profile["current_configuration"].clone();
    let thresholds = json!({"questionable":0.01,"error":0.03,"blunder":0.06,"large_blunder":0.15});
    config["analysis"]["error_thresholds"] = thresholds.clone();
    let (status, saved) = call(
        app.clone(),
        "PUT",
        "/api/configuration",
        json!({"expected_revision":profile["revision"],"configuration":config}),
    )
    .await;
    assert_eq!(status, 200, "{saved}");
    let (status, initial) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"human_player":0,"match_length":5}),
    )
    .await;
    assert_eq!(status, 201, "{initial}");
    assert_eq!(initial["play_mode"], "coach");
    assert_eq!(initial["tutor_threshold"], 0.03);
    assert_eq!(initial["error_thresholds"], thresholds);
    assert_eq!(initial["analysis_plies"], 3);
    assert_eq!(initial["engine"]["plies"], 1);
    let mut session = ready(app.clone(), initial).await;
    let uri = format!("/api/play/sessions/{}", session["id"].as_str().unwrap());
    if session["phase"] == "roll" {
        session = call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":session["version"],"action":"roll"}),
        )
        .await
        .1;
        if !session["pending_decision"].is_null() {
            let p = &session["pending_decision"];
            session = call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":session["version"],"action":"roll","decision_id":p["id"]}),
            )
            .await
            .1;
        }
    }
    assert_eq!(session["phase"], "move", "{session}");
    let checkpoint = session.clone();
    let action = if session["legal_moves"].as_array().unwrap().is_empty() {
        "pass"
    } else {
        "move"
    };
    let mut request = json!({"version":session["version"],"action":action});
    if action == "move" {
        request["move"] = json!(0);
    }
    let (status, first) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        request.clone(),
    )
    .await;
    assert_eq!(status, 200, "{first}");
    let staged = !first["pending_decision"].is_null();
    let committed = if staged {
        assert_eq!(first["position"], checkpoint["position"]);
        assert_eq!(first["history"], checkpoint["history"]);
        assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, first);
        let mut confirmation = request.clone();
        confirmation["version"] = first["version"].clone();
        confirmation["decision_id"] = first["pending_decision"]["id"].clone();
        let (status, confirmed) =
            call(app.clone(), "POST", &format!("{uri}/actions"), confirmation).await;
        assert_eq!(status, 200, "{confirmed}");
        confirmed
    } else {
        first
    };
    assert!(committed["pending_decision"].is_null());
    assert!(
        committed["history"].as_array().unwrap().len()
            > checkpoint["history"].as_array().unwrap().len()
    );
    let decision = committed["decisions"].as_array().unwrap().last().unwrap();
    assert_eq!(
        staged,
        action == "move"
            && decision["loss"].as_f64().unwrap() >= thresholds["error"].as_f64().unwrap()
    );
    assert_eq!(decision["action"], action);
    assert_eq!(decision["committed"], true);
    assert_eq!(decision["revealed"], true);
    assert_eq!(decision["attempts"][0]["assisted"], false);
    assert!(
        decision["alternatives"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["equity"].is_null() && a["probabilities"].is_null())
    );
    assert_eq!(
        call(app.clone(), "GET", &uri, Value::Null).await.1,
        committed
    );
    assert_eq!(
        call(app, "POST", &format!("{uri}/actions"), request)
            .await
            .0,
        409
    );
    let match_id: uuid::Uuid =
        sqlx::query_scalar("SELECT library_match_id FROM play_sessions WHERE id=$1")
            .bind(uuid::Uuid::parse_str(committed["id"].as_str().unwrap()).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (_, metadata) = call(
        app.clone(),
        "GET",
        &format!("/api/matches/{match_id}/metadata"),
        Value::Null,
    )
    .await;
    assert_eq!(metadata["play_mode"], "coach");
    assert_eq!(metadata["play_assisted"], true);
    let (_,copy)=call(app.clone(),"POST","/api/library/actions",json!({"action":"copy","match_ids":[match_id],"target_database_id":"00000000-0000-0000-0000-000000000001"})).await;
    let (_, metadata) = call(
        app,
        "GET",
        &format!(
            "/api/matches/{}/metadata",
            copy["results"][0]["created_id"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await;
    assert_eq!(metadata["play_mode"], "coach");
    assert_eq!(metadata["play_assisted"], true);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn routine_no_double_rolls_without_acknowledgment(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    for mode in ["coach", "training", "tutor"] {
        let native_id = "4P8PAAABAAAAAA:MAGgAAAAAAAE";
        let mut session = seed(
            &pool,
            &app,
            0,
            &"[s]".repeat(15),
            "00",
            native_id,
            "roll",
            json!([]),
        )
        .await;
        session["play_mode"] = json!(mode);
        session["analysis_plies"] = json!(1);
        session["position"]["position_id"] = json!("4P8PAAABAAAAAA");
        session["position"]["gnubg_match_id"] = json!("MAGgAAAAAAAE");
        replace_native(
            &pool,
            &session,
            &format!("(;FF[4]GM[6]MI[length:5][game:0][ws:0][bs:0]PW[Human]PB[GNUbg]RU[Crawford];PL[W];PL[W]AE[a:y]AW[a]AB{})", "[s]".repeat(15)),
            native_id,
        ).await;
        let uri = format!(
            "/api/play/sessions/{}/actions",
            session["id"].as_str().unwrap()
        );
        let (status, rolled) = call(
            app.clone(),
            "POST",
            &uri,
            json!({"version":0,"action":"roll"}),
        )
        .await;
        assert_eq!(status, 200, "{mode}: {rolled}");
        assert_eq!(rolled["phase"], "move", "{mode}: {rolled}");
        assert!(rolled["pending_decision"].is_null());
        assert_eq!(rolled["position"]["dice"].as_array().unwrap().len(), 2);
        assert_eq!(rolled["decisions"][0]["action"], "roll");
        assert_eq!(rolled["decisions"][0]["committed"], true);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn feedback_cube_responses_preserve_native_units_for_both_colors(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    for human in [0, 1] {
        for action in ["take", "drop"] {
            let (_, mut state) = call(
                app.clone(),
                "POST",
                "/api/play/sessions",
                json!({"play_mode":"coach","human_player":human,"match_length":5}),
            )
            .await;
            state["analysis_plies"] = json!(1);
            state["phase"] = json!("double_response");
            // MatchID's move, turn and doubled fields come from GNUbg's ID;
            // toggle the absolute color fields to mirror the offered cube.
            use base64::Engine as _;
            let mut bytes = base64::engine::general_purpose::STANDARD_NO_PAD
                .decode("cBGgAAAAAAAE")
                .unwrap();
            if human == 1 {
                bytes[0] ^= 1 << 6;
                bytes[1] ^= 1 << 3;
            }
            let match_id = base64::engine::general_purpose::STANDARD_NO_PAD.encode(bytes);
            state["position"]["position_id"] = json!("4HPwATDgc/ABMA");
            state["position"]["gnubg_match_id"] = json!(match_id);
            let source = format!(
                "(;FF[4]GM[6]MI[length:5][game:0][ws:0][bs:0]PW[Human]PB[GNUbg]RU[Crawford];PL[{}];{}[double])",
                if human == 0 { "B" } else { "W" },
                if human == 0 { "B" } else { "W" }
            );
            replace_native(
                &pool,
                &state,
                &source,
                &format!("4HPwATDgc/ABMA:{match_id}"),
            )
            .await;
            let uri = format!("/api/play/sessions/{}", state["id"].as_str().unwrap());
            let (status, staged) = call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":0,"action":action}),
            )
            .await;
            assert_eq!(status, 200, "human={human} action={action} {staged}");
            assert_eq!(
                staged["pending_decision"]["alternatives"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            assert_eq!(staged["position"], state["position"]);
            assert_eq!(staged["pending_decision"]["original_action"], action);
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn tutor_warns_neutrally_retries_first_attempt_and_native_failure_preserves_pending(
    pool: PgPool,
) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    call(app.clone(), "GET", "/api/settings", Value::Null).await;
    sqlx::query("UPDATE app_settings SET analysis_plies=1")
        .execute(&pool)
        .await
        .unwrap();
    // Use an actual native checker checkpoint with a deliberately poor legal play.
    let (_, initial) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"tutor","tutor_threshold":0.000001,"human_player":0,"match_length":5}),
    )
    .await;
    let mut session = ready(app.clone(), initial).await;
    let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
    let uri = format!("/api/play/sessions/{id}");
    if session["phase"] == "roll" {
        // Bypass no-double feedback only while obtaining the deterministic native dice checkpoint.
        sqlx::query("UPDATE play_sessions SET state=jsonb_set(state,'{play_mode}','\"competition\"') WHERE id=$1").bind(id).execute(&pool).await.unwrap();
        session = call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":session["version"],"action":"roll"}),
        )
        .await
        .1;
        sqlx::query(
            "UPDATE play_sessions SET state=jsonb_set(state,'{play_mode}','\"tutor\"') WHERE id=$1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }
    assert_eq!(session["phase"], "move");
    let last = session["legal_moves"].as_array().unwrap().len() - 1;
    let (status, staged) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":session["version"],"action":"move","move":last}),
    )
    .await;
    assert_eq!(status, 200, "{staged}");
    assert_eq!(staged["position"], session["position"]);
    assert_eq!(staged["history"], session["history"]);
    let p = &staged["pending_decision"];
    assert_eq!(p["warning"], true);
    assert!(p["loss"].is_null());
    assert!(p["first_loss"].is_null());
    assert!(p["alternatives"].as_array().unwrap().is_empty());
    let raw: Value = sqlx::query_scalar("SELECT state FROM play_sessions WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(raw["pending_decision"]["attempts"][0]["assisted"], false);
    let best = raw["pending_decision"]["alternatives"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["loss"] == 0.0)
        .unwrap()["move_index"]
        .clone();
    let snapshot = &raw["engine_snapshots"]["rules"];
    sqlx::query("UPDATE play_sessions SET state=jsonb_set(state,'{engine_snapshots,rules,gnubg_path}',to_jsonb($2::text)) WHERE id=$1")
        .bind(id)
        .bind("/missing/native-feedback")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":staged["version"],"action":"move","move":last,"decision_id":p["id"]})
        )
        .await
        .0,
        503
    );
    let after_failure: Value = sqlx::query_scalar("SELECT state FROM play_sessions WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after_failure["pending_decision"], raw["pending_decision"]);
    assert_eq!(after_failure["version"], raw["version"]);
    sqlx::query(
        "UPDATE play_sessions SET state=jsonb_set(state,'{engine_snapshots,rules}',$2) WHERE id=$1",
    )
    .bind(id)
    .bind(snapshot)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, staged);
    let (status, retry) = call(
        app.clone(),
        "POST",
        &format!("{uri}/feedback"),
        json!({"version":staged["version"],"operation":"retry","decision_id":p["id"]}),
    )
    .await;
    assert_eq!(status, 200, "{retry}");
    assert!(retry["pending_decision"]["action"].is_null());
    let (status, committed) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":retry["version"],"action":"move","move":best}),
    )
    .await;
    assert_eq!(status, 200, "{committed}");
    assert!(committed["pending_decision"].is_null());
    let persisted: Value = sqlx::query_scalar("SELECT state FROM play_sessions WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let decision = persisted["decisions"].as_array().unwrap().last().unwrap();
    assert_eq!(decision["original_move_index"], last);
    assert_eq!(decision["move_index"], best);
    assert_eq!(decision["attempts"][0]["assisted"], false);
    assert_eq!(decision["attempts"][1]["assisted"], true);
    assert_eq!(decision["retry_count"], 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn completed_competition_review_is_persisted_and_uses_native_statistics(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let session = seed(
        &pool,
        &app,
        3,
        &"[s]".repeat(15),
        "61",
        "4P8PAAABAAAAAA:MAGnADAAAAAE",
        "move",
        json!([{"notation":"1/off","steps":[{"from":1,"to":-1}]}]),
    )
    .await;
    let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
    sqlx::query(
        "UPDATE play_sessions SET state=jsonb_set(state,'{analysis_plies}','1') WHERE id=$1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let uri = format!("/api/play/sessions/{id}");
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &format!("{uri}/review"),
            json!({"version":0})
        )
        .await
        .0,
        422
    );
    let (status, finished) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":0,"action":"move","move":0}),
    )
    .await;
    assert_eq!(status, 200, "{finished}");
    assert_eq!(finished["phase"], "match_over");
    let (status, reviewed) = call(
        app.clone(),
        "POST",
        &format!("{uri}/review"),
        json!({"version":finished["version"]}),
    )
    .await;
    assert_eq!(status, 200, "{reviewed}");
    assert!(reviewed["review"].is_object());
    assert_eq!(reviewed["review"]["analysis_plies"], 1);
    assert_eq!(reviewed["review"]["assisted"], false);
    assert_eq!(
        reviewed["review"]["independent_first_attempt_count"],
        reviewed["review"]["first_attempt_count"]
    );
    assert!(reviewed["review"]["games"][0]["statistics"].is_array());
    let (archive_status, archive) =
        call(app.clone(), "GET", "/api/study/reports", Value::Null).await;
    assert_eq!(archive_status, 200, "{archive}");
    assert_eq!(archive["items"].as_array().unwrap().len(), 1);
    assert_eq!(archive["items"][0]["id"], reviewed["id"]);
    let (report_status, report) = call(
        app.clone(),
        "GET",
        &format!("/api/study/reports/{id}"),
        Value::Null,
    )
    .await;
    assert_eq!(report_status, 200, "{report}");
    assert_eq!(report["review"], reviewed["review"]);
    assert_eq!(report["decisions"], reviewed["decisions"]);
    // Reopening a completed review is idempotent and never creates another archive entry.
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &format!("{uri}/review"),
            json!({"version":reviewed["version"]})
        )
        .await
        .0,
        200
    );
    assert_eq!(
        call(app.clone(), "GET", "/api/study/reports", Value::Null)
            .await
            .1["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call(app.clone(), "GET", &uri, Value::Null).await.1,
        reviewed
    );
    assert_eq!(
        call(
            app,
            "POST",
            &format!("{uri}/review"),
            json!({"version":finished["version"]})
        )
        .await
        .0,
        409
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn training_manual_hint_records_assistance_and_auto_training_reveals_full_feedback(
    pool: PgPool,
) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    call(app.clone(), "GET", "/api/settings", Value::Null).await;
    sqlx::query("UPDATE app_settings SET analysis_plies=1")
        .execute(&pool)
        .await
        .unwrap();
    for automatic in [false, true] {
        let (_,initial)=call(app.clone(),"POST","/api/play/sessions",json!({"play_mode":"training","training_auto_analysis":automatic,"human_player":0,"match_length":5})).await;
        let mut session = ready(app.clone(), initial).await;
        let uri = format!("/api/play/sessions/{}", session["id"].as_str().unwrap());
        if session["phase"] == "roll" {
            let (status, next) = call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":session["version"],"action":"roll"}),
            )
            .await;
            assert_eq!(status, 200, "{next}");
            session = next;
            if automatic && !session["pending_decision"].is_null() {
                let pending = &session["pending_decision"];
                assert!(pending["alternatives"][0]["equity"].is_number());
                assert_eq!(
                    pending["alternatives"][0]["probabilities"]
                        .as_array()
                        .unwrap()
                        .len(),
                    6
                );
                session=call(app.clone(),"POST",&format!("{uri}/actions"),json!({"version":session["version"],"action":"roll","decision_id":pending["id"]})).await.1;
            }
        }
        if !automatic {
            let (status, hinted) = call(
                app.clone(),
                "POST",
                &format!("{uri}/feedback"),
                json!({"version":session["version"],"operation":"hint"}),
            )
            .await;
            assert_eq!(status, 200, "{hinted}");
            assert!(hinted["pending_decision"]["action"].is_null());
            assert_eq!(hinted["pending_decision"]["assisted"], true);
            assert!(hinted["pending_decision"]["alternatives"][0]["equity"].is_number());
            session = hinted;
        }
        let (status, next) = call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":session["version"],"action":"move","move":0}),
        )
        .await;
        assert_eq!(status, 200, "{next}");
        if automatic {
            assert_eq!(next["position"], session["position"]);
            assert_eq!(next["pending_decision"]["revealed"], true);
            assert!(next["pending_decision"]["alternatives"][0]["equity"].is_number());
            assert_eq!(next["pending_decision"]["attempts"][0]["assisted"], false);
        } else {
            assert!(next["pending_decision"].is_null());
            assert_ne!(next["history"], session["history"]);
            let decision = next["decisions"].as_array().unwrap().last().unwrap();
            assert_eq!(decision["attempts"][0]["assisted"], true);
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn competition_position_analysis_and_existing_job_reads_are_blocked_until_complete(
    pool: PgPool,
) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (_, session) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    let mut session = ready(app.clone(), session).await;
    let uri = format!(
        "/api/play/sessions/{}/actions",
        session["id"].as_str().unwrap()
    );
    if session["phase"] == "roll" {
        session = call(
            app.clone(),
            "POST",
            &uri,
            json!({"version":session["version"],"action":"roll"}),
        )
        .await
        .1;
    }
    let (_, session) = call(
        app.clone(),
        "POST",
        &uri,
        json!({"version":session["version"],"action":"move","move":0}),
    )
    .await;
    let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
    let position:uuid::Uuid=sqlx::query_scalar("SELECT p.id FROM positions p JOIN play_sessions s ON s.library_match_id=p.match_uuid WHERE s.id=$1 ORDER BY ordinal LIMIT 1").bind(id).fetch_one(&pool).await.unwrap();
    for kind in ["analysis", "dice"] {
        let (status, result) = call(
            app.clone(),
            "POST",
            "/api/jobs",
            json!({"position_id":position,"kind":kind,"plies":1}),
        )
        .await;
        assert_eq!(status, 422, "{result}");
        assert!(result["error"].as_str().unwrap().contains("competition"));
    }
    let match_id: uuid::Uuid = sqlx::query_scalar("SELECT match_uuid FROM positions WHERE id=$1")
        .bind(position)
        .fetch_one(&pool)
        .await
        .unwrap();
    for path in [
        format!("/api/positions/{position}"),
        format!("/api/positions?match_id={match_id}"),
        format!("/api/matches/{match_id}"),
        format!("/api/matches/{match_id}/games"),
        format!("/api/matches/{match_id}/source"),
        format!("/api/matches/{match_id}/dice"),
    ] {
        assert_eq!(
            call(app.clone(), "GET", &path, Value::Null).await.0,
            422,
            "{path}"
        );
    }
    assert_eq!(
        call(
            app.clone(),
            "PUT",
            &format!("/api/positions/{position}/bookmark"),
            Value::Null
        )
        .await
        .0,
        200
    );
    let (status, browser) = call(app.clone(), "GET", "/api/position-browser", Value::Null).await;
    assert_eq!(status, 200, "{browser}");
    assert!(browser["items"].as_array().unwrap().is_empty(), "{browser}");
    neondojo::move_catalog::ensure_catalog(&pool, match_id)
        .await
        .unwrap();
    let move_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM match_moves WHERE match_uuid=$1 ORDER BY move_no LIMIT 1",
    )
    .bind(match_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    for path in [
        format!("/api/positions/{position}/opening-book"),
        format!("/api/matches/{match_id}/move-assessments?game_no=1"),
        format!("/api/matches/{match_id}/games/1/decisions"),
        format!("/api/moves/{move_id}"),
        format!("/api/positions/{position}/imported-analysis"),
    ] {
        assert_eq!(
            call(app.clone(), "GET", &path, Value::Null).await.0,
            422,
            "{path}"
        );
    }
    let position_id: String = sqlx::query_scalar("SELECT position_id FROM positions WHERE id=$1")
        .bind(position)
        .fetch_one(&pool)
        .await
        .unwrap();
    let (status, page) = call(
        app.clone(),
        "GET",
        &format!("/api/positions?position_id={position_id}"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{page}");
    assert!(page["items"].as_array().unwrap().is_empty(), "{page}");
    let job = neondojo::store::create_job(&pool, position, "analysis", 1)
        .await
        .unwrap();
    for path in [
        format!("/api/jobs/{}", job.id),
        format!("/api/jobs/{}/events", job.id),
    ] {
        let (status, result) = call(app.clone(), "GET", &path, Value::Null).await;
        assert_eq!(status, 422, "{result}");
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn changing_play_modes_is_durable_atomic_and_never_advances_native_state(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (_,session)=call(app.clone(),"POST","/api/play/sessions",json!({"play_mode":"competition","human_player":0,"match_length":5,"tutor_threshold":0.123})).await;
    let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
    let uri = format!("/api/play/sessions/{id}");
    let native: (String, String) =
        sqlx::query_as("SELECT sgf,native_id FROM play_sessions WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    pin_engine_path(&pool, id, "/missing/never-invoke-for-mode-switch").await;
    let mut current = session.clone();
    for mode in ["coach", "tutor", "training", "competition"] {
        let (status, next) = call(
            app.clone(),
            "POST",
            &format!("{uri}/mode"),
            json!({"version":current["version"],"play_mode":mode}),
        )
        .await;
        assert_eq!(status, 200, "{next}");
        assert_eq!(next["play_mode"], mode);
        assert_eq!(next["learning_used"], true);
        for field in [
            "position",
            "phase",
            "history",
            "opening",
            "scores",
            "engine",
            "analysis_plies",
            "tutor_threshold",
            "training_auto_analysis",
        ] {
            assert_eq!(next[field], session[field], "{field}");
        }
        assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, next);
        assert_eq!(
            call(
                app.clone(),
                "POST",
                &format!("{uri}/mode"),
                json!({"version":current["version"],"play_mode":mode})
            )
            .await
            .0,
            409
        );
        current = next;
    }
    assert_eq!(
        sqlx::query_as::<_, (String, String)>(
            "SELECT sgf,native_id FROM play_sessions WHERE id=$1"
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        native
    );
    let match_id: uuid::Uuid =
        sqlx::query_scalar("SELECT library_match_id FROM play_sessions WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let (_, metadata) = call(
        app.clone(),
        "GET",
        &format!("/api/matches/{match_id}/metadata"),
        Value::Null,
    )
    .await;
    assert_eq!(metadata["play_mode"], "competition");
    assert_eq!(metadata["play_assisted"], true);
    // Inter-game rest remains switchable; completed matches remain immutable.
    sqlx::query(
        "UPDATE play_sessions SET state=jsonb_set(state,'{phase}','\"game_over\"') WHERE id=$1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let (status, next) = call(
        app.clone(),
        "POST",
        &format!("{uri}/mode"),
        json!({"version":current["version"],"play_mode":"coach"}),
    )
    .await;
    assert_eq!(status, 200, "{next}");
    sqlx::query(
        "UPDATE play_sessions SET state=jsonb_set(state,'{phase}','\"match_over\"') WHERE id=$1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        call(
            app,
            "POST",
            &format!("{uri}/mode"),
            json!({"version":next["version"],"play_mode":"training"})
        )
        .await
        .0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn mode_switch_preserves_tutor_attempts_reveal_and_competition_retry_choice(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    call(app.clone(), "GET", "/api/settings", Value::Null).await;
    sqlx::query("UPDATE app_settings SET analysis_plies=1")
        .execute(&pool)
        .await
        .unwrap();
    let (_,initial)=call(app.clone(),"POST","/api/play/sessions",json!({"play_mode":"competition","human_player":0,"match_length":5,"tutor_threshold":0.000001})).await;
    let mut session = ready(app.clone(), initial).await;
    let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
    let uri = format!("/api/play/sessions/{id}");
    if session["phase"] == "roll" {
        session = call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":session["version"],"action":"roll"}),
        )
        .await
        .1;
    }
    let (_, tutor) = call(
        app.clone(),
        "POST",
        &format!("{uri}/mode"),
        json!({"version":session["version"],"play_mode":"tutor"}),
    )
    .await;
    let last = tutor["legal_moves"].as_array().unwrap().len() - 1;
    let (status, warning) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":tutor["version"],"action":"move","move":last}),
    )
    .await;
    assert_eq!(status, 200, "{warning}");
    assert_eq!(warning["pending_decision"]["warning"], true);
    assert!(warning["pending_decision"]["loss"].is_null());
    let decision_id = warning["pending_decision"]["id"].clone();
    let (_, competition) = call(
        app.clone(),
        "POST",
        &format!("{uri}/mode"),
        json!({"version":warning["version"],"play_mode":"competition"}),
    )
    .await;
    assert_eq!(competition["pending_decision"]["action"], "move");
    assert!(
        competition["pending_decision"]["alternatives"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(competition["pending_decision"]["id"], decision_id);
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":competition["version"],"action":"move","move":last})
        )
        .await
        .0,
        422
    );
    let (_, training) = call(
        app.clone(),
        "POST",
        &format!("{uri}/mode"),
        json!({"version":competition["version"],"play_mode":"training"}),
    )
    .await;
    assert_eq!(training["pending_decision"]["revealed"], true);
    assert!(training["pending_decision"]["loss"].is_number());
    let best = training["pending_decision"]["alternatives"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["loss"] == 0.0)
        .unwrap()["move_index"]
        .clone();
    let (_, tutor) = call(
        app.clone(),
        "POST",
        &format!("{uri}/mode"),
        json!({"version":training["version"],"play_mode":"tutor"}),
    )
    .await;
    assert_eq!(tutor["pending_decision"]["revealed"], true);
    assert_eq!(tutor["pending_decision"]["warning"], true);
    let (_, retry) = call(
        app.clone(),
        "POST",
        &format!("{uri}/feedback"),
        json!({"version":tutor["version"],"operation":"retry","decision_id":decision_id}),
    )
    .await;
    let (_, competition) = call(
        app.clone(),
        "POST",
        &format!("{uri}/mode"),
        json!({"version":retry["version"],"play_mode":"competition"}),
    )
    .await;
    assert!(competition["pending_decision"]["action"].is_null());
    let (status, committed) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":competition["version"],"action":"move","move":best}),
    )
    .await;
    assert_eq!(status, 200, "{committed}");
    assert!(committed["pending_decision"].is_null());
    let raw: Value = sqlx::query_scalar("SELECT state FROM play_sessions WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let decision = raw["decisions"].as_array().unwrap().last().unwrap();
    assert_eq!(decision["action"], "move");
    assert_eq!(decision["move_index"], best);
    assert_eq!(decision["original_move_index"], last);
    assert_eq!(decision["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(decision["attempts"][0]["assisted"], false);
    assert_eq!(decision["attempts"][1]["assisted"], true);
    assert_eq!(raw["learning_used"], true);
    assert_eq!(raw["play_mode"], "competition");
    assert_eq!(call(app, "GET", &uri, Value::Null).await.1, committed);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn completed_mixed_mode_review_keeps_competition_play_and_original_learning_choice(
    pool: PgPool,
) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    call(app.clone(), "GET", "/api/settings", Value::Null).await;
    sqlx::query("UPDATE app_settings SET analysis_plies=1")
        .execute(&pool)
        .await
        .unwrap();
    let (_, initial) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":1}),
    )
    .await;
    let mut session = ready(app.clone(), initial).await;
    let uri = format!("/api/play/sessions/{}", session["id"].as_str().unwrap());
    let mut competition_checker_played = false;
    for _ in 0..100 {
        if competition_checker_played
            && session["phase"] == "move"
            && session["legal_moves"].as_array().unwrap().len() > 1
        {
            break;
        }
        let action = match session["phase"].as_str().unwrap() {
            "roll" => "roll",
            "move" if session["legal_moves"].as_array().unwrap().is_empty() => "pass",
            "move" => "move",
            other => panic!("Unexpected early phase {other}"),
        };
        let mut body = json!({"version":session["version"],"action":action});
        if action == "move" {
            body["move"] = json!(0);
            competition_checker_played = true;
        }
        let (status, next) = call(app.clone(), "POST", &format!("{uri}/actions"), body).await;
        assert_eq!(status, 200, "{next}");
        session = next;
    }
    assert!(competition_checker_played);
    assert_eq!(session["phase"], "move");
    assert!(session["legal_moves"].as_array().unwrap().len() > 1);
    let (_, training) = call(
        app.clone(),
        "POST",
        &format!("{uri}/mode"),
        json!({"version":session["version"],"play_mode":"training"}),
    )
    .await;
    let last = training["legal_moves"].as_array().unwrap().len() - 1;
    let (status, pending) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":training["version"],"action":"move","move":last}),
    )
    .await;
    assert_eq!(status, 200, "{pending}");
    assert!(pending["pending_decision"]["first_loss"].as_f64().unwrap() > 0.0);
    let best = pending["pending_decision"]["alternatives"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["loss"] == 0.0)
        .unwrap()["move_index"]
        .clone();
    let decision_id = pending["pending_decision"]["id"].clone();
    let (_, retry) = call(
        app.clone(),
        "POST",
        &format!("{uri}/feedback"),
        json!({"version":pending["version"],"operation":"retry","decision_id":decision_id}),
    )
    .await;
    let (_, competition) = call(
        app.clone(),
        "POST",
        &format!("{uri}/mode"),
        json!({"version":retry["version"],"play_mode":"competition"}),
    )
    .await;
    let (status, next) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":competition["version"],"action":"move","move":best}),
    )
    .await;
    assert_eq!(status, 200, "{next}");
    session = next;
    for _ in 0..800 {
        if session["phase"] == "match_over" {
            break;
        }
        let action = match session["phase"].as_str().unwrap() {
            "roll" => "roll",
            "move" if session["legal_moves"].as_array().unwrap().is_empty() => "pass",
            "move" => "move",
            "double_response" => "take",
            other => panic!("Unexpected phase {other}"),
        };
        let mut body = json!({"version":session["version"],"action":action});
        if action == "move" {
            body["move"] = json!(0);
        }
        let (status, next) = call(app.clone(), "POST", &format!("{uri}/actions"), body).await;
        assert_eq!(status, 200, "{next}");
        session = next;
    }
    assert_eq!(session["phase"], "match_over");
    assert_eq!(session["play_mode"], "competition");
    assert_eq!(session["learning_used"], true);
    let (status, reviewed) = call(
        app.clone(),
        "POST",
        &format!("{uri}/review"),
        json!({"version":session["version"]}),
    )
    .await;
    assert_eq!(status, 200, "{reviewed}");
    let review = &reviewed["review"];
    let original = &reviewed["decisions"][0];
    let games = review["games"].as_array().unwrap();
    let final_loss: f64 = games
        .iter()
        .flat_map(|g| g["decisions"].as_array().unwrap())
        .filter(|d| d["player"] == 0)
        .map(|d| d["equity_loss"].as_f64().unwrap())
        .sum();
    let native_loss: f64 = games
        .iter()
        .map(|g| {
            g["statistics"][0]["normalized_equity_loss"]
                .as_f64()
                .unwrap()
        })
        .sum();
    let native_count: i64 = games
        .iter()
        .map(|g| {
            g["statistics"][0]["unforced_checker_moves"]
                .as_i64()
                .unwrap()
                + g["statistics"][0]["close_cube_decisions"].as_i64().unwrap()
        })
        .sum();
    let delta = original["first_loss"].as_f64().unwrap() - original["loss"].as_f64().unwrap();
    let denominator = native_count + original["first_rating_decisions"].as_i64().unwrap()
        - original["rating_decisions"].as_i64().unwrap();
    assert!((review["final_choice_loss"].as_f64().unwrap() - final_loss).abs() < 1e-10);
    assert!((review["first_attempt_loss"].as_f64().unwrap() - final_loss - delta).abs() < 1e-10);
    assert_eq!(review["first_attempt_rating_decisions"], denominator);
    assert!(
        (review["first_attempt_rating"].as_f64().unwrap()
            - 500.0 * (native_loss + delta) / denominator as f64)
            .abs()
            < 1e-8
    );
    assert!(review["first_attempt_count"].as_u64().unwrap() > 1);
    assert_eq!(review["assisted"], true);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn mode_responses_report_actual_live_library_link_after_copy_and_deletion(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (_, initial) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"coach","human_player":0,"match_length":5}),
    )
    .await;
    let id = uuid::Uuid::parse_str(initial["id"].as_str().unwrap()).unwrap();
    let match_id: uuid::Uuid =
        sqlx::query_scalar("SELECT library_match_id FROM play_sessions WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let uri = format!("/api/play/sessions/{id}");
    let (status, changed) = call(
        app.clone(),
        "POST",
        &format!("{uri}/mode"),
        json!({"version":initial["version"],"play_mode":"competition"}),
    )
    .await;
    assert_eq!(status, 200, "{changed}");
    assert_eq!(changed["library_match_id"], match_id.to_string());
    assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, changed);
    let (status,copy)=call(app.clone(),"POST","/api/library/actions",json!({"action":"copy","match_ids":[match_id],"target_database_id":"00000000-0000-0000-0000-000000000001"})).await;
    assert_eq!(status, 200, "{copy}");
    let copy_id = copy["results"][0]["created_id"].as_str().unwrap();
    assert_ne!(copy_id, match_id.to_string());
    assert_eq!(
        call(app.clone(), "GET", &uri, Value::Null).await.1["library_match_id"],
        match_id.to_string()
    );
    assert_eq!(
        call(
            app.clone(),
            "POST",
            "/api/library/actions",
            json!({"action":"delete","match_ids":[match_id]})
        )
        .await
        .0,
        200
    );
    // Corrupt the embedded projection deliberately: the relational live link
    // remains authoritative even when old saved JSON contains a stale UUID.
    sqlx::query("UPDATE play_sessions SET state=jsonb_set(state,'{library_match_id}',to_jsonb($2::text)) WHERE id=$1").bind(id).bind(match_id.to_string()).execute(&pool).await.unwrap();
    let loaded = call(app.clone(), "GET", &uri, Value::Null).await.1;
    assert!(loaded["library_match_id"].is_null(), "{loaded}");
    let listed = call(app.clone(), "GET", "/api/play/sessions", Value::Null)
        .await
        .1;
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == initial["id"])
            .unwrap()["library_match_id"]
            .is_null()
    );
    let (status, detached) = call(
        app,
        "POST",
        &format!("{uri}/mode"),
        json!({"version":loaded["version"],"play_mode":"training"}),
    )
    .await;
    assert_eq!(status, 200, "{detached}");
    assert!(detached["library_match_id"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn live_tutor_threshold_rechecks_pending_warning_without_playing_or_changing_defaults(
    pool: PgPool,
) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (_, defaults) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    sqlx::query("UPDATE app_settings SET analysis_plies=1")
        .execute(&pool)
        .await
        .unwrap();
    let (status, initial) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({
            "play_mode":"competition", "human_player":0, "match_length":5, "tutor_threshold":0.04
        }),
    )
    .await;
    assert_eq!(status, 201, "{initial}");
    let mut session = ready(app.clone(), initial).await;
    let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
    let uri = format!("/api/play/sessions/{id}");
    if session["phase"] == "roll" {
        session = call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({
                "version":session["version"], "action":"roll"
            }),
        )
        .await
        .1;
    }
    let (status, next) = call(
        app.clone(),
        "POST",
        &format!("{uri}/mode"),
        json!({
            "version":session["version"], "play_mode":"tutor", "tutor_threshold":0.000001
        }),
    )
    .await;
    assert_eq!(status, 200, "{next}");
    session = next;
    let last = session["legal_moves"].as_array().unwrap().len() - 1;
    let (status, warning) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({
            "version":session["version"], "action":"move", "move":last
        }),
    )
    .await;
    assert_eq!(status, 200, "{warning}");
    assert_eq!(warning["pending_decision"]["warning"], true);
    // Break the pinned engines first: the saved state compared below holds
    // their paths.
    pin_engine_path(&pool, id, "/missing/threshold-must-not-run-native-engine").await;
    let before: (Value, String, String) =
        sqlx::query_as("SELECT state,sgf,native_id FROM play_sessions WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(before.0["pending_decision"]["loss"].as_f64().unwrap() < 1.0);
    let mut current = warning;
    for (threshold, warns) in [(1.0, false), (0.000001, true)] {
        let (status, next) = call(
            app.clone(),
            "POST",
            &format!("{uri}/mode"),
            json!({
                "version":current["version"], "play_mode":"tutor", "tutor_threshold":threshold
            }),
        )
        .await;
        assert_eq!(status, 200, "{next}");
        assert_eq!(next["tutor_threshold"], threshold);
        assert_eq!(next["pending_decision"]["warning"], warns);
        assert_eq!(next["pending_decision"]["revealed"], false);
        assert!(next["pending_decision"]["loss"].is_null());
        assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, next);
        assert_eq!(
            call(
                app.clone(),
                "POST",
                &format!("{uri}/mode"),
                json!({
                    "version":current["version"], "play_mode":"tutor", "tutor_threshold":0.2
                })
            )
            .await
            .0,
            409
        );
        let after: (Value, String, String) =
            sqlx::query_as("SELECT state,sgf,native_id FROM play_sessions WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let mut expected = before.0.clone();
        expected["version"] = next["version"].clone();
        expected["tutor_threshold"] = json!(threshold);
        expected["pending_decision"]["warning"] = json!(warns);
        assert_eq!(after, (expected, before.1.clone(), before.2.clone()));
        current = next;
    }
    for invalid in [0.0, -0.01, 1.00001] {
        let (status, _) = call(
            app.clone(),
            "POST",
            &format!("{uri}/mode"),
            json!({
                "version":current["version"], "play_mode":"coach", "tutor_threshold":invalid
            }),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, current);
    }
    let (_, settings) = call(app, "GET", "/api/settings", Value::Null).await;
    assert_eq!(settings["tutor_threshold"], defaults["tutor_threshold"]);
    assert_eq!(settings["default_play_mode"], defaults["default_play_mode"]);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn native_resign_levels_score_and_continue_current_game(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    for (level, cube, phase) in [
        (1, 1, "roll"),
        (2, 1, "move"),
        (3, 1, "move"),
        (1, 2, "move"),
        (2, 2, "roll"),
        (3, 2, "move"),
    ] {
        let session = seed(
            &pool,
            &app,
            0,
            &"[s]".repeat(15),
            "61",
            "4P8PAAABAAAAAA:MAGnAAAAAAAE",
            "move",
            json!([{"notation":"1/off","steps":[{"from":1,"to":-1}]}]),
        )
        .await;
        let id = uuid::Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
        let mut session = session;
        session["phase"] = json!(phase);
        let mut sgf: String = sqlx::query_scalar("SELECT sgf FROM play_sessions WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let native_id = match (cube, phase) {
            (1, "roll") => "4P8PAAABAAAAAA:MAGgAAAAAAAE",
            (2, "roll") => "4P8PAAABAAAAAA:MQGgAAAAAAAE",
            (2, _) => "4P8PAAABAAAAAA:MQGnAAAAAAAE",
            _ => "4P8PAAABAAAAAA:MAGnAAAAAAAE",
        };
        if phase == "roll" {
            sgf = sgf.replace("DI[61]", "");
        }
        if cube == 2 {
            sgf = sgf.replace(")", ";CV[2])");
        }
        replace_native(&pool, &session, &sgf, native_id).await;
        let uri = format!("/api/play/sessions/{}", session["id"].as_str().unwrap());
        let (status, finished) = call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":0,"action":"resign","resign_level":level}),
        )
        .await;
        assert_eq!(status, 200, "{finished}");
        let points = level * cube;
        assert_eq!(
            finished["phase"],
            if points >= 5 {
                "match_over"
            } else {
                "game_over"
            }
        );
        assert_eq!(
            call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":finished["version"],"action":"end_session"})
            )
            .await
            .0,
            422
        );
        assert_eq!(finished["scores"], json!([0, points]));
        assert_eq!(finished["winner"], 1);
        assert_eq!(finished["history"][0]["notation"], "resign");
        assert_eq!(finished["version"], 1);
        assert_library_checkpoint(&pool, &app, &finished).await;
        let expected = json!({"winner":1,"points":points,"cube":cube,"multiplier":level,"reason":"resignation","scores_after":[0,points]});
        assert_eq!(finished["game_results"][0]["result"], expected);
        let library_id: uuid::Uuid =
            sqlx::query_scalar("SELECT library_match_id FROM play_sessions WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let library_sgf: String = sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1")
            .bind(library_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            library_sgf.contains(&format!("RE[B+{points}R]")),
            "{library_sgf}"
        );
        let games = neondojo::store::match_games(&pool, library_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(serde_json::to_value(&games[0]).unwrap()["result"], expected);
        let (status, history) = call(
            app.clone(),
            "GET",
            &format!("{uri}/history?version=1"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{history}");
        assert_eq!(history["items"][0]["kind"], "resign");
        if points >= 5 {
            let (status, games) = call(
                app.clone(),
                "GET",
                &format!("/api/matches/{library_id}/games"),
                Value::Null,
            )
            .await;
            assert_eq!(status, 200, "{games}");
            assert_eq!(games[0]["result"], expected);
            continue;
        }
        let (status, next) = call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":1,"action":"next_game"}),
        )
        .await;
        assert_eq!(status, 200, "{next}");
        assert_eq!(next["phase"], "opening_roll");
        let continued = ready(app.clone(), next).await;
        assert_eq!(continued["game_no"], 2);
        assert_eq!(continued["scores"], json!([0, points]));
        assert_library_checkpoint(&pool, &app, &continued).await;
    }
}

async fn resignation_database_checkpoint(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('sessions',(SELECT jsonb_agg(to_jsonb(s) ORDER BY id) FROM play_sessions s),'matches',(SELECT jsonb_agg(to_jsonb(m) ORDER BY id) FROM matches m),'positions',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM positions p))")
        .fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn native_resign_decline_and_validation_leave_database_unchanged(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (_, mut session) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    session["phase"] = json!("move");
    session["legal_moves"] = json!([]);
    let sgf = format!(
        "(;FF[4]GM[6]MI[length:5][game:0][ws:0][bs:0]PW[Human]PB[GNUbg]RU[Crawford];PL[W]DI[21];PL[W]AE[a:y]AW{}AB[x])",
        "[g]".repeat(15)
    );
    let gid = "AQAAAP9/AAAAAA:MAGlAAAAAAAE";
    replace_native(&pool, &session, &sgf, gid).await;
    let uri = format!(
        "/api/play/sessions/{}/actions",
        session["id"].as_str().unwrap()
    );
    let before = resignation_database_checkpoint(&pool).await;
    let (status, declined) = call(
        app.clone(),
        "POST",
        &uri,
        json!({"version":0,"action":"resign","resign_level":1}),
    )
    .await;
    assert_eq!(status, 422, "{declined}");
    assert!(
        declined["error"].as_str().unwrap().contains("declined"),
        "{declined}"
    );
    assert_eq!(resignation_database_checkpoint(&pool).await, before);
    for body in [
        json!({"version":0,"action":"resign"}),
        json!({"version":0,"action":"resign","resign_level":null}),
        json!({"version":0,"action":"resign","resign_level":0}),
        json!({"version":0,"action":"resign","resign_level":4}),
        json!({"version":0,"action":"resign","resign_level":-1}),
        json!({"version":0,"action":"resign","resign_level":1.5}),
        json!({"version":0,"action":"resign","resign_level":"1"}),
        json!({"version":0,"action":"resign","resign_level":1,"move":0}),
        json!({"version":0,"action":"pass","resign_level":1}),
        json!({"version":0,"action":"move","move":0,"resign_level":1}),
        json!({"version":0,"action":"resign","resign_level":1,"decision_id":uuid::Uuid::new_v4()}),
    ] {
        let (status, error) = call(app.clone(), "POST", &uri, body.clone()).await;
        assert_eq!(status, 422, "{body}: {error}");
        assert_eq!(resignation_database_checkpoint(&pool).await, before);
    }
    let (status, stale) = call(
        app.clone(),
        "POST",
        &uri,
        json!({"version":1,"action":"resign","resign_level":3}),
    )
    .await;
    assert_eq!(status, 409, "{stale}");
    assert_eq!(resignation_database_checkpoint(&pool).await, before);
    for phase in [
        "opening_roll",
        "opening_result",
        "double_response",
        "game_over",
        "match_over",
    ] {
        session["phase"] = json!(phase);
        replace_native(&pool, &session, &sgf, gid).await;
        let before = resignation_database_checkpoint(&pool).await;
        let (status, error) = call(
            app.clone(),
            "POST",
            &uri,
            json!({"version":0,"action":"resign","resign_level":3}),
        )
        .await;
        assert_eq!(status, 422, "{phase}: {error}");
        assert_eq!(resignation_database_checkpoint(&pool).await, before);
    }
    session["phase"] = json!("move");
    replace_native(&pool, &session, &sgf, "invalid-checkpoint").await;
    let before = resignation_database_checkpoint(&pool).await;
    let (status, error) = call(
        app.clone(),
        "POST",
        &uri,
        json!({"version":0,"action":"resign","resign_level":3}),
    )
    .await;
    assert_eq!(status, 503, "{error}");
    assert_eq!(resignation_database_checkpoint(&pool).await, before);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn native_resign_cannot_bypass_explicit_learning_checkpoint(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (_, initial) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":0,"match_length":5}),
    )
    .await;
    let mut session = ready(app.clone(), initial).await;
    if session["phase"] == "roll" {
        let uri = format!(
            "/api/play/sessions/{}/actions",
            session["id"].as_str().unwrap()
        );
        let (status, rolled) = call(
            app.clone(),
            "POST",
            &uri,
            json!({"version":session["version"],"action":"roll"}),
        )
        .await;
        assert_eq!(status, 200, "{rolled}");
        session = rolled;
    }
    assert_eq!(session["phase"], "move");
    let uri = format!("/api/play/sessions/{}", session["id"].as_str().unwrap());
    let (status, training) = call(
        app.clone(),
        "POST",
        &format!("{uri}/mode"),
        json!({"version":session["version"],"play_mode":"training"}),
    )
    .await;
    assert_eq!(status, 200, "{training}");
    let (status, mut pending) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":training["version"],"action":"move","move":0}),
    )
    .await;
    assert_eq!(status, 200, "{pending}");
    assert_eq!(pending["pending_decision"]["action"], "move");
    for mode in ["training", "tutor"] {
        if mode == "tutor" {
            let (status, changed) = call(
                app.clone(),
                "POST",
                &format!("{uri}/mode"),
                json!({"version":pending["version"],"play_mode":"tutor"}),
            )
            .await;
            assert_eq!(status, 200, "{changed}");
            pending = changed;
        }
        for decision_id in [Value::Null, pending["pending_decision"]["id"].clone()] {
            let before = resignation_database_checkpoint(&pool).await;
            let (status, error) = call(app.clone(), "POST", &format!("{uri}/actions"),
                json!({"version":pending["version"],"action":"resign","resign_level":3,"decision_id":decision_id})).await;
            assert_eq!(status, 422, "{mode}: {error}");
            assert!(
                error["error"]
                    .as_str()
                    .unwrap()
                    .contains("pending decision"),
                "{error}"
            );
            assert_eq!(resignation_database_checkpoint(&pool).await, before);
        }
    }
    let (status, retried) = call(app.clone(), "POST", &format!("{uri}/feedback"),
        json!({"version":pending["version"],"operation":"retry","decision_id":pending["pending_decision"]["id"]})).await;
    assert_eq!(status, 200, "{retried}");
    assert!(retried["pending_decision"]["action"].is_null());
    let (status, resigned) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":retried["version"],"action":"resign","resign_level":3}),
    )
    .await;
    assert_eq!(status, 200, "{resigned}");
    assert!(resigned["pending_decision"].is_null());
    assert_eq!(
        resigned["decisions"], retried["decisions"],
        "An uncommitted hint must not be logged as played by resignation"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn native_resign_black_human_has_white_winner_and_resumable_history(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (status, session) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":1,"match_length":5}),
    )
    .await;
    assert_eq!(status, 201, "{session}");
    let session = ready(app.clone(), session).await;
    let uri = format!("/api/play/sessions/{}", session["id"].as_str().unwrap());
    let (status, finished) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":session["version"],"action":"resign","resign_level":3}),
    )
    .await;
    assert_eq!(status, 200, "{finished}");
    assert_eq!(finished["phase"], "game_over");
    assert_eq!(finished["scores"], json!([3, 0]));
    assert_eq!(finished["winner"], 0);
    assert_eq!(
        finished["history"].as_array().unwrap().last().unwrap(),
        &json!({"player":1,"dice":[],"notation":"resign"})
    );
    assert_library_checkpoint(&pool, &app, &finished).await;
    let (status, snapshots) = call(
        app.clone(),
        "GET",
        &format!("{uri}/history?version={}", finished["version"]),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{snapshots}");
    assert_eq!(
        snapshots["items"].as_array().unwrap().last().unwrap()["kind"],
        "resign"
    );
    assert_eq!(
        call(app.clone(), "GET", &uri, Value::Null).await.1,
        finished
    );
    let (status, next) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":finished["version"],"action":"next_game"}),
    )
    .await;
    assert_eq!(status, 200, "{next}");
    let continued = ready(app.clone(), next).await;
    assert_eq!(continued["scores"], json!([3, 0]));
    assert_eq!(continued["game_no"], 2);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn money_game_opens_plays_doubles_and_finishes_with_a_saved_report(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    for human in [0, 1] {
        let (status, initial) = call(
            app.clone(),
            "POST",
            "/api/play/sessions",
            json!({"human_player":human,"match_length":0,"play_mode":"competition"}),
        )
        .await;
        assert_eq!(status, 201, "{initial}");
        assert_eq!(initial["match_length"], 0);
        assert_eq!(initial["phase"], "opening_roll");
        let mut s = ready(app.clone(), initial).await;
        let uri = format!("/api/play/sessions/{}", s["id"].as_str().unwrap());
        assert!(
            ["move", "roll"].contains(&s["phase"].as_str().unwrap()),
            "zero is not an already reached match target: {s}"
        );
        if s["phase"] == "move" {
            let (status, next) = call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":s["version"],"action":"move","move":0}),
            )
            .await;
            assert_eq!(status, 200, "{next}");
            s = next;
        }
        assert_eq!(s["phase"], "roll", "{s}");
        assert_eq!(
            s["can_double"], true,
            "money play has no point-target cube restriction"
        );
        let (status, next) = call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":s["version"],"action":"double"}),
        )
        .await;
        assert_eq!(status, 200, "{next}");
        s = next;
        if s["phase"] != "game_over" {
            let (status, finished) = call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":s["version"],"action":"resign","resign_level":3}),
            )
            .await;
            assert_eq!(status, 200, "{finished}");
            s = finished;
        }
        assert_eq!(s["phase"], "game_over");
        assert_eq!(s["game_no"], 1);
        assert_eq!(s["match_length"], 0);
        assert!(
            s["scores"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n.as_i64().unwrap() > 0),
            "native money-game award is retained"
        );
        assert_eq!(s["game_results"][0]["result"]["scores_after"], s["scores"]);
        assert_library_checkpoint(&pool, &app, &s).await;
        let match_id = s["library_match_id"].as_str().unwrap();
        let (status, positions) = call(
            app.clone(),
            "GET",
            &format!("/api/positions?match_id={match_id}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{positions}");
        assert!(!positions["items"].as_array().unwrap().is_empty());
        let position_id = positions["items"][0]["position_id"].as_str().unwrap();
        let (status, occurrences) = call(
            app.clone(),
            "GET",
            &format!("/api/library/matches?position_id={position_id}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{occurrences}");
        assert!(
            occurrences["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["id"] == match_id)
        );
        let (_, export) = call(app.clone(), "GET", &format!("{uri}/sgf"), Value::Null).await;
        let source = export.as_str().unwrap();
        assert!(source.contains("length:0"));
        let parsed = neondojo::import::parse_sgf(source).unwrap();
        assert_eq!(parsed.match_length, 0);
        let (status, reviewed) = call(
            app.clone(),
            "POST",
            &format!("{uri}/review"),
            json!({"version":s["version"]}),
        )
        .await;
        assert_eq!(status, 200, "{reviewed}");
        assert!(reviewed["review"].is_object());
        let (status, report) = call(
            app.clone(),
            "GET",
            &format!(
                "/api/study/reports/{}",
                reviewed["study_report_id"].as_str().unwrap()
            ),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{report}");
        assert_eq!(report["match_length"], 0);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn money_game_coach_evaluates_and_commits_decisions_without_ending_early(pool: PgPool) {
    let app = api::router(AppState::new(pool, engine()));
    let (status, initial) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"human_player":0,"match_length":0,"play_mode":"coach"}),
    )
    .await;
    assert_eq!(status, 201, "{initial}");
    let mut state = ready(app.clone(), initial).await;
    let uri = format!(
        "/api/play/sessions/{}/actions",
        state["id"].as_str().unwrap()
    );
    for _ in 0..2 {
        let action = match state["phase"].as_str().unwrap() {
            "roll" => "roll",
            "double_response" => "take",
            "move" if state["legal_moves"].as_array().unwrap().is_empty() => "pass",
            "move" => "move",
            _ => panic!("unexpected phase {state}"),
        };
        let mut request = json!({"version":state["version"],"action":action});
        if action == "move" {
            request["move"] = json!(0);
        }
        let (status, mut committed) = call(app.clone(), "POST", &uri, request.clone()).await;
        assert_eq!(status, 200, "{committed}");
        if !committed["pending_decision"].is_null() {
            assert_ne!(action, "pass");
            let loss = committed["pending_decision"]["first_loss"]
                .as_f64()
                .unwrap();
            if matches!(action, "move" | "roll") {
                assert!(loss >= state["error_thresholds"]["error"].as_f64().unwrap());
            }
            request["version"] = committed["version"].clone();
            request["decision_id"] = committed["pending_decision"]["id"].clone();
            let (status, next) = call(app.clone(), "POST", &uri, request).await;
            assert_eq!(status, 200, "{next}");
            committed = next;
        }
        assert!(
            committed["decisions"].as_array().unwrap().last().unwrap()["first_loss"].is_number()
        );
        assert_eq!(committed["match_length"], 0);
        assert_ne!(committed["phase"], "match_over");
        state = committed;
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn money_series_preserves_scores_and_distinct_game_reports_with_original_history(
    pool: PgPool,
) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (status, initial) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"human_player":0,"match_length":0,"play_mode":"competition"}),
    )
    .await;
    assert_eq!(status, 201, "{initial}");
    let mut state = ready(app.clone(), initial).await;
    let id = state["id"].as_str().unwrap().to_owned();
    let uri = format!("/api/play/sessions/{id}");
    let mut reports = Vec::new();
    let mut previous_scores = [0, 0];
    for game in 1..=2 {
        // Exercise actual decisions in both games, not only empty resignation reports.
        for _ in 0..3 {
            let action = match state["phase"].as_str().unwrap() {
                "roll" => "roll",
                "double_response" => "take",
                "move" => "move",
                _ => panic!("{state}"),
            };
            let mut body = json!({"version":state["version"],"action":action});
            if action == "move" {
                body["move"] = json!(0);
            }
            let (status, next) = call(app.clone(), "POST", &format!("{uri}/actions"), body).await;
            assert_eq!(status, 200, "{next}");
            state = next;
            if action == "move" {
                break;
            }
        }
        let (status, finished) = call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":state["version"],"action":"resign","resign_level":3}),
        )
        .await;
        assert_eq!(status, 200, "{finished}");
        assert_eq!(finished["phase"], "game_over");
        assert_eq!(finished["game_no"], game);
        let scores = [
            finished["scores"][0].as_i64().unwrap(),
            finished["scores"][1].as_i64().unwrap(),
        ];
        assert!(scores[0] >= previous_scores[0] && scores[1] >= previous_scores[1]);
        assert!(scores.iter().sum::<i64>() > previous_scores.iter().sum());
        previous_scores = scores;
        // A direct next-game request must first secure this game's report.
        let (status, next) = call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":finished["version"],"action":"next_game"}),
        )
        .await;
        assert_eq!(status, 200, "{next}");
        assert_eq!(next["phase"], "opening_roll");
        assert_eq!(next["game_no"], game + 1);
        assert_eq!(next["scores"], finished["scores"]);
        assert!(next["review"].is_null());
        let (_, archive) = call(app.clone(), "GET", "/api/study/reports", Value::Null).await;
        let entries: Vec<_> = archive["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["library_match_id"] == finished["library_match_id"])
            .collect();
        assert_eq!(entries.len(), game as usize);
        let row = entries.iter().find(|r| r["game_no"] == game).unwrap();
        let report_id = row["id"].as_str().unwrap().to_owned();
        let (status, report) = call(
            app.clone(),
            "GET",
            &format!("/api/study/reports/{report_id}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{report}");
        assert_eq!(report["game_no"], game);
        assert_eq!(report["scores"], finished["scores"]);
        assert_eq!(report["review"]["games"].as_array().unwrap().len(), 1);
        assert_eq!(report["review"]["games"][0]["game_no"], game);
        let (status, history) = call(
            app.clone(),
            "GET",
            &format!(
                "/api/study/reports/{report_id}/history?version={}",
                report["version"]
            ),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{history}");
        assert_eq!(
            history["items"].as_array().unwrap().len(),
            report["history"].as_array().unwrap().len()
        );
        reports.push((report_id, report, history));
        assert_eq!(
            call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":finished["version"],"action":"next_game"})
            )
            .await
            .0,
            409
        );
        state = ready(app.clone(), next).await;
    }
    assert_ne!(reports[0].0, reports[1].0);
    for (report_id, report, history) in &reports {
        assert_eq!(
            call(
                app.clone(),
                "GET",
                &format!("/api/study/reports/{report_id}"),
                Value::Null
            )
            .await
            .1,
            report.clone()
        );
        assert_eq!(
            call(
                app.clone(),
                "GET",
                &format!(
                    "/api/study/reports/{report_id}/history?version={}",
                    report["version"]
                ),
                Value::Null
            )
            .await
            .1,
            history.clone()
        );
    }
    let (status, deleted) = call(
        app.clone(),
        "POST",
        "/api/library/actions",
        json!({"action":"delete","match_ids":[state["library_match_id"]]}),
    )
    .await;
    assert_eq!(status, 200, "{deleted}");
    for (report_id, mut report, history) in reports {
        report["library_match_id"] = Value::Null;
        assert_eq!(
            call(
                app.clone(),
                "GET",
                &format!("/api/study/reports/{report_id}"),
                Value::Null
            )
            .await
            .1,
            report
        );
        assert_eq!(
            call(
                app.clone(),
                "GET",
                &format!(
                    "/api/study/reports/{report_id}/history?version={}",
                    report["version"]
                ),
                Value::Null
            )
            .await
            .1,
            history
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn money_next_game_preserves_checkpoint_on_report_failure_and_migrates_legacy_report(
    pool: PgPool,
) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (_, initial) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"human_player":0,"match_length":0,"play_mode":"competition"}),
    )
    .await;
    let mut state = ready(app.clone(), initial).await;
    let id = state["id"].as_str().unwrap().to_owned();
    let uri = format!("/api/play/sessions/{id}");
    for _ in 0..3 {
        let action = if state["phase"] == "roll" {
            "roll"
        } else if state["phase"] == "double_response" {
            "take"
        } else {
            "move"
        };
        let mut body = json!({"version":state["version"],"action":action});
        if action == "move" {
            body["move"] = json!(0);
        }
        let (status, next) = call(app.clone(), "POST", &format!("{uri}/actions"), body).await;
        assert_eq!(status, 200, "{next}");
        state = next;
        if action == "move" {
            break;
        }
    }
    let (status, finished) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":state["version"],"action":"resign","resign_level":3}),
    )
    .await;
    assert_eq!(status, 200, "{finished}");
    let session_id: uuid::Uuid = id.parse().unwrap();
    let path: String = sqlx::query_scalar("SELECT gnubg_path FROM play_sessions WHERE id=$1")
        .bind(session_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let pinned = pin_engine_path(&pool, session_id, "/unavailable").await;
    for action in ["next_game", "end_session"] {
        assert_eq!(
            call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":finished["version"],"action":action})
            )
            .await
            .0,
            503
        );
    }
    // The saved session exposes its pinned engines; compare it unchanged.
    pin_engine_path(&pool, session_id, &pinned).await;
    assert_eq!(
        call(app.clone(), "GET", &uri, Value::Null).await.1,
        finished
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM study_report_snapshots WHERE session_id=$1"
        )
        .bind(session_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    pin_engine_path(&pool, session_id, &path).await;
    let (status, mut legacy) = call(
        app.clone(),
        "POST",
        &format!("{uri}/review"),
        json!({"version":finished["version"]}),
    )
    .await;
    assert_eq!(status, 200, "{legacy}");
    sqlx::query("DELETE FROM study_report_snapshots WHERE session_id=$1")
        .bind(session_id)
        .execute(&pool)
        .await
        .unwrap();
    legacy["phase"] = json!("match_over");
    legacy.as_object_mut().unwrap().remove("study_report_id");
    legacy
        .as_object_mut()
        .unwrap()
        .remove("study_report_version");
    sqlx::query("UPDATE play_sessions SET state=$2 WHERE id=$1")
        .bind(session_id)
        .bind(&legacy)
        .execute(&pool)
        .await
        .unwrap();
    let (status, next) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":legacy["version"],"action":"next_game"}),
    )
    .await;
    assert_eq!(status, 200, "{next}");
    assert_eq!(next["scores"], finished["scores"]);
    let (status, report) = call(
        app.clone(),
        "GET",
        &format!("/api/study/reports/{id}"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["review"], legacy["review"]);
    assert_eq!(report["study_report_id"], id);
    let next = ready(app.clone(), next).await;
    let (status, finished) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":next["version"],"action":"resign","resign_level":3}),
    )
    .await;
    assert_eq!(status, 200, "{finished}");
    let (status, ended) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":finished["version"],"action":"end_session"}),
    )
    .await;
    assert_eq!(status, 200, "{ended}");
    assert_ne!(ended["study_report_id"], id);
    assert_eq!(ended["money_session_ended"], true);
    assert_eq!(
        call(
            app.clone(),
            "GET",
            &format!("/api/study/reports/{id}"),
            Value::Null
        )
        .await
        .1,
        report
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn money_end_session_keeps_final_score_sgf_and_all_game_reports(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (_, initial) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"human_player":0,"match_length":0,"play_mode":"competition"}),
    )
    .await;
    let uri = format!("/api/play/sessions/{}", initial["id"].as_str().unwrap());
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":initial["version"],"action":"end_session"})
        )
        .await
        .0,
        422
    );
    let mut state = ready(app.clone(), initial).await;
    let mut first_report = Value::Null;
    for game in 1..=2 {
        let (status, finished) = call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":state["version"],"action":"resign","resign_level":3}),
        )
        .await;
        assert_eq!(status, 200, "{finished}");
        assert_eq!(finished["game_no"], game);
        state = finished;
        if game == 1 {
            let (status, reviewed) = call(
                app.clone(),
                "POST",
                &format!("{uri}/review"),
                json!({"version":state["version"]}),
            )
            .await;
            assert_eq!(status, 200, "{reviewed}");
            first_report = call(
                app.clone(),
                "GET",
                &format!(
                    "/api/study/reports/{}",
                    reviewed["study_report_id"].as_str().unwrap()
                ),
                Value::Null,
            )
            .await
            .1;
            let (status, paused) = call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":reviewed["version"],"action":"pause"}),
            )
            .await;
            assert_eq!(status, 200, "{paused}");
            assert_eq!(paused["phase"], "game_over");
            assert_eq!(paused["money_session_ended"], false);
            let (status, resumed) = call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":paused["version"],"action":"resume"}),
            )
            .await;
            assert_eq!(status, 200, "{resumed}");
            assert_eq!(resumed["scores"], reviewed["scores"]);
            assert_eq!(resumed["study_report_id"], reviewed["study_report_id"]);
            let (status, next) = call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":resumed["version"],"action":"next_game"}),
            )
            .await;
            assert_eq!(status, 200, "{next}");
            state = ready(app.clone(), next).await;
        }
    }
    let session_id: uuid::Uuid = state["id"].as_str().unwrap().parse().unwrap();
    let (status, reviewed) = call(
        app.clone(),
        "POST",
        &format!("{uri}/review"),
        json!({"version":state["version"]}),
    )
    .await;
    assert_eq!(status, 200, "{reviewed}");
    state = reviewed;
    let archived_final = call(
        app.clone(),
        "GET",
        &format!(
            "/api/study/reports/{}",
            state["study_report_id"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await
    .1;
    pin_engine_path(&pool, session_id, "/unavailable").await;
    let original: (String, String) =
        sqlx::query_as("SELECT sgf,native_id FROM play_sessions WHERE id=$1")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let (status, ended) = call(
        app.clone(),
        "POST",
        &format!("{uri}/actions"),
        json!({"version":state["version"],"action":"end_session"}),
    )
    .await;
    assert_eq!(status, 200, "{ended}");
    assert_eq!(ended["phase"], "match_over");
    assert_eq!(ended["money_session_ended"], true);
    assert_eq!(ended["scores"], state["scores"]);
    assert_eq!(ended["game_no"], 2);
    assert_eq!(ended["history"], state["history"]);
    assert_eq!(
        ended["version"].as_i64(),
        Some(state["version"].as_i64().unwrap() + 1)
    );
    assert!(ended["review"].is_object());
    assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, ended);
    let preserved: (String, String) =
        sqlx::query_as("SELECT sgf,native_id FROM play_sessions WHERE id=$1")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(preserved, original);
    let archive = call(app.clone(), "GET", "/api/study/reports", Value::Null)
        .await
        .1;
    assert_eq!(archive["items"].as_array().unwrap().len(), 2);
    assert_eq!(
        call(
            app.clone(),
            "GET",
            &format!(
                "/api/study/reports/{}",
                first_report["study_report_id"].as_str().unwrap()
            ),
            Value::Null
        )
        .await
        .1,
        first_report
    );
    let last = call(
        app.clone(),
        "GET",
        &format!(
            "/api/study/reports/{}",
            ended["study_report_id"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await
    .1;
    assert_eq!(last, archived_final);
    assert_eq!(ended["study_report_id"], state["study_report_id"]);
    assert_eq!(ended["study_report_version"], state["study_report_version"]);
    assert_eq!(last["scores"], ended["scores"]);
    assert_eq!(last["review"]["games"].as_array().unwrap().len(), 1);
    assert_eq!(last["review"]["games"][0]["game_no"], 2);
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &format!("{uri}/actions"),
            json!({"version":state["version"],"action":"next_game"})
        )
        .await
        .0,
        409
    );
    for action in ["next_game", "end_session", "roll"] {
        assert_eq!(
            call(
                app.clone(),
                "POST",
                &format!("{uri}/actions"),
                json!({"version":ended["version"],"action":action})
            )
            .await
            .0,
            422
        );
    }
    assert_eq!(call(app.clone(), "GET", &uri, Value::Null).await.1, ended);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn native_match_forfeit_archives_a_loss_and_fences_competing_actions(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), engine()));
    let (status, session) = call(
        app.clone(),
        "POST",
        "/api/play/sessions",
        json!({"play_mode":"competition","human_player":1,"match_length":7}),
    )
    .await;
    assert_eq!(status, 201, "{session}");
    let mut opening = ready(app.clone(), session).await;
    let id = opening["id"].as_str().unwrap();
    let uri = format!("/api/play/sessions/{id}");
    let action_uri = format!("{uri}/actions");
    if opening["phase"] == "roll" {
        let (status, rolled) = call(
            app.clone(),
            "POST",
            &action_uri,
            json!({"version":opening["version"], "action":"roll"}),
        )
        .await;
        assert_eq!(status, 200, "{rolled}");
        opening = rolled;
    }
    assert_eq!(opening["phase"], "move", "{opening}");
    let action = if opening["legal_moves"].as_array().unwrap().is_empty() {
        json!({"version":opening["version"], "action":"pass"})
    } else {
        json!({"version":opening["version"], "action":"move", "move":0})
    };
    let (status, session) = call(app.clone(), "POST", &action_uri, action).await;
    assert_eq!(status, 200, "{session}");
    assert!(!session["history"].as_array().unwrap().is_empty());

    let version = session["version"].as_i64().unwrap();
    let request = json!({"version":version, "action":"forfeit_match"});
    let (a, b) = tokio::join!(
        call(app.clone(), "POST", &action_uri, request.clone()),
        call(app.clone(), "POST", &action_uri, request),
    );
    let ((status, finished), (conflict, _)) = if a.0 == 200 { (a, b) } else { (b, a) };
    assert_eq!(status, 200, "{finished}");
    assert_eq!(conflict, 409);
    assert_eq!(finished["version"], version + 1);
    assert_eq!(finished["phase"], "match_over");
    assert_eq!(finished["match_forfeited"], true);
    assert_eq!(finished["winner"], 0);
    assert_eq!(finished["scores"], json!([7, 0]));
    assert_eq!(finished["history"], session["history"]);
    assert_eq!(finished["game_results"], session["game_results"]);
    assert_library_checkpoint(&pool, &app, &finished).await;
    assert_eq!(
        call(app.clone(), "GET", &uri, Value::Null).await.1,
        finished
    );
    let (status, history) = call(
        app.clone(),
        "GET",
        &format!("{uri}/history?version={}", version + 1),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{history}");
    assert_eq!(
        history["items"].as_array().unwrap().len(),
        session["history"].as_array().unwrap().len()
    );
    let (status, sgf) = call(app.clone(), "GET", &format!("{uri}/sgf"), Value::Null).await;
    assert_eq!(status, 200, "{sgf}");
    let metadata = neondojo::databases::source_metadata(sgf.as_str().unwrap());
    assert_eq!(metadata.result_state, "completed");
    assert_eq!(
        (metadata.white_score, metadata.black_score),
        (Some(7), Some(0))
    );
    let (status, _) = call(
        app,
        "POST",
        &action_uri,
        json!({"version":version + 1,"action":"next_game"}),
    )
    .await;
    assert_eq!(status, 422);
}
