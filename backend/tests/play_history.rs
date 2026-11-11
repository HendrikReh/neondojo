use axum::{body::Body, http::Request};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

fn bits(value: &Value, offset: usize, width: usize) -> u32 {
    let bytes = STANDARD_NO_PAD.decode(value.as_str().unwrap()).unwrap();
    (0..width)
        .map(|n| (((bytes[(offset + n) / 8] >> ((offset + n) % 8)) & 1) as u32) << n)
        .sum()
}
async fn checkpoint(pool: &PgPool, sgf: &str, history: Value) -> (Uuid, Value) {
    let id = Uuid::new_v4();
    let session = json!({"id":id,"version":7,"human_player":0,"human_name":"Human","match_length":5,
        "bearoff_direction":"clockwise","engine":{"id":"gnubg","version":"fixture","plies":0},
        "position":{"id":id,"match_uuid":id,"ordinal":0,"game_no":1,"ply":0,"player":0,"dice":[],"board":[],"position_id":"","gnubg_match_id":"","notation":""},
        "phase":"roll","can_double":false,"legal_moves":[],"history":history,"scores":[0,0],"game_no":1,"winner":null});
    sqlx::query("INSERT INTO play_sessions(id,version,state,gnubg_path,sgf,native_id,library_archived) VALUES($1,7,$2,'/missing/native', $3,'unchanged',true)")
        .bind(id).bind(&session).bind(sgf).execute(pool).await.unwrap();
    (id, session)
}
async fn get(pool: &PgPool, id: Uuid, version: i64, expected: u16) -> Value {
    let app = api::router(AppState::new(
        pool.clone(),
        Engine::new("/missing/native".into()),
    ));
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/play/sessions/{id}/history?version={version}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(status, expected, "{}", String::from_utf8_lossy(&bytes));
    serde_json::from_slice(&bytes).unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn play_history_snapshots_are_versioned_and_read_only(pool: PgPool) {
    let sgf = "(;GM[6]FF[4]MI[length:5][ws:0][bs:0];W[31hefe];B[double];W[take];B[42qusu])";
    let (id, session) = checkpoint(
        &pool,
        sgf,
        json!([
            {"player":0,"dice":[3,1],"notation":"8/5 6/5"},
            {"player":1,"dice":[],"notation":"double"},
            {"player":0,"dice":[],"notation":"take"},
            {"player":1,"dice":[4,2],"notation":"8/4 6/4"}
        ]),
    )
    .await;
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(s) FROM play_sessions s WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let result = get(&pool, id, 7, 200).await;
    assert_eq!(result["session_id"], json!(id));
    assert_eq!(result["version"], 7);
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), session["history"].as_array().unwrap().len());
    for (index, item) in items.iter().enumerate() {
        assert_eq!(item["index"], index);
        assert_eq!(item["move_no"], index + 1);
        assert_eq!(item["game_no"], 1);
        assert_eq!(
            item["before"]["player"],
            session["history"][index]["player"]
        );
        assert_eq!(item["before"]["dice"], session["history"][index]["dice"]);
        assert_eq!(item["after"]["dice"], json!([]));
        assert_eq!(item["before"]["match_uuid"], json!(id));
    }
    assert_eq!(items[0]["kind"], "checker");
    assert_eq!(items[0]["before"]["notation"], "8/5 6/5");
    assert_eq!(items[0]["after"]["board"], items[1]["before"]["board"]);
    assert_eq!(
        items[0]["after"]["position_id"],
        items[1]["before"]["position_id"]
    );
    assert_eq!(
        items[2]["after"]["position_id"],
        items[3]["before"]["position_id"]
    );
    assert_eq!(items[1]["kind"], "cube");
    assert_eq!(bits(&items[1]["after"]["gnubg_match_id"], 12, 1), 1);
    assert_eq!(bits(&items[2]["before"]["gnubg_match_id"], 6, 1), 1);
    assert_eq!(bits(&items[2]["before"]["gnubg_match_id"], 11, 1), 0);
    assert_eq!(bits(&items[2]["after"]["gnubg_match_id"], 0, 4), 1);
    assert_eq!(bits(&items[2]["after"]["gnubg_match_id"], 4, 2), 0);
    assert_eq!(bits(&items[2]["after"]["gnubg_match_id"], 12, 1), 0);
    get(&pool, id, 6, 409).await;
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(s) FROM play_sessions s WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM matches")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn play_history_empty_pending_match_has_no_events(pool: PgPool) {
    let (id, _) = checkpoint(&pool, "", json!([])).await;
    assert_eq!(get(&pool, id, 7, 200).await["items"], json!([]));
}

