use super::{EngineId, SageEngine, context, process};
use crate::{
    engine::{Analysis, Engine},
    match_analysis::AnalyzedGame,
};
use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use utoipa::ToSchema;

/// Measured engine work for one or more decisions, excluding process startup,
/// state replay and integrity checks. CPU time covers every thread of the
/// engine process (`getrusage(RUSAGE_SELF)`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EngineCost {
    pub cpu_us: u64,
    pub wall_us: u64,
}

impl std::ops::AddAssign for EngineCost {
    fn add_assign(&mut self, other: Self) {
        self.cpu_us = self.cpu_us.saturating_add(other.cpu_us);
        self.wall_us = self.wall_us.saturating_add(other.wall_us);
    }
}

pub(crate) fn validate_evaluation(equity: f64, probabilities: &[f64]) -> Result<()> {
    ensure!(
        equity.is_finite()
            && probabilities.len() == 6
            && probabilities
                .iter()
                .all(|p| p.is_finite() && (0.0..=1.0).contains(p)),
        "Invalid NeonGammon evaluation"
    );
    ensure!(
        (probabilities[0] + probabilities[3] - 1.0).abs() <= 1e-9
            && probabilities[2] <= probabilities[1]
            && probabilities[1] <= probabilities[0]
            && probabilities[5] <= probabilities[4]
            && probabilities[4] <= probabilities[3] + 1e-12,
        "Inconsistent NeonGammon probabilities"
    );
    Ok(())
}

/// Whether the player on roll cannot play a checker with `dice`, decided by
/// the rule validation that also checks recorded moves.
pub(crate) fn forced_pass(position_id: &str, match_id: &str, dice: [i32; 2]) -> Result<bool> {
    let player = context::Context::decode(match_id)?.player;
    let board: [i32; 26] = crate::backgammon::ids::decode_board(position_id, match_id)?
        .try_into()
        .ok()
        .context("Invalid board length")?;
    Ok(crate::backgammon::apply_checker_turn(&board, player, dice, &[]).is_ok())
}

/// A configured engine instance. Shared operations dispatch here; callers
/// that need an engine-specific implementation match exhaustively, so a new
/// variant is a compile error at each of them.
#[derive(Clone)]
pub enum AnalysisEngine {
    Gnubg(Engine),
    OpenSage(SageEngine),
}

impl AnalysisEngine {
    pub(crate) fn for_checker_rules(self, cube_enabled: bool, match_id: &str) -> Result<Self> {
        if cube_enabled {
            return Ok(self);
        }
        match self {
            Self::Gnubg(engine) => Ok(Self::Gnubg(engine.with_cubeless_checker())),
            Self::OpenSage(engine) => {
                let context = context::Context::decode(match_id)?;
                Ok(Self::OpenSage(engine.for_cubeless_checker(
                    context.away1 == 0 && !context.jacoby,
                )?))
            }
        }
    }

    pub(crate) fn id(&self) -> EngineId {
        match self {
            Self::Gnubg(_) => EngineId::Gnubg,
            Self::OpenSage(_) => EngineId::NeonGammon,
        }
    }

    /// The GNUbg rules engine of a saved rules snapshot; any other engine fails.
    pub(crate) fn into_rules_engine(self) -> Result<Engine> {
        match self {
            Self::Gnubg(engine) => Ok(engine),
            Self::OpenSage(_) => anyhow::bail!("Saved play rules engine must be GNUbg"),
        }
    }

    /// The same engine for an evaluation a person waits for. NeonGammon then
    /// uses its interactive worker, which no analysis holds for a whole game;
    /// GNUbg starts a process per evaluation anyway.
    pub(crate) fn interactive(self) -> Self {
        match self {
            Self::Gnubg(engine) => Self::Gnubg(engine.interactive()),
            Self::OpenSage(mut engine) => {
                engine.slot = process::Slot::Interactive;
                Self::OpenSage(engine)
            }
        }
    }

    /// The executable identity benchmarks are keyed by: GNUbg's resolved
    /// binary, or NeonGammon's absolute Python path as saved in its profile.
    pub(crate) async fn resolved_binary(&self) -> Result<String> {
        match self {
            Self::Gnubg(engine) => crate::engine::resolve_binary_async(engine.binary()).await,
            Self::OpenSage(engine) => Ok(engine.binary().to_owned()),
        }
    }

    /// Full installation check: a real position evaluation for GNUbg, the
    /// verified worker identity for NeonGammon.
    pub(crate) async fn check_installation(&self) -> Result<String> {
        match self {
            Self::Gnubg(engine) => engine.check_installation().await,
            Self::OpenSage(engine) => engine.verify().await,
        }
    }

    /// GNUbg's saved checker move filter; other engines have none.
    pub(crate) fn move_filter(&self) -> Option<crate::engine::GnubgMoveFilter> {
        match self {
            Self::Gnubg(engine) => engine.move_filter(),
            Self::OpenSage(_) => None,
        }
    }

    /// GNUbg's `set threads` count; NeonGammon threads belong to its profile.
    pub(crate) fn gnubg_threads(&self) -> Option<i32> {
        match self {
            Self::Gnubg(engine) => Some(engine.calculation_threads()),
            Self::OpenSage(_) => None,
        }
    }

