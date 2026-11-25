use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Stdio, sync::OnceLock, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};
use utoipa::ToSchema;

pub(crate) mod cpu;
mod installation;
pub(crate) use installation::Installation;

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

/// Hold admission through the entire Play startup. A pending benchmark writer
/// also rejects new sessions; nested subprocesses reuse the outer read guard.
pub(crate) async fn try_play_start<F: std::future::Future>(work: F) -> Option<F::Output> {
    let _admission = ENGINE_WORK.try_read().ok()?;
    Some(ENGINE_ACCESS.scope((), work).await)
}

/// Bound a request's total time, including admission to the shared engine gate.
pub(crate) async fn with_shared_access_timeout<F: std::future::Future>(
    timeout: Duration,
    work: F,
) -> Result<F::Output, tokio::time::error::Elapsed> {
    tokio::time::timeout(timeout, with_shared_access(work)).await
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

/// [`resolve_binary`] for async callers. The `$PATH` search and
/// canonicalization query the file system, so they run on the blocking pool.
pub(crate) async fn resolve_binary_async(binary: &str) -> Result<String> {
    let binary = binary.to_owned();
    tokio::task::spawn_blocking(move || resolve_binary(&binary)).await?
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

/// Deepest GNUbg evaluation NeonDojo offers, in NeonDojo's ply convention.
pub(crate) const GNUBG_MAX_PLIES: i32 = 5;

/// NeonDojo stores, shows and compares every depth in the XG / NeonGammon
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
        (crate::engines::EngineId::parse(engine_id) == Some(crate::engines::EngineId::Gnubg)
            && matches!(gnubg_plies(plies), Ok(2 | 4)))
        .then_some(Self::Top14)
    }
}

