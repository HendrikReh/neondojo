use axum::{body::Body, http::Request};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use http_body_util::BodyExt;
use neondojo::{
    api::{self, AppState},
    engine::Engine,
    import,
    model::Position,
    store,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SOURCE: &str = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[31hefe];B[double];W[take];B[42qusu])\n(;GM[6]FF[4]MI[length:7][ws:6][bs:2]RU[CrawfordGame];W[31hefe])";

async fn call(app: axum::Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(
                    body.map(|v| Body::from(v.to_string()))
                        .unwrap_or_else(Body::empty),
                )
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&bytes)})),
    )
}

fn encoded(v: &Value) -> String {
    v.to_string().bytes().map(|b| format!("%{b:02X}")).collect()
}

async fn search(app: axum::Router, fields: Value) -> Value {
    let (status, page) = call(
        app,
        "GET",
        &format!(
            "/api/position-browser?filters={}",
            encoded(&definition(fields))
        ),
        None,
    )
    .await;
    assert_eq!(status, 200, "{page}");
    page
}

fn definition(mut fields: Value) -> Value {
    fields["schema_version"] = json!(1);
    fields
}

async fn setup(pool: &PgPool) -> (axum::Router, Uuid, Vec<Position>) {
    let imported = store::import_match(
        pool,
        "fixture_100%.sgf",
        "sgf",
        SOURCE.as_bytes(),
        SOURCE,
        &import::parse_sgf(SOURCE).unwrap(),
    )
    .await
    .unwrap();
    let id = imported.r#match.id;
    save_match(pool, id).await;
    neondojo::move_catalog::ensure_catalog(pool, id)
        .await
        .unwrap();
    let positions = sqlx::query_as("SELECT * FROM positions WHERE match_uuid=$1 ORDER BY ordinal")
        .bind(id)
        .fetch_all(pool)
        .await
        .unwrap();
    (
        api::router(AppState::new(pool.clone(), Engine::new("unused".into()))),
        id,
        positions,
    )
}

