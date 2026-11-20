use super::dice;
use crate::{
    engine::Engine,
    engines::AnalysisEngine,
    play::engine_match::{self, GameDice},
};
use uuid::Uuid;

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL, GNUbg Python and locked NEONDOJO_SAGE_PYTHON"]
async fn mixed_duels_retain_checker_evaluations_for_both_sides(pool: sqlx::PgPool) {
    let rules = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()));
    rules.check_installation().await.unwrap();
    let sage = crate::engines::SageEngine::new(
        crate::engines::SageEngine::probe(std::env::var("NEONDOJO_SAGE_PYTHON").unwrap(), 2)
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
        let finished = engine_match::play(
            &rules,
            bots,
            [2, 2],
            DuelUnit::Match { length: 3 },
            |game| {
                Ok(GameDice {
                    opening: dice::opening(Uuid::nil(), game)?,
                    turns: dice::turns(Uuid::nil(), game),
                })
            },
        )
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
    let first = engine_match::play(
        &rules,
        [&engine_a, &engine_b],
        [1, 1],
        DuelUnit::Match { length: 5 },
        schedule,
    )
    .await
    .unwrap();
    let second = engine_match::play(
        &rules,
        [&engine_b, &engine_a],
        [1, 1],
        DuelUnit::Match { length: 5 },
        schedule,
    )
    .await
    .unwrap();
    assert!(first.games > 1);
    assert_eq!(first.scores, second.scores);
    assert_eq!(first.games, second.games);
    let measured = first
        .costs
        .map(|side| side.expect("GNUbg reports its engine cost"));
    let checker_turns = crate::import::sgf::parse_sgf_source_moves(&first.sgf)
        .unwrap()
        .1
        .iter()
        .filter(|turn| turn.kind == "checker")
        .count();
    assert_eq!(
        measured
            .iter()
            .map(|side| side.moves as usize)
            .sum::<usize>(),
        checker_turns
    );
    assert!(
        measured
            .iter()
            .all(|side| side.cost.cpu_us > 0 && side.cost.wall_us > 0)
    );
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

#[tokio::test]
#[ignore = "requires local GNUbg with embedded Python"]
async fn cubeless_money_block_plays_independent_games_without_cube_or_jacoby() {
    let rules = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()));
    rules.check_installation().await.unwrap();
    let gnubg = AnalysisEngine::Gnubg(rules.clone());
    let seed = Uuid::from_u128(29);
    let schedule = |game| {
        Ok(GameDice {
            opening: dice::opening(seed, game)?,
            turns: dice::turns(seed, game),
        })
    };
    let unit = DuelUnit::MoneyCubeless { games: 12 };
    let first = engine_match::play(&rules, [&gnubg, &gnubg], [1, 1], unit, schedule)
        .await
        .unwrap();
    let second = engine_match::play(&rules, [&gnubg, &gnubg], [1, 1], unit, schedule)
        .await
        .unwrap();
    assert_eq!(first.games, 12);
    let outcomes = first.outcomes.unwrap();
    assert_eq!(outcomes.iter().flatten().sum::<i32>(), 12);
    assert_eq!(
        first.scores,
        outcomes.map(|side| side[0] + 2 * side[1] + 3 * side[2])
    );
    // Deterministic engines replay the same dice identically.
    assert_eq!(
        (first.scores, first.sgf.len()),
        (second.scores, second.sgf.len())
    );
    assert!(first.sgf.contains("RU[NoCube]"), "{}", &first.sgf[..300]);
    assert!(!first.sgf.contains("Jacoby"), "gammons must count");
    let turns = crate::import::sgf::parse_sgf_source_moves(&first.sgf)
        .unwrap()
        .1;
    assert!(turns.iter().all(|turn| turn.kind == "checker"));
    let archived = crate::import::parse_sgf(&first.sgf).unwrap();
    assert_eq!(archived.games.len(), 12);
    assert!(first.costs.iter().all(Option::is_some));
}