/// Upper bound for the version probe plus one raw evaluation.
const INSTALLATION_CHECK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct Engine {
    binary: String,
    cubeless_checker: bool,
    cpu_workload: cpu::Workload,
    expected_version: Option<String>,
    threads: Option<i32>,
    move_filter: Option<GnubgMoveFilter>,
    installation_timeout: Duration,
    roll_timeout: Duration,
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
    /// in NeonDojo's ply convention (1-ply is the raw evaluation).
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
            cubeless_checker: false,
            cpu_workload: cpu::Workload::Background,
            expected_version: None,
            threads: None,
            move_filter: None,
            installation_timeout: INSTALLATION_CHECK_TIMEOUT,
            roll_timeout: ROLL_TIMEOUT,
        }
    }

    pub(crate) fn interactive(mut self) -> Self {
        self.cpu_workload = cpu::Workload::Foreground;
        self
    }

    pub(crate) async fn reserve_cpu(&self, threads: u32) -> Result<cpu::CpuPermit<'static>> {
        cpu::HostCpuBudget::host()
            .await
            .acquire(threads, self.cpu_workload)
            .await
    }

    pub(crate) fn with_cubeless_checker(mut self) -> Self {
        self.cubeless_checker = true;
        self
    }

    fn configure_checker_rules(&self, commands: &mut String) {
        if self.cubeless_checker {
            commands
                .push_str("set cube use off\nset evaluation chequerplay evaluation cubeful off\n");
        }
    }

    /// Lets tests prove the bound without waiting the production ten seconds.
    #[cfg(test)]
    pub(crate) fn with_installation_timeout(mut self, timeout: Duration) -> Self {
        self.installation_timeout = timeout;
        self
    }

    pub(crate) fn with_threads(mut self, threads: Option<i32>) -> Self {
        self.threads = threads.filter(|&threads| threads != 1);
        self
    }

    pub(crate) fn calculation_threads(&self) -> i32 {
        self.threads.unwrap_or(1)
    }

    pub(crate) fn thread_commands(&self, commands: &str) -> Result<String> {
        if let Some(threads) = self.threads {
            ensure!(
                threads >= crate::engines::EngineId::Gnubg.min_threads(),
                "Unsupported GNUbg thread count: {threads}"
            );
            Ok(format!("set threads {threads}\nshow threads\n{commands}"))
        } else {
            Ok(commands.to_owned())
        }
    }

    pub(crate) fn verify_threads(&self, output: &str) -> Result<()> {
        if let Some(threads) = self.threads {
            ensure!(
                reported_threads(output) == Some(threads),
                "GNUbg did not accept the requested {threads} calculation threads"
            );
        }
        Ok(())
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
                tokio::time::timeout(self.installation_timeout, async {
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
                .with_context(|| {
                    format!(
                        "GNUbg installation check timed out after {} seconds",
                        self.installation_timeout.as_secs()
                    )
                })?
            })
            .await
    }

    pub async fn version(&self) -> Result<String> {
        let _permit = engine_permit().await;
        self.version_unlocked(&resolve_binary_async(&self.binary).await?)
            .await
    }

    /// Keep the health check responsive when a benchmark owns the engine.
    /// A started version probe uses its normal subprocess timeout.
    pub(crate) async fn health_version(&self) -> Result<String> {
        let (_permit, _cpu) = tokio::time::timeout(Duration::from_millis(300), async {
            let permit = engine_permit().await;
            let cpu = cpu::HostCpuBudget::host()
                .await
                .acquire(1, cpu::Workload::Foreground)
                .await?;
            anyhow::Ok((permit, cpu))
        })
        .await
        .context("GNUbg is busy")??;
        self.run_version(&resolve_binary_async(&self.binary).await?)
            .await
    }

    async fn version_unlocked(&self, binary: &str) -> Result<String> {
        let _cpu = self.reserve_cpu(1).await?;
        self.run_version(binary).await
    }

    async fn run_version(&self, binary: &str) -> Result<String> {
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
        self.run_observed(directory, commands, timeout, |_| {})
            .await
    }

    /// Like `run_with_limit`, and passes the standard output read so far to
    /// `observe` after each chunk while GNUbg runs.
    async fn run_observed(
        &self,
        directory: &Path,
        commands: &str,
        timeout: Option<Duration>,
        mut observe: impl FnMut(&[u8]) + Send,
    ) -> Result<String> {
        let _permit = engine_permit().await;
        // A job may outlive an upgrade at the same path. Fence every native
        // invocation, including later games, dice batches and forced-pass calls.
        let installation = match self.expected_version {
            Some(_) => Some(
                Installation::bind(self)
                    .await
                    .context("Cannot execute the saved GNUbg job configuration")?,
            ),
            None => None,
        };
        // Start exactly the executable that was checked.
        let binary = match &installation {
            Some(checked) => checked.executable().to_owned(),
            None => resolve_binary_async(&self.binary)
                .await
                .context("Could not start GNUbg; check GNUBG_BIN")?,
        };
        // A direct binary proves its version with its own startup banner.
        let mut startup = installation
            .as_ref()
            .filter(|installation| installation.reports_version_at_startup());
        tokio::fs::write(
            directory.join("commands.txt"),
            self.thread_commands(commands)?,
        )
        .await?;
        let cpu = self
            .reserve_cpu(u32::try_from(self.calculation_threads())?)
            .await?;
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
        let observed = async {
            let mut stdout = stdout;
            let mut output = Vec::new();
            let mut chunk = vec![0; 64 * 1024];
            loop {
                let read = stdout.read(&mut chunk).await?;
                if read == 0 {
                    break;
                }
                output.extend_from_slice(&chunk[..read]);
                ensure!(
                    output.len() <= MAX_OUTPUT as usize,
                    "GNUbg output limit exceeded"
                );
                // Stop another version before anyone observes its output.
                if let Some(installation) = startup
                    && let Some(end) = output.iter().position(|&byte| byte == b'\n')
                {
                    installation
                        .check_startup_banner(self, &output[..end])
                        .context("Cannot execute the saved GNUbg job configuration")?;
                    startup = None;
                }
                observe(&output);
            }
            if let Some(installation) = startup {
                installation
                    .check_startup_banner(self, &output)
                    .context("Cannot execute the saved GNUbg job configuration")?;
            }
            Ok::<_, anyhow::Error>(output)
        };
        let work = async {
            let (out, err, status) = tokio::try_join!(observed, read(Box::pin(stderr)), async {
                Ok::<_, anyhow::Error>(child.wait().await?)
            })?;
            ensure!(
                status.success(),
                "GNUbg process failed: {}",
                String::from_utf8_lossy(&err)
            );
            // GNUbg can return exit code 0 after a failed command. Validate diagnostics and artifacts too.
            let err = gnubg_diagnostics(&String::from_utf8_lossy(&err));
            ensure!(err.is_empty(), "GNUbg: {err}");
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
        drop(cpu); // Wrapper rechecks start their own one-thread version process.
        self.verify_threads(&output)?;
        // Discard output if an installation replacement raced the invocation;
        // no checkpoint or completed result may incorporate another version.
        if let Some(installation) = &installation {
            installation
                .recheck(self)
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
        crate::import::mat::normalize_native_sgf(&native, crate::import::mat::event_date(bytes))
    }

    /// Import a MAT transcript, ending each game before a move that fails
    /// [`Self::normalize_mat`]. The warnings name every such game and each
    /// header score that contradicts the previous result.
    pub async fn import_mat(&self, bytes: &[u8]) -> Result<crate::import::ConvertedMatch> {
        let recovered =
            crate::import::mat::recover(
                bytes,
                |text| async move { self.normalize_mat(&text).await },
            )
            .await?;
        Ok(crate::import::ConvertedMatch {
            sgf: recovered.sgf,
            warnings: recovered.warnings,
        })
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
        self.configure_checker_rules(&mut commands);
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
            let forced_commands = commands.replace(&hint, FORCED_PASS);
            let output = self.run(temp.path(), &forced_commands).await?;
            vec![parse_pass(&output, plies, self.cubeless_checker)?]
        } else {
            let mut candidates = parse_hints(&output, !self.cubeless_checker)?;
            crate::match_analysis::retain_hint_paths(
                &temp.path().join("hint.sgf"),
                position_id,
                &mut candidates,
            )
            .await;
            candidates
        };
        Ok(Analysis {
            engine,
            plies,
            candidates,
        })
    }

    /// Evaluate the position once for each roll in one GNUbg process and return
    /// the analyses in the given order. Rolls that the rules cannot play are
    /// evaluated as forced passes. `evaluated` receives the number of finished
    /// rolls while GNUbg runs. The usual version check brackets the process,
    /// so a replacement during the batch discards all of its rolls.
    pub(crate) async fn analyze_rolls<F>(
        &self,
        position_id: &str,
        match_id: &str,
        plies: i32,
        rolls: &[[i32; 2]],
        mut evaluated: impl FnMut(usize) -> F,
    ) -> Result<Vec<Analysis>>
    where
        F: std::future::Future<Output = Result<()>>,
    {
        let native = gnubg_plies(plies)?;
        ensure!(
            valid_id(position_id, 14) && valid_id(match_id, 12),
            "Invalid GNUbg ID"
        );
        ensure!(
            (1..=MAX_BATCH_ROLLS).contains(&rolls.len()),
            "Invalid number of rolls"
        );
        let mut blocked = Vec::with_capacity(rolls.len());
        for &dice in rolls {
            ensure!(dice.iter().all(|die| (1..=6).contains(die)), "Invalid dice");
            blocked.push(crate::engines::forced_pass(position_id, match_id, dice)?);
        }
        let mut commands = format!(
            "set display off\nset evaluation chequerplay evaluation plies {native}\nset evaluation cubedecision evaluation plies {native}\nset output digits 6\n"
        );
        self.configure_checker_rules(&mut commands);
        let count = self
            .append_move_filter(&mut commands, "evaluation", plies)
            .unwrap_or(10);
        // Passing changes the player settings, so forced passes come last.
        let order: Vec<usize> = (0..rolls.len())
            .filter(|&index| !blocked[index])
            .chain((0..rolls.len()).filter(|&index| blocked[index]))
            .collect();
        for &index in &order {
            let [a, b] = rolls[index];
            // Every roll starts again from the original position and match context.
            commands.push_str(&format!(
                "set gnubgid {position_id}:{match_id}\nset dice {a} {b}\n"
            ));
            if blocked[index] {
                commands.push_str(FORCED_PASS);
            } else {
                commands.push_str(&format!("hint {count}\nsave game hint-{index}.sgf\n"));
            }
        }
        let temp = tempfile::tempdir()?;
        let (finished, mut progress) = tokio::sync::watch::channel(0);
        let mut roll_output = RollProgress::default();
        let last_in_flight = rolls.len() - 1;
        let run = self.run_observed(
            temp.path(),
            &commands,
            Some(self.roll_timeout * rolls.len() as u32),
            move |output| {
                // The final roll is only confirmed after validating the batch.
                let done = roll_output.finished_rolls(output).min(last_in_flight);
                finished.send_if_modified(|reported| {
                    let changed = *reported != done;
                    *reported = done;
                    changed
                });
            },
        );
        // Reports progress until GNUbg finishes; only a failed report ends it.
        let report = async {
            while progress.changed().await.is_ok() {
                let done = *progress.borrow_and_update();
                evaluated(done).await?;
            }
            std::future::pending::<()>().await;
            Ok::<(), anyhow::Error>(())
        };
        let output = tokio::select! {
            biased;
            output = run => output?,
            reported = report => {
                reported?;
                bail!("Dice progress reporting stopped")
            }
        };
        let engine = output
            .lines()
            .find(|l| l.starts_with("GNU Backgammon "))
            .unwrap_or("GNU Backgammon")
            .to_string();
        let expected: Vec<[i32; 2]> = order.iter().map(|&index| rolls[index]).collect();
        let parts = split_rolls(&output, &expected)?;
        let mut analyses: Vec<Option<Analysis>> = rolls.iter().map(|_| None).collect();
        for (&index, part) in order.iter().zip(parts) {
            let [a, b] = rolls[index];
            let candidates = if blocked[index] {
                vec![parse_pass(part, plies, self.cubeless_checker)?]
            } else {
                ensure!(
                    !part.contains("There are no legal moves."),
                    "GNUbg found no legal move for {a}-{b}, but the rules allow one"
                );
                let mut candidates = parse_hints(part, !self.cubeless_checker)?;
                crate::match_analysis::retain_hint_paths(
                    &temp.path().join(format!("hint-{index}.sgf")),
                    position_id,
                    &mut candidates,
                )
                .await;
                candidates
            };
            analyses[index] = Some(Analysis {
                engine: engine.clone(),
                plies,
                candidates,
            });
        }
        evaluated(rolls.len()).await?;
        analyses
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .context("GNUbg skipped a roll")
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
        let mut commands = decision_hint_commands(position_id, match_id, plies)?;
        self.configure_checker_rules(&mut commands);
        commands.push_str("hint 10000\nsave game hint.sgf\n");
        let directory = tempfile::tempdir()?;
        let output = self
            .run_with_timeout(directory.path(), &commands, Duration::from_secs(180))
            .await?;
        let (mut candidates, evaluated_plies) =
            parse_full_checker_hints(&output, plies, !self.cubeless_checker)?;
        crate::match_analysis::retain_hint_paths(
            &directory.path().join("hint.sgf"),
            position_id,
            &mut candidates,
        )
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

/// GNUbg's standard-error diagnostics, each distinct line once in first-seen
/// order. GNUbg repeats some messages, such as the MAT reader's "not a valid
/// .mat file", once per parse attempt.
fn gnubg_diagnostics(stderr: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && seen.insert(*line))
        .collect::<Vec<_>>()
        .join("\n")
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

/// GNUbg passes for the player on roll and evaluates the resulting position.
const FORCED_PASS: &str = "set automatic roll off\nset automatic move off\nset player 0 human\nset player 1 human\nmove\neval\n";
/// Printed by `set dice`; it starts each roll's output in a batch.
const ROLL_MARKER: &str = "The dice have been set to ";
/// The 21 distinct rolls, or all 36 ordered ones.
const MAX_BATCH_ROLLS: usize = 36;
/// The time a single position evaluation had before rolls were batched.
const ROLL_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Default)]
struct RollProgress {
    scan_from: usize,
    started: usize,
}

