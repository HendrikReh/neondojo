use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{
    api::{self, AppState},
    engine::Engine,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

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
fn snapshot(id: Uuid) -> Value {
    json!({
        "id":id,"version":3,"human_player":0,"human_name":"Archive learner",
        "match_length":7,"bearoff_direction":"right","engine":{"id":"gnubg","version":"saved engine","plies":1},
        "phase":"match_over","can_double":false,"legal_moves":[],"history":[],"scores":[7,3],"game_no":2,"winner":0,
        "position":{"id":id,"match_uuid":id,"ordinal":0,"game_no":2,"ply":0,"player":0,"dice":[],"board":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"position_id":"4HPwATDgc/ABMA","gnubg_match_id":"cAgAAAAAAAAA","notation":""},
        "play_mode":"tutor","review":{"analysis_plies":2,"games":[],"assisted":true,"first_attempt_loss":0.12,"final_choice_loss":0.02,"first_attempt_count":3,"retry_count":1,"first_attempt_rating":4.5,"first_attempt_rating_decisions":3,"first_attempt_rating_method":"emg500","first_attempt_rating_profile":"saved-profile","independent_first_attempt_count":2,"assisted_first_attempt_count":1}
    })
}
async fn insert(pool: &PgPool, state: &Value, index: i32) {
    sqlx::query("INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id,created_at,updated_at) VALUES($1,$2,'/unavailable','invalid sgf deliberately','saved', '2026-01-01'::timestamptz + $3 * interval '1 second','2026-01-02'::timestamptz + $3 * interval '1 second')")
        .bind(state["id"].as_str().unwrap().parse::<Uuid>().unwrap()).bind(state).bind(index).execute(pool).await.unwrap();
}
fn app(pool: PgPool) -> axum::Router {
    api::router(AppState::new(pool, Engine::new("/unavailable".into())))
}

#[sqlx::test(migrations = "./migrations")]
async fn existing_reports_are_paginated_beyond_100_sessions_and_read_without_engine(pool: PgPool) {
    let router = app(pool.clone());
    let mut ids = Vec::new();
    for index in 0..105 {
        let id = Uuid::new_v4();
        insert(&pool, &snapshot(id), index).await;
        ids.push(id.to_string());
    }
    for (phase, review) in [
        ("match_over", Value::Null),
        ("move", snapshot(Uuid::new_v4())["review"].clone()),
    ] {
        let mut state = snapshot(Uuid::new_v4());
        state["phase"] = json!(phase);
        state["review"] = review;
        insert(&pool, &state, 200).await;
        assert_eq!(
            get(
                &router,
                &format!("/api/study/reports/{}", state["id"].as_str().unwrap())
            )
            .await
            .0,
            404
        );
    }
    let mut uri = "/api/study/reports?limit=40".to_string();
    let mut seen = Vec::new();
    loop {
        let (status, page) = get(&router, &uri).await;
        assert_eq!(status, 200, "{page}");
        for report in page["items"].as_array().unwrap() {
            assert_eq!(report["first_attempt_rating"], 4.5);
            assert_eq!(report["assisted"], true);
            assert_eq!(report["play_mode"], "tutor");
            assert_eq!(report["scores"], json!([7, 3]));
            assert!(
                report.get("review").is_none(),
                "list must only return summaries"
            );
            seen.push(report["id"].as_str().unwrap().to_string());
        }
        let Some(cursor) = page["next_cursor"].as_str() else {
            break;
        };
        uri = format!("/api/study/reports?limit=40&after={cursor}");
    }
    ids.reverse();
    assert_eq!(seen, ids);
    let (status, detail) = get(&router, &format!("/api/study/reports/{}", ids[0])).await;
    assert_eq!(status, 200, "{detail}");
    assert_eq!(detail["review"], snapshot(Uuid::new_v4())["review"]);
    assert_eq!(detail["version"], 3);
    assert_eq!(detail["engine"]["version"], "saved engine");
    // Rebuilding the application with an unavailable engine keeps the archive intact.
    let restarted = app(pool.clone());
    assert_eq!(
        get(&restarted, &format!("/api/study/reports/{}", ids[104]))
            .await
            .0,
        200
    );
    let version: i64 =
        sqlx::query_scalar("SELECT (state->>'version')::bigint FROM play_sessions WHERE id=$1")
            .bind(ids[0].parse::<Uuid>().unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(version, 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn validates_archive_queries_and_handles_empty_missing_and_legacy_reports(pool: PgPool) {
    let router = app(pool.clone());
    assert_eq!(
        get(&router, "/api/study/reports").await,
        (200, json!({"items":[],"next_cursor":null}))
    );
    for query in [
        "limit=0",
        "limit=101",
        "after=garbage",
        "after=bnVsbA",
        "limit=-1",
    ] {
        assert_eq!(
            get(&router, &format!("/api/study/reports?{query}")).await.0,
            422,
            "{query}"
        );
    }
    assert_eq!(
        get(&router, &format!("/api/study/reports/{}", Uuid::new_v4()))
            .await
            .0,
        404
    );
    let id = Uuid::new_v4();
    let mut state = snapshot(id);
    state.as_object_mut().unwrap().remove("play_mode");
    state["review"]["first_attempt_rating"] = Value::Null;
    insert(&pool, &state, 0).await;
    let (_, page) = get(&router, "/api/study/reports").await;
    assert_eq!(page["items"][0]["play_mode"], "competition");
    assert!(page["items"][0]["first_attempt_rating"].is_null());
    let (_, schema) = get(&router, "/api/openapi.json").await;
    assert!(schema["paths"]["/api/study/reports"].is_object());
    assert!(schema["paths"]["/api/study/reports/{id}"].is_object());
}

#[sqlx::test(migrations = "./migrations")]
async fn reports_remain_accessible_after_their_library_match_is_deleted(pool: PgPool) {
    let id = Uuid::new_v4();
    let match_id = Uuid::new_v4();
    let state = snapshot(id);
    insert(&pool, &state, 0).await;
    sqlx::query("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf,display_name) VALUES($1,'archive.sgf','sgf',$2,'Learner','GNUbg',7,0,''::bytea,'','My learning match')")
        .bind(match_id).bind(match_id.to_string()).execute(&pool).await.unwrap();
    sqlx::query("UPDATE play_sessions SET library_match_id=$1 WHERE id=$2")
        .bind(match_id)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let router = app(pool.clone());
    let (_, page) = get(&router, "/api/study/reports").await;
    assert_eq!(page["items"][0]["name"], "My learning match");
    assert_eq!(page["items"][0]["library_match_id"], json!(match_id));
    sqlx::query("DELETE FROM matches WHERE id=$1")
        .bind(match_id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, page) = get(&router, "/api/study/reports").await;
    assert_eq!(status, 200);
    assert_eq!(page["items"][0]["name"], "Archive learner · GNUbg");
    assert!(page["items"][0]["library_match_id"].is_null());
    let (status, report) = get(&router, &format!("/api/study/reports/{id}")).await;
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["review"], state["review"]);
    assert!(report["library_match_id"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
async fn match_metadata_exposes_only_its_existing_finished_report(pool: PgPool) {
    let router = app(pool.clone());
    for (index, phase, review, available) in [
        (
            0,
            "match_over",
            snapshot(Uuid::new_v4())["review"].clone(),
            true,
        ),
        (1, "match_over", Value::Null, false),
        (2, "move", snapshot(Uuid::new_v4())["review"].clone(), false),
    ] {
        let match_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();
        sqlx::query("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf) VALUES($1,'report.sgf','sgf',$2,'Hendrik','GNUbg',7,0,''::bytea,'')")
            .bind(match_id).bind(match_id.to_string()).execute(&pool).await.unwrap();
        let mut state = snapshot(session_id);
        state["phase"] = json!(phase);
        state["review"] = review;
        insert(&pool, &state, index).await;
        sqlx::query("UPDATE play_sessions SET library_match_id=$1 WHERE id=$2")
            .bind(match_id)
            .bind(session_id)
            .execute(&pool)
            .await
            .unwrap();
        let (status, metadata) = get(&router, &format!("/api/matches/{match_id}/metadata")).await;
        assert_eq!(status, 200, "{metadata}");
        assert!(
            metadata.get("study_report_id").is_some(),
            "metadata must explicitly indicate report availability"
        );
        assert_eq!(
            metadata["study_report_id"],
            if available {
                json!(session_id)
            } else {
                Value::Null
            }
        );
        assert_eq!(metadata["play_session_id"], json!(session_id));
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn money_report_groups_page_complete_sessions_with_authoritative_totals(pool: PgPool) {
    let session_a = Uuid::new_v4();
    let session_b = Uuid::new_v4();
    for (id, scores, ended) in [
        (session_a, json!([10, 6]), true),
        (session_b, json!([0, 2]), false),
    ] {
        let mut state = snapshot(id);
        state["match_length"] = json!(0);
        state["phase"] = json!("match_over");
        state["money_session_ended"] = json!(ended);
        state["scores"] = scores;
        if ended {
            state["review"] = Value::Null;
        }
        state["game_no"] = json!(1);
        insert(&pool, &state, if ended { 0 } else { 150 }).await;
    }
    for (session_id, game, scores, index) in [
        (session_a, 1, json!([3, 0]), 100),
        (session_a, 2, json!([3, 6]), 200),
        (session_a, 3, json!([8, 6]), 400),
    ] {
        let mut state = snapshot(session_id);
        state["match_length"] = json!(0);
        state["game_no"] = json!(game);
        state["scores"] = scores;
        sqlx::query("INSERT INTO study_report_snapshots(id,session_id,game_no,state,sgf,created_at,updated_at) VALUES($1,$2,$3,$4,'saved', '2026-01-01'::timestamptz + $5 * interval '1 second', '2026-01-02'::timestamptz + $5 * interval '1 second')")
            .bind(Uuid::new_v4()).bind(session_id).bind(game).bind(state).bind(index).execute(&pool).await.unwrap();
    }
    let point_id = Uuid::new_v4();
    insert(&pool, &snapshot(point_id), 300).await;
    let router = app(pool.clone());
    let (status, first) = get(&router, "/api/study/reports?group_by=session&limit=1").await;
    assert_eq!(status, 200, "{first}");
    let games = first["items"].as_array().unwrap();
    assert_eq!(
        games.len(),
        3,
        "A session must not be split across pages: {first}"
    );
    for (index, report) in games.iter().enumerate() {
        assert_eq!(report["session_id"], session_a.to_string());
        assert_eq!(report["game_no"], index + 1);
        assert_eq!(report["session_scores"], json!([10, 6]));
        assert_eq!(report["money_session_ended"], true);
        assert!(report.get("review").is_none());
    }
    let cursor = first["next_cursor"].as_str().unwrap();
    // Cursors for report and session pagination must not be mixed.
    assert_eq!(
        get(&router, &format!("/api/study/reports?after={cursor}"))
            .await
            .0,
        422
    );
    let (status, second) = get(
        &router,
        &format!("/api/study/reports?group_by=session&limit=1&after={cursor}"),
    )
    .await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["items"][0]["id"], point_id.to_string());
    let (_, third) = get(
        &router,
        &format!(
            "/api/study/reports?group_by=session&limit=1&after={}",
            second["next_cursor"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(third["items"][0]["session_id"], session_b.to_string());
    assert_eq!(third["items"][0]["session_scores"], json!([0, 2]));
    assert_eq!(third["items"][0]["money_session_ended"], false);
    assert!(third["next_cursor"].is_null());
    let (_, flat) = get(&router, "/api/study/reports?limit=1").await;
    assert_eq!(flat["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        get(
            &router,
            &format!(
                "/api/study/reports?group_by=session&after={}",
                flat["next_cursor"].as_str().unwrap()
            )
        )
        .await
        .0,
        422
    );
    assert_eq!(
        get(&router, "/api/study/reports?group_by=unknown").await.0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and a dummy OPENAI_API_KEY; no provider call"]
async fn assessment_start_is_versioned_deduplicated_and_language_scoped(pool: PgPool) {
    let router = app(pool.clone());
    let id = Uuid::new_v4();
    let mut state = snapshot(id);
    state["review"]["games"] = json!([{
        "engine": "test",
        "game_no": 1,
        "decisions": [{
            "decision_no": 1, "checker_no": 1, "player": 0, "kind": "checker",
            "played": "8/5 6/5", "best": "8/5 6/5", "played_equity": 0.1,
            "best_equity": 0.1, "equity_loss": 0.0, "severity": "none",
            "dice": [3,1], "evaluation_status": "evaluated"
        }]
    }]);
    insert(&pool, &state, 0).await;
    let uri = format!("/api/study/reports/{id}/assessment?language=de");
    let post = |version: i64| {
        let request = Request::post(&uri)
            .header("content-type", "application/json")
            .body(Body::from(json!({"report_version":version}).to_string()))
            .unwrap();
        router.clone().oneshot(request)
    };
    let (status, empty) = get(&router, &uri).await;
    assert_eq!(status, 200);
    assert_eq!(empty["availability"], "ready");
    assert!(empty["job"].is_null());
    assert_eq!(post(2).await.unwrap().status(), 409);
    let (first, second) = tokio::join!(post(3), post(3));
    for response in [first, second] {
        let response = response.unwrap();
        assert_eq!(response.status(), 200);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let response: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(response["job"]["status"], "queued");
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM match_assessments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let (_, english) = get(&router, &uri.replace("language=de", "language=en")).await;
    assert!(english["job"].is_null());
    sqlx::query("UPDATE match_assessments SET status='completed',result=$1")
        .bind(
            json!({"summary":"Saved","next_match_focus":"Focus","topics":[],
                     "key_decisions":[],"training":[],"limitations":[]}),
        )
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(post(3).await.unwrap().status(), 200);
    let (_, saved) = get(&router, &uri).await;
    assert_eq!(saved["job"]["status"], "completed");
    assert_eq!(saved["job"]["result"]["summary"], "Saved");
}