#[sqlx::test(migrations = "./migrations")]
async fn play_history_preserves_hits_passes_black_setup_and_game_boundaries(pool: PgPool) {
    // Native SETBOARD export says White despite a Black first action. It must
    // be normalized only for parsing, leaving the saved checkpoint untouched.
    let sgf = "(;GM[6]FF[4]MI[length:5][ws:0][bs:0]RE[W+1R];PL[W]AE[a:y]AW[e]AB[b];B[31beef])\n(;GM[6]FF[4]MI[length:5][ws:1][bs:0];PL[W]AE[a:y]AW[y]AB[s][s][t][t][u][u][v][v][w][w][x][x];W[61])";
    let (id, _) = checkpoint(
        &pool,
        sgf,
        json!([
            {"player":1,"dice":[3,1],"notation":"23/20*/19"},
            {"player":1,"dice":[],"notation":"resign"},
            {"player":0,"dice":[6,1],"notation":"pass"}
        ]),
    )
    .await;
    let result = get(&pool, id, 7, 200).await;
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["before"]["player"], 1);
    assert_ne!(items[0]["before"]["board"], items[0]["after"]["board"]);
    assert_eq!(items[0]["after"]["board"][25], 1);
    assert!(
        items[0]["before"]["notation"]
            .as_str()
            .unwrap()
            .contains('*')
    );
    assert_eq!(items[1]["kind"], "resign");
    assert_eq!(items[1]["before"]["board"], items[1]["after"]["board"]);
    assert_eq!(items[2]["game_no"], 2);
    assert_eq!(items[2]["move_no"], 1);
    assert_eq!(items[2]["before"]["ply"], 1);
    assert_eq!(items[2]["before"]["notation"], "Cannot move");
    assert_eq!(items[2]["before"]["board"], items[2]["after"]["board"]);
    assert_eq!(bits(&items[1]["after"]["gnubg_match_id"], 36, 15), 0);
    assert_eq!(bits(&items[2]["before"]["gnubg_match_id"], 36, 15), 1);
    assert_eq!(bits(&items[2]["after"]["gnubg_match_id"], 36, 15), 1);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT sgf FROM play_sessions WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        sgf
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn play_history_drop_keeps_game_board_cube_value_and_start_scores(pool: PgPool) {
    let (id, _) = checkpoint(&pool,
        "(;GM[6]FF[4]MI[length:5][ws:2][bs:1]RE[W+2];CV[2]CO[w];W[double];B[drop])",
        json!([{"player":0,"dice":[],"notation":"double"},{"player":1,"dice":[],"notation":"drop"}])).await;
    let result = get(&pool, id, 7, 200).await;
    let drop = &result["items"][1];
    assert_eq!(drop["before"]["board"], drop["after"]["board"]);
    assert_eq!(bits(&drop["before"]["gnubg_match_id"], 12, 1), 1);
    assert_eq!(bits(&drop["after"]["gnubg_match_id"], 12, 1), 0);
    assert_eq!(bits(&drop["after"]["gnubg_match_id"], 0, 4), 1);
    assert_eq!(bits(&drop["after"]["gnubg_match_id"], 4, 2), 0);
    assert_eq!(bits(&drop["after"]["gnubg_match_id"], 36, 15), 2);
    assert_eq!(bits(&drop["after"]["gnubg_match_id"], 51, 15), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn play_history_rejects_mismatched_saved_records_and_unknown_sessions(pool: PgPool) {
    get(&pool, Uuid::new_v4(), 7, 404).await;
    for history in [
        json!([]),
        json!([{"player":1,"dice":[3,1],"notation":"8/5 6/5"}]),
        json!([{"player":0,"dice":[4,1],"notation":"8/5 6/5"}]),
        json!([{"player":0,"dice":[],"notation":"double"}]),
    ] {
        let (id, _) = checkpoint(&pool, "(;GM[6]FF[4]MI[length:5];W[31hefe])", history).await;
        get(&pool, id, 7, 500).await;
        get(&pool, id, 6, 409).await;
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn play_history_winning_bearoff_keeps_exact_final_board_without_next_roll(pool: PgPool) {
    for (player, setup, action, remaining_sign) in [
        (0, "AW[a]AB[s][s][s]", "W[61az]", -1),
        (1, "AB[x]AW[f][f][f]", "B[61xz]", 1),
    ] {
        let sgf = format!(
            "(;GM[6]FF[4]MI[length:5][ws:2][bs:1];PL[{}]AE[a:y]{setup};{action})",
            if player == 0 { "W" } else { "B" }
        );
        let (id, _) = checkpoint(
            &pool,
            &sgf,
            json!([{"player":player,"dice":[6,1],"notation":"1/off"}]),
        )
        .await;
        let response = get(&pool, id, 7, 200).await;
        let item = &response["items"][0];
        assert_eq!(item["before"]["notation"], "1/off");
        assert_eq!(item["after"]["player"], player);
        assert_eq!(item["after"]["dice"], json!([]));
        let board = item["after"]["board"].as_array().unwrap();
        assert_eq!(
            board.iter().map(|n| n.as_i64().unwrap()).sum::<i64>(),
            remaining_sign * 3
        );
        assert!(
            !board
                .iter()
                .any(|n| n.as_i64().unwrap().signum() == if player == 0 { 1 } else { -1 })
        );
        assert_eq!(bits(&item["after"]["gnubg_match_id"], 36, 15), 2);
        assert_eq!(bits(&item["after"]["gnubg_match_id"], 51, 15), 1);
    }
}
