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
async fn transfer_balances_unseen_cube_responses_and_records_first_pass(pool: PgPool) {
    for (index, best) in ["take", "drop"].into_iter().enumerate() {
        let sgf = format!(
            "(;GM[6]PW[Alice]PB[Bob]MI[length:7];W[31hefe])(;GM[6]PW[Alice]PB[Bob]MI[length:7][ws:{index}][bs:3]AE[a:y]AW[e]AB[s]PL[B]CV[2]CO[b];B[double];W[{best}])"
        );
        let imported = store::import_match(
            &pool,
            "transfer.sgf",
            "sgf",
            sgf.as_bytes(),
            &sgf,
            &import::parse_sgf(&sgf).unwrap(),
        )
        .await
        .unwrap();
        job(
            &pool,
            imported.r#match.id,
            0,
            None,
            &[
                (2, decision(1, 1, "double", "double", 0.0)),
                (2, decision(2, 0, best, best, 0.0)),
            ],
        )
        .await;
    }
    let router = app(&pool);
    let path = "/api/study/transfer-rounds";
    let scope = json!({"player":"Alice", "game_plan":"pure_race", "kind":"cube_response"});
    let (status, mut round) = call(&router, "POST", path, scope.clone()).await;
    assert_eq!(status, 201, "{round}");
    assert_eq!(round["total"], 2);
    let id = round["id"].as_str().unwrap().to_owned();
    let mut answers = Vec::new();
    for _ in 0..2 {
        assert!(round["result"].is_null());
        let (_, answered) = call(&router, "POST", &format!("/api/study/card-rounds/{id}/answer"), json!({"version":round["version"], "card_version":0, "request_id":Uuid::new_v4(), "answer":null})).await;
        answers.push(
            answered["result"]["answer"]["solution"]["decision"]["best"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
        let (status, next) = call(
            &router,
            "POST",
            &format!("/api/study/card-rounds/{id}/advance"),
            json!({"version":answered["version"],"marked":false}),
        )
        .await;
        assert_eq!(status, 200, "{next}");
        round = next;
    }
    answers.sort();
    assert_eq!(answers, ["drop", "take"]);
    assert_eq!(call(&router, "POST", path, scope).await.0, 422);
    let (_, history) = call(&router, "GET", path, Value::Null).await;
    assert_eq!(history[0]["revealed"], 2);
    assert_eq!(history[0]["scored"], 0);
    assert!(history[0]["mean_loss"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn database_player_choices_include_matches_without_analysis(pool: PgPool) {
    source(&pool).await;
    let database = Uuid::new_v4();
    sqlx::query("INSERT INTO databases(id,name) VALUES($1,'Online')")
        .bind(database)
        .execute(&pool)
        .await
        .unwrap();
    let sgf = SOURCE.replace("Alice", "Hendrik Reh");
    store::import_match_in_database(
        &pool,
        "online.sgf",
        "sgf",
        sgf.as_bytes(),
        &sgf,
        &import::parse_sgf(&sgf).unwrap(),
        database,
    )
    .await
    .unwrap();

    let router = app(&pool);
    let url = format!("/api/study/mistakes?database_id={database}");
    let (status, choices) = call(&router, "GET", &url, Value::Null).await;
    assert_eq!(status, 200, "{choices}");
    assert_eq!(choices["players"], json!(["Bob", "Hendrik Reh"]));
    let (status, result) = call(
        &router,
        "GET",
        &format!("{url}&player=hEndrik%20reh"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["players"], choices["players"]);
    assert_eq!(result["analyzed_matches"], 0);
    assert_eq!(result["analyzed_decisions"], 0);
    assert_eq!(result["groups"], json!([]));
}

#[sqlx::test(migrations = "./migrations")]
async fn mistake_rates_count_correct_and_subthreshold_decisions_with_the_same_best_action(
    pool: PgPool,
) {
    let mut sgf = "(;GM[6]PW[Alice]PB[Bob]MI[length:7];W[31hefe])".to_owned();
    let mut decisions = Vec::new();
    for (index, (played, best, loss)) in [
        ("drop", "take", 0.12),
        ("take", "take", 0.0),
        ("drop", "take", 0.04),
        ("take", "drop", 0.2),
        ("drop", "drop", 0.0),
    ]
    .into_iter()
    .enumerate()
    {
        sgf.push_str(&format!("(;GM[6]PW[Alice]PB[Bob]MI[length:7][ws:2][bs:3]AE[a:y]AW[e]AB[s]PL[B]CV[2]CO[b];B[double];W[{played}])"));
        let game = index as i32 + 2;
        decisions.push((game, decision(1, 1, "double", "double", 0.0)));
        decisions.push((game, decision(2, 0, played, best, loss)));
    }
    let id = store::import_match(
        &pool,
        "opportunities.sgf",
        "sgf",
        sgf.as_bytes(),
        &sgf,
        &import::parse_sgf(&sgf).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    job(&pool, id, 0, None, &decisions).await;
    let router = app(&pool);
    for (query, errors, rate, loss_per_100) in [
        ("", 1, 1.0 / 3.0, 4.0),
        ("&min_loss=0.04", 2, 2.0 / 3.0, 16.0 / 3.0),
    ] {
        let (status, data) = call(
            &router,
            "GET",
            &format!("/api/study/mistakes?player=Alice{query}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{data}");
        assert_eq!(data["analyzed_decisions"], 5);
        let groups = data["groups"].as_array().unwrap();
        let wrong_pass = groups
            .iter()
            .find(|g| g["mistake_kind"] == "wrong_pass")
            .unwrap();
        let wrong_take = groups
            .iter()
            .find(|g| g["mistake_kind"] == "wrong_take")
            .unwrap();
        assert_eq!(wrong_pass["count"], errors);
        assert_eq!(wrong_pass["rates"]["decision_count"], 3);
        assert_eq!(wrong_pass["rates"]["small_sample"], true);
        assert!((wrong_pass["rates"]["error_rate"].as_f64().unwrap() - rate).abs() < 1e-10);
        assert!(
            (wrong_pass["rates"]["loss_per_100"].as_f64().unwrap() - loss_per_100).abs() < 1e-10
        );
        assert_eq!(wrong_take["rates"]["decision_count"], 2);
        assert_eq!(wrong_take["rates"]["error_rate"], 0.5);
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn active_practice_scores_all_three_decisions_without_writing_learning_state(pool: PgPool) {
    let id = source(&pool).await;
    let saved = job(&pool, id, 0, Some(1), &game1(0.125)).await;
    let router = app(&pool);
    let endpoint = "/api/study/mistakes/attempt";
    for (number, answer, best, loss) in [
        (
            1,
            json!({"kind":"checker","steps":[{"from":8,"to":5},{"from":6,"to":5}]}),
            "6/3 6/5",
            0.125,
        ),
        (
            2,
            json!({"kind":"cube_offer","action":"double"}),
            "no double",
            0.125,
        ),
        (
            3,
            json!({"kind":"cube_response","action":"drop"}),
            "drop",
            0.0,
        ),
    ] {
        let (status, result) = call(
            &router,
            "POST",
            endpoint,
            json!({
                "reference":{"job_id":saved,"game_no":1,"decision_no":number}, "answer":answer
            }),
        )
        .await;
        assert_eq!(status, 200, "{number}: {result}");
        assert_eq!(result["evaluation"]["best"], best);
        assert_eq!(result["evaluation"]["equity_loss"], loss);
        assert_eq!(result["evaluation"]["provenance"]["kind"], "stored");
        assert_eq!(result["original"]["equity_loss"], 0.125);
    }
    let wrong = json!({"reference":{"job_id":saved,"game_no":1,"decision_no":1},"answer":{"kind":"cube_offer","action":"double"}});
    assert_eq!(call(&router, "POST", endpoint, wrong).await.0, 422);
    let newer = job(&pool, id, 2, Some(1), &game1(0.25)).await;
    assert_ne!(saved, newer);
    let stale = json!({"reference":{"job_id":saved,"game_no":1,"decision_no":2},"answer":{"kind":"cube_offer","action":"double"}});
    assert_eq!(call(&router, "POST", endpoint, stale).await.0, 404);
    let writes: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM learning_cards) + (SELECT count(*) FROM review_sessions) + (SELECT count(*) FROM card_reviews)").fetch_one(&pool).await.unwrap();
    assert_eq!(writes, 0);
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn selects_persisted_game_before_filters_without_mixing_other_game_sources(pool: PgPool) {
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
    sqlx::query("INSERT INTO analysis_selections(match_id,analysis_id) VALUES($1,$2)")
        .bind(id)
        .bind(format!("job:{deep}"))
        .execute(&pool)
        .await
        .unwrap();
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
    assert_eq!(a["analyzed_decisions"], 2);
    assert_eq!(a["analyzed_matches"], 1);
    let groups = a["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2, "{a}");
    assert!(groups.iter().any(|g| g["mistake_kind"] == "wrong_take"));
    assert!(!groups.iter().any(|g| g["mistake_kind"] == "wrong_pass"));
    assert!(groups.iter().any(|g| g["mistake_kind"] == "checker"));
    let (_, b) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Bob&min_loss=0.15",
        Value::Null,
    )
    .await;
    assert_eq!(
        b["groups"],
        json!([]),
        "unselected game-2 decisions must not fill selected game-1 scope"
    );
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
async fn missed_double_practice_mixes_correct_rolls_and_keeps_answers_hidden(pool: PgPool) {
    let sgf = concat!(
        "(;GM[6]PW[Alice]PB[Bob]MI[length:7];W[31hefe])",
        "(;GM[6]MI[length:7]AE[a:y]AW[f]AB[s]PL[W];W[31fccb])",
        "(;GM[6]MI[length:7][ws:1]AE[a:y]AW[f]AB[s]PL[W];W[31fccb])",
        "(;GM[6]MI[length:7][ws:1]AE[a:y]AW[f]AB[s]PL[W];W[31fccb])",
        "(;GM[6]MI[length:7][ws:2]AE[a:y]AW[f]AB[s]PL[W]CV[2]CO[b];W[31fccb])",
        "(;GM[6]MI[length:7][ws:3]AE[a:y]AW[f]AB[s]PL[W];W[31fccb])",
        "(;GM[6]MI[length:7][ws:4]AE[a:y]AW[f]AB[s]PL[W];W[31fccb])",
        "(;GM[6]MI[length:7][ws:5]AE[a:y]AW[f]AB[s]PL[W]CV[2]CO[w];W[31fccb])",
    );
    let imported = store::import_match(
        &pool,
        "mixed-cube.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap()
    .r#match;
    let saved = job(
        &pool,
        imported.id,
        0,
        None,
        &[
            (2, decision(1, 0, "no double", "double", 0.2)),
            (3, decision(1, 0, "no double", "no double", 0.0)),
            (4, decision(1, 0, "no double", "no double", 0.0)),
            (5, decision(1, 0, "no double", "no double", 0.0)),
            (6, decision(1, 0, "no double", "double", 0.02)),
            (7, decision(1, 0, "no double", "double", 0.15)),
            (8, decision(1, 0, "no double", "no double", 0.0)),
        ],
    )
    .await;
    let router = app(&pool);
    let (_, stats) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    let group = &stats["groups"][0];
    assert_eq!(group["mistake_kind"], "missed_double");
    assert_eq!(group["count"], 2);
    let mut request = json!({
        "player": "Alice", "game_plan": group["game_plan"], "mistake_kind": "missed_double"
    });
    let (status, round) = call(
        &router,
        "POST",
        "/api/study/mistakes/practice",
        request.clone(),
    )
    .await;
    assert_eq!(status, 200, "{round}");
    let items = round["items"].as_array().unwrap();
    assert_eq!(items.len(), 3, "{round}");
    let mut games = Vec::new();
    for item in items {
        assert_eq!(item["kind"], "cube_offer");
        assert_eq!(item["position"]["dice"], json!([0, 0]));
        assert_eq!(item["position"]["notation"], "");
        for key in ["played", "best", "equity_loss"] {
            assert!(item.get(key).is_none(), "{item}");
        }
        let game = item["reference"]["game_no"].as_i64().unwrap();
        games.push(game);
        let (best, loss) = match game {
            2 => ("double", 0.2),
            3 => ("no double", 0.0),
            7 => ("double", 0.15),
            _ => panic!("unexpected question: {item}"),
        };
        let (status, result) = call(
            &router,
            "POST",
            "/api/study/mistakes/attempt",
            json!({
                "reference": item["reference"],
                "answer": {"kind": "cube_offer", "action": "no_double"}
            }),
        )
        .await;
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["evaluation"]["best"], best);
        assert_eq!(result["evaluation"]["equity_loss"], loss);
        assert_eq!(result["evaluation"]["provenance"]["kind"], "stored");
        let (status, reveal) = call(
            &router,
            "POST",
            "/api/study/mistakes/reveal",
            item["reference"].clone(),
        )
        .await;
        assert_eq!(status, 200, "{reveal}");
        assert_eq!(reveal, result["original"]);
    }
    games.sort_unstable();
    assert_eq!(games, [2, 3, 7]);
    for game in [5, 8] {
        let unavailable = json!({"job_id": saved, "game_no": game, "decision_no": 1});
        assert_eq!(
            call(&router, "POST", "/api/study/mistakes/reveal", unavailable)
                .await
                .0,
            404
        );
    }
    request["min_loss"] = json!(0.25);
    let (_, empty) = call(&router, "POST", "/api/study/mistakes/practice", request).await;
    assert_eq!(empty["items"], json!([]));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn take_and_pass_rounds_mix_both_best_actions_and_score_the_responder(pool: PgPool) {
    let mut sgf = "(;GM[6]PW[Alice]PB[Bob]MI[length:9];W[31hefe])".to_owned();
    let mut decisions = Vec::new();
    for (score, played, best, loss) in [
        (0, "take", "drop", 0.2),
        (1, "drop", "take", 0.15),
        (2, "take", "take", 0.0),
        (3, "drop", "drop", 0.0),
        (4, "drop", "take", 0.02),
    ] {
        sgf.push_str(&format!(
            "(;GM[6]MI[length:9][ws:{score}]AE[a:y]AW[e]AB[s]PL[B]CV[2]CO[b];B[double];W[{played}])"
        ));
        decisions.push((score + 2, decision(1, 1, "double", "double", 0.0)));
        let mut response = decision(2, 0, played, best, loss);
        response["best_equity"] = json!(if best == "drop" { -1.0 } else { -0.75 });
        response["played_equity"] = json!(response["best_equity"].as_f64().unwrap() - loss);
        decisions.push((score + 2, response));
    }
    let imported = store::import_match(
        &pool,
        "mixed-cube-responses.sgf",
        "sgf",
        sgf.as_bytes(),
        &sgf,
        &import::parse_sgf(&sgf).unwrap(),
    )
    .await
    .unwrap()
    .r#match;
    job(&pool, imported.id, 0, None, &decisions).await;
    let router = app(&pool);
    let (_, stats) = call(
        &router,
        "GET",
        "/api/study/mistakes?player=Alice",
        Value::Null,
    )
    .await;
    for (kind, mistake_game, contrast_game, correct, contrast, loss) in [
        ("wrong_take", 2, 4, "drop", "take", 0.2),
        ("wrong_pass", 3, 5, "take", "drop", 0.15),
    ] {
        let group = stats["groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|group| group["mistake_kind"] == kind)
            .unwrap();
        assert_eq!(group["count"], 1);
        let (status, round) = call(
            &router,
            "POST",
            "/api/study/mistakes/practice",
            json!({"player":"Alice", "game_plan":group["game_plan"], "mistake_kind":kind}),
        )
        .await;
        assert_eq!(status, 200, "{round}");
        let items = round["items"].as_array().unwrap();
        let mut games = Vec::new();
        for item in items {
            assert_eq!(item["kind"], "cube_response");
            assert_eq!(item["position"]["player"], 0);
            assert_eq!(item["position"]["dice"], json!([0, 0]));
            assert_eq!(item["position"]["notation"], "");
            for key in ["best", "played", "equity_loss"] {
                assert!(item.get(key).is_none(), "{item}");
            }
            games.push(item["reference"]["game_no"].as_i64().unwrap());
        }
        games.sort_unstable();
        assert_eq!(games, [mistake_game, contrast_game], "{round}");

        for (game, answer, best, cost) in [
            (mistake_game, correct, correct, 0.0),
            (mistake_game, contrast, correct, loss),
            (contrast_game, contrast, contrast, 0.0),
        ] {
            let item = items
                .iter()
                .find(|item| item["reference"]["game_no"] == game)
                .unwrap();
            let (status, checked) = call(
                &router,
                "POST",
                "/api/study/mistakes/attempt",
                json!({
                    "reference": item["reference"],
                    "answer": {"kind": "cube_response", "action": answer}
                }),
            )
            .await;
            assert_eq!(status, 200, "{checked}");
            let result = &checked["evaluation"];
            assert_eq!(result["best"], best);
            assert_eq!(result["chosen"], answer);
            assert_eq!(result["equity_loss"], cost);
            assert_eq!(
                result["best_equity"],
                if best == "drop" { -1.0 } else { -0.75 }
            );
            assert_eq!(result["provenance"]["kind"], "stored");
            let (status, revealed) = call(
                &router,
                "POST",
                "/api/study/mistakes/reveal",
                item["reference"].clone(),
            )
            .await;
            assert_eq!(status, 200, "{revealed}");
            assert_eq!(revealed, checked["original"]);
        }
    }
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
    let second = job(&pool, id, 2, None, &game1(0.2)).await;
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

#[sqlx::test(migrations = "./migrations")]
async fn card_suggestions_span_games_and_save_only_selected_decisions(pool: PgPool) {
    let match_id = source(&pool).await;
    let history_sgf = SOURCE.replace("Bob", "Carol");
    let history = store::import_match(
        &pool,
        "history.sgf",
        "sgf",
        history_sgf.as_bytes(),
        &history_sgf,
        &import::parse_sgf(&history_sgf).unwrap(),
    )
    .await
    .unwrap()
    .r#match
    .id;
    job(&pool, history, 1, Some(1), &game1(0.08)).await;
    let mut selected_decisions = game1(0.12);
    selected_decisions.extend([
        (2, decision(1, 1, "double", "no double", 0.25)),
        (2, decision(2, 0, "drop", "take", 0.35)),
    ]);
    let first = job(&pool, match_id, 1, None, &selected_decisions).await;
    let router = app(&pool);
    let (status, deck) = call(&router, "POST", "/api/decks", json!({"name":"Suggestions"})).await;
    assert_eq!(status, 201, "{deck}");
    let url = format!(
        "/api/decks/{}/card-suggestions",
        deck["id"].as_str().unwrap()
    );
    let preview = format!("{url}?match_id={match_id}&player=0&min_loss=0.10");
    let (status, result) = call(&router, "GET", &preview, Value::Null).await;
    assert_eq!(status, 200, "{result}");
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert!(items.iter().all(|item| item["position"]["player"] == 0));
    let checker = items
        .iter()
        .find(|item| item["reference"]["decision_no"] == 1)
        .unwrap();
    assert_eq!(checker["reference"]["job_id"], json!(first));
    // Historical errors below the chosen threshold do not contribute.
    assert_eq!(checker["recurring_matches"], 0);
    let (_, recurring) = call(
        &router,
        "GET",
        &format!("{url}?match_id={match_id}&player=0&min_loss=0.08"),
        Value::Null,
    )
    .await;
    assert!(
        recurring["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["recurring_matches"] == 2)
    );
    let picked = items
        .iter()
        .find(|item| item["reference"]["game_no"] == 2)
        .unwrap();
    let body = json!({"match_id": match_id, "decisions": [picked["reference"]]});
    let (status, result) = call(&router, "POST", &url, body.clone()).await;
    assert_eq!(
        (status, result),
        (200, json!({"added":1,"updated":0,"total":1}))
    );
    sqlx::query("UPDATE learning_cards SET review_count=4")
        .execute(&pool)
        .await
        .unwrap();
    let (_, updated) = call(&router, "GET", &preview, Value::Null).await;
    let items = updated["items"].as_array().unwrap();
    assert_eq!(
        items.iter().filter(|item| item["in_deck"] == true).count(),
        1
    );
    assert!(
        items
            .iter()
            .any(|item| item["in_deck"] == true && item["card_id"].is_string())
    );
    assert_eq!(call(&router, "POST", &url, body.clone()).await.0, 200);
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT review_count FROM learning_cards")
            .fetch_one(&pool)
            .await
            .unwrap(),
        4
    );
    let replacement = job(
        &pool,
        match_id,
        2,
        Some(2),
        &[
            (2, decision(1, 1, "double", "no double", 0.25)),
            (2, decision(2, 0, "drop", "take", 0.35)),
        ],
    )
    .await;
    sqlx::query("INSERT INTO analysis_selections(match_id,analysis_id) VALUES($1,$2)")
        .bind(match_id)
        .bind(format!("job:{replacement}"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(call(&router, "POST", &url, body).await.0, 409);
}

#[sqlx::test(migrations = "./migrations")]
async fn decision_search_filters_by_type_score_cube_and_loss_with_stable_pages(pool: PgPool) {
    let id = source(&pool).await;
    // Game 1 of the 7-point match starts at 0-0 with a centered cube.
    let saved = job(
        &pool,
        id,
        2,
        Some(1),
        &[
            (1, decision(1, 0, "8/5 6/5", "6/3 6/5", 0.03)),
            (1, decision(2, 1, "double", "no double", 0.2)),
            (1, decision(3, 0, "take", "drop", 0.125)),
            (1, decision(4, 1, "24/20 24/22", "24/20 24/22", 0.0)),
        ],
    )
    .await;
    let router = app(&pool);
    let search = |query: &str| {
        let router = router.clone();
        let uri = format!("/api/study/decisions?{query}");
        async move { call(&router, "GET", &uri, Value::Null).await }
    };

    let (status, all) = search("player=alice").await;
    assert_eq!(status, 200, "{all}");
    assert_eq!(all["total"], 2);
    // Largest loss first.
    assert_eq!(all["items"][0]["kind"], "cube_response");
    assert_eq!(all["items"][0]["played"], "take");
    assert_eq!(all["items"][0]["best"], "drop");
    assert_eq!(all["items"][0]["equity_loss"], 0.125);
    assert_eq!(all["items"][0]["mistake_kind"], "wrong_take");
    assert_eq!(all["items"][0]["match_length"], 7);
    assert_eq!(all["items"][0]["away"], json!([7, 7]));
    assert_eq!(all["items"][0]["cube_value"], 1);
    assert_eq!(all["items"][0]["cube_owner"], Value::Null);
    assert_eq!(all["items"][0]["practicable"], true);
    assert_eq!(
        all["items"][0]["reference"],
        json!({"job_id":saved,"game_no":1,"decision_no":3})
    );
    assert_eq!(all["items"][1]["kind"], "checker");

    let (_, takes) = search("player=Alice&kind=cube_response&min_loss=0.05&away=7").await;
    assert_eq!(takes["total"], 1);
    let (_, none) = search("player=Alice&kind=cube_response&away=2").await;
    assert_eq!(none["total"], 0);
    let (_, small) = search("player=Alice&max_loss=0.05").await;
    assert_eq!(small["total"], 1);
    assert_eq!(small["items"][0]["kind"], "checker");
    let (_, owned) = search("player=Alice&cube_owner=self").await;
    assert_eq!(owned["total"], 0);
    let (_, centered) = search("player=Alice&cube_owner=centered&cube_value=1").await;
    assert_eq!(centered["total"], 2);
    let (_, kind) = search("player=Bob&mistake_kind=premature_double").await;
    assert_eq!(kind["total"], 1);
    // Bob's correct checker play has no loss and is not an error type.
    let (_, bob) = search("player=Bob").await;
    assert_eq!(bob["total"], 2);
    assert_eq!(bob["items"][1]["mistake_kind"], Value::Null);
    assert_eq!(bob["items"][1]["practicable"], false);

    let (_, first) = search("player=Alice&limit=1").await;
    let (_, second) = search("player=Alice&limit=1&page=2").await;
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(first["items"][0]["reference"]["decision_no"], 3);
    assert_eq!(second["items"][0]["reference"]["decision_no"], 1);
    assert_eq!(second["total"], 2);

    // Ascending loss reverses the order and pages without gaps or repeats.
    let (status, ascending) = search("player=Alice&direction=asc").await;
    assert_eq!(status, 200, "{ascending}");
    assert_eq!(ascending["total"], 2);
    assert_eq!(ascending["items"][0]["reference"]["decision_no"], 1);
    assert_eq!(ascending["items"][1]["reference"]["decision_no"], 3);
    let (_, last) = search("player=Alice&direction=asc&limit=1&page=2").await;
    assert_eq!(last["items"][0]["reference"]["decision_no"], 3);
    let (_, descending) = search("player=Alice&direction=desc").await;
    assert_eq!(descending["items"], all["items"]);

    for invalid in [
        "player=Alice&direction=up",
        "player=",
        "player=Alice&limit=101",
        "player=Alice&page=0",
        "player=Alice&away=0",
        "player=Alice&cube_value=3",
        "player=Alice&min_loss=0.2&max_loss=0.1",
        "kind=checker",
    ] {
        assert_eq!(search(invalid).await.0, 422, "{invalid}");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn aliases_of_one_profile_count_as_one_player(pool: PgPool) {
    let mut jobs = Vec::new();
    for (file, white, loss) in [("bot.sgf", "Alice", 0.12), ("online.sgf", "Alicia", 0.2)] {
        let sgf = format!("(;GM[6]PW[{white}]PB[Bob]MI[length:7];W[31hefe])");
        let id = store::import_match(
            &pool,
            file,
            "sgf",
            sgf.as_bytes(),
            &sgf,
            &import::parse_sgf(&sgf).unwrap(),
        )
        .await
        .unwrap()
        .r#match
        .id;
        jobs.push(job(&pool, id, 0, None, &game1(loss)[..1]).await);
    }
    let router = app(&pool);
    let mistakes = |player: &'static str| {
        let router = router.clone();
        async move {
            let (status, data) = call(
                &router,
                "GET",
                &format!("/api/study/mistakes?player={player}"),
                Value::Null,
            )
            .await;
            assert_eq!(status, 200, "{data}");
            data
        }
    };
    // Two unrelated names are two players.
    let separate = mistakes("Alice").await;
    assert_eq!(separate["players"], json!(["Alice", "Alicia", "Bob"]));
    assert_eq!(separate["analyzed_matches"], 1);
    assert_eq!(separate["player_matches"], 1);

    // Merging Alicia into Alice's profile, as the player database does.
    let merged = sqlx::query(
        "UPDATE player_aliases SET profile_id=(
             SELECT profile_id FROM player_aliases WHERE name='Alice')
         WHERE name='Alicia'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(merged.rows_affected(), 1);

    // A profile name that is only an alias resolves to the offered choice.
    let (status, resolved) = call(
        &router,
        "GET",
        "/api/study/mistakes?resolve=alicia",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{resolved}");
    assert_eq!(resolved["resolved_player"], "Alice");
    assert!(resolved["groups"].as_array().unwrap().is_empty());
    for player in ["Alice", "Alicia"] {
        let data = mistakes(player).await;
        assert_eq!(data["players"], json!(["Alice", "Bob"]), "{player}");
        assert_eq!(data["analyzed_matches"], 2, "{player}");
        assert_eq!(data["analyzed_decisions"], 2, "{player}");
        assert_eq!(data["groups"][0]["count"], 2, "{player}");
        assert_eq!(data["player_matches"], 2, "{player}");
        assert_eq!(data["unanalyzed_matches"], 0, "{player}");
    }
    let (status, found) = call(
        &router,
        "GET",
        "/api/study/decisions?player=Alice",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{found}");
    assert_eq!(found["total"], 2);
    let (status, practice) = call(
        &router,
        "POST",
        "/api/study/mistakes/practice",
        json!({"player":"Alice","game_plan":separate["groups"][0]["game_plan"],"mistake_kind":"checker"}),
    )
    .await;
    assert_eq!(status, 200, "{practice}");
    // Both matches hold the same opening decision, which practice asks once.
    let items = practice["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let asked = items[0]["reference"]["job_id"].as_str().unwrap();
    assert!(jobs.iter().any(|job| job.to_string() == asked));
}

#[sqlx::test(migrations = "./migrations")]
async fn independent_only_leaves_out_matches_played_with_learning_aids(pool: PgPool) {
    let mut jobs = Vec::new();
    for (file, loss) in [("coach.sgf", 0.2), ("alone.sgf", 0.12)] {
        let sgf = format!("(;GM[6]PW[Alice]PB[Bob]MI[length:7]C[{file}];W[31hefe])");
        let id = store::import_match(
            &pool,
            file,
            "sgf",
            sgf.as_bytes(),
            &sgf,
            &import::parse_sgf(&sgf).unwrap(),
        )
        .await
        .unwrap()
        .r#match
        .id;
        jobs.push((id, job(&pool, id, 0, None, &game1(loss)[..1]).await));
    }
    // The first match was played against the bot with Coach feedback.
    sqlx::query("UPDATE matches SET play_mode='coach',play_assisted=true WHERE id=$1")
        .bind(jobs[0].0)
        .execute(&pool)
        .await
        .unwrap();
    let router = app(&pool);
    for (query, matches, errors) in [("", 2, 2), ("&independent_only=true", 1, 1)] {
        let (status, data) = call(
            &router,
            "GET",
            &format!("/api/study/mistakes?player=Alice{query}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{data}");
        assert_eq!(data["analyzed_matches"], matches, "{query}");
        assert_eq!(data["player_matches"], matches, "{query}");
        assert_eq!(data["groups"][0]["count"], errors, "{query}");
    }
    let (status, practice) = call(
        &router,
        "POST",
        "/api/study/mistakes/practice",
        json!({"player":"Alice","game_plan":"opening","mistake_kind":"checker","independent_only":true}),
    )
    .await;
    assert_eq!(status, 200, "{practice}");
    let items = practice["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0]["reference"]["job_id"].as_str().unwrap(),
        jobs[1].1.to_string()
    );
}
