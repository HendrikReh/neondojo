mod profile_support;
use profile_support::{activate, call, create, initialized};
use serde_json::{Value, json};
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn legacy_workspace_references_do_not_block_profile_activation(pool: PgPool) {
    let (app, initial) = initialized(&pool, true).await;
    let mut configuration = initial["current_configuration"].clone();
    let missing = json!("11111111-1111-4111-8111-111111111111");
    configuration["library"]["database_id"] = missing.clone();
    configuration["positions"]["definition"]["database_id"] = missing.clone();
    configuration["learning"]["deck_id"] = missing.clone();
    configuration["annotations"]["filters"] =
        json!({"q":"old search","tag":"","kind":"","database_id":missing});
    configuration["appearance"]["mode"] = json!("dark");
    let target = create(&app, &initial, "Legacy workspace", configuration.clone()).await;
    let standard = &initial["active_profile"];
    let (code, state) = call(
        app.clone(),
        "POST",
        &format!(
            "/api/profiles/{}/activate",
            standard["id"].as_str().unwrap()
        ),
        activate(&target, standard, "keep"),
    )
    .await;
    assert_eq!(code, 200, "{state}");
    let target = &target["active_profile"];
    let (code, state) = call(
        app.clone(),
        "POST",
        &format!("/api/profiles/{}/activate", target["id"].as_str().unwrap()),
        activate(&state, target, "keep"),
    )
    .await;
    assert_eq!(code, 200, "{state}");
    assert_eq!(state["current_configuration"], configuration);
    let (code, availability) =
        call(app, "GET", "/api/configuration/availability", Value::Null).await;
    assert_eq!(code, 200, "{availability}");
    assert_eq!(availability["issues"], json!([]));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn profiles_create_switch_save_rename_and_delete_with_exact_snapshots(pool: PgPool) {
    let (app, initial) = initialized(&pool, true).await;
    let standard = initial["active_profile"].clone();
    let mut training = initial["current_configuration"].clone();
    training["appearance"]["mode"] = json!("dark");
    training["general"]["player_name"] = json!("Training");
    training["analysis"]["plies"] = json!(1);
    let state = create(&app, &initial, " Training ", training.clone()).await;
    let training_profile = state["active_profile"].clone();
    assert_eq!(training_profile["name"], "Training");
    assert_eq!(state["dirty"], false);
    let (code, mut state) = call(
        app.clone(),
        "POST",
        &format!(
            "/api/profiles/{}/activate",
            standard["id"].as_str().unwrap()
        ),
        activate(&state, &standard, "keep"),
    )
    .await;
    assert_eq!(code, 200, "{state}");
    assert_eq!(
        state["current_configuration"],
        initial["current_configuration"]
    );
    let mut current = state["current_configuration"].clone();
    current["general"]["player_name"] = json!("Unsaved");
    let (_, changed) = call(
        app.clone(),
        "PUT",
        "/api/configuration",
        json!({"expected_revision":state["revision"],"configuration":current}),
    )
    .await;
    state = changed;
    let (code, changed) = call(
        app.clone(),
        "POST",
        &format!(
            "/api/profiles/{}/activate",
            training_profile["id"].as_str().unwrap()
        ),
        activate(&state, &training_profile, "save_current"),
    )
    .await;
    assert_eq!(code, 200, "{changed}");
    state = changed;
    assert_eq!(state["current_configuration"], training);
    let (_, saved_standard) = call(
        app.clone(),
        "GET",
        &format!("/api/profiles/{}", standard["id"].as_str().unwrap()),
        Value::Null,
    )
    .await;
    assert_eq!(
        saved_standard["configuration"]["general"]["player_name"],
        "Unsaved"
    );
    let (code,changed)=call(app.clone(),"PATCH",&format!("/api/profiles/{}",training_profile["id"].as_str().unwrap()),json!({"name":"Renamed","expected_state_revision":state["revision"],"expected_profile_revision":state["active_profile"]["revision"]})).await;
    assert_eq!(code, 200, "{changed}");
    state = changed;
    assert_eq!(state["active_profile"]["name"], "Renamed");
    assert_eq!(state["dirty"], false);
    let (code,error)=call(app.clone(),"DELETE",&format!("/api/profiles/{}",training_profile["id"].as_str().unwrap()),json!({"expected_state_revision":state["revision"],"expected_profile_revision":state["active_profile"]["revision"]})).await;
    assert_eq!(code, 409);
    assert_eq!(error["code"], "active_profile");
    let (code,_)=call(app.clone(),"DELETE",&format!("/api/profiles/{}",standard["id"].as_str().unwrap()),json!({"expected_state_revision":state["revision"],"expected_profile_revision":saved_standard["revision"]})).await;
    assert_eq!(code, 200);
    let (_, profiles) = call(app, "GET", "/api/profiles", Value::Null).await;
    assert_eq!(profiles.as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn normalized_names_and_profile_revisions_are_enforced(pool: PgPool) {
    let (app, state) = initialized(&pool, true).await;
    let state = create(&app, &state, "Café", state["current_configuration"].clone()).await;
    for name in [" café ", "Cafe\u{301}"] {
        let (code,error)=call(app.clone(),"POST","/api/profiles",json!({"name":name,"expected_state_revision":state["revision"],"configuration":state["current_configuration"]})).await;
        assert_eq!(code, 409, "{error}");
        assert_eq!(error["code"], "duplicate_name");
    }
    let(code,error)=call(app.clone(),"PATCH",&format!("/api/profiles/{}",state["active_profile"]["id"].as_str().unwrap()),json!({"name":"Other","expected_state_revision":state["revision"],"expected_profile_revision":0})).await;
    assert_eq!(code, 409, "{error}");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn failed_target_validation_rolls_back_source_save_and_all_projections(pool: PgPool) {
    let (app, initial) = initialized(&pool, true).await;
    let standard = initial["active_profile"].clone();
    let state = create(
        &app,
        &initial,
        "Target",
        initial["current_configuration"].clone(),
    )
    .await;
    let mut unavailable = state["current_configuration"].clone();
    unavailable["opening_book"]["provider"] = json!("xg");
    unavailable["opening_book"]["xg_path"] = json!("/definitely/missing/book.ob");
    // Overwrite the inactive profile with a type-correct unavailable snapshot.
    // Current config is kept matching the request by first directly restoring state to this inactive target value.
    let target_id = state["active_profile"]["id"].as_str().unwrap();
    sqlx::query("UPDATE configuration_profiles SET configuration=$1 WHERE id=$2")
        .bind(unavailable)
        .bind(uuid::Uuid::parse_str(target_id).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let (code, state) = call(
        app.clone(),
        "POST",
        &format!(
            "/api/profiles/{}/activate",
            standard["id"].as_str().unwrap()
        ),
        activate(&state, &standard, "keep"),
    )
    .await;
    assert_eq!(code, 200, "{state}");
    let mut current = state["current_configuration"].clone();
    current["general"]["player_name"] = json!("Must remain unsaved");
    let (_, before) = call(
        app.clone(),
        "PUT",
        "/api/configuration",
        json!({"expected_revision":state["revision"],"configuration":current}),
    )
    .await;
    let (_, target) = call(
        app.clone(),
        "GET",
        &format!("/api/profiles/{target_id}"),
        Value::Null,
    )
    .await;
    let (_, settings_before) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    let (code, error) = call(
        app.clone(),
        "POST",
        &format!("/api/profiles/{target_id}/activate"),
        activate(&before, &target, "save_current"),
    )
    .await;
    assert_eq!(code, 422, "{error}");
    assert_eq!(error["code"], "unavailable_resources");
    let (_, after) = call(app.clone(), "GET", "/api/configuration", Value::Null).await;
    assert_eq!(before, after);
    let (_, settings_after) = call(app, "GET", "/api/settings", Value::Null).await;
    assert_eq!(settings_before, settings_after);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn profile_activation_preserves_queued_and_recovered_job_snapshots(pool: PgPool) {
    let (app, initial) = initialized(&pool, true).await;
    let sgf = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][game:0][ws:0][bs:0];W[31hefe])";
    let imported = neondojo::store::import_match(
        &pool,
        "profile.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &neondojo::import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let position = neondojo::store::positions(&pool, Some(imported.r#match.id), None, None, 10)
        .await
        .unwrap()
        .items
        .remove(0);
    let (code, old) = call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":position.id,"kind":"analysis"}),
    )
    .await;
    assert_eq!(code, 202, "{old}");
    assert_eq!(old["plies"], 3);
    let old_id: uuid::Uuid = serde_json::from_value(old["id"].clone()).unwrap();
    let before:Value=sqlx::query_scalar("SELECT jsonb_build_object('engine_snapshot',engine_snapshot,'engine_config',engine_config,'plies',plies) FROM jobs WHERE id=$1").bind(old_id).fetch_one(&pool).await.unwrap();
    let mut config = initial["current_configuration"].clone();
    config["analysis"]["plies"] = json!(1);
    let state = create(&app, &initial, "Fast", config).await;
    assert_eq!(state["current_configuration"]["analysis"]["plies"], 1);
    let (code, new) = call(
        app.clone(),
        "POST",
        "/api/jobs",
        json!({"position_id":position.id,"kind":"analysis"}),
    )
    .await;
    assert_eq!(code, 202, "{new}");
    assert_eq!(new["plies"], 1);
    assert_ne!(old["id"], new["id"]);
    // Lease recovery changes execution state, never the captured configuration.
    sqlx::query(
        "UPDATE jobs SET status='running',lease_until=now()-interval '1 second' WHERE id=$1",
    )
    .bind(old_id)
    .execute(&pool)
    .await
    .unwrap();
    let standard = initial["active_profile"].clone();
    let (code, restored) = call(
        app,
        "POST",
        &format!(
            "/api/profiles/{}/activate",
            standard["id"].as_str().unwrap()
        ),
        activate(&state, &standard, "keep"),
    )
    .await;
    assert_eq!(code, 200, "{restored}");
    let after:Value=sqlx::query_scalar("SELECT jsonb_build_object('engine_snapshot',engine_snapshot,'engine_config',engine_config,'plies',plies) FROM jobs WHERE id=$1").bind(old_id).fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn overwriting_an_inactive_profile_does_not_activate_it(pool: PgPool) {
    let (app, initial) = initialized(&pool, true).await;
    let standard = initial["active_profile"].clone();
    let state = create(
        &app,
        &initial,
        "Training",
        initial["current_configuration"].clone(),
    )
    .await;
    let mut config = state["current_configuration"].clone();
    config["appearance"]["mode"] = json!("dark");
    let(code,saved)=call(app.clone(),"PUT",&format!("/api/profiles/{}/snapshot",standard["id"].as_str().unwrap()),json!({"expected_state_revision":state["revision"],"expected_profile_revision":standard["revision"],"configuration":config})).await;
    assert_eq!(code, 200, "{saved}");
    assert_eq!(saved["active_profile"]["id"], state["active_profile"]["id"]);
    assert_eq!(saved["profile"]["id"], standard["id"]);
    assert_eq!(
        saved["profile"]["configuration"],
        saved["current_configuration"]
    );
    assert_eq!(saved["dirty"], true);
    let(code,restored)=call(app,"POST","/api/configuration/restore-profile",json!({"expected_state_revision":saved["revision"],"expected_profile_revision":saved["active_profile"]["revision"]})).await;
    assert_eq!(code, 200, "{restored}");
    assert_eq!(restored["dirty"], false);
    assert_eq!(
        restored["current_configuration"],
        state["active_profile"]["configuration"]
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn saving_non_engine_preferences_does_not_require_an_available_engine(pool: PgPool) {
    let (app, state) = initialized(&pool, false).await;
    let mut config = state["current_configuration"].clone();
    config["analysis"]["scope"] = json!("match");
    let(code,saved)=call(app,"PUT",&format!("/api/profiles/{}/snapshot",state["active_profile"]["id"].as_str().unwrap()),json!({"expected_state_revision":state["revision"],"expected_profile_revision":state["active_profile"]["revision"],"configuration":config})).await;
    assert_eq!(code, 200, "{saved}");
    assert_eq!(saved["dirty"], false);
    assert_eq!(saved["current_configuration"]["analysis"]["scope"], "match");
}
