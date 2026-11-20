use neondojo::import::parse_sgf;
use serde_json::{Value, json};

fn result(source: &str) -> Value {
    serde_json::to_value(&parse_sgf(source).unwrap().games[0]).unwrap()["result"].clone()
}
fn terminal_white(extra: &str, black: &str) -> String {
    format!("(;GM[6]MI[length:7][ws:2][bs:1]{extra}AE[a:y]AW[a]{black};W[11az])")
}
fn black_checkers(point: &str, count: usize) -> String {
    format!("AB{}", format!("[{point}]").repeat(count))
}
#[test]
fn decisive_board_explains_single_gammon_and_backgammon() {
    for (loser, count, multiplier, reason) in [
        ("m", 14, 1, "single"),
        ("m", 15, 2, "gammon"),
        ("b", 15, 3, "backgammon"),
        ("y", 15, 3, "backgammon"),
    ] {
        assert_eq!(
            result(&terminal_white("CV[2]CO[w]", &black_checkers(loser, count))),
            json!({"winner":0,"points":2*multiplier,"cube":2,"multiplier":multiplier,"reason":reason,"scores_after":[2+2*multiplier,1]})
        );
    }
}
#[test]
fn drop_uses_cube_before_offer_and_recorded_result_does_not_invent_gammon() {
    assert_eq!(
        result("(;GM[6]MI[length:7]CV[2]CO[b];W[31hefe];B[double];W[drop])"),
        json!({"winner":1,"points":2,"cube":2,"multiplier":1,"reason":"drop","scores_after":[0,2]})
    );
}
#[test]
fn recorded_resignation_and_unfinished_results_are_honest() {
    for (re, reason, multiplier) in [
        ("RE[B+4]", "recorded", Value::Null),
        ("RE[B+4R]", "resignation", json!(2)),
    ] {
        assert_eq!(
            result(&format!("(;GM[6]MI[length:7]CV[2]CO[b]{re};W[31hefe])")),
            json!({"winner":1,"points":4,"cube":2,"multiplier":multiplier,"reason":reason,"scores_after":[0,4]})
        );
    }
    assert_eq!(result("(;GM[6]MI[length:7];W[31hefe])"), Value::Null);
}
#[test]
fn jacoby_suppresses_undoubled_gammons() {
    let sgf = format!(
        "(;GM[6]RU[Jacoby]AE[a:y]AW[a]{};W[11az])",
        black_checkers("m", 15)
    );
    assert_eq!(
        result(&sgf),
        json!({"winner":0,"points":1,"cube":1,"multiplier":1,"reason":"single","scores_after":[1,0]})
    );
}
#[test]
fn conflicting_recorded_points_never_get_a_false_cube_formula() {
    assert_eq!(
        result(&terminal_white(
            "CV[2]CO[w]RE[W+3]",
            &black_checkers("m", 15)
        )),
        json!({"winner":0,"points":3,"cube":2,"multiplier":null,"reason":"recorded","scores_after":[5,1]})
    );
}

#[test]
fn imported_xg_terminal_game_without_re_is_gammon_at_actual_cube() {
    let imported = parse_sgf(include_str!("fixtures/game-results-match.sgf")).unwrap();
    let game = serde_json::to_value(&imported.games[5]).unwrap();
    assert_eq!(
        game["result"],
        json!({"winner":1,"points":4,"cube":2,"multiplier":2,"reason":"gammon","scores_after":[6,7]})
    );
}

