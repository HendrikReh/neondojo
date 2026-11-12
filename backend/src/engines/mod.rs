//! Local analysis adapters. GNUbg remains the independent import/rules engine.
mod context;
mod game;
mod process;

use crate::{
    engine::{Analysis, Candidate, Engine},
    match_analysis::AnalyzedGame,
};
use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{path::Path, time::Duration};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SageSnapshot {
    pub python: String,
    pub package_version: String,
    pub artifact_fingerprint: String,
    pub threads: u32,
    pub plies: i32,
    pub adapter_version: u32,
    /// Capability proven during installation verification; absent in legacy snapshots.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub one_ply_verified: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EngineFork {
    pub repository: &'static str,
    pub revision: &'static str,
}

impl SageSnapshot {
    pub fn known_fork(&self) -> Option<EngineFork> {
        // Both wheels retain the upstream package version. Identify the
        // documented builds by their full runtime fingerprints.
        let revision = match self.artifact_fingerprint.as_str() {
            "e76f82a04e35c3180e5837a76ce0f6f6b9ecfe459816284a2d25dc51e28d13d3" => {
                "2e71beed9c24b5bbc7564c03374d6d588ff24119"
            }
            "5b741451d4f49e69eaa1063fc44f5de67d9e2a528cf9b75f32733136d55b352b" => {
                "5b146c7d86fa965f2185e3871f3c56df030fd76f"
            }
            _ => return None,
        };
        Some(EngineFork {
            repository: "HendrikReh/bgsage",
            revision,
        })
    }
}

#[derive(Clone)]
pub struct SageEngine {
    snapshot: SageSnapshot,
    slot: process::Slot,
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageCheckerCandidate {
    pub board: [i32; 26],
    pub notation: String,
    pub equity: f64,
    pub probabilities: Vec<f64>,
}

/// All equities and probabilities are from the ORIGINAL doubler/on-roll player.
/// Take/drop response decisions negate take/pass; never reevaluate a flipped board.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageCubeEvaluation {
    pub no_double: f64,
    pub take: f64,
    pub pass: f64,
    pub probabilities: Vec<f64>,
}

fn valid_sage_threads(threads: u32) -> bool {
    i32::try_from(threads)
        .is_ok_and(|threads| threads >= crate::configuration::engine_min_threads("bgsage"))
}

