use super::Assessment;
use crate::{
    game_plan::{self, GamePlan},
    import::ids::{Field, MatchId},
    match_analysis::evaluated_equities,
    play::PlaySession,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

#[derive(Serialize, Default)]
struct Counts {
    evaluated: usize,
    errors: usize,
    loss: f64,
    unavailable: usize,
    forced: usize,
}

#[derive(Serialize)]
struct Evidence<'a> {
    evidence_ref: String,
    game: i32,
    decision: i32,
    engine: &'a str,
    played: &'a str,
    best: &'a str,
    loss: f64,
    severity: &'static str,
    topics: Vec<&'static str>,
    context: Value,
    first_attempt: Option<Value>,
}

pub(super) fn build(session: &PlaySession, sgf: &str, language: &str) -> Result<Value> {
    let review = session.review.as_ref().context("Missing report analysis")?;
    let history = crate::play::build_history_snapshots(session, sgf)?;
    let attempts: BTreeMap<_, _> = session
        .decisions
        .iter()
        .filter(|decision| decision.committed && decision.original_action.is_some())
        .map(|decision| {
            (
                (
                    decision.game_no,
                    decision.history_index,
                    attempt_kind(decision),
                ),
                decision,
            )
        })
        .collect();
    let mut evidence = Vec::new();
    let mut category: BTreeMap<&str, Counts> = BTreeMap::new();
    let mut human_count = 0;
    let mut context_missing = 0;
    for game in &review.games {
        let events: Vec<_> = history
            .items
            .iter()
            .filter(|event| event.game_no == game.game_no && event.kind != "resign")
            .collect();
        let (mut event_index, mut checker_no, mut aligned) = (0, 0, true);
        for (index, decision) in game.decisions.iter().enumerate() {
            let no_double = decision.kind == "cube" && decision.played == "no double";
            if decision.kind == "checker" {
                checker_no += 1;
            }
            let event = events.get(event_index).filter(|event| {
                let before = &event.before;
                let dice_match = decision.dice.is_some_and(|dice| {
                    before.dice.as_slice() == dice || before.dice.as_slice() == [dice[1], dice[0]]
                });
                aligned
                    && decision.decision_no == index as i32 + 1
                    && before.player == decision.player
                    && match decision.kind.as_str() {
                        "checker" => {
                            event.kind == "checker"
                                && decision.checker_no == Some(checker_no)
                                && dice_match
                        }
                        "cube" if no_double => {
                            event.kind == "checker" && decision.checker_no == Some(checker_no + 1)
                        }
                        "cube" => {
                            event.kind == "cube"
                                && before.notation.eq_ignore_ascii_case(&decision.played)
                        }
                        _ => false,
                    }
            });
            aligned &= event.is_some();
            if !no_double {
                event_index += 1;
            }
            if decision.player != session.human_player {
                continue;
            }
            human_count += 1;
            let evaluated = evaluated_equities(decision).is_some();
            let severity = session
                .error_thresholds
                .classify(decision.equity_loss)
                .as_str();
            let mut topics = vec![if decision.kind == "checker" {
                "checker"
            } else {
                "cube"
            }];
            let mut context = Value::Null;
            let mut first_attempt = None;
            if let Some(event) = event {
                let position = &event.before;
                if let (Ok(board), Some(match_id)) = (
                    <&[i32; 26]>::try_from(position.board.as_slice()),
                    MatchId::decode(&position.gnubg_match_id),
                ) {
                    let plans = game_plan::classify(board).map_err(anyhow::Error::msg)?;
                    let (plan, opponent_plan) = if decision.player == 0 {
                        (plans.white, plans.black)
                    } else {
                        (plans.black, plans.white)
                    };
                    if matches!(plan, GamePlan::PureRace | GamePlan::Racing) {
                        topics.push("race");
                    }
                    let contact = plan != GamePlan::PureRace;
                    let (pips, _) = game_plan::absolute_pip_counts(board);
                    let player_backgame = backgame_candidate(board, pips, decision.player);
                    let opponent_backgame = backgame_candidate(board, pips, 1 - decision.player);
                    if contact && (player_backgame || opponent_backgame) {
                        topics.push("backgame");
                    }
                    if contact {
                        topics.push("contact");
                    }
                    if plan == GamePlan::Priming {
                        topics.push("prime");
                    }
                    if plan == GamePlan::Anchoring {
                        topics.push("anchor");
                    }
                    let home = if decision.player == 0 {
                        board[7..].iter().all(|n| *n <= 0)
                    } else {
                        board[..19].iter().all(|n| *n >= 0)
                    };
                    if decision.kind == "checker" {
                        if checker_no <= 3 {
                            topics.push("opening");
                        }
                        if home {
                            topics.push("bear_off");
                        }
                        // The saved native recommendation and actual play provide both
                        // sides of the comparison only when their hit markings differ.
                        if evaluated && hits(&decision.best) != hits(&decision.played) {
                            topics.push("hit");
                        }
                    }
                    let owner = match match_id.get(Field::CUBE_OWNER) {
                        0 => "white",
                        1 => "black",
                        _ => "centered",
                    };
                    let cube = 1 << match_id.get(Field::CUBE_LOG2);
                    let response = matches!(decision.played.as_str(), "take" | "drop");
                    context = json!({
                        "score_white_black": [match_id.get(Field::SCORE_0), match_id.get(Field::SCORE_1)],
                        "crawford": match_id.get(Field::CRAWFORD) == 1,
                        "cube_value_before_offer": cube, "cube_owner": owner,
                        "offered_cube": response.then_some(cube * 2),
                        "dice": decision.dice.filter(|_| decision.kind == "checker"),
                        "absolute_board": board.as_slice(), "pips_white_black": pips,
                        "game_plan": plan, "opponent_plan": opponent_plan, "contact": contact,
                        "can_bear_off": home, "checker_no": decision.checker_no,
                        "player_backgame_candidate": contact && player_backgame,
                        "opponent_backgame_candidate": contact && opponent_backgame,
                        "played_hits": (decision.kind == "checker").then(|| hits(&decision.played)),
                        "best_hits": (decision.kind == "checker").then(|| hits(&decision.best)),
                    });
                    let kind = if decision.kind == "checker" {
                        "checker"
                    } else if no_double {
                        "no_double"
                    } else {
                        &decision.played
                    };
                    if let Some(attempt) = attempts.get(&(game.game_no, event.index, kind))
                        && attempt.first_loss.is_some()
                    {
                        first_attempt = Some(json!({
                            "action": attempt.original_notation.as_deref()
                                .filter(|text| !text.is_empty()).or(attempt.original_action.as_deref()),
                            "loss": attempt.first_loss,
                            "assisted": attempt.attempts.first().map_or(attempt.assisted, |a| a.assisted),
                            "retries": attempt.retry_count,
                        }));
                    }
                }
            }
            if context.is_null() {
                context_missing += 1;
            }
            for topic in &topics {
                let counts = category.entry(topic).or_default();
                if evaluated {
                    counts.evaluated += 1;
                    counts.loss += decision.equity_loss;
                    counts.errors +=
                        usize::from(decision.equity_loss >= session.error_thresholds.questionable);
                } else if decision.evaluation_status == "forced" {
                    counts.forced += 1;
                } else {
                    counts.unavailable += 1;
                }
            }
            if evaluated {
                evidence.push(Evidence {
                    evidence_ref: format!("{}:{}", game.game_no, decision.decision_no),
                    game: game.game_no,
                    decision: decision.decision_no,
                    engine: &game.engine,
                    played: &decision.played,
                    best: &decision.best,
                    loss: decision.equity_loss,
                    severity,
                    topics,
                    context,
                    first_attempt,
                });
            }
        }
    }
    ensure!(!evidence.is_empty(), "No evaluated player decisions");
    let evaluated = evidence.len();
    evidence.sort_by(|a, b| {
        priority(b)
            .total_cmp(&priority(a))
            .then_with(|| a.game.cmp(&b.game))
            .then_with(|| a.decision.cmp(&b.decision))
    });
    let mut chosen = HashSet::new();
    for topic in [
        "cube", "checker", "opening", "bear_off", "hit", "backgame", "race", "prime", "contact",
        "anchor",
    ] {
        if let Some((index, _)) = evidence
            .iter()
            .enumerate()
            .find(|(_, item)| item.topics.contains(&topic))
        {
            chosen.insert(index);
        }
    }
    for topic in ["cube", "checker"] {
        if let Some((index, _)) = evidence
            .iter()
            .enumerate()
            .find(|(_, item)| item.topics.contains(&topic) && priority(item) == 0.0)
        {
            chosen.insert(index);
        }
    }
    for index in 0..evidence.len() {
        if chosen.len() >= 24 {
            break;
        }
        chosen.insert(index);
    }
    let selected: Vec<_> = evidence
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| chosen.contains(&index).then_some(item))
        .collect();
    Ok(json!({
        "output_language": language,
        "target_player": if session.human_player == 0 { "white" } else { "black" },
        "match_context": {
            "match_length": session.match_length, "final_score_white_black": session.scores,
            "play_mode": session.play_mode, "conceded": session.match_forfeited,
            "reached_games": session.game_no, "analyzed_games": review.games.len(),
        },
        "analysis": {
            "plies": review.analysis_plies, "assisted": review.assisted,
            "first_attempt_pr_on_reached_positions": review.first_attempt_rating,
            "first_attempt_pr_method": review.first_attempt_rating_method,
            "independent_first_attempts": review.independent_first_attempt_count,
            "assisted_first_attempts": review.assisted_first_attempt_count,
            "native_statistics_complete": review.games.iter().all(|game| game.statistics.is_some()),
        },
        "coverage": {
            "snapshot_decisions": human_count, "evaluated_decisions": evaluated,
            "decisions_without_aligned_position": context_missing, "examples": selected.len(),
            "backgame_membership": "candidate only: two or more anchors in the opponent home board while behind in pips",
            "hit_membership": "only known when saved best and played differ in hit notation; no denominator for all hit choices",
            "opening_membership": "first three checker plies of the game",
            "board_convention": "absolute, white positive moves 24 to 1, black negative moves 1 to 24; bars 25 and 0",
        },
        "assessment_basis": "played choices; separately labeled recorded first attempts",
        "category_summaries": category,
        "decision_evidence": selected,
    }))
}

