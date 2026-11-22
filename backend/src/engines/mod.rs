//! Local analysis adapters. GNUbg remains the independent import/rules engine.
mod context;
mod game;
mod process;
mod provider;

pub(crate) use provider::{EngineId, display_name};

use crate::{
    engine::{Analysis, Candidate, Engine},
    match_analysis::AnalyzedGame,
};
use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
use utoipa::ToSchema;

/// NeonGammon package versions NeonDojo accepts (`neongammon` releases of
/// HendrikReh/neongammon). The sidecar checks the same list in
/// `SUPPORTED_VERSIONS` of `tools/probe_open_sage.py`.
pub const SUPPORTED_SAGE_PACKAGE_VERSIONS: [&str; 2] = ["3.0.0", "3.1.0"];

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
    /// Cubeless evaluation for cubeless money duels; absent means cubeful.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cubeless: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EngineFork {
    pub repository: &'static str,
    pub revision: &'static str,
}

impl SageSnapshot {
    pub(crate) fn supports_difficulty_presets(&self) -> bool {
        self.package_version == "3.1.0"
    }

    pub fn known_fork(&self) -> Option<EngineFork> {
        // Identify the documented releases by their full runtime fingerprints.
        let revision = match self.artifact_fingerprint.as_str() {
            // Release v3.1.0, official GitHub cp312 wheel.
            "26e7701c27c6c5e0cd3cc864768e27675f0d72149c2ad545ffb0c5293ff705d5" => {
                "04e0bb702c3f7549a05f91af4e862fdef2af954f"
            }
            // Release v3.0.0, official GitHub cp312 wheel.
            "621c35546142fe9d4762ad763e3c2dac3372a03c8a67c6d264fa634dfc69f7f7" => {
                "cbfcb014c7f8cf33d75e7f06cbdfc77260cb53ed"
            }
            _ => return None,
        };
        Some(EngineFork {
            repository: "HendrikReh/neongammon",
            revision,
        })
    }
}

/// Named opponent presets backed by the calibrated NeonGammon policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SageDifficulty {
    Beginner,
    Casual,
    Intermediate,
    FullStrength,
}

impl SageDifficulty {
    pub(crate) fn checker_plies(self) -> i32 {
        match self {
            Self::FullStrength => 3,
            _ => 1,
        }
    }

    pub(crate) fn accepts_checker_plies(self, plies: i32) -> bool {
        // Older clients may still send the former Full Strength depth explicitly.
        plies == self.checker_plies() || (self == Self::FullStrength && plies == 2)
    }
}

#[derive(Deserialize)]
pub(crate) struct SageOpponentCube {
    pub should_double: bool,
    pub should_take: bool,
}

/// Which checker candidates the sidecar must search at the profile depth.
#[derive(Clone, Copy)]
enum CheckerScope {
    /// The engine's own selection, which may prune weaker candidates.
    Searched,
    /// The engine's selection plus the played move.
    Played([i32; 26]),
    /// Every legal move.
    AllLegal,
    /// The playing bot's selective 3-ply search over the two leading candidates.
    BotTopTwo,
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

/// A cube decision without probabilities, in the same perspective and with
/// the same equities as [`SageCubeEvaluation`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageCubeDecision {
    pub no_double: f64,
    pub take: f64,
    pub pass: f64,
}

impl SageCubeDecision {
    fn validate(&self) -> Result<()> {
        ensure!(
            [self.no_double, self.take, self.pass]
                .iter()
                .all(|equity| equity.is_finite()),
            "Invalid NeonGammon cube decision"
        );
        Ok(())
    }
}

/// Which part of a cube evaluation the sidecar computes.
#[derive(Clone, Copy)]
enum CubeScope {
    /// Equities and the cubeless probabilities.
    Evaluation,
    /// Equities only (`BgBotAnalyzer.cube_decision`).
    Decision,
}

fn cube_request(board: &[i32; 26], context: &context::Context, scope: CubeScope) -> Value {
    let mut payload = json!({"operation":"cube", "board":board, "context":context});
    if matches!(scope, CubeScope::Decision) {
        payload["decision_only"] = json!(true);
    }
    payload
}

