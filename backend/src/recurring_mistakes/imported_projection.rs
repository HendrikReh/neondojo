//! Read-only foreign analysis validation against the original played source.
use crate::{
    import::sgf::SourceMove,
    imported_analysis::ImportedAnalysisData,
    match_analysis::{CubeEquityUnit, CubeEvaluation, Decision, cube_options},
    model::Position,
};
use uuid::Uuid;

/// Reconstruct the original event from the immutable catalog, without parsing a full match.
pub(crate) fn source_move(
    event: &crate::move_catalog::StoredMove,
    checker_ply: Option<i32>,
) -> Option<SourceMove> {
    let kind = match event.kind.as_str() {
        "checker" => "checker",
        "cube" => "cube",
        _ => return None,
    };
    let dice = event.dice.as_slice().try_into().ok()?;
    let after_board = if kind == "checker" {
        let position = Position {
            id: event.position_uuid.unwrap_or(Uuid::nil()),
            match_uuid: event.match_uuid,
            ordinal: checker_ply.unwrap_or(event.move_no),
            game_no: event.game_no,
            ply: checker_ply.unwrap_or(0),
            player: event.player,
            dice: event.dice.clone(),
            board: event.board.clone(),
            position_id: event.position_id.clone(),
            gnubg_match_id: event.gnubg_match_id.clone(),
            notation: String::new(),
        };
        crate::move_assessments::after_play(&position, &event.notation)?.to_vec()
    } else {
        event.board.clone()
    };
    Some(SourceMove {
        game_no: event.game_no,
        move_no: event.move_no,
        kind,
        player: event.player,
        notation: event.notation.clone(),
        checker_ply,
        board: event.board.clone(),
        dice,
        position_id: event.position_id.clone(),
        match_id: event.gnubg_match_id.clone(),
        after_board,
        after_match_id: String::new(),
    })
}

/// Recover the board and actor from the immutable source event. A skipped
/// double shares the checker event but occurs before its dice were rolled.
pub(crate) fn review_position(
    source: &SourceMove,
    match_id: Uuid,
    position_uuid: Option<Uuid>,
    before_roll: bool,
) -> Option<Position> {
    use crate::import::ids::{Field, MatchId, decode_board, hide_dice};
    let state = MatchId::decode(&source.match_id)?;
    let response = !before_roll && matches!(action(&source.notation), Some("take" | "drop"));
    if state.get(Field::TURN) != source.player
        || (state.get(Field::DOUBLED) != 0) != response
        || decode_board(&source.position_id, &source.match_id).ok()? != source.board
    {
        return None;
    }
    let (dice, match_state) = if before_roll {
        ([0, 0], hide_dice(&source.match_id)?)
    } else {
        (source.dice, source.match_id.clone())
    };
    Some(Position {
        id: position_uuid.unwrap_or(Uuid::nil()),
        match_uuid: match_id,
        ordinal: source.checker_ply.unwrap_or(source.move_no),
        game_no: source.game_no,
        ply: source.checker_ply.unwrap_or(0),
        player: source.player,
        dice: dice.to_vec(),
        board: source.board.clone(),
        position_id: source.position_id.clone(),
        gnubg_match_id: match_state,
        notation: String::new(),
    })
}

pub(crate) struct Projection {
    pub(crate) decision: Decision,
    pub(crate) position: Position,
    pub(crate) depth: Option<String>,
    pub(crate) depth_plies: Option<i32>,
}

