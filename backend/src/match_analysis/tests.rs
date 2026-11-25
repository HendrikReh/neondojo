use super::*;
use crate::import::parse_sgf;
const MATCH: &str = include_str!("../../tests/fixtures/WC2014.gnubg.sgf");

async fn run_native_game_analysis(
    engine: &Engine,
    directory: &std::path::Path,
    source: &str,
    game_no: i32,
    report_command: &str,
) -> String {
    tokio::fs::write(directory.join("input.sgf"), source)
        .await
        .unwrap();
    let mut commands = String::from(concat!(
        "set confirm default yes\nset display off\n",
        "set output mwc off\nset output digits 6\nset analysis luck off\n",
    ));
    for kind in ["chequerplay", "cubedecision"] {
        commands.push_str(&format!(
                "set analysis {kind} type evaluation\nset analysis {kind} evaluation plies 0\nset analysis {kind} evaluation cubeful on\nset analysis {kind} evaluation deterministic on\nset analysis {kind} evaluation noise 0\n"
            ));
    }
    commands.push_str("load match input.sgf\nfirst game\n");
    if game_no > 1 {
        commands.push_str(&format!("next game {}\n", game_no - 1));
    }
    commands.push_str("analyse clear game\nanalyse game\n");
    commands.push_str(report_command);
    engine
        .run_with_timeout(directory, &commands, std::time::Duration::from_secs(30))
        .await
        .unwrap()
}

#[test]
fn cube_responses_use_responder_equities_in_both_export_formats() {
    for (take_equity, best, take_loss, drop_loss) in [
        (-0.840856, "take", 0.0, 0.159144),
        (-1.25, "drop", 0.25, 0.0),
        (-1.0, "take", 0.0, 0.0),
    ] {
        for perspective in [-1.0, 1.0] {
            let block = format!(
                "1. No double {:+.6}\n2. Double, take {:+.6}\n3. Double, pass {:+.6}\n",
                -0.75 * perspective,
                take_equity * perspective,
                -perspective,
            );
            let take = cube_decision(&block, 0, "take").unwrap();
            let drop = cube_decision(&block, 0, "drop").unwrap();

            assert_eq!(take.best, best, "{block}");
            assert_eq!(drop.best, best, "{block}");
            assert_eq!(take.played_equity, Some(take_equity), "{block}");
            assert_eq!(drop.played_equity, Some(-1.0), "{block}");
            assert_eq!(take.best_equity, Some(take_equity.max(-1.0)), "{block}");
            assert_eq!(drop.best_equity, take.best_equity, "{block}");
            assert_eq!(take.equity_loss, take_loss, "{block}");
            assert_eq!(drop.equity_loss, drop_loss, "{block}");
            let outcomes = take.cube_evaluation.unwrap();
            assert_eq!(outcomes.double_take, take_equity, "{block}");
            assert_eq!(outcomes.double_pass, -1.0, "{block}");
            assert_eq!(outcomes.no_double, -0.75, "{block}");
        }
    }
}

#[test]
fn legacy_cube_checkpoint_keeps_known_equities_without_outcomes() {
    let mut data =
        serde_json::to_value(evaluated_cube_decision(0, "no double", 0.4, 0.6, 1.0).unwrap())
            .unwrap();
    data.as_object_mut().unwrap().remove("cube_evaluation");
    let decision: Decision = serde_json::from_value(data).unwrap();
    assert_eq!(decision.played_equity, Some(0.4));
    assert_eq!(decision.best_equity, Some(0.6));
    assert!(decision.cube_evaluation.is_none());
}

