use super::*;

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