/// Whether a saved NeonGammon engine version names a cubeful profile at
/// `plies`, in the format [`SageEngine::new`] writes.
pub(crate) fn sage_version_has_cubeful_depth(version: &str, plies: i32) -> bool {
    version.starts_with("NeonGammon ") && version.contains(&format!("; {plies}ply cubeful;"))
}

pub(crate) fn valid_sage_depth(plies: i32) -> bool {
    (1..=4).contains(&plies)
}

fn valid_sage_threads(threads: u32) -> bool {
    i32::try_from(threads).is_ok_and(|threads| threads >= EngineId::NeonGammon.min_threads())
}

impl SageEngine {
    pub fn new(snapshot: SageSnapshot) -> Result<Self> {
        ensure!(
            valid_sage_depth(snapshot.plies) && valid_sage_threads(snapshot.threads),
            "Unsupported NeonGammon depth or thread count"
        );
        ensure!(
            snapshot.plies != 1 || snapshot.one_ply_verified,
            "NeonGammon 1-ply capability has not been verified"
        );
        ensure!(
            snapshot.adapter_version == 1
                && SUPPORTED_SAGE_PACKAGE_VERSIONS.contains(&snapshot.package_version.as_str()),
            "Unsupported NeonGammon adapter or package version"
        );
        ensure!(
            snapshot.artifact_fingerprint.len() == 64
                && snapshot
                    .artifact_fingerprint
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit()),
            "Invalid NeonGammon artifact fingerprint"
        );
        ensure!(
            Path::new(&snapshot.python).is_absolute(),
            "NeonGammon Python path must be absolute"
        );
        let version = format!(
            "NeonGammon {}; model {}; {}ply {}; threads {}; adapter {}",
            snapshot.package_version,
            snapshot.artifact_fingerprint,
            snapshot.plies,
            if snapshot.cubeless {
                "cubeless"
            } else {
                "cubeful"
            },
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
        ensure!(valid_sage_depth(plies), "Unsupported NeonGammon depth");
        ensure!(
            valid_sage_threads(threads),
            "Unsupported NeonGammon thread count"
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
                .context("NeonGammon Python was not found on PATH")?
        };
        let mut snapshot = SageSnapshot {
            python: python
                .to_str()
                .context("NeonGammon Python path is not UTF-8")?
                .into(),
            // Replaced by the installation's own identity below.
            package_version: String::new(),
            artifact_fingerprint: String::new(),
            threads,
            plies,
            adapter_version: 1,
            one_ply_verified: plies == 1,
            cubeless: false,
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
            "Requested depth differs from immutable NeonGammon profile"
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

    /// Every legal checker play at the profile depth, for hints that rate any
    /// move. Plain requests may omit candidates the engine's filter pruned.
    pub async fn all_legal_checker_boards(
        &self,
        position_id: &str,
        match_id: &str,
    ) -> Result<Vec<SageCheckerCandidate>> {
        Ok(self
            .checker_boards_scoped(position_id, match_id, None, CheckerScope::AllLegal, None)
            .await?
            .0)
    }

    /// Checker candidates plus the engine cost the worker measured for them.
    pub async fn checker_boards_costed(
        &self,
        position_id: &str,
        match_id: &str,
        dice: Option<[i32; 2]>,
        force_board: Option<[i32; 26]>,
    ) -> Result<(Vec<SageCheckerCandidate>, Option<EngineCost>)> {
        self.checker_boards_costed_with_batch(position_id, match_id, dice, force_board, None)
            .await
    }

    pub(crate) async fn top_two_checker_boards(
        &self,
        position_id: &str,
        match_id: &str,
    ) -> Result<(Vec<SageCheckerCandidate>, Option<EngineCost>)> {
        self.checker_boards_scoped(position_id, match_id, None, CheckerScope::BotTopTwo, None)
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
        Ok(self
            .checker_boards_costed_with_batch(position_id, match_id, dice, force_board, batch)
            .await?
            .0)
    }

    async fn checker_boards_costed_with_batch(
        &self,
        position_id: &str,
        match_id: &str,
        dice: Option<[i32; 2]>,
        force_board: Option<[i32; 26]>,
        batch: Option<&mut process::VerifiedBatch>,
    ) -> Result<(Vec<SageCheckerCandidate>, Option<EngineCost>)> {
        let scope = force_board.map_or(CheckerScope::Searched, CheckerScope::Played);
        self.checker_boards_scoped(position_id, match_id, dice, scope, batch)
            .await
    }

    async fn checker_boards_scoped(
        &self,
        position_id: &str,
        match_id: &str,
        dice: Option<[i32; 2]>,
        scope: CheckerScope,
        batch: Option<&mut process::VerifiedBatch>,
    ) -> Result<(Vec<SageCheckerCandidate>, Option<EngineCost>)> {
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
        let mut payload =
            json!({"operation":"checker", "board":board, "context":context, "dice":dice});
        match scope {
            CheckerScope::Searched => {}
            CheckerScope::Played(played) => {
                payload["force_board"] = json!(context::to_sage(&played, context.player));
            }
            CheckerScope::AllLegal => payload["all_legal"] = json!(true),
            CheckerScope::BotTopTwo => payload["top_two"] = json!(true),
        }
        let (result, cost) = self.request_with_batch(payload, batch).await?;
        let mut candidates: Vec<SageCheckerCandidate> = serde_json::from_value(result)?;
        ensure!(
            !candidates.is_empty(),
            "NeonGammon returned no checker candidates"
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
                    "NeonGammon omitted checker notation"
                );
            }
            candidate.board = context::from_sage(&candidate.board, context.player)?;
        }
        candidates.sort_by(|a, b| b.equity.total_cmp(&a.equity));
        Ok((candidates, cost))
    }

