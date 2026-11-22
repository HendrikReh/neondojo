//! Independent match databases, indexed library browsing and bounded bulk management.
use crate::annotations::normalize_tag;
use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use crate::play::PAUSED_MATCHES_DATABASE_ID;
use crate::search::literal_pattern;
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    io::{Cursor, Write},
};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

mod actions;
mod browse;
mod repo;
mod routes;
mod source_metadata;
use actions::*;
pub use browse::browse;
use browse::*;
use repo::*;
pub(crate) use repo::{database_matches, require_library_destination};
pub use routes::{DatabaseDoc, routes};
pub use source_metadata::{SourceMetadata, backfill_source_metadata, source_metadata};

pub(crate) async fn default_database_id(pool: &PgPool) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM databases WHERE is_default")
        .fetch_one(pool)
        .await
}
const MAX_ACTIONS: usize = 100;
const MAX_EXPORTS: usize = 50;
const MAX_EXPORT_BYTES: i64 = 64 * 1024 * 1024;
pub(crate) fn conflict(message: &str) -> ApiError {
    ApiError(StatusCode::CONFLICT, message.into())
}
pub(crate) fn query<T>(q: Result<Query<T>, QueryRejection>) -> ApiResult<T> {
    q.map(|q| q.0).map_err(|e| ApiError::invalid(e.body_text()))
}
pub(crate) fn json<T>(j: Result<Json<T>, JsonRejection>) -> ApiResult<T> {
    j.map(|j| j.0).map_err(|e| ApiError::invalid(e.body_text()))
}

