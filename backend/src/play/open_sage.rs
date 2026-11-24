//! Sage chooses actions; GNUbg alone executes rules, rolls dice and scores games.
use super::{
    NativeRequest, NativeSnapshot, PlayAlternative, PlayPhase, PlaySession,
    rules_process::RulesProcess,
};
use crate::{
    engine::Engine,
    engines::{EngineCost, SageCheckerCandidate, SageDifficulty, SageEngine},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, time::Duration};

use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

// Session reads can arrive in bursts. Keep at most one speculative task
// pending, including while an analysis or benchmark holds the engine gate.
static WARMUP: Semaphore = Semaphore::const_new(1);
const TURN_TIMEOUT: Duration = Duration::from_secs(180);

pub(super) fn schedule_warmup(session: &PlaySession, shutdown: &CancellationToken) {
    if session.paused
        || session.money_session_ended
        || matches!(session.phase, PlayPhase::MatchOver)
    {
        return;
    }
    let Some(snapshot) = session
        .engine_snapshots
        .as_ref()
        .and_then(|snapshots| snapshots.bot.open_sage.as_ref())
    else {
        return;
    };
    let Ok(permit) = WARMUP.try_acquire() else {
        return;
    };
    let snapshot = snapshot.clone();
    let shutdown = shutdown.clone();
    tokio::spawn(async move {
        let _permit = permit;
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => {}
            result = async { SageEngine::for_play(snapshot)?.warm_play().await } => {
                if let Err(error) = result {
                    tracing::debug!(%error, "Could not warm NeonGammon for play");
                }
            }
        }
    });
}

/// The chosen move, Sage's candidates and the engine cost of the evaluation.
type CheckerChoice = (
    Option<String>,
    Vec<SageCheckerCandidate>,
    Option<EngineCost>,
);

async fn chosen_move(
    bot: &SageEngine,
    snapshot: &NativeSnapshot,
    top_two: bool,
) -> Result<CheckerChoice> {
    let (position, context) = snapshot
        .native_id
        .split_once(':')
        .context("Invalid native ID")?;
    let (mut candidates, cost) = if top_two {
        bot.top_two_checker_boards(position, context).await?
    } else {
        bot.checker_boards_costed(position, context, None, None)
            .await?
    };
    let best = candidates
        .first()
        .context("NeonGammon returned no chosen move")?;
    if snapshot.legal_moves.is_empty() {
        let before: [i32; 26] = snapshot
            .board
            .as_slice()
            .try_into()
            .context("Invalid native board")?;
        ensure!(
            best.board == before,
            "NeonGammon did not confirm forced pass"
        );
        return Ok((None, candidates, cost));
    }
    let mut legal = HashMap::new();
    for choice in &snapshot.legal_moves {
        let board =
            super::native_move_board(&snapshot.board, snapshot.player, &snapshot.dice, choice)?;
        if board == best.board {
            return Ok((Some(choice.notation.clone()), candidates, cost));
        }
        legal.entry(board).or_insert(choice.notation.as_str());
    }

    // Sage can stop at a winning bear-off before using all playable dice.
    // Use its highest-ranked native candidate only for this terminal discrepancy.
    let bore_off =
        |candidate: &SageCheckerCandidate| all_borne_off(&candidate.board, snapshot.player);
    let first_legal = candidates
        .iter()
        .find(|candidate| legal.contains_key(&candidate.board));
    ensure!(
        bore_off(best) && first_legal.is_some_and(bore_off),
        "NeonGammon best move is absent from GNUbg legal results for {} (player {}, dice {:?}): best {:?} -> {:?}; GNUbg has {} candidates",
        snapshot.native_id,
        snapshot.player,
        snapshot.dice,
        best.notation,
        best.board,
        legal.len()
    );
    // The first retained candidate and its notation must describe the played board.
    candidates.retain_mut(|candidate| {
        let Some(&notation) = legal.get(&candidate.board) else {
            return false;
        };
        candidate.notation = notation.to_owned();
        true
    });
    Ok((Some(candidates[0].notation.clone()), candidates, cost))
}

pub(super) struct ChosenAction {
    pub action: &'static str,
    pub notation: Option<String>,
    pub checker: Option<Vec<SageCheckerCandidate>>,
    /// Engine work for this decision, when the worker reported it.
    pub cost: Option<EngineCost>,
}