#[tokio::test]
#[ignore = "requires local GNUbg and NEONDOJO_SAGE_PYTHON"]
async fn cubeless_open_sage_plays_a_money_block_against_gnubg() {
    let python = std::env::var("NEONDOJO_SAGE_PYTHON").unwrap();
    let mut profile = crate::engines::SageSnapshot {
        cubeless: true,
        ..crate::engines::SageEngine::probe(python, 2).await.unwrap()
    };
    profile.plies = 2;
    let sage = crate::engines::SageEngine::new(profile).unwrap();
    assert!(sage.version().await.unwrap().contains("2ply cubeless"));
    let sage = AnalysisEngine::OpenSage(sage);
    let rules = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()));
    let gnubg = AnalysisEngine::Gnubg(rules.clone());
    let seed = Uuid::from_u128(31);
    let schedule = |game| {
        Ok(GameDice {
            opening: dice::opening(seed, game)?,
            turns: dice::turns(seed, game),
        })
    };
    let unit = DuelUnit::MoneyCubeless { games: 3 };
    for bots in [[&gnubg, &sage], [&sage, &gnubg]] {
        let finished = engine_match::play(&rules, bots, [1, 2], unit, schedule)
            .await
            .unwrap();
        assert_eq!(finished.outcomes.unwrap().iter().flatten().sum::<i32>(), 3);
        let turns = crate::import::sgf::parse_sgf_source_moves(&finished.sgf)
            .unwrap()
            .1;
        assert!(turns.iter().all(|turn| turn.kind == "checker"));
        assert!(finished.costs.iter().all(Option::is_some));
    }
}

use super::{
    Competitor, DiceMode, DuelFormat, GameOutcomes, MatchCost, MatchResult, SeriesConfig,
    SideCostSummary, runner::match_plan, student_t_975, summarize,
};
use crate::play::engine_match::DuelUnit;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[test]
fn seeds_follow_board_sides_and_pair_statistics_exclude_the_unpaired_match() {
    let profile = Competitor {
        engine_id: "gnubg".into(),
        plies: 1,
    };
    let mut config = SeriesConfig {
        a: profile.clone(),
        b: profile,
        format: DuelFormat::Match,
        block_games: None,
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
            cost: Default::default(),
            outcomes: None,
        })
        .collect::<Vec<_>>();
    let stats = summarize(DuelFormat::Match, config.mode, &results);
    assert_eq!(stats.complete_pairs, 20);
    assert_eq!((stats.wins_a, stats.wins_b), (20, 20));
    assert_eq!(stats.win_rate_a, Some(0.5));
    // Every pair splits 1–1, so the pair means have no variance.
    assert_eq!(stats.confidence_interval_a, Some([0.5, 0.5]));
    results.truncate(1);
    assert_eq!(
        summarize(DuelFormat::Match, config.mode, &results).win_rate_a,
        None
    );
    config.mode = DiceMode::Random;
    assert_ne!(match_plan(&config, 0).seed, match_plan(&config, 1).seed);
    assert!(!match_plan(&config, 1).swapped);
    assert_eq!(
        summarize(DuelFormat::Match, config.mode, &results).win_rate_a,
        Some(1.0)
    );
}

/// Pairs as (A wins both legs, B wins both legs, split 1–1).
fn paired_results(a_both: usize, b_both: usize, split: usize) -> Vec<MatchResult> {
    let outcomes = std::iter::repeat_n([true, true], a_both)
        .chain(std::iter::repeat_n([false, false], b_both))
        .chain(std::iter::repeat_n([true, false], split));
    outcomes
        .flatten()
        .enumerate()
        .map(|(index, a_wins)| MatchResult {
            match_index: index as i32,
            seed: Uuid::nil(),
            score_a: if a_wins { 5 } else { 2 },
            score_b: if a_wins { 2 } else { 5 },
            games: 3,
            cost: Default::default(),
            outcomes: None,
        })
        .collect()
}

fn assert_interval(results: &[MatchResult], expected: [f64; 2]) {
    let [lower, upper] = summarize(DuelFormat::Match, DiceMode::Paired, results)
        .confidence_interval_a
        .unwrap();
    assert!(
        (lower - expected[0]).abs() < 1e-4,
        "lower {lower} vs {}",
        expected[0]
    );
    assert!(
        (upper - expected[1]).abs() < 1e-4,
        "upper {upper} vs {}",
        expected[1]
    );
}