impl SageEngine {
    pub fn new(snapshot: SageSnapshot) -> Result<Self> {
        ensure!(
            matches!(snapshot.plies, 1 | 2) && valid_sage_threads(snapshot.threads),
            "Unsupported Open Sage depth or thread count"
        );
        ensure!(
            snapshot.plies != 1 || snapshot.one_ply_verified,
            "Open Sage 1-ply capability has not been verified"
        );
        ensure!(
            snapshot.adapter_version == 1 && snapshot.package_version == "2.0.20260907",
            "Unsupported Open Sage adapter or package version"
        );
        ensure!(
            snapshot.artifact_fingerprint.len() == 64
                && snapshot
                    .artifact_fingerprint
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit()),
            "Invalid Open Sage artifact fingerprint"
        );
        ensure!(
            Path::new(&snapshot.python).is_absolute(),
            "Open Sage Python path must be absolute"
        );
        let version = format!(
            "Open Sage {}; model {}; {}ply cubeful; threads {}; adapter {}",
            snapshot.package_version,
            snapshot.artifact_fingerprint,
            snapshot.plies,
            snapshot.threads,
            snapshot.adapter_version
        );
        Ok(Self {
            snapshot,
            version,
            slot: process::Slot::General,
        })
    }

    pub(crate) fn for_play(snapshot: SageSnapshot) -> Result<Self> {
        let mut engine = Self::new(snapshot)?;
        engine.slot = process::Slot::Play;
        Ok(engine)
    }

    pub(crate) async fn warm_play(&self) -> Result<()> {
        process::warm_play(&self.snapshot).await
    }

    pub async fn probe(python: String, threads: u32) -> Result<SageSnapshot> {
        let mut snapshot = Self::probe_profile(python.clone(), 2, threads).await?;
        // A failed 1-ply probe keeps the legacy 2-ply installation usable.
        if let Ok(one_ply) = Self::probe_profile(python, 1, threads).await {
            snapshot.one_ply_verified =
                one_ply.artifact_fingerprint == snapshot.artifact_fingerprint;
        }
        Ok(snapshot)
    }

    pub async fn probe_profile(python: String, plies: i32, threads: u32) -> Result<SageSnapshot> {
        ensure!(matches!(plies, 1 | 2), "Unsupported Open Sage depth");
        ensure!(
            valid_sage_threads(threads),
            "Unsupported Open Sage thread count"
        );
        // Preserve the venv executable symlink; canonicalizing it loses pyvenv.cfg.
        let path = Path::new(&python);
        let python = if path.is_absolute() {
            path.to_path_buf()
        } else if python.contains(std::path::is_separator) {
            std::env::current_dir()?.join(path)
        } else {
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|base| base.join(path))
                .find(|path| path.is_file())
                .context("Open Sage Python was not found on PATH")?
        };
        let mut snapshot = SageSnapshot {
            python: python
                .to_str()
                .context("Open Sage Python path is not UTF-8")?
                .into(),
            package_version: "2.0.20260907".into(),
            artifact_fingerprint: String::new(),
            threads,
            plies,
            adapter_version: 1,
            one_ply_verified: plies == 1,
        };
        let identity = process::probe(&snapshot).await?;
        snapshot.package_version = identity["package_version"]
            .as_str()
            .context("Missing Sage package identity")?
            .into();
        snapshot.artifact_fingerprint = identity["artifact_fingerprint"]
            .as_str()
            .context("Missing Sage asset identity")?
            .into();
        Self::new(snapshot.clone())?;
        Ok(snapshot)
    }

    pub fn snapshot(&self) -> &SageSnapshot {
        &self.snapshot
    }
    pub fn binary(&self) -> &str {
        &self.snapshot.python
    }
    pub fn saved_version(&self) -> Option<&str> {
        Some(&self.version)
    }
    pub async fn restart(&self) -> Result<()> {
        process::restart(&self.snapshot).await
    }
    pub async fn version(&self) -> Result<String> {
        process::request(
            &self.snapshot,
            json!({"operation":"identity"}),
            Duration::from_secs(120),
            self.slot,
        )
        .await?;
        Ok(self.version.clone())
    }
    fn check_depth(&self, plies: i32) -> Result<()> {
        ensure!(
            plies == self.snapshot.plies,
            "Requested depth differs from immutable Open Sage profile"
        );
        Ok(())
    }

    pub async fn checker_boards(
        &self,
        position_id: &str,
        match_id: &str,
        dice: Option<[i32; 2]>,
        force_board: Option<[i32; 26]>,
    ) -> Result<Vec<SageCheckerCandidate>> {
        self.checker_boards_with_batch(position_id, match_id, dice, force_board, None)
            .await
    }

    async fn checker_boards_with_batch(
        &self,
        position_id: &str,
        match_id: &str,
        dice: Option<[i32; 2]>,
        force_board: Option<[i32; 26]>,
        batch: Option<&mut process::VerifiedBatch>,
    ) -> Result<Vec<SageCheckerCandidate>> {
        let context = context::Context::decode(match_id)?;
        ensure!(
            !context.pending_double,
            "Cannot play checkers before cube response"
        );
        let board = context.board(position_id, match_id)?;
        let dice = dice.unwrap_or(context.dice);
        ensure!(
            dice.iter().all(|die| (1..=6).contains(die)),
            "Invalid checker dice"
        );
        let payload = json!({"operation":"checker", "board":board, "context":context, "dice":dice, "force_board":force_board.map(|board| context::to_sage(&board, context.player))});
        let result = self.request_with_batch(payload, batch).await?;
        let mut candidates: Vec<SageCheckerCandidate> = serde_json::from_value(result)?;
        ensure!(
            !candidates.is_empty(),
            "Open Sage returned no checker candidates"
        );
        for candidate in &mut candidates {
            validate_evaluation(candidate.equity, &candidate.probabilities)?;
            if candidate.board == board {
                ensure!(
                    candidate.notation.is_empty(),
                    "Invalid forced-pass notation"
                );
                // Existing rule validation independently proves that no die can be played.
                let absolute = context::from_sage(&board, context.player)?;
                crate::import::sgf::apply_checker_turn(&absolute, context.player, dice, &[])?;
                candidate.notation = "Cannot move".into();
            } else {
                ensure!(
                    !candidate.notation.is_empty(),
                    "Open Sage omitted checker notation"
                );
            }
            candidate.board = context::from_sage(&candidate.board, context.player)?;
        }
        candidates.sort_by(|a, b| b.equity.total_cmp(&a.equity));
        Ok(candidates)
    }

    pub async fn analyze(
        &self,
        position_id: &str,
        match_id: &str,
        plies: i32,
        dice: Option<[i32; 2]>,
    ) -> Result<Analysis> {
        self.check_depth(plies)?;
        let candidates = self
            .checker_boards(position_id, match_id, dice, None)
            .await?;
        Ok(Analysis {
            engine: self.version.clone(),
            plies,
            candidates: candidates
                .into_iter()
                .enumerate()
                .map(|(index, candidate)| Candidate {
                    rank: index as i32 + 1,
                    notation: candidate.notation,
                    equity: candidate.equity,
                    probabilities: candidate.probabilities,
                })
                .collect(),
        })
    }

    pub async fn analyze_checker_choices(
        &self,
        position_id: &str,
        match_id: &str,
        plies: i32,
        forced_pass: bool,
    ) -> Result<Analysis> {
        let result = self.analyze(position_id, match_id, plies, None).await?;
        ensure!(
            !forced_pass
                || result.candidates.len() == 1 && result.candidates[0].notation == "Cannot move",
            "Expected a forced pass but Open Sage returned playable moves"
        );
        Ok(result)
    }

    pub async fn accepts_resignation(
        &self,
        position_id: &str,
        match_id: &str,
        level: i32,
    ) -> Result<bool> {
        ensure!((1..=3).contains(&level), "Invalid resignation level");
        let context = context::Context::decode(match_id)?;
        ensure!(
            !context.pending_double,
            "Cannot resign with cube response pending"
        );
        let board = context.board(position_id, match_id)?;
        let result = process::request(
            &self.snapshot,
            json!({"operation":"resignation", "board":board,
            "context":context, "dice":context.dice, "level":level}),
            Duration::from_secs(180),
            self.slot,
        )
        .await?;
        result["accept"]
            .as_bool()
            .context("Open Sage omitted resignation decision")
    }

    pub async fn cube(&self, position_id: &str, match_id: &str) -> Result<SageCubeEvaluation> {
        self.cube_with_batch(position_id, match_id, None).await
    }

    async fn cube_with_batch(
        &self,
        position_id: &str,
        match_id: &str,
        batch: Option<&mut process::VerifiedBatch>,
    ) -> Result<SageCubeEvaluation> {
        let context = context::Context::decode(match_id)?;
        let board = context.board(position_id, match_id)?;
        let value = self
            .request_with_batch(
                json!({"operation":"cube", "board":board, "context":context}),
                batch,
            )
            .await?;
        let result: SageCubeEvaluation = serde_json::from_value(value)?;
        for equity in [result.no_double, result.take, result.pass] {
            validate_evaluation(equity, &result.probabilities)?;
        }
        Ok(result)
    }

    async fn request_with_batch(
        &self,
        payload: serde_json::Value,
        batch: Option<&mut process::VerifiedBatch>,
    ) -> Result<serde_json::Value> {
        let timeout = Duration::from_secs(180);
        match batch {
            Some(batch) => batch.request(payload, timeout).await,
            None => process::request(&self.snapshot, payload, timeout, self.slot).await,
        }
    }
}

fn validate_evaluation(equity: f64, probabilities: &[f64]) -> Result<()> {
    ensure!(
        equity.is_finite()
            && probabilities.len() == 6
            && probabilities
                .iter()
                .all(|p| p.is_finite() && (0.0..=1.0).contains(p)),
        "Invalid Open Sage evaluation"
    );
    ensure!(
        (probabilities[0] + probabilities[3] - 1.0).abs() <= 1e-9
            && probabilities[2] <= probabilities[1]
            && probabilities[1] <= probabilities[0]
            && probabilities[5] <= probabilities[4]
            && probabilities[4] <= probabilities[3] + 1e-12,
        "Inconsistent Open Sage probabilities"
    );
    Ok(())
}

#[derive(Clone)]
pub enum AnalysisEngine {
    Gnubg(Engine),
    OpenSage(SageEngine),
}
impl AnalysisEngine {
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
