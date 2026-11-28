//! Persisted head-to-head series, with a versioned dice schedule per match or pair.
mod checkpoint;
mod dice;
mod progress;
mod routes;
mod runner;
mod stats;
#[cfg(test)]
mod tests;

pub(crate) use routes::{EngineMatchDoc, routes};
pub(crate) use runner::ENGINE_DUELS_DATABASE_ID;
pub use runner::worker;
#[cfg(test)]
use stats::student_t_975;
use stats::{GameOutcomes, MoneyStatistics, SeriesStatistics, SideCostSummary, summarize};

use crate::settings::EngineSnapshot;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DiceMode {
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
enum DuelFormat {
    #[default]
    Match,
    MoneyCubeless,
}

const MAX_BLOCK_GAMES: i32 = 1000;

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
struct Competitor {
    engine_id: String,
    plies: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
struct CreateSeries {
    a: Competitor,
    b: Competitor,
    #[serde(default)]
    format: DuelFormat,
    /// Point match length; 1–25 for matches, 0 or omitted for money blocks.
    #[serde(default)]
    #[schema(minimum = 0, maximum = 25)]
    match_length: i32,
    /// Games per stored block; required for cubeless money blocks.
    #[schema(minimum = 1, maximum = 1000)]
    block_games: Option<i32>,
    /// Matches or money blocks; pairs in paired mode.
    #[schema(minimum = 1, maximum = 1000)]
    count: i32,
    #[serde(default)]
    mode: DiceMode,
    seed: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
struct SeriesConfig {
    a: Competitor,
    b: Competitor,
    /// Absent in series created before money blocks existed.
    #[serde(default)]
    format: DuelFormat,
    /// 0 for money blocks.
    match_length: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    block_games: Option<i32>,
    count: i32,
    mode: DiceMode,
    seed: Uuid,
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

#[derive(Debug, Deserialize, Serialize, ToSchema)]
struct SeriesEngines {
    rules_version: String,
    a: EngineSnapshot,
    b: EngineSnapshot,
}

#[derive(Debug, FromRow, Serialize, ToSchema)]
struct Series {
    id: Uuid,
    #[sqlx(json)]
    configuration: SeriesConfig,
    #[sqlx(json)]
    engines: SeriesEngines,
    dice_version: String,
    status: String,
    total_matches: i32,
    completed_matches: i32,
    /// Saved games in the unfinished match or block of an aborted series.
    #[sqlx(default)]
    pub saved_games_in_unit: Option<i32>,
    error: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Serialize, ToSchema)]
struct MatchResult {
    match_index: i32,
    seed: Uuid,
    score_a: i32,
    score_b: i32,
    games: i32,
    #[serde(skip)]
    #[sqlx(flatten)]
    cost: MatchCost,
    /// Money blocks: games won by A as single, gammon, backgammon, then B.
    #[serde(skip)]
    outcomes: Option<Vec<i32>>,
}

/// Stored engine work per side. Each side is all NULL for matches played
/// before the measurement existed.
#[derive(Debug, Default, FromRow)]
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

#[derive(Serialize, ToSchema)]
struct SeriesDetails {
    series: Series,
    live_progress: Option<progress::LiveProgress>,
    statistics: SeriesStatistics,
    matches: Vec<MatchResult>,
    next_offset: Option<i32>,
    previous_offset: Option<i32>,
}

#[derive(Serialize, ToSchema)]
struct SeriesPage {
    runs: Vec<Series>,
    next_cursor: Option<Uuid>,
}