#[test]
fn student_t_quantile_matches_reference_values() {
    // scipy.stats.t.ppf(0.975, df)
    for (df, expected) in [
        (19, 2.093_024_054_408_308_7),
        (20, 2.085_963_447_265_864_4),
        (50, 2.008_559_112_100_760_7),
        (99, 1.984_216_951_586_417_2),
        (999, 1.962_341_461_133_449_5),
    ] {
        assert!((student_t_975(df) - expected).abs() < 1e-4, "df {df}");
    }
}

#[test]
fn pair_interval_uses_the_empirical_variance_of_pair_means() {
    // Reference: mean ± t(0.975, n−1) · s / √n with scipy, s the sample
    // standard deviation of the pair means.
    // 100 pairs: 23 won 2–0 by A, 16 by B, 61 split (a real GNUbg/Open Sage series).
    assert_interval(&paired_results(23, 16, 61), [0.473_123_1, 0.596_876_9]);
    assert_interval(&paired_results(8, 2, 10), [0.496_269_7, 0.803_730_3]);
    // Bounds stay within [0, 1].
    assert_interval(&paired_results(18, 0, 2), [0.877_974_1, 1.0]);
    assert_interval(&paired_results(20, 0, 0), [1.0, 1.0]);
    // Below 20 complete pairs no interval is shown.
    assert_eq!(
        summarize(
            DuelFormat::Match,
            DiceMode::Paired,
            &paired_results(10, 0, 9)
        )
        .confidence_interval_a,
        None
    );
}

#[test]
fn engine_cost_is_summed_per_side_over_measured_counted_matches() {
    let mut results = paired_results(1, 1, 1);
    // Legacy matches without a measurement stay out of the cost summary.
    results[0].cost = MatchCost {
        cpu_us_a: Some(3_000_000),
        wall_us_a: Some(1_000_000),
        moves_a: Some(20),
        cpu_us_b: Some(500_000),
        wall_us_b: Some(400_000),
        moves_b: Some(25),
    };
    results[1].cost = MatchCost {
        cpu_us_a: Some(1_000_000),
        wall_us_a: Some(1_000_000),
        moves_a: Some(20),
        ..Default::default()
    };
    // The unpaired last match is not counted in paired mode.
    results.push(MatchResult {
        match_index: 6,
        seed: Uuid::nil(),
        score_a: 5,
        score_b: 0,
        games: 1,
        cost: MatchCost {
            cpu_us_a: Some(9_000_000_000),
            wall_us_a: Some(9_000_000_000),
            moves_a: Some(1),
            ..Default::default()
        },
        outcomes: None,
    });
    let stats = summarize(DuelFormat::Match, DiceMode::Paired, &results);
    assert_eq!(
        stats.cost_a,
        Some(SideCostSummary {
            measured_matches: 2,
            moves: 40,
            cpu_ms_per_move: 100.0,
            wall_ms_per_move: 50.0,
        })
    );
    assert_eq!(
        stats.cost_b,
        Some(SideCostSummary {
            measured_matches: 1,
            moves: 25,
            cpu_ms_per_move: 20.0,
            wall_ms_per_move: 16.0,
        })
    );
    assert_eq!(
        summarize(
            DuelFormat::Match,
            DiceMode::Paired,
            &paired_results(1, 0, 0)
        )
        .cost_a,
        None
    );
}

/// Two paired cubeless money blocks of 10 games each per pair index.
fn money_pairs(pairs: i32) -> Vec<MatchResult> {
    let points = |side: [i32; 3]| side[0] + 2 * side[1] + 3 * side[2];
    (0..pairs)
        .flat_map(|i| {
            let a1 = [4 + i % 3, i32::from(i % 5 == 0), i32::from(i % 7 == 0)];
            let gammon_b1 = i32::from(i % 2 == 0);
            let b1 = [10 - a1.iter().sum::<i32>() - gammon_b1, gammon_b1, 0];
            let a2 = [5 - i % 2, 0, 0];
            let (gammon_b2, backgammon_b2) = (i32::from(i % 3 == 0), i32::from(i % 4 == 0));
            let b2 = [
                10 - a2.iter().sum::<i32>() - gammon_b2 - backgammon_b2,
                gammon_b2,
                backgammon_b2,
            ];
            [(a1, b1), (a2, b2)]
        })
        .enumerate()
        .map(|(index, (a, b))| MatchResult {
            match_index: index as i32,
            seed: Uuid::nil(),
            score_a: points(a),
            score_b: points(b),
            games: 10,
            cost: Default::default(),
            outcomes: Some([a, b].concat()),
        })
        .collect()
}

