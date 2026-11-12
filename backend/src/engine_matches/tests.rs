use super::dice;
use crate::{
    engine::Engine,
    engines::AnalysisEngine,
    play::engine_match::{self, GameDice},
};
use uuid::Uuid;

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL, GNUbg Python and locked BGTUTOR_SAGE_PYTHON"]
async fn mixed_duels_retain_checker_evaluations_for_both_sides(pool: sqlx::PgPool) {
    let rules = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()));
    rules.check_installation().await.unwrap();
    let sage = crate::engines::SageEngine::new(
        crate::engines::SageEngine::probe(std::env::var("BGTUTOR_SAGE_PYTHON").unwrap(), 2)
            .await
            .unwrap(),
    )
    .unwrap();
    let engines = [
        AnalysisEngine::Gnubg(rules.clone()),
        AnalysisEngine::OpenSage(sage),
    ];
    for sage_side in [1, 0] {
        let bots = if sage_side == 1 {
            [&engines[0], &engines[1]]
        } else {
            [&engines[1], &engines[0]]
        };
        let finished = engine_match::play(&rules, bots, [2, 2], 3, |game| {
            Ok(GameDice {
                opening: dice::opening(Uuid::nil(), game)?,
                turns: dice::turns(Uuid::nil(), game),
            })
        })
        .await
        .unwrap();
        let records = crate::imported_analysis::extract(&finished.sgf).unwrap();
        let (_, moves) = crate::import::sgf::parse_sgf_source_moves(&finished.sgf).unwrap();
        let sage_moves: Vec<_> = moves
            .iter()
            // GNUbg can automatically record a blocked roll without asking
            // either playing engine. No evaluation exists for those passes.
            .filter(|m| m.kind == "checker" && m.player == sage_side && m.board != m.after_board)
            .collect();
        assert!(!sage_moves.is_empty());
        for m in sage_moves {
            let record = records
                .iter()
                .find(|r| {
                    r.kind == "checker" && r.game_no == m.game_no && r.source_move_no == m.move_no
                })
                .unwrap_or_else(|| panic!("Open Sage checker evaluation was discarded: {m:?}; retained {} checker records", records.iter().filter(|r| r.kind == "checker" && r.engine == "Open Sage").count()));
            assert_eq!(record.engine, "Open Sage");
            assert_eq!(record.depth_plies, Some(2));
            assert_eq!(record.position_id.as_deref(), Some(m.position_id.as_str()));
            assert_eq!(record.gnubg_match_id.as_deref(), Some(m.match_id.as_str()));
            assert!(!record.candidates.is_empty());
            assert!(
                record
                    .candidates
                    .iter()
                    .all(|c| c.equity.is_some() && c.equity_kind == "cubeful")
            );
        }
        assert!(
            records.iter().any(|r| r.engine == "GNU Backgammon"
                && r.kind == "checker"
                && r.player != sage_side)
        );
        assert!(
            finished.games > 1,
            "Exercise retained evaluations across games"
        );
        let imported = crate::import::parse_sgf(&finished.sgf).unwrap();
        let stored = crate::store::import_match(
            &pool,
            "mixed-duel.sgf",
            "sgf",
            finished.sgf.as_bytes(),
            &finished.sgf,
            &imported,
        )
        .await
        .unwrap();
        let expected_count = records.iter().filter(|r| r.engine == "Open Sage").count() as i64;
        let stored_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM imported_analyses WHERE match_uuid=$1 AND data->>'engine'='Open Sage'",
        )
        .bind(stored.r#match.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stored_count, expected_count);
        let position: Uuid = sqlx::query_scalar("SELECT position_uuid FROM imported_analyses WHERE match_uuid=$1 AND data->>'engine'='Open Sage' ORDER BY game_no,source_move_no LIMIT 1")
            .bind(stored.r#match.id).fetch_one(&pool).await.unwrap();
        let router = crate::api::router(crate::api::AppState::new(pool.clone(), rules.clone()));
        let response = router
            .oneshot(
                Request::get(format!("/api/positions/{position}/imported-analysis"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = read_json(response).await;
        let sage = body
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["engine"] == "Open Sage")
            .unwrap();
        assert_eq!(sage["depth_plies"], 2);
        assert!(sage["candidates"][0]["equity"].is_number());
    }
}

#[tokio::test]
#[ignore = "requires local GNUbg with embedded Python"]
async fn identical_engines_split_a_pair_with_multiple_games() {
    let rules = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()));
    rules.check_installation().await.unwrap();
    let engine_a = AnalysisEngine::Gnubg(rules.clone());
    let engine_b = AnalysisEngine::Gnubg(rules.clone());
    let seed = Uuid::from_u128(13);
    assert_eq!(
        dice::opening(seed, 1).unwrap(),
        [[1, 1], [2, 2], [4, 4], [6, 3]]
    );
    let schedule = |game| {
        Ok(GameDice {
            opening: dice::opening(seed, game)?,
            turns: dice::turns(seed, game),
        })
    };
    let first = engine_match::play(&rules, [&engine_a, &engine_b], [0, 0], 5, schedule)
        .await
        .unwrap();
    let second = engine_match::play(&rules, [&engine_b, &engine_a], [0, 0], 5, schedule)
        .await
        .unwrap();
    assert!(first.games > 1);
    assert_eq!(first.scores, second.scores);
    assert_eq!(first.games, second.games);
    let archived = crate::import::parse_sgf(&first.sgf).unwrap();
    assert_eq!(archived.games.len(), first.games as usize);
    let metadata = crate::databases::source_metadata(&first.sgf);
    assert_eq!(metadata.result_state, "completed");
    assert_eq!(metadata.white_score, Some(first.scores[0]));
    assert_eq!(metadata.black_score, Some(first.scores[1]));
    let first = crate::import::sgf::parse_sgf_source_moves(&first.sgf)
        .unwrap()
        .1;
    let second = crate::import::sgf::parse_sgf_source_moves(&second.sgf)
        .unwrap()
        .1;
    assert!(first.iter().any(|turn| turn.kind == "cube"));
    assert_eq!(
        first
            .iter()
            .map(|turn| (&turn.notation, turn.dice, turn.player))
            .collect::<Vec<_>>(),
        second
            .iter()
            .map(|turn| (&turn.notation, turn.dice, turn.player))
            .collect::<Vec<_>>()
    );
}