pub(super) async fn choose(
    bot: &SageEngine,
    snapshot: &NativeSnapshot,
    top_two: bool,
) -> Result<ChosenAction> {
    let (position, context) = snapshot
        .native_id
        .split_once(':')
        .context("Invalid native ID")?;
    let mut notation = None;
    let mut checker = None;
    // A roll without a cube option consults no engine and costs nothing.
    let mut cost = Some(EngineCost::default());
    let action = match snapshot.phase {
        PlayPhase::Move => {
            let (chosen, candidates, measured) = chosen_move(bot, snapshot, top_two).await?;
            cost = measured;
            notation = chosen;
            checker = Some(candidates);
            if notation.is_some() { "move" } else { "pass" }
        }
        PlayPhase::Roll => {
            if snapshot.can_double {
                let (cube, measured) = bot.cube_decision_costed(position, context).await?;
                cost = measured;
                let (best, _, _) = crate::match_analysis::cube_options(
                    "no double",
                    cube.no_double,
                    cube.take,
                    cube.pass,
                )
                .context("Missing cube offer options")?;
                if best == "double" { "double" } else { "roll" }
            } else {
                "roll"
            }
        }
        PlayPhase::DoubleResponse => {
            let (cube, measured) = bot.cube_decision_costed(position, context).await?;
            cost = measured;
            crate::match_analysis::cube_options("take", -cube.no_double, -cube.take, -cube.pass)
                .context("Missing cube response options")?
                .0
        }
        _ => anyhow::bail!("Unexpected NeonGammon turn phase"),
    };
    Ok(ChosenAction {
        action,
        notation,
        checker,
        cost,
    })
}

fn all_borne_off(board: &[i32; 26], player: i32) -> bool {
    let checker_sign = if player == 0 { 1 } else { -1 };
    board.iter().all(|point| point.signum() != checker_sign)
}

async fn choose_difficulty(
    bot: &SageEngine,
    snapshot: &NativeSnapshot,
    difficulty: SageDifficulty,
    seed: u64,
) -> Result<(&'static str, Option<String>)> {
    let (position, context) = snapshot
        .native_id
        .split_once(':')
        .context("Invalid native ID")?;
    let action = match snapshot.phase {
        PlayPhase::Move if snapshot.legal_moves.is_empty() => "pass",
        PlayPhase::Move => {
            let chosen = bot
                .opponent_checker_board(position, context, difficulty, seed)
                .await?;
            let mut winning = None;
            for choice in &snapshot.legal_moves {
                let board = super::native_move_board(
                    &snapshot.board,
                    snapshot.player,
                    &snapshot.dice,
                    choice,
                )?;
                if chosen == board {
                    return Ok(("move", Some(choice.notation.clone())));
                }
                if winning.is_none() && all_borne_off(&board, snapshot.player) {
                    winning = Some(choice);
                }
            }
            // A terminal NeonGammon play can stop before using all playable dice.
            if let Some(choice) = winning
                && all_borne_off(&chosen, snapshot.player)
            {
                return Ok(("move", Some(choice.notation.clone())));
            }
            anyhow::bail!("NeonGammon opponent selected an illegal checker board");
        }
        PlayPhase::Roll if !snapshot.can_double => "roll",
        PlayPhase::Roll => {
            let cube = bot
                .opponent_cube(position, context, difficulty, seed)
                .await?;
            if cube.should_double { "double" } else { "roll" }
        }
        PlayPhase::DoubleResponse => {
            let cube = bot
                .opponent_cube(position, context, difficulty, seed)
                .await?;
            if cube.should_take { "take" } else { "drop" }
        }
        _ => anyhow::bail!("Unexpected NeonGammon turn phase"),
    };
    Ok((action, None))
}

