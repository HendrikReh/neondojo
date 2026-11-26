use super::recurring_mistakes::prepare_decision_index;
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
async fn decision_index_tracks_in_place_imported_changes(pool: PgPool) {
    let (_, ids) = imported(&pool).await;
    let router = app(&pool);
    let search = || {
        call(
            &router,
            "GET",
            "/api/study/decisions?player=Alice",
            Value::Null,
        )
    };
    prepare_decision_index(&pool).await;
    let (status, before) = search().await;
    assert_eq!(status, 200, "{before}");
    assert_eq!(before["total"], 2);

    sqlx::query(
        "UPDATE imported_analyses
         SET data=jsonb_set(data,'{engine_version}',to_jsonb('9.9'::text)) WHERE id=$1",
    )
    .bind(ids[0])
    .execute(&pool)
    .await
    .unwrap();
    prepare_decision_index(&pool).await;
    let (status, after) = search().await;
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["total"], 2);
    let updated = after["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["reference"]["imported_analysis_id"] == ids[0].to_string())
        .unwrap();
    assert!(
        updated["engine"].as_str().unwrap().contains("9.9"),
        "{updated}"
    );

    sqlx::query("DELETE FROM imported_analyses WHERE id=$1")
        .bind(ids[0])
        .execute(&pool)
        .await
        .unwrap();
    prepare_decision_index(&pool).await;
    let (status, after_delete) = search().await;
    assert_eq!(status, 200, "{after_delete}");
    assert_eq!(after_delete["total"], 1);
    assert_eq!(
        after_delete["items"][0]["reference"]["imported_analysis_id"],
        ids[1].to_string()
    );
}