use super::{Competitor, DiceMode, MatchResult, SeriesConfig, runner::match_plan, summarize};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[test]
fn seeds_follow_board_sides_and_pair_statistics_exclude_the_unpaired_match() {
    let profile = Competitor {
        engine_id: "gnubg".into(),
        plies: 0,
    };
    let mut config = SeriesConfig {
        a: profile.clone(),
        b: profile,
        mode: DiceMode::default(),
        match_length: 5,
        count: 20,
        seed: Uuid::nil(),
    };
    let first = match_plan(&config, 0);
    let second = match_plan(&config, 1);
    assert!(!first.swapped);
    assert!(second.swapped);
    assert_eq!(first.seed, second.seed);
    assert_ne!(first.seed, match_plan(&config, 2).seed);
    let mut results = (0..41)
        .map(|index| MatchResult {
            match_index: index,
            seed: match_plan(&config, index).seed,
            score_a: if index % 2 == 0 { 5 } else { 3 },
            score_b: if index % 2 == 0 { 3 } else { 5 },
            games: 5,
        })
        .collect::<Vec<_>>();
    let stats = summarize(config.mode, &results);
    assert_eq!(stats.complete_pairs, 20);
    assert_eq!((stats.wins_a, stats.wins_b), (20, 20));
    assert_eq!(stats.win_rate_a, Some(0.5));
    let [lower, upper] = stats.confidence_interval_a.unwrap();
    assert!(lower < 0.5 && upper > 0.5);
    results.truncate(1);
    assert_eq!(summarize(config.mode, &results).win_rate_a, None);
    config.mode = DiceMode::Random;
    assert_ne!(match_plan(&config, 0).seed, match_plan(&config, 1).seed);
    assert!(!match_plan(&config, 1).swapped);
    assert_eq!(summarize(config.mode, &results).win_rate_a, Some(1.0));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL and local GNUbg with embedded Python"]
async fn series_api_persists_results_recovers_expired_claim_and_cancels(pool: sqlx::PgPool) {
    let rules = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()));
    let router = crate::api::router(crate::api::AppState::new(pool.clone(), rules));
    let create = || {
        Request::post("/api/engine-matches").header("content-type", "application/json")
        .body(Body::from(r#"{"a":{"engine_id":"gnubg","plies":0},"b":{"engine_id":"gnubg","plies":0},"match_length":1,"count":1,"seed":"00000000-0000-0000-0000-000000000000"}"#)).unwrap()
    };
    let response = router.clone().oneshot(create()).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = read_json(response).await;
    assert_eq!(body["configuration"]["mode"], "paired");
    assert_eq!(body["total_matches"], 2);
    let id = body["id"].as_str().unwrap();
    sqlx::query("UPDATE engine_match_runs SET status='running',lease_until=now()-interval '1 second',lease_token=$2 WHERE id=$1")
        .bind(Uuid::parse_str(id).unwrap()).bind(Uuid::new_v4()).execute(&pool).await.unwrap();

    assert!(super::runner::run_one(&pool).await.unwrap());
    let detail = Request::get(format!("/api/engine-matches/{id}"))
        .body(Body::empty())
        .unwrap();
    let body = read_json(router.clone().oneshot(detail).await.unwrap()).await;
    assert_eq!(body["series"]["status"], "completed", "{body}");
    assert_eq!(body["series"]["completed_matches"], 2);
    assert_eq!(body["statistics"]["wins_a"], 1);
    assert_eq!(body["statistics"]["wins_b"], 1);
    assert_eq!(body["matches"][0]["seed"], body["matches"][1]["seed"]);
    let sgf = Request::get(format!("/api/engine-matches/{id}/matches/1/sgf"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router.clone().oneshot(sgf).await.unwrap().status(),
        StatusCode::OK
    );

    let databases = read_json(
        router
            .clone()
            .oneshot(Request::get("/api/databases").body(Body::empty()).unwrap())
            .await
            .unwrap(),
    )
    .await;
    let duels = databases
        .as_array()
        .unwrap()
        .iter()
        .find(|database| database["name"] == "Engine Duels")
        .unwrap();
    assert_eq!(duels["is_system"], true);
    assert_eq!(duels["match_count"], 2);
    let library = Request::get(format!(
        "/api/library/matches?database_id={}",
        duels["id"].as_str().unwrap()
    ))
    .body(Body::empty())
    .unwrap();
    let library = read_json(router.clone().oneshot(library).await.unwrap()).await;
    let matches = library["items"].as_array().unwrap();
    assert_eq!(matches.len(), 2);
    assert_ne!(matches[0]["id"], matches[1]["id"]);
    assert_ne!(matches[0]["white"], matches[1]["white"]);
    for result in body["matches"].as_array().unwrap() {
        let (player, white, black) = if result["match_index"] == 1 {
            ("B gnubg 0-ply", "score_b", "score_a")
        } else {
            ("A gnubg 0-ply", "score_a", "score_b")
        };
        let match_entry = matches
            .iter()
            .find(|entry| entry["white"] == player)
            .unwrap();
        assert_eq!(match_entry["white_score"], result[white]);
        assert_eq!(match_entry["black_score"], result[black]);
        assert_eq!(match_entry["result_state"], "completed");
        assert_eq!(match_entry["game_count"], result["games"]);
        assert!(match_entry["position_count"].as_u64().unwrap() > 0);
    }

    let second = read_json(router.clone().oneshot(create()).await.unwrap()).await;
    let id = second["id"].as_str().unwrap();
    let cancel = Request::post(format!("/api/engine-matches/{id}/cancel"))
        .body(Body::empty())
        .unwrap();
    let body = read_json(router.clone().oneshot(cancel).await.unwrap()).await;
    assert_eq!(body["status"], "cancelled");
    assert!(!super::runner::run_one(&pool).await.unwrap());

    sqlx::query(
        "INSERT INTO engine_match_runs(id,configuration,engines,dice_version,status,total_matches,created_at)
         SELECT gen_random_uuid(),configuration,engines,dice_version,'cancelled',total_matches,created_at-interval '1 day'
         FROM engine_match_runs CROSS JOIN generate_series(1,51) WHERE id=$1",
    )
    .bind(Uuid::parse_str(id).unwrap())
    .execute(&pool).await.unwrap();
    let page = |url: &str| Request::get(url).body(Body::empty()).unwrap();
    let first = read_json(
        router
            .clone()
            .oneshot(page("/api/engine-matches"))
            .await
            .unwrap(),
    )
    .await;
    let cursor = first["next_cursor"].as_str().unwrap();
    assert_eq!(first["runs"].as_array().unwrap().len(), 50);
    let second = read_json(
        router
            .oneshot(page(&format!("/api/engine-matches?before={cursor}")))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(second["runs"].as_array().unwrap().len(), 3);
    assert!(
        second["runs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|run| run["id"] != cursor)
    );
    assert!(second["next_cursor"].is_null());
}

async fn read_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
#[ignore = "requires local GNUbg and BGTUTOR_SAGE_PYTHON"]
async fn open_sage_and_gnubg_complete_both_legs() {
    let python = std::env::var("BGTUTOR_SAGE_PYTHON").unwrap();
    let sage = crate::engines::SageEngine::probe(python, 2).await.unwrap();
    let sage = AnalysisEngine::OpenSage(crate::engines::SageEngine::new(sage).unwrap());
    let rules = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()));
    let gnubg = AnalysisEngine::Gnubg(rules.clone());
    let seed = Uuid::nil();
    let schedule = |game| {
        Ok(GameDice {
            opening: dice::opening(seed, game)?,
            turns: dice::turns(seed, game),
        })
    };
    let first = engine_match::play(&rules, [&gnubg, &sage], [0, 2], 1, schedule)
        .await
        .unwrap();
    let second = engine_match::play(&rules, [&sage, &gnubg], [2, 0], 1, schedule)
        .await
        .unwrap();
    assert_eq!(first.games, 1);
    assert_eq!(second.games, 1);
    let first = crate::import::sgf::parse_sgf_source_moves(&first.sgf)
        .unwrap()
        .1;
    let second = crate::import::sgf::parse_sgf_source_moves(&second.sgf)
        .unwrap()
        .1;
    for (a, b) in first.iter().zip(&second) {
        assert_eq!(a.player, b.player);
        assert_eq!(a.dice, b.dice);
    }
}
