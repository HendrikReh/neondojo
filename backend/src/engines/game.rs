use super::SageEngine;
use crate::{
    import::sgf::parse_sgf_source_moves,
    match_analysis::{
        AnalyzedGame, decision, evaluated_cube_decision, game_count, requires_cube_analysis,
    },
};
use anyhow::{Context, Result, ensure};
use std::time::Duration;

impl SageEngine {
    pub async fn analyze_game(&self, sgf: &str, game_no: i32, plies: i32) -> Result<AnalyzedGame> {
        self.analyze_game_with_limit(sgf, game_no, plies, Some(Duration::from_secs(600)))
            .await
    }

    pub async fn analyze_game_with_limit(
        &self,
        sgf: &str,
        game_no: i32,
        plies: i32,
        limit: Option<Duration>,
    ) -> Result<AnalyzedGame> {
        self.check_depth(plies)?;
        ensure!(
            (1..=game_count(sgf)?).contains(&game_no),
            "Game number outside match"
        );
        crate::engine::with_shared_access(async {
            let work = async {
                let mut batch = super::process::VerifiedBatch::begin(&self.snapshot).await?;
                let game = self.game(sgf, game_no, Some(&mut batch)).await?;
                // Provisional decisions never leave this scope or reach persistence
                // until the final package/RECORD/model verification succeeds.
                batch.finish().await?;
                Ok(game)
            };
            match limit {
                Some(limit) => tokio::time::timeout(limit, work)
                    .await
                    .context("Open Sage game analysis timed out; active worker terminated")?,
                None => work.await,
            }
        })
        .await
    }

