use axum::{body::Body, http::Request};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
    import, store,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SOURCE: &str = "(;GM[6]PW[Alice]PB[Bob]MI[length:7];W[31hefe];B[double];W[take];B[42qusu])(;GM[6]MI[length:7][ws:2][bs:3]AE[a:y]AW[e]AB[s]PL[B]CV[2]CO[b];B[double];W[drop])";
fn app(pool: &PgPool) -> axum::Router {
    api::router(AppState::new(
        pool.clone(),
        Engine::new("/unavailable".into()),
    ))
}
async fn call(app: &axum::Router, method: &str, uri: &str, body: Value) -> (u16, Value) {
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
async fn source(pool: &PgPool) -> Uuid {
    store::import_match(
        pool,
        "recurring.sgf",
        "sgf",
        SOURCE.as_bytes(),
        SOURCE,
        &import::parse_sgf(SOURCE).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id
}
fn decision(n: i32, player: i32, played: &str, best: &str, loss: f64) -> Value {
    let checker = played.contains('/');
    json!({"decision_no":n,"checker_no":if checker{Some(if player==0{1}else{2})}else{None},"player":player,"kind":if checker{"checker"}else{"cube"},"played":played,"best":best,"played_equity":0.0,"best_equity":loss,"equity_loss":loss,"severity":"bad","dice":if checker{Some([3,1])}else{None},"evaluation_status":"evaluated"})
}
async fn job(
    pool: &PgPool,
    match_id: Uuid,
    plies: i32,
    scope: Option<i32>,
    decisions: &[(i32, Value)],
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,result,game_no,total_games,completed_games) VALUES($1,$2,'match',$3,'completed',$4,$5,1,1)").bind(id).bind(match_id).bind(plies).bind(json!({"engine":"saved GNUbg"})).bind(scope).execute(pool).await.unwrap();
    for (game, d) in decisions {
        sqlx::query(
            "INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,$2) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(game)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO move_analyses(job_id,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(id).bind(game).bind(d["decision_no"].as_i64().unwrap() as i32).bind(d["player"].as_i64().unwrap() as i32).bind(d["kind"].as_str().unwrap()).bind(d["equity_loss"].as_f64().unwrap()).bind(d).execute(pool).await.unwrap();
    }
    id
}
fn game1(loss: f64) -> Vec<(i32, Value)> {
    vec![
        (1, decision(1, 0, "8/5 6/5", "6/3 6/5", loss)),
        (1, decision(2, 1, "double", "no double", loss)),
        (1, decision(3, 0, "take", "drop", loss)),
        (1, decision(4, 1, "24/20 24/22", "13/9 13/11", loss)),
    ]
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn selects_deepest_per_game_before_filters_and_uses_actual_decision_player(pool: PgPool) {
    let id = source(&pool).await;
    let deep = job(&pool, id, 3, Some(1), &game1(0.1)).await;
    job(&pool, id, 0, None, &game1(0.9)).await;
    job(
        &pool,
        id,
        2,
        Some(2),
        &[
            (2, decision(1, 1, "double", "no double", 0.2)),
            (2, decision(2, 0, "drop", "take", 0.3)),
        ],
    )
    .await;
    let router = app(&pool);
    let (status, all) = call(&router, "GET", "/api/study/mistakes", Value::Null).await;
    assert_eq!(status, 200, "{all}");
    assert_eq!(all["players"], json!(["Alice", "Bob"]));
    assert_eq!(all["groups"], json!([]));
    let (status, a) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=aLiCe",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{a}");
    assert_eq!(a["analyzed_decisions"], 3);
    assert_eq!(a["analyzed_matches"], 1);
    let groups = a["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 3, "{a}");
    assert!(groups.iter().any(|g| g["mistake_kind"] == "wrong_take"));
    assert!(groups.iter().any(|g| g["mistake_kind"] == "wrong_pass"));
    assert!(groups.iter().any(|g| g["mistake_kind"] == "checker"));
    let (_, b) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Bob&min_loss=0.15",
        Value::Null,
    )
    .await;
    assert_eq!(b["groups"].as_array().unwrap().len(), 1, "{b}");
    assert_eq!(b["groups"][0]["total_loss"], 0.2);
    assert_eq!(b["groups"][0]["mistake_kind"], "premature_double");
    let group = groups
        .iter()
        .find(|g| g["mistake_kind"] == "wrong_take")
        .unwrap();
    let (status, practice) = call(
        &router,
        "POST",
        "/api/study/mistakes/practice",
        json!({"player":"Alice","game_plan":group["game_plan"],"mistake_kind":"wrong_take"}),
    )
    .await;
    assert_eq!(status, 200, "{practice}");
    let item = &practice["items"][0];
    assert_eq!(item["reference"]["job_id"], json!(deep));
    assert_eq!(item["position"]["player"], 0);
    assert_eq!(item["position"]["notation"], "");
    assert_eq!(item["kind"], "cube_response");
    for key in ["best", "played", "equity_loss"] {
        assert!(item.get(key).is_none());
    }
    let (status, reveal) = call(
        &router,
        "POST",
        "/api/study/mistakes/reveal",
        item["reference"].clone(),
    )
    .await;
    assert_eq!(status, 200, "{reveal}");
    assert_eq!(reveal["best"], "drop");
    assert_eq!(reveal["played"], "take");
    assert_eq!(reveal["equity_loss"], 0.1);
    for table in ["learning_cards", "decks", "card_reviews"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
    sqlx::query("UPDATE matches SET source_revision=source_revision+1 WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(
            &router,
            "POST",
            "/api/study/mistakes/reveal",
            item["reference"].clone()
        )
        .await
        .0,
        404
    );
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn excludes_uncheckpointed_stale_incomplete_and_competition_sources(pool: PgPool) {
    let id = source(&pool).await;
    let j = job(&pool, id, 2, None, &game1(0.2)).await;
    let router = app(&pool);
    for condition in [
        "DELETE FROM match_analysis_games WHERE job_id=$1",
        "UPDATE jobs SET status='failed' WHERE id=$1",
        "UPDATE jobs SET status='completed',source_revision=10 WHERE id=$1",
    ] {
        sqlx::query(condition).bind(j).execute(&pool).await.unwrap();
        let (status, v) = call(
            &router,
            "GET",
            "/api/study/mistakes?player=Alice",
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(v["groups"], json!([]));
    }
    sqlx::query("UPDATE jobs SET source_revision=0 WHERE id=$1")
        .bind(j)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,1)")
        .bind(j)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id,library_match_id) VALUES($1,$2,'/unavailable','','saved',$3)").bind(Uuid::new_v4()).bind(json!({"phase":"move","play_mode":"competition"})).bind(id).execute(&pool).await.unwrap();
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
    for query in [
        "min_loss=-1",
        "min_loss=NaN",
        "game_plan=unknown",
        "player=%20",
        "player=Alice&unknown=1",
    ] {
        assert_eq!(
            call(
                &router,
                "GET",
                &format!("/api/study/mistakes?{query}"),
                Value::Null
            )
            .await
            .0,
            422,
            "{query}"
        );
    }
    assert_eq!(
        call(
            &router,
            "POST",
            "/api/study/mistakes/reveal",
            json!({"job_id":j,"game_no":1,"decision_no":1})
        )
        .await
        .0,
        404
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn no_double_is_preroll_and_malformed_or_forced_evaluations_are_excluded(pool: PgPool) {
    use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
    let sgf = "(;GM[6]PW[ Alice ]PB[Bob]MI[length:7]AE[a:y]AW[f]AB[s]PL[W];W[31fccb])";
    let id = store::import_match(
        &pool,
        "preroll.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    let mut malformed = decision(2, 0, "6/3 6/2", "6/3 6/2", 0.5);
    malformed["best_equity"] = json!(0.0);
    let j = job(
        &pool,
        id,
        0,
        None,
        &[
            (1, decision(1, 0, "no double", "double", 0.2)),
            (1, malformed),
        ],
    )
    .await;
    let router = app(&pool);
    let (_, v) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(v["analyzed_decisions"], 1, "{v}");
    assert_eq!(v["groups"][0]["mistake_kind"], "missed_double");
    let (_,questions)=call(&router,"POST","/api/study/mistakes/practice",json!({"player":"Alice","game_plan":v["groups"][0]["game_plan"],"mistake_kind":"missed_double"})).await;
    let q = &questions["items"][0];
    assert_eq!(q["position"]["dice"], json!([0, 0]));
    let bytes = STANDARD_NO_PAD
        .decode(q["position"]["gnubg_match_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(bytes[1] & 0x80, 0);
    assert_eq!(
        bytes[2] & 0x1f,
        0,
        "the subsequent checker roll must not leak in Match ID"
    );
    sqlx::query("UPDATE move_analyses SET data=jsonb_set(data,'{evaluation_status}','\"forced\"') WHERE job_id=$1 AND decision_no=1").bind(j).execute(&pool).await.unwrap();
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
#[ignore = "requires PostgreSQL"]
async fn occurrence_counts_are_complete_but_practice_is_distinct_and_limited_to_twenty(
    pool: PgPool,
) {
    let mut source = String::new();
    let mut rows = Vec::new();
    for n in 0..26 {
        let score = if n == 25 { 0 } else { n };
        source.push_str(&format!(
            "(;GM[6]PW[Alice]PB[Bob]MI[length:100][ws:{score}][bs:0];W[31hefe])"
        ));
        rows.push((
            n + 1,
            decision(1, 0, "8/5 6/5", "6/3 6/5", 0.1 + f64::from(n) / 100.0),
        ));
    }
    let id = store::import_match(
        &pool,
        "many.sgf",
        "sgf",
        source.as_bytes(),
        &source,
        &import::parse_sgf(&source).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    job(&pool, id, 0, None, &rows).await;
    let router = app(&pool);
    let (_, v) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(v["groups"][0]["count"], 26);
    assert_eq!(v["groups"][0]["match_count"], 1);
    let (_, practice) = call(
        &router,
        "POST",
        "/api/study/mistakes/practice",
        json!({"player":"Alice","game_plan":v["groups"][0]["game_plan"],"mistake_kind":"checker"}),
    )
    .await;
    let items = practice["items"].as_array().unwrap();
    assert_eq!(items.len(), 20);
    let contexts: std::collections::HashSet<_> = items
        .iter()
        .map(|i| i["position"]["gnubg_match_id"].as_str().unwrap())
        .collect();
    assert_eq!(contexts.len(), 20);
    let mut previous = f64::INFINITY;
    for item in items {
        let (_, answer) = call(
            &router,
            "POST",
            "/api/study/mistakes/reveal",
            item["reference"].clone(),
        )
        .await;
        let loss = answer["equity_loss"].as_f64().unwrap();
        assert!(loss <= previous);
        previous = loss;
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn player_choices_and_selected_player_do_not_scan_unrelated_large_sources(pool: PgPool) {
    let id = source(&pool).await;
    job(&pool, id, 0, None, &game1(0.2)).await;
    let sgf = SOURCE.replace("Alice", "Carol");
    let other = store::import_match(
        &pool,
        "other.sgf",
        "sgf",
        sgf.as_bytes(),
        &sgf,
        &import::parse_sgf(&sgf).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    job(&pool, other, 0, None, &game1(0.2)).await;
    sqlx::query("UPDATE matches SET sgf=repeat('x',67108865) WHERE id=$1")
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();
    let router = app(&pool);
    let (status, choices) = call(&router, "GET", "/api/study/mistakes", Value::Null).await;
    assert_eq!(status, 200, "{choices}");
    assert_eq!(choices["players"], json!(["Alice", "Bob", "Carol"]));
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
    let (status, error) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Carol",
        Value::Null,
    )
    .await;
    assert_eq!(status, 422);
    assert!(error["error"].as_str().unwrap().contains("narrow"));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn equal_depth_ties_use_timestamp_then_uuid_and_invalid_winner_never_falls_back(
    pool: PgPool,
) {
    let id = source(&pool).await;
    let first = job(&pool, id, 2, None, &game1(0.1)).await;
    let second = job(&pool, id, 2, Some(1), &game1(0.2)).await;
    sqlx::query("UPDATE jobs SET created_at=CASE WHEN id=$1 THEN '2026-01-01'::timestamptz ELSE '2026-01-02'::timestamptz END WHERE id=ANY($2)").bind(first).bind(vec![first,second]).execute(&pool).await.unwrap();
    let router = app(&pool);
    let (_, newest) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert!(
        newest["groups"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["total_loss"] == 0.2)
    );
    sqlx::query("UPDATE jobs SET created_at='2026-01-01'::timestamptz WHERE id=ANY($1)")
        .bind(vec![first, second])
        .execute(&pool)
        .await
        .unwrap();
    let winner = first.max(second);
    let expected = if winner == first { 0.1 } else { 0.2 };
    let (_, tied) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert!(
        tied["groups"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["total_loss"] == expected)
    );
    let (_,q)=call(&router,"POST","/api/study/mistakes/practice",json!({"player":"Alice","game_plan":tied["groups"][0]["game_plan"],"mistake_kind":tied["groups"][0]["mistake_kind"]})).await;
    assert_eq!(q["items"][0]["reference"]["job_id"], winner.to_string());
    sqlx::query("UPDATE move_analyses SET data=jsonb_set(data,'{best}','\"\"') WHERE job_id=$1")
        .bind(winner)
        .execute(&pool)
        .await
        .unwrap();
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
#[ignore = "requires PostgreSQL"]
async fn database_and_exact_names_scope_native_sources_without_opponent_leaks(pool: PgPool) {
    let id = source(&pool).await;
    job(&pool, id, 0, None, &game1(0.2)).await;
    let db = Uuid::new_v4();
    sqlx::query("INSERT INTO databases(id,name) VALUES($1,'Other')")
        .bind(db)
        .execute(&pool)
        .await
        .unwrap();
    let sgf = SOURCE.replace("Alice", "Alice Cooper");
    let other = store::import_match(
        &pool,
        "names.sgf",
        "sgf",
        sgf.as_bytes(),
        &sgf,
        &import::parse_sgf(&sgf).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    job(&pool, other, 0, None, &game1(0.4)).await;
    sqlx::query("UPDATE matches SET database_id=$2 WHERE id=$1")
        .bind(other)
        .bind(db)
        .execute(&pool)
        .await
        .unwrap();
    let router = app(&pool);
    let (_, a) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(a["analyzed_decisions"], 2);
    assert!(
        a["groups"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["total_loss"] == 0.2)
    );
    let (_, empty) = call(
        &router,
        "GET",
        &format!("/api/study/mistakes?player=Alice&database_id={db}"),
        Value::Null,
    )
    .await;
    assert_eq!(empty["groups"], json!([]));
    assert_eq!(empty["players"], json!(["Alice Cooper", "Bob"]));
    assert_eq!(
        call(
            &router,
            "GET",
            &format!("/api/study/mistakes?database_id={}", Uuid::new_v4()),
            Value::Null
        )
        .await
        .0,
        422
    );
    sqlx::query(
        "INSERT INTO saved_positions(position_uuid) SELECT id FROM positions WHERE match_uuid=$1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE move_analyses SET position_uuid=(SELECT id FROM positions WHERE match_uuid=$1 ORDER BY ordinal LIMIT 1) WHERE job_id IN (SELECT id FROM jobs WHERE match_uuid=$1) AND game_no=1 AND decision_no=1")
        .bind(id).execute(&pool).await.unwrap();
    let (status, deleted) = call(
        &router,
        "POST",
        "/api/library/actions",
        json!({"action":"delete","match_ids":[id]}),
    )
    .await;
    assert_eq!(status, 200, "{deleted}");
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
#[ignore = "requires PostgreSQL"]
async fn native_source_bytes_are_counted_once_per_match_and_keep_every_game(pool: PgPool) {
    let sgf = (0..3)
        .map(|score| format!("(;GM[6]PW[Alice]PB[Bob]MI[length:7][ws:{score}];W[31hefe])"))
        .collect::<String>();
    let id = store::import_match(
        &pool,
        "large-native.sgf",
        "sgf",
        sgf.as_bytes(),
        &sgf,
        &import::parse_sgf(&sgf).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    // Legal whitespace pads one source below both the 32 MiB parser and 64 MiB
    // request limits. Charging its 22 MiB once for each game incorrectly exceeds
    // the request bound despite there being just one source to parse and fetch.
    sqlx::query("UPDATE matches SET sgf=repeat(' ',22*1024*1024)||sgf WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let decisions = (1..=3)
        .map(|game| (game, decision(1, 0, "8/5 6/5", "6/3 6/5", 0.2)))
        .collect::<Vec<_>>();
    job(&pool, id, 0, None, &decisions).await;
    let router = app(&pool);
    let (status, groups) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{groups}");
    assert_eq!(groups["analyzed_decisions"], 3, "{groups}");
    assert_eq!(groups["analyzed_matches"], 1);
    assert_eq!(groups["groups"][0]["count"], 3);
    let (status, practice)=call(&router,"POST","/api/study/mistakes/practice",json!({"player":"Alice","game_plan":groups["groups"][0]["game_plan"],"mistake_kind":"checker"})).await;
    assert_eq!(status, 200, "{practice}");
    let questions = practice["items"].as_array().unwrap();
    assert_eq!(questions.len(), 3, "{practice}");
    let games = questions
        .iter()
        .map(|q| q["reference"]["game_no"].as_i64().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(games, std::collections::BTreeSet::from([1, 2, 3]));
    let contexts = questions
        .iter()
        .map(|q| q["position"]["gnubg_match_id"].as_str().unwrap())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        contexts.len(),
        3,
        "each game's source score must survive reconstruction"
    );
}
