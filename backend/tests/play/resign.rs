use super::*;

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
