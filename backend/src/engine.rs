use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Stdio, sync::OnceLock, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};
use utoipa::ToSchema;

// Every application-created engine subprocess shares this gate. The benchmark
// owns the writer for its entire run; its task-local scope avoids recursive reads.
static ENGINE_WORK: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());
tokio::task_local! { static ENGINE_ACCESS: (); }

async fn engine_permit() -> Option<tokio::sync::RwLockReadGuard<'static, ()>> {
    if ENGINE_ACCESS.try_with(|_| ()).is_ok() {
        None
    } else {
        Some(ENGINE_WORK.read().await)
    }
}

/// Acquire the ordinary shared gate before entering a timed pipeline. The task
/// scope lets nested version/export commands reuse it even if a writer queues.
pub(crate) async fn with_shared_access<F: std::future::Future>(work: F) -> F::Output {
    let _permit = engine_permit().await;
    ENGINE_ACCESS.scope((), work).await
}

pub(crate) async fn benchmark_exclusive<F: std::future::Future>(work: F) -> F::Output {
    let _exclusive = ENGINE_WORK.write().await;
    ENGINE_ACCESS.scope((), work).await
}

/// Resolve before changing the child process's working directory. The canonical
/// path also identifies the executable in saved play sessions and benchmarks.
pub(crate) fn resolve_binary(binary: &str) -> Result<String> {
    let path = Path::new(binary);
    let candidate = if path.is_absolute() || binary.contains(std::path::is_separator) {
        path.to_path_buf()
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|directory| directory.join(path))
            .find(|candidate| candidate.is_file())
            .context("Configured GNUbg executable was not found on PATH")?
    };
    let resolved = std::fs::canonicalize(candidate)
        .context("Cannot resolve the configured GNUbg executable")?;
    Ok(resolved
        .to_str()
        .context("GNUbg executable path must be UTF-8")?
        .to_owned())
}

fn reported_threads(output: &str) -> Option<i32> {
    output.lines().find_map(|line| {
        let line = line.trim();
        line.strip_suffix(" calculation threads.")
            .or_else(|| line.strip_suffix(" calculation thread."))?
            .parse()
            .ok()
    })
}

/// Deepest GNUbg evaluation BG Tutor offers, in BG Tutor's ply convention.
pub(crate) const GNUBG_MAX_PLIES: i32 = 5;

/// BG Tutor stores, shows and compares every depth in the XG / Open Sage
/// convention, where 1-ply is the raw neural-net evaluation. GNUbg counts that
/// evaluation as 0-ply. This is the only conversion into GNUbg's native level;
/// apply it where a command or request is written for GNUbg.
pub(crate) fn gnubg_plies(plies: i32) -> Result<i32> {
    ensure!(
        (1..=GNUBG_MAX_PLIES).contains(&plies),
        "GNUbg depth must be 1..{GNUBG_MAX_PLIES} ply"
    );
    Ok(plies - 1)
}