#[derive(Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct Database {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub version: i64,
    pub is_default: bool,
    pub is_system: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub match_count: i64,
}
#[derive(Deserialize, ToSchema)]
pub struct DatabaseWrite {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub version: Option<i64>,
}
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DatabaseDelete {
    pub version: i64,
    #[serde(default)]
    pub delete_matches: bool,
}
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DatabaseVersion {
    pub version: i64,
}
/// One stored occurrence of the searched checker position.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LibraryOccurrence {
    pub id: Uuid,
    pub game_no: i32,
    pub move_no: i32,
    pub database_name: String,
}
#[derive(Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct DatabaseMatch {
    /// Present in position-filtered results, where every stored occurrence is a row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[sqlx(skip)]
    pub occurrence: Option<LibraryOccurrence>,
    /// Sides belonging to the requested player profile, when filtered by player_id.
    /// 0 is White and 1 is Black. Both may belong to the same profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[sqlx(skip)]
    pub player_sides: Option<Vec<i16>>,
    /// Profile of the other side, when filtered by player_id and that side
    /// belongs to a different catalogued player.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[sqlx(skip)]
    pub opponent_player_id: Option<Uuid>,
    pub id: Uuid,
    /// False for independent saved-position context whose source match was deleted.
    pub source_available: bool,
    pub play_session_id: Option<Uuid>,
    /// A live play checkpoint that must be explicitly resumed.
    pub play_paused: bool,
    /// Saved learning report available for this match; never implies generation.
    pub study_report_id: Option<Uuid>,
    pub play_phase: Option<String>,
    pub play_mode: Option<crate::play::PlayMode>,
    pub play_assisted: bool,
    /// Persistent numeric library entry ID; UUID remains the action identity.
    pub match_no: i64,
    pub filename: String,
    pub format: String,
    pub white: String,
    pub black: String,
    pub match_length: i32,
    pub position_count: i32,
    pub created_at: DateTime<Utc>,
    pub database_id: Uuid,
    pub display_name: String,
    pub played_on: Option<NaiveDate>,
    pub tournament: Option<String>,
    pub round: Option<String>,
    pub place: Option<String>,
    pub result_state: String,
    pub white_score: Option<i32>,
    pub black_score: Option<i32>,
    pub white_pr: Option<f64>,
    pub black_pr: Option<f64>,
    pub pr_method: Option<String>,
    pub pr_profile: Option<String>,
    pub analysis_status: Option<String>,
    pub game_count: i64,
    /// Source irregularities the import resolved; see `/api/matches/{id}/import-warnings`.
    pub import_warning_count: i32,
    pub tags: Vec<String>,
    pub comment: String,
    pub version: i64,
}
#[derive(Serialize, Deserialize, ToSchema)]
pub struct LibraryPage {
    pub items: Vec<DatabaseMatch>,
    pub next_cursor: Option<String>,
    /// Total matching rows when `page` is supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
}
#[derive(Default, Clone, Serialize, Deserialize, IntoParams)]
pub struct LibraryQuery {
    pub database_id: Option<Uuid>,
    pub q: Option<String>,
    pub player: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_id: Option<Uuid>,
    /// Matches of a virtual player profile; excludes `player_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_player_id: Option<Uuid>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub date_field: Option<String>,
    pub tag: Option<String>,
    pub comment: Option<String>,
    /// Search checker Position-ID; returns each occurrence, including repeats in one game.
    pub position_id: Option<String>,
    pub tournament: Option<String>,
    pub round: Option<String>,
    pub match_name: Option<String>,
    pub match_type: Option<String>,
    pub result: Option<String>,
    pub match_points_min: Option<i32>,
    pub match_points_max: Option<i32>,
    /// Sort field: id (or match_no), created_at, played_at, name, white, black,
    /// white_pr, black_pr, match_length, score, result, tournament, format,
    /// position_count, analysis_status, tags, or comment. With position_id, also
    /// occurrence_database (case-insensitive name), occurrence_game, occurrence_move.
    /// score compares the
    /// visible numeric white/black score pair; result retains result-state order.
    pub sort: Option<String>,
    /// asc or desc (default desc). Missing values sort last in either direction;
    /// equal values use ascending UUID order for stable pagination.
    pub direction: Option<String>,
    pub after: Option<String>,
    /// One-based result page. Mutually exclusive with `after`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<i64>,
    pub limit: Option<i64>,
}
#[derive(Serialize, Deserialize)]
struct OccurrenceCursor {
    id: Uuid,
    ordinal: i32,
}
#[derive(Serialize, Deserialize)]
struct LibraryCursor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    occurrence: Option<OccurrenceCursor>,
    scope: String,
    key: Option<String>,
    id: Uuid,
    // Annotation values can be tens of thousands of Unicode characters. Resolve
    // those keys from the scoped anchor instead of putting them into the URL.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    anchor: bool,
}
#[derive(Deserialize, ToSchema)]
pub struct MetadataWrite {
    pub version: i64,
    #[serde(default)]
    pub display_name: String,
    pub played_on: Option<NaiveDate>,
    pub tournament: Option<String>,
    pub round: Option<String>,
    pub place: Option<String>,
    pub result_state: String,
    pub white_score: Option<i32>,
    pub black_score: Option<i32>,
}
#[derive(Deserialize, ToSchema)]
pub struct LibraryAction {
    pub action: String,
    pub match_ids: Vec<Uuid>,
    pub database_id: Option<Uuid>,
    pub target_database_id: Option<Uuid>,
    #[serde(default)]
    pub add_tags: Vec<String>,
    #[serde(default)]
    pub remove_tags: Vec<String>,
}
#[derive(Serialize, ToSchema)]
pub struct ActionItem {
    pub match_id: Uuid,
    pub status: String,
    pub created_id: Option<Uuid>,
    pub existing_id: Option<Uuid>,
    pub error: Option<String>,
}
#[derive(Serialize, ToSchema)]
pub struct ActionResponse {
    pub results: Vec<ActionItem>,
}
#[derive(Deserialize, ToSchema)]
pub struct ExportRequest {
    pub match_ids: Vec<Uuid>,
    pub format: String,
}

const DB_SELECT: &str = "SELECT d.*, (SELECT count(*) FROM matches m WHERE m.database_id=d.id) AS match_count FROM databases d";
fn validate_database(w: &DatabaseWrite) -> ApiResult<()> {
    if w.name.trim().is_empty()
        || w.name.chars().count() > 120
        || w.description.chars().count() > 2000
    {
        return Err(ApiError::invalid(
            "name must be 1–120 characters; description at most 2000",
        ));
    }
    Ok(())
}

impl LibraryQuery {
    /// The player whose sides the rows report: `(id, virtual profile)`.
    fn player_scope(&self) -> Option<(Uuid, bool)> {
        self.player_id
            .map(|id| (id, false))
            .or(self.virtual_player_id.map(|id| (id, true)))
    }
}