#[tokio::test]
#[ignore = "requires local GNUbg with weights"]
async fn native_performance_statistics_exclude_forced_moves_and_trivial_cube_choices() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    // Two legal opening moves; the second player's pre-roll cube choice is trivial.
    let source = "(;GM[6]MI[length:7];W[31xufe];B[42qusu])";
    let game = engine.analyze_game(source, 1, 1).await.unwrap();
    let value = serde_json::to_value(game).unwrap();
    assert_eq!(value["statistics"][0]["unforced_checker_moves"], 1);
    assert_eq!(value["statistics"][1]["unforced_checker_moves"], 1);
    assert_eq!(value["statistics"][1]["total_cube_decisions"], 1);
    assert_eq!(value["statistics"][1]["close_cube_decisions"], 0);
    // Independent CLI oracle: query native statistics directly, without
    // running our text-export parser or reconstructing losses from decisions.
    let temp = tempfile::tempdir().unwrap();
    let native =
        run_native_game_analysis(&engine, temp.path(), source, 1, "show statistics game\n").await;
    assert!(
        Regex::new(r"(?m)^Unforced moves\s+1\s+1\s*$")
            .unwrap()
            .is_match(&native)
    );
    assert!(
        Regex::new(r"(?m)^Close or actual cube decisions\s+0\s+0\s*$")
            .unwrap()
            .is_match(&native)
    );
    let overall = native.split_once("Overall statistics").unwrap().1;
    let row = overall
        .lines()
        .find(|line| line.starts_with("Error total EMG"))
        .unwrap();
    let oracle_white: f64 = row.split_whitespace().nth(4).unwrap().parse().unwrap();
    assert!(oracle_white < 0.0);
    assert_eq!(
        value["statistics"][0]["normalized_equity_loss"]
            .as_f64()
            .unwrap(),
        -oracle_white
    );
}

#[tokio::test]
#[ignore = "requires local GNUbg with weights"]
async fn native_performance_statistics_cover_money_games() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    // A money game: GNUbg reports its totals in points instead of MWC.
    let source = "(;GM[6]MI[length:0];W[31xufe];B[42qusu])";
    let game = engine.analyze_game(source, 1, 1).await.unwrap();
    let statistics = game.statistics.expect("money-game native statistics");
    assert_eq!(statistics[0].unforced_checker_moves, 1);
    assert_eq!(statistics[1].unforced_checker_moves, 1);
    // Independent CLI oracle, as for the point match above.
    let temp = tempfile::tempdir().unwrap();
    let native =
        run_native_game_analysis(&engine, temp.path(), source, 1, "show statistics game\n").await;
    let overall = native.split_once("Overall statistics").unwrap().1;
    let row = overall
        .lines()
        .find(|line| line.starts_with("Error total EMG"))
        .unwrap();
    assert!(row.starts_with("Error total EMG (Points)"), "{row}");
    let oracle_white: f64 = row.split_whitespace().nth(4).unwrap().parse().unwrap();
    assert!(oracle_white < 0.0);
    assert_eq!(statistics[0].normalized_equity_loss, -oracle_white);
}

#[tokio::test]
#[ignore = "requires local GNUbg with weights"]
async fn native_performance_statistics_cover_cube_only_and_forced_checker_games() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let source = "(;GM[6]MI[length:7];W[31hefe])\n(;GM[6]MI[length:7]AE[a:y]AW[f]AB[s]PL[W]RE[W+1];W[double];B[drop])";
    let cube = engine.analyze_game(source, 2, 1).await.unwrap();
    let native = cube
        .statistics
        .as_ref()
        .expect("cube-only native statistics");
    for player in native {
        assert_eq!(player.total_checker_moves, 0);
        assert_eq!(player.unforced_checker_moves, 0);
        assert_eq!(player.total_cube_decisions, 1);
        assert_eq!(player.close_cube_decisions, 1);
    }
    for source in [
        "(;GM[6]MI[length:7][ws:6][bs:0]RU[Crawford:CrawfordGame]AE[a:y]AW[f]AB[s]PL[W];W[66fz])",
        "(;GM[6]MI[length:7][ws:6][bs:0]RU[Crawford:CrawfordGame]AE[a:y]AW[y]AB[s][s]PL[W];W[66])",
    ] {
        let game = engine.analyze_game(source, 1, 1).await.unwrap();
        let native = game.statistics.as_ref().expect("forced native statistics");
        assert_eq!(native[0].total_checker_moves, 1);
        assert_eq!(native[0].unforced_checker_moves, 0);
        assert_eq!(native[0].normalized_equity_loss, 0.0);
    }
}