impl RollProgress {
    fn finished_rolls(&mut self, output: &[u8]) -> usize {
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
fn split_rolls<'a>(output: &'a str, rolls: &[[i32; 2]]) -> Result<Vec<&'a str>> {
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

fn parse_pass(output: &str, plies: i32, cubeless: bool) -> Result<Candidate> {
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

#[cfg(test)]
mod tests {
    #[test]
    fn reports_each_gnubg_diagnostic_line_once() {
        let stderr = "input.mat: not a valid .mat file\n".repeat(6);
        assert_eq!(
            super::gnubg_diagnostics(&stderr),
            "input.mat: not a valid .mat file"
        );
        assert_eq!(
            super::gnubg_diagnostics("  first\n\nsecond\r\nfirst\n second \n"),
            "first\nsecond"
        );
        assert_eq!(super::gnubg_diagnostics(" \n\t\n"), "");
    }

    #[test]
    fn parses_ranked_engine_hints_and_rejects_missing_results() {
        let output = "    1. Cubeful 0-ply    8/5 6/5                      Eq.: +0.219\n       0.551 0.174 0.013 - 0.449 0.124 0.005\n        0-ply cubeful prune [expert]\n    2. Cubeful 0-ply    24/23 13/10                  Eq.: -0.012 (-0.231)\n       0.497 0.137 0.008 - 0.503 0.140 0.007\n";
        let hints = super::parse_hints(output, true).unwrap();
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0].notation, "8/5 6/5");
        assert_eq!(hints[0].equity, 0.219);
        assert_eq!(hints[0].probabilities[0], 0.551);
        assert!(super::parse_hints("No game in progress", true).is_err());
        assert!(super::parse_full_checker_hints(output, 2, true).is_err());
        let single = output.split("    2.").next().unwrap();
        // GNUbg's native 0-ply is NeonDojo's 1-ply raw evaluation.
        assert_eq!(
            super::parse_full_checker_hints(single, 1, true).unwrap().1,
            1
        );
        assert_eq!(hints[0].plies, Some(1));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn installation_check_bounds_a_hanging_evaluation() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::{Duration, Instant};
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("hanging-gnubg");
        std::fs::write(
            &binary,
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'GNU Backgammon fixture'; exit 0; fi\nexec sleep 60\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let engine = super::Engine::new(binary.to_str().unwrap().to_owned())
            .with_installation_timeout(Duration::from_millis(300));
        let started = Instant::now();
        let error = engine.check_installation().await.unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error:#}");
        // The timed-out evaluation is dropped, which kills the hanging child.
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    /// A wrapper fixture whose run executes `during_run` and logs every launch.
    #[cfg(unix)]
    fn fenced_wrapper(during_run: &str) -> (tempfile::TempDir, super::Engine) {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_str().unwrap();
        std::fs::write(directory.path().join("version"), "GNU Backgammon fixture\n").unwrap();
        let binary = directory.path().join("gnubg-wrapper");
        std::fs::write(
            &binary,
            format!(
                "#!/bin/sh\nif [ \"$1\" = --version ]; then echo version >>'{root}/launches'; cat '{root}/version'; exit 0; fi\necho run >>'{root}/launches'\ncat '{root}/version'\n{during_run}\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let engine = super::Engine::new(binary.to_str().unwrap().to_owned())
            .with_expected_version(Some("GNU Backgammon fixture".into()));
        (directory, engine)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn saved_version_run_rechecks_a_wrapper_and_its_bytes() {
        let (directory, engine) = fenced_wrapper("");
        let output = engine
            .run_with_timeout(directory.path(), "", std::time::Duration::from_secs(5))
            .await
            .unwrap();
        assert!(output.starts_with("GNU Backgammon fixture"));
        assert_eq!(
            std::fs::read_to_string(directory.path().join("launches")).unwrap(),
            "version\nrun\nversion\n"
        );
        // Without a saved version no invocation is fenced.
        let unfenced = super::Engine::new(engine.binary().to_owned());
        std::fs::remove_file(directory.path().join("launches")).unwrap();
        unfenced
            .run_with_timeout(directory.path(), "", std::time::Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(directory.path().join("launches")).unwrap(),
            "run\n"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn saved_version_run_discards_output_after_same_version_replacement() {
        // The replacement reports the same version, so only its bytes differ.
        let (directory, engine) = fenced_wrapper(
            "cp \"$0\" \"$0.new\"; echo '# replaced' >>\"$0.new\"; mv \"$0.new\" \"$0\"",
        );
        let error = engine
            .run_with_timeout(directory.path(), "", std::time::Duration::from_secs(5))
            .await
            .unwrap_err();
        let error = format!("{error:#}");
        assert!(
            error.contains("GNUbg changed during execution")
                && error.contains("installation changed"),
            "{error}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn saved_version_run_rechecks_the_wrapper_target_version() {
        let (directory, engine) =
            fenced_wrapper("echo 'GNU Backgammon new' >\"$(dirname \"$0\")/version\"");
        let error = engine
            .run_with_timeout(directory.path(), "", std::time::Duration::from_secs(5))
            .await
            .unwrap_err();
        let error = format!("{error:#}");
        assert!(
            error.contains("GNUbg changed during execution")
                && error.contains("found 'GNU Backgammon new'"),
            "{error}"
        );
    }

    /// The real executable copied to a temporary path, so it is a direct
    /// binary whose bytes the test can replace.
    #[cfg(unix)]
    fn copied_gnubg() -> (tempfile::TempDir, String, String) {
        let source =
            super::resolve_binary(&std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("gnubg");
        std::fs::copy(&source, &binary).unwrap();
        let version = std::process::Command::new(&binary)
            .arg("--version")
            .output()
            .unwrap();
        let version = String::from_utf8(version.stdout).unwrap();
        let version = version.lines().next().unwrap().trim().to_owned();
        (directory, binary.to_str().unwrap().to_owned(), version)
    }

    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "requires a local native GNUbg binary"]
    async fn saved_version_run_of_a_direct_binary_checks_its_banner_and_bytes() {
        use std::time::{Duration, Instant};
        let (directory, binary, version) = copied_gnubg();
        let work = tempfile::tempdir().unwrap();
        let fenced =
            super::Engine::new(binary.clone()).with_expected_version(Some(version.clone()));
        let output = fenced
            .run_with_timeout(work.path(), "show threads\n", Duration::from_secs(10))
            .await
            .unwrap();
        assert!(output.starts_with(&version));

        // The banner of the running process is the saved-version check.
        let error = super::Engine::new(binary.clone())
            .with_expected_version(Some("GNU Backgammon 0.0".into()))
            .run_with_timeout(work.path(), "show threads\n", Duration::from_secs(10))
            .await
            .unwrap_err();
        let error = format!("{error:#}");
        assert!(
            error.contains("Cannot execute the saved GNUbg job configuration")
                && error.contains(&format!("found '{version}'")),
            "{error}"
        );

        // Fencing cost of one run: two version probes before, the bytes now.
        let unfenced = super::Engine::new(binary.clone());
        let started = Instant::now();
        for _ in 0..5 {
            unfenced.version().await.unwrap();
            unfenced.version().await.unwrap();
        }
        let probes = started.elapsed() / 5;
        let started = Instant::now();
        for _ in 0..5 {
            let installation = super::Installation::bind(&fenced).await.unwrap();
            installation.recheck(&fenced).await.unwrap();
        }
        eprintln!(
            "fence per run: two --version probes {probes:?}, digest before and after {:?}",
            started.elapsed() / 5
        );

        // A replacement after the start reports the same version, yet differs.
        let replace = format!(
            "cp '{binary}' '{binary}.new' && printf x >>'{binary}.new' && mv '{binary}.new' '{binary}'"
        );
        let installation = super::Installation::bind(&fenced).await.unwrap();
        assert!(installation.reports_version_at_startup());
        assert!(
            std::process::Command::new("/bin/sh")
                .args(["-c", &replace])
                .status()
                .unwrap()
                .success()
        );
        let error = installation.recheck(&fenced).await.unwrap_err();
        assert!(
            format!("{error:#}").contains("installation changed"),
            "{error:#}"
        );
        drop(directory);
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

    fn all_rolls() -> Vec<[i32; 2]> {
        (1..=6).flat_map(|a| (a..=6).map(move |b| [a, b])).collect()
    }

    /// The batch must reproduce each individual evaluation exactly.
    async fn assert_batch_matches_individual_rolls(
        engine: &super::Engine,
        position_id: &str,
        match_id: &str,
        plies: i32,
    ) -> Vec<super::Analysis> {
        let rolls = all_rolls();
        let mut reported = Vec::new();
        let batch = engine
            .analyze_rolls(position_id, match_id, plies, &rolls, |done| {
                reported.push(done);
                std::future::ready(Ok(()))
            })
            .await
            .unwrap();
        assert_eq!(reported.last(), Some(&21));
        assert!(
            reported.windows(2).all(|pair| pair[0] <= pair[1]),
            "{reported:?}"
        );
        for (dice, analysis) in rolls.iter().zip(&batch) {
            let single = engine
                .analyze(position_id, match_id, plies, Some(*dice))
                .await
                .unwrap();
            assert_eq!(
                serde_json::to_value(analysis).unwrap(),
                serde_json::to_value(&single).unwrap(),
                "{dice:?} at {plies}-ply"
            );
        }
        batch
    }

    #[tokio::test]
    #[ignore = "requires local GNUbg"]
    async fn real_gnubg_roll_batch_matches_individual_evaluations() {
        let binary = std::env::var("GNUBG_BIN").unwrap_or("gnubg".into());
        let engine = super::Engine::new(binary.clone());
        let opening =
            assert_batch_matches_individual_rolls(&engine, "4HPwATDgc/ABMA", "cInlAAAAAAAE", 1)
                .await;
        assert!(
            opening
                .iter()
                .all(|analysis| analysis.candidates[0].path.is_some())
        );
        // NeonDojo's 3-ply is GNUbg's 2-ply, pruned by the analysis move filter.
        let filtered =
            super::Engine::new(binary).with_move_filter(Some(super::GnubgMoveFilter::Top14));
        assert_batch_matches_individual_rolls(&filtered, "4HPwATDgc/ABMA", "cInlAAAAAAAE", 3).await;
    }

    #[tokio::test]
    #[ignore = "requires local GNUbg"]
    async fn real_gnubg_roll_batch_evaluates_forced_passes_in_place() {
        let engine = super::Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let analyses =
            assert_batch_matches_individual_rolls(&engine, "Y54pATDg28EBYA", "cAlnAQAAAAAE", 1)
                .await;
        let passes: Vec<[i32; 2]> = all_rolls()
            .into_iter()
            .zip(&analyses)
            .filter(|(_, analysis)| analysis.candidates[0].notation == "dance")
            .map(|(dice, _)| dice)
            .collect();
        assert_eq!(passes, [[1, 1], [1, 4], [1, 6], [4, 4], [4, 6], [6, 6]]);
    }

    #[tokio::test]
    #[ignore = "requires local GNUbg"]
    async fn real_gnubg_roll_batch_stops_when_progress_cannot_be_saved() {
        let engine = super::Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let error = engine
            .analyze_rolls("4HPwATDgc/ABMA", "cInlAAAAAAAE", 1, &all_rolls(), |done| {
                std::future::ready(if done > 0 {
                    Err(anyhow::anyhow!("Job lease was lost"))
                } else {
                    Ok(())
                })
            })
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "Job lease was lost");
    }

    #[cfg(unix)]
    fn dice_fixture(script: &str) -> (tempfile::TempDir, super::Engine) {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("dice-fixture");
        std::fs::write(&binary, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let engine = super::Engine::new(binary.to_str().unwrap().to_owned());
        (directory, engine)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dice_progress_never_finishes_on_extra_markers() {
        let (directory, mut engine) = dice_fixture(
            r#"printf '%s\n' "$$" > "${0%/*}/pid"
             printf '%s\n' 'GNU Backgammon fixture' \
              'The dice have been set to 1 and 1.' \
              'The dice have been set to 1 and 2.' \
              'The dice have been set to 1 and 3.' \
              'The dice have been set to 1 and 4.'
             exec sleep 60"#,
        );
        assert_eq!(engine.roll_timeout, std::time::Duration::from_secs(90));
        engine.roll_timeout = std::time::Duration::from_millis(500);
        let mut reports = Vec::new();
        let error = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            engine.analyze_rolls(
                "4HPwATDgc/ABMA",
                "cInlAAAAAAAE",
                1,
                &[[1, 1], [1, 2]],
                |done| {
                    reports.push(done);
                    std::future::ready(Ok(()))
                },
            ),
        )
        .await
        .expect("the batch must enforce its scaled deadline")
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("GNUbg timed out after 1 seconds"),
            "{error:#}"
        );
        assert_eq!(reports, [1]);
        let pid = std::fs::read_to_string(directory.path().join("pid")).unwrap();
        let process = std::process::Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .output()
            .unwrap();
        assert!(!process.status.success(), "timed-out child was not reaped");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dice_batch_rejects_output_past_the_limit() {
        let (_directory, engine) = dice_fixture("exec head -c 8388609 /dev/zero");
        let error = engine
            .analyze_rolls("4HPwATDgc/ABMA", "cInlAAAAAAAE", 1, &[[1, 1]], |_| {
                std::future::ready(Ok(()))
            })
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "GNUbg output limit exceeded");
    }

    #[test]
    fn roll_progress_counts_markers_once_across_read_boundaries() {
        let output = b"banner\nThe dice have been set to 1 and 1.\nanalysis\nThe dice have been set to 1 and 2.\nanalysis\nThe dice have been set to 1 and 3.\n";
        let marker = format!("\n{}", super::ROLL_MARKER);
        for chunk_size in 1..=marker.len() + 1 {
            let mut progress = super::RollProgress::default();
            for end in (chunk_size..output.len())
                .step_by(chunk_size)
                .chain([output.len()])
            {
                let prefix = &output[..end];
                let expected = prefix
                    .windows(marker.len())
                    .filter(|window| *window == marker.as_bytes())
                    .count()
                    .saturating_sub(1);
                assert_eq!(
                    progress.finished_rolls(prefix),
                    expected,
                    "chunk size {chunk_size}, length {end}"
                );
            }
        }
    }

    #[test]
    #[ignore = "requires replaying a synthetic 8 MiB GNUbg batch in a release build"]
    fn benchmark_dice_progress_replay() {
        use std::{hint::black_box, time::Instant};
        const CHUNK_BYTES: usize = 8 * 1024;
        for bytes in [80 * 1024, 8 * 1024 * 1024] {
            let mut output = vec![b'x'; bytes];
            for end in output.iter_mut().skip(79).step_by(80) {
                *end = b'\n';
            }
            let rolls = all_rolls();
            for (index, [a, b]) in rolls.iter().enumerate() {
                let marker = format!("\n{}{a} and {b}.\n", super::ROLL_MARKER);
                let start = (index + 1) * (bytes / (rolls.len() + 1));
                output[start..start + marker.len()].copy_from_slice(marker.as_bytes());
            }
            let measure = |observe: &mut dyn FnMut(&[u8]) -> usize| {
                let started = Instant::now();
                let mut finished = 0;
                for end in (CHUNK_BYTES..bytes).step_by(CHUNK_BYTES).chain([bytes]) {
                    finished = black_box(observe(black_box(&output[..end])));
                }
                let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;
                assert_eq!(finished, rolls.len() - 1);
                elapsed_ms
            };
            let mut before = Vec::new();
            let mut after = Vec::new();
            for _ in 0..5 {
                // Mirror the full-prefix scan from main (0c8d0d0).
                before.push(measure(&mut |prefix| {
                    let marker = format!("\n{}", super::ROLL_MARKER);
                    prefix
                        .windows(marker.len())
                        .filter(|window| *window == marker.as_bytes())
                        .count()
                        .saturating_sub(1)
                }));
                let mut scanner = super::RollProgress::default();
                after.push(measure(&mut |prefix| scanner.finished_rolls(prefix)));
            }
            before.sort_by(f64::total_cmp);
            after.sort_by(f64::total_cmp);
            println!(
                "{bytes} bytes, {CHUNK_BYTES} bytes/read, median ms {:.4} -> {:.4}",
                before[2], after[2]
            );
        }
    }

    #[test]
    fn roll_output_is_split_at_the_dice_markers_in_order() {
        let output = "banner\nThe dice have been set to 1 and 2.\n    1. first\nSetting GNUbg ID x\nboard\nThe dice have been set to 6 and 6.\n    1. second\n";
        let parts = super::split_rolls(output, &[[1, 2], [6, 6]]).unwrap();
        assert_eq!(
            parts,
            [
                "The dice have been set to 1 and 2.\n    1. first\n",
                "The dice have been set to 6 and 6.\n    1. second\n"
            ]
        );
        assert!(super::split_rolls(output, &[[6, 6], [1, 2]]).is_err());
        assert!(super::split_rolls(output, &[[1, 2], [6, 6], [3, 3]]).is_err());
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
        use tower::ServiceExt;
        let app = crate::api::router(crate::api::AppState::new(
            sqlx::PgPool::connect_lazy("postgres://unused").unwrap(),
            Engine::new("/missing-engine".into()),
        ));
        let response = tokio::time::timeout(
            Duration::from_secs(1),
            app.oneshot(
                axum::http::Request::post("/api/play/sessions")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"human_player":0,"match_length":5}"#,
                    ))
                    .unwrap(),
            ),
        )
        .await
        .expect("Play startup waited on the benchmark writer")
        .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
        let body = axum::body::to_bytes(response.into_body(), 2048)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&body).contains("benchmark"));
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
