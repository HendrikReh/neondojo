use neondojo::{api, engine::Engine, import, model::Position, store};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
mod support;
use support::call;

const SOURCE: &str = "(;GM[6]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])";

fn app(pool: &PgPool) -> axum::Router {
    api::router(api::AppState::new(
        pool.clone(),
        Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())),
    ))
}

async fn positions(pool: &PgPool, source: &str) -> Vec<Position> {
    let imported = store::import_match(
        pool,
        "position-learning.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &import::parse_sgf(source).unwrap(),
    )
    .await
    .unwrap();
    sqlx::query("INSERT INTO saved_positions(position_uuid) SELECT id FROM positions WHERE match_uuid=$1 ON CONFLICT DO NOTHING")
        .bind(imported.r#match.id).execute(pool).await.unwrap();
    store::positions(pool, Some(imported.r#match.id), None, None, 100)
        .await
        .unwrap()
        .items
}

async fn independent_positions(pool: &PgPool) -> Vec<Position> {
    let mut first = positions(pool, SOURCE).await;
    let mut second = positions(pool, &SOURCE.replace("Alice", "Alicia")).await;
    vec![first.remove(0), second.remove(1)]
}

async fn create_deck(app: &axum::Router) -> Uuid {
    let (status, result) = call(
        app.clone(),
        "POST",
        "/api/decks",
        json!({"name":"Positions"}),
    )
    .await;
    assert_eq!(status, 201, "{result}");
    Uuid::parse_str(result["id"].as_str().unwrap()).unwrap()
}

async fn native_analysis(pool: &PgPool, position: Uuid, plies: i32, notation: &str) -> Uuid {
    let id = Uuid::new_v4();
    let result = json!({"engine":"GNU Backgammon native fixture", "plies":plies,"candidates":[
        {"rank":1,"notation":notation,"equity":0.123,"probabilities":[0.6,0.1,0.01,0.4,0.05,0.005]}
    ]});
    sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result) VALUES($1,$2,'analysis',$3,'completed',$4)")
        .bind(id).bind(position).bind(plies).bind(result).execute(pool).await.unwrap();
    id
}

async fn alias(pool: &PgPool, position: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation) SELECT $1,match_uuid,ordinal+100,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation FROM positions WHERE id=$2")
        .bind(id).bind(position).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO saved_positions(position_uuid) VALUES($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    id
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_reports_each_input_and_maps_repeated_identities_without_fake_answers(pool: PgPool) {
    let positions = positions(&pool, SOURCE).await;
    let analyzed = &positions[0];
    let unanalysed = &positions[1];
    let job = native_analysis(&pool, analyzed.id, 1, "native best play").await;
    let same_identity = alias(&pool, analyzed.id).await;
    native_analysis(&pool, same_identity, 1, "alias best play").await;
    sqlx::query("INSERT INTO analysis_selections(match_id,analysis_id) VALUES($1,$2)")
        .bind(analyzed.match_uuid)
        .bind(format!("job:{job}"))
        .execute(&pool)
        .await
        .unwrap();
    let missing = Uuid::new_v4();
    let app = app(&pool);
    let deck = create_deck(&app).await;
    let path = format!("/api/decks/{deck}/positions");
    let requested = [
        analyzed.id,
        unanalysed.id,
        missing,
        analyzed.id,
        same_identity,
    ];
    let (status, result) = call(
        app.clone(),
        "POST",
        &path,
        json!({"position_ids":requested}),
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["added"], 1);
    assert_eq!(result["updated"], 0);
    assert_eq!(result["total"], 1);
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), requested.len());
    for (item, requested) in items.iter().zip(requested) {
        assert_eq!(item["position_id"], requested.to_string());
    }
    assert_eq!(items[0]["status"], "added");
    assert_eq!(items[1]["status"], "analysis_required");
    assert!(items[1]["message"].as_str().unwrap().contains("Analyze"));
    assert_eq!(items[2]["status"], "not_found");
    assert!(items[2]["message"].is_string());
    assert_eq!(
        items[4]["status"], "analysis_required",
        "a different occurrence's job cannot replace the selected scope"
    );
    assert_eq!(items[3]["status"], "duplicate");
    assert_eq!(items[3]["card_id"], items[0]["card_id"]);
    let payload: Value = sqlx::query_scalar("SELECT payload FROM learning_cards")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(payload["source"]["position_id"], analyzed.id.to_string());
    assert_eq!(payload["source"]["job_id"], job.to_string());
    assert_eq!(
        payload["solution"]["candidates"][0]["notation"],
        "native best play"
    );
    assert_eq!(payload["position"]["position_id"], analyzed.position_id);
    assert_eq!(
        payload["position"]["gnubg_match_id"],
        analyzed.gnubg_match_id
    );
    let (status, again) = call(
        app,
        "POST",
        &path,
        json!({"position_ids":[analyzed.id,analyzed.id]}),
    )
    .await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(again["added"], 0);
    assert_eq!(again["updated"], 0);
    assert_eq!(again["total"], 1);
    assert!(
        again["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["status"] == "duplicate")
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_strictly_validates_bounded_requests_and_missing_decks(pool: PgPool) {
    let app = app(&pool);
    let deck = create_deck(&app).await;
    let path = format!("/api/decks/{deck}/positions");
    for body in [
        json!({}),
        json!({"position_ids":[]}),
        json!({"position_ids":vec![Uuid::new_v4();201]}),
        json!({"position_ids":["not-a-uuid"]}),
        json!({"position_ids":[Uuid::new_v4()],"job_id":Uuid::new_v4()}),
        json!({"position_ids":null}),
    ] {
        let (status, error) = call(app.clone(), "POST", &path, body.clone()).await;
        assert_eq!(status, 422, "request {body}: {error}");
        assert!(error["error"].is_string());
    }
    let (status, result) = call(
        app.clone(),
        "POST",
        &path,
        json!({"position_ids":vec![Uuid::new_v4();200]}),
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["items"].as_array().unwrap().len(), 200);
    assert_eq!(result["total"], 0);
    assert_eq!(
        call(
            app,
            "POST",
            &format!("/api/decks/{}/positions", Uuid::new_v4()),
            json!({"position_ids":[Uuid::new_v4()]})
        )
        .await
        .0,
        404
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_preserves_fsrs_history_and_only_versions_changed_native_payloads(pool: PgPool) {
    let position = positions(&pool, SOURCE).await.remove(0);
    native_analysis(&pool, position.id, 1, "first native answer").await;
    let app = app(&pool);
    let deck = create_deck(&app).await;
    let legacy = format!("/api/decks/{deck}/cards");
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &legacy,
            json!({"position_id":position.id})
        )
        .await
        .0,
        200
    );
    let (_, cards) = call(app.clone(), "GET", &legacy, Value::Null).await;
    let card = cards["items"][0]["id"].as_str().unwrap();
    let (_, reveal) = call(
        app.clone(),
        "POST",
        &format!("/api/cards/{card}/reveal"),
        json!({"expected_version":0}),
    )
    .await;
    let (status, review) = call(
        app.clone(),
        "POST",
        &format!("/api/cards/{card}/review"),
        json!({"session_id":reveal["session_id"],"request_id":Uuid::new_v4(),"rating":"good"}),
    )
    .await;
    assert_eq!(status, 200, "{review}");
    let before: (Value, Value) =
        sqlx::query_as("SELECT to_jsonb(c)-'payload'-'updated_at',payload FROM learning_cards c")
            .fetch_one(&pool)
            .await
            .unwrap();
    let path = format!("/api/decks/{deck}/positions");
    let (_, duplicate) = call(
        app.clone(),
        "POST",
        &path,
        json!({"position_ids":[position.id]}),
    )
    .await;
    assert_eq!(duplicate["items"][0]["status"], "duplicate");
    let better = native_analysis(&pool, position.id, 2, "deeper native answer").await;
    let (status, result) = call(
        app.clone(),
        "POST",
        &path,
        json!({"position_ids":[position.id]}),
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["added"], 0);
    assert_eq!(result["updated"], 1);
    assert_eq!(result["items"][0]["status"], "updated");
    assert_eq!(result["items"][0]["card_id"], card);
    let (mut after, payload): (Value, Value) =
        sqlx::query_as("SELECT to_jsonb(c)-'payload'-'updated_at',payload FROM learning_cards c")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        after["version"].as_i64().unwrap(),
        before.0["version"].as_i64().unwrap() + 1
    );
    after["version"] = before.0["version"].clone();
    assert_eq!(
        after, before.0,
        "all FSRS history and identity fields stay intact"
    );
    assert_eq!(payload["source"]["job_id"], better.to_string());
    assert_eq!(payload["solution"]["plies"], 2);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM card_reviews")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    let other = create_deck(&app).await;
    let (_, result) = call(
        app.clone(),
        "POST",
        &format!("/api/decks/{other}/positions"),
        json!({"position_ids":[position.id]}),
    )
    .await;
    assert_eq!(result["added"], 1);
    assert_eq!(result["items"][0]["card_id"], card);
    let (_, legacy_duplicate) =
        call(app, "POST", &legacy, json!({"position_id":position.id})).await;
    assert_eq!(
        legacy_duplicate["updated"], 1,
        "legacy card API keeps its existing counting contract"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_keeps_dice_score_and_cube_contexts_as_distinct_questions(pool: PgPool) {
    let mut selected = Vec::new();
    for source in [
        "(;GM[6]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[31hefe])",
        "(;GM[6]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[42hfhd])",
        "(;GM[6]PW[Alice]PB[Bob]MI[length:7][ws:1][bs:0];W[31hefe])",
        "(;GM[6]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0]CV[2]CO[w];W[31hefe])",
    ] {
        let position = positions(&pool, source).await.remove(0);
        native_analysis(&pool, position.id, 1, "native best").await;
        selected.push(position);
    }
    assert!(
        selected
            .iter()
            .all(|p| p.position_id == selected[0].position_id)
    );
    let app = app(&pool);
    let deck = create_deck(&app).await;
    let ids: Vec<_> = selected.iter().map(|p| p.id).collect();
    let (status, result) = call(
        app,
        "POST",
        &format!("/api/decks/{deck}/positions"),
        json!({"position_ids":ids}),
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["added"], 4);
    let payloads: Vec<Value> = sqlx::query_scalar("SELECT payload FROM learning_cards")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(payloads.len(), 4);
    for position in selected {
        let payload = payloads
            .iter()
            .find(|p| p["source"]["position_id"] == position.id.to_string())
            .unwrap();
        assert_eq!(
            payload["position"]["gnubg_match_id"],
            position.gnubg_match_id
        );
        assert_eq!(payload["position"]["dice"], json!(position.dice));
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_reuses_one_selected_match_solution_without_individual_override(pool: PgPool) {
    let positions = positions(&pool, SOURCE).await;
    let match_job = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,result,total_games,completed_games) VALUES($1,$2,'match',2,'completed',$3,1,1)")
        .bind(match_job).bind(positions[0].match_uuid).bind(json!({"engine":"Native match engine"})).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no,summary) VALUES($1,1,'{}')")
        .bind(match_job)
        .execute(&pool)
        .await
        .unwrap();
    for (index, position) in positions.iter().enumerate() {
        let decision_no = index as i32 + 1;
        let data = json!({"decision_no":decision_no,"checker_no":decision_no,"player":position.player,"kind":"checker","played":position.notation,"best":"native match best","played_equity":0.0,"best_equity":0.08,"equity_loss":0.08,"severity":"bad","dice":position.dice,"evaluation_status":"evaluated"});
        sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,$3,$4,$5,'checker',0.08,$6)")
            .bind(match_job).bind(position.id).bind(position.game_no).bind(decision_no).bind(position.player).bind(data).execute(&pool).await.unwrap();
    }
    native_analysis(&pool, positions[1].id, 1, "individual native best").await;
    let app = app(&pool);
    let deck = create_deck(&app).await;
    let (status, result) = call(
        app,
        "POST",
        &format!("/api/decks/{deck}/positions"),
        json!({"position_ids":[positions[0].id,positions[1].id]}),
    )
    .await;
    assert_eq!(status, 200, "{result}");
    let payloads: Vec<Value> = sqlx::query_scalar("SELECT payload FROM learning_cards")
        .fetch_all(&pool)
        .await
        .unwrap();
    let first = payloads
        .iter()
        .find(|p| p["source"]["position_id"] == positions[0].id.to_string())
        .unwrap();
    assert_eq!(first["source"]["kind"], "match");
    assert_eq!(first["source"]["job_id"], match_job.to_string());
    assert_eq!(first["solution"]["engine"], "Native match engine");
    assert_eq!(first["solution"]["plies"], 2);
    assert_eq!(first["solution"]["decision"]["best"], "native match best");
    let second = payloads
        .iter()
        .find(|p| p["source"]["position_id"] == positions[1].id.to_string())
        .unwrap();
    assert_eq!(second["source"]["kind"], "match");
    assert_eq!(second["source"]["job_id"], match_job.to_string());
    assert_eq!(second["solution"]["plies"], 2);
    assert_eq!(second["solution"]["decision"]["best"], "native match best");
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_rejects_unprojectable_imported_analysis_and_empty_native_candidates(pool: PgPool) {
    let source = include_str!("fixtures/analysis-game1.sgf");
    let position = positions(&pool, source).await.remove(0);
    sqlx::query("UPDATE imported_analyses SET data=jsonb_set(data,'{candidates}',(SELECT jsonb_agg(candidate || '{\"equity_kind\":\"unknown\"}'::jsonb) FROM jsonb_array_elements(data->'candidates') candidate)) WHERE position_uuid=$1 AND kind='checker'")
        .bind(position.id).execute(&pool).await.unwrap();
    assert!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM imported_analyses WHERE position_uuid=$1"
        )
        .bind(position.id)
        .fetch_one(&pool)
        .await
        .unwrap()
            > 0
    );
    let app = app(&pool);
    let deck = create_deck(&app).await;
    let path = format!("/api/decks/{deck}/positions");
    for _ in 0..2 {
        let (status, result) = call(
            app.clone(),
            "POST",
            &path,
            json!({"position_ids":[position.id]}),
        )
        .await;
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["items"][0]["status"], "analysis_required");
        assert!(result["items"][0].get("card_id").is_none());
        assert_eq!(result["total"], 0);
        sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,result) VALUES($1,$2,'analysis',2,'completed',$3)")
            .bind(Uuid::new_v4()).bind(position.id).bind(json!({"engine":"GNU Backgammon","plies":2,"candidates":[]})).execute(&pool).await.unwrap();
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM learning_cards")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_deck_limit_rolls_back_memberships_and_shared_payload_refreshes(pool: PgPool) {
    let positions = independent_positions(&pool).await;
    for position in &positions {
        native_analysis(&pool, position.id, 1, "original native answer").await;
    }
    let app = app(&pool);
    let deck = create_deck(&app).await;
    let path = format!("/api/decks/{deck}/positions");
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &path,
            json!({"position_ids":[positions[0].id]})
        )
        .await
        .0,
        200
    );
    let before: (Value, i64) = sqlx::query_as("SELECT payload,version FROM learning_cards")
        .fetch_one(&pool)
        .await
        .unwrap();
    native_analysis(
        &pool,
        positions[0].id,
        2,
        "better answer that must roll back",
    )
    .await;
    sqlx::query("WITH inserted AS (INSERT INTO learning_cards(id,signature,payload) SELECT gen_random_uuid(),'bound-'||n,'{}'::jsonb FROM generate_series(1,999) n RETURNING id) INSERT INTO deck_cards(deck_id,card_id) SELECT $1,id FROM inserted").bind(deck).execute(&pool).await.unwrap();
    let (status, error) = call(
        app,
        "POST",
        &path,
        json!({"position_ids":[positions[0].id,positions[1].id]}),
    )
    .await;
    assert_eq!(status, 422, "{error}");
    let counts: (i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM learning_cards),(SELECT count(*) FROM deck_cards WHERE deck_id=$1)").bind(deck).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1000, 1000));
    let after: (Value, i64) = sqlx::query_as(
        "SELECT payload,version FROM learning_cards WHERE payload->'source'->>'position_id'=$1",
    )
    .bind(positions[0].id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        after, before,
        "a rejected batch cannot refresh shared payloads or versions"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_propagates_unexpected_database_errors_and_rolls_back_valid_writes(pool: PgPool) {
    let positions = independent_positions(&pool).await;
    native_analysis(&pool, positions[0].id, 1, "valid native answer").await;
    native_analysis(&pool, positions[1].id, 1, "reject transaction").await;
    sqlx::query("ALTER TABLE learning_cards ADD CONSTRAINT reject_test_write CHECK (payload->'solution'->'candidates'->0->>'notation' <> 'reject transaction')").execute(&pool).await.unwrap();
    let app = app(&pool);
    let deck = create_deck(&app).await;
    let (status, error) = call(
        app,
        "POST",
        &format!("/api/decks/{deck}/positions"),
        json!({"position_ids":[positions[0].id,Uuid::new_v4(),positions[1].id]}),
    )
    .await;
    assert_eq!(status, 500, "{error}");
    assert!(error["error"].is_string());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM learning_cards")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM deck_cards")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_and_legacy_imports_share_stable_lock_order_and_card_identity(pool: PgPool) {
    let positions = independent_positions(&pool).await;
    for position in &positions {
        native_analysis(&pool, position.id, 1, "native best").await;
    }
    let app = app(&pool);
    let a = create_deck(&app).await;
    let b = create_deck(&app).await;
    let ids: Vec<_> = positions.iter().map(|p| p.id).collect();
    let mut reverse = ids.clone();
    reverse.reverse();
    let a_path = format!("/api/decks/{a}/positions");
    let b_path = format!("/api/decks/{b}/positions");
    let (left, right) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(
            call(app.clone(), "POST", &a_path, json!({"position_ids":ids})),
            call(
                app.clone(),
                "POST",
                &b_path,
                json!({"position_ids":reverse})
            ),
        )
    })
    .await
    .expect("opposite selection orders must not deadlock");
    assert_eq!(left.0, 200, "{:?}", left);
    assert_eq!(right.0, 200, "{:?}", right);
    assert_eq!(left.1["added"], 2);
    assert_eq!(right.1["added"], 2);
    for position in positions {
        let (status, result) = call(
            app.clone(),
            "POST",
            &format!("/api/decks/{a}/cards"),
            json!({"position_id":position.id}),
        )
        .await;
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["added"], 0);
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM learning_cards")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM deck_cards")
            .fetch_one(&pool)
            .await
            .unwrap(),
        4
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_requires_a_usable_selected_native_match_solution_and_accepts_forced_moves(
    pool: PgPool,
) {
    let position = positions(&pool, SOURCE).await.remove(0);
    let match_job = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,result,total_games,completed_games) VALUES($1,$2,'match',2,'completed',$3,1,1)")
        .bind(match_job).bind(position.match_uuid).bind(json!({"engine":"Native match engine"})).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no,summary) VALUES($1,1,'{}')")
        .bind(match_job)
        .execute(&pool)
        .await
        .unwrap();
    let mut data = json!({"decision_no":1,"checker_no":1,"player":position.player,"kind":"checker","played":position.notation,"best":"","played_equity":null,"best_equity":null,"equity_loss":0.0,"severity":"none","dice":position.dice,"evaluation_status":"unavailable"});
    sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,$3,1,$4,'checker',0,$5)")
        .bind(match_job).bind(position.id).bind(position.game_no).bind(position.player).bind(&data).execute(&pool).await.unwrap();
    let app = app(&pool);
    let deck = create_deck(&app).await;
    let path = format!("/api/decks/{deck}/positions");
    for (status, best, played_equity, best_equity, loss) in [
        ("unavailable", "", Value::Null, Value::Null, 0.0),
        ("evaluated", "  ", json!(0.0), json!(0.1), 0.1),
        ("evaluated", "native best", Value::Null, Value::Null, 0.0),
        ("forced", "native forced", Value::Null, Value::Null, 0.1),
    ] {
        data["evaluation_status"] = json!(status);
        data["best"] = json!(best);
        data["played_equity"] = played_equity;
        data["best_equity"] = best_equity;
        data["equity_loss"] = json!(loss);
        sqlx::query("UPDATE move_analyses SET data=$2,equity_loss=$3 WHERE job_id=$1")
            .bind(match_job)
            .bind(&data)
            .bind(loss)
            .execute(&pool)
            .await
            .unwrap();
        let (code, result) = call(
            app.clone(),
            "POST",
            &path,
            json!({"position_ids":[position.id]}),
        )
        .await;
        assert_eq!(code, 200, "{result}");
        assert_eq!(
            result["items"][0]["status"], "analysis_required",
            "source {data}: {result}"
        );
        assert!(result["items"][0].get("card_id").is_none());
        assert_eq!(result["total"], 0);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM learning_cards")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }
    data["evaluation_status"] = json!("forced");
    data["best"] = json!("Cannot move");
    data["equity_loss"] = json!(0.0);
    sqlx::query("UPDATE move_analyses SET data=$2,equity_loss=0 WHERE job_id=$1")
        .bind(match_job)
        .bind(&data)
        .execute(&pool)
        .await
        .unwrap();
    let (code, result) = call(app, "POST", &path, json!({"position_ids":[position.id]})).await;
    assert_eq!(code, 200, "{result}");
    assert_eq!(result["items"][0]["status"], "added");
    let solution: Value = sqlx::query_scalar("SELECT payload->'solution' FROM learning_cards")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(solution["decision"]["best"], "Cannot move");
    assert!(solution["decision"]["played_equity"].is_null());
    assert!(solution["decision"]["best_equity"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_does_not_fall_back_from_the_selected_individual_hint_with_an_empty_answer(
    pool: PgPool,
) {
    let position = positions(&pool, SOURCE).await.remove(0);
    native_analysis(&pool, position.id, 1, "older native best").await;
    let selected = native_analysis(&pool, position.id, 2, "   ").await;
    let app = app(&pool);
    let deck = create_deck(&app).await;
    let path = format!("/api/decks/{deck}/positions");
    let (status, result) = call(
        app.clone(),
        "POST",
        &path,
        json!({"position_ids":[position.id]}),
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["items"][0]["status"], "analysis_required");
    assert_eq!(result["total"], 0);
    sqlx::query("UPDATE jobs SET result=jsonb_set(result,'{candidates,0,notation}','\"selected native best\"'::jsonb) WHERE id=$1")
        .bind(selected).execute(&pool).await.unwrap();
    let (status, result) = call(app, "POST", &path, json!({"position_ids":[position.id]})).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["items"][0]["status"], "added");
    let source: Uuid =
        sqlx::query_scalar("SELECT (payload->'source'->>'job_id')::uuid FROM learning_cards")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(source, selected);
}

#[sqlx::test(migrations = "./migrations")]
async fn catalog_cards_use_explicit_selection_without_engine_or_depth_override(pool: PgPool) {
    let position = positions(&pool, SOURCE).await.remove(0);
    let selected = native_analysis(&pool, position.id, 1, "selected shallow answer").await;
    let deeper = native_analysis(&pool, position.id, 4, "unselected deep answer").await;
    let app = app(&pool);
    let deck = create_deck(&app).await;
    let (status, response) = call(
        app.clone(),
        "POST",
        &format!("/api/decks/{deck}/cards"),
        json!({"position_id":position.id,"analysis_id":format!("job:{selected}")}),
    )
    .await;
    assert_eq!(status, 200, "{response}");
    let payload: Value = sqlx::query_scalar("SELECT payload FROM learning_cards")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(payload["source"]["job_id"], selected.to_string());
    assert_eq!(payload["solution"]["plies"], 1);
    let (status, response) = call(
        app,
        "PUT",
        &format!("/api/matches/{}/analysis-selection", position.match_uuid),
        json!({"analysis_id":format!("job:{deeper}")}),
    )
    .await;
    assert_eq!(status, 200, "{response}");
    let after: Value = sqlx::query_scalar("SELECT payload FROM learning_cards")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        after, payload,
        "changing a saved selection must not rewrite historical card solutions"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn catalog_cards_keep_imported_provenance_and_reject_other_position_scope(pool: PgPool) {
    let position = positions(&pool, include_str!("fixtures/analysis-game1.sgf"))
        .await
        .remove(0);
    sqlx::query("UPDATE imported_analyses SET data=jsonb_set(data || '{\"depth\":null,\"depth_plies\":null}'::jsonb,'{candidates}',(SELECT jsonb_agg(candidate || '{\"depth\":null,\"depth_plies\":null}'::jsonb) FROM jsonb_array_elements(data->'candidates') candidate)) WHERE position_uuid=$1 AND kind='checker'")
        .bind(position.id).execute(&pool).await.unwrap();
    let app = app(&pool);
    let deck = create_deck(&app).await;
    let (status, response) = call(
        app.clone(),
        "POST",
        &format!("/api/decks/{deck}/cards"),
        json!({"position_id":position.id,"analysis_id":format!("imported:{}",position.match_uuid)}),
    )
    .await;
    assert_eq!(status, 200, "{response}");
    let payload: Value = sqlx::query_scalar("SELECT payload FROM learning_cards")
        .fetch_one(&pool)
        .await
        .unwrap();
    let record: (Uuid, Value) = sqlx::query_as("SELECT id,data FROM imported_analyses WHERE position_uuid=$1 AND kind='checker' ORDER BY source_ordinal LIMIT 1")
        .bind(position.id).fetch_one(&pool).await.unwrap();
    assert!(payload["source"]["job_id"].is_null());
    assert_eq!(
        payload["source"]["imported_analysis_id"],
        record.0.to_string()
    );
    assert_eq!(payload["solution"]["imported"], record.1);
    assert!(
        payload["solution"]["plies"].is_null(),
        "unknown source depth must remain absent"
    );
    let other = positions(&pool, SOURCE).await.remove(0);
    let job = native_analysis(&pool, other.id, 2, "unrelated answer").await;
    let (status, _) = call(
        app,
        "POST",
        &format!("/api/decks/{deck}/cards"),
        json!({"position_id":position.id,"analysis_id":format!("job:{job}")}),
    )
    .await;
    assert_eq!(status, 422);
}

#[sqlx::test(migrations = "./migrations")]
async fn catalog_imported_readiness_filters_match_card_creation(pool: PgPool) {
    let position = positions(&pool, include_str!("fixtures/analysis-game1.sgf"))
        .await
        .remove(0);
    let app = app(&pool);
    let filter =
        json!({"schema_version":1,"analysis_state":"ready","position_id":position.position_id});
    let encoded: String = filter
        .to_string()
        .bytes()
        .map(|b| format!("%{b:02X}"))
        .collect();
    let (status, page) = call(
        app.clone(),
        "GET",
        &format!("/api/position-browser?filters={encoded}"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{page}");
    assert!(
        page["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["position"]["id"] == position.id.to_string()),
        "{page}"
    );
    let hit = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["position"]["id"] == position.id.to_string())
        .unwrap();
    assert!(hit["native_error"]["job_id"].is_null());
    assert_eq!(
        hit["native_error"]["analysis_id"],
        format!("imported:{}", position.match_uuid)
    );
    assert!(hit["native_error"]["imported_analysis_id"].is_string());
    let (status, readiness) = call(
        app.clone(),
        "POST",
        "/api/position-browser/analysis-readiness",
        json!({"position_ids":[position.id]}),
    )
    .await;
    assert_eq!(status, 200, "{readiness}");
    assert_eq!(readiness["analysis_required"], 0, "{readiness}");
    assert_eq!(readiness["unavailable"], 0, "{readiness}");
    let original:Value=sqlx::query_scalar("SELECT data FROM imported_analyses WHERE position_uuid=$1 AND kind='checker' ORDER BY source_ordinal LIMIT 1").bind(position.id).fetch_one(&pool).await.unwrap();
    sqlx::query("UPDATE matches SET source_revision=source_revision+1 WHERE id=$1")
        .bind(position.match_uuid)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE match_moves SET notation='invalid play' WHERE position_uuid=$1")
        .bind(position.id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, page) = call(
        app,
        "GET",
        &format!("/api/position-browser?filters={encoded}"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(
        page["items"],
        json!([]),
        "a cached projection cannot outlive its source context revision"
    );
    let after:Value=sqlx::query_scalar("SELECT data FROM imported_analyses WHERE position_uuid=$1 AND kind='checker' ORDER BY source_ordinal LIMIT 1").bind(position.id).fetch_one(&pool).await.unwrap();
    assert_eq!(
        after, original,
        "projection refresh must leave the imported source untouched"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn catalog_batch_cards_reject_unavailable_selected_decisions(pool: PgPool) {
    let position = positions(&pool, SOURCE).await.remove(0);
    let job = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,completed_games,result) VALUES($1,$2,'match',1,'completed',1,1,'{\"engine\":\"Test native engine\"}')").bind(job).bind(position.match_uuid).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,1)")
        .bind(job)
        .execute(&pool)
        .await
        .unwrap();
    let decision = json!({"decision_no":1,"checker_no":1,"player":position.player,"kind":"checker","played":position.notation,"best":"","played_equity":null,"best_equity":null,"equity_loss":0.0,"severity":"none","dice":position.dice,"evaluation_status":"unavailable"});
    sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,1,1,$3,'checker',0,$4)").bind(job).bind(position.id).bind(position.player).bind(decision).execute(&pool).await.unwrap();
    let app = app(&pool);
    let deck = create_deck(&app).await;
    let (status, response) = call(
        app,
        "POST",
        &format!("/api/decks/{deck}/cards"),
        json!({"match_id":position.match_uuid,"analysis_id":format!("job:{job}")}),
    )
    .await;
    assert_eq!(status, 422, "{response}");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM learning_cards")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn catalog_consumers_keep_open_sage_checkpoint_provenance_without_job_summary(pool: PgPool) {
    let position = positions(&pool, SOURCE).await.remove(0);
    let job = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,completed_games,engine_snapshot) VALUES($1,$2,'match',2,'completed',1,1,$3)")
        .bind(job).bind(position.match_uuid).bind(json!({"engine_id":"bgsage","gnubg_path":"unused","engine_version":null,"open_sage":null})).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,1)")
        .bind(job)
        .execute(&pool)
        .await
        .unwrap();
    let decision = json!({"decision_no":1,"checker_no":1,"player":position.player,"kind":"checker","played":position.notation,"best":"24/21 6/5","played_equity":0.0,"best_equity":0.2,"equity_loss":0.2,"severity":"blunder","dice":position.dice,"evaluation_status":"evaluated"});
    sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,1,1,$3,'checker',0.2,$4)").bind(job).bind(position.id).bind(position.player).bind(decision).execute(&pool).await.unwrap();
    let app = app(&pool);
    let deck = create_deck(&app).await;
    let (status, page) = call(app.clone(), "GET", "/api/position-browser", Value::Null).await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["items"][0]["native_error"]["engine"], "NeonGammon");
    for body in [
        json!({"position_id":position.id,"analysis_id":format!("job:{job}")}),
        json!({"match_id":position.match_uuid,"analysis_id":format!("job:{job}")}),
    ] {
        let (status, response) = call(
            app.clone(),
            "POST",
            &format!("/api/decks/{deck}/cards"),
            body,
        )
        .await;
        assert_eq!(status, 200, "{response}");
        let engine: String =
            sqlx::query_scalar("SELECT payload#>>'{solution,engine}' FROM learning_cards")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(engine, "NeonGammon");
    }
    let (_, groups) = call(
        app.clone(),
        "GET",
        "/api/study/mistakes?player=Alice&min_loss=0.1",
        Value::Null,
    )
    .await;
    let (status,practice)=call(app,"POST","/api/study/mistakes/practice",json!({"player":"Alice","min_loss":0.1,"mistake_kind":"checker","game_plan":groups["groups"][0]["game_plan"]})).await;
    assert_eq!(status, 200, "{practice}");
    assert_eq!(practice["items"][0]["engine"], "NeonGammon");
}
