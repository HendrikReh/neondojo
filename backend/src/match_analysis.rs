//! Native GNUbg game analysis, preserving the complete match context.
use crate::engine::Engine;
use crate::import::{
    parse_sgf,
    sgf::{Node, parse_trees},
};
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tokio::io::AsyncReadExt;
use utoipa::ToSchema;

pub const PERFORMANCE_RATING_METHOD: &str = "gnubg_emg_500_v1";

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
            (decisions > 0).then(|| 500.0 * player.normalized_equity_loss / decisions as f64);
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
    /// "none", "doubtful" (>= 0.04), "bad" (>= 0.08), or "very_bad" (>= 0.16).
    pub severity: String,
    /// The original imported dice for checker decisions; absent for cube decisions.
    pub dice: Option<[i32; 2]>,
    /// "evaluated" or "forced". A forced dance has null equities and zero loss;
    /// missing analysis for any evaluable decision fails the game analysis.
    pub evaluation_status: String,
}

pub fn game_count(sgf: &str) -> Result<i32> {
    let trees = crate::import::sgf::parse_trees(sgf)?;
    for tree in &trees {
        anyhow::ensure!(
            tree[0].get("GM").is_some_and(|v| v.as_slice() == ["6"]),
            "Only backgammon SGF is supported"
        );
    }
    Ok(i32::try_from(trees.len())?)
}

impl Engine {
    pub async fn analyze_game(&self, sgf: &str, game_no: i32, plies: i32) -> Result<AnalyzedGame> {
        ensure!((0..=2).contains(&plies), "plies must be 0..2");
        ensure!(
            (1..=game_count(sgf)?).contains(&game_no),
            "Game number outside match"
        );
        // Validate the entire played match before handing it to the native loader,
        // which can otherwise continue with a valid prefix of a malformed game.
        parse_sgf(sgf)?;
        let temp = tempfile::tempdir()?;
        tokio::fs::write(temp.path().join("input.sgf"), analysis_input(sgf)?).await?;
        let mut commands = String::from(
            "set confirm default yes\nset confirm new no\nset display off\nset analysis moves on\nset analysis cube on\nset analysis luck off\nset analysis player 0 analyse on\nset analysis player 1 analyse on\nset output mwc off\nset output digits 6\nset export moves number 10000\nset export include statistics on\n",
        );
        for kind in ["chequerplay", "cubedecision"] {
            commands.push_str(&format!("set analysis {kind} type evaluation\nset analysis {kind} evaluation plies {plies}\nset analysis {kind} evaluation cubeful on\nset analysis {kind} evaluation deterministic on\nset analysis {kind} evaluation noise 0\n"));
        }
        for mark in ["verybad", "bad", "doubtful", "unmarked"] {
            commands.push_str(&format!(
                "set export moves display {mark} on\nset export cube display {mark} on\n"
            ));
        }
        for mark in ["actual", "close", "missed"] {
            commands.push_str(&format!("set export cube display {mark} on\n"));
        }
        commands.push_str("load match input.sgf\nfirst game\n");
        if game_no > 1 {
            commands.push_str(&format!("next game {}\n", game_no - 1));
        }
        commands.push_str("analyse clear game\nanalyse game\nsave game analyzed.sgf\nexport game text analyzed.txt\n");
        let output = self
            .run_with_timeout(temp.path(), &commands, std::time::Duration::from_secs(600))
            .await?;
        for line in output.lines() {
            let lower = line.trim().to_ascii_lowercase();
            ensure!(
                !["error", "warning", "unknown", "illegal", "invalid"]
                    .iter()
                    .any(|p| lower.starts_with(p)),
                "GNUbg rejected game analysis: {line}"
            );
        }
        let native = read_analysis_artifact(&temp.path().join("analyzed.sgf")).await?;
        let export = read_analysis_artifact(&temp.path().join("analyzed.txt")).await?;
        let decisions = parse_game(sgf, game_no, &native, &export)?;
        let statistics = parse_native_statistics(&export, game_no, &decisions);
        let engine = output
            .lines()
            .find(|l| l.starts_with("GNU Backgammon "))
            .context("GNUbg engine version missing")?
            .to_owned();
        Ok(AnalyzedGame {
            engine,
            game_no,
            decisions,
            statistics,
        })
    }
}

