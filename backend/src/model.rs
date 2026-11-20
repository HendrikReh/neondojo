use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Analysis,
    Dice,
    Match,
}
impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Analysis => "analysis",
            Self::Dice => "dice",
            Self::Match => "match",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum JobResult {
    Analysis(crate::engine::Analysis),
    Dice(crate::engine::Distribution),
    Match(Box<MatchAnalysisSummary>),
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MatchSummary {
    pub id: Uuid,
    /// False for retained saved-position context after the source match was deleted.
    pub source_available: bool,
    pub filename: String,
    pub format: String,
    pub white: String,
    pub black: String,
    pub match_length: i32,
    pub position_count: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GameResultReason {
    Single,
    Gammon,
    Backgammon,
    Drop,
    Resignation,
    Recorded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GameResult {
    /// 0 is White; 1 is Black.
    pub winner: i32,
    pub points: i32,
    /// Accepted cube value, before any declined offer.
    pub cube: i32,
    /// Only present when the source supports this cube calculation.
    pub multiplier: Option<i32>,
    pub reason: GameResultReason,
    pub scores_after: [i32; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MatchGame {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[sqlx(skip)]
    /// Terminal source evidence, an explicit award, or an unambiguous delta established by both explicit next-game scores.
    pub result: Option<GameResult>,
    pub game_no: i32,
    pub position_count: i32,
    pub white_score: i32,
    pub black_score: i32,
    pub crawford: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Position {
    pub id: Uuid,
    pub match_uuid: Uuid,
    pub ordinal: i32,
    pub game_no: i32,
    pub ply: i32,
    pub player: i32,
    pub dice: Vec<i32>,
    pub board: Vec<i32>,
    pub position_id: String,
    pub gnubg_match_id: String,
    pub notation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Job {
    /// Immutable provider, installation and model identity; absent for legacy GNUbg work.
    #[serde(default)]
    #[sqlx(json(nullable))]
    pub engine_snapshot: Option<crate::settings::EngineSnapshot>,
    pub id: Uuid,
    pub position_uuid: Option<Uuid>,
    pub match_uuid: Option<Uuid>,
    /// Source game number for a single-game match analysis; null means the whole match.
    pub game_no: Option<i32>,
    pub total_games: i32,
    pub completed_games: i32,
    #[schema(value_type=JobKind)]
    pub kind: String,
    /// Depth in Neondojo's ply convention; 1-ply is the raw evaluation.
    #[schema(minimum = 1, maximum = 5)]
    pub plies: i32,
    #[schema(value_type=JobStatus)]
    pub status: String,
    #[schema(minimum = 0, maximum = 100)]
    pub progress: i32,
    #[schema(value_type=Option<JobResult>)]
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub attempts: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow, ToSchema)]
pub struct AnalysisComparisonRun {
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub job: Job,
    /// Selected game's checkpoint from a whole-match analysis.
    #[sqlx(json(nullable))]
    pub game_summary: Option<MatchAnalysisSummary>,
}

#[derive(Debug, Clone, Serialize, FromRow, ToSchema)]
pub struct JobEvent {
    pub id: i64,
    pub job_id: Uuid,
    pub event: String,
    pub data: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// JSON encoded in the `data:` line of each job SSE message.
#[derive(Debug, Serialize, ToSchema)]
pub struct JobProgress {
    pub status: JobStatus,
    #[schema(minimum = 0, maximum = 100)]
    pub progress: i32,
    pub error: Option<String>,
    pub attempt: Option<i32>,
    pub total_games: Option<i32>,
    pub completed_games: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ImportResponse {
    pub r#match: MatchSummary,
    pub duplicate: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MatchPage {
    pub items: Vec<MatchSummary>,
    pub next_cursor: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PositionPage {
    pub items: Vec<Position>,
    /// Opaque cursor; continue with the same filters.
    pub next_cursor: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DiceCount {
    pub dice: [i32; 2],
    pub count: i64,
    pub expected_probability: f64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DiceStats {
    pub total: i64,
    pub rolls: Vec<DiceCount>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct PlayerAnalysisSummary {
    pub player: i32,
    pub checker_decisions: i32,
    pub cube_decisions: i32,
    pub forced_moves: i32,
    pub unavailable: i32,
    pub doubtful: i32,
    pub bad: i32,
    pub very_bad: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub blunder: i32,
    /// Sum of normalized equity losses, not a performance rating (PR).
    pub checker_equity_loss: f64,
    pub cube_equity_loss: f64,
}

fn is_zero(value: &i32) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MatchAnalysisSummary {
    pub engine: String,
    pub plies: i32,
    pub total_games: i32,
    pub completed_games: i32,
    pub total_moves: i32,
    pub analyzed_moves: i32,
    pub players: [PlayerAnalysisSummary; 2],
    /// Native GNUbg normalized loss and non-trivial decision denominator.
    /// Missing for legacy results or if any analyzed game lacks native statistics.
    #[serde(default)]
    pub performance_rating: Option<PerformanceRatingSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PerformanceRatingSummary {
    /// gnubg_emg_500_v1: 500 × native normalized loss / (unforced checker + close/actual cube).
    /// GNUbg counting is not interchangeable with XG or BMAB counting.
    pub method: String,
    pub profile: String,
    /// Number of games contributing complete native statistics.
    pub games: i32,
    pub players: [PlayerPerformanceRating; 2],
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct PlayerPerformanceRating {
    pub player: i32,
    /// Null if the native denominator is zero; lower values indicate less equity lost.
    pub value: Option<f64>,
    pub normalized_equity_loss: f64,
    pub unforced_checker_moves: i32,
    pub close_cube_decisions: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MoveAnalysis {
    pub id: i64,
    /// Imported board before this checker move; cube-only decisions have no checker board.
    pub position_uuid: Option<Uuid>,
    pub game_no: i32,
    #[serde(flatten)]
    pub decision: crate::match_analysis::Decision,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MoveAnalysisPage {
    pub items: Vec<MoveAnalysis>,
    pub next_cursor: Option<i64>,
}

/// Decision severity for normalized equity loss. Existing category identifiers are retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MoveSeverity {
    None,
    Doubtful,
    Bad,
    Blunder,
    VeryBad,
}

impl MoveSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Doubtful => "doubtful",
            Self::Bad => "bad",
            Self::Blunder => "blunder",
            Self::VeryBad => "very_bad",
        }
    }
}