/// Validate identity and context before deriving a normalized decision. No source
/// record, engine job, or learning history is changed by this projection.
pub(crate) fn project(
    record: &ImportedAnalysisData,
    source: &SourceMove,
    match_id: Uuid,
    position_uuid: Option<Uuid>,
    source_format: &str,
) -> Option<Projection> {
    if !(matches!(source_format, "xg" | "bgf")
        || source_format == "sgf" && record.kind == "checker")
        || record.game_no != source.game_no
        || record.source_move_no != source.move_no
        || record.player != source.player
        || record.checker_ply != source.checker_ply
        || !(0..=1).contains(&source.player)
        || record.engine.trim().is_empty()
        || record.candidates.len() > 256
        || record
            .position_id
            .as_ref()
            .is_some_and(|id| id != &source.position_id)
    {
        return None;
    }
    let checker = record.kind == "checker" && source.kind == "checker";
    let virtual_cube = record.kind == "cube" && source.kind == "checker";
    let played_action = action(&source.notation);
    if !(checker
        || virtual_cube
        || record.kind == "cube"
            && source.kind == "cube"
            && matches!(played_action, Some("double" | "take" | "drop")))
    {
        return None;
    }
    let position = review_position(source, match_id, position_uuid, virtual_cube)?;
    if record
        .gnubg_match_id
        .as_ref()
        .is_some_and(|id| id != &position.gnubg_match_id)
        || record
            .dice
            .is_some_and(|value| value.as_slice() != position.dice)
    {
        return None;
    }
    let response = !virtual_cube && matches!(played_action, Some("take" | "drop"));
    let mut cube_evaluation = None;
    let (played, best, played_equity, best_equity, equity_loss, depth, depth_plies) = if checker {
        let mut bests = record.candidates.iter().filter(|c| c.rank == 1);
        let best = bests.next()?;
        if bests.next().is_some() {
            return None;
        }
        let actual = crate::move_assessments::after_play(&position, &source.notation)?;
        if actual.as_slice() != source.after_board.as_slice()
            || crate::move_assessments::after_play(&position, &record.played)? != actual
        {
            return None;
        }
        crate::move_assessments::after_play(&position, &best.notation)?;
        let best_eq = checker_equity(best, source_format)?;
        // Every matching candidate must demonstrate the same value. Ambiguous
        // duplicated finals cannot supply a guessed played loss.
        let mut played_eq = None;
        for c in &record.candidates {
            if crate::move_assessments::after_play(&position, &c.notation) == Some(actual) {
                if c.rank <= 0 {
                    return None;
                }
                let value = checker_equity(c, source_format)?;
                if played_eq.is_some_and(|old: f64| (old - value).abs() > 1e-7) {
                    return None;
                }
                played_eq = Some(value);
            }
        }
        let played_eq = played_eq?;
        let loss = if source_format == "sgf" {
            crate::move_assessments::rounded_loss(played_eq, best_eq)?
        } else {
            loss(best_eq, played_eq)?
        };
        (
            source.notation.clone(),
            best.notation.clone(),
            Some(played_eq),
            Some(best_eq),
            loss,
            best.depth.clone().or_else(|| record.depth.clone()),
            best.depth_plies.or(record.depth_plies),
        )
    } else {
        let played = if virtual_cube {
            "no double"
        } else {
            played_action?
        };
        if action(&record.played) != Some(played) {
            return None;
        }
        let (best, played_eq, best_eq, loss, evaluation): ImportedCubeEvaluation =
            if source_format == "xg" {
                let c = record.cube.as_ref()?;
                if c["equity_kind"] != "cubeful" || c["perspective"] != "doubler" {
                    return None;
                }
                let nd = number(&c["no_double"])?;
                let dt = number(&c["double_take"])?;
                let dp = number(&c["double_pass"])?;
                if [nd, dt, dp].contains(&-99.0) {
                    return None;
                }
                let sign = if response { -1.0 } else { 1.0 };
                let evaluation = CubeEvaluation {
                    unit: CubeEquityUnit::Cubeful,
                    no_double: sign * nd,
                    double_take: sign * dt,
                    double_pass: sign * dp,
                    probabilities: None,
                };
                let (best, pe, be) = cube_options(
                    played,
                    evaluation.no_double,
                    evaluation.double_take,
                    evaluation.double_pass,
                )?;
                (best, Some(pe), Some(be), loss(be, pe)?, evaluation)
            } else {
                bgblitz_cube(record, played)?
            };
        cube_evaluation = Some(evaluation);
        (
            played.to_owned(),
            best.to_owned(),
            played_eq,
            best_eq,
            loss,
            record.depth.clone(),
            record.depth_plies,
        )
    };
    Some(Projection {
        decision: Decision {
            decision_no: source.move_no,
            checker_no: source.checker_ply,
            player: source.player,
            kind: record.kind.clone(),
            played,
            best,
            played_equity,
            best_equity,
            equity_loss,
            severity: crate::configuration::ErrorThresholds::default()
                .classify(equity_loss)
                .as_str()
                .into(),
            dice: checker.then_some(source.dice),
            evaluation_status: "evaluated".into(),
            checker_candidates: vec![],
            cube_evaluation,
        },
        position,
        depth,
        depth_plies,
    })
}