/// GNUbg's own denominator excludes checker plays with at most one legal move
/// and cube choices that are neither close nor actual. Its statistics also use
/// native normalized losses, rather than sums reconstructed from move exports.
/// See the GNUbg manual, Statistics / Overall rating, and analysis.c GetStatistics.
/// https://www.gnu.org/software/gnubg/manual/gnubg.html#gnubg-matches_review_stat_overall
fn parse_native_statistics(
    export: &str,
    game_no: i32,
    decisions: &[Decision],
) -> Option<[NativePlayerStatistics; 2]> {
    static HEADER: OnceLock<Regex> = OnceLock::new();
    let header =
        HEADER.get_or_init(|| Regex::new(r"(?m)^Game statistics for game (\d+)\r?$").unwrap());
    let mut sections = header.captures_iter(export);
    let section = sections.next()?;
    if section[1].parse::<i32>().ok()? != game_no
        || sections.next().is_some()
        || decisions
            .iter()
            .any(|d| d.evaluation_status == "unavailable")
    {
        return None;
    }
    let statistics = &export[section.get(0)?.end()..];
    let (checker, cube_and_overall) = statistics.split_once("Cube statistics")?;
    let (cube, overall) = cube_and_overall.split_once("Overall statistics")?;
    let counts = |section: &str, label: &str| -> Option<[i32; 2]> {
        let mut values = section
            .lines()
            .filter_map(|line| line.trim().strip_prefix(label));
        let row = values.next()?;
        if values.next().is_some() {
            return None;
        }
        let values: Vec<_> = row.split_whitespace().map(str::parse::<i32>).collect();
        match values.as_slice() {
            [Ok(white), Ok(black)] if *white >= 0 && *black >= 0 => Some([*white, *black]),
            _ => None,
        }
    };
    let total_moves = counts(checker, "Total moves")?;
    let unforced = counts(checker, "Unforced moves")?;
    let total_cube = counts(cube, "Total cube decisions")?;
    let close_cube = counts(cube, "Close or actual cube decisions")?;
    static LOSS: OnceLock<Regex> = OnceLock::new();
    let loss = LOSS.get_or_init(|| {
        Regex::new(
        r"(?m)^Error total EMG \(MWC\)\s+(-\d+\.\d+)\s+\([^\n)]*\)\s+(-\d+\.\d+)\s+\([^\n)]*\)\s*$"
    ).unwrap()
    });
    let mut rows = loss.captures_iter(overall);
    let row = rows.next()?;
    if rows.next().is_some() {
        return None;
    }
    let losses = [-row[1].parse::<f64>().ok()?, -row[2].parse::<f64>().ok()?];
    let mut players = Vec::with_capacity(2);
    for player in 0..2 {
        let count = |kind| {
            decisions
                .iter()
                .filter(|d| d.player == player as i32 && d.kind == kind)
                .count()
        };
        if total_moves[player] as usize != count("checker")
            || total_cube[player] as usize != count("cube")
            || unforced[player] > total_moves[player]
            || close_cube[player] > total_cube[player]
            || !losses[player].is_finite()
            || losses[player] < 0.0
        {
            return None;
        }
        players.push(NativePlayerStatistics {
            total_checker_moves: total_moves[player],
            total_cube_decisions: total_cube[player],
            unforced_checker_moves: unforced[player],
            close_cube_decisions: close_cube[player],
            normalized_equity_loss: losses[player],
        });
    }
    players.try_into().ok()
}

