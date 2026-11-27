use super::{EngineCost, EngineId, VerifiedBatch, context, process};
use crate::engine::{Analysis, Candidate};
use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
use utoipa::ToSchema;

pub(super) const RELEASES_JSON: &str = include_str!("../../../shared/neongammon-releases.json");

#[derive(Deserialize)]
struct Release {
    presets: bool,
    fingerprint: Option<&'static str>,
    revision: Option<&'static str>,
}

static RELEASES: std::sync::LazyLock<std::collections::BTreeMap<&str, Release>> =
    std::sync::LazyLock::new(|| {
        serde_json::from_str(RELEASES_JSON).expect("embedded NeonGammon releases")
    });

pub static SUPPORTED_SAGE_PACKAGE_VERSIONS: std::sync::LazyLock<Vec<&str>> =
    std::sync::LazyLock::new(|| RELEASES.keys().copied().collect());

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
    pub(crate) fn describe(&self, cube_mode: &str) -> String {
        format!(
            "NeonGammon {}; model {}; {}ply {}; threads {}; adapter {}",
            self.package_version,
            self.artifact_fingerprint,
            self.plies,
            cube_mode,
            self.threads,
            self.adapter_version
        )
    }

    pub(crate) fn supports_difficulty_presets(&self) -> bool {
        RELEASES
            .get(self.package_version.as_str())
            .is_some_and(|release| release.presets)
    }

    pub fn known_fork(&self) -> Option<EngineFork> {
        // Identify the documented releases by their full runtime fingerprints.
        let revision = RELEASES
            .values()
            .find(|release| release.fingerprint == Some(self.artifact_fingerprint.as_str()))?
            .revision?;
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
pub(super) enum CheckerScope {
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
    pub(super) snapshot: SageSnapshot,
    pub(super) slot: process::Slot,
    pub(super) version: String,
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
    pub(super) fn validate(&self) -> Result<()> {
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
pub(super) enum CubeScope {
    /// Equities and the cubeless probabilities.
    Evaluation,
    /// Equities only (`BgBotAnalyzer.cube_decision`).
    Decision,
}

pub(super) fn cube_request(
    board: &[i32; 26],
    context: &context::Context,
    scope: CubeScope,
) -> Value {
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
        let version = snapshot.describe(if snapshot.cubeless {
            "cubeless"
        } else {
            "cubeful"
        });
        Ok(Self {
            snapshot,
            version,
            slot: process::Slot::General,
        })
    }

    pub(super) fn for_cubeless_checker(&self, money_without_jacoby: bool) -> Result<Self> {
        ensure!(
            money_without_jacoby,
            "NeonGammon cubeless checker evaluation requires money play without Jacoby"
        );
        let mut snapshot = self.snapshot.clone();
        snapshot.cubeless = true;
        let mut engine = Self::new(snapshot)?;
        engine.slot = self.slot;
        Ok(engine)
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
        let python = tokio::task::spawn_blocking(move || -> Result<_> {
            // Preserve the venv executable symlink; canonicalizing it loses pyvenv.cfg.
            let path = Path::new(&python);
            if path.is_absolute() {
                Ok(path.to_path_buf())
            } else if python.contains(std::path::is_separator) {
                Ok(std::env::current_dir()?.join(path))
            } else {
                std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                    .map(|base| base.join(path))
                    .find(|path| path.is_file())
                    .context("NeonGammon Python was not found on PATH")
            }
        })
        .await??;
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

    /// Checks in a separate process that this exact profile still runs and
    /// matches its saved identity.
    pub(crate) async fn verify(&self) -> Result<String> {
        let identity = process::probe(&self.snapshot).await?;
        process::validate_identity(&self.snapshot, &identity)?;
        Ok(self.version.clone())
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

    pub(super) fn check_depth(&self, plies: i32) -> Result<()> {
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
        Ok(self
            .checker_boards_costed(position_id, match_id, dice, force_board)
            .await?
            .0)
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
        let scope = force_board.map_or(CheckerScope::Searched, CheckerScope::Played);
        self.checker_boards_scoped(position_id, match_id, dice, scope, None)
            .await
    }

    pub(crate) async fn play_checker_boards(
        &self,
        position_id: &str,
        match_id: &str,
        top_two: bool,
        batch: Option<&mut VerifiedBatch<'_>>,
    ) -> Result<(Vec<SageCheckerCandidate>, Option<EngineCost>)> {
        let scope = if top_two {
            CheckerScope::BotTopTwo
        } else {
            CheckerScope::Searched
        };
        self.checker_boards_scoped(position_id, match_id, None, scope, batch)
            .await
    }

    async fn checker_boards_scoped(
        &self,
        position_id: &str,
        match_id: &str,
        dice: Option<[i32; 2]>,
        scope: CheckerScope,
        batch: Option<&mut VerifiedBatch<'_>>,
    ) -> Result<(Vec<SageCheckerCandidate>, Option<EngineCost>)> {
        let (payload, player) = Self::checker_request(position_id, match_id, dice, scope)?;
        let (result, cost) = self.scoped_request(payload, batch).await?;
        Ok((Self::absolute_candidates(result, player)?, cost))
    }

    pub(super) fn checker_request(
        position_id: &str,
        match_id: &str,
        dice: Option<[i32; 2]>,
        scope: CheckerScope,
    ) -> Result<(Value, i32)> {
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
        Ok((payload, context.player))
    }

    // The transport has already checked probability nesting, forced moves and
    // forced-pass legality for this exact row before releasing the worker.
    pub(super) fn absolute_candidates(
        result: Value,
        player: i32,
    ) -> Result<Vec<SageCheckerCandidate>> {
        let mut candidates: Vec<SageCheckerCandidate> = serde_json::from_value(result)?;
        for candidate in &mut candidates {
            if candidate.notation.is_empty() {
                candidate.notation = "Cannot move".into();
            }
            candidate.board = context::from_sage(&candidate.board, player)?;
        }
        candidates.sort_by(|a, b| b.equity.total_cmp(&a.equity));
        Ok(candidates)
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
            .request_costed(json!({
                "operation":"opponent_checker",
                "board":board,
                "context":context,
                "dice":context.dice,
                "difficulty":difficulty,
                "seed":seed
            }))
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
            .request_costed(json!({
                "operation":"opponent_cube",
                "board":board,
                "context":context,
                "difficulty":difficulty,
                "seed":seed
            }))
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
            for chunk in rolls.chunks(self.chunk_size()) {
                let (rows, players): (Vec<_>, Vec<_>) = chunk
                    .iter()
                    .map(|&dice| {
                        Self::checker_request(
                            position_id,
                            match_id,
                            Some(dice),
                            CheckerScope::Searched,
                        )
                    })
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .unzip();
                let results = self
                    .evaluate_rows("checker_batch", &rows, Some(&mut batch))
                    .await?;
                for (row, player) in results.into_iter().zip(players) {
                    let candidates = Self::absolute_candidates(row, player)?;
                    analyses.push(Analysis {
                        engine: self.version.clone(),
                        plies,
                        candidates: self.ranked_candidates(candidates),
                    });
                    evaluated(analyses.len()).await?;
                }
            }
            batch.finish().await?;
            Ok(analyses)
        })
        .await
    }

    pub(super) fn ranked_candidates(
        &self,
        candidates: Vec<SageCheckerCandidate>,
    ) -> Vec<Candidate> {
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
            json!({
                "operation":"resignation",
                "board":board,
                "context":context,
                "dice":context.dice,
                "level":level
            }),
            Duration::from_secs(180),
            self.slot,
        )
        .await?;
        result["accept"]
            .as_bool()
            .context("NeonGammon omitted resignation decision")
    }

    pub async fn cube(&self, position_id: &str, match_id: &str) -> Result<SageCubeEvaluation> {
        let context = context::Context::decode(match_id)?;
        let board = context.board(position_id, match_id)?;
        let (value, _) = self
            .request_costed(cube_request(&board, &context, CubeScope::Evaluation))
            .await?;
        Ok(serde_json::from_value(value)?)
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
        self.play_cube_decision(position_id, match_id, None).await
    }

    pub(crate) async fn play_cube_decision(
        &self,
        position_id: &str,
        match_id: &str,
        batch: Option<&mut VerifiedBatch<'_>>,
    ) -> Result<(SageCubeDecision, Option<EngineCost>)> {
        let context = context::Context::decode(match_id)?;
        let board = context.board(position_id, match_id)?;
        let (value, cost) = self
            .scoped_request(cube_request(&board, &context, CubeScope::Decision), batch)
            .await?;
        let result: SageCubeDecision = serde_json::from_value(value)?;
        result.validate()?;
        Ok((result, cost))
    }

    // Deep searches yield the shared worker thread budget more frequently.
    fn chunk_size(&self) -> usize {
        if self.snapshot.plies >= 3 { 4 } else { 64 }
    }

    pub(super) async fn evaluate_rows(
        &self,
        operation: &str,
        rows: &[Value],
        batch: Option<&mut VerifiedBatch<'_>>,
    ) -> Result<Vec<Value>> {
        let mut results = Vec::with_capacity(rows.len());
        if let Some(batch) = batch {
            for chunk in rows.chunks(self.chunk_size()) {
                let mut parts = vec![chunk];
                while let Some(part) = parts.pop() {
                    let (result, _) = batch
                        .request_costed(
                            json!({"operation": operation, "rows": part}),
                            Duration::from_secs(180 * part.len() as u64),
                        )
                        .await?;
                    match result {
                        Value::Array(values) => results.extend(values),
                        // A legal roll can have hundreds of moves. Retry smaller
                        // frames inside the same integrity boundary.
                        _ if part.len() > 1 && result == json!({"split": true}) => {
                            let (left, right) = part.split_at(part.len() / 2);
                            parts.extend([right, left]);
                        }
                        _ => anyhow::bail!("Invalid NeonGammon batch result"),
                    }
                }
            }
        } else {
            for row in rows {
                results.push(self.request_costed(row.clone()).await?.0);
            }
        }
        Ok(results)
    }

    async fn scoped_request(
        &self,
        payload: Value,
        batch: Option<&mut VerifiedBatch<'_>>,
    ) -> Result<(Value, Option<EngineCost>)> {
        match batch {
            Some(batch) => {
                batch
                    .request_costed(payload, Duration::from_secs(180))
                    .await
            }
            None => self.request_costed(payload).await,
        }
    }

    async fn request_costed(&self, payload: Value) -> Result<(Value, Option<EngineCost>)> {
        process::request_costed(&self.snapshot, payload, Duration::from_secs(180), self.slot).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let fork = snapshot("eaa6f2a81b42320a80dc8ed2114dd84fc18239ce1f9cfcc1ca31b503e66bfebe")
            .known_fork()
            .expect("release v3.1.1 is recognized");
        assert_eq!(fork.repository, "HendrikReh/neongammon");
        assert_eq!(fork.revision, "abaedf9799d442fb4d53b83f2e3079cf17d8b6ce");

        let fork = snapshot("02d3ad593cf480a0813858fdcb8373645b0b0048e2ce31080180730a90192199")
            .known_fork()
            .expect("release v3.1.2 is recognized");
        assert_eq!(fork.repository, "HendrikReh/neongammon");
        assert_eq!(fork.revision, "758d701441cdaaa30c042c96b53953039ba2ba29");

        let fork = snapshot("e876576f9a9daabefb8a5e98b1f19d4f79272b1e49a66d332f91aa9a705cdd92")
            .known_fork()
            .expect("release v3.2.0 is recognized");
        assert_eq!(fork.repository, "HendrikReh/neongammon");
        assert_eq!(fork.revision, "9d9486074fdabc6957e3694d20be354c008515d9");

        let fork = snapshot("32376db9d2ea5c39264ed153e835080ccbcb3c7201b46b764c3c5511c79f4a74")
            .known_fork()
            .expect("release v3.2.1 is recognized");
        assert_eq!(fork.repository, "HendrikReh/neongammon");
        assert_eq!(fork.revision, "4553ed8727feb5f45a992a6c416da03d26e9ffe9");
    }

    #[test]
    fn engine_accepts_only_supported_package_versions() {
        for &version in SUPPORTED_SAGE_PACKAGE_VERSIONS.iter() {
            let snapshot = SageSnapshot {
                python: "/usr/bin/python3".into(),
                package_version: version.into(),
                ..snapshot(&"a".repeat(64))
            };
            assert!(SageEngine::new(snapshot).is_ok(), "{version}");
        }
        let snapshot = SageSnapshot {
            python: "/usr/bin/python3".into(),
            package_version: "2.0.20261118".into(),
            ..snapshot(&"a".repeat(64))
        };
        assert!(SageEngine::new(snapshot).is_err());
    }

    #[test]
    fn cube_requests_ask_for_probabilities_unless_only_the_decision_is_used() {
        let context = context::Context::decode("cAkHAAAAAAAE").unwrap();
        let board = context.board("4HPwATDgc/ABMA", "cAkHAAAAAAAE").unwrap();
        let evaluation = cube_request(&board, &context, CubeScope::Evaluation);
        let decision = cube_request(&board, &context, CubeScope::Decision);
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
        let decision: SageCubeDecision =
            serde_json::from_value(serde_json::json!({"no_double":0.2,"take":0.3,"pass":1.0}))
                .unwrap();
        decision.validate().unwrap();
        let invalid = SageCubeDecision {
            take: f64::NAN,
            ..decision
        };
        assert!(invalid.validate().is_err());
        assert!(
            serde_json::from_value::<SageCubeDecision>(
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
