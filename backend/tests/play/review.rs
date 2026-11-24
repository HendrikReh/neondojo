use super::*;

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
    let match_job = neondojo::store::create_match_job(&pool, match_id, 1, 1)
        .await
        .unwrap();
    let missing = uuid::Uuid::new_v4();
    let (status, batch) = call(
        app,
        "POST",
        "/api/jobs/status",
        json!({"ids":[job.id,match_job.id,missing]}),
    )
    .await;
    assert_eq!(status, 200, "{batch}");
    let mut restricted = [job.id, match_job.id];
    restricted.sort();
    assert_eq!(
        batch,
        json!({"jobs":[],"missing":[missing],"restricted":restricted})
    );
}
