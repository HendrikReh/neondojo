//! Durable native GNUbg matches. Each action runs in a bounded isolated process;
//! optimistic revisions fence concurrent requests without holding DB locks.
pub(crate) mod engine_match;
mod feedback;
mod forfeit;
mod library;
mod native;
mod open_sage;
mod pause;
mod repo;
mod reports;
mod review_gate;
mod routes;
mod rules_process;
mod service;
pub use feedback::{
    FeedbackOperation, PlayAlternative, PlayAttempt, PlayDecision, PlayFeedbackRequest, PlayMode,
    PlayReview, PlayReviewRequest,
};
use feedback::{
    analyze_decision, default_analysis_plies, present, should_pause_for_feedback, stage_choice,
};
pub(crate) use feedback::{default_auto_analysis, default_play_mode, default_threshold};
pub use library::{
    BOT_DATABASE_ID, PAUSED_MATCHES_DATABASE_ID, backfill as backfill_library_matches,
};
use native::*;
pub(crate) use native::{benchmark_checker_turn, benchmark_sage_checker_turn, probe_native};
use repo::*;
pub(crate) use review_gate::{
    active_competition_sql, finished_session_sql, require_job_review_available,
    require_position_review_available, require_review_available, require_review_available_on,
    review_blocked_sql,
};
pub(crate) use routes::{PlayDoc, routes};
use service::*;
pub(crate) use service::{build_history_snapshots, named_export};

