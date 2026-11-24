use super::*;

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
