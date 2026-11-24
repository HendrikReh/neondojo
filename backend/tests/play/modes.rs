use super::*;

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
