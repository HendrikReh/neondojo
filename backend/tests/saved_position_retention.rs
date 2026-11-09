use axum::{body::Body, http::Request};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
    import,
    model::Position,
    store,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SOURCE: &str = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][game:0][ws:0][bs:0]DT[2025-04-03]EV[Open]RE[W+1];W[31hefe];B[42qusu])\n(;GM[6]FF[4]MI[length:7][game:1][ws:6][bs:3]RE[W+1];W[31hefe])";

async fn call(pool: &PgPool, method: &str, path: &str, body: Value) -> (u16, Value) {
    let response = api::router(AppState::new(pool.clone(), Engine::new("unused".into())))
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
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn imported(pool: &PgPool) -> (Uuid, Vec<Position>) {
    let result = store::import_match(
        pool,
        "retain.sgf",
        "sgf",
        SOURCE.as_bytes(),
        SOURCE,
        &import::parse_sgf(SOURCE).unwrap(),
    )
    .await
    .unwrap();
    let id = result.r#match.id;
    bgtutor::move_catalog::ensure_catalog(pool, id)
        .await
        .unwrap();
    let positions = store::positions(pool, Some(id), None, None, 100)
        .await
        .unwrap()
        .items;
    (id, positions)
}

async fn save(pool: &PgPool, id: Uuid) -> Value {
    let (status, bookmark) = call(
        pool,
        "PUT",
        &format!("/api/positions/{id}/bookmark"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{bookmark}");
    bookmark
}

async fn delete(pool: &PgPool, id: Uuid) {
    let (status, result) = call(
        pool,
        "POST",
        "/api/library/actions",
        json!({"action":"delete","match_ids":[id]}),
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["results"][0]["status"], "deleted", "{result}");
}

async fn get(pool: &PgPool, path: &str) -> Value {
    let (status, result) = call(pool, "GET", path, Value::Null).await;
    assert_eq!(status, 200, "{path}: {result}");
    result
}

async fn owner(pool: &PgPool, position: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT match_uuid FROM positions WHERE id=$1")
        .bind(position)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn saved_positions_with_mixed_legacy_context_share_their_known_catalog_game(pool: PgPool) {
    let (source, positions) = imported(&pool).await;
    assert_eq!(positions[0].game_no, positions[1].game_no);
    sqlx::query("UPDATE positions SET gnubg_match_id='legacy unknown' WHERE id=$1")
        .bind(positions[1].id)
        .execute(&pool)
        .await
        .unwrap();
    save(&pool, positions[0].id).await;
    save(&pool, positions[1].id).await;
    delete(&pool, source).await;
    let host = owner(&pool, positions[0].id).await;
    assert_eq!(owner(&pool, positions[1].id).await, host);
    let games = store::match_games(&pool, host).await.unwrap().unwrap();
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].position_count, 2);
    assert_eq!((games[0].white_score, games[0].black_score), (0, 0));
    assert_eq!(
        store::get_position(&pool, positions[1].id)
            .await
            .unwrap()
            .unwrap()
            .gnubg_match_id,
        "legacy unknown"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn uncatalogued_legacy_saved_positions_retain_one_game_with_available_score_context(
    pool: PgPool,
) {
    let sgf = SOURCE.replace("game:0][ws:0][bs:0]", "game:0][ws:4][bs:2]");
    let result = store::import_match(
        &pool,
        "legacy.sgf",
        "sgf",
        sgf.as_bytes(),
        &sgf,
        &import::parse_sgf(&sgf).unwrap(),
    )
    .await
    .unwrap();
    let source = result.r#match.id;
    let positions = store::positions(&pool, Some(source), None, None, 100)
        .await
        .unwrap()
        .items;
    // Before game catalogs existed, positions were the only stored game state.
    sqlx::query("DELETE FROM match_games WHERE match_uuid=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE positions SET gnubg_match_id='legacy unknown' WHERE id=$1")
        .bind(positions[1].id)
        .execute(&pool)
        .await
        .unwrap();
    save(&pool, positions[0].id).await;
    save(&pool, positions[1].id).await;
    delete(&pool, source).await;
    let host = owner(&pool, positions[0].id).await;
    let games = store::match_games(&pool, host).await.unwrap().unwrap();
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].position_count, 2);
    assert_eq!((games[0].white_score, games[0].black_score), (4, 2));
    assert_eq!(owner(&pool, positions[1].id).await, host);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn match_deletion_retains_exact_saved_occurrences_with_source_context_and_no_ghosts(
    pool: PgPool,
) {
    let (source, positions) = imported(&pool).await;
    let position = &positions[0];
    let before = get(&pool, &format!("/api/matches/{source}/metadata")).await;
    assert_eq!(before["source_available"], true);
    let saved = save(&pool, position.id).await;
    delete(&pool, source).await;

    assert_eq!(
        call(&pool, "GET", &format!("/api/matches/{source}"), Value::Null)
            .await
            .0,
        404
    );
    assert_eq!(
        call(
            &pool,
            "GET",
            &format!("/api/matches/{source}/metadata"),
            Value::Null
        )
        .await
        .0,
        404
    );
    let host = owner(&pool, position.id).await;
    assert_ne!(host, source);
    let mut expected = serde_json::to_value(position).unwrap();
    expected["match_uuid"] = json!(host);
    assert_eq!(
        get(&pool, &format!("/api/positions/{}", position.id)).await,
        expected
    );
    assert_eq!(
        get(&pool, &format!("/api/positions/{}/bookmark", position.id)).await,
        saved
    );
    for unsaved in &positions[1..] {
        assert_eq!(
            call(
                &pool,
                "GET",
                &format!("/api/positions/{}", unsaved.id),
                Value::Null
            )
            .await
            .0,
            404
        );
    }
    let context = get(&pool, &format!("/api/matches/{host}/metadata")).await;
    for key in [
        "match_no",
        "white",
        "black",
        "match_length",
        "played_on",
        "tournament",
        "result_state",
        "white_score",
        "black_score",
    ] {
        assert_eq!(context[key], before[key], "retained {key}");
    }
    assert_eq!(context["source_available"], false);
    assert_eq!(
        get(&pool, &format!("/api/matches/{host}")).await["source_available"],
        false
    );
    let source_size:(i64,i64,i64,i64) = sqlx::query_as("SELECT octet_length(original)::bigint,octet_length(sgf)::bigint,(SELECT count(*) FROM positions WHERE match_uuid=m.id),(SELECT count(*) FROM match_games WHERE match_uuid=m.id) FROM matches m WHERE id=$1")
        .bind(host).fetch_one(&pool).await.unwrap();
    assert_eq!(source_size, (0, 0, 1, 1));
    assert_eq!(get(&pool, "/api/library/matches").await["items"], json!([]));
    assert_eq!(get(&pool, "/api/matches").await["items"], json!([]));
    let databases = get(&pool, "/api/databases").await;
    assert_eq!(databases.as_array().unwrap().len(), 1);
    assert_eq!(databases[0]["match_count"], 0);
    let browser = get(&pool, "/api/position-browser").await;
    assert_eq!(browser["items"].as_array().unwrap().len(), 1);
    let hit = &browser["items"][0];
    assert_eq!(hit["position"]["id"], position.id.to_string());
    assert_eq!(hit["source_available"], false);
    assert_eq!(hit["match_info"]["source_available"], false);
    assert_eq!(hit["match_no"], before["match_no"]);
    assert_eq!(hit["database_name"], "My matches");
    assert_eq!(hit["database_id"], before["database_id"]);
    assert_eq!(
        save(&pool, position.id).await,
        saved,
        "re-save cannot duplicate"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn database_deletion_uses_the_same_retention_and_internal_hosts_are_protected(pool: PgPool) {
    let (source, positions) = imported(&pool).await;
    let (status, db) = call(
        &pool,
        "POST",
        "/api/databases",
        json!({"name":"Tournament"}),
    )
    .await;
    assert_eq!(status, 201, "{db}");
    let db = db["id"].as_str().unwrap();
    let (status, filter) = call(
        &pool,
        "POST",
        "/api/position-filters",
        json!({"name":"Deleted database","definition":{"schema_version":1,"database_id":db}}),
    )
    .await;
    assert_eq!(status, 201, "{filter}");
    let (status, moved) = call(
        &pool,
        "POST",
        "/api/library/actions",
        json!({"action":"move","match_ids":[source],"target_database_id":db}),
    )
    .await;
    assert_eq!(status, 200, "{moved}");
    save(&pool, positions[0].id).await;
    let (status, result) = call(
        &pool,
        "DELETE",
        &format!("/api/databases/{db}?version=1&delete_matches=true"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 204, "{result}");
    let host = owner(&pool, positions[0].id).await;
    let host_db: Uuid = sqlx::query_scalar("SELECT database_id FROM matches WHERE id=$1")
        .bind(host)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(host_db.to_string(), db);
    let browser = get(&pool, "/api/position-browser").await;
    assert_eq!(browser["items"][0]["database_name"], "Tournament");
    assert_eq!(browser["items"][0]["database_id"], db);
    assert_eq!(
        get(&pool, "/api/databases").await.as_array().unwrap().len(),
        1
    );
    assert_eq!(
        get(&pool, "/api/position-filters").await[0]["database_missing"],
        true
    );
    let encoded = filter["definition"]
        .to_string()
        .bytes()
        .map(|b| format!("%{b:02X}"))
        .collect::<String>();
    assert_eq!(
        call(
            &pool,
            "GET",
            &format!("/api/position-browser?filters={encoded}"),
            Value::Null
        )
        .await
        .0,
        422
    );
    for (method, path, body) in [
        (
            "DELETE",
            format!("/api/databases/{host_db}?version=1&delete_matches=true"),
            Value::Null,
        ),
        (
            "PUT",
            format!("/api/databases/{host_db}"),
            json!({"name":"Expose","version":1}),
        ),
        ("GET", format!("/api/matches/{host}/source"), Value::Null),
        (
            "POST",
            "/api/library/export".into(),
            json!({"match_ids":[host],"format":"original"}),
        ),
        (
            "POST",
            "/api/library/export".into(),
            json!({"match_ids":[host],"format":"sgf"}),
        ),
        (
            "PUT",
            format!("/api/matches/{host}/metadata"),
            json!({"version":0,"result_state":"unknown"}),
        ),
    ] {
        assert_eq!(
            call(&pool, method, &path, body).await.0,
            404,
            "guard {method} {path}"
        );
    }
    for action in ["copy", "move", "delete", "tags"] {
        let (status, result) = call(
            &pool,
            "POST",
            "/api/library/actions",
            json!({"action":action,"match_ids":[host],"target_database_id":Uuid::from_u128(1)}),
        )
        .await;
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["results"][0]["status"], "not_found", "{result}");
    }
    assert_eq!(
        call(
            &pool,
            "POST",
            &format!("/api/matches/{host}/analysis"),
            json!({"plies":0})
        )
        .await
        .0,
        422
    );
    assert!(store::create_match_job(&pool, host, 0, 1).await.is_err());
    let parsed = import::parse_sgf(SOURCE).unwrap();
    assert!(
        store::import_match_in_database(
            &pool,
            "bad.sgf",
            "sgf",
            SOURCE.as_bytes(),
            SOURCE,
            &parsed,
            host_db
        )
        .await
        .is_err()
    );
}

async fn hint(pool: &PgPool, position: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    let result = json!({"engine":"GNUbg fixture","plies":0,"candidates":[{"rank":1,"notation":"8/5 6/5","equity":0.12,"probabilities":[0.6,0.1,0.01,0.4,0.05,0.005]}]});
    sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result) VALUES($1,$2,'analysis',0,'completed',$3)").bind(id).bind(position).bind(result).execute(pool).await.unwrap();
    id
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn retained_notes_analyses_and_existing_cards_remain_usable_without_resetting_learning(
    pool: PgPool,
) {
    let (source, positions) = imported(&pool).await;
    let position = positions[0].id;
    save(&pool, position).await;
    let job = hint(&pool, position).await;
    let game: Uuid =
        sqlx::query_scalar("SELECT id FROM match_games WHERE match_uuid=$1 AND game_no=1")
            .bind(source)
            .fetch_one(&pool)
            .await
            .unwrap();
    let mov: Uuid = sqlx::query_scalar("SELECT id FROM match_moves WHERE position_uuid=$1")
        .bind(position)
        .fetch_one(&pool)
        .await
        .unwrap();
    for (column, target, note) in [
        ("match_uuid", source, "Match note"),
        ("game_uuid", game, "Game note"),
        ("move_uuid", mov, "Move note"),
        ("position_uuid", position, "Position note"),
    ] {
        sqlx::query(&format!("INSERT INTO annotations(id,{column},tags,comment,version) VALUES($1,$2,ARRAY['study'],$3,1)")).bind(Uuid::new_v4()).bind(target).bind(note).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO imported_analyses(match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data) VALUES($1,$2,0,1,1,'checker','{\"engine\":\"Imported fixture\"}')").bind(source).bind(position).execute(&pool).await.unwrap();
    let (status, deck) = call(&pool, "POST", "/api/decks", json!({"name":"Keep reviews"})).await;
    assert_eq!(status, 201, "{deck}");
    let deck = deck["id"].as_str().unwrap();
    let (status, added) = call(
        &pool,
        "POST",
        &format!("/api/decks/{deck}/cards"),
        json!({"position_id":position}),
    )
    .await;
    assert_eq!(status, 200, "{added}");
    let card = get(&pool, &format!("/api/decks/{deck}/cards")).await["items"][0].clone();
    let card_id = card["id"].as_str().unwrap();
    let (status, revealed) = call(
        &pool,
        "POST",
        &format!("/api/cards/{card_id}/reveal"),
        json!({"expected_version":0}),
    )
    .await;
    assert_eq!(status, 200, "{revealed}");
    let (status,reviewed)=call(&pool,"POST",&format!("/api/cards/{card_id}/review"),json!({"expected_version":0,"session_id":revealed["session_id"],"request_id":Uuid::new_v4(),"rating":"good"})).await;
    assert_eq!(status, 200, "{reviewed}");
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM learning_cards c WHERE id=$1")
        .bind(Uuid::parse_str(card_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    delete(&pool, source).await;
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM learning_cards c WHERE id=$1")
        .bind(Uuid::parse_str(card_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        before, after,
        "learning payload and FSRS state remain unchanged"
    );
    assert!(store::get_job(&pool, job).await.unwrap().is_some());
    let browser = get(&pool, "/api/position-browser").await;
    let notes = browser["items"][0]["notes"].as_array().unwrap();
    for scope in ["match", "game", "move", "position"] {
        assert!(
            notes.iter().any(|n| n["scope"] == scope),
            "preserved {scope}: {notes:?}"
        );
    }
    assert_eq!(browser["items"][0]["card_ready"], true);
    let version = after["version"].as_i64().unwrap();
    let (status, revealed) = call(
        &pool,
        "POST",
        &format!("/api/cards/{card_id}/reveal"),
        json!({"expected_version":version}),
    )
    .await;
    assert_eq!(status, 200, "{revealed}");
    assert!(
        revealed["answer"]["notes"]
            .as_str()
            .unwrap()
            .contains("Match note")
    );
    assert!(
        revealed["answer"]["notes"]
            .as_str()
            .unwrap()
            .contains("Position note")
    );
    let (status, result) = call(
        &pool,
        "POST",
        &format!("/api/decks/{deck}/positions"),
        json!({"position_ids":[position]}),
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["added"], 0);
    assert_eq!(result["total"], 1);
    let imported: (Uuid, i64) = sqlx::query_as(
        "SELECT match_uuid,count(*) OVER() FROM imported_analyses WHERE position_uuid=$1",
    )
    .bind(position)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(imported, (owner(&pool, position).await, 1));
    assert!(
        store::create_job(&pool, position, "analysis", 0)
            .await
            .is_ok()
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn deletion_without_saved_positions_removes_the_entire_source_and_busy_jobs_still_guard_it(
    pool: PgPool,
) {
    let (source, positions) = imported(&pool).await;
    save(&pool, positions[0].id).await;
    let job = store::create_job(&pool, positions[0].id, "analysis", 0)
        .await
        .unwrap();
    let (_, result) = call(
        &pool,
        "POST",
        "/api/library/actions",
        json!({"action":"delete","match_ids":[source]}),
    )
    .await;
    assert_eq!(result["results"][0]["status"], "busy");
    assert_eq!(owner(&pool, positions[0].id).await, source);
    sqlx::query("UPDATE jobs SET status='failed' WHERE id=$1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(
            &pool,
            "DELETE",
            &format!("/api/positions/{}/bookmark", positions[0].id),
            Value::Null
        )
        .await
        .0,
        204
    );
    delete(&pool, source).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM matches")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM positions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn native_decisions_survive_only_for_saved_positions_without_fabricating_match_pr(
    pool: PgPool,
) {
    use bgtutor::model::PlayerAnalysisSummary;
    let (source, positions) = imported(&pool).await;
    let saved = positions[0].id;
    save(&pool, saved).await;
    let players = [
        PlayerAnalysisSummary::default(),
        PlayerAnalysisSummary {
            player: 1,
            ..Default::default()
        },
    ];
    let result = json!({"engine":"GNUbg","plies":0,"total_games":2,"completed_games":2,"total_moves":3,"analyzed_moves":3,"players":players,
        "performance_rating":{"method":"gnubg_emg_500_v1","profile":"GNUbg; 0-ply cubeful deterministic","games":2,"players":[
            {"player":0,"value":2.5,"normalized_equity_loss":0.005,"unforced_checker_moves":1,"close_cube_decisions":0},
            {"player":1,"value":4.0,"normalized_equity_loss":0.008,"unforced_checker_moves":1,"close_cube_decisions":0}]}});
    assert!(bgtutor::match_analysis::match_performance_rating(&result).is_some());
    let job = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,completed_games,result) VALUES($1,$2,'match',0,'completed',2,2,$3)").bind(job).bind(source).bind(&result).execute(&pool).await.unwrap();
    let snapshot = json!({"engine_id":"gnubg","gnubg_path":"/fixture/native-gnubg","engine_version":"GNUbg original"});
    sqlx::query(
        "UPDATE jobs SET engine_config='original-native-profile',engine_snapshot=$2 WHERE id=$1",
    )
    .bind(job)
    .bind(&snapshot)
    .execute(&pool)
    .await
    .unwrap();
    for p in &positions {
        let data = json!({"decision_no":p.ply,"checker_no":p.ply,"player":p.player,"kind":"checker","played":p.notation,"best":"8/5 6/5","played_equity":0.1,"best_equity":0.2,"equity_loss":0.1,"severity":"bad","dice":p.dice,"evaluation_status":"evaluated"});
        sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,$3,$4,$5,'checker',0.1,$6)").bind(job).bind(p.id).bind(p.game_no).bind(p.ply).bind(p.player).bind(data).execute(&pool).await.unwrap();
    }
    for n in [1, 2] {
        sqlx::query("INSERT INTO match_analysis_games(job_id,game_no,summary) VALUES($1,$2,$3)")
            .bind(job)
            .bind(n)
            .bind(&result)
            .execute(&pool)
            .await
            .unwrap();
    }
    assert_eq!(
        get(&pool, &format!("/api/matches/{source}/metadata")).await["white_pr"],
        2.5
    );
    delete(&pool, source).await;
    let host = owner(&pool, saved).await;
    let retained = store::get_job(&pool, job).await.unwrap().unwrap();
    assert_eq!(retained.match_uuid, Some(host));
    assert_eq!(retained.plies, 0);
    assert_eq!(retained.result.as_ref().unwrap()["engine"], "GNUbg");
    assert!(
        bgtutor::match_analysis::match_performance_rating(retained.result.as_ref().unwrap())
            .is_none()
    );
    let provenance: (String, Value) =
        sqlx::query_as("SELECT engine_config,engine_snapshot FROM jobs WHERE id=$1")
            .bind(job)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(provenance, ("original-native-profile".into(), snapshot));
    let checkpoints: Vec<(i32, Value)> =
        sqlx::query_as("SELECT game_no,summary FROM match_analysis_games WHERE job_id=$1")
            .bind(job)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(checkpoints, vec![(1, result)]);
    let decisions: Vec<Uuid> =
        sqlx::query_scalar("SELECT position_uuid FROM move_analyses WHERE job_id=$1")
            .bind(job)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(decisions, vec![saved]);
    let context = get(&pool, &format!("/api/matches/{host}/metadata")).await;
    assert!(context["white_pr"].is_null());
    assert!(context["black_pr"].is_null());
    let browser = get(&pool, "/api/position-browser").await;
    assert_eq!(
        browser["items"][0]["native_error"]["job_id"],
        job.to_string()
    );
    assert_eq!(browser["items"][0]["native_error"]["equity_loss"], 0.1);
    assert_eq!(browser["items"][0]["card_ready"], true);
    let (status, deck) = call(
        &pool,
        "POST",
        "/api/decks",
        json!({"name":"Native retained"}),
    )
    .await;
    assert_eq!(status, 201, "{deck}");
    let deck = deck["id"].as_str().unwrap();
    let (status, added) = call(
        &pool,
        "POST",
        &format!("/api/decks/{deck}/positions"),
        json!({"position_ids":[saved]}),
    )
    .await;
    assert_eq!(status, 200, "{added}");
    assert_eq!(added["added"], 1);
    assert_eq!(
        call(
            &pool,
            "POST",
            &format!("/api/decks/{deck}/cards"),
            json!({"job_id":job})
        )
        .await
        .0,
        422
    );
    assert_eq!(
        call(
            &pool,
            "GET",
            &format!("/api/jobs/{job}/statistics"),
            Value::Null
        )
        .await
        .0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn match_and_database_deletion_leave_running_job_lease_and_engine_snapshot_unchanged(
    pool: PgPool,
) {
    let (source, positions) = imported(&pool).await;
    save(&pool, positions[0].id).await;
    let (status, db) = call(
        &pool,
        "POST",
        "/api/databases",
        json!({"name":"Busy source"}),
    )
    .await;
    assert_eq!(status, 201, "{db}");
    let db = db["id"].as_str().unwrap();
    let (_, moved) = call(
        &pool,
        "POST",
        "/api/library/actions",
        json!({"action":"move","match_ids":[source],"target_database_id":db}),
    )
    .await;
    assert_eq!(moved["results"][0]["status"], "moved");
    let job = store::create_match_job(&pool, source, 2, 2).await.unwrap();
    sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()+interval '5 minutes',engine_config='worker-profile',engine_snapshot=$3 WHERE id=$1")
        .bind(job.id).bind(Uuid::new_v4()).bind(json!({"engine_id":"gnubg","gnubg_path":"/worker/gnubg","engine_version":"worker-version"})).execute(&pool).await.unwrap();
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(j) FROM jobs j WHERE id=$1")
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let (_, result) = call(
        &pool,
        "POST",
        "/api/library/actions",
        json!({"action":"delete","match_ids":[source]}),
    )
    .await;
    assert_eq!(result["results"][0]["status"], "busy");
    assert_eq!(
        call(
            &pool,
            "DELETE",
            &format!("/api/databases/{db}?version=1&delete_matches=true"),
            Value::Null
        )
        .await
        .0,
        409
    );
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(j) FROM jobs j WHERE id=$1")
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(owner(&pool, positions[0].id).await, source);
    assert_eq!(
        get(&pool, &format!("/api/matches/{source}")).await["source_available"],
        true
    );
}

#[sqlx::test(migrations = false)]
#[ignore = "requires PostgreSQL"]
async fn retention_migration_only_adds_infrastructure_and_preserves_existing_user_rows(
    pool: PgPool,
) {
    let all = sqlx::migrate!("./migrations");
    let previous = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            all.iter().filter(|m| m.version < 23).cloned().collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    previous.run(&pool).await.unwrap();
    let source = Uuid::new_v4();
    let position = Uuid::new_v4();
    sqlx::query("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf,display_name,played_on,tournament,result_state,white_score,black_score,metadata_version) VALUES($1,'legacy.sgf','sgf','legacy-hash','Alice','Bob',7,1,'private bytes'::bytea,$2,'Manual name','2025-04-03','Manual event','completed',7,3,9)")
        .bind(source).bind(SOURCE).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation) VALUES($1,$2,0,1,1,0,ARRAY[3,1],array_fill(0,ARRAY[26]),'legacy-position','legacy-match','8/5 6/5')")
        .bind(position).bind(source).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO saved_positions(position_uuid,saved_at) VALUES($1,'2025-04-03T12:00:00Z')",
    )
    .bind(position)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO annotations(id,position_uuid,tags,comment,version) VALUES($1,$2,ARRAY['manual'],'Private note',3)").bind(Uuid::new_v4()).bind(position).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO learning_cards(id,signature,payload,version,review_count) VALUES($1,'legacy-card','{\"source\":\"private\"}',4,2)").bind(Uuid::new_v4()).execute(&pool).await.unwrap();
    let old_match: Value = sqlx::query_scalar("SELECT to_jsonb(m) FROM matches m")
        .fetch_one(&pool)
        .await
        .unwrap();
    let old_databases: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(d) FROM databases d ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    let mut old_children = Vec::new();
    for table in [
        "positions",
        "saved_positions",
        "annotations",
        "learning_cards",
    ] {
        let rows: Vec<Value> = sqlx::query_scalar(&format!("SELECT to_jsonb(t) FROM {table} t"))
            .fetch_all(&pool)
            .await
            .unwrap();
        old_children.push(rows);
    }
    all.run(&pool).await.unwrap();
    let unchanged:Value=sqlx::query_scalar("SELECT to_jsonb(m)-ARRAY['collection_only','source_match_id','source_match_no','source_database_id','source_database_name'] FROM matches m").fetch_one(&pool).await.unwrap();
    assert_eq!(unchanged, old_match);
    let databases:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(d)-'collection_only' FROM databases d WHERE NOT collection_only ORDER BY id").fetch_all(&pool).await.unwrap();
    assert_eq!(databases, old_databases);
    for (table, expected) in [
        "positions",
        "saved_positions",
        "annotations",
        "learning_cards",
    ]
    .into_iter()
    .zip(old_children)
    {
        let rows: Vec<Value> = sqlx::query_scalar(&format!("SELECT to_jsonb(t) FROM {table} t"))
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(rows, expected, "preserved {table}");
    }
    assert_eq!(
        get(&pool, &format!("/api/matches/{source}")).await["source_available"],
        true
    );
    let flags: (bool, Option<Uuid>, Option<i64>) = sqlx::query_as(
        "SELECT collection_only,source_match_id,source_match_no FROM matches WHERE id=$1",
    )
    .bind(source)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(flags, (false, None, None));
    assert_eq!(
        get(&pool, "/api/databases").await.as_array().unwrap().len(),
        1
    );
}
