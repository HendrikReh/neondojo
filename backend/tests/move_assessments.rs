use axum::{body::Body, http::Request};
use bgtutor::{api, engine::Engine, import, jobs, model::Job, store};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SOURCE: &str = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])\n(;GM[6]FF[4]MI[length:7][ws:1][bs:0];W[31hefe];B[42qusu])";

async fn setup(pool: &PgPool) -> (axum::Router, Uuid, Vec<Uuid>) {
    let id = import_source(pool, SOURCE).await;
    let positions =
        sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 ORDER BY ordinal")
            .bind(id)
            .fetch_all(pool)
            .await
            .unwrap();
    let state = api::AppState::new(pool.clone(), Engine::new("gnubg".into()));
    (api::router(state), id, positions)
}

async fn import_source(pool: &PgPool, source: &str) -> Uuid {
    store::import_match(
        pool,
        "assessment.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &import::parse_sgf(source).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id
}

async fn get(app: &axum::Router, uri: &str) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn uri(id: Uuid, game: i32, job: Option<Uuid>) -> String {
    format!(
        "/api/matches/{id}/move-assessments?game_no={game}{}",
        job.map(|j| format!("&job_id={j}")).unwrap_or_default()
    )
}

async fn imported(pool: &PgPool, id: Uuid, position: Uuid, game: i32, loss: f64, unit: &str) {
    let (notation, player, ply): (String, i32, i32) =
        sqlx::query_as("SELECT notation,player,ply FROM positions WHERE id=$1")
            .bind(position)
            .fetch_one(pool)
            .await
            .unwrap();
    let best = if player == 0 { "13/10 6/5" } else { "13/9 6/4" };
    let candidates = json!([
        {"rank":1,"notation":best,"equity":0.5,"equity_kind":unit,"probabilities":{},"depth":"2-ply","depth_plies":2,"method":"evaluation","raw":null},
        {"rank":2,"notation":notation.split_whitespace().rev().collect::<Vec<_>>().join(" "),"equity":0.5-loss,"equity_kind":unit,"probabilities":{},"depth":"2-ply","depth_plies":2,"method":"evaluation","raw":null}
    ]);
    let data = json!({"game_no":game,"source_move_no":ply,"checker_ply":ply,"kind":"checker","player":player,"played":notation,"engine":"Imported fixture","engine_version":"1","method":"evaluation","depth":"2-ply","depth_plies":2,"candidates":candidates,"cube":null,"raw":null});
    sqlx::query("INSERT INTO imported_analyses(match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data) VALUES($1,$2,(SELECT coalesce(max(source_ordinal)+1,0) FROM imported_analyses WHERE match_uuid=$1),$3,$4,'checker',$5)")
        .bind(id).bind(position).bind(game).bind(ply).bind(data).execute(pool).await.unwrap();
}

async fn native(pool: &PgPool, job: &Job, position: Uuid, game: i32, loss: f64, status: &str) {
    let (notation, player, ply): (String, i32, i32) =
        sqlx::query_as("SELECT notation,player,ply FROM positions WHERE id=$1")
            .bind(position)
            .fetch_one(pool)
            .await
            .unwrap();
    let data = json!({"decision_no":ply,"checker_no":ply,"player":player,"kind":"checker","played":notation,"best":"13/10 6/5","played_equity":0.5-loss,"best_equity":0.5,"equity_loss":loss,"severity":"very_bad","dice":null,"evaluation_status":status});
    sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,$3,$4,$5,'checker',$6,$7)")
        .bind(job.id).bind(position).bind(game).bind(ply).bind(player).bind(loss).bind(data).execute(pool).await.unwrap();
}

async fn checkpoint(pool: &PgPool, job: &Job, game: i32) {
    sqlx::query("UPDATE jobs SET status=CASE WHEN status='queued' THEN 'running' ELSE status END WHERE id=$1").bind(job.id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,$2)")
        .bind(job.id)
        .bind(game)
        .execute(pool)
        .await
        .unwrap();
}

