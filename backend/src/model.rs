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
}
impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Analysis => "analysis",
            Self::Dice => "dice",
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
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MatchSummary {
    pub id: Uuid,
    pub filename: String,
    pub format: String,
    pub white: String,
    pub black: String,
    pub match_length: i32,
    pub position_count: i32,
    pub created_at: DateTime<Utc>,
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
    pub id: Uuid,
    pub position_uuid: Uuid,
    #[schema(value_type=JobKind)]
    pub kind: String,
    #[schema(minimum = 0, maximum = 2)]
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