use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{api, engine::Engine, store};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn get(pool: &PgPool, uri: &str) -> Value {
    let response = api::router(api::AppState::new(
        pool.clone(),
        Engine::new("/missing/engine".into()),
    ))
    .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
    .await
    .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    serde_json::from_slice(&body).unwrap()
}
#[sqlx::test(migrations = "./migrations")]
async fn stored_import_game_results_are_projected_without_reimport(pool: PgPool) {
    let source = include_str!("fixtures/game-results-match.sgf");
    let imported = parse_sgf(source).unwrap();
    let response = store::import_match(
        &pool,
        "results.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &imported,
    )
    .await
    .unwrap();
    let games = get(
        &pool,
        &format!("/api/matches/{}/games", response.r#match.id),
    )
    .await;
    assert_eq!(
        games[3]["result"],
        json!({"winner":0,"points":4,"cube":4,"multiplier":null,"reason":"recorded","scores_after":[6,2]})
    );
    assert!(
        games
            .as_array()
            .unwrap()
            .iter()
            .all(|game| !game["result"].is_null())
    );
    assert_eq!(games[5]["result"]["reason"], "gammon");
    assert_eq!(games[5]["result"]["scores_after"], json!([6, 7]));
}
#[sqlx::test(migrations = "./migrations")]
async fn legacy_play_results_project_at_final_event_without_mutating_checkpoint(pool: PgPool) {
    let source = "(;GM[6]MI[length:7][ws:2][bs:1]CV[2]CO[b];W[31hefe];B[double];W[drop])\n(;GM[6]MI[length:7][ws:2][bs:3];B[31qtst])";
    let id = Uuid::new_v4();
    let session = json!({"id":id,"version":7,"human_player":0,"human_name":"Human","match_length":7,
        "bearoff_direction":"clockwise","engine":{"id":"gnubg","version":"fixture","plies":1},
        "position":{"id":id,"match_uuid":id,"ordinal":0,"game_no":2,"ply":1,"player":0,"dice":[],"board":[],"position_id":"","gnubg_match_id":"","notation":""},
        "phase":"roll","can_double":false,"legal_moves":[],"history":[
            {"player":0,"dice":[3,1],"notation":"8/5 6/5"},
            {"player":1,"dice":[],"notation":"double"},
            {"player":0,"dice":[],"notation":"drop"},
            {"player":1,"dice":[3,1],"notation":"8/5 6/5"}],"scores":[2,3],"game_no":2,"winner":null});
    sqlx::query("INSERT INTO play_sessions(id,version,state,gnubg_path,sgf,native_id,library_archived) VALUES($1,7,$2,'/missing/native',$3,'unchanged',true)").bind(id).bind(session).bind(source).execute(&pool).await.unwrap();
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(s) FROM play_sessions s WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let value = get(&pool, &format!("/api/play/sessions/{id}")).await;
    let expected = json!([{"game_no":1,"history_index":2,"result":{"winner":1,"points":2,"cube":2,"multiplier":1,"reason":"drop","scores_after":[2,3]}}]);
    assert_eq!(value["game_results"], expected);
    let list = get(&pool, "/api/play/sessions").await;
    assert_eq!(list[0]["game_results"], expected);
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(s) FROM play_sessions s WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
}

#[test]
fn inferred_result_does_not_change_following_implicit_score_context() {
    let source = format!(
        "{}(;GM[6]MI[length:7];W[31hefe])",
        terminal_white("CV[2]CO[w]", &black_checkers("m", 15))
    );
    let imported = parse_sgf(&source).unwrap();
    assert_eq!(
        serde_json::to_value(&imported.games[0]).unwrap()["result"]["scores_after"],
        json!([6, 1])
    );
    assert_eq!(
        imported.games[1].white_score, 2,
        "derived results must not advance the importer's score authority"
    );
    assert_eq!(imported.games[1].black_score, 1);
}

#[test]
fn unknown_or_malformed_awards_do_not_supply_a_result() {
    for re in ["?", "Void", "W+R", "W++2", "W+2RR", "W+2unknown"] {
        assert_eq!(
            result(&format!("(;GM[6]MI[length:7]RE[{re}];W[31hefe])")),
            Value::Null,
            "{re}"
        );
    }
}

#[test]
fn trailing_setup_metadata_cannot_change_the_played_terminal_cube_or_board() {
    let terminal = terminal_white("CV[2]CO[w]", &black_checkers("m", 15));
    for metadata in ["CV[4]", "CV[4]AE[a:y]AW[a]AB[x]"] {
        let source = format!("{};{metadata})", terminal.trim_end_matches(')'));
        assert_eq!(
            result(&source),
            json!({"winner":0,"points":4,"cube":2,"multiplier":2,"reason":"gammon","scores_after":[6,1]})
        );
    }
}

#[test]
fn explicit_next_game_score_records_missing_result_without_claiming_multiplier() {
    let imported = parse_sgf(include_str!("fixtures/game-results-match.sgf")).unwrap();
    assert_eq!(
        serde_json::to_value(&imported.games[3]).unwrap()["result"],
        json!({"winner":0,"points":4,"cube":4,"multiplier":null,"reason":"recorded","scores_after":[6,2]})
    );
    assert!(imported.games.iter().all(|game| game.result.is_some()));
}

#[test]
fn implicit_partial_or_ambiguous_next_scores_do_not_invent_an_outcome() {
    for next in [
        "",
        "[ws:2]",
        "[bs:3]",
        "[ws:2][bs:3]",
        "[ws:0][bs:1]",
        "[ws:1][bs:1]",
    ] {
        let source = format!(
            "(;GM[6]MI[length:7][ws:1][bs:1];W[31hefe])(;GM[6]MI[length:7]{next};W[31hefe])"
        );
        assert_eq!(result(&source), Value::Null, "{next}");
    }
}

#[test]
fn explicit_unknown_or_void_result_is_not_overridden_by_next_scores() {
    for re in ["Void", "?", "W+R", "W+2unknown"] {
        let source = format!(
            "(;GM[6]MI[length:7][ws:1][bs:1]RE[{re}];W[31hefe])(;GM[6]MI[length:7][ws:3][bs:1];W[31hefe])"
        );
        assert_eq!(result(&source), Value::Null, "{re}");
    }
}

#[test]
fn declared_game_gap_cannot_attribute_multiple_games_score_delta_to_one_game() {
    let source = "(;GM[6]MI[length:7][game:0][ws:1][bs:1];W[31hefe])(;GM[6]MI[length:7][game:2][ws:3][bs:1];W[31hefe])";
    assert_eq!(result(source), Value::Null);
}
