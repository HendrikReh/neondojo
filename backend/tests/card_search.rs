use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{api, engine::Engine, import, store};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn call(app: &axum::Router, path: &str) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"body":String::from_utf8_lossy(&bytes)})),
    )
}
async fn fixture(pool: &PgPool, count: u128) -> (axum::Router, Uuid, Uuid, Uuid) {
    let sgf = "(;GM[6]PW[Mochy]PB[Hendrik]MI[length:7][ws:0][bs:0];W[31hefe])";
    let imported = store::import_match(
        pool,
        "search.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    neondojo::move_catalog::ensure_catalog(pool, imported.r#match.id)
        .await
        .unwrap();
    let position = store::positions(pool, Some(imported.r#match.id), None, None, 1)
        .await
        .unwrap()
        .items
        .remove(0);
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    for (id, name) in [(a, "Race"), (b, "Cube")] {
        sqlx::query("INSERT INTO decks(id,name) VALUES($1,$2)")
            .bind(id)
            .bind(name)
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO annotations(id,version,match_uuid,tags,comment) VALUES(gen_random_uuid(),1,$1,ARRAY['race','50%'],'Exact 50%_ lesson')").bind(imported.r#match.id).execute(pool).await.unwrap();
    for i in 1..=count {
        let mut pos = position.clone();
        pos.player = if i % 2 == 0 { 1 } else { 0 };
        let payload = json!({"kind":if i%2==0 {"cube_offer"}else{"checker"},"position":pos,"players":["Mochy","Hendrik"],"prompt":"What is the best checker play?","source":{"kind":"analysis","job_id":Uuid::new_v4(),"match_id":imported.r#match.id,"position_id":position.id,"game_no":1,"decision_no":null},"solution":{"engine":"Hidden answer engine","plies":2,"candidates":[],"decision":null},"played":"Secret played move"});
        let id = Uuid::from_u128(i);
        sqlx::query("INSERT INTO learning_cards(id,signature,payload,review_count,due_at) VALUES($1,$2,$3,$4,now()+interval '1 day' * $5)")
            .bind(id).bind(id.to_string()).bind(payload).bind(if i%2==0 {1}else{0}).bind(if i%2==0 {1}else{-1}).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO deck_cards(deck_id,card_id) VALUES($1,$2)")
            .bind(a)
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
        if i == 1 {
            sqlx::query("INSERT INTO deck_cards(deck_id,card_id) VALUES($1,$2)")
                .bind(b)
                .bind(id)
                .execute(pool)
                .await
                .unwrap();
        }
    }
    (
        api::router(api::AppState::new(
            pool.clone(),
            Engine::new("unused".into()),
        )),
        a,
        b,
        position.id,
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn search_filters_before_pagination_and_shares_one_row_across_decks(pool: PgPool) {
    let (app, a, b, position) = fixture(&pool, 61).await;
    let (status, first) = call(&app, "/api/cards?limit=50").await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["total"], 61);
    assert_eq!(first["items"].as_array().unwrap().len(), 50);
    let card = &first["items"][0];
    assert_eq!(card["browse"]["decks"].as_array().unwrap().len(), 2);
    assert_eq!(card["browse"]["tags"], json!(["50%", "race"]));
    assert_eq!(card["browse"]["comment_excerpt"], "Exact 50%_ lesson");
    assert!(card.get("answer").is_none());
    assert!(!card.to_string().contains("Hidden answer"));
    assert_eq!(card["position"]["notation"], "");
    let (_, last) = call(
        &app,
        &format!(
            "/api/cards?limit=50&after={}",
            first["next_cursor"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(last["items"].as_array().unwrap().len(), 11);
    assert!(last["next_cursor"].is_null());
    assert_eq!(last["total"], 61);
    let (status, numbered) = call(&app, "/api/cards?limit=50&page=2").await;
    assert_eq!(status, 200, "{numbered}");
    assert_eq!(numbered["total"], 61);
    assert_eq!(numbered["items"], last["items"]);
    for (query, total) in [
        ("q=MOCHY", 61),
        ("player=MOCHY", 31),
        ("kind=cube_offer", 30),
        ("status=new", 31),
        ("status=due", 31),
        ("status=reviewed", 30),
        ("tag=RACE", 61),
        ("comment=50%25_", 61),
        ("comment=50%25x", 0),
        ("q=Secret", 0),
        ("q=Hidden", 0),
    ] {
        let (status, result) = call(&app, &format!("/api/cards?{query}")).await;
        assert_eq!(status, 200, "{query}: {result}");
        assert_eq!(result["total"], total, "{query}: {result}");
    }
    let (_, filtered) = call(
        &app,
        &format!("/api/cards?deck_id={b}&player=Mochy&tag=race"),
    )
    .await;
    assert_eq!(filtered["total"], 1);
    let (_, scoped) = call(
        &app,
        &format!("/api/decks/{a}/cards?kind=cube_offer&limit=1"),
    )
    .await;
    assert_eq!(scoped["total"], 30);
    assert_eq!(scoped["items"][0]["kind"], "cube_offer");
    let checker_id: String = sqlx::query_scalar("SELECT position_id FROM positions WHERE id=$1")
        .bind(position)
        .fetch_one(&pool)
        .await
        .unwrap();
    let (_, exact) = call(&app, &format!("/api/cards?position_id={checker_id}")).await;
    assert_eq!(exact["total"], 61);
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM review_sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sessions, 0);
    let count: i64 = sqlx::query_scalar("SELECT sum(review_count) FROM learning_cards")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 30);
}

#[sqlx::test(migrations = "./migrations")]
async fn search_validates_filters_and_reads_fresh_notes_in_each_scope(pool: PgPool) {
    let (app, a, b, position) = fixture(&pool, 2).await;
    for query in [
        "limit=0",
        "limit=101",
        "kind=invalid",
        "status=invalid",
        "position_id=bad",
        "deck_id=bad",
        "player=%00",
    ] {
        let (status, error) = call(&app, &format!("/api/cards?{query}")).await;
        assert_eq!(status, 422, "{query}: {error}");
        assert!(error["error"].is_string(), "{error}");
    }
    assert_eq!(
        call(&app, &format!("/api/cards?deck_id={}", Uuid::new_v4()))
            .await
            .0,
        404
    );
    assert_eq!(
        call(&app, &format!("/api/decks/{a}/cards?deck_id={b}"))
            .await
            .0,
        422
    );
    let game:Uuid=sqlx::query_scalar("SELECT g.id FROM match_games g JOIN positions p ON p.match_uuid=g.match_uuid AND p.game_no=g.game_no WHERE p.id=$1").bind(position).fetch_one(&pool).await.unwrap();
    let movement: Uuid = sqlx::query_scalar("SELECT id FROM match_moves WHERE position_uuid=$1")
        .bind(position)
        .fetch_one(&pool)
        .await
        .unwrap();
    for (column, id, tag) in [
        ("position_uuid", position, "position-note"),
        ("game_uuid", game, "game-note"),
        ("move_uuid", movement, "move-note"),
    ] {
        sqlx::query(&format!("INSERT INTO annotations(id,version,{column},tags,comment) VALUES(gen_random_uuid(),1,$1,ARRAY[$2],$2)")).bind(id).bind(tag).execute(&pool).await.unwrap();
        let (_, result) = call(&app, &format!("/api/cards?tag={tag}")).await;
        assert_eq!(result["total"], 2, "{result}");
        assert!(
            result["items"][0]["browse"]["comment_excerpt"]
                .as_str()
                .unwrap()
                .contains(tag)
        );
    }
    sqlx::query("UPDATE annotations SET comment='new lesson' WHERE position_uuid=$1")
        .bind(position)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(&app, "/api/cards?comment=new%20lesson").await.1["total"],
        2
    );
    // Shared cards stay searchable even after their final deck membership is removed.
    sqlx::query("DELETE FROM decks")
        .execute(&pool)
        .await
        .unwrap();
    let (_, unassigned) = call(&app, "/api/cards").await;
    assert_eq!(unassigned["total"], 2);
    assert_eq!(unassigned["items"][0]["browse"]["decks"], json!([]));
}

#[sqlx::test(migrations = "./migrations")]
async fn cards_and_saved_notes_remain_searchable_after_source_match_deletion(pool: PgPool) {
    let (app, _, _, position) = fixture(&pool, 2).await;
    sqlx::query("INSERT INTO saved_positions(position_uuid) VALUES($1)")
        .bind(position)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO annotations(id,version,position_uuid,tags,comment) VALUES(gen_random_uuid(),1,$1,ARRAY['keep'],'Independent lesson')").bind(position).execute(&pool).await.unwrap();
    let match_id: Uuid = sqlx::query_scalar("SELECT match_uuid FROM positions WHERE id=$1")
        .bind(position)
        .fetch_one(&pool)
        .await
        .unwrap();
    let game: Uuid =
        sqlx::query_scalar("SELECT id FROM match_games WHERE match_uuid=$1 AND game_no=1")
            .bind(match_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let movement: Uuid = sqlx::query_scalar("SELECT id FROM match_moves WHERE position_uuid=$1")
        .bind(position)
        .fetch_one(&pool)
        .await
        .unwrap();
    for (column, id, tag) in [
        ("game_uuid", game, "saved-game"),
        ("move_uuid", movement, "saved-move"),
    ] {
        sqlx::query(&format!("INSERT INTO annotations(id,version,{column},tags,comment) VALUES(gen_random_uuid(),1,$1,ARRAY[$2],$2)")).bind(id).bind(tag).execute(&pool).await.unwrap();
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/library/actions")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"action":"delete","match_ids":[match_id]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let (status, result) = call(&app, "/api/cards?tag=keep&comment=Independent").await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["total"], 2);
    assert!(
        result["items"][0]["browse"]["tags"]
            .as_array()
            .unwrap()
            .contains(&json!("keep"))
    );
    for tag in ["saved-game", "saved-move"] {
        assert_eq!(
            call(&app, &format!("/api/cards?tag={tag}")).await.1["total"],
            2,
            "{tag}"
        );
    }
    assert!(
        result["items"][0]["position"]["notation"]
            .as_str()
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn cube_notes_use_actual_events_after_virtual_no_double_decisions(pool: PgPool) {
    let (app, _, _, position) = fixture(&pool, 2).await;
    let match_id: Uuid = sqlx::query_scalar("SELECT match_uuid FROM positions WHERE id=$1")
        .bind(position)
        .fetch_one(&pool)
        .await
        .unwrap();
    let job: Uuid = sqlx::query_scalar(
        "SELECT (payload#>>'{source,job_id}')::uuid FROM learning_cards WHERE id=$1",
    )
    .bind(Uuid::from_u128(1))
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO jobs(id,match_uuid,kind,plies,status) VALUES($1,$2,'match',1,'completed')",
    )
    .bind(job)
    .bind(match_id)
    .execute(&pool)
    .await
    .unwrap();
    for (n, kind, played) in [
        (1, "cube", "no double"),
        (2, "checker", "8/5 6/5"),
        (3, "cube", "double"),
    ] {
        sqlx::query("INSERT INTO move_analyses(job_id,game_no,decision_no,kind,player,equity_loss,data) VALUES($1,1,$2,$3,0,0,$4)").bind(job).bind(n).bind(kind).bind(json!({"played":played})).execute(&pool).await.unwrap();
    }
    for (n, player, played, tag) in [
        (2, 0, "double", "actual-offer"),
        (3, 1, "take", "other-response"),
    ] {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO match_moves(id,match_uuid,game_no,move_no,kind,player,notation,board,dice,position_id,gnubg_match_id) SELECT $1,match_uuid,game_no,$2,'cube',$3,$4,board,ARRAY[0,0],position_id,gnubg_match_id FROM positions WHERE id=$5")
            .bind(id).bind(n).bind(player).bind(played).bind(position).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO annotations(id,version,move_uuid,tags,comment) VALUES(gen_random_uuid(),1,$1,ARRAY[$2],$2)").bind(id).bind(tag).execute(&pool).await.unwrap();
    }
    sqlx::query("UPDATE learning_cards SET payload=jsonb_set(jsonb_set(jsonb_set(jsonb_set(payload,'{kind}','\"cube_offer\"'),'{source,position_id}','null'),'{source,decision_no}','3'),'{played}','\"double\"') WHERE id=$1").bind(Uuid::from_u128(1)).execute(&pool).await.unwrap();
    assert_eq!(
        call(&app, "/api/cards?tag=actual-offer").await.1["total"],
        1
    );
    assert_eq!(
        call(&app, "/api/cards?tag=other-response").await.1["total"],
        0
    );
}
