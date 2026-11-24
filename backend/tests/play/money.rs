use super::*;

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