#[test]
fn native_performance_statistics_reject_missing_malformed_and_partial_exports() {
    let decisions = vec![decision(0, "checker", "played", "best", Some((0.0, 0.1)))];
    let export = "Game statistics for game 1\nChequerplay statistics\nTotal moves 1 0\nUnforced moves 1 0\nCube statistics\nTotal cube decisions 0 0\nClose or actual cube decisions 0 0\nOverall statistics\nError total EMG (MWC) -0.100000 ( -1.000%) -0.000000 ( -0.000%)\n";
    let native = parse_native_statistics(export, 1, &decisions, true).unwrap();
    assert_eq!(native[0].normalized_equity_loss, 0.1);
    for changed in [
        export.replace("Game statistics for game 1", "Game statistics for game 10"),
        export.replace("Unforced moves 1 0", "Unforced moves 2 0"),
        export.replace("Total moves 1 0", "Total moves 0 0"),
        export.replace("Close or actual cube decisions 0 0", ""),
        export.replace("-0.100000", "NaN"),
        export.replace("-0.100000", "+0.100000"),
        export.replace("Overall statistics", "Unknown statistics"),
        format!("{export}{export}"),
    ] {
        assert!(
            parse_native_statistics(&changed, 1, &decisions, true).is_none(),
            "{changed}"
        );
    }
    assert!(parse_native_statistics(export, 2, &decisions, true).is_none());
    let mut unavailable = decisions;
    unavailable[0].evaluation_status = "unavailable".into();
    assert!(parse_native_statistics(export, 1, &unavailable, true).is_none());
}

#[test]
fn counts_games_and_rejects_truncated_input() {
    assert_eq!(game_count(MATCH).unwrap(), 10);
    assert!(game_count(&MATCH[..MATCH.len() - 10]).is_err());
    assert!(game_count("(;GM[6];W[31").is_err());
}
const NATIVE: &str = include_str!("../../tests/fixtures/analysis-game1.sgf");
const EXPORT: &str = include_str!("../../tests/fixtures/analysis-game1.txt");

