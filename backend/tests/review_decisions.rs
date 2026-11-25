use crate::support;
use neondojo::{import, store};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{foreign_analysis as imported, send as call};
use uuid::Uuid;
fn app(pool: &PgPool) -> axum::Router {
    support::app(pool, "/unavailable")
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_import_records_use_one_primary_decision_in_every_projection(pool: PgPool) {
    let (id, ids) = imported(&pool).await;
    sqlx::query("INSERT INTO imported_analyses(id,match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data) SELECT $2,match_uuid,position_uuid,50,game_no,source_move_no,kind,jsonb_set(data,'{candidates,1,equity}','0.16') FROM imported_analyses WHERE id=$1")
        .bind(ids[0]).bind(Uuid::nil()).execute(&pool).await.unwrap();
    let router = app(&pool);
    let (_, review) = call(
        &router,
        "GET",
        &format!("/api/matches/{id}/review-decisions?game_no=1"),
        Value::Null,
    )
    .await;
    assert_eq!(
        review["items"].as_array().unwrap().len(),
        1,
        "one decision cannot be counted twice: {review}"
    );
    assert_eq!(review["items"][0]["equity_loss"], 0.2);
    let (_, marks) = call(
        &router,
        "GET",
        &format!("/api/matches/{id}/move-assessments?game_no=1"),
        Value::Null,
    )
    .await;
    assert_eq!(marks["items"].as_array().unwrap().len(), 1);
    assert_eq!(marks["items"][0]["equity_loss"], 0.2);
    let (_, stats) = call(
        &router,
        "GET",
        &format!("/api/matches/{id}/analysis-statistics?game_no=1"),
        Value::Null,
    )
    .await;
    assert_eq!(stats["summary"]["players"][0]["checker_equity_loss"], 0.2);
    let position = marks["items"][0]["position_id"].as_str().unwrap();
    let (_, selected) = call(
        &router,
        "GET",
        &format!("/api/positions/{position}/analysis-result"),
        Value::Null,
    )
    .await;
    assert_eq!(selected["records"].as_array().unwrap().len(), 1);
    assert_eq!(selected["records"][0]["id"], ids[0].to_string());
    let (_, original) = call(
        &router,
        "GET",
        &format!("/api/positions/{position}/imported-analysis"),
        Value::Null,
    )
    .await;
    assert_eq!(
        original.as_array().unwrap().len(),
        2,
        "original analyses remain accessible and unchanged"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn imported_catalog_statistics_preserve_unknown_depth_and_do_not_invent_pr(pool: PgPool) {
    let (id, _) = imported(&pool).await;
    let (status, data) = call(
        &app(&pool),
        "GET",
        &format!("/api/matches/{id}/analysis-statistics?analysis_id=imported:{id}"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{data}");
    assert_eq!(data["analysis_id"], format!("imported:{id}"));
    assert_eq!(data["summary"]["players"][0]["checker_equity_loss"], 0.4);
    assert_eq!(data["summary"]["analyzed_moves"], 2);
    assert!(data["summary"]["plies"].is_null());
    assert!(data["summary"]["performance_rating"].is_null());
    assert_eq!(data["summary"]["recorded_depth"], "XGRoller++");
    assert_eq!(data["error_breakdown"][0]["checker"]["very_bad"], 2);
    // One incomplete game must remain partial after losing its imported assessment.
    sqlx::query("DELETE FROM imported_analyses WHERE match_uuid=$1 AND game_no=2")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, partial) = call(
        &app(&pool),
        "GET",
        &format!("/api/matches/{id}/analysis-statistics?analysis_id=imported:{id}"),
        Value::Null,
    )
    .await;
    assert_eq!(partial["summary"]["analyzed_moves"], 1);
    assert_eq!(partial["summary"]["completed_games"], 1);
    assert!(partial["error_breakdown"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
async fn catalog_selection_is_shared_by_review_and_marks_without_source_fallback(pool: PgPool) {
    let (id, _) = imported(&pool).await;
    let job = store::create_match_job(&pool, id, 2, 2).await.unwrap();
    sqlx::query("UPDATE jobs SET status='running' WHERE id=$1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let position: Uuid =
        sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 AND game_no=1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let decision = json!({"decision_no":1,"checker_no":1,"player":0,"kind":"checker","played":"8/5 6/5","best":"6/3 6/5","played_equity":0.1,"best_equity":0.2,"equity_loss":0.1,"severity":"none","dice":[3,1],"evaluation_status":"evaluated"});
    sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,1,1,0,'checker',0.1,$3)")
        .bind(job.id).bind(position).bind(decision).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,1)")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET status='running',completed_games=1 WHERE id=$1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let router = app(&pool);
    let uri = format!(
        "/api/matches/{id}/review-decisions?analysis_id=job:{}",
        job.id
    );
    let (status, rows) = call(&router, "GET", &uri, Value::Null).await;
    assert_eq!(status, 200, "{rows}");
    assert_eq!(rows["items"].as_array().unwrap().len(), 1, "{rows}");
    assert_eq!(rows["items"][0]["source"], "native");
    let (status, selected) = call(
        &router,
        "PUT",
        &format!("/api/matches/{id}/analysis-selection"),
        json!({"analysis_id":format!("job:{}",job.id)}),
    )
    .await;
    assert_eq!(status, 200, "{selected}");
    let (_, marks) = call(
        &router,
        "GET",
        &format!("/api/matches/{id}/move-assessments?game_no=1"),
        Value::Null,
    )
    .await;
    assert_eq!(marks["analysis_id"], format!("job:{}", job.id));
    assert_eq!(marks["items"][0]["equity_loss"], 0.1);
    assert_eq!(marks["items"][0]["severity"], rows["items"][0]["severity"]);
    let (_, missing) = call(
        &router,
        "GET",
        &format!("/api/matches/{id}/move-assessments?game_no=2"),
        Value::Null,
    )
    .await;
    assert_eq!(missing["items"], json!([]));
    let (_, explicit_import) = call(
        &router,
        "GET",
        &format!("/api/matches/{id}/review-decisions?analysis_id=imported:{id}"),
        Value::Null,
    )
    .await;
    assert_eq!(explicit_import["items"].as_array().unwrap().len(), 2);
    assert_eq!(explicit_import["items"][0]["equity_loss"], 0.2);
}

#[sqlx::test(migrations = "./migrations")]
async fn paginated_imports_have_provenance_filtering_and_scope_bound_cursors(pool: PgPool) {
    let (id, ids) = imported(&pool).await;
    let router = app(&pool);
    let uri = format!("/api/matches/{id}/review-decisions?limit=1&errors_only=true");
    let (status, page) = call(&router, "GET", &uri, Value::Null).await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    let row = &page["items"][0];
    assert_eq!(row["id"], format!("imported:{}", ids[0]));
    assert_eq!(row["game_no"], 1);
    assert_eq!(row["source"], "imported");
    assert_eq!(row["equity_loss"], 0.2);
    assert_eq!(row["severity"], "very_bad");
    assert_eq!(row["depth"], "XGRoller++");
    assert!(row["job_id"].is_null());
    assert!(row["move_id"].is_string());
    let cursor = page["next_cursor"].as_str().unwrap();
    let (status, next) = call(
        &router,
        "GET",
        &format!("{uri}&after={cursor}"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{next}");
    assert_eq!(next["items"][0]["game_no"], 2);
    assert!(next["next_cursor"].is_null());
    assert_eq!(
        call(
            &router,
            "GET",
            &format!("{uri}&after={cursor}&player=1"),
            Value::Null
        )
        .await
        .0,
        422
    );
    let (status, numbered) = call(&router, "GET", &format!("{uri}&page=2"), Value::Null).await;
    assert_eq!(status, 200, "{numbered}");
    assert_eq!(numbered["total"], 2);
    assert_eq!(numbered["items"], next["items"]);
    let (_, beyond) = call(&router, "GET", &format!("{uri}&page=3"), Value::Null).await;
    assert_eq!(beyond["total"], 2);
    assert_eq!(beyond["items"], json!([]));
    for filter in ["player=1", "kind=cube", "severity=none"] {
        let (_, empty) = call(
            &router,
            "GET",
            &format!("/api/matches/{id}/review-decisions?{filter}"),
            Value::Null,
        )
        .await;
        assert_eq!(empty["items"], json!([]), "{empty}");
    }
    for filter in [
        "limit=0",
        "limit=101",
        "player=2",
        "kind=other",
        "game_no=0",
        "after=bad",
        "page=0",
        "page=1&after=bad",
    ] {
        assert_eq!(
            call(
                &router,
                "GET",
                &format!("/api/matches/{id}/review-decisions?{filter}"),
                Value::Null
            )
            .await
            .0,
            422,
            "{filter}"
        );
    }
}
#[sqlx::test(migrations = "./migrations")]
async fn pending_and_empty_jobs_do_not_fall_back_to_imports(pool: PgPool) {
    let (id, _) = imported(&pool).await;
    let job = store::create_match_job(&pool, id, 1, 2).await.unwrap();
    let router = app(&pool);
    let uri = format!("/api/matches/{id}/review-decisions?job_id={}", job.id);
    let (status, _) = call(&router, "GET", &uri, Value::Null).await;
    assert_eq!(status, 422, "a pending job is not an analysis result");
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,1)")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = call(&router, "GET", &uri, Value::Null).await;
    assert_eq!(status, 422, "an empty checkpoint is not a result");
    let (_, imported) = call(
        &router,
        "GET",
        &format!("/api/matches/{id}/review-decisions?analysis_id=imported:{id}"),
        Value::Null,
    )
    .await;
    assert_eq!(imported["items"].as_array().unwrap().len(), 2);
}
#[sqlx::test(migrations = "./migrations")]
async fn imported_sgf_checker_analysis_remains_reviewable_without_jobs(pool: PgPool) {
    let source = include_str!("fixtures/analysis-game1.sgf");
    let id = store::import_match(
        &pool,
        "analysis.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &import::parse_sgf(source).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    let (status, page) = call(
        &app(&pool),
        "GET",
        &format!("/api/matches/{id}/review-decisions"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{page}");
    assert!(!page["items"].as_array().unwrap().is_empty(), "{page}");
    assert!(
        page["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["source"] == "imported")
    );
}
#[sqlx::test(migrations = "./migrations")]
async fn native_checkpoints_preserve_values_without_imported_fallback(pool: PgPool) {
    let (id, _) = imported(&pool).await;
    let job = store::create_match_job(&pool, id, 2, 2).await.unwrap();
    sqlx::query("UPDATE jobs SET status='running' WHERE id=$1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let position: Uuid =
        sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 AND game_no=1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let decision = json!({"decision_no":2,"checker_no":1,"player":0,"kind":"checker","played":"8/5 6/5","best":"6/3 6/5","played_equity":0.1,"best_equity":0.2,"equity_loss":0.1,"severity":"none","dice":[3,1],"evaluation_status":"evaluated"});
    sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,1,2,0,'checker',0.1,$3)").bind(job.id).bind(position).bind(decision).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,1)")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let uri = format!(
        "/api/matches/{id}/review-decisions?job_id={}&errors_only=true",
        job.id
    );
    let (status, page) = call(&app(&pool), "GET", &uri, Value::Null).await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["items"][0]["source"], "native");
    assert_eq!(page["items"][0]["equity_loss"], 0.1);
    assert_eq!(page["items"][0]["severity"], "blunder");
    assert_eq!(page["items"][0]["depth_plies"], 2);
    sqlx::query(
        "UPDATE move_analyses SET data=jsonb_set(data,'{player}','1'::jsonb) WHERE job_id=$1",
    )
    .bind(job.id)
    .execute(&pool)
    .await
    .unwrap();
    let (_, invalid) = call(&app(&pool), "GET", &format!("{uri}&game_no=1"), Value::Null).await;
    assert_eq!(
        invalid["items"],
        json!([]),
        "mismatched native perspective is unavailable"
    );
}
#[sqlx::test(migrations = "./migrations")]
async fn native_cube_checkpoints_recover_exact_source_context(pool: PgPool) {
    let sgf = "(;GM[6]PW[Alice]PB[Bob]MI[length:7];W[31hefe];B[double];W[take];B[42qusu])";
    let id = store::import_match(
        &pool,
        "cube.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    let job = store::create_match_job(&pool, id, 1, 2).await.unwrap();
    sqlx::query("UPDATE jobs SET status='running' WHERE id=$1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    for (no, checker, player, kind, played, best) in [
        (1, Some(1), 0, "cube", "no double", "double"),
        (2, Some(1), 0, "checker", "8/5 6/5", "6/3 6/5"),
        (3, None, 1, "cube", "double", "no double"),
        (4, None, 0, "cube", "take", "drop"),
    ] {
        let mut decision = json!({"decision_no":no,"checker_no":checker,"player":player,"kind":kind,"played":played,"best":best,"played_equity":0.0,"best_equity":0.2,"equity_loss":0.2,"severity":"very_bad","dice":null,"evaluation_status":"evaluated"});
        if played == "take" {
            decision["played_equity"] = json!(-1.2);
            decision["best_equity"] = json!(-1.0);
            decision["cube_evaluation"] = json!({
                "unit": "cubeful", "no_double": -0.9, "double_take": -1.2, "double_pass": -1.0
            });
        }
        sqlx::query("INSERT INTO move_analyses(job_id,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,1,$2,$3,$4,0.2,$5)")
            .bind(job.id).bind(no).bind(player).bind(kind).bind(decision).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,1)")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, page) = call(
        &app(&pool),
        "GET",
        &format!(
            "/api/matches/{id}/review-decisions?job_id={}&kind=cube&player=0&errors_only=true",
            job.id
        ),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{page}");
    let rows = page["items"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["played"], "no double");
    assert!(rows[0].get("cube_evaluation").is_none());
    assert_eq!(rows[0]["review_position"]["dice"], json!([0, 0]));
    assert_eq!(rows[0]["review_position"]["ply"], 1);
    let (event, match_id): (Uuid, String) = sqlx::query_as(
        "SELECT id,gnubg_match_id FROM match_moves WHERE match_uuid=$1 AND notation='Take'",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rows[1]["move_id"], json!(event));
    assert_eq!(rows[1]["review_position"]["gnubg_match_id"], match_id);
    assert_eq!(rows[1]["review_position"]["player"], 0);
    assert_eq!(rows[1]["cube_evaluation"]["double_take"], -1.2);
    assert_eq!(rows[1]["cube_evaluation"]["double_pass"], -1.0);
}
#[sqlx::test(migrations = "./migrations")]
async fn imported_cube_uses_validated_actor_loss_and_source_event(pool: PgPool) {
    let (id, _) = imported(&pool).await;
    sqlx::query("INSERT INTO imported_analyses(match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data) SELECT match_uuid,position_uuid,2,game_no,source_move_no,'cube',(data - 'gnubg_match_id') || $2 FROM imported_analyses WHERE match_uuid=$1 AND game_no=1")
        .bind(id).bind(json!({"kind":"cube","played":"no double","dice":[0,0],"candidates":[],"cube":{"equity_kind":"cubeful","perspective":"doubler","no_double":0.4,"double_take":0.6,"double_pass":1.0}})).execute(&pool).await.unwrap();
    let uri = format!("/api/matches/{id}/review-decisions?kind=cube&errors_only=true");
    let (status, page) = call(&app(&pool), "GET", &uri, Value::Null).await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    let row = &page["items"][0];
    assert_eq!(row["played"], "no double");
    assert_eq!(row["best"], "double");
    assert_eq!(row["player"], 0);
    assert_eq!(
        row["cube_evaluation"],
        json!({"unit":"cubeful","no_double":0.4,"double_take":0.6,"double_pass":1.0})
    );
    assert!(row["dice"].is_null());
    assert!(row["move_id"].is_string());
    assert_eq!(row["review_position"]["dice"], json!([0, 0]));
    assert_eq!(row["review_position"]["player"], 0);
    assert!((row["equity_loss"].as_f64().unwrap() - 0.2).abs() < 1e-9);
    sqlx::query("UPDATE imported_analyses SET data=jsonb_set(data,'{cube,perspective}','\"unknown\"') WHERE match_uuid=$1 AND kind='cube'").bind(id).execute(&pool).await.unwrap();
    let (_, unknown) = call(&app(&pool), "GET", &uri, Value::Null).await;
    assert_eq!(unknown["items"], json!([]));
}
#[sqlx::test(migrations = "./migrations")]
async fn cursor_rejects_changed_selection_and_foreign_jobs(pool: PgPool) {
    let (id, _) = imported(&pool).await;
    let router = app(&pool);
    let uri = format!("/api/matches/{id}/review-decisions?limit=1");
    let (_, page) = call(&router, "GET", &uri, Value::Null).await;
    let cursor = page["next_cursor"].as_str().unwrap();
    sqlx::query("UPDATE matches SET source_revision=source_revision+1 WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(
            &router,
            "GET",
            &format!("{uri}&after={cursor}"),
            Value::Null
        )
        .await
        .0,
        422
    );
    assert_eq!(
        call(
            &router,
            "GET",
            &format!("{uri}&analysis_id=job:{}", Uuid::new_v4()),
            Value::Null
        )
        .await
        .0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn native_forced_and_unavailable_rows_keep_status_and_never_become_errors(pool: PgPool) {
    let source = include_str!("fixtures/analysis-game1.sgf");
    let id = store::import_match(
        &pool,
        "forced.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &import::parse_sgf(source).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    let job = store::create_match_job(&pool, id, 1, 1).await.unwrap();
    sqlx::query("UPDATE jobs SET status='running' WHERE id=$1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let (position,player,ply):(Uuid,i32,i32)=sqlx::query_as("SELECT id,player,ply FROM positions WHERE match_uuid=$1 AND notation='Cannot move' LIMIT 1").bind(id).fetch_one(&pool).await.unwrap();
    for (decision_no, status) in [(1, "forced"), (2, "unavailable")] {
        let d = json!({"decision_no":decision_no,"checker_no":ply,"player":player,"kind":"checker","played":"Cannot move","best":"Cannot move","played_equity":null,"best_equity":null,"equity_loss":0.0,"severity":"none","dice":null,"evaluation_status":status});
        sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,1,$3,$4,'checker',0,$5)").bind(job.id).bind(position).bind(decision_no).bind(player).bind(d).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,1)")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let uri = format!("/api/matches/{id}/review-decisions?job_id={}", job.id);
    let (_, page) = call(&app(&pool), "GET", &uri, Value::Null).await;
    assert_eq!(page["items"].as_array().unwrap().len(), 2, "{page}");
    assert_eq!(page["items"][0]["evaluation_status"], "forced");
    assert_eq!(page["items"][1]["evaluation_status"], "unavailable");
    assert!(page["items"][1]["played_equity"].is_null());
    let (_, errors) = call(
        &app(&pool),
        "GET",
        &format!("{uri}&errors_only=true"),
        Value::Null,
    )
    .await;
    assert_eq!(errors["items"], json!([]));
}
#[sqlx::test(migrations = "./migrations")]
async fn sgf_decimal_threshold_errors_are_not_lost_to_float_subtraction(pool: PgPool) {
    let (id, _) = imported(&pool).await;
    sqlx::query("UPDATE matches SET format='sgf' WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE imported_analyses SET data=jsonb_set(jsonb_set(data,'{candidates,0,equity}','0.15'),'{candidates,1,equity}','0.145') WHERE match_uuid=$1")
        .bind(id).execute(&pool).await.unwrap();
    let (_, page) = call(
        &app(&pool),
        "GET",
        &format!("/api/matches/{id}/review-decisions?errors_only=true&severity=doubtful"),
        Value::Null,
    )
    .await;
    assert_eq!(
        page["items"].as_array().unwrap().len(),
        2,
        "0.15 minus 0.145 is exactly the Questionable boundary: {page}"
    );
}

/// Every review decision of a filter, in order, from cursor pages, which
/// scan the analysis without the numbered pages' hit set.
async fn cursor_pages(app: &axum::Router, base: &str) -> Vec<Value> {
    let mut items = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let uri = match &after {
            Some(cursor) => format!("{base}&limit=50&after={cursor}"),
            None => format!("{base}&limit=50"),
        };
        let (status, page) = call(app, "GET", &uri, Value::Null).await;
        assert_eq!(status, 200, "{uri}: {page}");
        items.extend(page["items"].as_array().unwrap().iter().cloned());
        match page["next_cursor"].as_str() {
            Some(cursor) => after = Some(cursor.to_owned()),
            None => return items,
        }
    }
}

/// Numbered pages must return the cursor order, total and an empty page after
/// the last one.
async fn assert_numbered_pages_match_cursor(app: &axum::Router, base: &str) -> usize {
    let expected = cursor_pages(app, base).await;
    let mut numbered = Vec::new();
    let pages = expected.len().div_ceil(50).max(1);
    for page in 1..=pages + 1 {
        let uri = format!("{base}&limit=50&page={page}");
        let (status, body) = call(app, "GET", &uri, Value::Null).await;
        assert_eq!(status, 200, "{uri}: {body}");
        assert_eq!(body["total"], expected.len(), "{uri}");
        assert!(body["next_cursor"].is_null(), "{uri}");
        let items = body["items"].as_array().unwrap();
        if page > pages || expected.is_empty() {
            assert!(items.is_empty(), "{uri}");
        }
        numbered.extend(items.iter().cloned());
    }
    assert_eq!(numbered, expected, "{base}");
    expected.len()
}

#[sqlx::test(migrations = "./migrations")]
async fn numbered_pages_follow_the_cursor_order_for_every_filter(pool: PgPool) {
    let bytes = include_bytes!("fixtures/WC2014.xg");
    let sgf = import::xg_to_sgf(bytes).unwrap();
    let id = store::import_match(
        &pool,
        "WC2014.xg",
        "xg",
        bytes,
        &sgf,
        &import::parse_sgf(&sgf).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    let app = app(&pool);
    let base = format!("/api/matches/{id}/review-decisions?analysis_id=imported:{id}");
    assert_eq!(assert_numbered_pages_match_cursor(&app, &base).await, 738);
    let built: String = sqlx::query_scalar(
        "SELECT indexed_at::text FROM review_hit_sets WHERE match_id=$1 AND analysis_id=$2",
    )
    .bind(id)
    .bind(format!("imported:{id}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut sizes = Vec::new();
    for filter in [
        "&errors_only=true",
        "&severity=doubtful",
        "&severity=blunder",
        "&game_no=3",
        "&player=1",
        "&kind=cube",
        "&kind=checker&errors_only=true&player=0",
        "&game_no=5&kind=cube&severity=bad",
        "&game_no=1&player=0&severity=very_bad",
    ] {
        sizes.push(assert_numbered_pages_match_cursor(&app, &format!("{base}{filter}")).await);
    }
    assert_eq!(sizes[0], 74, "{sizes:?}");
    assert!(sizes.contains(&0), "{sizes:?}");
    // The hit set was built once; later pages and filters reuse it.
    let reused: String = sqlx::query_scalar(
        "SELECT indexed_at::text FROM review_hit_sets WHERE match_id=$1 AND analysis_id=$2",
    )
    .bind(id)
    .bind(format!("imported:{id}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(reused, built);
}

#[sqlx::test(migrations = "./migrations")]
async fn review_query_plans_use_the_current_page_parameters(pool: PgPool) {
    let (id, _) = imported(&pool).await;
    let session = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with((*pool.connect_options()).clone())
        .await
        .unwrap();
    let app = app(&session);
    let base = format!("/api/matches/{id}/review-decisions?analysis_id=imported:{id}&limit=1");
    for _ in 0..6 {
        let (status, numbered) = call(&app, "GET", &format!("{base}&page=1"), Value::Null).await;
        assert_eq!(status, 200, "{numbered}");
        assert_eq!(numbered["total"], 2);
        let (status, cursor) = call(&app, "GET", &base, Value::Null).await;
        assert_eq!(status, 200, "{cursor}");
        assert_eq!(numbered["items"], cursor["items"]);
    }
    let cached: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pg_prepared_statements WHERE statement LIKE $1")
            .bind("%WITH native_order AS (%")
            .fetch_one(&session)
            .await
            .unwrap();
    assert_eq!(
        cached, 0,
        "the review query must keep parameter-specific plans"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn numbered_pages_follow_native_checkpoints_and_in_place_updates(pool: PgPool) {
    let (id, _) = imported(&pool).await;
    let job = store::create_match_job(&pool, id, 2, 2).await.unwrap();
    sqlx::query("UPDATE jobs SET status='running' WHERE id=$1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    for game in 1..=2 {
        let position: Uuid =
            sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 AND game_no=$2")
                .bind(id)
                .bind(game)
                .fetch_one(&pool)
                .await
                .unwrap();
        let decision = json!({"decision_no":2,"checker_no":1,"player":0,"kind":"checker","played":"8/5 6/5","best":"6/3 6/5","played_equity":0.1,"best_equity":0.2,"equity_loss":0.1,"severity":"none","dice":[3,1],"evaluation_status":"evaluated"});
        sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,$3,2,0,'checker',0.1,$4)").bind(job.id).bind(position).bind(game).bind(decision).execute(&pool).await.unwrap();
    }
    // Only game 1 is checkpointed; the saved game 2 decision is incomplete.
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,1)")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let app = app(&pool);
    let base = format!("/api/matches/{id}/review-decisions?job_id={}", job.id);
    assert_eq!(assert_numbered_pages_match_cursor(&app, &base).await, 1);
    assert_eq!(
        assert_numbered_pages_match_cursor(&app, &format!("{base}&errors_only=true")).await,
        1
    );
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,2)")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(assert_numbered_pages_match_cursor(&app, &base).await, 2);
    // An in-place change drops the hit set; the invalid row is not counted.
    sqlx::query("UPDATE move_analyses SET data=jsonb_set(data,'{player}','1'::jsonb) WHERE job_id=$1 AND game_no=2")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM review_hit_sets WHERE match_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(assert_numbered_pages_match_cursor(&app, &base).await, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn imported_nocube_gnubg_checker_rows_cover_the_game(pool: PgPool) {
    // The played candidates from GNUbg 1.08.003's cubeless 0-ply export of
    // the two-checker SGF. Its "0" evaluation flags have no cubeful C suffix.
    let sgf = "(;GM[6]FF[4]MI[length:0]RU[NoCube]AP[GNU Backgammon:1.08.003];W[31hefe]A[0][hefe E ver 3 0.551469 0.173548 0.012695 0.124446 0.005039 0.159696 0 0 1 0.000000 1];B[42qusu]A[0][qusu E ver 3 0.455755 0.134102 0.007872 0.175897 0.008552 -0.130966 0 0 1 0.000000 1])";
    let id = store::import_match(
        &pool,
        "cubeless-analysis.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    let (status, data) = call(
        &app(&pool),
        "GET",
        &format!("/api/matches/{id}/analysis-statistics?analysis_id=imported:{id}"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{data}");
    assert_eq!(data["games"][0]["analyzed"], true, "{data}");
    assert_eq!(data["summary"]["analyzed_moves"], 2);
    for player in data["summary"]["players"].as_array().unwrap() {
        assert_eq!(player["cube_decisions"], 0);
    }
}