async fn read_analysis_artifact(path: &std::path::Path) -> Result<String> {
    const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
    let file = tokio::fs::File::open(path)
        .await
        .context("GNUbg analysis artifact is missing")?;
    ensure!(
        file.metadata().await?.len() <= MAX_ARTIFACT_BYTES,
        "GNUbg analysis artifact exceeds 64 MiB limit"
    );
    let mut text = String::new();
    file.take(MAX_ARTIFACT_BYTES + 1)
        .read_to_string(&mut text)
        .await?;
    ensure!(
        text.len() as u64 <= MAX_ARTIFACT_BYTES,
        "GNUbg analysis artifact exceeds 64 MiB limit"
    );
    Ok(text)
}

/// Keep all game state, but exclude imported analysis and arbitrary annotations.
/// Fixed names also prevent player-name newlines from resembling export headers.
fn analysis_input(sgf: &str) -> Result<String> {
    let trees = parse_trees(sgf)?;
    let (_, contexts) = crate::import::sgf::parse_sgf_with_context(sgf)?;
    let mut out = String::new();
    let property = |out: &mut String, key: &str, values: &[String]| {
        out.push_str(key);
        for value in values {
            out.push('[');
            out.push_str(&value.replace('\\', "\\\\").replace(']', "\\]"));
            out.push(']');
        }
    };
    for (game_index, nodes) in trees.into_iter().enumerate() {
        let context = &contexts[game_index];
        out.push('(');
        for (index, node) in nodes.into_iter().enumerate() {
            if index == 0 {
                out.push(';');
                for key in ["FF", "GM", "CA", "RU", "RE"] {
                    if let Some(values) = node.get(key) {
                        if key == "RU" && context.crawford {
                            // Native GNUbg recognizes CrawfordGame only together
                            // with its Crawford rule flag; strict SGF accepts either.
                            property(&mut out, key, &["Crawford:CrawfordGame".into()]);
                        } else {
                            property(&mut out, key, values);
                        }
                    }
                }
                // The importer inherits omitted scores/length, whereas GNUbg
                // defaults a missing MI to a fresh money session.
                property(
                    &mut out,
                    "MI",
                    &[
                        format!("length:{}", context.length),
                        format!("game:{game_index}"),
                        format!("ws:{}", context.scores[0]),
                        format!("bs:{}", context.scores[1]),
                    ],
                );
                out.push_str("PW[Player White]PB[Player Black]");
            }
            // GNUbg ignores setup properties in the game-info root node. Emit
            // setup before the move, and translate standard CO to GNUbg's CP.
            if ["CV", "CO", "CP", "PL", "AE", "AW", "AB"]
                .iter()
                .any(|key| node.contains_key(*key))
            {
                out.push(';');
                if let Some(values) = node.get("CV") {
                    property(&mut out, "CV", values);
                }
                let owner = node.get("CO").or_else(|| {
                    node.get("CP")
                        .filter(|v| v.len() == 1 && ["c", "w", "b"].contains(&v[0].as_str()))
                });
                if let Some(values) = owner {
                    if values.as_slice() == ["n"] {
                        property(&mut out, "CP", &["c".into()]);
                    } else {
                        property(&mut out, "CP", values);
                    }
                }
                for key in ["PL", "AE", "AW", "AB"] {
                    if let Some(values) = node.get(key) {
                        property(&mut out, key, values);
                    }
                }
            }
            for key in ["W", "B"] {
                if let Some(values) = node.get(key) {
                    out.push(';');
                    property(&mut out, key, values);
                }
            }
        }
        out.push_str(")\n");
    }
    Ok(out)
}

fn moves(nodes: &[Node]) -> Result<Vec<(i32, &str, &Node)>> {
    let mut moves = Vec::new();
    for node in nodes {
        for (player, key) in [(0, "W"), (1, "B")] {
            if let Some(values) = node.get(key) {
                ensure!(values.len() == 1, "Native SGF move has multiple values");
                moves.push((player, values[0].as_str(), node));
            }
        }
    }
    Ok(moves)
}

fn severity(loss: f64) -> &'static str {
    if loss >= 0.160 {
        "very_bad"
    } else if loss >= 0.080 {
        "bad"
    } else if loss >= 0.040 {
        "doubtful"
    } else {
        "none"
    }
}