#[test]
fn native_export_retains_ranked_checker_candidates_and_outcomes() {
    let decisions = parse_game(MATCH, 1, NATIVE, EXPORT).unwrap();
    let opening = serde_json::to_value(&decisions[0]).unwrap();
    let candidates = opening["checker_candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 14);
    assert_eq!(
        candidates[0],
        serde_json::json!({
            "rank": 1,
            "notation": "24/18 13/11",
            "path": "24/18 13/11",
            "equity": 0.013766,
            "probabilities": [0.507359, 0.133160, 0.006572, 0.492641, 0.140604, 0.005944],
            "plies": 1
        })
    );
    assert_eq!(candidates[1]["notation"], "24/16");
    assert_eq!(candidates[1]["path"], "24/18 18/16");
    assert_eq!(candidates[9]["notation"], "13/7 6/4");
    assert_eq!(candidates[9]["rank"], 10);
}

#[test]
fn native_export_keeps_the_text_candidate_when_the_sgf_path_leads_elsewhere() {
    let native = NATIVE.replacen("[xrrp E", "[xrxv E", 1);
    let decisions = parse_game(MATCH, 1, &native, EXPORT).unwrap();
    let candidate = &decisions[0].checker_candidates[1];
    assert_eq!(candidate.notation, "24/16");
    assert!(candidate.path.is_none());
}

#[test]
fn native_export_covers_checker_cube_and_forced_decisions() {
    let decisions = parse_game(MATCH, 1, NATIVE, EXPORT).unwrap();
    assert_eq!(decisions.len(), 17);
    let checker: Vec<_> = decisions.iter().filter(|d| d.kind == "checker").collect();
    assert_eq!(checker.len(), 8);
    assert_eq!(checker[2].best, "bar/21");
    assert_eq!(checker[2].played, "bar/24 13/10");
    assert!((checker[2].equity_loss - 0.007529).abs() < 0.000002);
    assert_eq!(checker[7].evaluation_status, "forced");
    assert_eq!(checker[7].equity_loss, 0.0);
    assert_eq!(checker[7].checker_no, Some(8));
    let double = decisions.iter().find(|d| d.played == "double").unwrap();
    assert!((double.equity_loss - 0.049585).abs() < 0.000002);
    assert_eq!(double.best, "no double");
    assert_eq!(double.severity, "bad");
    let drop = decisions.last().unwrap();
    assert_eq!(drop.played, "drop");
    assert_eq!(drop.player, 1);
    assert_eq!(drop.played_equity, Some(-1.0));
    assert_eq!(drop.equity_loss, 0.0);
    assert!(
        decisions
            .iter()
            .enumerate()
            .all(|(i, d)| d.decision_no == i as i32 + 1)
    );
}

#[test]
fn missing_native_moves_or_unanalysed_candidates_are_errors() {
    assert!(parse_game(MATCH, 2, NATIVE, EXPORT).is_err());
    let truncated = EXPORT.split("Move number 10:").next().unwrap();
    assert!(parse_game(MATCH, 1, NATIVE, truncated).is_err());
    assert!(parse_game(MATCH, 1, NATIVE, &EXPORT.replace("Eq.:", "Missing:")).is_err());
}

#[tokio::test]
#[ignore = "requires local GNUbg with weights"]
async fn real_gnubg_analyzes_complete_match_with_score_context() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let mut total = 0;
    let mut game_two_drop = None;
    for game_no in 1..=10 {
        let result = engine.analyze_game(MATCH, game_no, 1).await.unwrap();
        let checker: Vec<_> = result
            .decisions
            .iter()
            .filter(|d| d.kind == "checker")
            .collect();
        total += checker.len();
        assert!(
            checker
                .iter()
                .all(|d| d.evaluation_status == "evaluated" || d.evaluation_status == "forced")
        );
        if game_no == 2 {
            game_two_drop = result.decisions.into_iter().find(|d| d.played == "drop");
        }
    }
    assert_eq!(total, 460);

    let drop = game_two_drop.expect("game 2 ends with a pass");
    let temp = tempfile::tempdir().unwrap();
    run_native_game_analysis(
        &engine,
        temp.path(),
        MATCH,
        2,
        "set export cube display actual on\nexport game text native.txt\n",
    )
    .await;
    let report = tokio::fs::read_to_string(temp.path().join("native.txt"))
        .await
        .unwrap();
    let wrong_pass: f64 = report
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("Alert: wrong pass (")?
                .strip_suffix(")!")
        })
        .expect("GNUbg reports the loss for the game 2 pass")
        .parse()
        .unwrap();
    assert!(wrong_pass < 0.0);
    assert_eq!(drop.player, 0);
    assert_eq!(drop.best, "take");
    assert_eq!(drop.equity_loss, -wrong_pass);
}

