//! Native GNUbg game analysis, preserving the complete match context.
use crate::engine::{Candidate, Engine, parse_checker_candidates};
use crate::import::{
    ImportedMatch, ImportedPosition,
    sgf::{GameContext, Node, parse_importable_sgf_with_context, parse_trees, write_property},
};
use anyhow::{Context, Result, ensure};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tokio::io::AsyncReadExt;
use utoipa::ToSchema;

mod export_parse;
mod native;
use export_parse::*;
pub(crate) use native::retain_hint_paths;
use native::*;

pub const PERFORMANCE_RATING_METHOD: &str = "gnubg_emg_500_v1";

pub(crate) fn performance_rating_value(loss: f64, decisions: i64) -> Option<f64> {
    (decisions > 0).then(|| 500.0 * loss / decisions as f64)
}

pub(crate) fn performance_rating_profile(engine: &str, plies: i32) -> String {
    format!("{engine}; {plies}-ply cubeful deterministic")
}

/// Return a whole-result rating only with complete native statistical coverage.
/// Callers selecting match overview values must also require a completed,
/// whole-match job (game_no IS NULL); a scoped game's result has the same format.
pub fn match_performance_rating(
    result: &serde_json::Value,
) -> Option<crate::model::PerformanceRatingSummary> {
    let summary: crate::model::MatchAnalysisSummary =
        serde_json::from_value(result.clone()).ok()?;
    let rating = summary.performance_rating?;
    if summary.total_games <= 0
        || summary.completed_games != summary.total_games
        || summary.analyzed_moves != summary.total_moves
        || summary.players.iter().any(|p| p.unavailable != 0)
        || rating.games != summary.total_games
        || rating.method != PERFORMANCE_RATING_METHOD
        || rating.profile != performance_rating_profile(&summary.engine, summary.plies)
    {
        return None;
    }
    for (index, player) in rating.players.iter().enumerate() {
        if player.player != index as i32
            || player.unforced_checker_moves < 0
            || player.close_cube_decisions < 0
            || !player.normalized_equity_loss.is_finite()
            || player.normalized_equity_loss < 0.0
        {
            return None;
        }
        let decisions = player
            .unforced_checker_moves
            .checked_add(player.close_cube_decisions)?;
        let expected =
            performance_rating_value(player.normalized_equity_loss, i64::from(decisions));
        match (player.value, expected) {
            (Some(value), Some(expected))
                if value.is_finite() && (value - expected).abs() <= 1e-9 => {}
            (None, None) => {}
            _ => return None,
        }
    }
    Some(rating)
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AnalyzedGame {
    pub engine: String,
    pub game_no: i32,
    pub decisions: Vec<Decision>,
    /// Authoritative native statistics. Absent if the export cannot prove full coverage.
    #[serde(default)]
    pub statistics: Option<[NativePlayerStatistics; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NativePlayerStatistics {
    pub total_checker_moves: i32,
    pub total_cube_decisions: i32,
    pub unforced_checker_moves: i32,
    pub close_cube_decisions: i32,
    /// GNUbg's combined normalized Error total EMG, exported to six decimals.
    pub normalized_equity_loss: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CubeEquityUnit {
    Cubeful,
    Mwc,
}

/// Recorded cube outcomes, from the player making the decision.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CubeEvaluation {
    pub unit: CubeEquityUnit,
    pub no_double: f64,
    pub double_take: f64,
    pub double_pass: f64,
    /// Cubeless win, gammon and backgammon chances followed by the three
    /// losing chances, for the same player. Absent in older checkpoints and
    /// for sources that do not record them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(min_items = 6, max_items = 6)]
    pub probabilities: Option<[f64; 6]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Decision {
    /// One-based sequence of all checker and cube decisions within this game.
    pub decision_no: i32,
    /// One-based imported checker position; also links its preceding no-double decision.
    /// Actual double/take/drop decisions have no checker number.
    pub checker_no: Option<i32>,
    /// Absolute color: 0 is White, 1 is Black.
    pub player: i32,
    /// Decision kind: "checker" or "cube".
    pub kind: String,
    /// Actual GNUbg checker notation, or "no double", "double", "take", "drop".
    pub played: String,
    /// Best checker notation or cube action under the native evaluation.
    pub best: String,
    /// Normalized cubeful equity of the actual decision, from this player's perspective.
    /// Cube equities use the cube value before an offer, including for its response.
    pub played_equity: Option<f64>,
    /// Best normalized cubeful equity from this player's perspective, in the same units.
    pub best_equity: Option<f64>,
    /// Nonnegative best-minus-played equity, rounded to GNUbg's six exported decimals.
    pub equity_loss: f64,
    /// Saved decision severity. Analysis views can reclassify the stored equity loss.
    pub severity: String,
    /// The original imported dice for checker decisions; absent for cube decisions.
    pub dice: Option<[i32; 2]>,
    /// "evaluated" or "forced". A forced dance has null equities and zero loss;
    /// missing analysis for any evaluable decision fails the game analysis.
    pub evaluation_status: String,
    /// Ranked alternatives saved by local game and match analyses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checker_candidates: Vec<Candidate>,
    /// Saved outcomes. Older checkpoints may only contain played and best equity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cube_evaluation: Option<CubeEvaluation>,
}

pub fn game_count(sgf: &str) -> Result<i32> {
    tree_game_count(&crate::import::sgf::parse_trees(sgf)?)
}

fn tree_game_count(trees: &[Vec<Node>]) -> Result<i32> {
    for tree in trees {
        anyhow::ensure!(
            tree[0].get("GM").is_some_and(|v| v.as_slice() == ["6"]),
            "Only backgammon SGF is supported"
        );
    }
    Ok(i32::try_from(trees.len())?)
}

/// GNUbg exports six decimals. Use the same precision for classification and
/// filter boundaries instead of exposing binary subtraction noise.
pub(crate) fn rounded_equity_difference(played: f64, best: f64) -> f64 {
    ((best - played) * 1_000_000.0).round() / 1_000_000.0
}

pub(crate) fn normalized_loss(played: f64, best: f64) -> f64 {
    rounded_equity_difference(played, best).max(0.0)
}

/// Check that a saved native decision contains a consistent pair of equities.
pub(crate) fn evaluated_equities(d: &Decision) -> Option<(f64, f64)> {
    let (played, best) = d.played_equity.zip(d.best_equity)?;
    (d.evaluation_status == "evaluated"
        && (0..=1).contains(&d.player)
        && matches!(d.kind.as_str(), "checker" | "cube")
        && !d.best.trim().is_empty()
        && !d.played.trim().is_empty()
        && d.equity_loss.is_finite()
        && d.equity_loss >= 0.0
        && played.is_finite()
        && best.is_finite()
        && best >= played - 0.000001
        && (d.equity_loss - (best - played).max(0.0)).abs() <= 0.000001000001)
        .then_some((played, best))
}

pub(crate) fn decision(
    player: i32,
    kind: &str,
    played: &str,
    best: &str,
    equities: Option<(f64, f64)>,
) -> Decision {
    let loss = equities.map_or(0.0, |(played, best)| normalized_loss(played, best));
    Decision {
        decision_no: 0,
        checker_no: None,
        player,
        kind: kind.into(),
        played: played.into(),
        best: best.into(),
        played_equity: equities.map(|e| e.0),
        best_equity: equities.map(|e| e.1),
        equity_loss: loss,
        severity: crate::configuration::ErrorThresholds::default()
            .classify(loss)
            .as_str()
            .into(),
        dice: None,
        evaluation_status: if equities.is_some() {
            "evaluated"
        } else {
            "forced"
        }
        .into(),
        checker_candidates: vec![],
        cube_evaluation: None,
    }
}

/// The response values are already in the responder's perspective here.
pub(crate) fn cube_options(
    played: &str,
    nd: f64,
    dt: f64,
    dp: f64,
) -> Option<(&'static str, f64, f64)> {
    match played {
        "take" | "drop" => Some((
            if dt >= dp { "take" } else { "drop" },
            if played == "take" { dt } else { dp },
            dt.max(dp),
        )),
        "no double" | "double" => {
            let double = dt.min(dp);
            Some((
                if nd >= double { "no double" } else { "double" },
                if played == "double" { double } else { nd },
                nd.max(double),
            ))
        }
        _ => None,
    }
}

pub(crate) fn evaluated_cube_decision(
    player: i32,
    played: &str,
    no_double: f64,
    double_take: f64,
    double_pass: f64,
) -> Result<Decision> {
    let (best, actual, optimal) = cube_options(played, no_double, double_take, double_pass)
        .context("Unsupported native cube decision")?;
    let mut result = decision(player, "cube", played, best, Some((actual, optimal)));
    result.cube_evaluation = Some(CubeEvaluation {
        unit: CubeEquityUnit::Cubeful,
        no_double,
        double_take,
        double_pass,
        probabilities: None,
    });
    Ok(result)
}

/// GNUbg skips the standard opening board. Custom positions can require cube
/// analysis even as the first recorded move. GetDPEq then checks score,
/// ownership and Crawford.
/// https://sources.debian.org/src/gnubg/1.05.002-2/analysis.c/
/// https://sources.debian.org/src/gnubg/1.05.002-2/eval.c/
pub(crate) fn requires_cube_analysis(position: &crate::import::ImportedPosition) -> Result<bool> {
    if !position.cube_enabled {
        return Ok(false);
    }
    cube_analysis_available(&position.board, &position.match_id, position.player)
}

pub(crate) fn cube_analysis_available(board: &[i32], match_id: &str, player: i32) -> Result<bool> {
    if board == crate::backgammon::initial_board() {
        return Ok(false);
    }
    use crate::backgammon::ids::{Field, MatchId};
    let id = MatchId::decode(match_id).context("Invalid imported Match ID")?;
    let owner = id.get(Field::CUBE_OWNER);
    if id.get(Field::CRAWFORD) != 0 || (owner != 3 && owner != player) {
        return Ok(false);
    }
    let length = id.get(Field::LENGTH);
    let score = id.get(if player == 0 {
        Field::SCORE_0
    } else {
        Field::SCORE_1
    });
    let cube = 1 << id.get(Field::CUBE_LOG2);
    Ok(length == 0 || score + cube < length)
}

/// Aggregate one validated decision consistently for native and imported statistics.
pub(crate) fn accumulate_decision(
    players: &mut [crate::model::PlayerAnalysisSummary; 2],
    decision: &crate::match_analysis::Decision,
) -> Result<()> {
    let player = players
        .get_mut(decision.player as usize)
        .context("Invalid analysis player")?;
    match decision.kind.as_str() {
        "checker" => {
            player.checker_decisions += 1;
            player.checker_equity_loss += decision.equity_loss;
        }
        "cube" => {
            player.cube_decisions += 1;
            player.cube_equity_loss += decision.equity_loss;
        }
        _ => anyhow::bail!("Unknown analysis decision kind"),
    }
    if decision.evaluation_status == "forced" {
        player.forced_moves += 1;
    }
    if decision.evaluation_status == "unavailable" {
        player.unavailable += 1;
    }
    match decision.severity.as_str() {
        "doubtful" => player.doubtful += 1,
        "bad" => player.bad += 1,
        "blunder" => player.blunder += 1,
        "very_bad" => player.very_bad += 1,
        "none" => {}
        _ => anyhow::bail!("Unknown analysis error severity"),
    }
    Ok(())
}

#[cfg(test)]
mod tests;