/// The inverse of [`gnubg_plies`] for depths GNUbg reports in its output.
pub(crate) fn plies_from_gnubg(native: i32) -> i32 {
    native + 1
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GnubgMoveFilter {
    Top14,
}

impl GnubgMoveFilter {
    /// Pruned analysis at GNUbg's native 2- and 4-ply levels.
    pub(crate) fn for_analysis(engine_id: &str, plies: i32) -> Option<Self> {
        (engine_id == "gnubg" && matches!(gnubg_plies(plies), Ok(2 | 4))).then_some(Self::Top14)
    }
}

#[derive(Clone)]
pub struct Engine {
    binary: String,
    expected_version: Option<String>,
    threads: Option<i32>,
    move_filter: Option<GnubgMoveFilter>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Candidate {
    pub rank: i32,
    pub notation: String,
    /// Ordered checker steps supplied by the engine, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub equity: f64,
    /// Win, win gammon (including backgammon), win backgammon,
    /// loss, lose gammon (including backgammon), lose backgammon.
    pub probabilities: Vec<f64>,
    /// Depth actually used for this candidate, when recorded by the engine,
    /// in BG Tutor's ply convention (1-ply is the raw evaluation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plies: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Analysis {
    pub engine: String,
    pub plies: i32,
    pub candidates: Vec<Candidate>,
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

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DiceEvaluation {
    pub dice: [i32; 2],
    pub weight: i32,
    pub probability: f64,
    pub best: Candidate,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Distribution {
    pub engine: String,
    pub plies: i32,
    pub mean_equity: f64,
    pub rolls: Vec<DiceEvaluation>,
}

impl Engine {
    pub fn new(binary: String) -> Self {
        Self {
            binary,
            expected_version: None,
            threads: None,
            move_filter: None,
        }
    }

    pub(crate) fn with_threads(mut self, threads: Option<i32>) -> Self {
        self.threads = threads.filter(|&threads| threads != 1);
        self
    }

    pub(crate) fn calculation_threads(&self) -> i32 {
        self.threads.unwrap_or(1)
    }

    pub(crate) fn with_move_filter(mut self, filter: Option<GnubgMoveFilter>) -> Self {
        self.move_filter = filter;
        self
    }

    pub(crate) fn move_filter(&self) -> Option<GnubgMoveFilter> {
        self.move_filter
    }

    pub(crate) fn append_move_filter(
        &self,
        commands: &mut String,
        scope: &str,
        plies: i32,
    ) -> Option<i32> {
        let native = gnubg_plies(plies).ok()?;
        let stages: &[i32] = match (native, self.move_filter) {
            (2, Some(GnubgMoveFilter::Top14)) => &[0],
            (4, Some(GnubgMoveFilter::Top14)) => &[0, 2],
            _ => return None,
        };
        let count = 14;
        for stage in stages {
            commands.push_str(&format!(
                "set {scope} movefilter {native} {stage} {count} 0 0\n"
            ));
        }
        Some(count)
    }

    pub(crate) fn with_expected_version(mut self, version: Option<String>) -> Self {
        self.expected_version = version;
        self
    }

    pub(crate) fn saved_version(&self) -> Option<&str> {
        self.expected_version.as_deref()
    }

    pub fn binary(&self) -> &str {
        &self.binary
    }

    /// Check both the executable and its evaluation data. Dropping the timed-out
    /// evaluation kills its GNUbg child through `kill_on_drop`.
    pub async fn check_installation(&self) -> Result<String> {
        let _permit = engine_permit().await;
        ENGINE_ACCESS
            .scope((), async {
                tokio::time::timeout(Duration::from_secs(10), async {
                    let version = self.version().await?;
                    ensure!(
                        version.starts_with("GNU Backgammon "),
                        "Executable is not GNUbg"
                    );
                    // 1-ply is the raw neural-net evaluation.
                    let analysis = self
                        .analyze("4HPwATDgc/ABMA", "cInlAAAAAAAE", 1, None)
                        .await?;
                    ensure!(
                        !analysis.candidates.is_empty(),
                        "GNUbg returned no evaluated moves"
                    );
                    Ok(version)
                })
                .await
                .context("GNUbg installation check timed out after 10 seconds")?
            })
            .await
    }

    pub async fn version(&self) -> Result<String> {
        let _permit = engine_permit().await;
        self.version_unlocked(&resolve_binary(&self.binary)?).await
    }

    /// Keep the health check responsive when a benchmark owns the engine.
    /// A started version probe uses its normal subprocess timeout.
    pub(crate) async fn health_version(&self) -> Result<String> {
        let _permit = tokio::time::timeout(Duration::from_millis(300), engine_permit())
            .await
            .context("GNUbg is busy")?;
        self.version_unlocked(&resolve_binary(&self.binary)?).await
    }

    async fn version_unlocked(&self, binary: &str) -> Result<String> {
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            Command::new(binary)
                .arg("--version")
                .env("LC_ALL", "C")
                .kill_on_drop(true)
                .output(),
        )
        .await??;
        ensure!(result.status.success(), "GNUbg is unavailable");
        let version = String::from_utf8_lossy(&result.stdout)
            .lines()
            .next()
            .unwrap_or("GNUbg")
            .trim()
            .to_owned();
        self.validate_saved_version(&version)?;
        Ok(version)
    }

    pub(crate) fn validate_saved_version(&self, version: &str) -> Result<()> {
        if let Some(expected) = self.saved_version() {
            ensure!(
                version == expected,
                "GNUbg version changed since job submission: expected '{expected}', found '{version}'. Restore the original installation or submit a new analysis job"
            );
        }
        Ok(())
    }

    async fn run(&self, directory: &Path, commands: &str) -> Result<String> {
        self.run_with_timeout(directory, commands, Duration::from_secs(90))
            .await
    }

    pub(crate) async fn run_with_timeout(
        &self,
        directory: &Path,
        commands: &str,
        timeout: Duration,
    ) -> Result<String> {
        self.run_with_limit(directory, commands, Some(timeout))
            .await
    }

    pub(crate) async fn run_with_limit(
        &self,
        directory: &Path,
        commands: &str,
        timeout: Option<Duration>,
    ) -> Result<String> {
        let _permit = engine_permit().await;
        let binary =
            resolve_binary(&self.binary).context("Could not start GNUbg; check GNUBG_BIN")?;
        // A job may outlive an upgrade at the same path. Fence every native
        // invocation, including later games, dice rolls and forced-pass calls.
        if self.expected_version.is_some() {
            self.version_unlocked(&binary)
                .await
                .context("Cannot execute the saved GNUbg job configuration")?;
        }
        let commands_path = directory.join("commands.txt");
        if let Some(threads) = self.threads {
            ensure!(
                threads >= crate::configuration::engine_min_threads("gnubg"),
                "Unsupported GNUbg thread count: {threads}"
            );
            tokio::fs::write(
                commands_path,
                format!("set threads {threads}\nshow threads\n{commands}"),
            )
            .await?;
        } else {
            tokio::fs::write(commands_path, commands).await?;
        }
        let mut child = Command::new(&binary)
            .args(["-t", "-q", "-r", "-c", "commands.txt"])
            .current_dir(directory)
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Could not start GNUbg; check GNUBG_BIN")?;
        let stdout = child.stdout.take().context("GNUbg stdout missing")?;
        let stderr = child.stderr.take().context("GNUbg stderr missing")?;
        const MAX_OUTPUT: u64 = 8 * 1024 * 1024;
        let read =
            async |reader: std::pin::Pin<Box<dyn tokio::io::AsyncRead + Send>>| -> Result<Vec<u8>> {
                let mut output = Vec::new();
                reader.take(MAX_OUTPUT + 1).read_to_end(&mut output).await?;
                ensure!(
                    output.len() <= MAX_OUTPUT as usize,
                    "GNUbg output limit exceeded"
                );
                Ok(output)
            };
        let work = async {
            let (out, err, status) =
                tokio::try_join!(read(Box::pin(stdout)), read(Box::pin(stderr)), async {
                    Ok::<_, anyhow::Error>(child.wait().await?)
                })?;
            ensure!(
                status.success(),
                "GNUbg process failed: {}",
                String::from_utf8_lossy(&err)
            );
            // GNUbg can return exit code 0 after a failed command. Validate diagnostics and artifacts too.
            let err = String::from_utf8_lossy(&err);
            ensure!(err.trim().is_empty(), "GNUbg: {}", err.trim());
            Ok::<_, anyhow::Error>(String::from_utf8_lossy(&out).into_owned())
        };
        let output = if let Some(timeout) = timeout {
            match tokio::time::timeout(timeout, work).await {
                Ok(result) => result?,
                Err(_) => {
                    let _ = child.kill().await;
                    bail!("GNUbg timed out after {} seconds", timeout.as_secs())
                }
            }
        } else {
            work.await?
        };
        if let Some(threads) = self.threads {
            ensure!(
                reported_threads(&output) == Some(threads),
                "GNUbg did not accept the requested {threads} calculation threads"
            );
        }
        // Discard output if an installation replacement raced the invocation;
        // no checkpoint or completed result may incorporate another version.
        if self.expected_version.is_some() {
            self.version_unlocked(&binary)
                .await
                .context("GNUbg changed during execution; discarding the result")?;
        }
        Ok(output)
    }

    pub(crate) async fn max_parallel_threads(&self, cpu_threads: i32) -> Result<i32> {
        let temp = tempfile::tempdir()?;
        let commands = format!("set threads {cpu_threads}\nshow threads\n");
        let output = self
            .run_with_timeout(temp.path(), &commands, Duration::from_secs(5))
            .await?;
        reported_threads(&output)
            .filter(|count| (1..=cpu_threads).contains(count))
            .context("GNUbg did not report its calculation thread count")
    }

    pub async fn normalize_mat(&self, bytes: &[u8]) -> Result<String> {
        let temp = tempfile::tempdir()?;
        tokio::fs::write(
            temp.path().join("input.mat"),
            normalize_mat_resignations(bytes)?,
        )
        .await?;
        let output = self.run(temp.path(), "set confirm default yes\nset confirm new no\nset display off\nimport mat input.mat\nsave match canonical.sgf\n").await?;
        // GNUbg intentionally saves a valid prefix even after some MAT parse
        // failures. Exit status and artifact existence alone are insufficient.
        for line in output.lines() {
            let lower = line.trim().to_ascii_lowercase();
            if lower.contains("illegal")
                || lower.starts_with("unrecognised move")
                || lower.starts_with("warning")
                || lower.contains("match had errors")
                || lower.contains("not a valid .mat file")
                || lower.starts_with("invalid match length")
                || lower.starts_with("error reading")
            {
                bail!("GNUbg rejected the MAT file: {line}");
            }
        }
        let native = tokio::fs::read_to_string(temp.path().join("canonical.sgf"))
            .await
            .context("GNUbg could not import this MAT file")?;
        crate::import::sgf::normalize_mat_sgf(&native)
    }

    pub async fn analyze(
        &self,
        position_id: &str,
        match_id: &str,
        plies: i32,
        dice: Option<[i32; 2]>,
    ) -> Result<Analysis> {
        let native = gnubg_plies(plies)?;
        ensure!(
            valid_id(position_id, 14) && valid_id(match_id, 12),
            "Invalid GNUbg ID"
        );
        let mut commands = format!(
            "set display off\nset gnubgid {position_id}:{match_id}\nset evaluation chequerplay evaluation plies {native}\nset evaluation cubedecision evaluation plies {native}\nset output digits 6\n"
        );
        if let Some([a, b]) = dice {
            ensure!((1..=6).contains(&a) && (1..=6).contains(&b), "Invalid dice");
            commands.push_str(&format!("set dice {a} {b}\n"));
        }
        let count = self
            .append_move_filter(&mut commands, "evaluation", plies)
            .unwrap_or(10);
        let hint = format!("hint {count}\nsave game hint.sgf\n");
        commands.push_str(&hint);
        let temp = tempfile::tempdir()?;
        let output = self.run(temp.path(), &commands).await?;
        let engine = output
            .lines()
            .find(|l| l.starts_with("GNU Backgammon "))
            .unwrap_or("GNU Backgammon")
            .to_string();
        let candidates = if output.contains("There are no legal moves.") {
            // A dance is still a real checker decision. GNUbg evaluates the board
            // after passing, from the opponent's perspective; invert it back.
            let forced_commands = commands.replace(&hint, "set automatic roll off\nset automatic move off\nset player 0 human\nset player 1 human\nmove\neval\n");
            let output = self.run(temp.path(), &forced_commands).await?;
            vec![parse_pass(&output, plies)?]
        } else {
            let mut candidates = parse_hints(&output)?;
            crate::match_analysis::retain_hint_paths(temp.path(), position_id, &mut candidates)
                .await;
            candidates
        };
        Ok(Analysis {
            engine,
            plies,
            candidates,
        })
    }

    /// Evaluate all legal root moves at the requested depth, including plays
    /// that would be absent from or pruned in a normal hint.
    pub(crate) async fn analyze_checker_choices(
        &self,
        position_id: &str,
        match_id: &str,
        plies: i32,
        forced_pass: bool,
    ) -> Result<Analysis> {
        if forced_pass {
            return self.analyze(position_id, match_id, plies, None).await;
        }
        let commands = decision_hint_commands(position_id, match_id, plies)?
            + "hint 10000\nsave game hint.sgf\n";
        let directory = tempfile::tempdir()?;
        let output = self
            .run_with_timeout(directory.path(), &commands, Duration::from_secs(180))
            .await?;
        let (mut candidates, evaluated_plies) = parse_full_checker_hints(&output, plies)?;
        crate::match_analysis::retain_hint_paths(directory.path(), position_id, &mut candidates)
            .await;
        Ok(Analysis {
            engine: output
                .lines()
                .find(|line| line.starts_with("GNU Backgammon "))
                .unwrap_or("GNU Backgammon")
                .to_owned(),
            plies: evaluated_plies,
            candidates,
        })
    }

    /// Evaluate the cube in the on-roll player's pre-offer context.
    pub(crate) async fn analyze_cube_choices(
        &self,
        position_id: &str,
        match_id: &str,
        plies: i32,
    ) -> Result<(String, i32, CubeEquities)> {
        let commands = decision_hint_commands(position_id, match_id, plies)? + "hint 10000\n";
        let directory = tempfile::tempdir()?;
        let output = self.run(directory.path(), &commands).await?;
        let engine = output
            .lines()
            .find(|line| line.starts_with("GNU Backgammon "))
            .context("GNUbg did not report its version")?;
        static DEPTH: OnceLock<Regex> = OnceLock::new();
        let depth = DEPTH.get_or_init(|| Regex::new(r"(?m)^\s*(\d+)-ply cubeless equity").unwrap());
        let effective = plies_from_gnubg(
            depth
                .captures(&output)
                .context("GNUbg did not report its cube evaluation depth")?[1]
                .parse::<i32>()?,
        );
        ensure!(
            effective == plies,
            "GNUbg used a different cube evaluation depth"
        );
        Ok((engine.to_owned(), effective, parse_cube_equities(&output)?))
    }
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

pub(crate) fn decision_hint_commands(
    position_id: &str,
    match_id: &str,
    plies: i32,
) -> Result<String> {
    let native = gnubg_plies(plies)?;
    ensure!(
        valid_id(position_id, 14) && valid_id(match_id, 12),
        "Invalid GNUbg ID"
    );
    let mut commands = format!(
        "set display off\nset gnubgid {position_id}:{match_id}\nset output mwc off\nset output digits 6\n"
    );
    for kind in ["chequerplay", "cubedecision"] {
        commands.push_str(&format!("set evaluation {kind} evaluation plies {native}\nset evaluation {kind} evaluation cubeful on\nset evaluation {kind} evaluation deterministic on\nset evaluation {kind} evaluation noise 0\n"));
    }
    for depth in 1..=native {
        for previous in 0..depth {
            commands.push_str(&format!(
                "set evaluation movefilter {depth} {previous} 10000 0 0\n"
            ));
        }
    }
    Ok(commands)
}

fn normalize_mat_resignations(bytes: &[u8]) -> Result<Vec<u8>> {
    const MARKER: &[u8] = b"wants to resign";
    static RESULT: OnceLock<Regex> = OnceLock::new();
    let result = RESULT.get_or_init(|| {
        Regex::new(r"(?i)^Wins +[1-9][0-9]* +points?(?: +and +the +match)?$").unwrap()
    });
    let is_result = |value: &[u8]| {
        std::str::from_utf8(value.trim_ascii()).is_ok_and(|value| result.is_match(value))
    };
    let lines = bytes.split_inclusive(|b| *b == b'\n').collect::<Vec<_>>();
    let mut output = Vec::with_capacity(bytes.len());
    for (line_index, line) in lines.iter().enumerate() {
        let mut normalized = line.to_vec();
        // Restrict this dialect alias to numbered move rows, preserving all
        // byte offsets used by GNUbg's two-column MAT reader.
        if let Some(end_number) = line.iter().position(|b| *b == b')') {
            let number = line[..end_number].trim_ascii();
            if !number.is_empty() && number.iter().all(u8::is_ascii_digit) {
                for start in end_number + 1..line.len().saturating_sub(MARKER.len()) + 1 {
                    if !line[start..].starts_with(MARKER) {
                        continue;
                    }
                    let left = line[end_number + 1..start]
                        .iter()
                        .all(u8::is_ascii_whitespace);
                    let right =
                        start >= 16 && line[start - 2..start].iter().all(u8::is_ascii_whitespace);
                    let suffix = &line[start + MARKER.len()..];
                    let result_column = suffix
                        .iter()
                        .take_while(|b| b.is_ascii_whitespace())
                        .count();
                    let resolved = if left && result_column >= 2 && is_result(suffix) {
                        true
                    } else if suffix.trim_ascii().is_empty() {
                        lines[line_index + 1..]
                            .iter()
                            .find(|next| !next.trim_ascii().is_empty())
                            .is_some_and(|next| {
                                let column =
                                    next.iter().position(|b| !b.is_ascii_whitespace()).unwrap();
                                is_result(next)
                                    && ((left && column >= 16) || (right && column < 16))
                            })
                    } else {
                        false
                    };
                    ensure!(
                        (left || right) && resolved,
                        "MAT: resignation marker requires an opposite-player result"
                    );
                    // GNUbg ignores Resigns and derives the exact award from
                    // the following Wins record, including gammons/backgammons.
                    normalized[start..start + MARKER.len()].copy_from_slice(b"Resigns        ");
                }
            }
        }
        output.extend_from_slice(&normalized);
    }
    Ok(output)
}

pub fn valid_id(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'+' || c == b'/')
}

/// GNUbg uses the same rows for hints and game exports, marking the played
/// move with an asterisk in the game export. Reported depths are converted to
/// BG Tutor's ply convention.
pub(crate) fn parse_checker_candidates(output: &str) -> Result<Vec<(Candidate, bool)>> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(
            r"^\s*(\*)?\s*(\d+)\.\s+Cubeful\s+(\d+)-ply\s+(.+?)\s+Eq\.:\s*([+-]?[0-9]+\.[0-9]+)",
        )
        .unwrap()
    });
    let mut candidates = Vec::new();
    let mut lines = output.lines();
    while let Some(line) = lines.next() {
        if let Some(captures) = pattern.captures(line) {
            let probabilities = lines
                .next()
                .context("Missing GNUbg checker probabilities")?
                .split_whitespace()
                .filter(|token| *token != "-")
                .map(str::parse::<f64>)
                .collect::<Result<Vec<_>, _>>()?;
            let equity: f64 = captures[5].parse()?;
            ensure!(
                equity.is_finite()
                    && probabilities.len() == 6
                    && probabilities.iter().all(|p| (0.0..=1.0).contains(p)),
                "Incomplete GNUbg evaluation"
            );
            candidates.push((
                Candidate {
                    rank: captures[2].parse()?,
                    notation: captures[4].trim().into(),
                    path: None,
                    equity,
                    probabilities,
                    plies: Some(plies_from_gnubg(captures[3].parse()?)),
                },
                captures.get(1).is_some(),
            ));
        }
    }
    Ok(candidates)
}

