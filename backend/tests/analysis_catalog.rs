use axum::{body::Body, http::Request};
use bgtutor::{api, engine::Engine, import, model::Job, store};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SOURCE: &str = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])\n(;GM[6]FF[4]MI[length:7][ws:1][bs:0];W[31hefe];B[42qusu])";

async fn setup(pool: &PgPool) -> (axum::Router, Uuid, Vec<Uuid>) {
    let id = store::import_match(
        pool,
        "catalog.sgf",
        "sgf",
        SOURCE.as_bytes(),
        SOURCE,
        &import::parse_sgf(SOURCE).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    let positions =
        sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 ORDER BY ordinal")
            .bind(id)
            .fetch_all(pool)
            .await
            .unwrap();
    (
        api::router(api::AppState::new(
            pool.clone(),
            Engine::new("gnubg".into()),
        )),
        id,
        positions,
    )
}
async fn request(app: &axum::Router, method: &str, uri: &str, body: Value) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
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
async fn get(app: &axum::Router, uri: &str) -> (u16, Value) {
    request(app, "GET", uri, Value::Null).await
}
async fn select(app: &axum::Router, id: Uuid, key: Option<&str>) -> (u16, Value) {
    request(
        app,
        "PUT",
        &format!("/api/matches/{id}/analysis-selection"),
        json!({"analysis_id":key}),
    )
    .await
}
async fn imported(pool: &PgPool, id: Uuid, position: Uuid) {
    let data = json!({"game_no":1,"source_move_no":1,"checker_ply":1,"kind":"checker","player":0,"played":"8/5 6/5","engine":"Foreign engine","engine_version":"1","method":"evaluation","depth":"2-ply","depth_plies":2,"candidates":[{"rank":1,"notation":"8/5 6/5","equity":0.123456789,"equity_kind":"mwc","probabilities":{"win":0.6},"depth":"2-ply","raw":null}],"cube":null,"raw":null});
    sqlx::query("INSERT INTO imported_analyses(match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data) VALUES($1,$2,0,1,1,'checker',$3)").bind(id).bind(position).bind(data).execute(pool).await.unwrap();
}
async fn match_job(
    pool: &PgPool,
    id: Uuid,
    position: Uuid,
    game: Option<i32>,
    engine: &str,
    plies: i32,
    status: &str,
) -> Job {
    let job =
        store::create_match_job_scoped(pool, id, plies, if game.is_some() { 1 } else { 2 }, game)
            .await
            .unwrap();
    let (notation, player, ply, game_no): (String, i32, i32, i32) =
        sqlx::query_as("SELECT notation,player,ply,game_no FROM positions WHERE id=$1")
            .bind(position)
            .fetch_one(pool)
            .await
            .unwrap();
    let decision = json!({"decision_no":ply,"checker_no":ply,"player":player,"kind":"checker","played":notation,"best":notation,"played_equity":0.42,"best_equity":0.42,"equity_loss":0.0,"severity":"none","dice":[3,1],"evaluation_status":"evaluated"});
    sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,$3,$4,$5,'checker',0,$6)").bind(job.id).bind(position).bind(game_no).bind(ply).bind(player).bind(decision).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,$2)")
        .bind(job.id)
        .bind(game_no)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET status=$2, completed_games=1,result=$3 WHERE id=$1")
        .bind(job.id)
        .bind(status)
        .bind(json!({"engine":engine}))
        .execute(pool)
        .await
        .unwrap();
    if status == "completed" && game.is_none() {
        sqlx::query(
            "INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,2) ON CONFLICT DO NOTHING",
        )
        .bind(job.id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("UPDATE jobs SET completed_games=total_games WHERE id=$1")
            .bind(job.id)
            .execute(pool)
            .await
            .unwrap();
    }
    store::get_job(pool, job.id).await.unwrap().unwrap()
}
async fn position_job(pool: &PgPool, position: Uuid, plies: i32) -> Job {
    let job = store::create_job(pool, position, "analysis", plies)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET status='completed',result=$2 WHERE id=$1").bind(job.id).bind(json!({"engine":"Position engine","plies":plies,"candidates":[{"rank":1,"notation":"8/5 6/5","equity":0.314159265,"probabilities":[0.6,0.1,0.01,0.1,0.01]}]})).execute(pool).await.unwrap();
    store::get_job(pool, job.id).await.unwrap().unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn deleting_a_saved_analysis_removes_only_that_result_and_clears_its_selection(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    imported(&pool, id, p[0]).await;
    let old = match_job(&pool, id, p[0], None, "GNU Backgammon", 2, "completed").await;
    let deeper = match_job(&pool, id, p[0], None, "GNU Backgammon", 4, "completed").await;
    let running = match_job(&pool, id, p[0], Some(1), "GNU Backgammon", 3, "running").await;
    let key = format!("job:{}", old.id);
    assert_eq!(select(&app, id, Some(&key)).await.0, 200);
    let delete_path = |key: &str, revision| {
        format!("/api/matches/{id}/analyses/{key}?source_revision={revision}")
    };
    assert_eq!(
        request(&app, "DELETE", &delete_path(&key, 99), Value::Null)
            .await
            .0,
        409
    );
    let running_key = format!("job:{}", running.id);
    assert_eq!(
        request(&app, "DELETE", &delete_path(&running_key, 0), Value::Null)
            .await
            .0,
        409
    );
    let (status, response) = request(&app, "DELETE", &delete_path(&key, 0), Value::Null).await;
    assert_eq!(status, 204, "{response}");
    assert_eq!(get(&app, &format!("/api/jobs/{}", old.id)).await.0, 404);
    let (_, catalog) = get(&app, &format!("/api/matches/{id}/analyses")).await;
    assert_eq!(catalog["source_revision"], 0);
    assert!(catalog["selected_id"].is_null());
    assert_eq!(catalog["selected"]["id"], format!("job:{}", deeper.id));
    assert_eq!(catalog["items"].as_array().unwrap().len(), 3);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM positions WHERE match_uuid=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        4
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM move_analyses WHERE job_id=$1")
            .bind(old.id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn deleting_imported_analysis_does_not_modify_the_match_source_or_restore_removed_values(
    pool: PgPool,
) {
    let source = include_str!("fixtures/analysis-game1.sgf");
    let id = store::import_match(
        &pool,
        "analyzed.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &import::parse_sgf(source).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("gnubg".into()),
    ));
    let key = format!("imported:{id}");
    assert_eq!(select(&app, id, Some(&key)).await.0, 200);
    let (status, response) = request(
        &app,
        "DELETE",
        &format!("/api/matches/{id}/analyses/{key}?source_revision=0"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 204, "{response}");
    let (_, catalog) = get(&app, &format!("/api/matches/{id}/analyses")).await;
    assert_eq!(catalog["items"], json!([]));
    assert!(catalog["selected_id"].is_null());
    let (original, sgf): (Vec<u8>, String) =
        sqlx::query_as("SELECT original,sgf FROM matches WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(original, source.as_bytes());
    assert_eq!(sgf, source);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn catalog_preserves_history_and_selected_entry_across_pages(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    imported(&pool, id, p[0]).await;
    let old = match_job(&pool, id, p[0], None, "GNU Backgammon", 4, "completed").await;
    let scoped = match_job(&pool, id, p[2], Some(2), "Open Sage", 2, "completed").await;
    let hint = position_job(&pool, p[0], 1).await;
    let key = format!("job:{}", old.id);
    assert_eq!(select(&app, id, Some(&key)).await.0, 200);
    let (status, first) = get(
        &app,
        &format!("/api/matches/{id}/analyses?limit=1&position_id={}", p[0]),
    )
    .await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(first["selected_id"], key);
    assert_eq!(first["selected"]["id"], key);
    let mut keys = vec![first["items"][0]["id"].as_str().unwrap().to_owned()];
    let mut cursor = first["next_cursor"].as_str().map(str::to_owned);
    while let Some(c) = cursor {
        let (status, page) = get(
            &app,
            &format!(
                "/api/matches/{id}/analyses?limit=1&position_id={}&cursor={c}",
                p[0]
            ),
        )
        .await;
        assert_eq!(status, 200, "{page}");
        keys.extend(
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["id"].as_str().unwrap().to_owned()),
        );
        cursor = page["next_cursor"].as_str().map(str::to_owned);
    }
    keys.sort();
    let mut expected = vec![
        key,
        format!("job:{}", scoped.id),
        format!("job:{}", hint.id),
        format!("imported:{id}"),
    ];
    expected.sort();
    assert_eq!(keys, expected);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        3
    );
    let incompatible = get(
        &app,
        &format!(
            "/api/matches/{id}/analyses?cursor={}&position_id={}",
            first["next_cursor"].as_str().unwrap(),
            p[2]
        ),
    )
    .await;
    assert_eq!(incompatible.0, 422);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn automatic_selection_is_engine_independent_and_never_merges_scopes(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    imported(&pool, id, p[0]).await;
    let scoped = match_job(&pool, id, p[2], Some(2), "Open Sage", 4, "completed").await;
    let (_, catalog) = get(&app, &format!("/api/matches/{id}/analyses")).await;
    assert_eq!(catalog["selected"]["id"], format!("imported:{id}"));
    assert!(catalog["selected_id"].is_null());
    let older = match_job(&pool, id, p[0], None, "GNU Backgammon", 4, "completed").await;
    let newer = match_job(&pool, id, p[0], None, "Open Sage", 1, "completed").await;
    sqlx::query("UPDATE app_settings SET analysis_engine='gnubg'")
        .execute(&pool)
        .await
        .unwrap();
    let (_, catalog) = get(&app, &format!("/api/matches/{id}/analyses")).await;
    assert_eq!(catalog["selected"]["id"], format!("job:{}", newer.id));
    assert_eq!(
        select(&app, id, Some(&format!("job:{}", scoped.id)))
            .await
            .0,
        200
    );
    assert_eq!(
        get(&app, &format!("/api/positions/{}/analysis-result", p[0])).await,
        (200, Value::Null)
    );
    let (_, result) = get(
        &app,
        &format!(
            "/api/positions/{}/analysis-result?analysis_id=job:{}",
            p[0], older.id
        ),
    )
    .await;
    assert_eq!(result["analysis_id"], format!("job:{}", older.id));
    assert_eq!(result["decision"]["best_equity"], 0.42);
    assert!(result["analysis"].is_null());
    assert!(result["records"].as_array().unwrap().is_empty());
    assert_eq!(
        select(&app, id, None).await,
        (200, json!({"match_id":id,"analysis_id":null}))
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn selection_requires_committed_current_results_and_keeps_stale_selection_explicit(
    pool: PgPool,
) {
    let (app, id, p) = setup(&pool).await;
    let empty = store::create_match_job(&pool, id, 2, 2).await.unwrap();
    sqlx::query("UPDATE jobs SET status='completed',result='{}' WHERE id=$1")
        .bind(empty.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        select(&app, id, Some(&format!("job:{}", empty.id))).await.0,
        422
    );
    let partial = match_job(&pool, id, p[0], None, "GNU Backgammon", 1, "running").await;
    let (_, catalog) = get(&app, &format!("/api/matches/{id}/analyses")).await;
    assert!(catalog["selected"].is_null());
    let key = format!("job:{}", partial.id);
    assert_eq!(select(&app, id, Some(&key)).await.0, 200);
    assert_eq!(
        get(&app, &format!("/api/positions/{}/analysis-result", p[0]))
            .await
            .0,
        200
    );
    sqlx::query("UPDATE matches SET source_revision=source_revision+1 WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        get(&app, &format!("/api/positions/{}/analysis-result", p[0]))
            .await
            .0,
        422
    );
    let (_, catalog) = get(&app, &format!("/api/matches/{id}/analyses")).await;
    assert_eq!(catalog["selected_id"], key);
    assert!(catalog["selected"].is_null());
    assert_eq!(
        select(&app, id, Some(&format!("imported:{}", Uuid::new_v4())))
            .await
            .0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn exact_position_results_preserve_candidates_units_and_source_revision(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    imported(&pool, id, p[0]).await;
    let (_, foreign) = get(
        &app,
        &format!(
            "/api/positions/{}/analysis-result?analysis_id=imported:{id}",
            p[0]
        ),
    )
    .await;
    assert_eq!(foreign["records"][0]["candidates"][0]["equity_kind"], "mwc");
    assert_eq!(
        foreign["records"][0]["candidates"][0]["equity"],
        0.123456789
    );
    assert!(foreign["decision"].is_null());
    sqlx::query("UPDATE matches SET source_revision=3 WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let hint = position_job(&pool, p[0], 2).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT source_revision FROM jobs WHERE id=$1")
            .bind(hint.id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        3
    );
    let (_, result) = get(
        &app,
        &format!(
            "/api/positions/{}/analysis-result?analysis_id=job:{}",
            p[0], hint.id
        ),
    )
    .await;
    assert_eq!(result["analysis"]["candidates"][0]["equity"], 0.314159265);
    assert_eq!(
        result["analysis"]["candidates"].as_array().unwrap().len(),
        1
    );
    let (_, saved) = get(
        &app,
        &format!("/api/positions/{}/jobs?engine_id=gnubg", p[0]),
    )
    .await;
    assert_eq!(saved[0]["id"], hint.id.to_string());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn placeholders_and_empty_or_malformed_results_do_not_become_selections(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    imported(&pool, id, p[0]).await;
    sqlx::query("UPDATE imported_analyses SET data=jsonb_set(jsonb_set(data,'{candidates}','[]'),'{cube}','{\"equity_kind\":\"unknown\"}') WHERE match_uuid=$1")
        .bind(id).execute(&pool).await.unwrap();
    let empty = store::create_match_job(&pool, id, 2, 2).await.unwrap();
    sqlx::query("UPDATE jobs SET status='completed' WHERE id=$1")
        .bind(empty.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,1)")
        .bind(empty.id)
        .execute(&pool)
        .await
        .unwrap();
    let bad = position_job(&pool, p[0], 1).await;
    sqlx::query(
        "UPDATE jobs SET result=jsonb_set(result,'{candidates}','\"invalid\"') WHERE id=$1",
    )
    .bind(bad.id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("SELECT * FROM analysis_catalog_sources WHERE match_id=$1")
        .bind(id)
        .fetch_all(&pool)
        .await
        .expect("Catalog must tolerate malformed historical results");
    let (status, catalog) = get(&app, &format!("/api/matches/{id}/analyses")).await;
    assert_eq!(status, 200, "{catalog}");
    assert!(catalog["items"].as_array().unwrap().is_empty(), "{catalog}");
    assert!(catalog["selected"].is_null());
    assert_eq!(
        select(&app, id, Some(&format!("job:{}", empty.id))).await.0,
        422
    );
    assert_eq!(
        select(&app, id, Some(&format!("job:{}", bad.id))).await.0,
        422
    );
    assert_eq!(
        select(&app, id, Some(&format!("imported:{id}"))).await.0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn all_catalog_reads_and_writes_honor_live_competition_visibility(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    let hint = position_job(&pool, p[0], 2).await;
    let key = format!("job:{}", hint.id);
    assert_eq!(select(&app, id, Some(&key)).await.0, 200);
    sqlx::query("INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id,library_match_id) VALUES($1,$2,'/unavailable','','saved',$3)")
        .bind(Uuid::new_v4()).bind(json!({"play_mode":"competition","phase":"human_move","match_length":7})).bind(id).execute(&pool).await.unwrap();
    for path in [
        format!("/api/matches/{id}/analyses"),
        format!("/api/positions/{}/analysis-result", p[0]),
        format!("/api/positions/{}/jobs", p[0]),
    ] {
        assert_eq!(get(&app, &path).await.0, 422, "{path}");
    }
    assert_eq!(select(&app, id, None).await.0, 422);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM effective_analysis_selections WHERE match_id=$1"
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn retained_native_occurrences_remain_automatically_readable_without_full_match_claim(
    pool: PgPool,
) {
    let (app, id, p) = setup(&pool).await;
    let job = match_job(&pool, id, p[0], None, "GNU Backgammon", 2, "completed").await;
    sqlx::query("UPDATE matches SET source_match_id=id,source_match_no=match_no,source_database_id=database_id,source_database_name='Fixture',database_id='00000000-0000-0000-0000-000000000002',collection_only=true,original=''::bytea,sgf='',moves_cataloged=true,imported_analysis_loaded=true,source_metadata_loaded=true WHERE id=$1")
        .bind(id).execute(&pool).await.unwrap();
    let (status, catalog) = get(&app, &format!("/api/matches/{id}/analyses")).await;
    assert_eq!(status, 200, "{catalog}");
    assert_eq!(catalog["selected"]["id"], format!("job:{}", job.id));
    assert_eq!(catalog["selected"]["complete"], false);
    let (status, result) = get(&app, &format!("/api/positions/{}/analysis-result", p[0])).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["analysis_id"], format!("job:{}", job.id));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn calculation_monitor_uses_explicit_engine_filter_only(pool: PgPool) {
    let (_, id, p) = setup(&pool).await;
    let legacy = match_job(&pool, id, p[0], None, "GNU Backgammon", 2, "completed").await;
    let sage = match_job(&pool, id, p[0], None, "Open Sage", 1, "completed").await;
    sqlx::query("UPDATE jobs SET engine_snapshot=$2 WHERE id=$1").bind(sage.id).bind(json!({"engine_id":"bgsage","gnubg_path":"unused","engine_version":null,"open_sage":null})).execute(&pool).await.unwrap();
    sqlx::query("UPDATE app_settings SET analysis_engine='gnubg'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store::latest_match_job_scoped(&pool, id, None, None)
            .await
            .unwrap()
            .unwrap()
            .id,
        sage.id
    );
    assert_eq!(
        store::latest_match_job_scoped(&pool, id, None, Some("gnubg"))
            .await
            .unwrap()
            .unwrap()
            .id,
        legacy.id
    );
    assert_eq!(
        store::latest_match_job_scoped(&pool, id, None, Some("bgsage"))
            .await
            .unwrap()
            .unwrap()
            .id,
        sage.id
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn explicit_imported_selection_is_bound_to_the_selected_source_revision(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    imported(&pool, id, p[0]).await;
    let key = format!("imported:{id}");
    assert_eq!(select(&app, id, Some(&key)).await.0, 200);
    sqlx::query("UPDATE matches SET source_revision=source_revision+1 WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        get(&app, &format!("/api/positions/{}/analysis-result", p[0]))
            .await
            .0,
        422
    );
    let (_, catalog) = get(&app, &format!("/api/matches/{id}/analyses")).await;
    assert_eq!(catalog["selected_id"], key);
    assert!(catalog["selected"].is_null());
    assert_eq!(select(&app, id, Some(&key)).await.0, 200);
    assert_eq!(
        get(&app, &format!("/api/positions/{}/analysis-result", p[0]))
            .await
            .0,
        200
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn summary_only_source_evaluations_remain_visible_but_legacy_placeholders_do_not(
    pool: PgPool,
) {
    let (app, id, p) = setup(&pool).await;
    imported(&pool, id, p[0]).await;
    sqlx::query("UPDATE imported_analyses SET data=jsonb_set(data,'{candidates,0,equity}','null') WHERE match_uuid=$1").bind(id).execute(&pool).await.unwrap();
    let (_, catalog) = get(&app, &format!("/api/matches/{id}/analyses")).await;
    assert_eq!(catalog["selected"]["id"], format!("imported:{id}"));
    let (_, result) = get(&app, &format!("/api/positions/{}/analysis-result", p[0])).await;
    assert_eq!(
        result["records"][0]["candidates"][0]["probabilities"]["win"],
        0.6
    );
    assert!(result["records"][0]["candidates"][0]["equity"].is_null());
    assert!(result["decision"].is_null());
    sqlx::query("UPDATE imported_analyses SET data=jsonb_set(jsonb_set(data,'{candidates}','[]'),'{raw}',$2) WHERE match_uuid=$1")
        .bind(id).bind(json!({"currency":null,"best_equity":0.0,"equity_loss":0.0})).execute(&pool).await.unwrap();
    let (_, catalog) = get(&app, &format!("/api/matches/{id}/analyses")).await;
    assert_eq!(catalog["selected"]["id"], format!("imported:{id}"));
    let (_, result) = get(&app, &format!("/api/positions/{}/analysis-result", p[0])).await;
    assert_eq!(result["records"][0]["evaluated"], true);
    assert_eq!(result["records"][0]["raw"]["best_equity"], 0.0);
    assert!(
        result["records"][0]["candidates"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    sqlx::query("UPDATE imported_analyses SET data=data||$2 WHERE match_uuid=$1")
        .bind(id).bind(json!({"engine":"Unknown","method":"unknown","depth":null,"raw":{"analysis_id":"b86cd1b0-4d12-80e6-86ba-511a59c022f0","currency":null,"best_equity":0,"equity_loss":0}})).execute(&pool).await.unwrap();
    let (_, catalog) = get(&app, &format!("/api/matches/{id}/analyses")).await;
    assert!(catalog["items"].as_array().unwrap().is_empty());
    let (_, records) = get(&app, &format!("/api/positions/{}/imported-analysis", p[0])).await;
    assert_eq!(records[0]["evaluated"], false);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn imported_primary_is_stable_and_original_records_are_preserved(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    imported(&pool, id, p[0]).await;
    sqlx::query("INSERT INTO imported_analyses(match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data) SELECT match_uuid,position_uuid,1,game_no,source_move_no,kind,jsonb_set(data,'{candidates,0,equity}','0.9') FROM imported_analyses WHERE match_uuid=$1 AND source_ordinal=0")
        .bind(id).execute(&pool).await.unwrap();
    let (_, catalog) = get(&app, &format!("/api/matches/{id}/analyses")).await;
    assert_eq!(catalog["selected"]["checker_count"], 1);
    let (_, result) = get(&app, &format!("/api/positions/{}/analysis-result", p[0])).await;
    assert_eq!(result["records"].as_array().unwrap().len(), 1);
    assert_eq!(result["records"][0]["candidates"][0]["equity"], 0.123456789);
    sqlx::query("UPDATE imported_analyses SET data=data||$2 WHERE match_uuid=$1 AND source_ordinal=0")
        .bind(id).bind(json!({"engine":"Unknown","method":"unknown","depth":null,"candidates":[],"raw":{"analysis_id":"b86cd1b0-4d12-80e6-86ba-511a59c022f0","currency":null,"best_equity":0,"equity_loss":0}})).execute(&pool).await.unwrap();
    let (_, result) = get(&app, &format!("/api/positions/{}/analysis-result", p[0])).await;
    assert_eq!(result["records"].as_array().unwrap().len(), 1);
    assert_eq!(result["records"][0]["candidates"][0]["equity"], 0.9);
    let (_, originals) = get(&app, &format!("/api/positions/{}/imported-analysis", p[0])).await;
    assert_eq!(originals.as_array().unwrap().len(), 2);
    assert_eq!(originals[0]["evaluated"], false);
    assert_eq!(originals[1]["evaluated"], true);
}