fn decision(
    player: i32,
    kind: &str,
    played: &str,
    best: &str,
    equities: Option<(f64, f64)>,
) -> Decision {
    // GNUbg exports six decimals; preserve that precision at classification and
    // database filter boundaries instead of exposing binary subtraction noise.
    let loss = equities.map_or(0.0, |(played, best)| {
        (((best - played) * 1_000_000.0).round() / 1_000_000.0).max(0.0)
    });
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
        severity: severity(loss).into(),
        dice: None,
        evaluation_status: if equities.is_some() {
            "evaluated"
        } else {
            "forced"
        }
        .into(),
    }
}

fn number(value: &str) -> Result<f64> {
    let value: f64 = value.parse().context("Invalid native analysis equity")?;
    ensure!(value.is_finite(), "Non-finite native analysis equity");
    Ok(value)
}

fn cube_decision(block: &str, player: i32, played: &str) -> Result<Decision> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"(?m)^\s*\d\. (No double|Double, take|Double, pass)\s+([+-]?[0-9]+\.[0-9]+)")
            .unwrap()
    });
    let mut equities = std::collections::BTreeMap::new();
    for row in pattern.captures_iter(block) {
        ensure!(
            equities
                .insert(row[1].to_owned(), number(&row[2])?)
                .is_none(),
            "Duplicate native cube equity"
        );
    }
    ensure!(
        equities.len() == 3,
        "GNUbg returned incomplete cube analysis"
    );
    let no_double = equities["No double"];
    let take = equities["Double, take"];
    let pass = equities["Double, pass"];
    // Exported cube equities are all from the doubler's perspective, in units
    // of the cube before the offer. The responder chooses its own best equity.
    let (actual, best, label) = match played {
        "take" | "drop" => {
            let best = (-take).max(-pass);
            (
                if played == "take" { -take } else { -pass },
                best,
                if take <= pass { "take" } else { "drop" },
            )
        }
        "double" | "no double" => {
            let double = take.min(pass);
            (
                if played == "double" {
                    double
                } else {
                    no_double
                },
                no_double.max(double),
                if no_double >= double {
                    "no double"
                } else {
                    "double"
                },
            )
        }
        _ => bail!("Unsupported native cube decision"),
    };
    Ok(decision(
        player,
        "cube",
        played,
        label,
        Some((actual, best)),
    ))
}

/// GNUbg skips the standard opening board. Custom positions can require cube
/// analysis even as the first recorded move. GetDPEq then checks score,
/// ownership and Crawford.
/// https://sources.debian.org/src/gnubg/1.05.002-2/analysis.c/
/// https://sources.debian.org/src/gnubg/1.05.002-2/eval.c/
fn requires_cube_analysis(position: &crate::import::ImportedPosition) -> Result<bool> {
    if position.board.as_slice() == crate::import::sgf::initial_board() {
        return Ok(false);
    }
    let id = STANDARD_NO_PAD.decode(&position.match_id)?;
    ensure!(id.len() == 9, "Invalid imported Match ID");
    let bits = |offset: usize, width: usize| -> i32 {
        (0..width).fold(0, |value, i| {
            value | (i32::from((id[(offset + i) / 8] >> ((offset + i) % 8)) & 1) << i)
        })
    };
    let owner = bits(4, 2);
    if bits(7, 1) != 0 || (owner != 3 && owner != position.player) {
        return Ok(false);
    }
    let length = bits(21, 15);
    let score = bits(36 + 15 * position.player as usize, 15);
    let cube = 1 << bits(0, 4);
    Ok(length == 0 || score + cube < length)
}