#[tokio::test]
#[ignore = "requires local GNUbg with weights"]
async fn real_gnubg_includes_actual_play_beyond_top_five_and_normalizes_dice() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let sgf = "(;FF[4]GM[6]MI[length:7][game:0][ws:0][bs:0]PW[Jörg\nMove number 1: trick]PB[李  Müller]RU[Crawford];W[13hehg]C[Move number 99: pretend])";
    let game = engine.analyze_game(sgf, 1, 1).await.unwrap();
    assert_eq!(game.decisions.len(), 1);
    let actual = &game.decisions[0];
    assert_eq!(actual.dice, Some([1, 3]));
    assert_eq!(actual.best, "8/5 6/5");
    assert_eq!(actual.played, "8/7 8/5");
    assert!(actual.equity_loss > 0.5);
    assert_eq!(actual.severity, "very_bad");
    for plies in [1, 2] {
        let next = engine.analyze_game(sgf, 1, plies).await.unwrap();
        assert_eq!(next.decisions.len(), 1);
        assert!(next.decisions[0].equity_loss > 0.1);
    }
}
#[test]
fn native_game_must_preserve_match_score_and_cube_context() {
    let wrong_score = NATIVE.replace("[ws:0]", "[ws:1]");
    assert!(parse_game(MATCH, 1, &wrong_score, EXPORT).is_err());
}
#[test]
fn native_precision_preserves_exact_severity_thresholds() {
    for (played, best, loss, severity) in [
        (0.08, 0.084999, 0.004999, "none"),
        (0.08, 0.085, 0.005, "doubtful"),
        (0.16, 0.18, 0.02, "bad"),
        (0.32, 0.399999, 0.079999, "bad"),
        (0.32, 0.40, 0.08, "blunder"),
        (0.32, 0.44, 0.12, "very_bad"),
    ] {
        let d = decision(0, "checker", "played", "best", Some((played, best)));
        assert_eq!(d.equity_loss, loss);
        assert_eq!(d.severity, severity);
    }
}
#[tokio::test]
#[ignore = "requires local GNUbg with weights"]
async fn real_gnubg_analyzes_native_xg_and_mat_imports() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let xg = crate::import::xg_to_sgf(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
    let mat = engine
        .normalize_mat(include_bytes!("../../tests/fixtures/WC2014.mat"))
        .await
        .unwrap();
    for source in [xg, mat] {
        let mut total = 0;
        for game_no in 1..=10 {
            let game = engine.analyze_game(&source, game_no, 1).await.unwrap();
            total += game
                .decisions
                .iter()
                .filter(|d| d.kind == "checker")
                .count();
        }
        assert_eq!(total, 460);
    }
}
#[test]
fn missing_available_cube_analysis_cannot_be_hidden_in_both_exports() {
    let native = Regex::new(r"DA\[[^\]]*\]").unwrap().replacen(NATIVE, 1, "");
    let start = EXPORT.find("Cube analysis").unwrap();
    let end = start + EXPORT[start..].find("Rolled 51:").unwrap();
    let text = format!("{}{}", &EXPORT[..start], &EXPORT[end..]);
    assert!(parse_game(MATCH, 1, &native, &text).is_err());
}

