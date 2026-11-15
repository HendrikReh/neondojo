use axum::{body::Body, http::Request};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
mod profile_support;

async fn call(app: axum::Router, method: &str, path: &str, body: Value) -> (u16, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let code = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (code, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn configuration_requires_explicit_initialization(pool: PgPool) {
    let app = api::router(AppState::new(pool, Engine::new("/missing/gnubg".into())));
    let (code, error) = call(app, "GET", "/api/configuration", json!(null)).await;
    assert_eq!(code, 409, "{error}");
    assert_eq!(error["code"], "configuration_not_initialized");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL and native GNUbg"]
async fn play_uses_default_thresholds_when_configuration_cannot_be_loaded(pool: PgPool) {
    let (app, _) = profile_support::initialized(&pool, true).await;
    sqlx::query("UPDATE configuration_state SET current_configuration = jsonb_set(current_configuration, '{analysis,error_thresholds}', '[]')")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = call(app.clone(), "GET", "/api/configuration", Value::Null).await;
    assert_eq!(status, 422);
    let (status, session) = call(
        app,
        "POST",
        "/api/play/sessions",
        json!({"human_player": 0, "match_length": 5}),
    )
    .await;
    assert_eq!(status, 201, "{session}");
    assert_eq!(
        session["tutor_threshold"],
        bgtutor::configuration::ErrorThresholds::default().error,
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn initialization_imports_browser_values_and_keeps_backend_authority(pool: PgPool) {
    let (app, state) = profile_support::initialized(&pool, false).await;
    assert_eq!(state["active_profile"]["name"], "Standard");
    assert_eq!(state["current_configuration"]["general"]["locale"], "de");
    assert_eq!(state["current_configuration"]["general"]["player_name"], "");
    assert_eq!(state["current_configuration"]["analysis"]["plies"], 2);
    assert_eq!(
        state["current_configuration"]["appearance"]["board_theme"],
        "walnut"
    );
    assert_eq!(state["dirty"], false);
    let (code,again)=call(app,"POST","/api/configuration/initialize",json!({"expected_settings_version":-1,"expected_book_version":-1,"configuration":profile_support::config()})).await;
    assert_eq!(code, 200);
    assert_eq!(state, again);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM configuration_profiles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn updates_are_durable_dirty_and_compare_and_swap_without_requiring_an_engine(pool: PgPool) {
    let (app, state) = profile_support::initialized(&pool, false).await;
    let mut config = state["current_configuration"].clone();
    config["appearance"]["mode"] = json!("dark");
    let body = json!({"expected_revision":state["revision"],"configuration":config});
    let (code, changed) = call(app.clone(), "PUT", "/api/configuration", body.clone()).await;
    assert_eq!(code, 200, "{changed}");
    assert_eq!(changed["dirty"], true);
    assert_eq!(changed["active_profile"], state["active_profile"]);
    assert_eq!(changed["differences"][0]["path"], "appearance.mode");
    let (code, error) = call(app.clone(), "PUT", "/api/configuration", body).await;
    assert_eq!(code, 409, "{error}");
    let (_, loaded) = call(app.clone(), "GET", "/api/configuration", Value::Null).await;
    assert_eq!(loaded, changed);
    let (code,restored)=call(app,"PUT","/api/configuration",json!({"expected_revision":loaded["revision"],"configuration":state["current_configuration"]})).await;
    assert_eq!(code, 200, "{restored}");
    assert_eq!(restored["dirty"], false);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn stale_concurrent_writes_cannot_mix_projection_values(pool: PgPool) {
    let (app, state) = profile_support::initialized(&pool, false).await;
    let mut a = state["current_configuration"].clone();
    a["general"]["player_name"] = json!("Alice");
    a["appearance"]["mode"] = json!("light");
    let mut b = state["current_configuration"].clone();
    b["general"]["player_name"] = json!("Bob");
    b["appearance"]["mode"] = json!("dark");
    let (a, b) = tokio::join!(
        call(
            app.clone(),
            "PUT",
            "/api/configuration",
            json!({"expected_revision":state["revision"],"configuration":a})
        ),
        call(
            app.clone(),
            "PUT",
            "/api/configuration",
            json!({"expected_revision":state["revision"],"configuration":b})
        )
    );
    assert!((a.0 == 200 && b.0 == 409) || (a.0 == 409 && b.0 == 200));
    let (_, current) = call(app.clone(), "GET", "/api/configuration", Value::Null).await;
    let (_, settings) = call(app, "GET", "/api/settings", Value::Null).await;
    assert_eq!(
        settings["player_name"],
        current["current_configuration"]["general"]["player_name"]
    );
    assert_eq!(
        current["current_configuration"]["appearance"]["mode"],
        if settings["player_name"] == "Alice" {
            "light"
        } else {
            "dark"
        }
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn invalid_configuration_is_rejected_before_any_projection_changes(pool: PgPool) {
    let (app, state) = profile_support::initialized(&pool, false).await;
    let mut config = state["current_configuration"].clone();
    config["general"]["player_name"] = json!("should rollback");
    config["appearance"]["board_theme"] = json!("unknown");
    let (code, error) = call(
        app.clone(),
        "PUT",
        "/api/configuration",
        json!({"expected_revision":state["revision"],"configuration":config}),
    )
    .await;
    assert_eq!(code, 422, "{error}");
    assert_eq!(error["issues"][0]["path"], "appearance.board_theme");
    let (_, current) = call(app, "GET", "/api/configuration", Value::Null).await;
    assert_eq!(state, current);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn initialization_is_concurrent_and_warns_about_missing_legacy_references(pool: PgPool) {
    let app = api::router(AppState::new(
        pool.clone(),
        Engine::new("/missing/gnubg".into()),
    ));
    let (_, settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    let (_, book) = call(app.clone(), "GET", "/api/opening-book", Value::Null).await;
    let mut config = profile_support::config();
    config["library"]["database_id"] = json!("11111111-1111-1111-1111-111111111111");
    config["learning"]["deck_id"] = json!("22222222-2222-2222-2222-222222222222");
    let body = json!({"expected_settings_version":settings["version"],"expected_book_version":book["config"]["version"],"configuration":config});
    let (a, b) = tokio::join!(
        call(
            app.clone(),
            "POST",
            "/api/configuration/initialize",
            body.clone()
        ),
        call(app.clone(), "POST", "/api/configuration/initialize", body)
    );
    assert_eq!(a.0, 200, "{}", a.1);
    assert_eq!(b.0, 200, "{}", b.1);
    assert_eq!(a.1["active_profile"]["id"], b.1["active_profile"]["id"]);
    let warned = if a.1.get("initialization_warnings").is_some() {
        &a.1
    } else {
        &b.1
    };
    assert_eq!(
        warned["initialization_warnings"].as_array().unwrap().len(),
        2
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM configuration_profiles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn legacy_settings_and_book_writes_update_current_only(pool: PgPool) {
    let (app, initial) = profile_support::initialized(&pool, true).await;
    let (_, mut settings) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    settings["player_name"] = json!("Legacy edit");
    let (code, saved) = call(app.clone(), "PUT", "/api/settings", settings).await;
    assert_eq!(code, 200, "{saved}");
    let (_, mut book) = call(app.clone(), "GET", "/api/opening-book", Value::Null).await;
    book["config"]["path"] = json!("/another/local/book.sqlite");
    let (code, saved) = call(
        app.clone(),
        "PUT",
        "/api/opening-book",
        book["config"].clone(),
    )
    .await;
    assert_eq!(code, 200, "{saved}");
    let (_, state) = call(app, "GET", "/api/configuration", Value::Null).await;
    assert_eq!(
        state["current_configuration"]["general"]["player_name"],
        "Legacy edit"
    );
    assert_eq!(
        state["current_configuration"]["opening_book"]["path"],
        "/another/local/book.sqlite"
    );
    assert_eq!(state["active_profile"], initial["active_profile"]);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn a_failure_in_the_second_projection_rolls_back_the_first(pool: PgPool) {
    let (app, state) = profile_support::initialized(&pool, false).await;
    let (_, settings_before) = call(app.clone(), "GET", "/api/settings", Value::Null).await;
    sqlx::raw_sql("CREATE FUNCTION reject_profile_book() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected second projection failure'; END $$; CREATE TRIGGER reject_profile_book BEFORE UPDATE ON opening_book_settings FOR EACH ROW EXECUTE FUNCTION reject_profile_book();").execute(&pool).await.unwrap();
    let mut config = state["current_configuration"].clone();
    config["general"]["player_name"] = json!("Must rollback");
    config["opening_book"]["path"] = json!("/new/book/path");
    let (code, error) = call(
        app.clone(),
        "PUT",
        "/api/configuration",
        json!({"expected_revision":state["revision"],"configuration":config}),
    )
    .await;
    assert_eq!(code, 500, "{error}");
    let (_, after) = call(app.clone(), "GET", "/api/configuration", Value::Null).await;
    assert_eq!(after, state);
    let (_, settings_after) = call(app, "GET", "/api/settings", Value::Null).await;
    assert_eq!(settings_after, settings_before);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn current_availability_reports_outages_without_changing_configuration(pool: PgPool) {
    let (app, state) = profile_support::initialized(&pool, false).await;
    let (code, availability) = call(
        app.clone(),
        "GET",
        "/api/configuration/availability",
        Value::Null,
    )
    .await;
    assert_eq!(code, 200, "{availability}");
    assert_eq!(availability["valid"], false);
    assert_eq!(availability["revision"], state["revision"]);
    assert_eq!(availability["issues"][0]["code"], "engine_unavailable");
    let (_, after) = call(app, "GET", "/api/configuration", Value::Null).await;
    assert_eq!(state, after);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn occurrence_sort_choices_are_persisted_in_configuration(pool: PgPool) {
    let (app, mut state) = profile_support::initialized(&pool, false).await;
    for sort in ["occurrence_database", "occurrence_game", "occurrence_move"] {
        let mut config = state["current_configuration"].clone();
        config["library"]["sort"] = json!(sort);
        let (code, updated) = call(
            app.clone(),
            "PUT",
            "/api/configuration",
            json!({"expected_revision":state["revision"],"configuration":config}),
        )
        .await;
        assert_eq!(code, 200, "{updated}");
        assert_eq!(updated["current_configuration"]["library"]["sort"], sort);
        state = updated;
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn position_column_choices_persist_without_changing_widths(pool: PgPool) {
    let (app, state) = profile_support::initialized(&pool, false).await;
    let mut config = state["current_configuration"].clone();
    config["positions"]["visible_columns"] = json!(["decks", "origin"]);
    config["positions"]["column_widths"] = json!({"decks": 222});
    let (code, updated) = call(
        app.clone(),
        "PUT",
        "/api/configuration",
        json!({"expected_revision": state["revision"], "configuration": config}),
    )
    .await;
    assert_eq!(code, 200, "{updated}");
    assert_eq!(
        updated["current_configuration"]["positions"]["visible_columns"],
        json!(["origin", "decks"])
    );
    let mut config = updated["current_configuration"].clone();
    config["positions"]["visible_columns"] = json!(["origin"]);
    let (code, result) = call(
        app.clone(),
        "PUT",
        "/api/configuration",
        json!({"expected_revision": updated["revision"], "configuration": config}),
    )
    .await;
    assert_eq!(code, 200, "{result}");
    let (_, loaded) = call(app, "GET", "/api/configuration", Value::Null).await;
    assert_eq!(
        loaded["current_configuration"]["positions"]["visible_columns"],
        json!(["origin"])
    );
    assert_eq!(
        loaded["current_configuration"]["positions"]["column_widths"],
        json!({"decks": 222})
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn legacy_deck_references_do_not_block_configuration_updates(pool: PgPool) {
    let (app, state) = profile_support::initialized(&pool, false).await;
    let deck = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO decks(id,name) VALUES($1,'Selected deck')")
        .bind(deck)
        .execute(&pool)
        .await
        .unwrap();
    let mut config = state["current_configuration"].clone();
    config["learning"]["deck_id"] = json!(deck);
    let (code, selected) = call(
        app.clone(),
        "PUT",
        "/api/configuration",
        json!({"expected_revision":state["revision"],"configuration":config}),
    )
    .await;
    assert_eq!(code, 200, "{selected}");
    sqlx::query("DELETE FROM decks WHERE id=$1")
        .bind(deck)
        .execute(&pool)
        .await
        .unwrap();
    let mut config = selected["current_configuration"].clone();
    config["appearance"]["mode"] = json!("dark");
    let (code, changed) = call(
        app.clone(),
        "PUT",
        "/api/configuration",
        json!({"expected_revision":selected["revision"],"configuration":config}),
    )
    .await;
    assert_eq!(code, 200, "{changed}");
    let mut config = changed["current_configuration"].clone();
    let missing_deck = uuid::Uuid::new_v4();
    config["learning"]["deck_id"] = json!(missing_deck);
    let (code, updated) = call(
        app,
        "PUT",
        "/api/configuration",
        json!({"expected_revision":changed["revision"],"configuration":config}),
    )
    .await;
    assert_eq!(code, 200, "{updated}");
    assert_eq!(
        updated["current_configuration"]["learning"]["deck_id"],
        json!(missing_deck)
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn occurrence_widths_roundtrip_without_changing_column_visibility_or_order(pool: PgPool) {
    let (app, state) = profile_support::initialized(&pool, false).await;
    let mut config = state["current_configuration"].clone();
    config["library"]["column_widths"] =
        json!({"white":211,"occurrence_database":240,"occurrence_game":90,"occurrence_move":100});
    let (code, updated) = call(
        app.clone(),
        "PUT",
        "/api/configuration",
        json!({"expected_revision":state["revision"],"configuration":config}),
    )
    .await;
    assert_eq!(code, 200, "{updated}");
    let (code, reloaded) = call(app.clone(), "GET", "/api/configuration", json!(null)).await;
    assert_eq!(code, 200);
    assert_eq!(
        reloaded["current_configuration"]["library"],
        config["library"]
    );
    for key in ["occurrence_database", "occurrence_game", "occurrence_move"] {
        for width in [0, 2401] {
            let mut invalid = config.clone();
            invalid["library"]["column_widths"][key] = json!(width);
            assert_eq!(
                call(
                    app.clone(),
                    "PUT",
                    "/api/configuration",
                    json!({"expected_revision":updated["revision"],"configuration":invalid})
                )
                .await
                .0,
                422
            );
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn money_game_target_is_a_valid_persisted_play_preference(pool: PgPool) {
    let (app, state) = profile_support::initialized(&pool, false).await;
    let mut config = state["current_configuration"].clone();
    config["play"]["match_length"] = json!(0);
    let (status, updated) = call(
        app.clone(),
        "PUT",
        "/api/configuration",
        json!({"expected_revision":state["revision"],"configuration":config}),
    )
    .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(updated["current_configuration"]["play"]["match_length"], 0);
    for target in [-1, 26] {
        config["play"]["match_length"] = json!(target);
        assert_eq!(
            call(
                app.clone(),
                "PUT",
                "/api/configuration",
                json!({"expected_revision":updated["revision"],"configuration":config})
            )
            .await
            .0,
            422
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn loads_pre_play_mode_profiles_without_changing_saved_preferences(pool: PgPool) {
    let (app, state) = profile_support::initialized(&pool, false).await;
    let mut legacy = state["current_configuration"].clone();
    for field in ["column_order", "visible_columns"] {
        legacy["library"][field]
            .as_array_mut()
            .unwrap()
            .retain(|id| id != "play_mode");
    }
    legacy["library"]["column_widths"] = json!({"actions": 110, "white": 211});
    legacy["positions"]["column_widths"] = json!({"actions": 40});
    sqlx::query("UPDATE configuration_profiles SET configuration=$1")
        .bind(&legacy)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE configuration_state SET current_configuration=$1")
        .bind(&legacy)
        .execute(&pool)
        .await
        .unwrap();

    let (status, loaded) = call(app.clone(), "GET", "/api/configuration", Value::Null).await;
    assert_eq!(status, 200, "{loaded}");
    assert_eq!(loaded["dirty"], false);
    let config = &loaded["current_configuration"];
    assert_eq!(
        config["library"]["column_order"],
        state["current_configuration"]["library"]["column_order"]
    );
    assert_eq!(
        config["library"]["visible_columns"],
        legacy["library"]["visible_columns"]
    );
    assert_eq!(
        config["library"]["column_widths"],
        json!({"actions": 160, "white": 211})
    );
    assert_eq!(config["positions"]["column_widths"], json!({"actions": 60}));
    let (status, result) = call(
        app,
        "PUT",
        "/api/configuration",
        json!({"expected_revision":loaded["revision"],"configuration":legacy}),
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result, loaded);
}