fn parse_game(sgf: &str, game_no: i32, native: &str, text: &str) -> Result<Vec<Decision>> {
    let original = parse_trees(sgf)?;
    let selected = original
        .get(usize::try_from(game_no - 1)?)
        .context("Game number outside match")?;
    let (native_match, native_contexts) =
        crate::import::sgf::parse_native_sgf_with_context(native)?;
    let native_positions = native_match.positions;
    let native = parse_trees(native)?;
    ensure!(native.len() == 1, "GNUbg did not export exactly one game");
    let source_moves = moves(selected)?;
    let native_moves = moves(&native[0])?;
    ensure!(
        source_moves.len() == native_moves.len(),
        "GNUbg truncated the selected game"
    );
    for ((player, played, _), (native_player, native_played, _)) in
        source_moves.iter().zip(&native_moves)
    {
        ensure!(
            player == native_player && played == native_played,
            "GNUbg changed the played move sequence"
        );
    }
    let (imported, source_contexts) = crate::import::sgf::parse_sgf_with_context(sgf)?;
    ensure!(
        source_contexts.get(usize::try_from(game_no - 1)?) == native_contexts.first(),
        "GNUbg changed the game score, rules, or checker/cube decision context"
    );
    let positions: Vec<_> = imported
        .positions
        .iter()
        .filter(|p| p.game_no == game_no)
        .collect();
    ensure!(
        positions.len() == native_positions.len(),
        "Native game position coverage changed"
    );
    for (original, analyzed) in positions.iter().zip(&native_positions) {
        ensure!(
            original.position_id == analyzed.position_id && original.match_id == analyzed.match_id,
            "GNUbg changed the board, match score, dice, rules or cube context"
        );
    }
    static HEADERS: OnceLock<Regex> = OnceLock::new();
    let headers = HEADERS.get_or_init(|| Regex::new(r"(?m)^Move number (\d+):[^\n]*").unwrap());
    let starts: Vec<_> = headers
        .captures_iter(text)
        .map(|c| (c.get(0).unwrap().start(), c[1].parse::<usize>()))
        .collect();
    ensure!(
        starts.len() == source_moves.len(),
        "GNUbg text export has incomplete move coverage"
    );
    static CANDIDATES: OnceLock<Regex> = OnceLock::new();
    let candidates = CANDIDATES.get_or_init(|| {
        Regex::new(
            r"(?m)^\s*(\*)?\s*(\d+)\.\s+Cubeful\s+\d+-ply\s+(.+?)\s+Eq\.:\s*([+-]?[0-9]+\.[0-9]+)",
        )
        .unwrap()
    });
    let mut decisions = Vec::new();
    let mut checker_no = 0usize;
    for (index, (player, played, node)) in native_moves.into_iter().enumerate() {
        ensure!(
            starts[index].1.as_ref().is_ok_and(|n| *n == index + 1),
            "Native export move numbering is incomplete"
        );
        let block = &text[starts[index].0..starts.get(index + 1).map_or(text.len(), |s| s.0)];
        if matches!(played, "double" | "take" | "drop") {
            decisions.push(cube_decision(block, player, played)?);
            continue;
        }
        let position = positions
            .get(checker_no)
            .context("Native checker move has no imported position")?;
        checker_no += 1;
        ensure!(position.player == player, "Native checker player mismatch");
        ensure!(
            !requires_cube_analysis(position)? || node.contains_key("DA"),
            "GNUbg omitted an available cube decision"
        );
        if node.contains_key("DA") {
            let mut cube = cube_decision(block, player, "no double")?;
            cube.checker_no = Some(checker_no as i32);
            decisions.push(cube);
        } else {
            ensure!(
                !block.contains("Cube analysis"),
                "Native cube analysis is inconsistent with SGF"
            );
        }
        let rows: Vec<_> = candidates.captures_iter(block).collect();
        let mut checker = if rows.is_empty() {
            ensure!(
                played.len() == 2 && block.lines().any(|l| l.trim() == "*    Cannot move"),
                "GNUbg returned no checker evaluation for an unforced move"
            );
            decision(player, "checker", "Cannot move", "Cannot move", None)
        } else {
            let best = &rows[0];
            ensure!(&best[2] == "1", "Native checker best candidate missing");
            let actual: Vec<_> = rows.iter().filter(|row| row.get(1).is_some()).collect();
            ensure!(
                actual.len() == 1,
                "Native export must identify exactly one actual checker play"
            );
            let actual = actual[0];
            let selected: usize = node
                .get("A")
                .and_then(|a| a.first())
                .context("Native checker analysis missing")?
                .parse()?;
            ensure!(
                actual[2].parse::<usize>()? == selected + 1,
                "Native actual checker candidate mismatch"
            );
            decision(
                player,
                "checker",
                actual[3].trim(),
                best[3].trim(),
                Some((number(&actual[4])?, number(&best[4])?)),
            )
        };
        checker.checker_no = Some(checker_no as i32);
        checker.dice = Some(position.dice);
        decisions.push(checker);
    }
    ensure!(
        checker_no == positions.len(),
        "GNUbg omitted imported checker positions"
    );
    for (index, decision) in decisions.iter_mut().enumerate() {
        decision.decision_no = index as i32 + 1;
    }
    Ok(decisions)
}