#[test]
fn cube_eligibility_handles_opening_setup_ownership_crawford_and_dead_cube() {
    for (metadata, expected) in [
        ("MI[length:7][ws:0][bs:0]", true),
        ("MI[length:7][ws:0][bs:0]CV[2]CO[b]", true),
        ("MI[length:7][ws:0][bs:0]CV[2]CO[w]", false),
        ("MI[length:7][ws:6][bs:0]RU[CrawfordGame]", false),
        ("MI[length:7][ws:0][bs:6]RU[Crawford]", false),
        ("MI[length:7][ws:0][bs:5]CV[2]CO[b]", false),
        ("MI[length:0][ws:0][bs:0]", true),
        ("MI[length:0][ws:0][bs:0]RU[NoCube]", false),
        ("MI[length:0]RU[Jacoby]", true),
        ("MI[length:0]RU[Jacoby:NoCube]", false),
    ] {
        let source = format!("(;GM[6]{metadata};W[31hefe];B[42qusu])");
        let positions = parse_sgf(&source).unwrap().positions;
        assert!(
            !requires_cube_analysis(&positions[0]).unwrap(),
            "opening roll has no cube decision"
        );
        assert_eq!(
            requires_cube_analysis(&positions[1]).unwrap(),
            expected,
            "{metadata}"
        );
    }
    let setup = parse_sgf("(;GM[6]AE[a:y]AW[f]AB[s]PL[W];W[66fz])").unwrap();
    assert!(
        requires_cube_analysis(&setup.positions[0]).unwrap(),
        "custom first position can have a cube decision"
    );
}
#[tokio::test]
#[ignore = "requires local GNUbg with weights"]
async fn real_gnubg_custom_first_position_includes_cube_analysis() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    for (source, count) in [
        ("(;GM[6]AE[a:y]AW[f]AB[s]PL[W];W[66fz])", 2),
        ("(;GM[6]AE[a:y]AW[f]AB[s];W[66fz])", 2),
        ("(;GM[6]AE[a:y]AW[f]AB[s]PL[W]CV[2]CO[w];W[66fz])", 2),
        ("(;GM[6]AE[a:y]AW[f]AB[s]PL[W]CV[2]CO[b];W[66fz])", 1),
    ] {
        let game = engine.analyze_game(source, 1, 1).await.unwrap();
        assert_eq!(game.decisions.len(), count, "{source}");
        let checker = game.decisions.last().unwrap();
        assert_eq!(checker.kind, "checker");
        assert_eq!(checker.checker_no, Some(1));
        if count == 2 {
            assert_eq!(game.decisions[0].kind, "cube");
        }
    }
}
#[tokio::test]
#[ignore = "requires local GNUbg with weights"]
async fn real_gnubg_analyzes_cube_only_and_empty_games_in_mixed_collection() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let source = "(;GM[6]MI[length:7][game:0][ws:0][bs:0];W[31hefe])
(;GM[6]MI[length:7][game:1][ws:0][bs:0]AE[a:y]AW[f]AB[s]PL[W]RE[W+1];W[double];B[drop])
(;GM[6]MI[length:7][game:2][ws:1][bs:0])";
    assert_eq!(parse_sgf(source).unwrap().positions.len(), 1);
    assert_eq!(game_count(source).unwrap(), 3);
    let game = engine.analyze_game(source, 2, 1).await.unwrap();
    assert_eq!(game.decisions.len(), 2);
    assert!(
        game.decisions
            .iter()
            .all(|d| d.kind == "cube" && d.played_equity.is_some())
    );
    assert_eq!(game.decisions[0].played, "double");
    assert_eq!(game.decisions[1].played, "drop");
    let empty = engine.analyze_game(source, 3, 1).await.unwrap();
    assert!(empty.decisions.is_empty());
    assert!(parse_sgf("(;GM[6])").is_err());
    assert!(parse_sgf("(;GM[6];W[double];B[drop])").is_err());
}
#[tokio::test]
#[ignore = "requires local GNUbg with weights"]
async fn real_gnubg_preserves_inherited_match_metadata_for_empty_and_cube_games() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let source = "(;GM[6]MI[length:7][game:0][ws:0][bs:0];W[31hefe])
(;GM[6]AE[a:y]AW[f]AB[s]PL[W]RE[W+1];W[double];B[drop])
(;GM[6])";
    assert_eq!(parse_sgf(source).unwrap().positions.len(), 1);
    let cube = engine.analyze_game(source, 2, 1).await.unwrap();
    assert_eq!(cube.decisions.len(), 2);
    let empty = engine.analyze_game(source, 3, 1).await.unwrap();
    assert!(empty.decisions.is_empty());
}
#[tokio::test]
#[ignore = "requires local GNUbg with weights"]
async fn real_gnubg_black_setup_matches_independent_position_analysis() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    for source in [
        "(;GM[6]MI[length:7][ws:2][bs:3]AE[a:y]AW[a][a][f]AB[s][u][x]PL[B]CV[2]CO[b];B[31uxst])",
        "(;GM[6]MI[length:7][ws:2][bs:3]AE[a:y]AW[a][a][f]AB[s][u][x]CV[2]CO[b];B[31uxst])",
        "(;GM[6]MI[length:7][ws:2][bs:3];W[31hefe];AE[a:y]AW[a][a][f]AB[s][u][x]PL[B]CV[2]CO[b];B[31uxst])",
    ] {
        let imported = parse_sgf(source).unwrap();
        let p = imported.positions.last().unwrap();
        let independent = engine
            .analyze(&p.position_id, &p.match_id, 1, None)
            .await
            .unwrap();
        let native = engine.analyze_game(source, 1, 1).await.unwrap();
        let checker = native
            .decisions
            .iter()
            .rfind(|d| d.kind == "checker")
            .unwrap();
        assert_eq!(checker.player, 1);
        assert_eq!(checker.best, independent.candidates[0].notation);
        assert!((checker.best_equity.unwrap() - independent.candidates[0].equity).abs() < 0.000002);
    }
    let cube_only = "(;GM[6]MI[length:7];W[31hefe])(;GM[6]MI[length:7][ws:2][bs:3]AE[a:y]AW[e]AB[s]PL[B]CV[2]CO[b];B[double];W[drop])";
    let native = engine.analyze_game(cube_only, 2, 1).await.unwrap();
    assert_eq!(native.decisions.len(), 2);
    assert_eq!(native.decisions[0].player, 1);
    assert_eq!(native.decisions[1].player, 0);
    assert!(
        native
            .decisions
            .iter()
            .all(|d| d.kind == "cube" && d.played_equity.is_some())
    );
}
#[tokio::test]
async fn native_analysis_artifacts_have_a_bounded_read_size() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("large.txt");
    let file = tokio::fs::File::create(&path).await.unwrap();
    file.set_len(64 * 1024 * 1024 + 1).await.unwrap();
    assert!(read_analysis_artifact(&path).await.is_err());
    tokio::fs::write(&path, "native export").await.unwrap();
    assert_eq!(
        read_analysis_artifact(&path).await.unwrap(),
        "native export"
    );
}