#[test]
fn money_blocks_report_points_per_game_with_a_pair_interval() {
    // Reference: scipy t interval over the 20 pair means of net points per game.
    let stats = summarize(
        DuelFormat::MoneyCubeless,
        DiceMode::Paired,
        &money_pairs(20),
    );
    let money = stats.money.unwrap();
    assert_eq!(money.games, 400);
    assert_eq!(money.points_per_game_a, Some(-0.0625));
    let [lower, upper] = money.confidence_interval.unwrap();
    assert!((lower - -0.131_616_095_196_110_9).abs() < 1e-4, "{lower}");
    assert!((upper - 0.006_616_095_196_110_88).abs() < 1e-4, "{upper}");
    assert_eq!(
        (money.outcomes_a, money.outcomes_b),
        (
            GameOutcomes {
                single: 189,
                gammon: 4,
                backgammon: 3
            },
            GameOutcomes {
                single: 182,
                gammon: 17,
                backgammon: 5
            },
        )
    );
    // Games won replace match wins; the match win-rate interval does not apply.
    assert_eq!((stats.wins_a, stats.wins_b), (196, 204));
    assert_eq!(stats.win_rate_a, Some(0.49));
    assert_eq!(stats.confidence_interval_a, None);
    // Without enough pairs there is no interval; an unpaired leg is not counted.
    let mut few = money_pairs(19);
    few.truncate(37);
    let money = summarize(DuelFormat::MoneyCubeless, DiceMode::Paired, &few)
        .money
        .unwrap();
    assert_eq!(money.games, 360);
    assert_eq!(money.confidence_interval, None);
    // Without pairing every block is an observation.
    let random = summarize(
        DuelFormat::MoneyCubeless,
        DiceMode::Random,
        &money_pairs(10),
    );
    assert!(random.money.unwrap().confidence_interval.is_some());
}

