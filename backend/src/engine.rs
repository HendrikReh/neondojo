use crate::async_temp_dir::AsyncTempDir;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};
use utoipa::ToSchema;

pub(crate) mod cpu;
mod gate;

/// Upper bound on simultaneous engine threads for this host (P-core budget on Apple Silicon).
pub async fn host_parallel_threads() -> Option<i32> {
    cpu::HostCpuBudget::host_parallel_threads().await
}
mod installation;
mod parse;
#[cfg(test)]
mod tests;

use gate::engine_permit;
pub(crate) use gate::{
    benchmark_exclusive, try_play_start, with_shared_access, with_shared_access_timeout,
};
pub(crate) use installation::Installation;
pub(crate) use parse::{
    CubeEquities, flip_probabilities, parse_checker_candidates, parse_cube_equities,
    parse_full_checker_hints, parse_hints,
};
use parse::{
    RollProgress, gnubg_diagnostics, parse_cube_depth, parse_pass, reported_threads, split_rolls,
};

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
/// GNUbg passes for the player on roll and evaluates the resulting position.
const FORCED_PASS: &str = "set automatic roll off\nset automatic move off\nset player 0 human\nset player 1 human\nmove\neval\n";
/// The 21 distinct rolls, or all 36 ordered ones.
const MAX_BATCH_ROLLS: usize = 36;
/// The time a single position evaluation had before rolls were batched.
const ROLL_TIMEOUT: Duration = Duration::from_secs(90);

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
        with_shared_access(async {
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

    pub(crate) async fn run(&self, directory: &Path, commands: &str) -> Result<String> {
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
        let temp = AsyncTempDir::new().await?;
        let commands = format!("set threads {cpu_threads}\nshow threads\n");
        let output = self
            .run_with_timeout(temp.path(), &commands, Duration::from_secs(5))
            .await?;
        reported_threads(&output)
            .filter(|count| (1..=cpu_threads).contains(count))
            .context("GNUbg did not report its calculation thread count")
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
        let temp = AsyncTempDir::new().await?;
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
            let candidates = parse_hints(&output, !self.cubeless_checker)?;
            crate::match_analysis::retain_hint_paths(
                &temp.path().join("hint.sgf"),
                position_id,
                candidates,
            )
            .await?
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
        let temp = AsyncTempDir::new().await?;
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
                let candidates = parse_hints(part, !self.cubeless_checker)?;
                crate::match_analysis::retain_hint_paths(
                    &temp.path().join(format!("hint-{index}.sgf")),
                    position_id,
                    candidates,
                )
                .await?
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
        let directory = AsyncTempDir::new().await?;
        let output = self
            .run_with_timeout(directory.path(), &commands, Duration::from_secs(180))
            .await?;
        let (candidates, evaluated_plies) =
            parse_full_checker_hints(&output, plies, !self.cubeless_checker)?;
        let candidates = crate::match_analysis::retain_hint_paths(
            &directory.path().join("hint.sgf"),
            position_id,
            candidates,
        )
        .await?;
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
        let directory = AsyncTempDir::new().await?;
        let output = self.run(directory.path(), &commands).await?;
        let engine = output
            .lines()
            .find(|line| line.starts_with("GNU Backgammon "))
            .context("GNUbg did not report its version")?;
        let effective = parse_cube_depth(&output)?;
        ensure!(
            effective == plies,
            "GNUbg used a different cube evaluation depth"
        );
        Ok((engine.to_owned(), effective, parse_cube_equities(&output)?))
    }
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
        commands.push_str(&format!(
            "set evaluation {kind} evaluation plies {native}\nset evaluation {kind} evaluation cubeful on\nset evaluation {kind} evaluation deterministic on\nset evaluation {kind} evaluation noise 0\n"
        ));
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

pub fn valid_id(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'+' || c == b'/')
}
