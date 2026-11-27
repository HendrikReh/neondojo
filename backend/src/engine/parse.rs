//! GNUbg text output parsers for hints, cube equities, forced passes, and roll batches.

use super::{Candidate, gnubg_plies, plies_from_gnubg};
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use std::sync::OnceLock;

/// Printed by `set dice`; it starts each roll's output in a batch.
pub(super) const ROLL_MARKER: &str = "The dice have been set to ";

pub(super) fn reported_threads(output: &str) -> Option<i32> {
    output.lines().find_map(|line| {
        let line = line.trim();
        line.strip_suffix(" calculation threads.")
            .or_else(|| line.strip_suffix(" calculation thread."))?
            .parse()
            .ok()
    })
}

pub(crate) struct CubeEquities {
    pub(crate) no_double: f64,
    pub(crate) take: f64,
    pub(crate) pass: f64,
    /// Cubeless win, gammon and backgammon chances followed by the three
    /// losing chances, in the same perspective as the equities.
    pub(crate) probabilities: Option<[f64; 6]>,
}

/// Swap the winning and losing halves when changing the player perspective.
pub(crate) fn flip_probabilities(p: [f64; 6]) -> [f64; 6] {
    [p[3], p[4], p[5], p[0], p[1], p[2]]
}

/// GNUbg prints the cubeless outcome chances on the line after the
/// "cubeless equity" heading of a cube analysis.
fn parse_cube_probabilities(output: &str) -> Option<[f64; 6]> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(
            r"(?m)cubeless equity[^\n]*\n\s*([0-9.]+) ([0-9.]+) ([0-9.]+) - ([0-9.]+) ([0-9.]+) ([0-9.]+)\s*$",
        )
        .unwrap()
    });
    let row = pattern.captures(output)?;
    let mut values = [0.0_f64; 6];
    for (index, value) in values.iter_mut().enumerate() {
        *value = row[index + 1].parse().ok()?;
    }
    (values.iter().all(|p| (0.0..=1.0).contains(p)) && (values[0] + values[3] - 1.0).abs() < 0.001)
        .then_some(values)
}

pub(super) fn parse_cube_depth(output: &str) -> Result<i32> {
    static DEPTH: OnceLock<Regex> = OnceLock::new();
    let depth = DEPTH.get_or_init(|| Regex::new(r"(?m)^\s*(\d+)-ply cubeless equity").unwrap());
    Ok(plies_from_gnubg(
        depth
            .captures(output)
            .context("GNUbg did not report its cube evaluation depth")?[1]
            .parse::<i32>()?,
    ))
}

pub(crate) fn parse_cube_equities(output: &str) -> Result<CubeEquities> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"(?m)^\s*\d\. (No double|Double, take|Double, pass)\s+([+-]?[0-9]+\.[0-9]+)")
            .unwrap()
    });
    let mut values = [None; 3];
    for row in pattern.captures_iter(output) {
        let index = match &row[1] {
            "No double" => 0,
            "Double, take" => 1,
            "Double, pass" => 2,
            _ => bail!("Unrecognized native cube action"),
        };
        let value = row[2].parse::<f64>()?;
        ensure!(value.is_finite(), "Non-finite native cube equity");
        ensure!(
            values[index].replace(value).is_none(),
            "Duplicate native cube equity"
        );
    }
    let [Some(no_double), Some(take), Some(pass)] = values else {
        bail!("GNUbg returned incomplete cube analysis");
    };
    Ok(CubeEquities {
        no_double,
        take,
        pass,
        probabilities: parse_cube_probabilities(output),
    })
}

/// GNUbg's standard-error diagnostics, each distinct line once in first-seen
/// order. GNUbg repeats some messages, such as the MAT reader's "not a valid
/// .mat file", once per parse attempt.
pub(super) fn gnubg_diagnostics(stderr: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && seen.insert(*line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// GNUbg uses the same rows for hints and game exports, marking the played
/// move with an asterisk in the game export. Reported depths are converted to
/// NeonDojo's ply convention.
pub(crate) fn parse_checker_candidates(
    output: &str,
    cube_enabled: bool,
) -> Result<Vec<(Candidate, bool)>> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(
            r"^\s*(\*)?\s*(\d+)\.\s+(Cube(?:ful|less))\s+(\d+)-ply\s+(.+?)\s+Eq\.:\s*([+-]?[0-9]+\.[0-9]+)",
        )
        .unwrap()
    });
    let mut candidates = Vec::new();
    let mut lines = output.lines();
    while let Some(line) = lines.next() {
        if let Some(captures) = pattern.captures(line) {
            ensure!(
                (&captures[3] == "Cubeful") == cube_enabled,
                "GNUbg checker evaluation used the wrong cube rule"
            );
            let probabilities = lines
                .next()
                .context("Missing GNUbg checker probabilities")?
                .split_whitespace()
                .filter(|token| *token != "-")
                .map(str::parse::<f64>)
                .collect::<Result<Vec<_>, _>>()?;
            let equity: f64 = captures[6].parse()?;
            ensure!(
                equity.is_finite()
                    && probabilities.len() == 6
                    && probabilities.iter().all(|p| (0.0..=1.0).contains(p)),
                "Incomplete GNUbg evaluation"
            );
            candidates.push((
                Candidate {
                    rank: captures[2].parse()?,
                    notation: captures[5].trim().into(),
                    path: None,
                    equity,
                    probabilities,
                    plies: Some(plies_from_gnubg(captures[4].parse()?)),
                },
                captures.get(1).is_some(),
            ));
        }
    }
    Ok(candidates)
}