#[test]
fn saved_configuration_selects_a_validated_duel_unit() {
    let legacy: SeriesConfig = serde_json::from_value(serde_json::json!({
        "a": {"engine_id": "gnubg", "plies": 2},
        "b": {"engine_id": "gnubg", "plies": 2},
        "match_length": 7, "count": 3, "mode": "paired", "seed": Uuid::nil(),
    }))
    .unwrap();
    assert_eq!(legacy.format, DuelFormat::Match);
    assert_eq!(legacy.unit().unwrap(), DuelUnit::Match { length: 7 });
    let mut money = legacy.clone();
    money.format = DuelFormat::MoneyCubeless;
    money.match_length = 0;
    money.block_games = Some(1000);
    assert_eq!(
        money.unit().unwrap(),
        DuelUnit::MoneyCubeless { games: 1000 }
    );
    for (length, games) in [(5, Some(100)), (0, None), (0, Some(0)), (0, Some(1001))] {
        money.match_length = length;
        money.block_games = games;
        assert!(money.unit().is_err(), "{length} {games:?}");
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL and local GNUbg with embedded Python"]
async fn cubeless_money_series_stores_blocks_and_points_per_game(pool: sqlx::PgPool) {
    let rules = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()));
    let router = crate::api::router(crate::api::AppState::new(pool.clone(), rules));
    let create = |body: serde_json::Value| {
        Request::post("/api/engine-matches")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let side = serde_json::json!({"engine_id": "gnubg", "plies": 1});
    for invalid in [
        serde_json::json!({"format": "money_cubeless", "count": 1}),
        serde_json::json!({"format": "money_cubeless", "block_games": 1001, "count": 1}),
        serde_json::json!({"format": "money_cubeless", "block_games": 4, "match_length": 5, "count": 1}),
        serde_json::json!({"format": "match", "block_games": 4, "match_length": 5, "count": 1}),
    ] {
        let mut body = invalid.clone();
        body["a"] = side.clone();
        body["b"] = side.clone();
        let response = router.clone().oneshot(create(body)).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{invalid}"
        );
    }
    let response = router
        .clone()
        .oneshot(create(serde_json::json!({
            "a": side, "b": side, "format": "money_cubeless", "block_games": 4,
            "count": 1, "seed": Uuid::from_u128(5),
        })))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = read_json(response).await;
    assert_eq!(body["configuration"]["block_games"], 4);
    assert_eq!(body["configuration"]["match_length"], 0);
    let id = body["id"].as_str().unwrap().to_owned();
    while super::runner::run_one(&pool).await.unwrap() {}
    let detail = Request::get(format!("/api/engine-matches/{id}"))
        .body(Body::empty())
        .unwrap();
    let body = read_json(router.clone().oneshot(detail).await.unwrap()).await;
    assert_eq!(body["series"]["status"], "completed", "{body}");
    let statistics = &body["statistics"];
    assert_eq!(statistics["money"]["games"], 8, "{body}");
    // Identical deterministic engines with swapped sides cancel out exactly.
    assert_eq!(statistics["money"]["points_per_game_a"], 0.0);
    assert_eq!(statistics["confidence_interval_a"], serde_json::Value::Null);
    assert_eq!(
        statistics["wins_a"].as_i64().unwrap() + statistics["wins_b"].as_i64().unwrap(),
        8
    );
    assert_eq!(body["matches"][0]["games"], 4);
    let library = Request::get(format!(
        "/api/library/matches?database_id={}",
        super::ENGINE_DUELS_DATABASE_ID
    ))
    .body(Body::empty())
    .unwrap();
    let library = read_json(router.clone().oneshot(library).await.unwrap()).await;
    let archived = library["items"].as_array().unwrap();
    assert_eq!(archived.len(), 2, "{library}");
    for entry in archived {
        assert_eq!(entry["game_count"], 4, "{entry}");
        // A money session has no final match result.
        assert_eq!(entry["result_state"], "unknown", "{entry}");
        assert_eq!(entry["match_length"], 0, "{entry}");
    }
    let sgf: String = sqlx::query_scalar(
        "SELECT sgf FROM engine_match_results WHERE run_id=$1 AND match_index=0",
    )
    .bind(Uuid::parse_str(&id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(sgf.contains("EV[Duel GNUbg 1-ply vs. GNUbg 1-ply (money, cubeless)]"));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL and local GNUbg with embedded Python"]
async fn series_api_persists_results_recovers_expired_claim_and_cancels(pool: sqlx::PgPool) {
    let rules = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()));
    let router = crate::api::router(crate::api::AppState::new(pool.clone(), rules));
    let create = || {
        Request::post("/api/engine-matches").header("content-type", "application/json")
        .body(Body::from(r#"{"a":{"engine_id":"gnubg","plies":1},"b":{"engine_id":"gnubg","plies":1},"match_length":1,"count":1,"seed":"00000000-0000-0000-0000-000000000000"}"#)).unwrap()
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
    for side in ["cost_a", "cost_b"] {
        let cost = &body["statistics"][side];
        assert_eq!(cost["measured_matches"], 2, "{body}");
        assert!(cost["moves"].as_i64().unwrap() > 0);
        assert!(cost["cpu_ms_per_move"].as_f64().unwrap() > 0.0);
        assert!(cost["wall_ms_per_move"].as_f64().unwrap() > 0.0);
    }
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
            ("B gnubg 1-ply", "score_b", "score_a")
        } else {
            ("A gnubg 1-ply", "score_a", "score_b")
        };
        let match_entry = matches
            .iter()
            .find(|entry| entry["white"] == player)
            .unwrap();
        assert_eq!(match_entry["white_score"], result[white]);
        assert_eq!(match_entry["black_score"], result[black]);
        assert_eq!(
            match_entry["tournament"],
            "Duel GNUbg 1-ply vs. GNUbg 1-ply"
        );
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
#[ignore = "requires local GNUbg and NEONDOJO_SAGE_PYTHON"]
async fn open_sage_and_gnubg_complete_both_legs() {
    let python = std::env::var("NEONDOJO_SAGE_PYTHON").unwrap();
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
    let first = engine_match::play(
        &rules,
        [&gnubg, &sage],
        [1, 2],
        DuelUnit::Match { length: 1 },
        schedule,
    )
    .await
    .unwrap();
    let second = engine_match::play(
        &rules,
        [&sage, &gnubg],
        [2, 1],
        DuelUnit::Match { length: 1 },
        schedule,
    )
    .await
    .unwrap();
    assert_eq!(first.games, 1);
    assert_eq!(second.games, 1);
    // Both the native GNUbg side and the external Open Sage side are measured.
    for side in first.costs.iter().chain(&second.costs) {
        let side = side.expect("each engine reports its cost");
        assert!(side.moves > 0 && side.cost.cpu_us > 0 && side.cost.wall_us > 0);
    }
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

#[tokio::test]
#[ignore = "requires local GNUbg and NEONDOJO_SAGE_PYTHON"]
async fn mixed_duel_preserves_legal_moves_for_retained_seed() {
    let python = std::env::var("NEONDOJO_SAGE_PYTHON").unwrap();
    let sage = crate::engines::SageEngine::probe_profile(python, 3, 10)
        .await
        .unwrap();
    let sage = AnalysisEngine::OpenSage(crate::engines::SageEngine::new(sage).unwrap());
    let rules = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()))
        .with_threads(Some(10));
    let gnubg = AnalysisEngine::Gnubg(rules.clone());
    let config = SeriesConfig {
        a: Competitor {
            engine_id: "gnubg".into(),
            plies: 3,
        },
        b: Competitor {
            engine_id: "bgsage".into(),
            plies: 3,
        },
        format: DuelFormat::Match,
        match_length: 5,
        block_games: None,
        count: 100,
        mode: DiceMode::Paired,
        seed: Uuid::parse_str("8c0a1bb0-90c3-444b-871a-492bcaca83ff").unwrap(),
    };
    let seed = match_plan(&config, 64).seed;
    engine_match::play(
        &rules,
        [&gnubg, &sage],
        [3, 3],
        DuelUnit::Match { length: 5 },
        |game| {
            Ok(GameDice {
                opening: dice::opening(seed, game)?,
                turns: dice::turns(seed, game),
            })
        },
    )
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL"]
async fn library_match_finds_moved_entries_and_restores_deleted_ones(pool: sqlx::PgPool) {
    let router = crate::api::router(crate::api::AppState::new(
        pool.clone(),
        Engine::new("gnubg".into()),
    ));
    let run = Uuid::new_v4();
    let sgf = include_str!("../../tests/fixtures/game-results-match.sgf");
    sqlx::query("INSERT INTO engine_match_runs(id,configuration,engines,dice_version,status,total_matches,completed_matches) VALUES($1,'{}','{}','test','completed',1,1)")
        .bind(run).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO engine_match_results(run_id,match_index,seed,score_a,score_b,games,sgf) VALUES($1,0,$2,7,3,2,$3)")
        .bind(run).bind(Uuid::nil()).bind(sgf).execute(&pool).await.unwrap();
    let open = |index: i32| {
        let router = router.clone();
        async move {
            router
                .oneshot(
                    Request::post(format!("/api/engine-matches/{run}/matches/{index}/library"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
        }
    };
    let library_entries = || async {
        sqlx::query_as::<_, (Uuid, Uuid)>(
            "SELECT id,database_id FROM matches ORDER BY created_at,id",
        )
        .fetch_all(&pool)
        .await
        .unwrap()
    };

    // A result archived before automatic saving gets its Engine Duels entry on first open.
    let response = open(0).await;
    assert_eq!(response.status(), StatusCode::OK);
    let restored = read_json(response).await;
    assert_eq!(restored["filename"], format!("Engine duel {run} #1.sgf"));
    let id = Uuid::parse_str(restored["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        library_entries().await,
        [(id, super::ENGINE_DUELS_DATABASE_ID)]
    );

    // Repeated opens and moved entries reuse the existing match.
    assert_eq!(read_json(open(0).await).await["id"], restored["id"]);
    let default = crate::databases::default_database_id(&pool).await.unwrap();
    sqlx::query("UPDATE matches SET database_id=$2 WHERE id=$1")
        .bind(id)
        .bind(default)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(read_json(open(0).await).await["id"], restored["id"]);
    assert_eq!(library_entries().await, [(id, default)]);

    // A deleted library entry is restored under Engine Duels.
    sqlx::query("DELETE FROM matches WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let again = read_json(open(0).await).await;
    assert_ne!(again["id"], restored["id"]);
    assert_eq!(library_entries().await.len(), 1);
    assert_eq!(
        library_entries().await[0].1,
        super::ENGINE_DUELS_DATABASE_ID
    );

    assert_eq!(open(1).await.status(), StatusCode::NOT_FOUND);
}