async fn hint(pool: &PgPool, position: Uuid, with_played: bool) -> Uuid {
    let notation: String = sqlx::query_scalar("SELECT notation FROM positions WHERE id=$1")
        .bind(position)
        .fetch_one(pool)
        .await
        .unwrap();
    let mut candidates =
        vec![json!({"rank":1,"notation":"13/10 6/5","equity":0.5,"probabilities":[]})];
    if with_played {
        candidates.push(json!({"rank":2,"notation":notation,"equity":0.42,"probabilities":[]}));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result) VALUES($1,$2,'analysis',2,'completed',$3)")
        .bind(id).bind(position).bind(json!({"engine":"GNUbg hint fixture","plies":2,"candidates":candidates})).execute(pool).await.unwrap();
    id
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn catalog_result_reuses_saved_analysis_independently_of_calculation_profile(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    sqlx::query("UPDATE app_settings SET analysis_engine='bgsage', analysis_plies=2")
        .execute(&pool)
        .await
        .unwrap();
    let job = store::create_match_job_scoped(&pool, id, 2, 1, Some(1))
        .await
        .unwrap();
    native(&pool, &job, p[0], 1, 0.08, "evaluated").await;
    sqlx::query("UPDATE jobs SET status='completed',result=$2 WHERE id=$1")
        .bind(job.id)
        .bind(json!({"engine":"GNU Backgammon fixture"}))
        .execute(&pool)
        .await
        .unwrap();
    let path = format!("/api/positions/{}/analysis-result", p[0]);
    assert_eq!(get(&app, &path).await, (200, Value::Null));

    checkpoint(&pool, &job, 1).await;
    let (status, saved) = get(&app, &path).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["position_id"], p[0].to_string());
    assert_eq!(saved["analysis_id"], format!("job:{}", job.id));
    assert_eq!(saved["engine"], "GNU Backgammon fixture");
    assert_eq!(saved["depth"], "2-ply cubeful");
    assert_eq!(saved["decision"]["best"], "13/10 6/5");
    assert_eq!(saved["decision"]["best_equity"], 0.5);
    assert_eq!(saved["decision"]["played_equity"], 0.42);
    assert_eq!(saved["decision"]["equity_loss"], 0.08);
    assert_eq!(
        get(&app, &format!("/api/positions/{}/analysis-result", p[2])).await,
        (200, Value::Null),
    );

    sqlx::query("UPDATE matches SET source_revision=source_revision+1 WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(get(&app, &path).await, (200, Value::Null));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn imported_assessments_match_replayed_actual_moves_and_never_cross_games(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    imported(&pool, id, p[0], 1, 0.16, "cubeful").await;
    imported(&pool, id, p[2], 2, 0.08, "cubeful").await;
    let (status, data) = get(&app, &uri(id, 1, None)).await;
    assert_eq!(status, 200, "{data}");
    assert_eq!(data["match_id"], id.to_string());
    assert_eq!(data["game_no"], 1);
    assert!(data["job_id"].is_null());
    assert_eq!(data["items"].as_array().unwrap().len(), 1, "{data}");
    assert_eq!(data["items"][0]["position_id"], p[0].to_string());
    assert_eq!(data["items"][0]["severity"], "very_bad");
    assert_eq!(data["items"][0]["equity_loss"], 0.16);
    assert_eq!(data["items"][0]["source"], "imported");
    assert_eq!(data["items"][0]["engine"], "Imported fixture 1");
    assert_eq!(data["items"][0]["depth_plies"], 2);
    let (_, game2) = get(&app, &uri(id, 2, None)).await;
    assert_eq!(game2["items"][0]["position_id"], p[2].to_string());
    assert_eq!(game2["items"][0]["severity"], "bad");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn selected_native_checkpoint_wins_and_recomputes_documented_thresholds(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    imported(&pool, id, p[0], 1, 0.2, "cubeful").await;
    hint(&pool, p[0], true).await;
    let job = store::create_match_job(&pool, id, 0, 2).await.unwrap();
    native(&pool, &job, p[0], 1, 0.0, "evaluated").await;
    assert_eq!(
        get(&app, &uri(id, 1, Some(job.id))).await.0,
        422,
        "uncheckpointed results cannot be selected"
    );
    checkpoint(&pool, &job, 1).await;
    sqlx::query("UPDATE jobs SET status='running',completed_games=1,result=$2 WHERE id=$1")
        .bind(job.id)
        .bind(json!({"engine":"GNU Backgammon fixture"}))
        .execute(&pool)
        .await
        .unwrap();
    for (loss, severity) in [
        (0.0, "none"),
        (0.039999, "none"),
        (0.04, "doubtful"),
        (0.08, "bad"),
        (0.16, "very_bad"),
    ] {
        sqlx::query("UPDATE move_analyses SET equity_loss=$2,data=jsonb_set(jsonb_set(jsonb_set(data,'{equity_loss}',to_jsonb($2::float8)),'{played_equity}',to_jsonb(0.5-$2::float8)),'{severity}','\"none\"'::jsonb) WHERE job_id=$1")
            .bind(job.id).bind(loss).execute(&pool).await.unwrap();
        let (status, data) = get(&app, &uri(id, 1, Some(job.id))).await;
        assert_eq!(status, 200, "{data}");
        let item = &data["items"][0];
        assert_eq!(item["severity"], severity, "{data}");
        assert_eq!(item["equity_loss"], loss);
        assert_eq!(item["source"], "native");
        assert_eq!(item["engine"], "GNU Backgammon fixture");
        assert_eq!(item["job_id"], job.id.to_string());
        assert_eq!(item["depth_plies"], 0);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn absent_invalid_or_incomparable_equities_remain_unknown(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    imported(&pool, id, p[0], 1, 0.2, "mwc").await;
    imported(&pool, id, p[1], 1, 0.2, "unknown").await;
    hint(&pool, p[0], false).await;
    let job = store::create_match_job(&pool, id, 0, 2).await.unwrap();
    native(&pool, &job, p[0], 1, 0.0, "unavailable").await;
    native(&pool, &job, p[1], 1, 0.0, "evaluated").await;
    checkpoint(&pool, &job, 1).await;
    sqlx::query("UPDATE move_analyses SET data=data || '{\"played_equity\":null,\"best_equity\":null}'::jsonb WHERE position_uuid=$1").bind(p[1]).execute(&pool).await.unwrap();
    let (status, data) = get(&app, &uri(id, 1, Some(job.id))).await;
    assert_eq!(status, 200, "{data}");
    assert_eq!(
        data["items"],
        json!([]),
        "missing evaluation must not turn into good or bad play: {data}"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn invalid_best_answer_or_mismatched_actual_move_is_unknown(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    let job = store::create_match_job(&pool, id, 0, 2).await.unwrap();
    native(&pool, &job, p[0], 1, 0.08, "evaluated").await;
    checkpoint(&pool, &job, 1).await;
    for patch in [
        json!({"best":""}),
        json!({"best":"invalid answer"}),
        json!({"best":"13/10 6/5","played":"13/10 6/5"}),
    ] {
        sqlx::query("UPDATE move_analyses SET data=data || $2 WHERE job_id=$1")
            .bind(job.id)
            .bind(patch)
            .execute(&pool)
            .await
            .unwrap();
        let (_, data) = get(&app, &uri(id, 1, Some(job.id))).await;
        assert_eq!(
            data["items"],
            json!([]),
            "invalid answer or actual play must not be classified: {data}"
        );
    }
    imported(&pool, id, p[0], 1, 0.16, "cubeful").await;
    sqlx::query("UPDATE imported_analyses SET data=jsonb_set(data,'{candidates,0,notation}','\"invalid answer\"'::jsonb) WHERE position_uuid=$1").bind(p[0]).execute(&pool).await.unwrap();
    let (_, data) = get(&app, &uri(id, 1, Some(job.id))).await;
    assert_eq!(data["items"], json!([]));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn a_forced_label_on_a_playable_position_cannot_imply_zero_loss(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    let job = store::create_match_job(&pool, id, 0, 2).await.unwrap();
    native(&pool, &job, p[0], 1, 0.0, "forced").await;
    checkpoint(&pool, &job, 1).await;
    let (_, data) = get(&app, &uri(id, 1, Some(job.id))).await;
    assert_eq!(
        data["items"],
        json!([]),
        "a playable move requires actual equities: {data}"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn genuine_forced_native_decision_has_explicit_zero_loss(pool: PgPool) {
    let source = include_str!("fixtures/analysis-game1.sgf");
    let id = import_source(&pool, source).await;
    let position:Uuid=sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 AND notation='Cannot move' ORDER BY ordinal LIMIT 1").bind(id).fetch_one(&pool).await.unwrap();
    let job = store::create_match_job(&pool, id, 0, 1).await.unwrap();
    native(&pool, &job, position, 1, 0.0, "forced").await;
    sqlx::query("UPDATE move_analyses SET data=data || '{\"best\":\"Cannot move\",\"played_equity\":null,\"best_equity\":null}'::jsonb WHERE job_id=$1").bind(job.id).execute(&pool).await.unwrap();
    checkpoint(&pool, &job, 1).await;
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("gnubg".into()),
    ));
    let (status, data) = get(&app, &uri(id, 1, Some(job.id))).await;
    assert_eq!(status, 200, "{data}");
    let assessment = data["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["position_id"] == position.to_string())
        .unwrap();
    assert_eq!(assessment["source"], "native");
    assert_eq!(assessment["severity"], "none");
    assert_eq!(assessment["equity_loss"], 0.0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn legacy_source_catalog_is_loaded_without_changing_original_or_manual_metadata(
    pool: PgPool,
) {
    let source = include_str!("fixtures/analysis-game1.sgf");
    let id = import_source(&pool, source).await;
    sqlx::query("DELETE FROM imported_analyses WHERE match_uuid=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE matches SET imported_analysis_loaded=false,display_name='My review title',tournament='Manual tournament',round='Final' WHERE id=$1").bind(id).execute(&pool).await.unwrap();
    let app = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("gnubg".into()),
    ));
    let (status, data) = get(&app, &uri(id, 1, None)).await;
    assert_eq!(status, 200, "{data}");
    assert!(
        !data["items"].as_array().unwrap().is_empty(),
        "actual source candidate equities must be usable: {data}"
    );
    assert!(
        data["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["source"] == "imported" && item["job_id"].is_null())
    );
    let saved: (Vec<u8>, String, String, String) =
        sqlx::query_as("SELECT original,display_name,tournament,round FROM matches WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        saved,
        (
            source.as_bytes().to_vec(),
            "My review title".into(),
            "Manual tournament".into(),
            "Final".into()
        )
    );
    let native_count: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        native_count, 0,
        "reading imported results must not create native work"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn individual_hint_requires_explicit_choice_and_actual_play(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    imported(&pool, id, p[0], 1, 0.16, "cubeful").await;
    let hint_id = hint(&pool, p[0], true).await;
    let (_, automatic) = get(&app, &uri(id, 1, None)).await;
    assert_eq!(automatic["items"][0]["source"], "imported");
    let chosen = format!("{}&analysis_id=job:{hint_id}", uri(id, 1, None));
    let (_, data) = get(&app, &chosen).await;
    assert_eq!(data["items"][0]["equity_loss"], 0.08, "{data}");
    assert_eq!(data["items"][0]["job_id"], hint_id.to_string());
    sqlx::query("UPDATE jobs SET result=jsonb_set(result,'{candidates}',jsonb_build_array(result->'candidates'->0)) WHERE id=$1").bind(hint_id).execute(&pool).await.unwrap();
    let (_, unknown) = get(&app, &chosen).await;
    assert_eq!(
        unknown["items"],
        json!([]),
        "missing actual move must not borrow imported assessment"
    );
    let (_, imported) = get(&app, &uri(id, 1, None)).await;
    assert_eq!(imported["items"][0]["equity_loss"], 0.16);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn actual_gnubg_game_results_assess_every_saved_checker_decision(pool: PgPool) {
    let id = import_source(&pool, include_str!("fixtures/analysis-game1.sgf")).await;
    let state = api::AppState::new(
        pool.clone(),
        Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())),
    );
    let job = store::create_match_job(&pool, id, 0, 1).await.unwrap();
    assert!(jobs::run_one(&state).await.unwrap());
    let saved: Job = sqlx::query_as("SELECT * FROM jobs WHERE id=$1")
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(saved.status, "completed", "{:?}", saved.error);
    let rows:Vec<(Uuid,f64)>=sqlx::query_as("SELECT position_uuid,equity_loss FROM move_analyses WHERE job_id=$1 AND kind='checker' ORDER BY decision_no").bind(job.id).fetch_all(&pool).await.unwrap();
    let app = api::router(state);
    let (status, data) = get(&app, &uri(id, 1, Some(job.id))).await;
    assert_eq!(status, 200, "{data}");
    assert_eq!(
        data["items"].as_array().unwrap().len(),
        rows.len(),
        "{data}"
    );
    for (position, loss) in rows {
        let item = data["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["position_id"] == position.to_string())
            .unwrap();
        assert_eq!(item["source"], "native", "{item}");
        assert_eq!(item["job_id"], job.id.to_string());
        assert_eq!(item["equity_loss"], loss);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn automatic_selection_respects_scope_and_explicit_selection_never_borrows(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    let job = store::create_match_job_scoped(&pool, id, 1, 1, Some(1))
        .await
        .unwrap();
    native(&pool, &job, p[0], 1, 0.08, "evaluated").await;
    checkpoint(&pool, &job, 1).await;
    sqlx::query("UPDATE jobs SET status='completed' WHERE id=$1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, data) = get(&app, &uri(id, 1, None)).await;
    assert_eq!(data["items"][0]["job_id"], job.id.to_string(), "{data}");
    let (_, game2) = get(&app, &uri(id, 2, None)).await;
    assert_eq!(game2["items"], json!([]));
    let selected = store::create_match_job(&pool, id, 0, 2).await.unwrap();
    assert_eq!(
        get(&app, &uri(id, 1, Some(selected.id))).await.0,
        422,
        "a pending selection must not borrow another result"
    );
    let other_id = import_source(&pool, &SOURCE.replace("Alice", "Other")).await;
    let other_position: Uuid =
        sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 ORDER BY ordinal LIMIT 1")
            .bind(other_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let other = store::create_match_job(&pool, other_id, 2, 2)
        .await
        .unwrap();
    native(&pool, &other, other_position, 1, 0.2, "evaluated").await;
    checkpoint(&pool, &other, 1).await;
    sqlx::query("UPDATE jobs SET status='completed' WHERE id=$1")
        .bind(other.id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, still) = get(&app, &uri(id, 1, None)).await;
    assert_eq!(still["items"][0]["job_id"], job.id.to_string());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn invalid_game_job_identity_and_scope_are_rejected(pool: PgPool) {
    let (app, id, p) = setup(&pool).await;
    let job = store::create_match_job_scoped(&pool, id, 0, 1, Some(1))
        .await
        .unwrap();
    native(&pool, &job, p[0], 1, 0.08, "evaluated").await;
    checkpoint(&pool, &job, 1).await;
    let other_id = import_source(&pool, &SOURCE.replace("Alice", "Other")).await;
    assert_eq!(get(&app, &uri(id, 0, None)).await.0, 422);
    assert_eq!(get(&app, &uri(id, 3, None)).await.0, 404);
    assert_eq!(get(&app, &uri(Uuid::new_v4(), 1, None)).await.0, 404);
    assert_eq!(get(&app, &uri(id, 1, Some(Uuid::new_v4()))).await.0, 422);
    assert_eq!(get(&app, &uri(id, 2, Some(job.id))).await.0, 422);
    assert_eq!(get(&app, &uri(other_id, 1, Some(job.id))).await.0, 422);
    let hint_id = hint(&pool, p[0], true).await;
    assert_eq!(get(&app, &uri(id, 1, Some(hint_id))).await.0, 200);
    assert!(
        get(
            &app,
            &format!("/api/matches/{id}/move-assessments?game_no=1&job_id=invalid")
        )
        .await
        .0 >= 400
    );
    assert!(
        get(&app, "/api/matches/invalid/move-assessments?game_no=1")
            .await
            .0
            >= 400
    );
    assert!(
        get(&app, &format!("/api/matches/{id}/move-assessments"))
            .await
            .0
            >= 400
    );
}
