//! Sage chooses actions; GNUbg alone executes rules, rolls dice and scores games.
use super::{
    NativeRequest, NativeSnapshot, PlayAlternative, PlayMove, PlayPhase, PlaySession,
    rules_process::RulesProcess,
};
use crate::{engine::Engine, engines::SageEngine};
use anyhow::{Context, Result, ensure};

fn resulting_board(
    board: &[i32],
    player: i32,
    dice: &[i32],
    choice: &PlayMove,
) -> Result<[i32; 26]> {
    let before: [i32; 26] = board.try_into().context("Invalid legal-move board")?;
    let dice: [i32; 2] = dice.try_into().context("Invalid legal-move dice")?;
    let mut pairs = Vec::new();
    for step in &choice.steps {
        let point = |point: i32| -> Result<usize> {
            if point == -1 && player == 0 || point == 26 && player == 1 {
                return Ok(0);
            }
            let relative = if player == 0 { point } else { 25 - point };
            ensure!(
                (0..=25).contains(&relative),
                "Invalid native legal move coordinate"
            );
            Ok(relative as usize)
        };
        pairs.push((point(step.from)?, point(step.to)?));
    }
    let mut expanded = before;
    let steps = crate::import::sgf::expand_xg_play(&mut expanded, player, dice, &pairs)?;
    crate::import::sgf::apply_checker_turn(&before, player, dice, &steps)
}

async fn chosen_move(bot: &SageEngine, snapshot: &NativeSnapshot) -> Result<Option<String>> {
    let (position, context) = snapshot
        .native_id
        .split_once(':')
        .context("Invalid native ID")?;
    let candidates = bot.checker_boards(position, context, None, None).await?;
    let best = candidates
        .first()
        .context("Open Sage returned no chosen move")?;
    if snapshot.legal_moves.is_empty() {
        let before: [i32; 26] = snapshot
            .board
            .as_slice()
            .try_into()
            .context("Invalid native board")?;
        ensure!(
            best.board == before,
            "Open Sage did not confirm forced pass"
        );
        return Ok(None);
    }
    for choice in &snapshot.legal_moves {
        if resulting_board(&snapshot.board, snapshot.player, &snapshot.dice, choice)? == best.board
        {
            return Ok(Some(choice.notation.clone()));
        }
    }
    anyhow::bail!("Open Sage best move is absent from GNUbg legal results")
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
    crate::engine::with_shared_access(async {
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
            plies: 0,
            native_id: previous.map_or("", |(_, id)| id),
            action,
            notation,
            resign_level,
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
            let (position, context) = snapshot
                .native_id
                .split_once(':')
                .context("Invalid native ID")?;
            let mut notation = None;
            let action = match snapshot.phase {
                PlayPhase::Move => {
                    notation = chosen_move(bot, &snapshot).await?;
                    if notation.is_some() { "move" } else { "pass" }
                }
                PlayPhase::Roll => {
                    if snapshot.can_double {
                        let cube = bot.cube(position, context).await?;
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
                    let cube = bot.cube(position, context).await?;
                    crate::match_analysis::cube_options(
                        "take",
                        -cube.no_double,
                        -cube.take,
                        -cube.pass,
                    )
                    .context("Missing cube response options")?
                    .0
                }
                _ => anyhow::bail!("Unexpected Open Sage turn phase"),
            };
            let request = NativeRequest {
                new: false,
                external_bot: true,
                accept_resignation: None,
                human_player: session.human_player,
                match_length: session.match_length,
                plies: 0,
                native_id: &snapshot.native_id,
                action,
                notation: notation.as_deref(),
                resign_level: None,
            };
            (process, snapshot, sgf) = process.execute(&request).await?;
        }
        anyhow::bail!("Open Sage did not yield within the bounded action chain")
    })
    .await
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
            plies: 0,
            native_id: previous.1,
            action: "inspect",
            notation: None,
            resign_level: None,
        };
        let process = RulesProcess::start(rules, Some(previous.0)).await?;
        let (process, before, _) = process.execute(&request).await?;
        ensure!(
            before.player == bot_player && before.dice == dice,
            "Benchmark checkpoint differs from reference"
        );
        let notation = chosen_move(bot, &before).await?;
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
            "Open Sage did not complete exactly one checker turn"
        );
        process.finish().await?;
        Ok(())
    })
    .await
}

pub(super) async fn alternatives(
    session: &PlaySession,
    engine: &SageEngine,
) -> Result<Vec<PlayAlternative>> {
    if matches!(session.phase, PlayPhase::Move) {
        let candidates = engine
            .checker_boards(
                &session.position.position_id,
                &session.position.gnubg_match_id,
                None,
                None,
            )
            .await?;
        let best = candidates
            .first()
            .context("Open Sage omitted best checker move")?
            .equity;
        if session.legal_moves.is_empty() {
            ensure!(
                candidates.len() == 1 && candidates[0].notation == "Cannot move",
                "Open Sage omitted forced pass"
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
            let after = resulting_board(
                &session.position.board,
                session.position.player,
                &session.position.dice,
                choice,
            )?;
            let candidate = candidates
                .iter()
                .find(|candidate| candidate.board == after)
                .context("Open Sage omitted a legal checker result")?;
            alternatives.push(PlayAlternative {
                rating_decisions: 0,
                action: "move".into(),
                move_index: Some(index),
                notation: choice.notation.clone(),
                loss: ((best - candidate.equity) * 1e6).round().max(0.0) / 1e6,
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
            loss: ((best - equity) * 1e6).round().max(0.0) / 1e6,
            equity: Some(equity),
            probabilities: Some(probabilities.clone()),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires locked Open Sage and GNUbg with embedded Python; no database"]
    async fn sage_executes_reference_checker_turns_using_gnubg_rules() {
        let python = std::env::var("BGTUTOR_SAGE_PYTHON").unwrap();
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
    #[ignore = "requires GNUbg Python and locked BGTUTOR_SAGE_PYTHON"]
    async fn sage_reference_uses_one_rules_process_per_turn() {
        let python = std::env::var("BGTUTOR_SAGE_PYTHON").unwrap();
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