// Browser behavior fixtures deliberately join the saved collection; import
// alone remains unbookmarked and is covered in position_bookmarks.rs.
async fn save_match(pool: &PgPool, id: Uuid) {
    let app = api::router(AppState::new(pool.clone(), Engine::new("unused".into())));
    let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1")
        .bind(id)
        .fetch_all(pool)
        .await
        .unwrap();
    for position in ids {
        let (status, response) = call(
            app.clone(),
            "PUT",
            &format!("/api/positions/{position}/bookmark"),
            None,
        )
        .await;
        assert_eq!(status, 200, "{response}");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn stale_game_plan_filter_reports_preparation_without_writing(pool: PgPool) {
    let (_, _, positions) = setup(&pool).await;
    sqlx::query("DELETE FROM position_game_plans")
        .execute(&pool)
        .await
        .unwrap();
    let options = (*pool.connect_options())
        .clone()
        .options([("default_transaction_read_only", "on")]);
    let reader = sqlx::postgres::PgPoolOptions::new()
        .connect_with(options)
        .await
        .unwrap();
    let app = api::router(AppState::new(reader.clone(), Engine::new("unused".into())));
    let path = format!(
        "/api/position-browser?filters={}",
        encoded(&definition(json!({"game_plan":"opening"})))
    );
    let (status, body) = call(app.clone(), "GET", &path, None).await;
    assert_eq!(status, 503, "{body}");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM position_game_plans")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(body["code"], "index_preparing");
    let state = AppState::new(pool.clone(), Engine::new("unused".into()));
    let worker = tokio::spawn(neondojo::browse_indexes::worker(state.clone()));
    let wait_for_page = || async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let (status, body) = call(app.clone(), "GET", &path, None).await;
                if status == 200 {
                    break;
                }
                assert_eq!(status, 503, "{body}");
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("background worker did not prepare the read-only page");
    };
    wait_for_page().await;

    // Invalidate one occurrence, then save a different one. A write notification
    // must wake the worker before its 60-second recovery poll.
    sqlx::query("DELETE FROM position_game_plans WHERE position_uuid=$1")
        .bind(positions[0].id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(call(app.clone(), "GET", &path, None).await.0, 503);
    let writer = api::router(state.clone());
    let (status, saved) = call(
        writer,
        "PUT",
        &format!("/api/positions/{}/bookmark", positions[1].id),
        None,
    )
    .await;
    assert_eq!(status, 200, "{saved}");
    wait_for_page().await;
    state.shutdown.cancel();
    worker.await.unwrap();
    reader.close().await;
}

#[sqlx::test(migrations = "./migrations")]
async fn similar_search_ranks_unsaved_occurrences_and_excludes_the_whole_match_by_default(
    pool: PgPool,
) {
    let (app, current_match, positions) = setup(&pool).await;
    let other = SOURCE.replace("Alice", "Carol");
    let other_match = store::import_match(
        &pool,
        "neighbors.sgf",
        "sgf",
        other.as_bytes(),
        &other,
        &import::parse_sgf(&other).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    let target = positions.first().unwrap();
    let url = format!("/api/positions/{}/similar", target.id);
    let (status, response) = call(app.clone(), "GET", &url, None).await;
    assert_eq!(status, 200, "{response}");
    let hits = response["items"].as_array().unwrap();
    assert_eq!(hits.len(), positions.len());
    assert!(
        hits.iter()
            .all(|hit| hit["position"]["match_uuid"] == other_match.to_string())
    );
    assert_eq!(hits[0]["distance"], 0);
    assert!(
        hits.windows(2)
            .all(|pair| pair[0]["distance"].as_i64() <= pair[1]["distance"].as_i64())
    );
    let (status, response) = call(
        app,
        "GET",
        &format!("{url}?include_current_match=true"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{response}");
    let hits = response["items"].as_array().unwrap();
    assert!(hits.iter().any(
        |hit| hit["position"]["match_uuid"] == current_match.to_string()
            && hit["position"]["game_no"] == 2
    ));
    assert!(
        hits.iter()
            .all(|hit| hit["position"]["id"] != target.id.to_string())
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn similar_context_filters_combine_and_follow_the_player_on_roll(pool: PgPool) {
    let target_source = "(;GM[6]PW[Alice]PB[Bob]MI[length:9][ws:4][bs:2]CV[2]CO[w];W[31hefe])";
    let other_source = "(;GM[6]PW[Carol]PB[Dan]MI[length:9][ws:2][bs:4]CV[2]CO[b];B[13qtst])";
    let sources = [
        target_source.to_owned(),
        other_source.to_owned(),
        other_source.replace("13qtst", "52lqqs"),
        other_source.replace("[ws:2][bs:4]", "[ws:4][bs:2]"),
        other_source.replace("length:9", "length:11"),
        other_source.replace("CV[2]", "CV[4]"),
        other_source.replace("CO[b]", "CO[w]"),
    ];
    let mut ids = Vec::new();
    for source in sources {
        let imported = store::import_match(
            &pool,
            "context.sgf",
            "sgf",
            source.as_bytes(),
            &source,
            &import::parse_sgf(&source).unwrap(),
        )
        .await
        .unwrap();
        ids.push(imported.r#match.id);
    }
    let position: Uuid = sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1")
        .bind(ids[0])
        .fetch_one(&pool)
        .await
        .unwrap();
    let app = api::router(AppState::new(pool, Engine::new("unused".into())));
    let url = format!("/api/positions/{position}/similar");
    for (query, expected) in [
        ("same_dice=true", vec![1, 3, 4, 5, 6]),
        ("same_score=true", vec![1, 2, 5, 6]),
        ("same_cube=true", vec![1, 2, 3, 4]),
        ("same_dice=true&same_score=true&same_cube=true", vec![1]),
    ] {
        let (status, response) = call(app.clone(), "GET", &format!("{url}?{query}"), None).await;
        assert_eq!(status, 200, "{response}");
        let mut actual: Vec<_> = response["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hit| {
                let id: Uuid =
                    serde_json::from_value(hit["position"]["match_uuid"].clone()).unwrap();
                ids.iter().position(|candidate| *candidate == id).unwrap()
            })
            .collect();
        actual.sort_unstable();
        assert_eq!(actual, expected, "{query}");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn similar_practice_reveals_the_saved_checker_answer(pool: PgPool) {
    let (app, match_id, positions) = setup(&pool).await;
    let position = &positions[0];
    let uri = format!("/api/positions/{}/practice-answer", position.id);
    let result = json!({"engine":"Saved GNU Backgammon hint","plies":2,"candidates":[{
        "rank":1,"notation":"8/5 6/5","equity":0.2,"probabilities":[]
    }]});
    sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result) VALUES($1,$2,'analysis',2,'completed',$3)")
        .bind(Uuid::new_v4())
        .bind(position.id)
        .bind(result)
        .execute(&pool)
        .await
        .unwrap();
    let (status, answer) = call(app.clone(), "POST", &uri, None).await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["best"], "8/5 6/5");
    assert_eq!(answer["engine"], "Saved GNU Backgammon hint");
    let match_job = native(&pool, match_id, position, 2, 0.0, "completed").await;
    sqlx::query("UPDATE move_analyses SET data=data || $1 WHERE job_id=$2")
        .bind(json!({"decision_no":1,"checker_no":1,"player":0,"kind":"checker","played":"8/5 6/5","dice":[3,1]}))
        .bind(match_job)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET result='{}' WHERE position_uuid=$1")
        .bind(position.id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, answer) = call(app.clone(), "POST", &uri, None).await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["engine"], "GNU Backgammon 2");
    assert_eq!(answer["best"], "8/5 6/5");
    let (status, _) = call(
        app,
        "POST",
        &format!("/api/positions/{}/practice-answer", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(status, 404);
}

#[sqlx::test(migrations = "./migrations")]
async fn similar_practice_uses_imported_answers_and_respects_competition(pool: PgPool) {
    let (app, match_id, positions) = setup(&pool).await;
    let position = &positions[0];
    let uri = format!("/api/positions/{}/practice-answer", position.id);
    sqlx::query("UPDATE matches SET format='xg',imported_analysis_loaded=true WHERE id=$1")
        .bind(match_id)
        .execute(&pool)
        .await
        .unwrap();
    let record = json!({
        "game_no":position.game_no,"source_move_no":1,"checker_ply":1,
        "position_id":position.position_id,"gnubg_match_id":position.gnubg_match_id,
        "dice":position.dice,"kind":"checker","player":position.player,"played":position.notation,
        "engine":"eXtreme Gammon","method":"evaluation","depth":"XGRoller++",
        "candidates":[{"rank":1,"notation":"8/5 6/5","equity":0.2,"equity_kind":"cubeful","probabilities":{},"raw":{}}],
        "raw":{}
    });
    for (index, data) in [(0, json!({})), (1, record)] {
        sqlx::query("INSERT INTO imported_analyses(match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data) VALUES($1,$2,$3,$4,1,'checker',$5)")
            .bind(match_id).bind(position.id).bind(index).bind(position.game_no).bind(data)
            .execute(&pool).await.unwrap();
    }
    let (status, answer) = call(app.clone(), "POST", &uri, None).await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["engine"], "eXtreme Gammon");
    assert_eq!(answer["depth"], "XGRoller++");

    sqlx::query("INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id,library_match_id) VALUES($1,$2,'/unavailable','','saved',$3)")
        .bind(Uuid::new_v4()).bind(json!({"phase":"move","play_mode":"competition","match_length":7})).bind(match_id)
        .execute(&pool).await.unwrap();
    let (status, response) = call(app, "POST", &uri, None).await;
    assert_eq!(status, 422, "{response}");
    assert!(response.get("best").is_none());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn similar_practice_scores_complete_moves_outside_the_hint_list(pool: PgPool) {
    let (_, _, positions) = setup(&pool).await;
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()));
    let position = &positions[0];
    let usual_hint = engine
        .analyze(&position.position_id, &position.gnubg_match_id, 2, None)
        .await
        .unwrap();
    assert!(
        usual_hint
            .candidates
            .iter()
            .all(|candidate| candidate.notation != "8/7 8/5")
    );
    let uri = format!("/api/positions/{}/practice-score", position.id);
    let app = api::router(AppState::new(pool, engine));
    let (status, invalid) = call(
        app.clone(),
        "POST",
        &uri,
        Some(json!({"steps":[{"from":8,"to":5}]})),
    )
    .await;
    assert_eq!(status, 422, "{invalid}");
    let (status, score) = call(
        app.clone(),
        "POST",
        &uri,
        Some(json!({"steps":[{"from":8,"to":5},{"from":8,"to":7}]})),
    )
    .await;
    assert_eq!(status, 200, "{score}");
    assert_eq!(score["best"], "8/5 6/5");
    assert_eq!(score["played"], "8/7 8/5");
    assert_eq!(score["depth"], "3-ply");
    let loss = score["equity_loss"].as_f64().unwrap();
    assert!(loss > 0.1);
    assert!(
        (score["best_equity"].as_f64().unwrap() - score["played_equity"].as_f64().unwrap() - loss)
            .abs()
            < 1e-6
    );
    let (status, score) = call(
        app,
        "POST",
        &uri,
        Some(json!({"steps":[{"from":6,"to":5},{"from":8,"to":5}]})),
    )
    .await;
    assert_eq!(status, 200, "{score}");
    assert_eq!(score["equity_loss"], 0.0);
}

#[sqlx::test(migrations = "./migrations")]
async fn similar_practice_scores_a_forced_pass(pool: PgPool) {
    let source = "(;GM[6]PW[Alice]PB[Bob]MI[length:7]AE[a:x]AW[y]AB[x][x][w][w];W[12])";
    let imported = store::import_match(
        &pool,
        "dance.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &import::parse_sgf(source).unwrap(),
    )
    .await
    .unwrap();
    let id: Uuid = sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1")
        .bind(imported.r#match.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    // As in Study, a forced pass is graded without starting an engine.
    let app = api::router(AppState::new(pool, Engine::new("/missing/gnubg".into())));
    let (status, score) = call(
        app,
        "POST",
        &format!("/api/positions/{id}/practice-score"),
        Some(json!({"steps":[]})),
    )
    .await;
    assert_eq!(status, 200, "{score}");
    assert_eq!(score["equity_loss"], 0.0);
    assert_eq!(score["played"], "Cannot move");
    assert_eq!(score["best"], "Cannot move");
    assert_eq!(score["best_equity"], Value::Null);
    assert_eq!(score["engine"], Value::Null);
}

#[sqlx::test(migrations = "./migrations")]
async fn similar_practice_scores_with_neongammon_and_never_falls_back_to_gnubg(pool: PgPool) {
    use std::os::unix::fs::PermissionsExt;
    let (_, _, positions) = setup(&pool).await;
    // A GNUbg stand-in that records whether anything starts it.
    let directory = tempfile::tempdir().unwrap();
    let started = directory.path().join("started");
    let gnubg = directory.path().join("gnubg");
    std::fs::write(
        &gnubg,
        format!("#!/bin/sh\ntouch '{}'\nexit 1\n", started.display()),
    )
    .unwrap();
    std::fs::set_permissions(&gnubg, std::fs::Permissions::from_mode(0o755)).unwrap();
    let gnubg = gnubg.display().to_string();
    sqlx::query("INSERT INTO app_settings(singleton,gnubg_path,analysis_engine,analysis_plies) VALUES(true,$1,'bgsage',2)")
        .bind(&gnubg)
        .execute(&pool)
        .await
        .unwrap();
    let app = api::router(AppState::new(pool, Engine::new(gnubg)));
    let uri = format!("/api/positions/{}/practice-score", positions[0].id);
    // This test database has no verified NeonGammon installation. Practice
    // evaluations run one at a time per process; wait for other tests' turns.
    let (status, error) = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let (status, body) = call(
                app.clone(),
                "POST",
                &uri,
                Some(json!({"steps":[{"from":8,"to":5},{"from":6,"to":5}]})),
            )
            .await;
            if status != 429 {
                break (status, body);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("another practice evaluation kept the engine busy");
    assert_eq!(status, 503, "{error}");
    assert_eq!(error["error"], "Exercise evaluation is unavailable");
    // A forced-pass check needs no engine; this position has a legal move.
    let (status, invalid) = call(app, "POST", &uri, Some(json!({"steps":[]}))).await;
    assert_eq!(status, 422, "{invalid}");
    assert!(
        !started.exists(),
        "scoring with NeonGammon selected must never start GNUbg"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn similar_practice_history_persists_rounds_and_deduplicates_save_retries(pool: PgPool) {
    let uri = "/api/study/similar-practice";
    let app = api::router(AppState::new(pool.clone(), Engine::new("unused".into())));
    let id = Uuid::new_v4();
    let completed_at = "2026-11-11T09:00:00Z";
    let retry_id = Uuid::new_v4();
    let (status, retry) = call(app.clone(), "POST", uri, Some(json!({"id":retry_id,"completed_at":"2026-11-11T09:01:00Z","total_positions":2,"losses":[0.0,0.0],"is_retry":true}))).await;
    assert_eq!(status, 200, "{retry}");
    assert_eq!(retry["average_equity_loss"], 0.0);
    let round = json!({"id":id,"completed_at":completed_at,"total_positions":4,"losses":[0.3,0.0,0.15],"is_retry":false});
    let (status, saved) = call(app.clone(), "POST", uri, Some(round.clone())).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["scored_moves"], 3);
    assert_eq!(saved["missed_moves"], 2);
    assert_eq!(saved["completed_at"], completed_at);
    assert!((saved["average_equity_loss"].as_f64().unwrap() - 0.15).abs() < 1e-12);
    assert_eq!(
        call(app.clone(), "POST", uri, Some(round)).await,
        (200, saved.clone())
    );
    let (status, _) = call(
        app.clone(),
        "POST",
        uri,
        Some(json!({"id":id,"completed_at":completed_at,"total_positions":4,"losses":[0.5],"is_retry":false})),
    )
    .await;
    assert_eq!(status, 409);
    for losses in [json!([]), json!([-0.01]), json!([0, 0, 0, 0, 0])] {
        let (status, _) = call(
            app.clone(),
            "POST",
            uri,
            Some(json!({"id":Uuid::new_v4(),"completed_at":completed_at,"total_positions":4,"losses":losses,"is_retry":false})),
        )
        .await;
        assert_eq!(status, 422);
    }
    let app = api::router(AppState::new(pool.clone(), Engine::new("unused".into())));
    let (status, page) = call(app.clone(), "GET", uri, None).await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["items"], json!([retry, saved]));
    assert_eq!(page["next_cursor"], Value::Null);
    sqlx::query("INSERT INTO similar_practice_summaries(id,completed_at,total_positions,scored_moves,missed_moves,total_equity_loss,is_retry)
                 SELECT gen_random_uuid(),'2000-01-01',1,1,0,0,false FROM generate_series(1,25)")
        .execute(&pool).await.unwrap();
    let (status, first) = call(app.clone(), "GET", uri, None).await;
    assert_eq!(status, 200, "{first}");
    let first_items = first["items"].as_array().unwrap();
    assert_eq!(first_items.len(), 25);
    assert_eq!(first_items[0]["id"], retry_id.to_string());
    assert_eq!(first["next_cursor"], first_items[24]["id"]);
    let cursor = first["next_cursor"].as_str().unwrap();
    let (status, older) = call(app.clone(), "GET", &format!("{uri}?before={cursor}"), None).await;
    assert_eq!(status, 200, "{older}");
    let older_items = older["items"].as_array().unwrap();
    assert_eq!(older_items.len(), 2);
    assert!(older_items.iter().all(|item| {
        !first_items
            .iter()
            .any(|previous| previous["id"] == item["id"])
    }));
    assert_eq!(older["next_cursor"], Value::Null);
    let (status, tiny) = call(
        app,
        "POST",
        uri,
        Some(json!({
            "id":Uuid::new_v4(),"completed_at":completed_at,"total_positions":2,
            "losses":[f64::from_bits(1),0.0],"is_retry":false
        })),
    )
    .await;
    assert_eq!(status, 200, "{tiny}");
    assert_eq!(tiny["average_equity_loss"], 0.0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and native GNUbg"]
async fn similar_practice_distinguishes_two_routes_to_the_same_point(pool: PgPool) {
    let source = "(;GM[6]PW[Alice]PB[Bob]MI[length:7]AE[a:x]AW[x]AB[u][q];W[31xwwt])";
    let imported = store::import_match(
        &pool,
        "routes.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &import::parse_sgf(source).unwrap(),
    )
    .await
    .unwrap();
    let id: Uuid = sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1")
        .bind(imported.r#match.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()));
    let app = api::router(AppState::new(pool, engine));
    let uri = format!("/api/positions/{id}/practice-score");
    for (intermediate, notation) in [(21, "24/21*/20"), (23, "24/20")] {
        let (status, score) = call(
            app.clone(),
            "POST",
            &uri,
            Some(json!({"steps":[{"from":24,"to":intermediate},{"from":intermediate,"to":20}]})),
        )
        .await;
        assert_eq!(status, 200, "{score}");
        assert_eq!(score["played"], notation);
    }
}

async fn note(pool: &PgPool, column: &str, target: Uuid, text: &str) {
    sqlx::query(&format!(
        "INSERT INTO annotations(id,{column},tags,comment,version) VALUES($1,$2,ARRAY['test'],$3,1)"
    ))
    .bind(Uuid::new_v4())
    .bind(target)
    .bind(text)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn notes_belong_to_the_exact_game_position_and_checker_move(pool: PgPool) {
    let (app, id, positions) = setup(&pool).await;
    let game: Uuid =
        sqlx::query_scalar("SELECT id FROM match_games WHERE match_uuid=$1 AND game_no=2")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let cube: Uuid = sqlx::query_scalar(
        "SELECT id FROM match_moves WHERE match_uuid=$1 AND kind='cube' ORDER BY move_no LIMIT 1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let checker: Uuid = sqlx::query_scalar("SELECT id FROM match_moves WHERE position_uuid=$1")
        .bind(positions[1].id)
        .fetch_one(&pool)
        .await
        .unwrap();
    note(&pool, "game_uuid", game, "gameonly").await;
    note(&pool, "move_uuid", cube, "cubeonly").await;
    note(&pool, "move_uuid", checker, "checkeronly").await;
    note(&pool, "position_uuid", positions[0].id, "positiononly %_").await;
    note(&pool, "match_uuid", id, "matchwide").await;
    assert_eq!(
        search(app.clone(), json!({"comment":"gameonly"})).await["items"][0]["position"]["id"],
        positions[2].id.to_string()
    );
    assert_eq!(
        search(app.clone(), json!({"q":"cubeonly"})).await["items"],
        json!([])
    );
    let found = search(app.clone(), json!({"q":"checkeronly"})).await;
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        found["items"][0]["position"]["id"],
        positions[1].id.to_string()
    );
    assert_eq!(
        search(app.clone(), json!({"comment":"%_"})).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        search(app.clone(), json!({"q":"matchwide"})).await["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let first = search(app, json!({"comment":"positiononly"})).await;
    assert_eq!(first["items"][0]["tags"], json!(["test"]));
    assert_eq!(first["items"][0]["notes"].as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn source_context_dice_player_and_duplicate_boards_stay_distinct(pool: PgPool) {
    let (app, _, positions) = setup(&pool).await;
    let all = search(app.clone(), json!({})).await;
    assert_eq!(all["items"].as_array().unwrap().len(), 3);
    assert_eq!(positions[0].position_id, positions[2].position_id);
    let first = search(app.clone(), json!({"die1":1,"die2":3,"white_score":0,"black_score":0,"cube_value":1,"crawford":false,"player":"ALICE"})).await;
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        first["items"][0]["position"]["id"],
        positions[0].id.to_string()
    );
    let second = search(app.clone(), json!({"player":"Bob","die1":2,"cube_value":2})).await;
    assert_eq!(second["items"][0]["cube_owner"], 0);
    assert_eq!(
        second["items"][0]["position"]["id"],
        positions[1].id.to_string()
    );
    let crawford = search(
        app.clone(),
        json!({"white_score":6,"black_score":2,"crawford":true}),
    )
    .await;
    assert_eq!(
        crawford["items"][0]["position"]["id"],
        positions[2].id.to_string()
    );
    assert_eq!(
        search(app.clone(), json!({"match_name":"100%"})).await["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        search(app, json!({"match_points_min":8})).await["items"],
        json!([])
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn cursor_is_stable_and_bound_to_every_filter_and_sort(pool: PgPool) {
    let (app, _, _) = setup(&pool).await;
    let filters = encoded(&definition(json!({})));
    let (status, first) = call(
        app.clone(),
        "GET",
        &format!("/api/position-browser?filters={filters}&limit=1"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{first}");
    let cursor = first["next_cursor"].as_str().unwrap();
    let (_, second) = call(
        app.clone(),
        "GET",
        &format!("/api/position-browser?filters={filters}&limit=1&after={cursor}"),
        None,
    )
    .await;
    assert_ne!(
        first["items"][0]["position"]["id"],
        second["items"][0]["position"]["id"]
    );
    for fields in [
        json!({"player":"Alice"}),
        json!({"sort":"loss"}),
        json!({"die1":1}),
    ] {
        let (status, body) = call(
            app.clone(),
            "GET",
            &format!(
                "/api/position-browser?filters={}&after={cursor}",
                encoded(&definition(fields))
            ),
            None,
        )
        .await;
        assert_eq!(status, 422, "{body}");
    }
    for uri in [
        "/api/position-browser?limit=101",
        "/api/position-browser?limit=0",
        "/api/position-browser?after=broken",
    ] {
        assert_eq!(call(app.clone(), "GET", uri, None).await.0, 422);
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn saved_filters_validate_names_schema_and_optimistic_versions(pool: PgPool) {
    let (app, _, _) = setup(&pool).await;
    let body = json!({"name":"  My Search  ","definition":definition(json!({"player":"Alice"}))});
    let (status, saved) = call(
        app.clone(),
        "POST",
        "/api/position-filters",
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, 201, "{saved}");
    assert_eq!(saved["name"], "My Search");
    assert_eq!(saved["version"], 1);
    assert_eq!(
        call(
            app.clone(),
            "POST",
            "/api/position-filters",
            Some(json!({"name":"my search","definition":definition(json!({}))}))
        )
        .await
        .0,
        409
    );
    let path = format!("/api/position-filters/{}", saved["id"].as_str().unwrap());
    let update =
        json!({"name":"Renamed","definition":definition(json!({"die1":2})),"expected_version":1});
    assert_eq!(
        call(app.clone(), "PUT", &path, Some(update.clone()))
            .await
            .1["version"],
        2
    );
    assert_eq!(call(app.clone(), "PUT", &path, Some(update)).await.0, 409);
    assert_eq!(
        call(
            app.clone(),
            "DELETE",
            &format!("{path}?expected_version=1"),
            None
        )
        .await
        .0,
        409
    );
    assert_eq!(
        call(
            app.clone(),
            "DELETE",
            &format!("{path}?expected_version=2"),
            None
        )
        .await
        .0,
        204
    );
    assert_eq!(
        call(
            app.clone(),
            "DELETE",
            &format!("{path}?expected_version=2"),
            None
        )
        .await
        .0,
        404
    );
    for bad in [
        json!({"schema_version":2}),
        json!({"schema_version":1,"typo":1}),
        json!({}),
        definition(json!({"die1":7})),
        definition(json!({"cube_value":3})),
        definition(json!({"min_loss":-1})),
        definition(json!({"analysis_state":"imported"})),
        definition(json!({"sort":"unknown"})),
        definition(json!({"white_score":-1})),
        definition(json!({"match_points_min":10,"match_points_max":2})),
    ] {
        let (status, error) = call(
            app.clone(),
            "POST",
            "/api/position-filters",
            Some(json!({"name":"bad","definition":bad})),
        )
        .await;
        assert_eq!(status, 422, "{error}");
    }
    assert_eq!(
        call(
            app,
            "POST",
            "/api/position-filters",
            Some(json!({"name":" ","definition":definition(json!({}))}))
        )
        .await
        .0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn deleted_database_reference_is_preserved_and_explicitly_rejected(pool: PgPool) {
    let (app, _, _) = setup(&pool).await;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO databases(id,name) VALUES($1,'Temporary')")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, saved) = call(
        app.clone(),
        "POST",
        "/api/position-filters",
        Some(json!({"name":"scoped","definition":definition(json!({"database_id":id}))})),
    )
    .await;
    sqlx::query("DELETE FROM databases WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, list) = call(app.clone(), "GET", "/api/position-filters", None).await;
    assert_eq!(list[0]["id"], saved["id"]);
    assert_eq!(list[0]["database_missing"], true);
    assert_eq!(
        call(
            app,
            "GET",
            &format!(
                "/api/position-browser?filters={}",
                encoded(&definition(json!({"database_id":id})))
            ),
            None
        )
        .await
        .0,
        422
    );
}

async fn native(
    pool: &PgPool,
    match_id: Uuid,
    position: &Position,
    plies: i32,
    loss: f64,
    status: &str,
) -> Uuid {
    let existing:Option<Uuid>=sqlx::query_scalar("SELECT j.id FROM jobs j WHERE j.match_uuid=$1 AND j.kind='match' AND j.plies=$2 AND j.status=$3 AND NOT EXISTS(SELECT 1 FROM move_analyses ma WHERE ma.job_id=j.id AND ma.position_uuid=$4) ORDER BY j.created_at DESC LIMIT 1")
        .bind(match_id).bind(plies).bind(status).bind(position.id).fetch_optional(pool).await.unwrap();
    let id = existing.unwrap_or_else(Uuid::new_v4);
    if existing.is_none() {
        sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,result,total_games,completed_games) VALUES($1,$2,'match',$3,$4,$5,1,1)")
            .bind(id).bind(match_id).bind(plies).bind(status).bind(json!({"engine":format!("GNU Backgammon {plies}")})).execute(pool).await.unwrap();
    }
    sqlx::query(
        "INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(position.game_no)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,$3,$4,$5,'checker',$6,$7)")
        .bind(id).bind(position.id).bind(position.game_no).bind(position.ordinal + 1).bind(position.player).bind(loss)
        .bind(json!({"decision_no":position.ordinal + 1,"checker_no":position.ply,"player":position.player,"kind":"checker","played":position.notation,"best":"8/5 6/5","severity":"bad","evaluation_status":"evaluated","played_equity":0.0,"best_equity":loss,"equity_loss":loss})).execute(pool).await.unwrap();
    id
}

#[sqlx::test(migrations = "./migrations")]
async fn native_loss_and_readiness_use_one_latest_completed_full_match(pool: PgPool) {
    let (app, id, positions) = setup(&pool).await;
    native(&pool, id, &positions[0], 1, 0.5, "completed").await;
    let authoritative = native(&pool, id, &positions[0], 2, 0.09, "completed").await;
    native(&pool, id, &positions[0], 3, 0.9, "running").await;
    sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result) VALUES($1,$2,'analysis',2,'completed',$3)")
        .bind(Uuid::new_v4()).bind(positions[1].id).bind(json!({"engine":"GNU Backgammon hint","plies":2,"candidates":[{"rank":1,"notation":"best","equity":0.2,"probabilities":[]}]})).execute(&pool).await.unwrap();
    let ready = search(app.clone(), json!({"analysis_state":"ready","sort":"loss"})).await;
    assert_eq!(ready["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        ready["items"][0]["native_error"]["job_id"],
        authoritative.to_string()
    );
    assert_eq!(ready["items"][0]["native_error"]["equity_loss"], 0.09);
    assert_eq!(ready["items"][0]["native_error"]["severity"], "blunder");
    let (status, readiness) = call(
        app.clone(),
        "POST",
        "/api/position-browser/analysis-readiness",
        Some(json!({"position_ids": [positions[0].id, positions[1].id, positions[2].id, positions[2].id, Uuid::new_v4()]})),
    )
    .await;
    assert_eq!(status, 200, "{readiness}");
    assert_eq!(
        readiness,
        json!({"analysis_required":2,"unavailable":1,"first_analysis_required":positions[1].id})
    );
    assert_eq!(
        search(app.clone(), json!({"min_loss":0.1})).await["items"],
        json!([])
    );
    assert_eq!(
        search(app, json!({"analysis_state":"missing"})).await["items"][0]["position"]["id"],
        positions[1].id.to_string()
    );
}

#[test]
fn openapi_covers_browser_and_strict_persisted_filter_contract() {
    use utoipa::OpenApi;
    let doc = serde_json::to_value(api::ApiDoc::openapi()).unwrap();
    assert!(doc["paths"]["/api/position-browser"]["get"].is_object());
    assert!(doc["paths"]["/api/position-filters/{id}"]["delete"].is_object());
    assert!(doc["components"]["schemas"]["PositionFilterDefinition"].is_object());
    let parameters = doc["paths"]["/api/position-browser"]["get"]["parameters"]
        .as_array()
        .unwrap();
    for name in ["filters", "after", "limit"] {
        let parameter = parameters.iter().find(|p| p["name"] == name).unwrap();
        assert_eq!(parameter["in"], "query");
        assert_eq!(parameter["required"], false);
    }
    let filters = parameters.iter().find(|p| p["name"] == "filters").unwrap();
    assert!(
        filters["description"]
            .as_str()
            .unwrap()
            .contains("PositionFilterDefinition")
    );
    assert!(filters["example"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn filters_are_database_scoped_and_do_not_merge_copied_positions(pool: PgPool) {
    let (app, original, _) = setup(&pool).await;
    let database = Uuid::new_v4();
    sqlx::query("INSERT INTO databases(id,name) VALUES($1,'Second')")
        .bind(database)
        .execute(&pool)
        .await
        .unwrap();
    let copy = store::import_match_in_database(
        &pool,
        "copy.sgf",
        "sgf",
        SOURCE.as_bytes(),
        SOURCE,
        &import::parse_sgf(SOURCE).unwrap(),
        database,
    )
    .await
    .unwrap()
    .r#match
    .id;
    save_match(&pool, copy).await;
    note(&pool, "match_uuid", original, "private-to-original").await;
    assert_eq!(
        search(app.clone(), json!({})).await["items"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    let found = search(app.clone(), json!({"database_id":database})).await;
    assert_eq!(found["items"].as_array().unwrap().len(), 3);
    assert!(
        found["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|hit| hit["position"]["match_uuid"] == copy.to_string())
    );
    assert_eq!(
        search(
            app,
            json!({"database_id":database,"q":"private-to-original"})
        )
        .await["items"],
        json!([])
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn loss_pagination_covers_known_losses_and_unknowns_once(pool: PgPool) {
    let (app, id, positions) = setup(&pool).await;
    native(&pool, id, &positions[0], 2, 0.04, "completed").await;
    native(&pool, id, &positions[1], 2, 0.16, "completed").await;
    let filters = encoded(&definition(json!({"sort":"loss"})));
    let mut after = String::new();
    let mut ids = vec![];
    loop {
        let (status, page) = call(
            app.clone(),
            "GET",
            &format!("/api/position-browser?filters={filters}&limit=1{after}"),
            None,
        )
        .await;
        assert_eq!(status, 200, "{page}");
        ids.push(
            page["items"][0]["position"]["id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
        let Some(cursor) = page["next_cursor"].as_str() else {
            break;
        };
        after = format!("&after={cursor}");
        assert!(ids.len() < 4);
    }
    assert_eq!(
        ids,
        vec![
            positions[1].id.to_string(),
            positions[0].id.to_string(),
            positions[2].id.to_string()
        ]
    );
}

async fn browser_page(app: axum::Router, filters: &str, query: &str) -> Value {
    let (status, page) = call(
        app,
        "GET",
        &format!("/api/position-browser?filters={filters}&limit=2{query}"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{page}");
    page
}

#[sqlx::test(migrations = "./migrations")]
async fn numbered_pages_count_and_order_the_same_hits_as_cursor_pages(pool: PgPool) {
    let (app, original, positions) = setup(&pool).await;
    let database = Uuid::new_v4();
    sqlx::query("INSERT INTO databases(id,name) VALUES($1,'Second')")
        .bind(database)
        .execute(&pool)
        .await
        .unwrap();
    let copy = store::import_match_in_database(
        &pool,
        "copy.sgf",
        "sgf",
        SOURCE.as_bytes(),
        SOURCE,
        &import::parse_sgf(SOURCE).unwrap(),
        database,
    )
    .await
    .unwrap()
    .r#match
    .id;
    save_match(&pool, copy).await;
    let copied: Vec<Position> =
        sqlx::query_as("SELECT * FROM positions WHERE match_uuid=$1 ORDER BY ordinal")
            .bind(copy)
            .fetch_all(&pool)
            .await
            .unwrap();
    native(&pool, original, &positions[0], 2, 0.04, "completed").await;
    native(&pool, original, &positions[1], 2, 0.16, "completed").await;
    native(&pool, copy, &copied[2], 2, 0.3, "completed").await;
    for (fields, total) in [
        (json!({}), 6),
        (json!({"sort":"loss"}), 6),
        (json!({"sort":"loss","direction":"asc"}), 6),
        (json!({"database_id":database}), 3),
        (json!({"database_id":database,"sort":"loss"}), 3),
        (json!({"analysis_state":"ready"}), 3),
        (json!({"analysis_state":"missing","sort":"loss"}), 3),
        (json!({"min_loss":0.1,"sort":"ply"}), 2),
    ] {
        let filters = encoded(&definition(fields.clone()));
        let mut cursor_items = vec![];
        let mut after = String::new();
        loop {
            let page = browser_page(app.clone(), &filters, &after).await;
            assert!(page.get("total").is_none(), "{page}");
            cursor_items.extend(page["items"].as_array().unwrap().iter().cloned());
            let Some(cursor) = page["next_cursor"].as_str() else {
                break;
            };
            after = format!("&after={cursor}");
            assert!(cursor_items.len() < 8, "{fields}");
        }
        assert_eq!(cursor_items.len(), total, "{fields}");
        let mut numbered = vec![];
        for number in 1..=total.div_ceil(2) {
            let page = browser_page(app.clone(), &filters, &format!("&page={number}")).await;
            assert_eq!(page["total"], total, "{fields}");
            let items = page["items"].as_array().unwrap();
            assert_eq!(items.len(), 2.min(total - numbered.len()), "{fields}");
            numbered.extend(items.iter().cloned());
            assert_eq!(
                page["next_cursor"].is_string(),
                numbered.len() < total,
                "{fields}"
            );
        }
        assert_eq!(numbered, cursor_items, "{fields}");
        let past = browser_page(
            app.clone(),
            &filters,
            &format!("&page={}", total.div_ceil(2) + 1),
        )
        .await;
        assert_eq!(past["items"], json!([]), "{fields}");
        assert_eq!(past["total"], total, "{fields}");
        assert!(past["next_cursor"].is_null(), "{fields}");
    }
    let losses: Vec<Value> = browser_page(
        app.clone(),
        &encoded(&definition(json!({"sort":"loss"}))),
        "&page=1",
    )
    .await["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["native_error"]["equity_loss"].clone())
        .collect();
    assert_eq!(losses, vec![json!(0.3), json!(0.16)]);
    let scoped = browser_page(
        app,
        &encoded(&definition(json!({"database_id":database,"sort":"loss"}))),
        "&page=1",
    )
    .await;
    assert_eq!(
        scoped["items"][0]["position"]["id"],
        copied[2].id.to_string()
    );
    assert_eq!(scoped["items"][0]["database_id"], database.to_string());
    assert_eq!(scoped["items"][0]["card_ready"], true);
    assert!(scoped["items"][1]["native_error"].is_null());
    assert_eq!(scoped["items"][1]["card_ready"], false);
}

#[sqlx::test(migrations = "./migrations")]
async fn generated_position_context_works_with_the_restore_search_path(pool: PgPool) {
    let (_, _, positions) = setup(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL search_path = ''")
        .execute(&mut *tx)
        .await
        .unwrap();
    type GeneratedContext = (
        Option<i32>,
        Option<i32>,
        Option<bool>,
        Option<i32>,
        Option<i32>,
    );
    let context: GeneratedContext = sqlx::query_as(
        "UPDATE public.positions SET gnubg_match_id='QYkGAAAAAAAA' WHERE id=$1
             RETURNING source_white_score, source_black_score, source_crawford,
                       source_cube_value, source_cube_owner",
    )
    .bind(positions[0].id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(context, (Some(0), Some(0), Some(false), Some(2), Some(0)));
}

#[sqlx::test(migrations = "./migrations")]
async fn malformed_legacy_ids_remain_unknown_and_saved_filter_count_is_bounded(pool: PgPool) {
    let (app, _, positions) = setup(&pool).await;
    sqlx::query("UPDATE positions SET gnubg_match_id='legacy-dummy' WHERE id=$1")
        .bind(positions[0].id)
        .execute(&pool)
        .await
        .unwrap();
    let found = search(app.clone(), json!({})).await;
    assert_eq!(found["items"][0]["cube_value"], Value::Null);
    assert_eq!(found["items"][0]["white_score"], Value::Null);
    let context: Option<Vec<i32>> =
        sqlx::query_scalar("SELECT position_browser_context('AAAAAAAAAAAA')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(context.is_none());
    sqlx::query("INSERT INTO position_filters(id,name,definition) SELECT gen_random_uuid(),'saved '||n,'{\"schema_version\":1}'::jsonb FROM generate_series(1,200) n").execute(&pool).await.unwrap();
    let (status, error) = call(
        app,
        "POST",
        "/api/position-filters",
        Some(json!({"name":"one too many","definition":definition(json!({}))})),
    )
    .await;
    assert_eq!(status, 422, "{error}");
    assert!(error["error"].as_str().unwrap().contains("200"));
}

#[sqlx::test(migrations = "./migrations")]
async fn readiness_keeps_stale_explicit_hint_unavailable_without_using_older_answer(pool: PgPool) {
    let (app, match_id, positions) = setup(&pool).await;
    for (plies, candidates) in [
        (
            1,
            json!([{"rank":1,"notation":"best","equity":0.1,"probabilities":[]}]),
        ),
        (2, json!([])),
    ] {
        let job = Uuid::new_v4();
        sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result) VALUES($1,$2,'analysis',$3,'completed',$4)")
            .bind(job).bind(positions[0].id).bind(plies)
            .bind(json!({"engine":"GNU Backgammon","plies":plies,"candidates":candidates}))
            .execute(&pool).await.unwrap();
        if plies == 2 {
            sqlx::query("INSERT INTO analysis_selections(match_id,analysis_id) VALUES($1,$2)")
                .bind(match_id)
                .bind(format!("job:{job}"))
                .execute(&pool)
                .await
                .unwrap();
        }
    }
    let found = search(app.clone(), json!({})).await;
    assert_eq!(found["items"][0]["card_ready"], false);
    assert_eq!(
        search(app.clone(), json!({"analysis_state":"ready"})).await["items"],
        json!([])
    );
    assert_eq!(
        search(app, json!({"analysis_state":"missing"})).await["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn an_empty_selected_individual_hint_blocks_match_solution_fallback(pool: PgPool) {
    let (app, id, positions) = setup(&pool).await;
    native(&pool, id, &positions[0], 2, 0.08, "completed").await;
    let selected = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result) VALUES($1,$2,'analysis',2,'completed',$3)")
        .bind(selected).bind(positions[0].id)
        .bind(json!({"engine":"GNU Backgammon","plies":2,"candidates":[]}))
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO analysis_selections(match_id,analysis_id) VALUES($1,$2)")
        .bind(id)
        .bind(format!("job:{selected}"))
        .execute(&pool)
        .await
        .unwrap();
    let found = search(app.clone(), json!({})).await;
    assert!(found["items"][0]["native_error"].is_null());
    assert_eq!(found["items"][0]["card_ready"], false);
    assert_eq!(
        search(app, json!({"analysis_state":"ready"})).await["items"],
        json!([])
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn unavailable_selected_native_decisions_do_not_become_zero_loss_or_fall_back_to_old_errors(
    pool: PgPool,
) {
    let (app, id, positions) = setup(&pool).await;
    native(&pool, id, &positions[0], 1, 0.08, "completed").await;
    let unknown = native(&pool, id, &positions[0], 3, 0.0, "completed").await;
    sqlx::query("UPDATE move_analyses SET data=$2 WHERE job_id=$1").bind(unknown)
        .bind(json!({"best":"","evaluation_status":"unavailable","played_equity":null,"best_equity":null,"equity_loss":0.0}))
        .execute(&pool).await.unwrap();
    let found = search(app.clone(), json!({})).await;
    assert_eq!(found["items"][0]["native_error"], Value::Null);
    assert_eq!(found["items"][0]["card_ready"], false);
    assert_eq!(
        search(app.clone(), json!({"analysis_state":"ready"})).await["items"],
        json!([])
    );
    assert_eq!(
        search(app, json!({"max_loss":0.01})).await["items"],
        json!([])
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn forced_zero_loss_is_ready_but_missing_native_equities_or_blank_answers_are_not(
    pool: PgPool,
) {
    let (app, id, positions) = setup(&pool).await;
    let job = native(&pool, id, &positions[0], 2, 0.0, "completed").await;
    let data = json!({"best":"Cannot move","evaluation_status":"forced","played_equity":null,"best_equity":null,"equity_loss":0.0});
    sqlx::query("UPDATE move_analyses SET data=$2 WHERE job_id=$1")
        .bind(job)
        .bind(data.clone())
        .execute(&pool)
        .await
        .unwrap();
    let found = search(app.clone(), json!({"analysis_state":"ready"})).await;
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    assert_eq!(found["items"][0]["native_error"]["equity_loss"], 0.0);
    for invalid in [
        json!({"best":"best","evaluation_status":"evaluated","played_equity":null,"best_equity":null,"equity_loss":0.0}),
        json!({"best":"   ","evaluation_status":"evaluated","played_equity":0.0,"best_equity":0.0,"equity_loss":0.0}),
        json!({"best":"Cannot move","evaluation_status":"forced","played_equity":null,"best_equity":null,"equity_loss":0.2}),
    ] {
        sqlx::query("UPDATE move_analyses SET data=$2 WHERE job_id=$1")
            .bind(job)
            .bind(invalid)
            .execute(&pool)
            .await
            .unwrap();
        let found = search(app.clone(), json!({})).await;
        assert_eq!(found["items"][0]["native_error"], Value::Null);
        assert_eq!(found["items"][0]["card_ready"], false);
    }
    sqlx::query("UPDATE move_analyses SET data=$2 WHERE job_id=$1")
        .bind(job)
        .bind(data)
        .execute(&pool)
        .await
        .unwrap();
    let selected = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result) VALUES($1,$2,'analysis',2,'completed',$3)")
        .bind(selected).bind(positions[0].id)
        .bind(json!({"engine":"GNU Backgammon","plies":2,"candidates":[{"notation":"   ","equity":0.0,"rank":1,"probabilities":[]}]}))
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO analysis_selections(match_id,analysis_id) VALUES($1,$2)")
        .bind(id)
        .bind(format!("job:{selected}"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        search(app, json!({"analysis_state":"ready"})).await["items"],
        json!([])
    );
}

async fn table_page(app: axum::Router, fields: &Value, limit: usize, after: &str) -> (u16, Value) {
    call(
        app,
        "GET",
        &format!(
            "/api/position-browser?filters={}&limit={limit}{after}",
            encoded(&definition(fields.clone()))
        ),
        None,
    )
    .await
}

async fn table_ids(app: axum::Router, fields: &Value, limit: usize) -> Vec<String> {
    let mut ids = vec![];
    let mut after = String::new();
    loop {
        let (status, page) = table_page(app.clone(), fields, limit, &after).await;
        assert_eq!(status, 200, "{fields}: {page}");
        ids.extend(
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|hit| hit["position"]["id"].as_str().unwrap().to_owned()),
        );
        let Some(cursor) = page["next_cursor"].as_str() else {
            break;
        };
        assert!(
            cursor.len() < 512,
            "unbounded table cursor: {}",
            cursor.len()
        );
        assert!(ids.len() < 30, "pagination did not terminate: {fields}");
        after = format!("&after={cursor}");
    }
    ids
}

#[derive(Debug, PartialEq, PartialOrd)]
enum TableKey {
    Number(i64),
    Loss(f64),
    Text(String),
    Pair(i64, i64),
    Tags(Vec<String>),
}

fn table_key(hit: &Value, sort: &str) -> Option<TableKey> {
    let p = &hit["position"];
    let text = |v: &Value| {
        v.as_str()
            .filter(|v| !v.trim().is_empty())
            .map(|v| TableKey::Text(v.to_lowercase()))
    };
    match sort {
        "match" => text(&hit["display_name"]),
        "match_id" => hit["match_no"].as_i64().map(TableKey::Number),
        "database" => text(&hit["database_name"]),
        "game" => p["game_no"].as_i64().map(TableKey::Number),
        "ply" => p["ply"].as_i64().map(TableKey::Number),
        "player" | "opponent" => {
            let white = (p["player"] == 0) == (sort == "player");
            text(&hit["match_info"][if white { "white" } else { "black" }])
        }
        "dice" => {
            let a = p["dice"][0].as_i64()?;
            let b = p["dice"][1].as_i64()?;
            Some(TableKey::Pair(a.min(b), a.max(b)))
        }
        "score" => Some(TableKey::Pair(
            hit["white_score"].as_i64()?,
            hit["black_score"].as_i64()?,
        )),
        "cube" => hit["cube_value"].as_i64().map(TableKey::Number),
        "played_move" => text(&p["notation"]),
        "tags" => {
            let tags: Vec<_> = hit["tags"]
                .as_array()?
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect();
            (!tags.is_empty()).then_some(TableKey::Tags(tags))
        }
        "comment" => {
            let comments: Vec<_> = hit["notes"]
                .as_array()?
                .iter()
                .filter_map(|n| {
                    n["comment_excerpt"]
                        .as_str()
                        .filter(|c| !c.trim().is_empty())
                        .map(str::to_lowercase)
                })
                .collect();
            (!comments.is_empty()).then(|| TableKey::Text(comments.join("\n")))
        }
        "loss" => hit["native_error"]["equity_loss"]
            .as_f64()
            .map(TableKey::Loss),
        _ => panic!("uncovered table sort: {sort}"),
    }
}

fn table_tie(hit: &Value) -> (String, String, i64, String) {
    (
        hit["display_name"].as_str().unwrap().to_lowercase(),
        hit["match_info"]["id"].as_str().unwrap().to_owned(),
        hit["position"]["ordinal"].as_i64().unwrap(),
        hit["position"]["id"].as_str().unwrap().to_owned(),
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn table_columns_sort_both_directions_with_nulls_last_ties_and_keyset_pages(pool: PgPool) {
    let (app, first_match, first) = setup(&pool).await;
    let database = Uuid::new_v4();
    sqlx::query("INSERT INTO databases(id,name) VALUES($1,'Alpha')")
        .bind(database)
        .execute(&pool)
        .await
        .unwrap();
    let other = SOURCE
        .replace("Alice", "Zelda")
        .replace("Bob", "Adam")
        .replace("length:7", "length:15")
        .replace("ws:6", "ws:10");
    let second_match = store::import_match_in_database(
        &pool,
        "second.sgf",
        "sgf",
        other.as_bytes(),
        &other,
        &import::parse_sgf(&other).unwrap(),
        database,
    )
    .await
    .unwrap()
    .r#match
    .id;
    save_match(&pool, second_match).await;
    neondojo::move_catalog::ensure_catalog(&pool, second_match)
        .await
        .unwrap();
    let second: Vec<Position> =
        sqlx::query_as("SELECT * FROM positions WHERE match_uuid=$1 ORDER BY ordinal")
            .bind(second_match)
            .fetch_all(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE matches SET display_name='Z match' WHERE id=$1")
        .bind(first_match)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE matches SET display_name='A match' WHERE id=$1")
        .bind(second_match)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE positions SET gnubg_match_id='legacy-unknown',notation='' WHERE id=$1")
        .bind(second[1].id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE matches SET black='' WHERE id=$1")
        .bind(second_match)
        .execute(&pool)
        .await
        .unwrap();
    native(&pool, first_match, &first[0], 1, 0.04, "completed").await;
    native(&pool, first_match, &first[1], 1, 0.16, "completed").await;
    native(&pool, second_match, &second[0], 1, 0.04, "completed").await;
    note(&pool, "position_uuid", first[0].id, "Zulu comment").await;
    note(&pool, "position_uuid", second[0].id, "Alpha comment").await;
    sqlx::query("UPDATE annotations SET tags=ARRAY['zebra','zulu'] WHERE position_uuid=$1")
        .bind(first[0].id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE annotations SET tags=ARRAY['alpha'] WHERE position_uuid=$1")
        .bind(second[0].id)
        .execute(&pool)
        .await
        .unwrap();
    let all = search(app.clone(), json!({})).await["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(all.len(), 6);
    for sort in [
        "match",
        "loss",
        "match_id",
        "database",
        "game",
        "ply",
        "player",
        "opponent",
        "dice",
        "score",
        "cube",
        "played_move",
        "tags",
        "comment",
    ] {
        for direction in ["asc", "desc"] {
            let mut expected = all.clone();
            expected.sort_by(|a, b| {
                let order = match (table_key(a, sort), table_key(b, sort)) {
                    (Some(a), Some(b)) => {
                        let order = a.partial_cmp(&b).unwrap();
                        if direction == "desc" {
                            order.reverse()
                        } else {
                            order
                        }
                    }
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                };
                order.then_with(|| table_tie(a).cmp(&table_tie(b)))
            });
            let expected: Vec<_> = expected
                .iter()
                .map(|h| h["position"]["id"].as_str().unwrap().to_owned())
                .collect();
            // Single-item pages cross every keyset boundary; one large page
            // checks the complete order without a cursor.
            for limit in [1, 100] {
                assert_eq!(
                    table_ids(
                        app.clone(),
                        &json!({"sort":sort,"direction":direction}),
                        limit
                    )
                    .await,
                    expected,
                    "{sort} {direction} page size {limit}"
                );
            }
        }
    }
    for sort in [
        "match_id",
        "database",
        "game",
        "ply",
        "player",
        "opponent",
        "dice",
        "score",
        "cube",
        "played_move",
        "tags",
        "comment",
    ] {
        assert_eq!(
            table_ids(app.clone(), &json!({"sort":sort}), 1).await,
            table_ids(app.clone(), &json!({"sort":sort,"direction":"asc"}), 1).await
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn table_hit_keeps_numeric_match_number_across_metadata_and_database_changes(pool: PgPool) {
    let (app, id, _) = setup(&pool).await;
    let no: i64 = sqlx::query_scalar("SELECT match_no FROM matches WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(no > 0);
    let found = search(app.clone(), json!({})).await;
    assert_eq!(found["items"][0]["match_no"], no);
    assert_eq!(found["items"][0]["match_info"]["id"], id.to_string());
    let database = Uuid::new_v4();
    sqlx::query("INSERT INTO databases(id,name) VALUES($1,'Moved')")
        .bind(database)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE matches SET database_id=$2,display_name='Renamed' WHERE id=$1")
        .bind(id)
        .bind(database)
        .execute(&pool)
        .await
        .unwrap();
    let found = search(app, json!({"sort":"match_id"})).await;
    assert!(
        found["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|h| h["match_no"] == no && h["match_info"]["id"] == id.to_string())
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn table_cursors_reject_wrong_direction_forged_unfiltered_or_deleted_anchors(pool: PgPool) {
    let (app, _, positions) = setup(&pool).await;
    let fields = json!({"sort":"tags","direction":"asc","player":"Alice"});
    let (status, page) = table_page(app.clone(), &fields, 1, "").await;
    assert_eq!(status, 200, "{page}");
    let cursor = page["next_cursor"].as_str().unwrap();
    let decoded: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(cursor).unwrap()).unwrap();
    let after = format!("&after={cursor}");
    let (status, saved) = call(
        app.clone(),
        "POST",
        "/api/position-filters",
        Some(json!({"name":"Directed table", "definition":definition(fields.clone())})),
    )
    .await;
    assert_eq!(status, 201, "{saved}");
    assert_eq!(saved["definition"]["direction"], "asc");
    assert_eq!(
        table_page(app.clone(), &saved["definition"], 1, &after)
            .await
            .0,
        200
    );
    for wrong in [
        json!({"sort":"tags","direction":"desc","player":"Alice"}),
        json!({"sort":"comment","direction":"asc","player":"Alice"}),
        json!({"sort":"tags","direction":"asc","player":"Bob"}),
    ] {
        assert_eq!(table_page(app.clone(), &wrong, 1, &after).await.0, 422);
    }
    for anchor in [positions[1].id, Uuid::new_v4()] {
        let mut forged = decoded.clone();
        forged["anchor"] = json!(anchor);
        let forged = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&forged).unwrap());
        assert_eq!(
            table_page(app.clone(), &fields, 1, &format!("&after={forged}"))
                .await
                .0,
            422
        );
    }
    let mut exhausted = decoded.clone();
    exhausted["anchor"] = json!(positions[2].id);
    let exhausted = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&exhausted).unwrap());
    let (status, empty) = table_page(app.clone(), &fields, 1, &format!("&after={exhausted}")).await;
    assert_eq!(status, 200, "{empty}");
    assert_eq!(empty["items"], json!([]));
    assert_eq!(empty["next_cursor"], Value::Null);
    let mut extra = decoded.clone();
    extra["sort_value"] = json!("invented");
    let extra = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&extra).unwrap());
    assert_eq!(
        table_page(app.clone(), &fields, 1, &format!("&after={extra}"))
            .await
            .0,
        422
    );
    let anchor = page["items"][0]["position"]["id"].as_str().unwrap();
    sqlx::query("UPDATE positions SET player=1 WHERE id=$1")
        .bind(Uuid::parse_str(anchor).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(table_page(app.clone(), &fields, 1, &after).await.0, 422);
    sqlx::query("DELETE FROM positions WHERE id=$1")
        .bind(Uuid::parse_str(anchor).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(table_page(app, &fields, 1, &after).await.0, 422);
}

#[sqlx::test(migrations = "./migrations")]
async fn table_note_sort_keys_use_full_scoped_content_and_keep_cursors_bounded(pool: PgPool) {
    let (app, id, positions) = setup(&pool).await;
    let common = "x".repeat(19_999);
    for (p, suffix) in [(&positions[0], "b"), (&positions[1], "a")] {
        note(&pool, "position_uuid", p.id, &format!("{common}{suffix}")).await;
        let tags: Vec<_> = (0..30)
            .map(|n| format!("{n:02}{}", "t".repeat(62)))
            .collect();
        sqlx::query("UPDATE annotations SET tags=$2 WHERE position_uuid=$1")
            .bind(p.id)
            .bind(tags)
            .execute(&pool)
            .await
            .unwrap();
    }
    let cube: Uuid = sqlx::query_scalar(
        "SELECT id FROM match_moves WHERE match_uuid=$1 AND kind='cube' LIMIT 1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    note(&pool, "move_uuid", cube, "AAA unrelated cube").await;
    sqlx::query("UPDATE annotations SET tags=ARRAY['aaa-unrelated-cube'] WHERE move_uuid=$1")
        .bind(cube)
        .execute(&pool)
        .await
        .unwrap();
    let ascending = table_ids(app.clone(), &json!({"sort":"comment","direction":"asc"}), 1).await;
    assert_eq!(
        ascending,
        vec![
            positions[1].id.to_string(),
            positions[0].id.to_string(),
            positions[2].id.to_string()
        ]
    );
    let descending = table_ids(
        app.clone(),
        &json!({"sort":"comment","direction":"desc"}),
        1,
    )
    .await;
    assert_eq!(
        descending,
        vec![
            positions[0].id.to_string(),
            positions[1].id.to_string(),
            positions[2].id.to_string()
        ]
    );
    let found = search(app.clone(), json!({"sort":"tags","direction":"asc"})).await;
    assert!(found["items"].as_array().unwrap().iter().all(|h| {
        !h["tags"]
            .as_array()
            .unwrap()
            .contains(&json!("aaa-unrelated-cube"))
    }));
    assert!(
        found["items"][0]["notes"][0]["comment_excerpt"]
            .as_str()
            .unwrap()
            .len()
            <= 280
    );
    assert_eq!(
        table_ids(app, &json!({"sort":"tags","direction":"desc"}), 1)
            .await
            .len(),
        3
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn mochy_player_filter_matches_on_roll_in_both_colors_without_opponent_leaks(pool: PgPool) {
    let app = api::router(AppState::new(pool.clone(), Engine::new("unused".into())));
    let mut expected = vec![];
    let mut exact = None;
    for (n, (white, black, turn, include)) in [
        ("Mochy", "Opponent A", "W", true),
        ("Opponent B", "Mochy", "B", true),
        ("Mochy", "Opponent C", "B", false),
        ("Opponent D", "Mochy", "W", false),
    ]
    .into_iter()
    .enumerate()
    {
        let moves = if turn == "W" { "hefe" } else { "qtst" };
        let source = format!(
            "(;GM[6]FF[4]PW[{white}]PB[{black}]MI[length:7][ws:0][bs:0];{turn}[31{moves}])"
        );
        let imported = store::import_match(
            &pool,
            &format!("mochy-{n}.sgf"),
            "sgf",
            source.as_bytes(),
            &source,
            &import::parse_sgf(&source).unwrap(),
        )
        .await
        .unwrap();
        save_match(&pool, imported.r#match.id).await;
        let position: Position = sqlx::query_as("SELECT * FROM positions WHERE match_uuid=$1")
            .bind(imported.r#match.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        if let Some(id) = &exact {
            assert_eq!(&position.position_id, id)
        } else {
            exact = Some(position.position_id.clone())
        }
        if include {
            expected.push(position.id.to_string())
        }
    }
    let result = search(app, json!({"player":"mochy","position_id":exact.unwrap(),"die1":3,"die2":1,"sort":"player","direction":"asc"})).await;
    let mut actual: Vec<_> = result["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["position"]["id"].as_str().unwrap().to_owned())
        .collect();
    expected.sort();
    actual.sort();
    assert_eq!(actual, expected);
}

#[test]
fn legacy_v1_saved_definition_serialization_preserves_the_original_definition_bytes() {
    use neondojo::position_browser::PositionFilterDefinition;
    for sort in ["match", "loss"] {
        let parsed: PositionFilterDefinition =
            serde_json::from_value(definition(json!({"sort":sort}))).unwrap();
        let expected = format!(
            r#"{{"schema_version":1,"database_id":null,"q":null,"player":null,"match_name":null,"tournament":null,"round":null,"tag":null,"comment":null,"position_id":null,"match_points_min":null,"match_points_max":null,"die1":null,"die2":null,"white_score":null,"black_score":null,"crawford":null,"cube_value":null,"analysis_state":null,"min_loss":null,"max_loss":null,"sort":"{sort}"}}"#
        );
        assert_eq!(serde_json::to_string(&parsed).unwrap(), expected);
        let mut with_direction = definition(json!({"sort":sort,"direction":"asc"}));
        let directed: PositionFilterDefinition =
            serde_json::from_value(with_direction.clone()).expect("optional direction accepted");
        assert_eq!(serde_json::to_value(directed).unwrap()["direction"], "asc");
        with_direction["direction"] = json!("sideways");
        assert!(serde_json::from_value::<PositionFilterDefinition>(with_direction).is_err());
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn global_cursors_are_invalidated_while_v1_saved_definitions_still_round_trip(pool: PgPool) {
    use neondojo::position_browser::PositionFilterDefinition;
    let (app, id, positions) = setup(&pool).await;
    native(&pool, id, &positions[0], 1, 0.04, "completed").await;
    native(&pool, id, &positions[1], 1, 0.16, "completed").await;
    for sort in ["match", "loss"] {
        let fields = json!({"sort":sort});
        let parsed: PositionFilterDefinition =
            serde_json::from_value(definition(fields.clone())).unwrap();
        let scope = format!("{:x}", Sha256::digest(serde_json::to_vec(&parsed).unwrap()));
        let (_, first) = table_page(app.clone(), &fields, 1, "").await;
        let item = &first["items"][0];
        let old = json!({"scope":scope,"name":"fixture_100%.sgf","match_id":id,"ordinal":item["position"]["ordinal"],"id":item["position"]["id"],"loss":item["native_error"]["equity_loss"]});
        let cursor = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&old).unwrap());
        let (status, rejected) =
            table_page(app.clone(), &fields, 1, &format!("&after={cursor}")).await;
        assert_eq!(status, 422, "Global cursor must be invalidated: {rejected}");
        let current_cursor = first["next_cursor"].as_str().unwrap();
        let (status, second) =
            table_page(app.clone(), &fields, 1, &format!("&after={current_cursor}")).await;
        assert_eq!(status, 200, "{second}");
        assert_ne!(second["items"][0]["position"]["id"], item["position"]["id"]);
        let generated: Value = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(first["next_cursor"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_ne!(generated["scope"], old["scope"]);
        assert!(
            generated["scope"]
                .as_str()
                .unwrap()
                .starts_with("saved-positions-v1:")
        );
        let mut expected = old.clone();
        expected["scope"] = generated["scope"].clone();
        assert_eq!(generated, expected);
        let (status, saved) = call(
            app.clone(),
            "POST",
            "/api/position-filters",
            Some(json!({"name":format!("legacy {sort}"),"definition":definition(fields.clone())})),
        )
        .await;
        assert_eq!(status, 201, "{saved}");
        assert!(saved["definition"].get("direction").is_none());
        let (_, restored) = table_page(
            app.clone(),
            &saved["definition"],
            1,
            &format!("&after={current_cursor}"),
        )
        .await;
        assert_eq!(restored["items"], second["items"]);
    }
}

#[test]
fn game_plan_filters_round_trip_without_changing_legacy_serialization() {
    use neondojo::position_browser::PositionFilterDefinition;
    let legacy: PositionFilterDefinition =
        serde_json::from_value(json!({"schema_version":1})).unwrap();
    assert!(
        serde_json::to_value(legacy)
            .unwrap()
            .get("game_plan")
            .is_none()
    );
    let filter: PositionFilterDefinition =
        serde_json::from_value(json!({"schema_version":1,"game_plan":"priming"})).unwrap();
    assert_eq!(
        serde_json::to_value(filter).unwrap()["game_plan"],
        "priming"
    );
    assert!(
        serde_json::from_value::<PositionFilterDefinition>(
            json!({"schema_version":1,"game_plan":"unknown"})
        )
        .is_err()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn game_plan_filter_uses_the_player_on_roll_and_precedes_pagination(pool: PgPool) {
    let (app, _, positions) = setup(&pool).await;
    for plan in ["pure_race", "racing", "attacking", "priming", "anchoring"] {
        let expected: Vec<_> = positions
            .iter()
            .filter(|p| {
                let plans =
                    neondojo::game_plan::classify(p.board.as_slice().try_into().unwrap()).unwrap();
                serde_json::to_value(if p.player == 0 {
                    plans.white
                } else {
                    plans.black
                })
                .unwrap()
                    == plan
            })
            .map(|p| p.id)
            .collect();
        let found = search(app.clone(), json!({"game_plan":plan})).await;
        let actual: Vec<Uuid> = found["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["position"]["id"].as_str().unwrap().parse().unwrap())
            .collect();
        assert_eq!(actual, expected, "{plan}");
    }
    let (status, page) = call(app.clone(), "GET", "/api/position-browser?limit=1", None).await;
    assert_eq!(status, 200);
    let cursor = page["next_cursor"].as_str().unwrap();
    let (status, _) = call(
        app,
        "GET",
        &format!(
            "/api/position-browser?limit=1&after={cursor}&filters={}",
            encoded(&json!({"schema_version":1,"game_plan":"priming"}))
        ),
        None,
    )
    .await;
    assert_eq!(status, 422);
    let cached: i64 = sqlx::query_scalar("SELECT count(*) FROM position_game_plans")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cached, positions.len() as i64);
}

#[sqlx::test(migrations = "./migrations")]
async fn game_plan_cache_is_versioned_and_retained_positions_remain_searchable(pool: PgPool) {
    let (app, id, positions) = setup(&pool).await;
    // Past the opening, so the pinned rules decide the labels.
    let board = vec![
        0, -1, 1, 1, 1, 1, 5, -1, 1, 1, 0, 0, -2, 2, 0, 0, 0, -3, -2, -4, -1, 0, 0, -1, 2, 0,
    ];
    for position in &positions[..2] {
        sqlx::query("UPDATE positions SET board=$2 WHERE id=$1")
            .bind(position.id)
            .bind(&board)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO position_game_plans(position_uuid,classifier,game_plan) VALUES($1,'old_rules','pure_race')").bind(position.id).execute(&pool).await.unwrap();
    }
    // Simulate a classifier version change on the edited fixture boards.
    sqlx::query("DELETE FROM position_game_plans WHERE classifier=$1")
        .bind(neondojo::game_plan::CLASSIFIER)
        .execute(&pool)
        .await
        .unwrap();
    neondojo::browse_indexes::backfill_batch(&pool, &mut Default::default())
        .await
        .unwrap();
    let white = search(app.clone(), json!({"game_plan":"attacking"})).await;
    assert!(
        white["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["position"]["id"] == positions[0].id.to_string())
    );
    assert!(
        !white["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["position"]["id"] == positions[1].id.to_string())
    );
    let black = search(app.clone(), json!({"game_plan":"priming"})).await;
    assert!(
        black["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["position"]["id"] == positions[1].id.to_string())
    );
    let (status, result) = call(
        app.clone(),
        "POST",
        "/api/library/actions",
        Some(json!({"action":"delete","match_ids":[id]})),
    )
    .await;
    assert_eq!(status, 200, "{result}");
    let after = search(app, json!({"game_plan":"attacking"})).await;
    assert_eq!(
        after["items"][0]["position"]["id"],
        positions[0].id.to_string()
    );
    assert_eq!(after["items"][0]["source_available"], false);
    let cached: i64 =
        sqlx::query_scalar("SELECT count(*) FROM position_game_plans WHERE classifier=$1")
            .bind(neondojo::game_plan::CLASSIFIER)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cached, positions.len() as i64);
}

#[sqlx::test(migrations = "./migrations")]
async fn large_game_plan_backfill_advances_in_background_without_browsing(pool: PgPool) {
    let (app, _, positions) = setup(&pool).await;
    sqlx::query("INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation) SELECT gen_random_uuid(),p.match_uuid,100+n,p.game_no,p.ply,p.player,p.dice,p.board,p.position_id,p.gnubg_match_id,p.notation FROM positions p CROSS JOIN generate_series(1,4100) n WHERE p.id=$1").bind(positions[0].id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO saved_positions(position_uuid) SELECT id FROM positions ON CONFLICT DO NOTHING").execute(&pool).await.unwrap();
    let path = format!(
        "/api/position-browser?filters={}",
        encoded(&json!({"schema_version":1,"game_plan":"priming"}))
    );
    let (status, error) = call(app.clone(), "GET", &path, None).await;
    assert_eq!(status, 503, "{error}");
    assert_eq!(error["code"], "index_preparing");
    let mut cursor = Default::default();
    assert!(
        neondojo::browse_indexes::backfill_batch(&pool, &mut cursor)
            .await
            .unwrap()
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM position_game_plans")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, positions.len() as i64 + 512);
    while neondojo::browse_indexes::backfill_batch(&pool, &mut cursor)
        .await
        .unwrap()
    {}
    assert_eq!(call(app, "GET", &path, None).await.0, 200);
}

#[sqlx::test(migrations = "./migrations")]
async fn readiness_agrees_with_transfer_after_match_source_revision_changes(pool: PgPool) {
    let (app, match_id, positions) = setup(&pool).await;
    let id = positions[0].id;
    native(&pool, match_id, &positions[0], 2, 0.09, "completed").await;
    sqlx::query("UPDATE matches SET source_revision=source_revision+1 WHERE id=$1")
        .bind(match_id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, readiness) = call(
        app.clone(),
        "POST",
        "/api/position-browser/analysis-readiness",
        Some(json!({"position_ids":[id]})),
    )
    .await;
    assert_eq!(status, 200, "{readiness}");
    assert_eq!(readiness["analysis_required"], 1);
    let (status, deck) = call(
        app.clone(),
        "POST",
        "/api/decks",
        Some(json!({"name":"Review probe"})),
    )
    .await;
    assert!(status < 300, "{deck}");
    let (status, transfer) = call(
        app,
        "POST",
        &format!("/api/decks/{}/positions", deck["id"].as_str().unwrap()),
        Some(json!({"position_ids":[id]})),
    )
    .await;
    assert_eq!(status, 200, "{transfer}");
    assert_eq!(
        transfer["items"][0]["status"], "analysis_required",
        "{transfer}"
    );
}
