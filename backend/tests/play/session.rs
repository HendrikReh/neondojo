use super::*;

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
async fn malformed_bodies_and_queries_return_the_json_error_body(pool: PgPool) {
    let app = api::router(AppState::new(pool, Engine::new("/unavailable".into())));
    // Rejected by the extractor before any handler or engine work runs.
    for (method, uri, body) in [
        ("POST", "/api/play/sessions", json!({"human_player": "x"})),
        ("GET", "/api/study/reports?limit=abc", Value::Null),
        ("GET", "/api/engine-matches?before=not-a-uuid", Value::Null),
    ] {
        let (status, error) = call(app.clone(), method, uri, body).await;
        assert_eq!(status, 422, "{method} {uri}: {error}");
        assert!(error["error"].is_string(), "{method} {uri}: {error}");
    }
}