    async fn game(
        &self,
        sgf: &str,
        game_no: i32,
        mut batch: Option<&mut super::process::VerifiedBatch>,
    ) -> Result<AnalyzedGame> {
        let (imported, moves) = parse_sgf_source_moves(sgf)?;
        let mut decisions = Vec::new();
        let expected_checkers = imported
            .positions
            .iter()
            .filter(|position| position.game_no == game_no)
            .count();
        let mut analyzed_checkers = 0;
        for event in moves.iter().filter(|event| event.game_no == game_no) {
            if event.kind == "resign" {
                continue;
            }
            if event.kind == "checker" {
                let position = imported
                    .positions
                    .iter()
                    .find(|position| {
                        position.game_no == game_no && Some(position.ply) == event.checker_ply
                    })
                    .context("Missing imported checker context")?;
                if requires_cube_analysis(position)? {
                    let cube = self
                        .cube_with_batch(&event.position_id, &event.match_id, batch.as_deref_mut())
                        .await?;
                    let mut result = evaluated_cube_decision(
                        event.player,
                        "no double",
                        cube.no_double,
                        cube.take,
                        cube.pass,
                    )?;
                    if let Some(evaluation) = &mut result.cube_evaluation {
                        evaluation.probabilities = cube.probabilities.as_slice().try_into().ok();
                    }
                    result.checker_no = event.checker_ply;
                    decisions.push(result);
                }
                let after: [i32; 26] = event
                    .after_board
                    .clone()
                    .try_into()
                    .ok()
                    .context("Invalid played board")?;
                let candidates = self
                    .checker_boards_with_batch(
                        &event.position_id,
                        &event.match_id,
                        None,
                        Some(after),
                        batch.as_deref_mut(),
                    )
                    .await?;
                let best = candidates
                    .first()
                    .context("Open Sage omitted best checker move")?;
                let played = candidates
                    .iter()
                    .find(|candidate| candidate.board == after)
                    .context("Open Sage omitted played checker move")?;
                let forced = event.board == event.after_board;
                let mut result = decision(
                    event.player,
                    "checker",
                    &event.notation,
                    &best.notation,
                    (!forced).then_some((played.equity, best.equity)),
                );
                result.checker_no = event.checker_ply;
                result.dice = Some(event.dice);
                if !forced {
                    result.checker_candidates = self.ranked_candidates(candidates);
                }
                decisions.push(result);
                analyzed_checkers += 1;
            } else {
                let played = event.notation.to_ascii_lowercase();
                let cube = self
                    .cube_with_batch(&event.position_id, &event.match_id, batch.as_deref_mut())
                    .await?;
                let response = matches!(played.as_str(), "take" | "drop");
                let sign = if response { -1.0 } else { 1.0 };
                let mut result = evaluated_cube_decision(
                    event.player,
                    &played,
                    sign * cube.no_double,
                    sign * cube.take,
                    sign * cube.pass,
                )?;
                if let Some(evaluation) = &mut result.cube_evaluation {
                    let probabilities: Option<[f64; 6]> =
                        cube.probabilities.as_slice().try_into().ok();
                    evaluation.probabilities = if response {
                        probabilities.map(crate::engine::flip_probabilities)
                    } else {
                        probabilities
                    };
                }
                decisions.push(result);
            }
        }
        ensure!(
            analyzed_checkers == expected_checkers,
            "Incomplete Open Sage game coverage"
        );
        for (index, decision) in decisions.iter_mut().enumerate() {
            decision.decision_no = i32::try_from(index + 1)?;
        }
        Ok(AnalyzedGame {
            engine: self.version.clone(),
            game_no,
            decisions,
            statistics: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn response_uses_negated_original_offer_units() {
        let take = evaluated_cube_decision(1, "take", -1.0, -2.0, -1.0).unwrap();
        assert_eq!(take.best, "drop");
        assert_eq!(take.played_equity, Some(-2.0));
        assert_eq!(take.equity_loss, 1.0);
        let drop = evaluated_cube_decision(1, "drop", -1.0, -2.0, -1.0).unwrap();
        assert_eq!(drop.equity_loss, 0.0);
    }

    #[tokio::test]
    #[ignore = "requires locked native Open Sage; no database"]
    async fn native_opening_analysis_retains_all_ranked_checker_candidates() {
        let python = std::env::var("BGTUTOR_SAGE_PYTHON").unwrap();
        for plies in [2, 4] {
            let profile = SageEngine::probe_profile(python.clone(), plies, 4)
                .await
                .unwrap();
            let engine = SageEngine::new(profile).unwrap();
            let game = engine
                .analyze_game("(;GM[6]MI[length:21];B[63lolr])", 1, plies)
                .await
                .unwrap();
            let decision = &game.decisions[0];
            let candidates = &decision.checker_candidates;
            assert_eq!(candidates.len(), 14);
            assert_eq!(candidates[0].notation, decision.best);
            assert_eq!(Some(candidates[0].equity), decision.best_equity);
            assert!(
                candidates
                    .iter()
                    .any(|candidate| Some(candidate.equity) == decision.played_equity)
            );
            assert!(
                candidates
                    .iter()
                    .all(|candidate| candidate.plies == Some(plies)
                        && candidate.probabilities.len() == 6)
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires locked native Open Sage; compares all WC2014 decisions; no database"]
    async fn native_atomic_games_match_individual_requests() {
        let python = std::env::var("BGTUTOR_SAGE_PYTHON").unwrap();
        let engine = SageEngine::new(SageEngine::probe(python, 4).await.unwrap()).unwrap();
        let source = crate::benchmarks::REFERENCE_SGF;
        let mut checkers = 0;
        let mut cubes = 0;
        for game_no in 1..=10 {
            // Same authoritative game mapping, but the established ordinary
            // transport verifies each request before and after evaluation.
            let individual = engine.game(source, game_no, None).await.unwrap();
            let atomic = engine.analyze_game(source, game_no, 2).await.unwrap();
            assert_eq!(
                serde_json::to_value(&individual).unwrap(),
                serde_json::to_value(&atomic).unwrap(),
                "game {game_no}"
            );
            checkers += atomic
                .decisions
                .iter()
                .filter(|d| d.kind == "checker")
                .count();
            cubes += atomic.decisions.iter().filter(|d| d.kind == "cube").count();
        }
        assert_eq!((checkers, cubes), (460, 278));
    }
}