use crate::json_errors::{Json, Path, Query};
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    engine::{Engine, resolve_binary_async},
    engines::EngineId,
    model::Position,
};
use anyhow::{Context, ensure};
use axum::{
    Router,
    extract::State,
    http::{StatusCode, header},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayEngine {
    pub id: String,
    pub version: String,
    /// Checker search depth. The NeonGammon Full Strength preset uses 2-ply for cube decisions.
    pub plies: i32,
    /// Pinned checker selection policy for this session.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub top_two: bool,
    /// Named NeonGammon opponent policy pinned to the match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<crate::engines::SageDifficulty>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayStep {
    pub from: i32,
    pub to: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayMove {
    pub notation: String,
    pub steps: Vec<PlayStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CheckerTurn {
    pub steps: Vec<PlayStep>,
    /// The die used by each step, in the same order.
    pub dice: Vec<i32>,
}

impl CheckerTurn {
    fn from_path(path: crate::backgammon::CheckerPath, player: i32) -> Self {
        let absolute = |point| if player == 0 { point } else { 25 - point };
        let (steps, dice) = path
            .steps
            .into_iter()
            .map(|(from, to, die)| {
                (
                    PlayStep {
                        from: absolute(from as i32),
                        to: absolute(if to == 0 { -1 } else { to as i32 }),
                    },
                    die,
                )
            })
            .unzip();
        Self { steps, dice }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LiveCheckerTurn {
    #[serde(flatten)]
    pub turn: CheckerTurn,
    /// Index into this session revision's GNUbg legal move catalog.
    pub move_index: usize,
}

#[derive(Deserialize, ToSchema)]
struct CheckerTurnsRequest {
    board: [i32; 26],
    player: i32,
    dice: [i32; 2],
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayHistory {
    pub player: i32,
    pub dice: Vec<i32>,
    pub notation: String,
}
/// Read-only snapshots aligned one-for-one with the saved session history.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayHistorySnapshots {
    pub session_id: Uuid,
    pub version: i64,
    pub items: Vec<PlayHistorySnapshot>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayHistorySnapshot {
    pub index: usize,
    pub game_no: i32,
    pub move_no: i32,
    #[schema(examples("checker", "cube", "resign"))]
    pub kind: String,
    pub before: Position,
    pub after: Position,
}
#[derive(Debug, Deserialize)]
struct HistoryQuery {
    version: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlayPhase {
    OpeningRoll,
    OpeningResult,
    Roll,
    Move,
    DoubleResponse,
    GameOver,
    MatchOver,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OpeningRoll {
    pub white_die: i32,
    pub black_die: i32,
    pub winner: i32,
    pub rolls: Vec<[i32; 2]>,
}
/// Completed source games, attached after their final played history event.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayGameResult {
    pub game_no: i32,
    pub history_index: usize,
    pub result: crate::model::GameResult,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlaySession {
    /// The saved checkpoint is available for explicit resumption.
    #[serde(default)]
    pub paused: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_snapshots: Option<PlayEngineSnapshots>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_results: Option<Vec<PlayGameResult>>,
    /// Current linked library entry; projected from the relational session link.
    #[serde(default)]
    pub library_match_id: Option<Uuid>,
    #[serde(default)]
    pub learning_used: bool,
    #[serde(default)]
    pub game_learning_used: bool,
    /// An explicitly closed money session cannot start another game.
    #[serde(default)]
    pub money_session_ended: bool,
    /// The human conceded the entire point match.
    #[serde(default)]
    pub match_forfeited: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub study_report_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub study_report_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_game_no: Option<i32>,
    #[serde(default)]
    pub review: Option<PlayReview>,
    #[serde(default)]
    pub play_mode: PlayMode,
    #[serde(default = "default_threshold")]
    pub tutor_threshold: f64,
    #[serde(default)]
    pub error_thresholds: crate::configuration::ErrorThresholds,
    #[serde(default = "default_auto_analysis")]
    pub training_auto_analysis: bool,
    #[serde(default = "default_analysis_plies")]
    pub analysis_plies: i32,
    #[serde(default)]
    pub pending_decision: Option<PlayDecision>,
    #[serde(default)]
    pub decisions: Vec<PlayDecision>,
    pub id: Uuid,
    pub version: i64,
    pub human_player: i32,
    pub human_name: String,
    pub match_length: i32,
    pub bearoff_direction: String,
    pub engine: PlayEngine,
    pub position: Position,
    pub phase: PlayPhase,
    pub can_double: bool,
    pub legal_moves: Vec<PlayMove>,
    /// Interactive paths derived from the saved roll and the rule engine's legal moves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legal_turns: Option<Vec<LiveCheckerTurn>>,
    pub history: Vec<PlayHistory>,
    pub scores: [i32; 2],
    pub game_no: i32,
    pub winner: Option<i32>,
    #[serde(default)]
    pub opening: Option<OpeningRoll>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayEngineSnapshots {
    pub rules: crate::settings::EngineSnapshot,
    pub bot: crate::settings::EngineSnapshot,
    pub analysis: crate::settings::EngineSnapshot,
}
impl PlaySession {
    fn rules_engine(&self, path: &str) -> anyhow::Result<Engine> {
        match &self.engine_snapshots {
            Some(snapshots) => snapshots.rules.engine()?.into_rules_engine(),
            None => {
                Ok(Engine::new(path.into())
                    .with_expected_version(Some(self.engine.version.clone())))
            }
        }
        .map(Engine::interactive)
    }
    fn analysis_engine(&self, path: &str) -> anyhow::Result<crate::engines::AnalysisEngine> {
        match &self.engine_snapshots {
            Some(snapshots) => snapshots.analysis.engine(),
            None => Ok(self.rules_engine(path)?.into()),
        }
    }
    fn analysis_version(&self) -> &str {
        self.engine_snapshots
            .as_ref()
            .and_then(|snapshots| snapshots.analysis.engine_version.as_deref())
            .unwrap_or(&self.engine.version)
    }
    fn bot_name(&self) -> &str {
        crate::engines::display_name(&self.engine.id).unwrap_or("GNUbg")
    }
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatePlaySession {
    pub play_mode: Option<PlayMode>,
    pub tutor_threshold: Option<f64>,
    pub training_auto_analysis: Option<bool>,
    #[schema(minimum = 0, maximum = 1)]
    pub human_player: i32,
    /// Zero starts a continuous money-game session; 1..25 starts a point match.
    #[schema(minimum = 0, maximum = 25)]
    pub match_length: i32,
    /// Name of the human side for this match only; blank uses the saved player name.
    #[schema(max_length = 120)]
    pub human_name: Option<String>,
    /// Bot engine for this match only (`gnubg` or `bgsage`); defaults to the saved play engine.
    pub play_engine: Option<String>,
    /// Bot checker depth for this match only. A named preset pins its own depth.
    pub play_plies: Option<i32>,
    /// Selective 3-ply for NeonGammon's two leading checker candidates.
    pub play_top_two: Option<bool>,
    /// Named NeonGammon opponent policy for this match only.
    pub play_difficulty: Option<crate::engines::SageDifficulty>,
}

impl CreatePlaySession {
    /// Applies this match's own player name, bot engine and strength over the
    /// saved settings without changing them.
    fn apply_match_choices(&self, settings: &mut crate::settings::AppSettings) -> ApiResult<()> {
        if let Some(name) = self.human_name.as_deref().map(str::trim) {
            if !crate::settings::valid_player_name(name) {
                return Err(ApiError::invalid(crate::settings::PLAYER_NAME_RULE));
            }
            if !name.is_empty() {
                name.clone_into(&mut settings.player_name);
            }
        }
        if self.play_engine.is_none()
            && self.play_plies.is_none()
            && self.play_top_two.is_none()
            && self.play_difficulty.is_none()
        {
            return Ok(());
        }
        let engine = self
            .play_engine
            .clone()
            .unwrap_or_else(|| settings.play_engine.clone());
        let provider = EngineId::parse(&engine);
        let depths = provider.map_or_else(Vec::new, |provider| provider.depths("play"));
        if let Some(difficulty) = self.play_difficulty
            && (!provider.is_some_and(EngineId::has_difficulty_presets)
                || self.play_top_two == Some(true)
                || self
                    .play_plies
                    .is_some_and(|plies| !difficulty.accepts_checker_plies(plies)))
        {
            return Err(ApiError::invalid(
                "A NeonGammon difficulty preset requires NeonGammon and its own playing depth",
            ));
        }
        let plies = match (self.play_plies, self.play_difficulty) {
            (_, Some(difficulty)) => difficulty.checker_plies(),
            (Some(plies), None) => plies,
            (None, None) => provider
                .and_then(EngineId::play_plies_for_override)
                .filter(|_| engine != settings.play_engine)
                .or_else(|| {
                    depths
                        .contains(&settings.play_plies)
                        .then_some(settings.play_plies)
                })
                .unwrap_or_else(|| depths.first().copied().unwrap_or_default()),
        };
        // A named preset pins its own depth independently of the manual picker.
        if self.play_difficulty.is_none() && !depths.contains(&plies) {
            return Err(ApiError::invalid(
                "Engine or depth is not verified for this purpose",
            ));
        }
        let top_two = self.play_top_two.unwrap_or(false);
        if top_two && !crate::settings::supports_top_two(&engine, plies) {
            return Err(ApiError::invalid(crate::settings::TOP_TWO_PROFILE_ERROR));
        }
        settings.play_engine = engine;
        settings.play_plies = plies;
        settings.play_top_two = top_two;
        Ok(())
    }
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PlayModeRequest {
    pub version: i64,
    pub play_mode: PlayMode,
    pub tutor_threshold: Option<f64>,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlayAction {
    OpeningRoll,
    StartOpeningMove,
    Roll,
    Move,
    Pass,
    Double,
    Take,
    Drop,
    Resign,
    ForfeitMatch,
    NextGame,
    EndSession,
    Pause,
    Resume,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PlayActionRequest {
    pub decision_id: Option<Uuid>,
    pub version: i64,
    pub action: PlayAction,
    #[serde(rename = "move")]
    pub move_index: Option<usize>,
    /// Resign the current game for a single (1), gammon (2), or backgammon (3).
    /// Required for resign; invalid for every other action.
    #[schema(minimum = 1, maximum = 3)]
    pub resign_level: Option<i32>,
}