pub(crate) fn parse_hints(output: &str) -> Result<Vec<Candidate>> {
    let candidates = parse_checker_candidates(output)?;
    ensure!(
        !candidates.is_empty(),
        "GNUbg returned no evaluable move candidates"
    );
    Ok(candidates
        .into_iter()
        .map(|(candidate, _)| candidate)
        .collect())
}

pub(crate) fn parse_full_checker_hints(output: &str, plies: i32) -> Result<(Vec<Candidate>, i32)> {
    let candidates = parse_hints(output)?;
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

fn parse_pass(output: &str, plies: i32) -> Result<Candidate> {
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
        equity: -values[6],
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

#[cfg(test)]
mod tests {
    #[test]
    fn parses_ranked_engine_hints_and_rejects_missing_results() {
        let output = "    1. Cubeful 0-ply    8/5 6/5                      Eq.: +0.219\n       0.551 0.174 0.013 - 0.449 0.124 0.005\n        0-ply cubeful prune [expert]\n    2. Cubeful 0-ply    24/23 13/10                  Eq.: -0.012 (-0.231)\n       0.497 0.137 0.008 - 0.503 0.140 0.007\n";
        let hints = super::parse_hints(output).unwrap();
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0].notation, "8/5 6/5");
        assert_eq!(hints[0].equity, 0.219);
        assert_eq!(hints[0].probabilities[0], 0.551);
        assert!(super::parse_hints("No game in progress").is_err());
        assert!(super::parse_full_checker_hints(output, 2).is_err());
        let single = output.split("    2.").next().unwrap();
        // GNUbg's native 0-ply is BG Tutor's 1-ply raw evaluation.
        assert_eq!(super::parse_full_checker_hints(single, 1).unwrap().1, 1);
        assert_eq!(hints[0].plies, Some(1));
    }

    #[cfg(unix)]
    fn native_sgf_engine(
        sgf: &str,
        reject_resignation_marker: bool,
    ) -> (tempfile::TempDir, super::Engine) {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("native-sgf-engine");
        let check = if reject_resignation_marker {
            "case \"$(cat input.mat)\" in *'wants to resign'*) echo 'Unrecognised move resignation in .mat file.';; esac\n"
        } else {
            ""
        };
        std::fs::write(
            &binary,
            format!(
                "#!/bin/sh\n{check}cat >canonical.sgf <<'CANONICAL_SGF'\n{sgf}\nCANONICAL_SGF\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let engine = super::Engine::new(binary.to_str().unwrap().to_owned());
        (directory, engine)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mat_native_joined_overshoot_bearoffs_become_strict_sgf_steps() {
        for (native, expected) in [
            ("(;GM[6]AE[a:y]AW[f][d][c]AB[x];W[52fz])", "6/4 4/off"),
            ("(;GM[6]AE[a:y]AW[d][c][c]AB[a][b];W[51dz])", "4/3 3/off"),
            ("(;GM[6]AE[a:y]AB[s][u][v]AW[a];B[52sz])", "6/4 4/off"),
        ] {
            assert!(crate::import::parse_sgf(native).is_err());
            let (_directory, engine) = native_sgf_engine(native, false);
            let normalized = engine
                .normalize_mat(b"native import boundary fixture")
                .await
                .unwrap();
            let imported = crate::import::parse_sgf(&normalized).unwrap();
            assert_eq!(imported.positions.len(), 1);
            assert_eq!(imported.positions[0].notation, expected);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mat_numbered_resignation_cells_are_normalized_before_gnubg() {
        let (_directory, engine) = native_sgf_engine("(;GM[6]RE[W+1R];W[31hefe])", true);
        let input = b"7 point match\n\n Game 1\n Alice : 0                         Bob : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n 2) wants to resign                Wins 1 point\n";
        assert!(engine.normalize_mat(input).await.is_ok());
    }

    #[test]
    fn mat_resignation_alias_preserves_columns_and_requires_a_result() {
        let input = b" Player wants to resign : 0\r\n 1) 31: 8/5 6/5                 wants to resign\r\n      Wins 2 points\r\n 2) wants to resign              Wins 4 points and the match\r\n";
        let normalized = super::normalize_mat_resignations(input).unwrap();
        assert_eq!(normalized.len(), input.len());
        assert!(normalized.starts_with(b" Player wants to resign : 0\r\n"));
        assert_eq!(
            normalized
                .windows(b"Resigns        ".len())
                .filter(|w| *w == b"Resigns        ")
                .count(),
            2
        );
        for input in [
            b" 1) wants to resign\n".as_slice(),
            b" 1) wants to resign              unexpected text\n",
            b" 1) wants to resignWins 1 point\n",
            b" 1) 31: 8/5 6/5                 wants to resign\n                                  Wins 2 points\n",
            b" 1) wants to resign\n      Wins 2 points\n",
        ] {
            assert!(super::normalize_mat_resignations(input).is_err());
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mat_native_normalization_rejects_incomplete_plays() {
        for native in [
            "(;GM[6];W[31he])",
            "(;GM[6]AE[a:y]AW[d][b]AB[c][c];W[51dz])",
            "(;GM[6];W[31hefe];W[42hdfd])",
        ] {
            let (_directory, engine) = native_sgf_engine(native, false);
            assert!(
                engine
                    .normalize_mat(b"native import boundary fixture")
                    .await
                    .is_err()
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires local GNUbg with weights"]
    async fn real_gnubg_finds_opening_31() {
        let engine = super::Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let result = engine
            .analyze("4HPwATDgc/ABMA", "cInlAAAAAAAE", 1, None)
            .await
            .unwrap();
        assert_eq!(result.candidates[0].notation, "8/5 6/5");
        assert!(result.candidates[0].equity > 0.1);
    }

    #[tokio::test]
    #[ignore = "requires local GNUbg with weights"]
    async fn real_gnubg_position_hint_retains_the_native_move_path() {
        let engine = super::Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let result = engine
            .analyze("4HPwATDgc/ABMA", "cAmvAgAAAAAE", 2, None)
            .await
            .unwrap();
        let candidate = result
            .candidates
            .iter()
            .find(|c| c.notation == "24/15")
            .unwrap();
        assert_eq!(candidate.path.as_deref(), Some("24/18 18/15"));
    }

    #[tokio::test]
    #[ignore = "requires local GNUbg with weights"]
    async fn real_gnubg_evaluates_a_forced_pass() {
        let engine = super::Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let result = engine
            .analyze("Y54pATDg28EBYA", "cAlnAQAAAAAE", 1, None)
            .await
            .unwrap();
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].notation, "dance");
        assert!(result.candidates[0].equity < -0.9);
        assert!(result.candidates[0].probabilities[0] < 0.4);
    }

    #[tokio::test]
    #[ignore = "requires local GNUbg"]
    async fn mat_with_valid_prefix_and_corrupt_tail_is_rejected() {
        let engine = super::Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let input=b"7 point match\n\n Game 1\n Alice : 0                         Bob : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\nThis is invalid trailing text\n";
        assert!(
            engine.normalize_mat(input).await.is_err(),
            "must not import a silently truncated match"
        );
        let invalid_move=b"7 point match\n\n Game 1\n Alice : 0                         Bob : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n 2) 65: 24/22 22/13\n";
        assert!(
            engine.normalize_mat(invalid_move).await.is_err(),
            "must reject a semantically invalid final move"
        );
    }
}

#[cfg(test)]
mod benchmark_gate_tests {
    use super::*;

    #[test]
    fn cube_analysis_keeps_outcome_chances_in_the_equity_perspective() {
        let block = "Cube analysis\n0-ply cubeless equity -0.007818 (Money: -0.007901)\n  0.492641 0.140604 0.005944 - 0.507359 0.133160 0.006572\nCubeful equities:\n1. No double           -0.013765\n2. Double, pass        +1.000000  (+1.013765)\n3. Double, take        -0.373791  (-0.360025)\n";
        let equities = parse_cube_equities(block).unwrap();
        assert_eq!(equities.no_double, -0.013765);
        let chances = equities.probabilities.unwrap();
        assert_eq!(
            chances,
            [0.492641, 0.140604, 0.005944, 0.507359, 0.133160, 0.006572]
        );
        assert_eq!(flip_probabilities(chances)[0], 0.507359);
        // Equities without the chance line stay usable.
        let bare = "1. No double +0.1\n2. Double, pass +1.0\n3. Double, take +0.2\n";
        assert!(parse_cube_equities(bare).unwrap().probabilities.is_none());
    }
    #[tokio::test]
    async fn exclusive_benchmark_blocks_normal_invocations_but_allows_its_own_checks() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let owner = tokio::spawn(benchmark_exclusive(async move {
            entered_tx.send(()).unwrap();
            assert!(
                Engine::new("/missing-engine".into())
                    .version()
                    .await
                    .is_err()
            );
            release_rx.await.unwrap();
        }));
        entered_rx.await.unwrap();
        let normal = Engine::new("/missing-engine".into());
        let waiting = normal.version();
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_millis(40), &mut waiting)
                .await
                .is_err()
        );
        release_tx.send(()).unwrap();
        owner.await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), waiting)
                .await
                .unwrap()
                .is_err()
        );
    }
    #[tokio::test]
    async fn shared_pipeline_waits_before_start_and_reuses_access_with_a_writer_queued() {
        let (exclusive_tx, exclusive_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let exclusive = tokio::spawn(benchmark_exclusive(async move {
            exclusive_tx.send(()).unwrap();
            release_rx.await.unwrap();
        }));
        exclusive_rx.await.unwrap();
        let (started_tx, mut started_rx) = tokio::sync::oneshot::channel();
        let pipeline = tokio::spawn(async move {
            let mut writer = None;
            with_shared_access(async {
                started_tx.send(()).unwrap();
                // The writer must queue behind our shared permit before nested access.
                writer = Some(tokio::spawn(benchmark_exclusive(async {})));
                tokio::task::yield_now().await;
                assert!(
                    Engine::new("/missing-engine".into())
                        .version()
                        .await
                        .is_err()
                );
            })
            .await;
            writer.unwrap().await.unwrap();
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(40), &mut started_rx)
                .await
                .is_err()
        );
        release_tx.send(()).unwrap();
        exclusive.await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), pipeline)
            .await
            .expect("nested engine access must not deadlock behind queued writer")
            .unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn benchmark_invocation_can_disable_deadline_without_changing_bounded_calls() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("slow-native-test");
        std::fs::write(
            &binary,
            "#!/usr/bin/env python3\nimport time\ntime.sleep(0.1)\nprint('finished')\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let engine = Engine::new(binary.to_string_lossy().into_owned());
        let bounded = engine
            .run_with_limit(directory.path(), "", Some(Duration::from_millis(5)))
            .await;
        assert!(bounded.unwrap_err().to_string().contains("timed out"));
        let unlimited = engine
            .run_with_limit(directory.path(), "", None)
            .await
            .unwrap();
        assert_eq!(unlimited.trim(), "finished");
    }
}