    /// Restart a persistent worker so a benchmark measures a cold start.
    /// Returns false for engines that start a fresh process per request.
    pub(crate) async fn restart_persistent_worker(&self) -> Result<bool> {
        match self {
            Self::Gnubg(_) => Ok(false),
            Self::OpenSage(engine) => engine.restart().await.map(|()| true),
        }
    }

    /// Cube equities and chances from the doubler's view, with the engine
    /// name and depth that produced them.
    pub(crate) async fn cube_equities(
        &self,
        position_id: &str,
        match_id: &str,
        plies: i32,
    ) -> Result<(String, i32, crate::engine::CubeEquities)> {
        match self {
            Self::Gnubg(engine) => {
                engine
                    .analyze_cube_choices(position_id, match_id, plies)
                    .await
            }
            Self::OpenSage(engine) => {
                let cube = engine.cube(position_id, match_id).await?;
                let name = engine
                    .saved_version()
                    .context("NeonGammon did not report its version")?;
                Ok((
                    name.to_owned(),
                    plies,
                    crate::engine::CubeEquities {
                        no_double: cube.no_double,
                        take: cube.take,
                        pass: cube.pass,
                        probabilities: cube.probabilities.as_slice().try_into().ok(),
                    },
                ))
            }
        }
    }

    pub async fn version(&self) -> Result<String> {
        match self {
            Self::Gnubg(engine) => engine.version().await,
            Self::OpenSage(engine) => engine.version().await,
        }
    }

    pub fn saved_version(&self) -> Option<&str> {
        match self {
            Self::Gnubg(engine) => engine.saved_version(),
            Self::OpenSage(engine) => engine.saved_version(),
        }
    }

    pub fn binary(&self) -> &str {
        match self {
            Self::Gnubg(engine) => engine.binary(),
            Self::OpenSage(engine) => engine.binary(),
        }
    }

    pub async fn analyze(
        &self,
        position_id: &str,
        match_id: &str,
        plies: i32,
        dice: Option<[i32; 2]>,
    ) -> Result<Analysis> {
        match self {
            Self::Gnubg(engine) => engine.analyze(position_id, match_id, plies, dice).await,
            Self::OpenSage(engine) => engine.analyze(position_id, match_id, plies, dice).await,
        }
    }

    /// Evaluate the position once for each roll, in the given order, and
    /// report the number of evaluated rolls after each one.
    pub async fn analyze_rolls<F>(
        &self,
        position_id: &str,
        match_id: &str,
        plies: i32,
        rolls: &[[i32; 2]],
        evaluated: impl FnMut(usize) -> F,
    ) -> Result<Vec<Analysis>>
    where
        F: std::future::Future<Output = Result<()>>,
    {
        match self {
            Self::Gnubg(engine) => {
                engine
                    .analyze_rolls(position_id, match_id, plies, rolls, evaluated)
                    .await
            }
            Self::OpenSage(engine) => {
                engine
                    .analyze_rolls(position_id, match_id, plies, rolls, evaluated)
                    .await
            }
        }
    }

    pub async fn analyze_checker_choices(
        &self,
        position_id: &str,
        match_id: &str,
        plies: i32,
        forced_pass: bool,
    ) -> Result<Analysis> {
        match self {
            Self::Gnubg(engine) => {
                engine
                    .analyze_checker_choices(position_id, match_id, plies, forced_pass)
                    .await
            }
            Self::OpenSage(engine) => {
                engine
                    .analyze_checker_choices(position_id, match_id, plies, forced_pass)
                    .await
            }
        }
    }

    pub async fn analyze_game(&self, sgf: &str, game_no: i32, plies: i32) -> Result<AnalyzedGame> {
        match self {
            Self::Gnubg(engine) => engine.analyze_game(sgf, game_no, plies).await,
            Self::OpenSage(engine) => engine.analyze_game(sgf, game_no, plies).await,
        }
    }

    pub async fn analyze_game_with_limit(
        &self,
        sgf: &str,
        game_no: i32,
        plies: i32,
        timeout: Option<Duration>,
    ) -> Result<AnalyzedGame> {
        match self {
            Self::Gnubg(engine) => {
                engine
                    .analyze_game_with_limit(sgf, game_no, plies, timeout)
                    .await
            }
            Self::OpenSage(engine) => {
                engine
                    .analyze_game_with_limit(sgf, game_no, plies, timeout)
                    .await
            }
        }
    }
}

impl From<Engine> for AnalysisEngine {
    fn from(engine: Engine) -> Self {
        Self::Gnubg(engine)
    }
}

#[cfg(test)]
mod tests {
    use super::forced_pass;

    #[test]
    fn forced_passes_follow_the_rules() {
        assert!(!forced_pass("4HPwATDgc/ABMA", "cInlAAAAAAAE", [3, 1]).unwrap());
        assert!(forced_pass("Y54pATDg28EBYA", "cAlnAQAAAAAE", [6, 6]).unwrap());
        assert!(!forced_pass("Y54pATDg28EBYA", "cAlnAQAAAAAE", [2, 3]).unwrap());
    }
}