#[cfg(test)]
mod tests {
    use super::*;
    const MATCH: &str = include_str!("../tests/fixtures/WC2014.gnubg.sgf");

    #[tokio::test]
    #[ignore = "requires local GNUbg with weights"]
    async fn native_performance_statistics_exclude_forced_moves_and_trivial_cube_choices() {
        let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        // Two legal opening moves; the second player's pre-roll cube choice is trivial.
        let source = "(;GM[6]MI[length:7];W[31xufe];B[42qusu])";
        let game = engine.analyze_game(source, 1, 0).await.unwrap();
        let value = serde_json::to_value(game).unwrap();
        assert_eq!(value["statistics"][0]["unforced_checker_moves"], 1);
        assert_eq!(value["statistics"][1]["unforced_checker_moves"], 1);
        assert_eq!(value["statistics"][1]["total_cube_decisions"], 1);
        assert_eq!(value["statistics"][1]["close_cube_decisions"], 0);
        assert!(
            (value["statistics"][0]["normalized_equity_loss"]
                .as_f64()
                .unwrap()
                - 0.267064)
                .abs()
                < 0.000002
        );
        // Independent CLI oracle: query native statistics directly, without
        // running our text-export parser or reconstructing losses from decisions.
        let temp = tempfile::tempdir().unwrap();
        tokio::fs::write(temp.path().join("input.sgf"), source)
            .await
            .unwrap();
        let native = engine.run_with_timeout(temp.path(),
            "set confirm default yes\nset display off\nset output mwc off\nset output digits 6\nset analysis luck off\nset analysis chequerplay evaluation plies 0\nset analysis cubedecision evaluation plies 0\nload match input.sgf\nanalyse clear game\nanalyse game\nshow statistics game\n",
            std::time::Duration::from_secs(30)).await.unwrap();
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
        assert_eq!(
            value["statistics"][0]["normalized_equity_loss"]
                .as_f64()
                .unwrap(),
            -oracle_white
        );
    }

