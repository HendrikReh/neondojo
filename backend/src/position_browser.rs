//! Saved checker occurrences with context-preserving search and durable filter definitions.
use crate::json_errors::{Json, Path, Query};
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    model::{MatchSummary, MoveSeverity, Position},
    search::literal_pattern,
};
use axum::{Router, extract::State, http::StatusCode, routing::get};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use std::collections::{BTreeSet, HashMap};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

mod browse;
#[cfg(test)]
mod browse_performance;
mod filters;
mod routes;
use browse::*;
use filters::*;
use routes::*;
pub use routes::{PositionBrowserDoc, routes};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PositionBrowserSort {
    #[default]
    Match,
    Loss,
    MatchId,
    Database,
    Game,
    Ply,
    Player,
    Opponent,
    Dice,
    Score,
    Cube,
    PlayedMove,
    Decks,
    Tags,
    Comment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PositionBrowserDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PositionAnalysisState {
    Ready,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PositionFilterDefinition {
    /// NeonGammon plan for the player on roll.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_plan: Option<crate::game_plan::GamePlan>,
    /// Supported definition version: exactly 1.
    #[schema(minimum = 1, maximum = 1)]
    pub schema_version: i32,
    pub database_id: Option<Uuid>,
    pub q: Option<String>,
    /// Literal name substring of the player on roll, rather than either match participant.
    pub player: Option<String>,
    pub match_name: Option<String>,
    pub tournament: Option<String>,
    pub round: Option<String>,
    pub tag: Option<String>,
    pub comment: Option<String>,
    pub position_id: Option<String>,
    pub match_points_min: Option<i32>,
    pub match_points_max: Option<i32>,
    pub die1: Option<i32>,
    pub die2: Option<i32>,
    pub white_score: Option<i32>,
    pub black_score: Option<i32>,
    pub crawford: Option<bool>,
    pub cube_value: Option<i32>,
    pub analysis_state: Option<PositionAnalysisState>,
    /// Selected normalized equity loss. Individual hints do not imply a played-move loss.
    pub min_loss: Option<f64>,
    pub max_loss: Option<f64>,
    #[serde(default)]
    pub sort: PositionBrowserSort,
    /// Omitted: loss descending; all other columns ascending. Legacy definitions
    /// omit this field when serialized to preserve their stored definition format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<PositionBrowserDirection>,
}

impl Default for PositionFilterDefinition {
    fn default() -> Self {
        serde_json::from_value(serde_json::json!({"schema_version":1}))
            .expect("valid default definition")
    }
}

impl PositionFilterDefinition {
    pub(crate) fn validate(mut self) -> ApiResult<Self> {
        if self.schema_version != 1 {
            return Err(ApiError::invalid(
                "Unsupported position filter schema_version; expected 1",
            ));
        }
        for (text, max) in [
            (&mut self.q, 200),
            (&mut self.player, 240),
            (&mut self.match_name, 240),
            (&mut self.tournament, 240),
            (&mut self.round, 120),
            (&mut self.comment, 200),
            (&mut self.position_id, 14),
        ] {
            if let Some(value) = text {
                *value = value.trim().to_owned();
                if value.chars().count() > max || value.chars().any(char::is_control) {
                    return Err(ApiError::invalid(
                        "Position filter text is too long or contains control characters",
                    ));
                }
                if value.is_empty() {
                    *text = None;
                }
            }
        }
        if let Some(tag) = self.tag.take() {
            let normalized = crate::annotations::normalize_tag(&tag)?;
            if !normalized.is_empty() {
                self.tag = Some(normalized);
            }
        }
        if self
            .position_id
            .as_deref()
            .is_some_and(|id| !crate::engine::valid_id(id, 14))
        {
            return Err(ApiError::invalid(
                "position_id must be a 14-character GNUbg checker ID",
            ));
        }
        if [
            self.match_points_min,
            self.match_points_max,
            self.white_score,
            self.black_score,
        ]
        .into_iter()
        .flatten()
        .any(|n| !(0..=32767).contains(&n))
            || self
                .match_points_min
                .zip(self.match_points_max)
                .is_some_and(|(min, max)| min > max)
        {
            return Err(ApiError::invalid(
                "Match points and scores must be 0–32767 with minimum <= maximum",
            ));
        }
        if [self.die1, self.die2]
            .into_iter()
            .flatten()
            .any(|n| !(1..=6).contains(&n))
        {
            return Err(ApiError::invalid("Dice must be 1–6"));
        }
        if self.die1.zip(self.die2).is_some_and(|(a, b)| a > b) {
            std::mem::swap(&mut self.die1, &mut self.die2);
        }
        if self
            .cube_value
            .is_some_and(|n| !crate::backgammon::board::is_valid_cube_value(n))
        {
            return Err(ApiError::invalid(format!(
                "cube_value must be a power of two from 1 to {}",
                crate::backgammon::board::MAX_CUBE_VALUE
            )));
        }
        if [self.min_loss, self.max_loss]
            .into_iter()
            .flatten()
            .any(|n| !n.is_finite() || !(0.0..=1000.0).contains(&n))
            || self
                .min_loss
                .zip(self.max_loss)
                .is_some_and(|(min, max)| min > max)
        {
            return Err(ApiError::invalid(
                "Loss bounds must be finite, 0–1000, with minimum <= maximum",
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PositionBrowserNote {
    #[schema(value_type=crate::annotations::AnnotationKind)]
    pub scope: String,
    pub tags: Vec<String>,
    pub comment_excerpt: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PositionNativeError {
    pub job_id: Option<Uuid>,
    pub analysis_id: String,
    pub imported_analysis_id: Option<Uuid>,
    pub engine: String,
    pub plies: Option<i32>,
    pub depth: Option<String>,
    pub equity_loss: f64,
    pub severity: MoveSeverity,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PositionDeckMembership {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PositionBrowserHit {
    pub position: Position,
    pub match_info: MatchSummary,
    /// False when only saved occurrences remain after source deletion.
    pub source_available: bool,
    /// Persistent library number, independent of the match UUID and row order.
    pub match_no: i64,
    pub database_id: Uuid,
    pub database_name: String,
    pub display_name: String,
    pub tournament: Option<String>,
    pub round: Option<String>,
    pub white_score: Option<i32>,
    pub black_score: Option<i32>,
    pub crawford: Option<bool>,
    pub cube_value: Option<i32>,
    /// -1: centered; 0: White; 1: Black; null: unknown source context.
    pub cube_owner: Option<i32>,
    pub tags: Vec<String>,
    pub notes: Vec<PositionBrowserNote>,
    pub card_ready: bool,
    /// Learning decks for this checker decision's full Position-ID/Match-ID identity.
    pub decks: Vec<PositionDeckMembership>,
    pub native_error: Option<PositionNativeError>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PositionBrowserPage {
    pub items: Vec<PositionBrowserHit>,
    pub next_cursor: Option<String>,
    /// Total matching rows when `page` is supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
}

#[derive(Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct PositionBrowserQuery {
    /// URL-encoded JSON PositionFilterDefinition, applied only to explicitly saved positions.
    /// Omitted: the saved collection in match order.
    #[param(example = r#"{"schema_version":1,"player":"Alice","sort":"match"}"#)]
    pub filters: Option<String>,
    pub after: Option<String>,
    /// One-based result page. Mutually exclusive with `after`.
    pub page: Option<i64>,
    /// Default 25, maximum 100.
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    scope: String,
    name: String,
    match_id: Uuid,
    ordinal: i32,
    id: Uuid,
    loss: Option<f64>,
}

/// Table cursors identify an anchor in the same filtered query. Sort values are
/// read from that row, keeping long comments and tag arrays out of the URL.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TableCursor {
    scope: String,
    anchor: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum BrowserCursor {
    Legacy(Cursor),
    Table(TableCursor),
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SavedPositionFilter {
    pub id: Uuid,
    pub name: String,
    pub definition: PositionFilterDefinition,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub database_missing: bool,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatePositionFilter {
    pub name: String,
    pub definition: PositionFilterDefinition,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdatePositionFilter {
    pub name: String,
    pub definition: PositionFilterDefinition,
    pub expected_version: i64,
}
#[derive(Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct DeletePositionFilter {
    pub expected_version: i64,
}

pub(crate) const SAVED_VISIBLE: &str = concat!("NOT ", crate::play::review_blocked_sql!("m.id"));

#[derive(Serialize, ToSchema)]
struct PositionAnalysisReadiness {
    analysis_required: i64,
    unavailable: i64,
    first_analysis_required: Option<Uuid>,
}