fn decision_seed(session: &PlaySession, snapshot: &NativeSnapshot) -> u64 {
    // The game checkpoint survives an engine restart or administrative revision.
    let mut hash = Sha256::new();
    hash.update(session.id.as_bytes());
    hash.update(snapshot.game_no.to_le_bytes());
    hash.update((snapshot.history.len() as u64).to_le_bytes());
    hash.update(snapshot.native_id.as_bytes());
    let mut seed = [0; 8];
    seed.copy_from_slice(&hash.finalize()[..8]);
    u64::from_le_bytes(seed)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn advance(
    session: &PlaySession,
    bot: &SageEngine,
    rules: &Engine,
    previous: Option<(&str, &str)>,
    action: &str,
    notation: Option<&str>,
    resign_level: Option<i32>,
) -> Result<(NativeSnapshot, String)> {
    crate::engine::with_shared_access_timeout(TURN_TIMEOUT, async {
        let accept_resignation = if action == "resign" {
            Some(
                bot.accepts_resignation(
                    &session.position.position_id,
                    &session.position.gnubg_match_id,
                    resign_level.context("Resignation level is missing")?,
                )
                .await?,
            )
        } else {
            None
        };
        let request = NativeRequest {
            new: previous.is_none(),
            external_bot: true,
            accept_resignation,
            human_player: session.human_player,
            match_length: session.match_length,
            plies: 1,
            native_id: previous.map_or("", |(_, id)| id),
            action,
            notation,
            resign_level,
            match_bots: None,
            cubeless: false,
        };
        let process = RulesProcess::start(rules, previous.map(|(sgf, _)| sgf)).await?;
        let (mut process, mut snapshot, mut sgf) = process.execute(&request).await?;
        if action == "opening_roll" || snapshot.resignation_declined {
            process.finish().await?;
            return Ok((snapshot, sgf));
        }
        // A pass can leave the bot on roll again after a blocked human turn, but
        // every completed checker turn ordinarily yields; bound the full chain.
        for _ in 0..32 {
            if snapshot.player == session.human_player
                || matches!(snapshot.phase, PlayPhase::GameOver | PlayPhase::MatchOver)
            {
                process.finish().await?;
                return Ok((snapshot, sgf));
            }
            let (action, notation) = if let Some(difficulty) = session.engine.difficulty {
                choose_difficulty(
                    bot,
                    &snapshot,
                    difficulty,
                    decision_seed(session, &snapshot),
                )
                .await?
            } else {
                let choice = choose(bot, &snapshot, session.engine.top_two).await?;
                (choice.action, choice.notation)
            };
            let request = NativeRequest {
                new: false,
                external_bot: true,
                accept_resignation: None,
                human_player: session.human_player,
                match_length: session.match_length,
                plies: 1,
                native_id: &snapshot.native_id,
                action,
                notation: notation.as_deref(),
                resign_level: None,
                match_bots: None,
                cubeless: false,
            };
            (process, snapshot, sgf) = process.execute(&request).await?;
        }
        anyhow::bail!("NeonGammon did not yield within the bounded action chain")
    })
    .await
    .context("NeonGammon play action timed out, including the wait for engine access")?
}

pub(super) async fn benchmark_checker_turn(
    bot: &SageEngine,
    rules: &Engine,
    previous: (&str, &str),
    bot_player: i32,
    dice: [i32; 2],
) -> Result<()> {
    crate::engine::with_shared_access(async {
        let mut request = NativeRequest {
            new: false,
            external_bot: true,
            accept_resignation: None,
            human_player: 1 - bot_player,
            match_length: 11,
            plies: 1,
            native_id: previous.1,
            action: "inspect",
            notation: None,
            resign_level: None,
            match_bots: None,
            cubeless: false,
        };
        let process = RulesProcess::start(rules, Some(previous.0)).await?;
        let (process, before, _) = process.execute(&request).await?;
        ensure!(
            before.player == bot_player && before.dice == dice,
            "Benchmark checkpoint differs from reference"
        );
        let (notation, _, _) = chosen_move(bot, &before, false).await?;
        request.action = if notation.is_some() { "move" } else { "pass" };
        request.notation = notation.as_deref();
        request.native_id = &before.native_id;
        let (process, after, _) = process.execute(&request).await?;
        ensure!(
            after.history.len() == before.history.len() + 1
                && after
                    .history
                    .last()
                    .is_some_and(|turn| turn.player == bot_player && turn.dice == dice),
            "NeonGammon did not complete exactly one checker turn"
        );
        process.finish().await?;
        Ok(())
    })
    .await
}

/// NeonGammon feedback alternatives. `rating_decisions` stays 0: it only
/// corrects GNUbg's native game statistics (unforced moves, close cube
/// decisions) for first attempts, and NeonGammon game analysis has no such
/// statistics, so its play reviews report no first-attempt PR.
pub(super) async fn alternatives(
    session: &PlaySession,
    engine: &SageEngine,
) -> Result<Vec<PlayAlternative>> {
    if matches!(session.phase, PlayPhase::Move) {
        let candidates = engine
            .all_legal_checker_boards(
                &session.position.position_id,
                &session.position.gnubg_match_id,
            )
            .await?;
        let best = candidates
            .first()
            .context("NeonGammon omitted best checker move")?
            .equity;
        if session.legal_moves.is_empty() {
            ensure!(
                candidates.len() == 1 && candidates[0].notation == "Cannot move",
                "NeonGammon omitted forced pass"
            );
            return Ok(vec![PlayAlternative {
                rating_decisions: 0,
                action: "pass".into(),
                move_index: None,
                notation: "pass".into(),
                loss: 0.0,
                equity: None,
                probabilities: None,
            }]);
        }
        let mut alternatives = Vec::new();
        for (index, choice) in session.legal_moves.iter().enumerate() {
            let after = super::native_move_board(
                &session.position.board,
                session.position.player,
                &session.position.dice,
                choice,
            )?;
            let candidate = candidates
                .iter()
                .find(|candidate| candidate.board == after)
                .context("NeonGammon omitted a legal checker result")?;
            alternatives.push(PlayAlternative {
                rating_decisions: 0,
                action: "move".into(),
                move_index: Some(index),
                notation: choice.notation.clone(),
                loss: crate::match_analysis::normalized_loss(candidate.equity, best),
                equity: Some(candidate.equity),
                probabilities: Some(candidate.probabilities.clone()),
            });
        }
        alternatives.sort_by(|a, b| a.loss.total_cmp(&b.loss));
        return Ok(alternatives);
    }
    let cube = engine
        .cube(
            &session.position.position_id,
            &session.position.gnubg_match_id,
        )
        .await?;
    let response = matches!(session.phase, PlayPhase::DoubleResponse);
    let values = if response {
        [("take", -cube.take), ("drop", -cube.pass)]
    } else {
        [
            ("roll", cube.no_double),
            ("double", cube.take.min(cube.pass)),
        ]
    };
    let probabilities = if response {
        [&cube.probabilities[3..6], &cube.probabilities[0..3]].concat()
    } else {
        cube.probabilities
    };
    let best = values
        .iter()
        .map(|(_, equity)| *equity)
        .fold(f64::NEG_INFINITY, f64::max);
    Ok(values
        .into_iter()
        .map(|(action, equity)| PlayAlternative {
            rating_decisions: 0,
            action: action.into(),
            move_index: None,
            notation: if action == "roll" {
                "No double".into()
            } else {
                action.into()
            },
            loss: crate::match_analysis::normalized_loss(equity, best),
            equity: Some(equity),
            probabilities: Some(probabilities.clone()),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires locked NeonGammon; no database"]
    async fn full_strength_uses_pinned_checker_depth_and_two_ply_cube() {
        use crate::backgammon::ids::{match_id, position_id};

        let python = std::env::var("NEONDOJO_SAGE_PYTHON").unwrap();
        let mut profile = SageEngine::probe_profile(python, 2, 2).await.unwrap();
        let opening = [
            0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0, 2, 0,
        ];
        let old_move = [
            0, -2, 0, 0, 0, 1, 4, 0, 3, 0, 0, 1, -5, 4, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0, 2, 0,
        ];
        let new_move = [
            0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 1, -5, 4, 0, 0, 0, -3, 0, -5, 0, 0, 0, 1, 1, 0,
        ];
        // At 5-away/4-away, 2-ply doubles and 3-ply waits in this position.
        let cube_board = [
            0, -1, 0, 2, -1, 0, 4, 2, 2, 0, 0, 0, -4, 3, 0, 0, 0, -3, 2, -4, -2, 0, 0, 0, 0, 0,
        ];
        let checker_position = position_id(&opening, 0);
        let checker_context = match_id(5, [0, 0], 1, 3, false, false, 0, [2, 1]);
        let cube_position = position_id(&cube_board, 0);
        let cube_context = match_id(5, [0, 1], 1, 3, false, false, 0, [0, 0]);
        for (plies, expected_move) in [(2, old_move), (3, new_move)] {
            profile.plies = plies;
            let bot = SageEngine::for_play(profile.clone()).unwrap();
            let chosen = bot
                .opponent_checker_board(
                    &checker_position,
                    &checker_context,
                    SageDifficulty::FullStrength,
                    42,
                )
                .await
                .unwrap();
            assert_eq!(chosen, expected_move);
            let cube = bot
                .opponent_cube(
                    &cube_position,
                    &cube_context,
                    SageDifficulty::FullStrength,
                    42,
                )
                .await
                .unwrap();
            assert!(cube.should_double && cube.should_take);
        }
    }

    #[tokio::test]
    #[ignore = "requires locked NeonGammon; no database"]
    async fn bot_cube_decision_equals_the_full_cube_evaluation() {
        use crate::backgammon::ids::{match_id, position_id};

        let python = std::env::var("NEONDOJO_SAGE_PYTHON").unwrap();
        let mut profile = SageEngine::probe_profile(python, 2, 2).await.unwrap();
        // The 5-away/4-away contact position of the Full Strength test.
        let cube_board = [
            0, -1, 0, 2, -1, 0, 4, 2, 2, 0, 0, 0, -4, 3, 0, 0, 0, -3, 2, -4, -2, 0, 0, 0, 0, 0,
        ];
        let position = position_id(&cube_board, 0);
        let offer = match_id(5, [0, 1], 1, 3, false, false, 0, [0, 0]);
        for plies in [2, 3] {
            profile.plies = plies;
            let bot = SageEngine::for_play(profile.clone()).unwrap();
            // Warm the worker so neither side pays its start-up.
            bot.cube(&position, &offer).await.unwrap();
            let (mut full_us, mut decision_us) = (Vec::new(), Vec::new());
            for _ in 0..5 {
                let started = std::time::Instant::now();
                let full = bot.cube(&position, &offer).await.unwrap();
                full_us.push(started.elapsed().as_micros());
                let started = std::time::Instant::now();
                let (decision, _) = bot.cube_decision_costed(&position, &offer).await.unwrap();
                decision_us.push(started.elapsed().as_micros());
                for (alone, evaluated) in [
                    (decision.no_double, full.no_double),
                    (decision.take, full.take),
                    (decision.pass, full.pass),
                ] {
                    assert!((alone - evaluated).abs() < 1e-9, "{plies}-ply");
                }
            }
            full_us.sort_unstable();
            decision_us.sort_unstable();
            eprintln!(
                "{plies}-ply cube request median: full {} us, decision only {} us",
                full_us[2], decision_us[2]
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires locked NeonGammon and GNUbg with embedded Python; no database"]
    async fn sage_executes_reference_checker_turns_using_gnubg_rules() {
        let python = std::env::var("NEONDOJO_SAGE_PYTHON").unwrap();
        let bot = SageEngine::new(SageEngine::probe(python, 2).await.unwrap()).unwrap();
        let rules = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let turns: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../benchmarks/play_turns_v1.json")).unwrap();
        for turn in turns {
            benchmark_checker_turn(
                &bot,
                &rules,
                (
                    turn["sgf"].as_str().unwrap(),
                    turn["native_id"].as_str().unwrap(),
                ),
                turn["bot_player"].as_i64().unwrap() as i32,
                serde_json::from_value(turn["dice"].clone()).unwrap(),
            )
            .await
            .unwrap();
        }
    }
}

#[cfg(test)]
mod reuse_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    #[ignore = "requires GNUbg Python and locked NEONDOJO_SAGE_PYTHON"]
    async fn sage_reference_uses_one_rules_process_per_turn() {
        let python = std::env::var("NEONDOJO_SAGE_PYTHON").unwrap();
        let bot = SageEngine::new(SageEngine::probe(python, 2).await.unwrap()).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let binary =
            crate::engine::resolve_binary(&std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()))
                .unwrap();
        let counter = directory.path().join("starts");
        let wrapper = directory.path().join("gnubg-wrapper");
        let script = format!(
            "#!/usr/bin/env python3\nimport os,sys\nif '-c' in sys.argv:\n with open({},'a') as f: f.write('start\\n')\nbinary={}\nos.execv(binary,[binary,*sys.argv[1:]])\n",
            serde_json::to_string(counter.to_str().unwrap()).unwrap(),
            serde_json::to_string(&binary).unwrap()
        );
        std::fs::write(&wrapper, script).unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let rules = Engine::new(wrapper.to_str().unwrap().into());
        let turns: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../benchmarks/play_turns_v1.json")).unwrap();
        for turn in &turns {
            benchmark_checker_turn(
                &bot,
                &rules,
                (
                    turn["sgf"].as_str().unwrap(),
                    turn["native_id"].as_str().unwrap(),
                ),
                turn["bot_player"].as_i64().unwrap() as i32,
                serde_json::from_value(turn["dice"].clone()).unwrap(),
            )
            .await
            .unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(counter).unwrap().lines().count(),
            turns.len(),
            "Each Sage checker turn must retain one GNUbg process for inspection and execution"
        );
    }
}