pub(crate) fn parse_hints(output: &str, cube_enabled: bool) -> Result<Vec<Candidate>> {
    let candidates = parse_checker_candidates(output, cube_enabled)?;
    ensure!(
        !candidates.is_empty(),
        "GNUbg returned no evaluable move candidates"
    );
    Ok(candidates
        .into_iter()
        .map(|(candidate, _)| candidate)
        .collect())
}

pub(crate) fn parse_full_checker_hints(
    output: &str,
    plies: i32,
    cube_enabled: bool,
) -> Result<(Vec<Candidate>, i32)> {
    let candidates = parse_hints(output, cube_enabled)?;
    let evaluated_plies = candidates[0]
        .plies
        .context("Missing native checker evaluation depth")?;
    ensure!(
        candidates.len() == 1
            || candidates
                .iter()
                .all(|candidate| candidate.plies == Some(plies)),
        "Native feedback contains lower-depth pruned candidates"
    );
    Ok((candidates, evaluated_plies))
}

#[derive(Default)]
pub(super) struct RollProgress {
    scan_from: usize,
    started: usize,
}

impl RollProgress {
    pub(super) fn finished_rolls(&mut self, output: &[u8]) -> usize {
        let marker = ROLL_MARKER.as_bytes();
        self.started += output[self.scan_from..]
            .windows(marker.len() + 1)
            .filter(|window| window[0] == b'\n' && &window[1..] == marker)
            .count();
        // Keep only the possible starts of a marker split across two reads.
        self.scan_from = output.len().saturating_sub(marker.len());
        // A marker precedes its roll's evaluation.
        self.started.saturating_sub(1)
    }
}

/// Each roll's evaluation output, in the order the rolls were set. The next
/// roll's position setup is cut off.
pub(super) fn split_rolls<'a>(output: &'a str, rolls: &[[i32; 2]]) -> Result<Vec<&'a str>> {
    let starts: Vec<usize> = output
        .match_indices(&format!("\n{ROLL_MARKER}"))
        .map(|(start, _)| start + 1)
        .collect();
    ensure!(
        starts.len() == rolls.len(),
        "GNUbg evaluated {} of {} rolls",
        starts.len(),
        rolls.len()
    );
    starts
        .iter()
        .zip(rolls)
        .enumerate()
        .map(|(index, (&start, [a, b]))| {
            let end = starts.get(index + 1).copied().unwrap_or(output.len());
            let part = &output[start..end];
            ensure!(
                part.starts_with(&format!("{ROLL_MARKER}{a} and {b}.")),
                "GNUbg output does not follow the roll order"
            );
            Ok(part.split("Setting GNUbg ID").next().unwrap_or(part))
        })
        .collect()
}

pub(super) fn parse_pass(output: &str, plies: i32, cubeless: bool) -> Result<Candidate> {
    let prefix = match gnubg_plies(plies)? {
        0 => "static:".to_owned(),
        native => format!("{native} ply:"),
    };
    let row = output
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
        .context("GNUbg returned no evaluation after the forced pass")?;
    let values = row
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()?;
    ensure!(
        values.len() == 7 && values.iter().all(|n| n.is_finite()),
        "Invalid forced-pass evaluation"
    );
    ensure!(
        values[..5].iter().all(|p| (0.0..=1.0).contains(p)),
        "Invalid forced-pass probabilities"
    );
    Ok(Candidate {
        rank: 1,
        notation: "dance".into(),
        path: None,
        equity: -values[if cubeless { 5 } else { 6 }],
        probabilities: vec![
            1.0 - values[0],
            values[3],
            values[4],
            values[0],
            values[1],
            values[2],
        ],
        plies: Some(plies),
    })
}
