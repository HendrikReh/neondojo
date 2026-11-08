use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Stdio, sync::OnceLock, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};
use utoipa::ToSchema;

#[derive(Clone)]
pub struct Engine {
    binary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Candidate {
    pub rank: i32,
    pub notation: String,
    pub equity: f64,
    /// Win, win gammon (including backgammon), win backgammon,
    /// loss, lose gammon (including backgammon), lose backgammon.
    pub probabilities: Vec<f64>,
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
        Self { binary }
    }

    pub async fn version(&self) -> Result<String> {
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            Command::new(&self.binary)
                .arg("--version")
                .env("LC_ALL", "C")
                .kill_on_drop(true)
                .output(),
        )
        .await??;
        ensure!(result.status.success(), "GNUbg is unavailable");
        Ok(String::from_utf8_lossy(&result.stdout)
            .lines()
            .next()
            .unwrap_or("GNUbg")
            .trim()
            .into())
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
        tokio::fs::write(directory.join("commands.txt"), commands).await?;
        let mut child = Command::new(&self.binary)
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
        match tokio::time::timeout(timeout, work).await {
            Ok(result) => result,
            Err(_) => {
                let _ = child.kill().await;
                bail!("GNUbg timed out after {} seconds", timeout.as_secs())
            }
        }
    }

    pub async fn normalize_mat(&self, bytes: &[u8]) -> Result<String> {
        let temp = tempfile::tempdir()?;
        tokio::fs::write(temp.path().join("input.mat"), bytes).await?;
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
        tokio::fs::read_to_string(temp.path().join("canonical.sgf"))
            .await
            .context("GNUbg could not import this MAT file")
    }

    pub async fn analyze(
        &self,
        position_id: &str,
        match_id: &str,
        plies: i32,
        dice: Option<[i32; 2]>,
    ) -> Result<Analysis> {
        ensure!((0..=2).contains(&plies), "plies must be 0..2");
        ensure!(
            valid_id(position_id, 14) && valid_id(match_id, 12),
            "Invalid GNUbg ID"
        );
        let mut commands = format!(
            "set display off\nset gnubgid {position_id}:{match_id}\nset evaluation chequerplay evaluation plies {plies}\nset evaluation cubedecision evaluation plies {plies}\nset output digits 6\n"
        );
        if let Some([a, b]) = dice {
            ensure!((1..=6).contains(&a) && (1..=6).contains(&b), "Invalid dice");
            commands.push_str(&format!("set dice {a} {b}\n"));
        }
        commands.push_str("hint 5\n");
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
            let forced_commands = commands.replace("hint 5\n", "set automatic roll off\nset automatic move off\nset player 0 human\nset player 1 human\nmove\neval\n");
            let output = self.run(temp.path(), &forced_commands).await?;
            vec![parse_pass(&output, plies)?]
        } else {
            parse_hints(&output)?
        };
        Ok(Analysis {
            engine,
            plies,
            candidates,
        })
    }
}

pub fn valid_id(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'+' || c == b'/')
}

fn parse_hints(output: &str) -> Result<Vec<Candidate>> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"^\s*(\d+)\.\s+Cubeful\s+\d+-ply\s+(.+?)\s+Eq\.:\s*([+-]?[\d.]+)").unwrap()
    });
    let mut candidates: Vec<Candidate> = Vec::new();
    for line in output.lines() {
        if let Some(captures) = pattern.captures(line) {
            candidates.push(Candidate {
                rank: captures[1].parse()?,
                notation: captures[2].trim().into(),
                equity: captures[3].parse()?,
                probabilities: vec![],
            });
        } else if let Some(candidate) = candidates.last_mut()
            && candidate.probabilities.is_empty()
        {
            let probabilities: Vec<f64> = line
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            if probabilities.len() == 6 {
                candidate.probabilities = probabilities;
            }
        }
    }
    ensure!(
        !candidates.is_empty(),
        "GNUbg returned no evaluable move candidates"
    );
    ensure!(
        candidates.iter().all(|c| c.equity.is_finite()
            && c.probabilities.len() == 6
            && c.probabilities.iter().all(|p| (0.0..=1.0).contains(p))),
        "Incomplete GNUbg evaluation"
    );
    Ok(candidates)
}

fn parse_pass(output: &str, plies: i32) -> Result<Candidate> {
    let prefix = if plies == 0 {
        "static:".to_owned()
    } else {
        format!("{plies} ply:")
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
        equity: -values[6],
        probabilities: vec![
            1.0 - values[0],
            values[3],
            values[4],
            values[0],
            values[1],
            values[2],
        ],
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
    }

    #[tokio::test]
    #[ignore = "requires local GNUbg with weights"]
    async fn real_gnubg_finds_opening_31() {
        let engine = super::Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let result = engine
            .analyze("4HPwATDgc/ABMA", "cInlAAAAAAAE", 0, None)
            .await
            .unwrap();
        assert_eq!(result.candidates[0].notation, "8/5 6/5");
        assert!(result.candidates[0].equity > 0.1);
    }

    #[tokio::test]
    #[ignore = "requires local GNUbg with weights"]
    async fn real_gnubg_evaluates_a_forced_pass() {
        let engine = super::Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let result = engine
            .analyze("Y54pATDg28EBYA", "cAlnAQAAAAAE", 0, None)
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