async fn checkpoint(pool: &PgPool, id: Uuid, game: i32) {
    let job = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,result,game_no,total_games,completed_games) VALUES($1,$2,'match',1,'completed','{}',$3,1,1)").bind(job).bind(id).bind(game).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,$2)")
        .bind(job)
        .bind(game)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO move_analyses(job_id,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,1,0,'checker',0,'{\"evaluation_status\":\"unavailable\"}')").bind(job).bind(game).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO analysis_selections(match_id,analysis_id) VALUES($1,$2) ON CONFLICT(match_id) DO UPDATE SET analysis_id=EXCLUDED.analysis_id").bind(id).bind(format!("job:{job}")).execute(pool).await.unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn imports_are_eligible_private_and_reveal_preserves_metadata(pool: PgPool) {
    let (_, ids) = imported(&pool).await;
    let router = app(&pool);
    let (status, choices) = call(&router, "GET", "/api/study/mistakes", Value::Null).await;
    assert_eq!(status, 200, "{choices}");
    assert_eq!(choices["players"], json!(["Alice", "Bob"]));
    prepare_decision_index(&pool).await;
    let (status, groups) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{groups}");
    assert_eq!(groups["analyzed_decisions"], 2, "{groups}");
    assert_eq!(groups["groups"][0]["count"], 2);
    let (_,practice)=call(&router,"POST","/api/study/mistakes/practice",json!({"player":"Alice","game_plan":groups["groups"][0]["game_plan"],"mistake_kind":"checker"})).await;
    assert_eq!(practice["items"].as_array().unwrap().len(), 2, "{practice}");
    let q = &practice["items"][0];
    assert!(q["reference"].get("job_id").is_none());
    assert!(
        ids.iter()
            .any(|id| q["reference"]["imported_analysis_id"] == id.to_string())
    );
    assert_eq!(q["source"], "xg");
    assert_eq!(q["plies"], Value::Null);
    assert_eq!(q["depth"], "XGRoller++");
    assert_eq!(q["method"], "evaluation");
    assert!(q["engine"].as_str().unwrap().contains("2.19"));
    for key in ["best", "played", "equity_loss"] {
        assert!(q.get(key).is_none());
    }
    let (status, a) = call(
        &router,
        "POST",
        "/api/study/mistakes/reveal",
        q["reference"].clone(),
    )
    .await;
    assert_eq!(status, 200, "{a}");
    assert_eq!(a["equity_loss"], 0.2);
    for key in ["engine", "plies", "depth", "method", "source"] {
        assert_eq!(a[key], q[key]);
    }
    let (status, checked) = call(
        &router,
        "POST",
        "/api/study/mistakes/attempt",
        json!({
            "reference":q["reference"],
            "answer":{"kind":"checker","steps":[{"from":6,"to":3},{"from":6,"to":5}]}
        }),
    )
    .await;
    assert_eq!(status, 200, "{checked}");
    assert_eq!(checked["evaluation"]["verdict"], "best");
    assert_eq!(checked["evaluation"]["provenance"]["kind"], "stored");
    assert_eq!(checked["evaluation"]["provenance"]["plies"], Value::Null);
    assert_eq!(checked["original"]["depth"], "XGRoller++");
    for table in ["learning_cards", "decks", "card_reviews", "jobs"] {
        let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
}
#[sqlx::test(migrations = "./migrations")]
async fn selected_native_checkpoint_stales_all_imported_references_without_cross_game_fallback(
    pool: PgPool,
) {
    let (id, ids) = imported(&pool).await;
    let router = app(&pool);
    checkpoint(&pool, id, 1).await;
    prepare_decision_index(&pool).await;
    let (_, groups) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(groups["analyzed_decisions"], 0, "{groups}");
    assert_eq!(groups["groups"], json!([]));
    assert_eq!(
        call(
            &router,
            "POST",
            "/api/study/mistakes/reveal",
            json!({"imported_analysis_id":ids[0],"game_no":1,"decision_no":1})
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(
            &router,
            "POST",
            "/api/study/mistakes/reveal",
            json!({"imported_analysis_id":ids[1],"game_no":2,"decision_no":1})
        )
        .await
        .0,
        404
    );
    checkpoint(&pool, id, 2).await;
    prepare_decision_index(&pool).await;
    assert_eq!(
        call(
            &router,
            "GET",
            "/api/study/mistakes?player=Alice",
            Value::Null
        )
        .await
        .1["groups"],
        json!([])
    );
}
#[sqlx::test(migrations = "./migrations")]
async fn imported_reference_validation_and_visibility(pool: PgPool) {
    let (id, ids) = imported(&pool).await;
    let router = app(&pool);
    for reference in [
        json!({"game_no":1,"decision_no":1}),
        json!({"job_id":Uuid::new_v4(),"imported_analysis_id":ids[0],"game_no":1,"decision_no":1}),
        json!({"imported_analysis_id":ids[0],"game_no":0,"decision_no":1}),
    ] {
        assert_eq!(
            call(&router, "POST", "/api/study/mistakes/reveal", reference)
                .await
                .0,
            422
        );
    }
    let reference = json!({"imported_analysis_id":ids[0],"game_no":1,"decision_no":1});
    assert_eq!(
        call(
            &router,
            "POST",
            "/api/study/mistakes/reveal",
            json!({"imported_analysis_id":ids[0],"game_no":2,"decision_no":1})
        )
        .await
        .0,
        404
    );
    sqlx::query("INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id,library_match_id) VALUES($1,$2,'/unavailable','','saved',$3)").bind(Uuid::new_v4()).bind(json!({"phase":"move","play_mode":"competition"})).bind(id).execute(&pool).await.unwrap();
    assert_eq!(
        call(&router, "GET", "/api/study/mistakes", Value::Null)
            .await
            .1["players"],
        json!([])
    );
    assert_eq!(
        call(
            &router,
            "POST",
            "/api/study/mistakes/reveal",
            reference.clone()
        )
        .await
        .0,
        404
    );
    sqlx::query("DELETE FROM play_sessions WHERE library_match_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM matches WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(&router, "POST", "/api/study/mistakes/reveal", reference)
            .await
            .0,
        404
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn real_binary_xg_legacy_catalog_is_loaded_without_engine_jobs(pool: PgPool) {
    let original = include_bytes!("fixtures/WC2014.xg");
    let sgf = import::xg_to_sgf(original).unwrap();
    let parsed = import::parse_sgf(&sgf).unwrap();
    let id = store::import_match(&pool, "WC2014.xg", "xg", original, &sgf, &parsed)
        .await
        .unwrap()
        .r#match
        .id;
    sqlx::query("DELETE FROM imported_analyses WHERE match_uuid=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM match_games WHERE match_uuid=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE matches SET imported_analysis_loaded=false,moves_cataloged=false WHERE id=$1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let router = app(&pool);
    let (status, choices) = call(&router, "GET", "/api/study/mistakes", Value::Null).await;
    assert_eq!(status, 200, "{choices}");
    assert_eq!(choices["players"].as_array().unwrap().len(), 2);
    let loaded: bool =
        sqlx::query_scalar("SELECT imported_analysis_loaded FROM matches WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!loaded, "choices must not parse legacy originals");
    let player = choices["players"][0].as_str().unwrap().replace(' ', "%20");
    prepare_decision_index(&pool).await;
    let (status, groups) = call(
        &router,
        "GET",
        &format!("/api/study/mistakes?player={player}"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{groups}");
    assert!(
        groups["analyzed_decisions"].as_u64().unwrap() > 0,
        "{groups}"
    );
    assert!(!groups["groups"].as_array().unwrap().is_empty(), "{groups}");
    let loaded: bool =
        sqlx::query_scalar("SELECT imported_analysis_loaded FROM matches WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(loaded);
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(jobs, 0);
}
#[sqlx::test(migrations = "./migrations")]
async fn cataloged_imports_use_source_events_without_fetching_large_analysis_artifacts(
    pool: PgPool,
) {
    let (id, _) = imported(&pool).await;
    prepare_decision_index(&pool).await;
    let router = app(&pool);
    assert_eq!(
        call(
            &router,
            "GET",
            "/api/study/mistakes?player=Alice",
            Value::Null
        )
        .await
        .0,
        200
    );
    sqlx::query("UPDATE matches SET sgf=repeat('x',67108865) WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, groups) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{groups}");
    assert_eq!(groups["analyzed_decisions"], 2, "{groups}");
}
#[sqlx::test(migrations = "./migrations")]
async fn unknown_units_and_source_identity_mismatches_are_not_counted(pool: PgPool) {
    let (_, ids) = imported(&pool).await;
    let router = app(&pool);
    sqlx::query("UPDATE imported_analyses SET data=jsonb_set(data,'{candidates,0,equity_kind}','\"mwc\"') WHERE id=$1").bind(ids[0]).execute(&pool).await.unwrap();
    sqlx::query(
        "UPDATE imported_analyses SET data=jsonb_set(data,'{source_move_no}','2') WHERE id=$1",
    )
    .bind(ids[1])
    .execute(&pool)
    .await
    .unwrap();
    prepare_decision_index(&pool).await;
    let (status, groups) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{groups}");
    assert_eq!(groups["groups"], json!([]));
    assert_eq!(groups["analyzed_decisions"], 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn legacy_catalog_batches_make_progress_without_returning_partial_counts(pool: PgPool) {
    for n in 0..17 {
        let sgf = format!("(;GM[6]PW[Alice]PB[Bob]MI[length:100][ws:{n}];W[31hefe])");
        let id = store::import_match(
            &pool,
            &format!("legacy-{n}.sgf"),
            "sgf",
            sgf.as_bytes(),
            &sgf,
            &import::parse_sgf(&sgf).unwrap(),
        )
        .await
        .unwrap()
        .r#match
        .id;
        sqlx::query("UPDATE matches SET format='xg',imported_analysis_loaded=true,moves_cataloged=false WHERE id=$1").bind(id).execute(&pool).await.unwrap();
    }
    // Put an unreadable source at the start of the worker's UUID ordering.
    let damaged: Uuid = sqlx::query_scalar(
        "UPDATE matches SET sgf='invalid' WHERE id=(SELECT id FROM matches ORDER BY id LIMIT 1)
         RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let router = app(&pool);
    let (status, error) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 503, "{error}");
    assert_eq!(error["code"], "index_preparing");
    let mut cursor = None;
    assert!(
        neondojo::recurring_mistakes::backfill_batch(&pool, &mut cursor)
            .await
            .unwrap_or_else(|_| panic!("decision index backfill failed"))
    );
    let done: i64 = sqlx::query_scalar("SELECT count(*) FROM matches WHERE moves_cataloged")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(done, 15);
    let (status, _) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 503);
    assert!(
        !neondojo::recurring_mistakes::backfill_batch(&pool, &mut cursor)
            .await
            .unwrap_or_else(|_| panic!("decision index backfill failed"))
    );
    assert!(cursor.is_none());
    let pending: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM matches WHERE NOT moves_cataloged")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        pending,
        [damaged],
        "the failing source must not block later batches"
    );
    sqlx::query("UPDATE matches SET sgf=convert_from(original,'UTF8') WHERE id=$1")
        .bind(damaged)
        .execute(&pool)
        .await
        .unwrap();
    prepare_decision_index(&pool).await;
    let (status, result) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{result}");
    let done: i64 = sqlx::query_scalar("SELECT count(*) FROM matches WHERE moves_cataloged")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(done, 17);
}

#[sqlx::test(migrations = "./migrations")]
async fn foreign_position_ownership_and_best_candidate_method_are_preserved(pool: PgPool) {
    let (_, ids) = imported(&pool).await;
    let router = app(&pool);
    sqlx::query("UPDATE imported_analyses SET position_uuid=(SELECT position_uuid FROM imported_analyses WHERE id=$2) WHERE id=$1").bind(ids[0]).bind(ids[1]).execute(&pool).await.unwrap();
    sqlx::query("UPDATE imported_analyses SET data=jsonb_set(data,'{candidates,0,method}','\"rollout\"') WHERE id=$1").bind(ids[1]).execute(&pool).await.unwrap();
    prepare_decision_index(&pool).await;
    let (status, groups) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{groups}");
    assert_eq!(groups["analyzed_decisions"], 1, "{groups}");
    assert_eq!(
        call(
            &router,
            "POST",
            "/api/study/mistakes/reveal",
            json!({"imported_analysis_id":ids[0],"game_no":1,"decision_no":1})
        )
        .await
        .0,
        404
    );
    let (status, answer) = call(
        &router,
        "POST",
        "/api/study/mistakes/reveal",
        json!({"imported_analysis_id":ids[1],"game_no":2,"decision_no":1}),
    )
    .await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["method"], "rollout");
}

#[sqlx::test(migrations = "./migrations")]
async fn imported_decision_hits_become_cards_with_their_imported_solution(pool: PgPool) {
    let (_, ids) = imported(&pool).await;
    prepare_decision_index(&pool).await;
    let router = app(&pool);
    let (status, page) = call(
        &router,
        "GET",
        "/api/study/decisions?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{page}");
    let references: Vec<Value> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["reference"].clone())
        .collect();
    assert_eq!(references.len(), 2, "{page}");
    assert!(references.iter().all(|r| r.get("job_id").is_none()));
    let (_, deck) = call(&router, "POST", "/api/decks", json!({"name":"Imported"})).await;
    let path = format!("/api/decks/{}/decisions", deck["id"].as_str().unwrap());
    let mut stale = references[0].clone();
    stale["decision_no"] = json!(999);
    let mut body = references.clone();
    body.push(stale);
    let (status, result) = call(&router, "POST", &path, json!({"decisions": body})).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["added"], 2, "{result}");
    assert_eq!(result["items"][2]["status"], "not_found", "{result}");
    let sources: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT payload#>>'{source,kind}', payload#>>'{source,imported_analysis_id}' FROM learning_cards ORDER BY 2",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(sources.len(), 2);
    for (kind, id) in sources {
        assert_eq!(kind, "imported");
        assert!(ids.iter().any(|known| Some(known.to_string()) == id));
    }
    let (_, again) = call(&router, "POST", &path, json!({"decisions": references})).await;
    assert_eq!(again["added"], 0, "{again}");
    assert_eq!(again["total"], 2, "{again}");
}

#[sqlx::test(migrations = "./migrations")]
async fn imported_match_cards_add_for_the_whole_match_and_one_player(pool: PgPool) {
    let (match_id, _) = imported(&pool).await;
    let router = app(&pool);
    let (_, deck) = call(&router, "POST", "/api/decks", json!({"name":"Match"})).await;
    let path = format!("/api/decks/{}/cards", deck["id"].as_str().unwrap());
    let (status, alice) = call(
        &router,
        "POST",
        &path,
        json!({"match_id": match_id, "player": 0}),
    )
    .await;
    assert_eq!(status, 200, "{alice}");
    let (status, all) = call(&router, "POST", &path, json!({"match_id": match_id})).await;
    assert_eq!(status, 200, "{all}");
    assert!(all["total"].as_i64().unwrap() >= alice["total"].as_i64().unwrap());
    let players: Vec<i32> = sqlx::query_scalar(
        "SELECT DISTINCT (payload#>>'{position,player}')::int FROM learning_cards ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(!players.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn decision_batch_reads_only_requested_primary_imports(pool: PgPool) {
    let (_, ids) = imported(&pool).await;
    // A secondary evaluation for the requested move must stay secondary even
    // when the client explicitly names its ID.
    let secondary = Uuid::new_v4();
    sqlx::query("INSERT INTO imported_analyses(id, match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data)
        SELECT $2,match_uuid,position_uuid,90,game_no,source_move_no,kind,data FROM imported_analyses WHERE id=$1")
        .bind(ids[0]).bind(secondary).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO imported_analyses(match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data)
        SELECT match_uuid,position_uuid,91,game_no,2,kind,data FROM imported_analyses WHERE id=$1")
        .bind(ids[0]).execute(&pool).await.unwrap();
    // Evaluating the unrequested primary row's payload raises a database
    // error. The selected row and its primary/secondary ranking stay real.
    sqlx::raw_sql("ALTER VIEW primary_imported_analyses RENAME TO fixture_primary_analyses;
        CREATE VIEW primary_imported_analyses AS SELECT id, match_uuid, position_uuid, source_ordinal, game_no, source_move_no, kind,
        CASE WHEN source_ordinal = 91 THEN to_jsonb(id::text::integer) ELSE data END AS data FROM fixture_primary_analyses")
        .execute(&pool).await.unwrap();
    let router = app(&pool);
    let (_, deck) = call(
        &router,
        "POST",
        "/api/decks",
        json!({"name":"Selected imports"}),
    )
    .await;
    let requested = [(ids[0], 1, 1), (ids[0], 1, 77), (secondary, 1, 1), (ids[0], 2, 1), (ids[0], 1, 1)]
        .map(|(id, game, decision)| json!({"imported_analysis_id":id,"game_no":game,"decision_no":decision}));
    let (status, result) = call(
        &router,
        "POST",
        &format!("/api/decks/{}/decisions", deck["id"].as_str().unwrap()),
        json!({"decisions":requested}),
    )
    .await;
    assert_eq!(status, 200, "{result}");
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), requested.len());
    for ((item, reference), expected) in items.iter().zip(requested).zip([
        "added",
        "not_found",
        "not_found",
        "not_found",
        "duplicate",
    ]) {
        assert_eq!(item["reference"], reference);
        assert_eq!(item["status"], expected, "{result}");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn decision_gets_report_a_stale_index_on_read_only_connections(pool: PgPool) {
    imported(&pool).await;
    let reader = sqlx::postgres::PgPoolOptions::new()
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET default_transaction_read_only = on")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let router = app(&reader);
    let paths = [
        "/api/study/mistakes?player=Alice",
        "/api/study/decisions?player=Alice",
    ];
    prepare_decision_index(&pool).await;
    sqlx::query("UPDATE decision_index_matches SET fingerprint='old-version'")
        .execute(&pool)
        .await
        .unwrap();
    for path in paths {
        let (status, body) = call(&router, "GET", path, Value::Null).await;
        assert_eq!(status, 503, "{path} {body}");
        assert_eq!(body["code"], "index_preparing");
    }
    prepare_decision_index(&pool).await;
    for path in paths {
        let (status, body) = call(&router, "GET", path, Value::Null).await;
        assert_eq!(status, 200, "{path} {body}");
    }
}