// A skipped cube offer can share a history index with the following checker play.
fn attempt_kind(attempt: &crate::play::PlayDecision) -> &str {
    match attempt.action.as_deref().unwrap_or("") {
        "move" | "pass" => "checker",
        "roll" | "no_double" => "no_double",
        action => action,
    }
}

fn backgame_candidate(board: &[i32; 26], pips: [i32; 2], player: i32) -> bool {
    let behind = if player == 0 {
        pips[0] > pips[1]
    } else {
        pips[1] > pips[0]
    };
    let anchors = if player == 0 {
        board[19..25].iter().filter(|&&n| n >= 2).count()
    } else {
        board[1..7].iter().filter(|&&n| n <= -2).count()
    };
    behind && anchors >= 2
}

fn hits(notation: &str) -> bool {
    notation.contains('*')
}

fn priority(item: &Evidence<'_>) -> f64 {
    item.loss.max(
        item.first_attempt
            .as_ref()
            .and_then(|a| a["loss"].as_f64())
            .unwrap_or(0.0),
    )
}

pub(super) fn validate(report: &Assessment, input: &Value) -> bool {
    let Some(evidence) = input["decision_evidence"].as_array() else {
        return false;
    };
    let valid_ref = |reference: &String| {
        evidence
            .iter()
            .any(|entry| entry["evidence_ref"].as_str() == Some(reference.as_str()))
    };
    let text = |value: &str| !value.trim().is_empty() && value.len() <= 4_000;
    let unique: HashSet<_> = report.topics.iter().map(|topic| topic.topic).collect();
    let unique_decisions: HashSet<_> = report
        .key_decisions
        .iter()
        .map(|item| &item.evidence_ref)
        .collect();
    text(&report.summary)
        && text(&report.next_match_focus)
        && report.topics.len() == 10
        && unique.len() == 10
        && report.topics.iter().all(|item| {
            text(&item.observation)
                && item.evidence_refs.len() <= 3
                && item.evidence_refs.iter().all(|reference| {
                    evidence.iter().any(|entry| {
                        entry["evidence_ref"].as_str() == Some(reference.as_str())
                            && entry["topics"].as_array().is_some_and(|topics| {
                                topics
                                    .contains(&serde_json::to_value(item.topic).unwrap_or_default())
                            })
                    })
                })
        })
        && report.key_decisions.len() <= 3
        && unique_decisions.len() == report.key_decisions.len()
        && report.key_decisions.iter().all(|item| {
            valid_ref(&item.evidence_ref) && text(&item.explanation) && text(&item.next_question)
        })
        && report.training.len() <= 3
        && report.training.iter().all(|item| {
            text(&item.focus)
                && text(&item.exercise)
                && text(&item.success_criterion)
                && !item.evidence_refs.is_empty()
                && item.evidence_refs.len() <= 3
                && item.evidence_refs.iter().all(valid_ref)
        })
        && report.limitations.len() <= 5
        && report.limitations.iter().all(|value| text(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{match_analysis::decision, play::PlaySession};

    #[test]
    fn skipped_double_keeps_pre_roll_context_and_does_not_take_a_checker_attempt() {
        // An initial W move is followed by B's cube opportunity and checker move.
        let sgf = "(;GM[6]MI[length:7][game:0][ws:0][bs:0];W[31hefe];B[51lqqr])";
        let mut opening = decision(0, "checker", "8/5 6/5", "8/5 6/5", Some((0.1, 0.1)));
        opening.decision_no = 1;
        opening.checker_no = Some(1);
        opening.dice = Some([3, 1]);
        let mut cube = decision(1, "cube", "no double", "double", Some((0.1, 0.4)));
        cube.decision_no = 2;
        cube.checker_no = Some(2);
        let mut checker = decision(1, "checker", "13/8 8/7", "13/8 6/5", Some((0.1, 0.2)));
        checker.decision_no = 3;
        checker.checker_no = Some(2);
        checker.dice = Some([5, 1]);
        let id = uuid::Uuid::new_v4();
        let mut session: PlaySession = serde_json::from_value(json!({
            "id":id, "version":3, "human_player":1, "human_name":"Private player",
            "match_length":7, "bearoff_direction":"right",
            "engine":{"id":"gnubg","version":"test","plies":2},
            "phase":"match_over","scores":[7,0],"game_no":1,"winner":0,
            "match_forfeited":true,"can_double":false,"legal_moves":[],
            "position":{"id":id,"match_uuid":id,"ordinal":0,"game_no":1,"ply":0,
                "player":0,"dice":[],"board":vec![0; 26],"position_id":"","gnubg_match_id":"","notation":""},
            "history":[{"player":0,"dice":[3,1],"notation":"8/5 6/5"},
                {"player":1,"dice":[5,1],"notation":"13/8 8/7"}],
            "decisions":[{"id":uuid::Uuid::new_v4(),"game_no":1,"history_index":1,
                "original_action":"move","action":"move","first_loss":0.2,"loss":0.1,
                "warning":false,"revealed":false,"assisted":false,"retry_count":1,
                "alternatives":[],"attempts":[],"committed":true}],
            "review":{"analysis_plies":2,"assisted":true,"first_attempt_loss":0.5,
                "final_choice_loss":0.4,"first_attempt_count":2,"retry_count":1,
                "first_attempt_rating":null,"first_attempt_rating_decisions":0,
                "first_attempt_rating_method":"test","first_attempt_rating_profile":"test",
                "independent_first_attempt_count":2,"assisted_first_attempt_count":0,
                "games":[{"engine":"test","game_no":1,"decisions":[opening,cube,checker]}]
            }
        })).unwrap();
        let input = build(&session, sgf, "de").unwrap();
        assert_eq!(input["coverage"]["evaluated_decisions"], 2);
        let items = input["decision_evidence"].as_array().unwrap();
        let cube = items
            .iter()
            .find(|item| item["evidence_ref"] == "1:2")
            .unwrap();
        let checker = items
            .iter()
            .find(|item| item["evidence_ref"] == "1:3")
            .unwrap();
        assert_eq!(cube["played"], "no double");
        assert_eq!(cube["context"]["score_white_black"], json!([0, 0]));
        assert_eq!(cube["context"]["cube_value_before_offer"], 1);
        assert!(cube["context"]["dice"].is_null());
        assert!(cube["first_attempt"].is_null());
        assert_eq!(checker["context"]["dice"], json!([5, 1]));
        assert_eq!(checker["first_attempt"]["loss"], 0.2);
        assert!(!input.to_string().contains("Private player"));

        let mut cube_attempt = session.decisions[0].clone();
        cube_attempt.original_action = Some("double".into());
        cube_attempt.action = Some("roll".into());
        cube_attempt.first_loss = Some(0.3);
        session.decisions.insert(0, cube_attempt);
        let input = build(&session, sgf, "de").unwrap();
        let items = input["decision_evidence"].as_array().unwrap();
        assert!(
            items
                .iter()
                .any(|item| item["evidence_ref"] == "1:2" && item["first_attempt"]["loss"] == 0.3)
        );
        assert!(
            items
                .iter()
                .any(|item| item["evidence_ref"] == "1:3" && item["first_attempt"]["loss"] == 0.2)
        );
    }
}