    #[tokio::test]
    #[ignore = "requires local GNUbg with weights"]
    async fn native_performance_statistics_cover_cube_only_and_forced_checker_games() {
        let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let source = "(;GM[6]MI[length:7];W[31hefe])\n(;GM[6]MI[length:7]AE[a:y]AW[f]AB[s]PL[W]RE[W+1];W[double];B[drop])";
        let cube = engine.analyze_game(source, 2, 0).await.unwrap();
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
            let game = engine.analyze_game(source, 1, 0).await.unwrap();
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
        let native = parse_native_statistics(export, 1, &decisions).unwrap();
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
                parse_native_statistics(&changed, 1, &decisions).is_none(),
                "{changed}"
            );
        }
        assert!(parse_native_statistics(export, 2, &decisions).is_none());
        let mut unavailable = decisions;
        unavailable[0].evaluation_status = "unavailable".into();
        assert!(parse_native_statistics(export, 1, &unavailable).is_none());
    }

    #[test]
    fn counts_games_and_rejects_truncated_input() {
        assert_eq!(game_count(MATCH).unwrap(), 10);
        assert!(game_count(&MATCH[..MATCH.len() - 10]).is_err());
        assert!(game_count("(;GM[6];W[31").is_err());
    }
    const NATIVE: &str = include_str!("../tests/fixtures/analysis-game1.sgf");
    const EXPORT: &str = include_str!("../tests/fixtures/analysis-game1.txt");

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
        assert_eq!(double.severity, "doubtful");
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
        let mut drops = Vec::new();
        for game_no in 1..=10 {
            let result = engine.analyze_game(MATCH, game_no, 0).await.unwrap();
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
            drops.extend(result.decisions.into_iter().filter(|d| d.played == "drop"));
        }
        assert_eq!(total, 460);
        assert!(
            drops
                .iter()
                .any(|d| (d.equity_loss - 0.138356).abs() < 0.000003)
        );
    }

    #[tokio::test]
    #[ignore = "requires local GNUbg with weights"]
    async fn real_gnubg_includes_actual_play_beyond_top_five_and_normalizes_dice() {
        let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let sgf = "(;FF[4]GM[6]MI[length:7][game:0][ws:0][bs:0]PW[Jörg\nMove number 1: trick]PB[李  Müller]RU[Crawford];W[13hehg]C[Move number 99: pretend])";
        let game = engine.analyze_game(sgf, 1, 0).await.unwrap();
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
            (0.08, 0.12, 0.04, "doubtful"),
            (0.16, 0.24, 0.08, "bad"),
            (0.32, 0.48, 0.16, "very_bad"),
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
        let xg = crate::import::xg_to_sgf(include_bytes!("../tests/fixtures/WC2014.xg")).unwrap();
        let mat = engine
            .normalize_mat(include_bytes!("../tests/fixtures/WC2014.mat"))
            .await
            .unwrap();
        for source in [xg, mat] {
            let mut total = 0;
            for game_no in 1..=10 {
                let game = engine.analyze_game(&source, game_no, 0).await.unwrap();
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
            let game = engine.analyze_game(source, 1, 0).await.unwrap();
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
        let game = engine.analyze_game(source, 2, 0).await.unwrap();
        assert_eq!(game.decisions.len(), 2);
        assert!(
            game.decisions
                .iter()
                .all(|d| d.kind == "cube" && d.played_equity.is_some())
        );
        assert_eq!(game.decisions[0].played, "double");
        assert_eq!(game.decisions[1].played, "drop");
        let empty = engine.analyze_game(source, 3, 0).await.unwrap();
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
        let cube = engine.analyze_game(source, 2, 0).await.unwrap();
        assert_eq!(cube.decisions.len(), 2);
        let empty = engine.analyze_game(source, 3, 0).await.unwrap();
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
                .analyze(&p.position_id, &p.match_id, 0, None)
                .await
                .unwrap();
            let native = engine.analyze_game(source, 1, 0).await.unwrap();
            let checker = native
                .decisions
                .iter()
                .rfind(|d| d.kind == "checker")
                .unwrap();
            assert_eq!(checker.player, 1);
            assert_eq!(checker.best, independent.candidates[0].notation);
            assert!(
                (checker.best_equity.unwrap() - independent.candidates[0].equity).abs() < 0.000002
            );
        }
        let cube_only = "(;GM[6]MI[length:7];W[31hefe])(;GM[6]MI[length:7][ws:2][bs:3]AE[a:y]AW[e]AB[s]PL[B]CV[2]CO[b];B[double];W[drop])";
        let native = engine.analyze_game(cube_only, 2, 0).await.unwrap();
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
        let cube =
            "(;GM[6]MI[length:7][ws:1][bs:0]AE[a:y]AW[e]AB[s]PL[B]CV[2]CO[b];B[double];W[drop])";
        let empty = "(;GM[6]MI[length:7][ws:1][bs:0])";
        let source = format!("(;GM[6]MI[length:7];W[31hefe]){cube}{empty}");
        let native = cube.replace("PL[B]", "PL[W]");
        let equities =
            "1. No double +0.500000\n2. Double, take +0.900000\n3. Double, pass +1.000000\n";
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
}
