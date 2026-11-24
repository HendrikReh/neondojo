use super::*;

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