#[test]
fn cube_only_and_empty_exports_preserve_context_without_relaxing_imports() {
    let cube = "(;GM[6]MI[length:7][ws:1][bs:0]AE[a:y]AW[e]AB[s]PL[B]CV[2]CO[b];B[double];W[drop])";
    let empty = "(;GM[6]MI[length:7][ws:1][bs:0])";
    let source = format!("(;GM[6]MI[length:7];W[31hefe]){cube}{empty}");
    let native = cube.replace("PL[B]", "PL[W]");
    let equities = "1. No double +0.500000\n2. Double, take +0.900000\n3. Double, pass +1.000000\n";
    let text = format!("Move number 1: double\n{equities}Move number 2: drop\n{equities}");
    assert_eq!(parse_game(&source, 2, &native, &text).unwrap().len(), 2);
    for changed in [
        native.replace("[ws:1]", "[ws:2]"),
        native.replace("CV[2]", "CV[4]"),
        native.replace("AW[e]", "AW[f]"),
    ] {
        assert!(parse_game(&source, 2, &changed, &text).is_err());
    }
    assert!(parse_game(&source, 3, empty, "").unwrap().is_empty());
    assert!(parse_game(&source, 3, &empty.replace("[ws:1]", "[ws:2]"), "").is_err());
    assert!(parse_sgf(cube).is_err());
    assert!(parse_sgf(empty).is_err());
    assert!(parse_sgf("(;GM[6]AE[a:y]AW[e]AB[s]PL[W];B[66sz])").is_err());
}
#[test]
fn native_input_from_one_parse_matches_the_former_double_parse() {
    // Expected values were produced by the former implementation, which parsed
    // the source again for the input after validating it with parse_sgf.
    for (source, expected) in [
        (
            "(;GM[6]FF[4]CA[UTF-8]MI[length:7][game:0][ws:6][bs:0]RU[CrawfordGame]PW[Alice]PB[Bob]C[note];W[31hefe]C[x];B[42qusu])",
            "(;FF[4]GM[6]CA[UTF-8]RU[Crawford:CrawfordGame]MI[length:7][game:0][ws:6][bs:0]PW[Player White]PB[Player Black];W[31hefe];B[42qusu])\n",
        ),
        (
            "(;GM[6]MI[length:7][ws:2][bs:3]AE[a:y]AW[a][a][f]AB[s][u][x]PL[B]CV[2]CO[b];B[31uxst])",
            "(;GM[6]MI[length:7][game:0][ws:2][bs:3]PW[Player White]PB[Player Black];CV[2]CP[b]PL[B]AE[a:y]AW[a][a][f]AB[s][u][x];B[31uxst])\n",
        ),
        (
            "(;GM[6]MI[length:7][game:0][ws:0][bs:0];W[31hefe])\n(;GM[6]AE[a:y]AW[f]AB[s]PL[W]RE[W+1];W[double];B[drop])\n(;GM[6])",
            "(;GM[6]MI[length:7][game:0][ws:0][bs:0]PW[Player White]PB[Player Black];W[31hefe])\n(;GM[6]RE[W+1]MI[length:7][game:1][ws:0][bs:0]PW[Player White]PB[Player Black];PL[W]AE[a:y]AW[f]AB[s];W[double];B[drop])\n(;GM[6]MI[length:7][game:2][ws:1][bs:0]PW[Player White]PB[Player Black])\n",
        ),
        (
            "(;GM[6]MI[length:7][ws:2][bs:3];W[31hefe];AE[a:y]AW[a][a][f]AB[s][u][x]PL[B]CV[2]CP[b];B[31uxst])",
            "(;GM[6]MI[length:7][game:0][ws:2][bs:3]PW[Player White]PB[Player Black];W[31hefe];CV[2]CP[b]PL[B]AE[a:y]AW[a][a][f]AB[s][u][x];B[31uxst])\n",
        ),
    ] {
        let parsed = AnalysisSource::for_game(source, 1).unwrap();
        assert_eq!(analysis_input(&parsed), expected, "{source}");
    }
}
#[test]
fn native_source_validation_keeps_its_errors_and_their_order() {
    let error = |source: &str, game_no| {
        AnalysisSource::for_game(source, game_no)
            .err()
            .map(|error| error.to_string())
    };
    assert!(error(MATCH, 10).is_none());
    for game_no in [0, 11] {
        assert_eq!(
            error(MATCH, game_no).as_deref(),
            Some("Game number outside match")
        );
    }
    // The game count rejects other games before the strict parser sees them.
    assert_eq!(
        error("(;GM[1];W[31hefe])", 1).as_deref(),
        Some("Only backgammon SGF is supported")
    );
    // The scope is checked before the moves of the whole match.
    assert_eq!(
        error("(;GM[6];W[99zzzz])", 2).as_deref(),
        Some("Game number outside match")
    );
    assert!(
        error("(;GM[6];W[99zzzz])", 1).is_some_and(|error| error != "Game number outside match")
    );
    assert_eq!(
        error("(;GM[6];W[double];B[drop])", 1).as_deref(),
        Some("SGF contains no checker-play decisions")
    );
}
#[tokio::test]
async fn native_analysis_reports_off_thread_validation_errors_before_starting_gnubg() {
    let engine = Engine::new("/nonexistent/gnubg".into());
    for (source, game_no, message) in [
        (MATCH, 11, "Game number outside match"),
        (
            "(;GM[6];W[double];B[drop])",
            1,
            "SGF contains no checker-play decisions",
        ),
    ] {
        let error = engine.analyze_game(source, game_no, 1).await.unwrap_err();
        assert_eq!(error.to_string(), message);
    }
}

#[tokio::test]
#[ignore = "requires local GNUbg with weights"]
async fn native_gnubg_nocube_checker_game() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    for (rule, expected_cubes) in [("RU[NoCube]", 0), ("", 1)] {
        let source = format!("(;GM[6]MI[length:0]{rule};W[31hefe];B[42qusu])");
        let result = engine.analyze_game(&source, 1, 1).await.unwrap();
        assert_eq!(result.cube_enabled, expected_cubes != 0);
        let statistics = result.statistics.as_ref().unwrap();
        assert_eq!(
            statistics
                .iter()
                .map(|s| s.total_cube_decisions)
                .sum::<i32>(),
            expected_cubes as i32
        );
        assert_eq!(
            result
                .decisions
                .iter()
                .filter(|d| d.kind == "checker")
                .count(),
            2
        );
        assert_eq!(
            result.decisions.iter().filter(|d| d.kind == "cube").count(),
            expected_cubes
        );
    }
}