    pub(crate) async fn opponent_checker_board(
        &self,
        position_id: &str,
        match_id: &str,
        difficulty: SageDifficulty,
        seed: u64,
    ) -> Result<[i32; 26]> {
        let context = context::Context::decode(match_id)?;
        ensure!(
            !context.pending_double,
            "Cannot play checkers before cube response"
        );
        let board = context.board(position_id, match_id)?;
        let (result, _) = self
            .request_with_batch(
                json!({"operation":"opponent_checker", "board":board, "context":context,
                       "dice":context.dice, "difficulty":difficulty, "seed":seed}),
                None,
            )
            .await?;
        context::from_sage(&serde_json::from_value(result)?, context.player)
    }

    pub(crate) async fn opponent_cube(
        &self,
        position_id: &str,
        match_id: &str,
        difficulty: SageDifficulty,
        seed: u64,
    ) -> Result<SageOpponentCube> {
        let context = context::Context::decode(match_id)?;
        let board = context.board(position_id, match_id)?;
        let (result, _) = self
            .request_with_batch(
                json!({"operation":"opponent_cube", "board":board, "context":context,
                       "difficulty":difficulty, "seed":seed}),
                None,
            )
            .await?;
        Ok(serde_json::from_value(result)?)
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
            candidates: self.ranked_candidates(candidates),
        })
    }

    /// Evaluate the position once per roll inside one verified batch: the
    /// installation is verified before the first and after the last roll
    /// instead of around every roll, and no analysis is returned before the
    /// final check succeeds. Progress reports only the number of rolls done.
    pub async fn analyze_rolls<F>(
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
        self.check_depth(plies)?;
        crate::engine::with_shared_access(async {
            let mut batch = process::VerifiedBatch::begin(&self.snapshot).await?;
            let mut analyses = Vec::with_capacity(rolls.len());
            for &dice in rolls {
                let candidates = self
                    .checker_boards_with_batch(
                        position_id,
                        match_id,
                        Some(dice),
                        None,
                        Some(&mut batch),
                    )
                    .await?;
                analyses.push(Analysis {
                    engine: self.version.clone(),
                    plies,
                    candidates: self.ranked_candidates(candidates),
                });
                evaluated(analyses.len()).await?;
            }
            batch.finish().await?;
            Ok(analyses)
        })
        .await
    }

    fn ranked_candidates(&self, candidates: Vec<SageCheckerCandidate>) -> Vec<Candidate> {
        candidates
            .into_iter()
            .enumerate()
            .map(|(index, candidate)| Candidate {
                rank: index as i32 + 1,
                notation: candidate.notation,
                // MoveAnalysis contains the final board. The sidecar's notation
                // is derived from it and does not describe a recorded engine path.
                path: None,
                equity: candidate.equity,
                probabilities: candidate.probabilities,
                plies: Some(self.snapshot.plies),
            })
            .collect()
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
            "Expected a forced pass but NeonGammon returned playable moves"
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
            .context("NeonGammon omitted resignation decision")
    }

    pub async fn cube(&self, position_id: &str, match_id: &str) -> Result<SageCubeEvaluation> {
        self.cube_with_batch(position_id, match_id, None).await
    }

    /// The cube decision alone, plus the engine cost the worker measured for
    /// it. The equities equal [`Self::cube`]'s; the sidecar skips the second,
    /// cubeless search for probabilities, so use this whenever only the
    /// equities are compared.
    pub async fn cube_decision_costed(
        &self,
        position_id: &str,
        match_id: &str,
    ) -> Result<(SageCubeDecision, Option<EngineCost>)> {
        let context = context::Context::decode(match_id)?;
        let board = context.board(position_id, match_id)?;
        let (value, cost) = self
            .request_with_batch(cube_request(&board, &context, CubeScope::Decision), None)
            .await?;
        let result: SageCubeDecision = serde_json::from_value(value)?;
        result.validate()?;
        Ok((result, cost))
    }

    async fn cube_with_batch(
        &self,
        position_id: &str,
        match_id: &str,
        batch: Option<&mut process::VerifiedBatch>,
    ) -> Result<SageCubeEvaluation> {
        Ok(self
            .cube_costed_with_batch(position_id, match_id, batch)
            .await?
            .0)
    }

    async fn cube_costed_with_batch(
        &self,
        position_id: &str,
        match_id: &str,
        batch: Option<&mut process::VerifiedBatch>,
    ) -> Result<(SageCubeEvaluation, Option<EngineCost>)> {
        let context = context::Context::decode(match_id)?;
        let board = context.board(position_id, match_id)?;
        let (value, cost) = self
            .request_with_batch(cube_request(&board, &context, CubeScope::Evaluation), batch)
            .await?;
        let result: SageCubeEvaluation = serde_json::from_value(value)?;
        for equity in [result.no_double, result.take, result.pass] {
            validate_evaluation(equity, &result.probabilities)?;
        }
        Ok((result, cost))
    }

    async fn request_with_batch(
        &self,
        payload: serde_json::Value,
        batch: Option<&mut process::VerifiedBatch>,
    ) -> Result<(serde_json::Value, Option<EngineCost>)> {
        let timeout = Duration::from_secs(180);
        match batch {
            Some(batch) => batch.request_costed(payload, timeout).await,
            None => process::request_costed(&self.snapshot, payload, timeout, self.slot).await,
        }
    }
}

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
    let board: [i32; 26] = crate::import::ids::decode_board(position_id, match_id)?
        .try_into()
        .ok()
        .context("Invalid board length")?;
    Ok(crate::import::sgf::apply_checker_turn(&board, player, dice, &[]).is_ok())
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
    /// The executable identity benchmarks are keyed by: GNUbg's resolved
    /// binary, or NeonGammon's absolute Python path as saved in its profile.
    pub(crate) fn resolved_binary(&self) -> Result<String> {
        match self {
            Self::Gnubg(engine) => crate::engine::resolve_binary(engine.binary()),
            Self::OpenSage(engine) => Ok(engine.binary().to_owned()),
        }
    }
    /// Full installation check: a real position evaluation for GNUbg, the
    /// verified worker identity for NeonGammon.
    pub(crate) async fn check_installation(&self) -> Result<String> {
        match self {
            Self::Gnubg(engine) => engine.check_installation().await,
            Self::OpenSage(engine) => engine.version().await,
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
    use super::SageSnapshot;

    #[test]
    fn forced_passes_follow_the_rules() {
        assert!(!super::forced_pass("4HPwATDgc/ABMA", "cInlAAAAAAAE", [3, 1]).unwrap());
        assert!(super::forced_pass("Y54pATDg28EBYA", "cAlnAQAAAAAE", [6, 6]).unwrap());
        assert!(!super::forced_pass("Y54pATDg28EBYA", "cAlnAQAAAAAE", [2, 3]).unwrap());
    }

    fn snapshot(artifact_fingerprint: &str) -> SageSnapshot {
        SageSnapshot {
            python: "python".into(),
            package_version: "3.0.0".into(),
            artifact_fingerprint: artifact_fingerprint.into(),
            threads: 4,
            plies: 2,
            adapter_version: 1,
            one_ply_verified: true,
            cubeless: false,
        }
    }

    #[test]
    fn known_fork_names_the_documented_builds() {
        let fork = snapshot("621c35546142fe9d4762ad763e3c2dac3372a03c8a67c6d264fa634dfc69f7f7")
            .known_fork()
            .expect("release v3.0.0 is recognized");
        assert_eq!(fork.repository, "HendrikReh/neongammon");
        assert_eq!(fork.revision, "cbfcb014c7f8cf33d75e7f06cbdfc77260cb53ed");

        let fork = snapshot("26e7701c27c6c5e0cd3cc864768e27675f0d72149c2ad545ffb0c5293ff705d5")
            .known_fork()
            .expect("release v3.1.0 is recognized");
        assert_eq!(fork.repository, "HendrikReh/neongammon");
        assert_eq!(fork.revision, "04e0bb702c3f7549a05f91af4e862fdef2af954f");
    }

    #[test]
    fn supported_package_versions_match_the_sidecar_check() {
        let probe = include_str!("../../../tools/probe_open_sage.py");
        let line = probe
            .lines()
            .find(|line| line.starts_with("SUPPORTED_VERSIONS = "))
            .expect("probe lists its supported versions");
        let listed: Vec<&str> = line.split('\'').skip(1).step_by(2).collect();
        assert_eq!(listed, super::SUPPORTED_SAGE_PACKAGE_VERSIONS);
    }

    #[test]
    fn engine_accepts_only_supported_package_versions() {
        for version in super::SUPPORTED_SAGE_PACKAGE_VERSIONS {
            let snapshot = SageSnapshot {
                python: "/usr/bin/python3".into(),
                package_version: version.into(),
                ..snapshot(&"a".repeat(64))
            };
            assert!(super::SageEngine::new(snapshot).is_ok(), "{version}");
        }
        let snapshot = SageSnapshot {
            python: "/usr/bin/python3".into(),
            package_version: "2.0.20261118".into(),
            ..snapshot(&"a".repeat(64))
        };
        assert!(super::SageEngine::new(snapshot).is_err());
    }

    #[test]
    fn cube_requests_ask_for_probabilities_unless_only_the_decision_is_used() {
        let context = super::context::Context::decode("cAkHAAAAAAAE").unwrap();
        let board = context.board("4HPwATDgc/ABMA", "cAkHAAAAAAAE").unwrap();
        let evaluation = super::cube_request(&board, &context, super::CubeScope::Evaluation);
        let decision = super::cube_request(&board, &context, super::CubeScope::Decision);
        // The full evaluation keeps the sidecar's default request unchanged.
        assert_eq!(
            evaluation,
            serde_json::json!({"operation":"cube", "board":board, "context":context})
        );
        assert_eq!(decision["decision_only"], serde_json::json!(true));
        let mut without_scope = decision.clone();
        without_scope
            .as_object_mut()
            .unwrap()
            .remove("decision_only");
        assert_eq!(without_scope, evaluation);
    }

    #[test]
    fn cube_decision_needs_finite_equities_but_no_probabilities() {
        let decision: super::SageCubeDecision =
            serde_json::from_value(serde_json::json!({"no_double":0.2,"take":0.3,"pass":1.0}))
                .unwrap();
        decision.validate().unwrap();
        let invalid = super::SageCubeDecision {
            take: f64::NAN,
            ..decision
        };
        assert!(invalid.validate().is_err());
        assert!(
            serde_json::from_value::<super::SageCubeDecision>(
                serde_json::json!({"no_double":0.2,"take":0.3})
            )
            .is_err()
        );
    }

    #[test]
    fn known_fork_ignores_unknown_builds() {
        assert!(snapshot(&"a".repeat(64)).known_fork().is_none());
    }
}