fn action(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "no double" => Some("no double"),
        "double" => Some("double"),
        "take" => Some("take"),
        "drop" | "pass" => Some("drop"),
        _ => None,
    }
}
fn number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .filter(|n| n.is_finite())
}
fn checker_equity(c: &crate::imported_analysis::ImportedCandidate, format: &str) -> Option<f64> {
    if format == "bgf" {
        // The displayed MWC/cubeless value is never an EMG substitute.
        if c.raw["eq"]["hasEMG"] != true {
            return None;
        }
        number(&c.raw["eq"]["emg"]).filter(|v| *v != -99.0)
    } else if c.equity_kind == "cubeful" {
        c.equity.filter(|n| n.is_finite() && *n != -99.0)
    } else {
        None
    }
}
fn loss(best: f64, played: f64) -> Option<f64> {
    let difference = best - played;
    (difference.is_finite() && difference >= -0.000001).then_some(difference.max(0.0))
}
type ImportedCubeEvaluation = (&'static str, Option<f64>, Option<f64>, f64, CubeEvaluation);

fn bgblitz_cube(record: &ImportedAnalysisData, played: &str) -> Option<ImportedCubeEvaluation> {
    let equity = &record.raw["equity"];
    let mwc = equity["useMatchEquity"].as_bool()?;
    let c = &equity["cubeDecision"];
    let nd = number(&c["eqNoDouble"])?;
    let dt = number(&c["eqDoubleTake"])?;
    let dp = number(&c["eqDoublePass"])?;
    if [nd, dt, dp].contains(&-99.0) || mwc && [nd, dt, dp].iter().any(|n| !(0.0..=1.0).contains(n))
    {
        return None;
    }
    let (best, pe, be) = cube_options(played, nd, dt, dp)?;
    let option_loss = loss(be, pe)?;
    let evaluation = CubeEvaluation {
        unit: if mwc {
            CubeEquityUnit::Mwc
        } else {
            CubeEquityUnit::Cubeful
        },
        no_double: nd,
        double_take: dt,
        double_pass: dp,
        probabilities: None,
    };
    let error = number(&record.raw["pr"]["cubeError"])?;
    if error > 0.0 || error == -99.0 {
        return None;
    }
    let normalized_loss = -error;
    // Source errors and options must agree about the actual best action.
    if (option_loss <= 1e-7) != (normalized_loss <= 1e-7) {
        return None;
    }
    if mwc {
        if let Some(value) = record.raw["pr"].get("cubeErrorMWP") {
            let error_mwp = number(value)?;
            if error_mwp > 0.0 || error_mwp == -99.0 || (option_loss + error_mwp).abs() > 0.000001 {
                return None;
            }
        }
        Some((best, None, None, normalized_loss, evaluation))
    } else {
        if (option_loss - normalized_loss).abs() > 0.000001 {
            return None;
        }
        Some((best, Some(pe), Some(be), normalized_loss, evaluation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        import::sgf::parse_sgf_source_moves,
        imported_analysis::{ImportedCandidate, extract},
    };
    use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
    use serde_json::{Value, json};

    fn fixture(sgf: &str, kind: &str, index: usize) -> (SourceMove, ImportedAnalysisData) {
        let (_, mut sources) = parse_sgf_source_moves(sgf).unwrap();
        let source = sources.remove(index);
        let mut state = STANDARD_NO_PAD.decode(&source.match_id).unwrap();
        let virtual_cube = kind == "cube" && source.kind == "checker";
        if virtual_cube {
            for bit in 15..21 {
                state[bit / 8] &= !(1 << (bit % 8));
            }
        }
        let record = ImportedAnalysisData {
            game_no: source.game_no,
            source_move_no: source.move_no,
            checker_ply: source.checker_ply,
            position_id: Some(source.position_id.clone()),
            gnubg_match_id: Some(STANDARD_NO_PAD.encode(state)),
            dice: Some(if virtual_cube { [0, 0] } else { source.dice }),
            kind: kind.into(),
            player: source.player,
            played: if virtual_cube {
                "No double".into()
            } else {
                source.notation.clone()
            },
            engine: "eXtreme Gammon".into(),
            engine_version: Some("2.19".into()),
            method: "xgroller".into(),
            depth: Some("XGRoller++".into()),
            depth_plies: None,
            candidates: vec![],
            cube: None,
            raw: Value::Null,
        };
        (source, record)
    }
    fn candidate(rank: i32, notation: &str, equity: f64) -> ImportedCandidate {
        ImportedCandidate {
            rank,
            notation: notation.into(),
            equity: Some(equity),
            equity_kind: "cubeful".into(),
            probabilities: Value::Null,
            depth: Some("XGRoller++".into()),
            depth_plies: None,
            method: Some("xgroller".into()),
            raw: Value::Null,
        }
    }
    fn checker() -> (SourceMove, ImportedAnalysisData) {
        let (s, mut r) = fixture("(;FF[4]GM[6];W[31hefe])", "checker", 0);
        r.candidates = vec![
            candidate(1, "24/21 6/5", 0.20),
            candidate(2, "6/5 8/5", 0.10),
        ];
        (s, r)
    }
    fn projection(r: &ImportedAnalysisData, s: &SourceMove, format: &str) -> Option<Projection> {
        project(r, s, Uuid::nil(), Some(Uuid::from_u128(7)), format)
    }
    #[test]
    fn xg_checker_joins_legal_final_board_and_preserves_depth_without_revealing_notation() {
        let (s, r) = checker();
        let p = projection(&r, &s, "xg").unwrap();
        assert!((p.decision.equity_loss - 0.1).abs() < 1e-9);
        assert_eq!(p.decision.severity, "blunder");
        assert_eq!(p.decision.played, s.notation);
        assert_eq!(p.decision.dice, Some(s.dice));
        assert_eq!(p.depth.as_deref(), Some("XGRoller++"));
        assert_eq!(p.depth_plies, None);
        assert_eq!(p.position.notation, "");
        assert_eq!(p.position.board, s.board);
        assert_eq!(p.position.id, Uuid::from_u128(7));
    }
    #[test]
    fn checker_requires_unique_best_legal_actual_and_original_identity() {
        let (s, r) = checker();
        let mut wrong = r.clone();
        wrong.candidates.pop();
        assert!(projection(&wrong, &s, "xg").is_none());
        let mut wrong = r.clone();
        wrong.candidates.push(r.candidates[0].clone());
        assert!(projection(&wrong, &s, "xg").is_none());
        let mut wrong = r.clone();
        wrong.candidates[0].notation = "24/1".into();
        assert!(projection(&wrong, &s, "xg").is_none());
        let mut wrong = r.clone();
        wrong.played = "24/21 6/5".into();
        assert!(projection(&wrong, &s, "xg").is_none());
        for field in 0..7 {
            let mut wrong = r.clone();
            match field {
                0 => wrong.game_no += 1,
                1 => wrong.source_move_no += 1,
                2 => wrong.player = 1,
                3 => wrong.checker_ply = Some(2),
                4 => wrong.position_id = Some("forged".into()),
                5 => wrong.gnubg_match_id = Some("forged".into()),
                _ => wrong.dice = Some([6, 6]),
            };
            assert!(projection(&wrong, &s, "xg").is_none(), "field {field}");
        }
    }
    #[test]
    fn bgblitz_uses_explicit_emg_when_display_equities_are_mwc() {
        let (s, mut r) = checker();
        r.engine = "BGBlitz".into();
        for (c, emg) in r.candidates.iter_mut().zip([0.00224164, -0.03425582]) {
            c.equity_kind = "mwc".into();
            c.equity = Some(0.5);
            c.raw = json!({"eq":{"hasEMG":true,"emg":emg}});
        }
        let p = projection(&r, &s, "bgf").unwrap();
        assert!((p.decision.equity_loss - 0.03649746).abs() < 1e-8);
        r.candidates[1].raw["eq"]["hasEMG"] = json!(false);
        assert!(projection(&r, &s, "bgf").is_none());
        r.candidates[1].raw = Value::Null;
        assert!(projection(&r, &s, "bgf").is_none());
    }
    #[test]
    fn checker_rejects_mwc_unknown_units_and_nonfinite_values() {
        let (s, r) = checker();
        for kind in ["mwc", "unknown", "cubeless"] {
            let mut wrong = r.clone();
            for c in &mut wrong.candidates {
                c.equity_kind = kind.into();
            }
            assert!(projection(&wrong, &s, "xg").is_none());
        }
        let mut wrong = r.clone();
        wrong.candidates[1].equity = Some(f64::NAN);
        assert!(projection(&wrong, &s, "xg").is_none());
    }
    fn cube(r: &mut ImportedAnalysisData, nd: f64, dt: f64, dp: f64) {
        r.cube = Some(
            json!({"equity_kind":"cubeful","perspective":"doubler","no_double":nd,"double_take":dt,"double_pass":dp}),
        );
    }
    #[test]
    fn xg_virtual_cube_hides_next_roll_and_uses_offer_minimum() {
        let (s, mut r) = fixture("(;FF[4]GM[6];W[31hefe])", "cube", 0);
        cube(&mut r, 0.4, 0.6, 1.0);
        let p = projection(&r, &s, "xg").unwrap();
        assert_eq!(p.decision.best, "double");
        assert_eq!(p.decision.played, "no double");
        assert_eq!(p.decision.dice, None);
        assert_eq!(p.position.dice, vec![0, 0]);
        assert_eq!(p.position.gnubg_match_id, r.gnubg_match_id.unwrap());
        assert!((p.decision.equity_loss - 0.2).abs() < 1e-9);
    }
    #[test]
    fn xg_actual_offers_and_responses_use_each_colors_perspective() {
        for prefix in [
            "(;FF[4]GM[6];W[double];B[take])",
            "(;FF[4]GM[6];W[31hefe];B[double];W[take])",
        ] {
            for index in [
                if prefix.contains("31") { 1 } else { 0 },
                if prefix.contains("31") { 2 } else { 1 },
            ] {
                let (s, mut r) = fixture(prefix, "cube", index);
                cube(&mut r, 0.9, 1.2, 1.0);
                let p = projection(&r, &s, "xg").unwrap();
                assert_eq!(p.decision.player, s.player);
                assert_eq!(p.position.board, s.board);
                assert_eq!(p.position.gnubg_match_id, s.match_id);
                if s.notation == "Take" {
                    assert_eq!(p.decision.best, "drop");
                    assert!((p.decision.equity_loss - 0.2).abs() < 1e-9);
                    let evaluation = p.decision.cube_evaluation.unwrap();
                    assert_eq!(evaluation.double_take, -1.2);
                    assert_eq!(evaluation.double_pass, -1.0);
                } else {
                    assert_eq!(p.decision.best, "double");
                    assert_eq!(p.decision.equity_loss, 0.0);
                }
            }
        }
    }
    #[test]
    fn bgblitz_response_mwc_uses_actor_options_and_proven_signed_normalized_loss() {
        let (s, mut r) = fixture("(;FF[4]GM[6];W[double];B[take])", "cube", 1);
        r.raw = json!({"equity":{"useMatchEquity":true,"cubeDecision":{"eqNoDouble":"0.42259570","eqDoubleTake":"0.41401069","eqDoublePass":"0.41765104"}},"pr":{"cubeError":"-0.08841271","cubeErrorMWP":"-0.00364035"}});
        r.cube = Some(json!({"equity_kind":"unknown","source":r.raw["equity"]}));
        let p = projection(&r, &s, "bgf").unwrap();
        assert_eq!(p.decision.best, "drop");
        assert_eq!(p.decision.player, 1);
        assert!((p.decision.equity_loss - 0.08841271).abs() < 1e-9);
        assert_eq!(p.decision.best_equity, None);
        assert_eq!(p.decision.played_equity, None);
        let evaluation = p.decision.cube_evaluation.unwrap();
        assert!(matches!(evaluation.unit, CubeEquityUnit::Mwc));
        assert_eq!(evaluation.double_take, 0.41401069);
        assert_eq!(evaluation.double_pass, 0.41765104);
        for bad in ["-99.00000000", "0.08841271", "NaN"] {
            r.raw["pr"]["cubeError"] = json!(bad);
            assert!(projection(&r, &s, "bgf").is_none());
        }
        r.raw["pr"]["cubeError"] = json!("-0.08841271");
        r.raw["pr"]["cubeErrorMWP"] = json!("-0.4");
        assert!(projection(&r, &s, "bgf").is_none());
    }
    #[test]
    fn real_binary_xg_parser_records_project_without_engine_calls() {
        let sgf =
            crate::import::xg_to_sgf(include_bytes!("../../tests/fixtures/WC2014.xg")).unwrap();
        let records = extract(&sgf).unwrap();
        let (_, sources) = parse_sgf_source_moves(&sgf).unwrap();
        let projected: Vec<_> = records
            .iter()
            .filter_map(|r| {
                sources
                    .iter()
                    .find(|s| s.game_no == r.game_no && s.move_no == r.source_move_no)
                    .and_then(|s| projection(r, s, "xg"))
            })
            .collect();
        assert!(
            projected
                .iter()
                .any(|p| p.decision.kind == "checker" && p.decision.equity_loss > 0.04)
        );
        assert!(
            projected
                .iter()
                .any(|p| p.decision.kind == "cube" && p.decision.equity_loss > 0.04)
        );
        assert!(
            projected
                .iter()
                .any(|p| p.depth.as_deref() == Some("XGRoller++"))
        );
    }
    #[test]
    fn bgf_parser_retains_emg_for_projection_without_rewriting_mwc() {
        let bytes = include_bytes!("../../tests/fixtures/tutorial.bgf");
        let text = std::str::from_utf8(bytes).unwrap();
        let (header, body) = text.split_once('\n').unwrap();
        let mut value: Value = serde_json::from_str(body).unwrap();
        value["games"][0]["moves"][0]["moveAnalysis"] = json!([
            {"move":{"from":[24,6,-1,-1],"to":[21,5,-1,-1]},"eq":{"useMatchEquity":true,"matchEquity":"0.52","hasEMG":true,"emg":"0.20"}},
            {"move":{"from":[8,6,-1,-1],"to":[5,5,-1,-1]},"eq":{"useMatchEquity":true,"matchEquity":"0.50","hasEMG":true,"emg":"0.10"}}
        ]);
        let sgf = crate::import::bgf_to_sgf(format!("{header}\n{value}").as_bytes()).unwrap();
        let records = extract(&sgf).unwrap();
        let (_, sources) = parse_sgf_source_moves(&sgf).unwrap();
        let p = projection(&records[0], &sources[0], "bgf").unwrap();
        assert!((p.decision.equity_loss - 0.1).abs() < 1e-9);
        assert_eq!(records[0].candidates[0].equity_kind, "mwc");
    }
    #[test]
    fn xg_cube_sentinel_is_never_a_demonstrated_normalized_loss() {
        let (s, mut r) = fixture("(;FF[4]GM[6];W[31hefe])", "cube", 0);
        for values in [(-99.0, 0.6, 1.0), (0.4, -99.0, 1.0), (0.4, 0.6, -99.0)] {
            cube(&mut r, values.0, values.1, values.2);
            assert!(projection(&r, &s, "xg").is_none());
        }
    }
    #[test]
    fn grouped_checker_candidates_use_legal_final_board_equivalence() {
        let (s, mut r) = fixture("(;FF[4]GM[6];W[11fefehghg])", "checker", 0);
        r.candidates = vec![
            candidate(1, "6/5(2) 8/7(2)", 0.3),
            candidate(2, "8/7 6/5 8/7 6/5", 0.3),
        ];
        let p = projection(&r, &s, "xg").unwrap();
        assert_eq!(p.decision.equity_loss, 0.0);
    }
    #[test]
    fn xg_wrong_offer_too_good_and_wrong_drop_use_source_actor_for_both_colors() {
        for sgf in [
            "(;FF[4]GM[6];W[double];B[drop])",
            "(;FF[4]GM[6];W[31hefe];B[double];W[drop])",
        ] {
            let first = if sgf.contains("31") { 1 } else { 0 };
            let (s, mut r) = fixture(sgf, "cube", first);
            cube(&mut r, 1.3, 1.2, 1.0);
            let p = projection(&r, &s, "xg").unwrap();
            assert_eq!(p.decision.best, "no double");
            assert!((p.decision.equity_loss - 0.3).abs() < 1e-9);
            let (s, mut r) = fixture(sgf, "cube", first + 1);
            cube(&mut r, 0.5, 0.8, 1.0);
            let p = projection(&r, &s, "xg").unwrap();
            assert_eq!(p.decision.best, "take");
            assert!((p.decision.equity_loss - 0.2).abs() < 1e-9);
        }
    }
    #[test]
    fn bgblitz_offer_mwc_minimum_and_virtual_source_shape_keep_demonstrated_loss() {
        for (sgf, index) in [
            ("(;FF[4]GM[6];W[double];B[take])", 0),
            ("(;FF[4]GM[6];W[31hefe])", 0),
        ] {
            let (s, mut r) = fixture(sgf, "cube", index);
            let virtual_cube = s.kind == "checker";
            let (nd, dt, dp, err, mwp) = if virtual_cube {
                (0.40, 0.45, 0.46, -0.20, -0.05)
            } else {
                (0.41741030, 0.44279509, 0.41422868, -0.07683743, -0.00318162)
            };
            r.raw = json!({"equity":{"useMatchEquity":true,"cubeDecision":{"eqNoDouble":nd,"eqDoubleTake":dt,"eqDoublePass":dp}},"pr":{"cubeError":err,"cubeErrorMWP":mwp}});
            r.cube = Some(
                json!({"equity_kind":"unknown","source":if virtual_cube {r.raw["equity"]["cubeDecision"].clone()} else {r.raw["equity"].clone()}}),
            );
            let p = projection(&r, &s, "bgf").unwrap();
            assert_eq!(
                p.decision.best,
                if virtual_cube { "double" } else { "no double" }
            );
            assert!((p.decision.equity_loss + err).abs() < 1e-9);
            r.raw["equity"]["useMatchEquity"] = Value::Null;
            assert!(projection(&r, &s, "bgf").is_none());
        }
    }
    #[test]
    fn unranked_played_candidate_remains_unknown() {
        let (s, mut r) = checker();
        r.candidates[1].rank = 0;
        assert!(projection(&r, &s, "xg").is_none());
    }
    #[test]
    fn legacy_optional_context_uses_authoritative_source_without_weakening_present_fields() {
        let (s, mut r) = checker();
        r.position_id = None;
        r.gnubg_match_id = None;
        r.dice = None;
        let p = projection(&r, &s, "xg").unwrap();
        assert_eq!(p.position.position_id, s.position_id);
        assert_eq!(p.position.gnubg_match_id, s.match_id);
        assert_eq!(p.position.dice, s.dice);
        let (s, mut r) = fixture("(;FF[4]GM[6];W[31hefe])", "cube", 0);
        cube(&mut r, 0.4, 0.6, 1.0);
        let expected_id = r.gnubg_match_id.take().unwrap();
        r.position_id = None;
        r.dice = None;
        let p = projection(&r, &s, "xg").unwrap();
        assert_eq!(p.position.gnubg_match_id, expected_id);
        assert_eq!(p.position.dice, vec![0, 0]);
    }
}
