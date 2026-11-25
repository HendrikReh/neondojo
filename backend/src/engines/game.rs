use super::{
    SageEngine,
    replay::{PreparedGame, PreparedMatch},
};
use crate::match_analysis::{
    AnalyzedGame, decision, evaluated_cube_decision, requires_cube_analysis,
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
        let source = sgf.to_owned();
        let replay = tokio::task::spawn_blocking(move || PreparedMatch::parse(&source)).await??;
        self.analyze_prepared_game(&replay, game_no, plies, limit)
            .await
    }

    pub(crate) async fn analyze_prepared_game(
        &self,
        replay: &PreparedMatch,
        game_no: i32,
        plies: i32,
        limit: Option<Duration>,
    ) -> Result<AnalyzedGame> {
        self.check_depth(plies)?;
        let game = replay.game(game_no)?;
        let cubeless_engine;
        let evaluator = if game.cube_enabled {
            ensure!(
                !self.snapshot.cubeless,
                "Cubeless profile cannot analyze a cube-enabled game"
            );
            self
        } else {
            ensure!(
                game.supports_cubeless_money(),
                "NeonGammon cubeless checker evaluation requires money play without Jacoby"
            );
            let mut snapshot = self.snapshot.clone();
            snapshot.cubeless = true;
            cubeless_engine = SageEngine::new(snapshot)?;
            &cubeless_engine
        };
        crate::engine::with_shared_access(async {
            let work = async {
                let mut batch = super::process::VerifiedBatch::begin(&evaluator.snapshot).await?;
                let game = evaluator.game(game, Some(&mut batch)).await?;
                // Provisional decisions never leave this scope or reach persistence
                // until the final package/RECORD/model verification succeeds.
                batch.finish().await?;
                Ok(game)
            };
            match limit {
                Some(limit) => tokio::time::timeout(limit, work)
                    .await
                    .context("NeonGammon game analysis timed out; active worker terminated")?,
                None => work.await,
            }
        })
        .await
    }

    async fn game(
        &self,
        game: &PreparedGame,
        mut batch: Option<&mut super::process::VerifiedBatch>,
    ) -> Result<AnalyzedGame> {
        use super::{CheckerScope, CubeScope, SageCubeEvaluation, context, cube_request};
        // Record the SGF decision order while collecting independent engine rows.
        let mut order = Vec::new();
        let mut checkers = Vec::new();
        let mut checker_players = Vec::new();
        let mut cubes = Vec::new();
        for event in &game.events {
            if event.kind == "resign" {
                continue;
            }
            let checker = event.kind == "checker";
            let needs_cube = if checker {
                requires_cube_analysis(game.position(event)?)?
            } else {
                ensure!(game.cube_enabled, "Unexpected cube action in cubeless game");
                true
            };
            if needs_cube {
                let ctx = context::Context::decode(&event.match_id)?;
                cubes.push(cube_request(
                    &ctx.board(&event.position_id, &event.match_id)?,
                    &ctx,
                    CubeScope::Evaluation,
                ));
                order.push((event, None));
            }
            if checker {
                let after = event
                    .after_board
                    .as_slice()
                    .try_into()
                    .context("Invalid played board")?;
                let (row, player) = Self::checker_request(
                    &event.position_id,
                    &event.match_id,
                    None,
                    CheckerScope::Played(after),
                )?;
                checkers.push(row);
                checker_players.push(player);
                order.push((event, Some(after)));
            }
        }
        ensure!(
            checkers.len() == game.positions.len(),
            "Incomplete NeonGammon game coverage"
        );
        let mut checker_results = self
            .evaluate_rows("checker_batch", &checkers, batch.as_deref_mut())
            .await?
            .into_iter()
            .zip(checker_players);
        let mut cube_results = self
            .evaluate_rows("cube_batch_nply", &cubes, batch)
            .await?
            .into_iter();
        let mut decisions = Vec::with_capacity(order.len());
        for (event, checker) in order {
            if let Some(after) = checker {
                let (row, player) = checker_results
                    .next()
                    .context("Missing NeonGammon checker row")?;
                let candidates = Self::absolute_candidates(row, player)?;
                let best = candidates
                    .first()
                    .context("NeonGammon omitted best checker move")?;
                let played = candidates
                    .iter()
                    .find(|candidate| candidate.board == after)
                    .context("NeonGammon omitted played checker move")?;
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
            } else {
                let cube: SageCubeEvaluation = serde_json::from_value(
                    cube_results.next().context("Missing NeonGammon cube row")?,
                )?;
                let played = if event.kind == "checker" {
                    "no double".into()
                } else {
                    event.notation.to_ascii_lowercase()
                };
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
                if event.kind == "checker" {
                    result.checker_no = event.checker_ply;
                }
                decisions.push(result);
            }
        }
        for (index, decision) in decisions.iter_mut().enumerate() {
            decision.decision_no = i32::try_from(index + 1)?;
        }
        Ok(AnalyzedGame {
            engine: self.version.clone(),
            game_no: game.game_no,
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
    #[ignore = "requires locked native NeonGammon; no database"]
    async fn native_opening_analysis_retains_all_ranked_checker_candidates() {
        let python = std::env::var("NEONDOJO_SAGE_PYTHON").unwrap();
        // Cubeful 2-ply searches only the move filter's survivors (plus the
        // played move) at full depth and drops the 1-ply rest; deeper levels
        // search every legal move. Counts are for the pinned NeonGammon 3.0.0.
        for (plies, expected) in [(2, 5), (4, 14)] {
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
            assert_eq!(candidates.len(), expected, "{plies}-ply");
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
    #[ignore = "requires locked native NeonGammon; compares all WC2014 decisions; no database"]
    async fn native_atomic_games_match_individual_requests() {
        let python = std::env::var("NEONDOJO_SAGE_PYTHON").unwrap();
        let engine = SageEngine::new(SageEngine::probe(python, 4).await.unwrap()).unwrap();
        let source = crate::benchmarks::REFERENCE_SGF;
        let replay = PreparedMatch::parse(source).unwrap();
        let mut checkers = 0;
        let mut cubes = 0;
        for game_no in 1..=10 {
            // Same authoritative game mapping, but the established ordinary
            // transport verifies each request before and after evaluation.
            let individual = engine
                .game(replay.game(game_no).unwrap(), None)
                .await
                .unwrap();
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
