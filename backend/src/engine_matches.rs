//! Persisted head-to-head series, with a versioned dice schedule per match or pair.
mod checkpoint;
mod dice;
mod progress;
mod routes;
mod runner;
mod stats;
#[cfg(test)]
mod tests;

pub use progress::LiveProgress;
pub(crate) use routes::{
    EngineMatchDoc, cancel_series, create_series, delete_series, list_series, resume_series,
    routes, series_details, series_library_match, series_sgf,
};
pub(crate) use runner::ENGINE_DUELS_DATABASE_ID;
pub use runner::worker;
#[cfg(test)]
use stats::student_t_975;
use stats::summarize;
pub use stats::{GameOutcomes, MoneyStatistics, SeriesStatistics, SideCostSummary};

use crate::settings::EngineSnapshot;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiceMode {
    #[default]
    Paired,
    Random,
}

impl DiceMode {
    fn matches_per_unit(self) -> i32 {
        match self {
            Self::Paired => 2,
            Self::Random => 1,
        }
    }
}

/// What one duel unit plays: a point match with the cube, or a block of
/// independent cubeless money games without the Jacoby rule.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DuelFormat {
    #[default]
    Match,
    MoneyCubeless,
}

const MAX_BLOCK_GAMES: i32 = 1000;

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct Competitor {
    pub engine_id: String,
    pub plies: i32,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateSeries {
    pub a: Competitor,
    pub b: Competitor,
    #[serde(default)]
    pub format: DuelFormat,
    /// Point match length; 1–25 for matches, 0 or omitted for money blocks.
    #[serde(default)]
    #[schema(minimum = 0, maximum = 25)]
    pub match_length: i32,
    /// Games per stored block; required for cubeless money blocks.
    #[schema(minimum = 1, maximum = 1000)]
    pub block_games: Option<i32>,
    /// Matches or money blocks; pairs in paired mode.
    #[schema(minimum = 1, maximum = 1000)]
    pub count: i32,
    #[serde(default)]
    pub mode: DiceMode,
    pub seed: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct SeriesConfig {
    pub a: Competitor,
    pub b: Competitor,
    /// Absent in series created before money blocks existed.
    #[serde(default)]
    pub format: DuelFormat,
    /// 0 for money blocks.
    pub match_length: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_games: Option<i32>,
    pub count: i32,
    pub mode: DiceMode,
    pub seed: Uuid,
}

impl SeriesConfig {
    /// Validated play format of one unit, as the runner executes it.
    fn unit(&self) -> anyhow::Result<crate::play::engine_match::DuelUnit> {
        use crate::play::engine_match::DuelUnit;
        match (self.format, self.block_games) {
            (DuelFormat::Match, None) if (1..=25).contains(&self.match_length) => {
                Ok(DuelUnit::Match {
                    length: self.match_length,
                })
            }
            (DuelFormat::MoneyCubeless, Some(games))
                if self.match_length == 0 && (1..=MAX_BLOCK_GAMES).contains(&games) =>
            {
                Ok(DuelUnit::MoneyCubeless { games })
            }
            _ => anyhow::bail!("Invalid saved duel format"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct SeriesEngines {
    pub rules_version: String,
    pub a: EngineSnapshot,
    pub b: EngineSnapshot,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Series {
    pub id: Uuid,
    #[sqlx(json)]
    pub configuration: SeriesConfig,
    #[sqlx(json)]
    pub engines: SeriesEngines,
    pub dice_version: String,
    pub status: String,
    pub total_matches: i32,
    pub completed_matches: i32,
    /// Saved games in the unfinished match or block of an aborted series.
    #[sqlx(default)]
    pub saved_games_in_unit: Option<i32>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct MatchResult {
    pub match_index: i32,
    pub seed: Uuid,
    pub score_a: i32,
    pub score_b: i32,
    pub games: i32,
    #[serde(skip)]
    #[sqlx(flatten)]
    cost: MatchCost,
    /// Money blocks: games won by A as single, gammon, backgammon, then B.
    #[serde(skip)]
    outcomes: Option<Vec<i32>>,
}

/// Stored engine work per side. Each side is all NULL for matches played
/// before the measurement existed.
#[derive(Debug, Clone, Default, FromRow)]
struct MatchCost {
    cpu_us_a: Option<i64>,
    wall_us_a: Option<i64>,
    moves_a: Option<i32>,
    cpu_us_b: Option<i64>,
    wall_us_b: Option<i64>,
    moves_b: Option<i32>,
}

impl MatchCost {
    fn side(&self, side: usize) -> Option<(i64, i64, i32)> {
        match side {
            0 => Some((self.cpu_us_a?, self.wall_us_a?, self.moves_a?)),
            _ => Some((self.cpu_us_b?, self.wall_us_b?, self.moves_b?)),
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SeriesDetails {
    pub series: Series,
    pub live_progress: Option<LiveProgress>,
    pub statistics: SeriesStatistics,
    pub matches: Vec<MatchResult>,
    pub next_offset: Option<i32>,
    pub previous_offset: Option<i32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SeriesPage {
    pub runs: Vec<Series>,
    pub next_cursor: Option<Uuid>,
}
