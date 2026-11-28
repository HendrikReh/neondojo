use crate::support;
use neondojo::{api, engine::Engine, import, model::Job, store};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::send as request;
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
async fn library_job_notifications_resolve_game_and_position_analysis_to_the_visible_match(
    pool: PgPool,
) {
    let (_, match_id, positions) = setup(&pool).await;
    let facade = neondojo::facade::BackendFacade::new(api::AppState::new(
        pool.clone(),
        Engine::new("unused".into()),
    ));
    let visible = [match_id];
    let other = [Uuid::new_v4()];
    let position = position_job(&pool, positions[0], 1).await;
    let game = match_job(
        &pool,
        match_id,
        positions[0],
        Some(1),
        "GNU Backgammon",
        2,
        "completed",
    )
    .await;
    for job in [position.id, game.id] {
        assert_eq!(
            facade
                .library_analysis_statuses(&visible, Some(job))
                .await
                .unwrap(),
            [(match_id, Some("partial".into()))]
        );
        assert!(
            facade
                .library_analysis_statuses(&other, Some(job))
                .await
                .unwrap()
                .is_empty()
        );
    }
    let dice = store::create_job(&pool, positions[0], "dice", 1)
        .await
        .unwrap();
    assert!(
        facade
            .library_analysis_statuses(&visible, Some(dice.id))
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrations = "./migrations")]
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
async fn catalog_preserves_history_and_selected_entry_across_pages(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    imported(&pool, id, p[0]).await;
    let old = match_job(&pool, id, p[0], None, "GNU Backgammon", 4, "completed").await;
    let scoped = match_job(&pool, id, p[2], Some(2), "NeonGammon", 2, "completed").await;
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
async fn automatic_selection_is_engine_independent_and_never_merges_scopes(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    imported(&pool, id, p[0]).await;
    let scoped = match_job(&pool, id, p[2], Some(2), "NeonGammon", 4, "completed").await;
    let (_, catalog) = get(&app, &format!("/api/matches/{id}/analyses")).await;
    assert_eq!(catalog["selected"]["id"], format!("imported:{id}"));
    assert!(catalog["selected_id"].is_null());
    let older = match_job(&pool, id, p[0], None, "GNU Backgammon", 4, "completed").await;
    let newer = match_job(&pool, id, p[0], None, "NeonGammon", 1, "completed").await;
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
async fn calculation_monitor_uses_explicit_engine_filter_only(pool: PgPool) {
    let (_, id, p) = setup(&pool).await;
    let legacy = match_job(&pool, id, p[0], None, "GNU Backgammon", 2, "completed").await;
    let sage = match_job(&pool, id, p[0], None, "NeonGammon", 1, "completed").await;
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
async fn malformed_explicit_keys_leave_the_match_without_a_selected_analysis(pool: PgPool) {
    let (_app, id, positions) = setup(&pool).await;
    imported(&pool, id, positions[0]).await;
    // The table's historical constraint permits 36 hex digits or hyphens.
    // Resolving either prefix must compare the original key as text.
    for prefix in ["job", "imported"] {
        sqlx::query(
            "INSERT INTO analysis_selections(match_id,analysis_id) VALUES($1,$2)
                     ON CONFLICT(match_id) DO UPDATE SET analysis_id=excluded.analysis_id",
        )
        .bind(id)
        .bind(format!("{prefix}:{}", "-".repeat(36)))
        .execute(&pool)
        .await
        .unwrap();
        let selected: Option<String> = sqlx::query_scalar(
            "SELECT analysis_id FROM effective_analysis_selections WHERE match_id=$1",
        )
        .bind(id)
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_eq!(selected, None);
    }
}

#[sqlx::test(migrations = "./migrations")]
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

async fn dice_job(pool: &PgPool, position: Uuid, plies: i32, status: &str) -> Job {
    let job = store::create_job(pool, position, "dice", plies)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET status=$2,result=$3 WHERE id=$1")
        .bind(job.id)
        .bind(status)
        .bind(
            (status == "completed").then(
                || json!({"engine":"Dice engine","plies":plies,"mean_equity":0.1,"rolls":[]}),
            ),
        )
        .execute(pool)
        .await
        .unwrap();
    store::get_job(pool, job.id).await.unwrap().unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn deleting_dice_equity_removes_only_that_finished_dice_job(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    let older = dice_job(&pool, p[0], 1, "completed").await;
    let latest = dice_job(&pool, p[0], 2, "completed").await;
    let failed = dice_job(&pool, p[0], 3, "failed").await;
    let running = dice_job(&pool, p[0], 4, "running").await;
    let checker = position_job(&pool, p[0], 2).await;
    let path = |position: Uuid, job: Uuid| format!("/api/positions/{position}/jobs/{job}");

    // Checker analyses stay in the catalog, active calculations stay intact,
    // and a job is addressed only through its own position.
    assert_eq!(
        request(&app, "DELETE", &path(p[0], checker.id), Value::Null)
            .await
            .0,
        422
    );
    assert_eq!(
        request(&app, "DELETE", &path(p[0], running.id), Value::Null)
            .await
            .0,
        409
    );
    assert_eq!(
        request(&app, "DELETE", &path(p[1], latest.id), Value::Null)
            .await
            .0,
        404
    );
    assert_eq!(
        request(&app, "DELETE", &path(p[0], Uuid::new_v4()), Value::Null)
            .await
            .0,
        404
    );
    assert_eq!(
        request(
            &app,
            "DELETE",
            &path(Uuid::new_v4(), latest.id),
            Value::Null
        )
        .await
        .0,
        404
    );

    let (status, response) = request(&app, "DELETE", &path(p[0], latest.id), Value::Null).await;
    assert_eq!(status, 204, "{response}");
    assert_eq!(get(&app, &format!("/api/jobs/{}", latest.id)).await.0, 404);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM job_events WHERE job_id=$1")
            .bind(latest.id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        request(&app, "DELETE", &path(p[0], latest.id), Value::Null)
            .await
            .0,
        404
    );
    assert_eq!(
        request(&app, "DELETE", &path(p[0], failed.id), Value::Null)
            .await
            .0,
        204
    );
    for job in [older.id, running.id, checker.id] {
        assert_eq!(get(&app, &format!("/api/jobs/{job}")).await.0, 200);
    }

    sqlx::query("INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id,library_match_id) VALUES($1,$2,'/unavailable','','saved',$3)")
        .bind(Uuid::new_v4()).bind(json!({"play_mode":"competition","phase":"human_move","match_length":7})).bind(id).execute(&pool).await.unwrap();
    assert_eq!(
        request(&app, "DELETE", &path(p[0], older.id), Value::Null)
            .await
            .0,
        422
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM jobs WHERE id=$1")
            .bind(older.id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn native_eligibility_needs_a_checkpointed_decision_and_selection_skips_the_counts(
    pool: PgPool,
) {
    let (_app, id, p) = setup(&pool).await;
    let complete = match_job(&pool, id, p[0], None, "GNU Backgammon", 2, "completed").await;
    let partial = match_job(&pool, id, p[0], None, "GNU Backgammon", 3, "running").await;
    sqlx::query("INSERT INTO move_analyses(job_id,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,1,99,0,'cube',0,'{}')")
        .bind(partial.id).execute(&pool).await.unwrap();
    // A saved decision makes a job eligible only once its game is checkpointed.
    let orphan = store::create_match_job_scoped(&pool, id, 4, 2, None)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET status='running' WHERE id=$1")
        .bind(orphan.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,1)")
        .bind(orphan.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO move_analyses(job_id,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,2,1,0,'checker',0,'{}')")
        .bind(orphan.id).execute(&pool).await.unwrap();

    let mut rows: Vec<(Uuid, i64, i64)> = sqlx::query_as(
        "SELECT job_id,checker_count,cube_count FROM analysis_catalog_sources
         WHERE match_id=$1 AND source='native'",
    )
    .bind(id)
    .fetch_all(&pool)
    .await
    .unwrap();
    rows.sort();
    let mut expected = vec![(complete.id, 1, 0), (partial.id, 1, 1)];
    expected.sort();
    assert_eq!(rows, expected);
    let selected: Option<Uuid> =
        sqlx::query_scalar("SELECT job_id FROM effective_analysis_selections WHERE match_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(selected, Some(complete.id));

    // Resolving the effective analysis must not count each job's decisions.
    let counts_decisions = |sql: &'static str| {
        let pool = pool.clone();
        async move {
            let plan: Value = sqlx::query_scalar(sql)
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
            aggregates_relation(&plan[0]["Plan"], "move_analyses")
        }
    };
    assert!(
        counts_decisions(
            "EXPLAIN (FORMAT JSON) SELECT checker_count FROM analysis_catalog_sources WHERE match_id=$1"
        )
        .await
    );
    assert!(
        !counts_decisions(
            "EXPLAIN (FORMAT JSON) SELECT job_id FROM effective_analysis_selections WHERE match_id=$1"
        )
        .await
    );
}

/// Whether an aggregate in the plan reads the relation below it.
fn aggregates_relation(node: &Value, relation: &str) -> bool {
    fn reads(node: &Value, relation: &str) -> bool {
        node["Relation Name"] == relation
            || node["Plans"]
                .as_array()
                .is_some_and(|plans| plans.iter().any(|plan| reads(plan, relation)))
    }
    (node["Node Type"] == "Aggregate" && reads(node, relation))
        || node["Plans"]
            .as_array()
            .is_some_and(|plans| plans.iter().any(|plan| aggregates_relation(plan, relation)))
}

mod library_job_watcher {
    use super::*;
    use neondojo::{
        api::AppState,
        facade::{BackendFacade, LibraryJobScope, LibraryJobSnapshot},
    };
    use std::time::Duration;
    use tokio::sync::mpsc;

    // Each expected transition must arrive before the 15 s safety read.
    async fn expect_snapshot(
        rx: &mut mpsc::Receiver<LibraryJobSnapshot>,
        match_id: Uuid,
        job_id: Uuid,
        status: &str,
        progress: i32,
    ) {
        tokio::time::timeout(Duration::from_secs(6), async {
            while let Some(snapshot) = rx.recv().await {
                if snapshot
                    .bulk
                    .iter()
                    .any(|job| job.id == job_id && job.status == status && job.progress == progress)
                {
                    assert_eq!(snapshot.rows, vec![(match_id, Some(status.into()))]);
                    return;
                }
            }
            panic!("Library watcher ended before {status} {progress}");
        })
        .await
        .unwrap_or_else(|_| panic!("Library stayed stale at {status} {progress}"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn row_and_bulk_follow_commits_and_reconcile_after_listener_reconnect(pool: PgPool) {
        let sgf = "(;GM[6]FF[4]MI[length:5][ws:0][bs:0];W[31hefe])";
        let imported = store::import_match(
            &pool,
            "watch.sgf",
            "sgf",
            sgf.as_bytes(),
            sgf,
            &import::parse_sgf(sgf).unwrap(),
        )
        .await
        .unwrap();
        let match_id = imported.r#match.id;
        let job = store::create_match_job(&pool, match_id, 1, 1)
            .await
            .unwrap();
        let facade = BackendFacade::new(AppState::new(pool.clone(), Engine::new("unused".into())));
        let (tx, mut rx) = mpsc::channel(1);
        let watch = LibraryJobScope {
            rows: vec![match_id],
            bulk: vec![job.id],
            pending: true,
        };
        let reader = tokio::spawn({
            let facade = facade.clone();
            async move { facade.follow_library_jobs(&watch, tx).await }
        });
        expect_snapshot(&mut rx, match_id, job.id, "queued", 0).await;

        for (event, status, progress) in [
            ("running", "running", 0),
            ("progress", "running", 40),
            ("completed", "completed", 100),
        ] {
            let mut tx = pool.begin().await.unwrap();
            sqlx::query("UPDATE jobs SET status=$2, progress=$3 WHERE id=$1")
                .bind(job.id)
                .bind(status)
                .bind(progress)
                .execute(&mut *tx)
                .await
                .unwrap();
            if status == "completed" {
                sqlx::query("UPDATE jobs SET completed_games=total_games WHERE id=$1")
                    .bind(job.id)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO match_analysis_games(job_id,game_no,summary) VALUES($1,1,'{\"completed_games\":1,\"total_games\":1}')")
                    .bind(job.id).execute(&mut *tx).await.unwrap();
                // The connection misses the terminal notification. Its new LISTEN
                // must reconcile the committed result even without another event.
                sqlx::query("ALTER TABLE job_events DISABLE TRIGGER job_events_notify")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                let stopped: Vec<bool> = sqlx::query_scalar("SELECT pg_terminate_backend(pid,5000) FROM pg_stat_activity WHERE datname=current_database() AND application_name='neondojo-job-events'")
                    .fetch_all(&mut *tx).await.unwrap();
                assert_eq!(stopped, [true]);
            }
            sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,$2,jsonb_build_object('status',$3::text,'progress',$4::int))")
                .bind(job.id).bind(event).bind(status).bind(progress).execute(&mut *tx).await.unwrap();
            tx.commit().await.unwrap();
            expect_snapshot(&mut rx, match_id, job.id, status, progress).await;
        }
        reader.abort();
        facade.state().shutdown.cancel();
    }
}
