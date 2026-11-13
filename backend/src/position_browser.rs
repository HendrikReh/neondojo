//! Saved checker occurrences with context-preserving search and durable filter definitions.
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    model::{MatchSummary, MoveSeverity, Position},
    search::literal_pattern,
};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
    routing::get,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use std::collections::{BTreeSet, HashMap};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

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
    /// Open Sage plan for the player on roll.
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
    /// Native normalized equity loss. Individual hints do not imply a played-move loss.
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
            (&mut self.tag, 64),
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
        if let Some(tag) = &mut self.tag {
            *tag = tag.to_lowercase();
        }
        if self.position_id.as_ref().is_some_and(|id| {
            id.len() != 14
                || !id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"+/".contains(&c))
        }) {
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
            .is_some_and(|n| n <= 0 || n > 32768 || !(n as u32).is_power_of_two())
        {
            return Err(ApiError::invalid(
                "cube_value must be a power of two from 1 to 32768",
            ));
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
    pub job_id: Uuid,
    pub engine: String,
    pub plies: i32,
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

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/position-browser", get(search))
        .route(
            "/api/position-filters",
            get(list_filters).post(create_filter),
        )
        .route(
            "/api/position-filters/{id}",
            axum::routing::put(update_filter).delete(delete_filter),
        )
}

fn query<T>(value: Result<Query<T>, QueryRejection>) -> ApiResult<T> {
    value
        .map(|v| v.0)
        .map_err(|e| ApiError::invalid(e.body_text()))
}
fn json<T>(value: Result<Json<T>, JsonRejection>) -> ApiResult<T> {
    value
        .map(|v| v.0)
        .map_err(|e| ApiError::invalid(e.body_text()))
}
fn conflict(message: &str) -> ApiError {
    ApiError(StatusCode::CONFLICT, message.into())
}
fn text_filter(q: &mut QueryBuilder<'_, Postgres>, column: &str, text: &Option<String>) {
    if let Some(text) = text {
        q.push(" AND lower(")
            .push(column)
            .push(") LIKE ")
            .push_bind(literal_pattern(text))
            .push(" ESCAPE '\\'");
    }
}

const NOTE_SCOPE: &str = "(a.match_uuid=p.match_uuid OR a.position_uuid=p.id OR a.game_uuid=g.id OR a.move_uuid IN (SELECT mv.id FROM match_moves mv WHERE mv.position_uuid=p.id AND mv.match_uuid=p.match_uuid AND mv.game_no=p.game_no AND mv.kind='checker'))";
const NAME: &str = "lower(coalesce(nullif(m.display_name,''),m.filename))";
// A global-library cursor must never resume inside the explicit saved collection.
const CURSOR_SCOPE: &str = "saved-positions-v1:";
// Matches learning::card_signature; source occurrence UUIDs can differ for one shared card.
const CHECKER_SIGNATURE: &str = "(p.position_id || ':' || p.gnubg_match_id || ':checker')";

fn table_sort_expression(sort: PositionBrowserSort) -> String {
    match sort {
        PositionBrowserSort::Match => NAME.into(),
        PositionBrowserSort::Loss => "native.equity_loss".into(),
        PositionBrowserSort::MatchId => "coalesce(m.source_match_no,m.match_no)".into(),
        PositionBrowserSort::Database => "lower(coalesce(m.source_database_name,d.name))".into(),
        PositionBrowserSort::Game => "p.game_no".into(),
        PositionBrowserSort::Ply => "p.ply".into(),
        PositionBrowserSort::Player => "lower(nullif(btrim(CASE WHEN p.player=0 THEN m.white ELSE m.black END),''))".into(),
        PositionBrowserSort::Opponent => "lower(nullif(btrim(CASE WHEN p.player=0 THEN m.black ELSE m.white END),''))".into(),
        PositionBrowserSort::Dice => "CASE WHEN p.dice[1] IS NOT NULL AND p.dice[2] IS NOT NULL THEN ARRAY[least(p.dice[1],p.dice[2]),greatest(p.dice[1],p.dice[2])] END".into(),
        PositionBrowserSort::Score => "CASE WHEN p.source_white_score IS NOT NULL AND p.source_black_score IS NOT NULL THEN ARRAY[p.source_white_score,p.source_black_score] END".into(),
        PositionBrowserSort::Cube => "p.source_cube_value".into(),
        PositionBrowserSort::PlayedMove => "lower(nullif(btrim(p.notation),''))".into(),
        PositionBrowserSort::Decks => format!("(SELECT string_agg(lower(dk.name),chr(10) ORDER BY lower(dk.name) COLLATE \"C\",dk.name COLLATE \"C\",dk.id) FROM learning_cards c JOIN deck_cards dc ON dc.card_id=c.id JOIN decks dk ON dk.id=dc.deck_id WHERE c.signature={CHECKER_SIGNATURE})"),
        PositionBrowserSort::Tags => format!("nullif(ARRAY(SELECT DISTINCT tag FROM annotations a CROSS JOIN LATERAL unnest(a.tags) tag WHERE {NOTE_SCOPE} ORDER BY tag),ARRAY[]::text[])"),
        PositionBrowserSort::Comment => format!("(SELECT string_agg(lower(a.comment),chr(10) ORDER BY a.target_kind,a.id) FROM annotations a WHERE {NOTE_SCOPE} AND btrim(a.comment)<>'')"),
    }
}

fn table_order(sql: &mut QueryBuilder<'_, Postgres>, alias: &str, direction: &str) {
    sql.push(" ORDER BY ")
        .push(alias)
        .push(".sort_value ")
        .push(direction)
        .push(" NULLS LAST,")
        .push(alias)
        .push(".sort_name,")
        .push(alias)
        .push(".sort_match_id,")
        .push(alias)
        .push(".sort_ordinal,")
        .push(alias)
        .push(".sort_id");
}
// Learning chooses the highest individual result first and rejects an empty
// answer there; it does not fall back to an older hint or a match solution.
const READY: &str = "coalesce((SELECT CASE WHEN jsonb_typeof(hint.result->'candidates')='array' THEN jsonb_array_length(hint.result->'candidates')>0 AND btrim(coalesce(hint.result#>>'{candidates,0,notation}',''))<>'' ELSE false END FROM selected_engine_jobs hint WHERE hint.position_uuid=p.id AND hint.kind='analysis' AND hint.status='completed' AND hint.result IS NOT NULL ORDER BY hint.plies DESC,hint.created_at DESC,hint.id DESC LIMIT 1),native.job_id IS NOT NULL)";

// Select provenance before checking whether that decision contains a native
// solution. An unavailable newest decision must not expose an older error.
const NATIVE_SOLUTION: &str = "btrim(coalesce(chosen.data->>'best',''))<>'' AND ((chosen.data->>'evaluation_status'='forced' AND chosen.equity_loss=0 AND chosen.data->'equity_loss'='0'::jsonb) OR (chosen.data->>'evaluation_status'='evaluated' AND jsonb_typeof(chosen.data->'played_equity')='number' AND jsonb_typeof(chosen.data->'best_equity')='number'))";

async fn database_exists(pool: &PgPool, id: Option<Uuid>) -> ApiResult<()> {
    if let Some(id) = id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM databases WHERE id=$1 AND NOT collection_only)",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(anyhow::Error::from)?;
        if !exists {
            return Err(ApiError::invalid(
                "The filter's database no longer exists; choose an existing database explicitly",
            ));
        }
    }
    Ok(())
}

#[utoipa::path(get,path="/api/position-browser",params(PositionBrowserQuery),responses((status=200,body=PositionBrowserPage),(status=422,body=ErrorBody)),tag="Positions")]
async fn search(
    State(s): State<AppState>,
    value: Result<Query<PositionBrowserQuery>, QueryRejection>,
) -> ApiResult<Json<PositionBrowserPage>> {
    let query = query(value)?;
    let filters = match query.filters {
        Some(value) if value.len() <= 16000 => {
            serde_json::from_str::<PositionFilterDefinition>(&value)
                .map_err(|e| ApiError::invalid(format!("Invalid position filters: {e}")))?
        }
        Some(_) => return Err(ApiError::invalid("Position filter definition is too large")),
        None => PositionFilterDefinition::default(),
    }
    .validate()?;
    let limit = query.limit.unwrap_or(25);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::invalid("limit must be 1–100"));
    }
    let offset = crate::search::page_offset(query.page, query.after.is_some(), limit)?;
    database_exists(&s.pool, filters.database_id).await?;
    if filters.game_plan.is_some() {
        crate::position_game_plans::ensure_saved(&s.pool, filters.database_id).await?;
    }
    let scope = format!(
        "{CURSOR_SCOPE}{:x}",
        Sha256::digest(serde_json::to_vec(&filters).map_err(anyhow::Error::from)?)
    );
    let legacy_order = filters.direction.is_none()
        && matches!(
            filters.sort,
            PositionBrowserSort::Match | PositionBrowserSort::Loss
        );
    let direction =
        match filters
            .direction
            .unwrap_or(if filters.sort == PositionBrowserSort::Loss {
                PositionBrowserDirection::Desc
            } else {
                PositionBrowserDirection::Asc
            }) {
            PositionBrowserDirection::Asc => "ASC",
            PositionBrowserDirection::Desc => "DESC",
        };
    let cursor = if let Some(after) = query.after {
        if after.len() > 4096 {
            return Err(ApiError::invalid("Invalid position cursor"));
        }
        let cursor: BrowserCursor = URL_SAFE_NO_PAD
            .decode(after)
            .ok()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .ok_or_else(|| ApiError::invalid("Invalid position cursor"))?;
        let valid = match &cursor {
            BrowserCursor::Legacy(cursor) => {
                legacy_order
                    && cursor.scope == scope
                    && !cursor.loss.is_some_and(|n| !n.is_finite() || n < 0.0)
            }
            BrowserCursor::Table(cursor) => !legacy_order && cursor.scope == scope,
        };
        if !valid {
            return Err(ApiError::invalid(
                "Cursor does not match the position filters and sort",
            ));
        }
        Some(cursor)
    } else {
        None
    };
    let anchor = match &cursor {
        Some(BrowserCursor::Legacy(cursor)) => Some(cursor.id),
        Some(BrowserCursor::Table(cursor)) => Some(cursor.anchor),
        None => None,
    };
    // Every sort and cursor anchor uses the same saved, filtered snapshot.
    // Explicit summary fields keep source SGF and original bytes out of list reads.
    let mut sql = QueryBuilder::<Postgres>::new("WITH filtered AS NOT MATERIALIZED (");
    sql.push("SELECT to_jsonb(p) AS position,jsonb_build_object('id',m.id,'source_available',NOT m.collection_only,'filename',m.filename,'format',m.format,'white',m.white,'black',m.black,'match_length',m.match_length,'position_count',m.position_count,'created_at',m.created_at) AS match_info,NOT m.collection_only AS source_available,coalesce(m.source_match_no,m.match_no) AS match_no,coalesce(m.source_database_id,m.database_id) AS database_id,coalesce(m.source_database_name,d.name) AS database_name,coalesce(nullif(m.display_name,''),m.filename) AS display_name,m.tournament,m.round,native.job_id,native.engine,native.plies,native.equity_loss,");
    sql.push(table_sort_expression(filters.sort))
        .push(" AS sort_value,m.id AS sort_match_id,p.ordinal AS sort_ordinal,p.id AS sort_id,");
    sql.push(READY).push(" AS card_ready,").push(NAME).push(" AS sort_name FROM positions p JOIN saved_positions saved ON saved.position_uuid=p.id JOIN matches m ON m.id=p.match_uuid JOIN databases d ON d.id=m.database_id LEFT JOIN match_games g ON g.match_uuid=p.match_uuid AND g.game_no=p.game_no LEFT JOIN LATERAL (SELECT chosen.job_id,chosen.engine,chosen.plies,chosen.equity_loss FROM (SELECT j.id AS job_id,coalesce(j.result->>'engine','GNU Backgammon') AS engine,j.plies,ma.equity_loss,ma.data FROM move_analyses ma JOIN selected_engine_jobs j ON j.id=ma.job_id WHERE ma.position_uuid=p.id AND j.source_revision=m.source_revision AND ma.kind='checker' AND ma.game_no=p.game_no AND j.kind='match' AND j.match_uuid=p.match_uuid AND (j.game_no IS NULL OR j.game_no=p.game_no) AND j.status='completed' ORDER BY j.plies DESC,j.created_at DESC,j.id DESC,ma.id DESC LIMIT 1) chosen WHERE ").push(NATIVE_SOLUTION).push(") native ON true WHERE NOT EXISTS(SELECT 1 FROM play_sessions live WHERE live.library_match_id=m.id AND coalesce(live.state->>'play_mode','competition')='competition' AND live.state->>'phase'<>'match_over' AND NOT (live.state->>'match_length'='0' AND live.state->>'phase'='game_over'))");
    if let Some(plan) = filters.game_plan {
        sql.push(" AND EXISTS(SELECT 1 FROM position_game_plans plans WHERE plans.position_uuid=p.id AND plans.classifier=")
            .push_bind(crate::game_plan::CLASSIFIER)
            .push(" AND plans.game_plan=")
            .push_bind(crate::position_game_plans::label(plan))
            .push(")");
    }
    if let Some(id) = filters.database_id {
        sql.push(" AND coalesce(m.source_database_id,m.database_id)=")
            .push_bind(id);
    }
    text_filter(
        &mut sql,
        "CASE WHEN p.player=0 THEN m.white ELSE m.black END",
        &filters.player,
    );
    text_filter(
        &mut sql,
        "coalesce(nullif(m.display_name,''),m.filename)",
        &filters.match_name,
    );
    text_filter(&mut sql, "m.tournament", &filters.tournament);
    text_filter(&mut sql, "m.round", &filters.round);
    if let Some(id) = &filters.position_id {
        sql.push(" AND p.position_id=").push_bind(id.clone());
    }
    for (column, value, operator) in [
        ("m.match_length", filters.match_points_min, ">="),
        ("m.match_length", filters.match_points_max, "<="),
        ("p.source_white_score", filters.white_score, "="),
        ("p.source_black_score", filters.black_score, "="),
        ("p.source_cube_value", filters.cube_value, "="),
    ] {
        if let Some(value) = value {
            sql.push(" AND ")
                .push(column)
                .push(operator)
                .push_bind(value);
        }
    }
    if let Some(value) = filters.crawford {
        sql.push(" AND p.source_crawford=").push_bind(value);
    }
    match (filters.die1, filters.die2) {
        (Some(a), Some(b)) => {
            sql.push(" AND least(p.dice[1],p.dice[2])=")
                .push_bind(a)
                .push(" AND greatest(p.dice[1],p.dice[2])=")
                .push_bind(b);
        }
        (Some(a), None) | (None, Some(a)) => {
            sql.push(" AND p.dice @> ").push_bind(vec![a]);
        }
        _ => {}
    }
    if filters.tag.is_some() || filters.comment.is_some() {
        sql.push(" AND EXISTS(SELECT 1 FROM annotations a WHERE ")
            .push(NOTE_SCOPE);
        if let Some(tag) = &filters.tag {
            sql.push(" AND a.tags @> ").push_bind(vec![tag.clone()]);
        }
        if let Some(comment) = &filters.comment {
            sql.push(" AND lower(a.comment) LIKE ")
                .push_bind(literal_pattern(comment))
                .push(" ESCAPE '\\'");
        }
        sql.push(")");
    }
    if let Some(q) = &filters.q {
        let pattern = literal_pattern(q);
        sql.push(" AND (lower(m.white || ' ' || m.black || ' ' || m.filename || ' ' || m.display_name || ' ' || coalesce(m.tournament,'') || ' ' || coalesce(m.round,'') || ' ' || p.position_id) LIKE ").push_bind(pattern.clone()).push(" ESCAPE '\\' OR EXISTS(SELECT 1 FROM annotations a WHERE ").push(NOTE_SCOPE).push(" AND a.search_text LIKE ").push_bind(pattern).push(" ESCAPE '\\'))");
    }
    if let Some(state) = filters.analysis_state {
        sql.push(" AND ");
        if state == PositionAnalysisState::Missing {
            sql.push("NOT ");
        }
        sql.push(READY);
    }
    if let Some(loss) = filters.min_loss {
        sql.push(" AND native.equity_loss>=").push_bind(loss);
    }
    if let Some(loss) = filters.max_loss {
        sql.push(" AND native.equity_loss<=").push_bind(loss);
    }
    if let Some(anchor) = anchor {
        // Anchor lookup and page read share one filtered snapshot. A left-joined
        // marker distinguishes an exhausted page from a deleted/out-of-scope
        // anchor without running a less restrictive second lookup.
        sql.push("),anchor AS (SELECT * FROM filtered WHERE sort_id=")
            .push_bind(anchor)
            .push("),page AS (SELECT f.* FROM filtered f CROSS JOIN anchor a WHERE ((f.sort_value IS NULL AND a.sort_value IS NOT NULL) OR (f.sort_value IS NOT NULL AND a.sort_value IS NOT NULL AND f.sort_value ")
            .push(if direction == "ASC" { ">" } else { "<" })
            .push(" a.sort_value) OR (f.sort_value IS NOT DISTINCT FROM a.sort_value AND (f.sort_name,f.sort_match_id,f.sort_ordinal,f.sort_id)>(a.sort_name,a.sort_match_id,a.sort_ordinal,a.sort_id)))");
        table_order(&mut sql, "f", direction);
        sql.push(" LIMIT ").push_bind(limit + 1)
            .push(") SELECT page.*,EXISTS(SELECT 1 FROM anchor) AS anchor_exists FROM (VALUES (1)) marker(n) LEFT JOIN page ON true");
        table_order(&mut sql, "page", direction);
    } else if let Some(offset) = offset {
        sql.push("), page AS (SELECT f.* FROM filtered f");
        table_order(&mut sql, "f", direction);
        sql.push(" LIMIT ").push_bind(limit + 1)
            .push(" OFFSET ").push_bind(offset)
            .push(") SELECT page.*,totals.total FROM (SELECT count(*) AS total FROM filtered) totals LEFT JOIN page ON true");
        table_order(&mut sql, "page", direction);
    } else {
        sql.push(") SELECT f.* FROM filtered f");
        table_order(&mut sql, "f", direction);
        sql.push(" LIMIT ").push_bind(limit + 1);
    }
    let mut rows = sql
        .build()
        .fetch_all(&s.pool)
        .await
        .map_err(anyhow::Error::from)?;
    if anchor.is_some() {
        if rows
            .first()
            .is_some_and(|row| !row.get::<bool, _>("anchor_exists"))
        {
            return Err(ApiError::invalid(
                "Position cursor anchor no longer matches the filters",
            ));
        }
        rows.retain(|row| {
            row.get::<Option<serde_json::Value>, _>("position")
                .is_some()
        });
    }
    let total = if offset.is_some() {
        let total = rows[0].get::<i64, _>("total");
        rows.retain(|row| row.get::<Option<Uuid>, _>("sort_id").is_some());
        Some(total)
    } else {
        None
    };
    let more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let mut items = Vec::with_capacity(rows.len());
    let mut next_cursor = None;
    for row in rows {
        let position: Position =
            serde_json::from_value(row.get("position")).map_err(anyhow::Error::from)?;
        if more {
            next_cursor = Some(
                URL_SAFE_NO_PAD.encode(
                    serde_json::to_vec(&if legacy_order {
                        BrowserCursor::Legacy(Cursor {
                            scope: scope.clone(),
                            name: row.get("sort_name"),
                            match_id: position.match_uuid,
                            ordinal: position.ordinal,
                            id: position.id,
                            loss: row.get("equity_loss"),
                        })
                    } else {
                        BrowserCursor::Table(TableCursor {
                            scope: scope.clone(),
                            anchor: position.id,
                        })
                    })
                    .map_err(anyhow::Error::from)?,
                ),
            );
        }
        let job_id: Option<Uuid> = row.get("job_id");
        let native_error = job_id.map(|job_id| {
            let loss: f64 = row.get("equity_loss");
            PositionNativeError {
                job_id,
                engine: row.get("engine"),
                plies: row.get("plies"),
                equity_loss: loss,
                severity: MoveSeverity::from_normalized_loss(loss),
            }
        });
        let context: serde_json::Value = row.get("position");
        let notes=sqlx::query(&format!("SELECT a.target_kind,a.tags,left(a.comment,280) AS comment_excerpt FROM positions p LEFT JOIN match_games g ON g.match_uuid=p.match_uuid AND g.game_no=p.game_no JOIN annotations a ON {NOTE_SCOPE} WHERE p.id=$1 ORDER BY a.target_kind,a.id"))
            .bind(position.id).fetch_all(&s.pool).await.map_err(anyhow::Error::from)?.into_iter()
            .map(|r|PositionBrowserNote{scope:r.get("target_kind"),tags:r.get("tags"),comment_excerpt:r.get("comment_excerpt")}).collect::<Vec<_>>();
        let tags = notes
            .iter()
            .flat_map(|n| n.tags.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        items.push(PositionBrowserHit {
            position,
            source_available: row.get("source_available"),
            match_info: serde_json::from_value(row.get("match_info"))
                .map_err(anyhow::Error::from)?,
            match_no: row.get("match_no"),
            database_id: row.get("database_id"),
            database_name: row.get("database_name"),
            display_name: row.get("display_name"),
            tournament: row.get("tournament"),
            round: row.get("round"),
            white_score: context["source_white_score"].as_i64().map(|v| v as i32),
            black_score: context["source_black_score"].as_i64().map(|v| v as i32),
            crawford: context["source_crawford"].as_bool(),
            cube_value: context["source_cube_value"].as_i64().map(|v| v as i32),
            cube_owner: context["source_cube_owner"].as_i64().map(|v| v as i32),
            tags,
            notes,
            card_ready: row.get("card_ready"),
            decks: Vec::new(),
            native_error,
        });
    }
    load_deck_memberships(&s.pool, &mut items).await?;
    Ok(Json(PositionBrowserPage {
        items,
        next_cursor,
        total,
    }))
}

async fn load_deck_memberships(pool: &PgPool, items: &mut [PositionBrowserHit]) -> ApiResult<()> {
    if items.is_empty() {
        return Ok(());
    }
    let ids: Vec<Uuid> = items.iter().map(|hit| hit.position.id).collect();
    let rows: Vec<(Uuid, Uuid, String)> = sqlx::query_as(&format!(
        "SELECT p.id,dk.id,dk.name FROM positions p JOIN learning_cards c ON c.signature={CHECKER_SIGNATURE} JOIN deck_cards dc ON dc.card_id=c.id JOIN decks dk ON dk.id=dc.deck_id WHERE p.id=ANY($1) ORDER BY lower(dk.name) COLLATE \"C\",dk.name COLLATE \"C\",dk.id"
    )).bind(ids).fetch_all(pool).await.map_err(anyhow::Error::from)?;
    let mut memberships = HashMap::<Uuid, Vec<PositionDeckMembership>>::new();
    for (position, id, name) in rows {
        memberships
            .entry(position)
            .or_default()
            .push(PositionDeckMembership { id, name });
    }
    for hit in items {
        hit.decks = memberships.remove(&hit.position.id).unwrap_or_default();
    }
    Ok(())
}

const SAVED_SELECT: &str = "SELECT f.*,coalesce((f.definition->>'database_id') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM databases d WHERE d.id=(f.definition->>'database_id')::uuid),false) AS database_missing FROM position_filters f";
fn saved(row: sqlx::postgres::PgRow) -> ApiResult<SavedPositionFilter> {
    Ok(SavedPositionFilter {
        id: row.get("id"),
        name: row.get("name"),
        definition: serde_json::from_value(row.get("definition")).map_err(anyhow::Error::from)?,
        version: row.get("version"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        database_missing: row.get("database_missing"),
    })
}
fn validate_name(name: String) -> ApiResult<String> {
    let name = name.trim().to_owned();
    if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(ApiError::invalid(
            "Filter name must be 1–120 characters without control characters",
        ));
    }
    Ok(name)
}
fn write_error(e: sqlx::Error) -> ApiError {
    if e.as_database_error()
        .is_some_and(|e| e.is_unique_violation())
    {
        conflict("A position filter with this name already exists")
    } else {
        anyhow::Error::from(e).into()
    }
}

#[utoipa::path(get,path="/api/position-filters",responses((status=200,body=[SavedPositionFilter])),tag="Positions")]
async fn list_filters(State(s): State<AppState>) -> ApiResult<Json<Vec<SavedPositionFilter>>> {
    let rows = sqlx::query(&format!("{SAVED_SELECT} ORDER BY lower(f.name),f.id"))
        .fetch_all(&s.pool)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(Json(rows.into_iter().map(saved).collect::<ApiResult<_>>()?))
}

#[utoipa::path(post,path="/api/position-filters",request_body=CreatePositionFilter,responses((status=201,body=SavedPositionFilter),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Positions")]
async fn create_filter(
    State(s): State<AppState>,
    value: Result<Json<CreatePositionFilter>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<SavedPositionFilter>)> {
    let input = json(value)?;
    let name = validate_name(input.name)?;
    let definition = input.definition.validate()?;
    database_exists(&s.pool, definition.database_id).await?;
    let mut tx = s.pool.begin().await.map_err(anyhow::Error::from)?;
    sqlx::query("SELECT pg_advisory_xact_lock(74619781644280)")
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM position_filters")
        .fetch_one(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    if count >= 200 {
        return Err(ApiError::invalid(
            "At most 200 saved position filters are supported",
        ));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO position_filters(id,name,definition) VALUES($1,$2,$3)")
        .bind(id)
        .bind(name)
        .bind(serde_json::to_value(definition).map_err(anyhow::Error::from)?)
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
    let result = saved(
        sqlx::query(&format!("{SAVED_SELECT} WHERE f.id=$1"))
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?,
    )?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(put,path="/api/position-filters/{id}",params(("id"=Uuid,Path)),request_body=UpdatePositionFilter,responses((status=200,body=SavedPositionFilter),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Positions")]
async fn update_filter(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    value: Result<Json<UpdatePositionFilter>, JsonRejection>,
) -> ApiResult<Json<SavedPositionFilter>> {
    let input = json(value)?;
    let name = validate_name(input.name)?;
    let definition = input.definition.validate()?;
    if input.expected_version < 1 {
        return Err(ApiError::invalid("expected_version must be positive"));
    }
    database_exists(&s.pool, definition.database_id).await?;
    let mut tx = s.pool.begin().await.map_err(anyhow::Error::from)?;
    let changed=sqlx::query("UPDATE position_filters SET name=$2,definition=$3,version=version+1,updated_at=now() WHERE id=$1 AND version=$4").bind(id).bind(name).bind(serde_json::to_value(definition).map_err(anyhow::Error::from)?).bind(input.expected_version).execute(&mut *tx).await.map_err(write_error)?.rows_affected();
    let row = sqlx::query(&format!("{SAVED_SELECT} WHERE f.id=$1"))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    if changed == 0 {
        return Err(conflict(
            "Position filter was changed; reload before saving",
        ));
    }
    let result = saved(row)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(result))
}

#[utoipa::path(delete,path="/api/position-filters/{id}",params(("id"=Uuid,Path),DeletePositionFilter),responses((status=204),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Positions")]
async fn delete_filter(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    value: Result<Query<DeletePositionFilter>, QueryRejection>,
) -> ApiResult<StatusCode> {
    let input = query(value)?;
    if input.expected_version < 1 {
        return Err(ApiError::invalid("expected_version must be positive"));
    }
    let changed = sqlx::query("DELETE FROM position_filters WHERE id=$1 AND version=$2")
        .bind(id)
        .bind(input.expected_version)
        .execute(&s.pool)
        .await
        .map_err(anyhow::Error::from)?
        .rows_affected();
    if changed == 0 {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM position_filters WHERE id=$1)")
                .bind(id)
                .fetch_one(&s.pool)
                .await
                .map_err(anyhow::Error::from)?;
        return Err(if exists {
            conflict("Position filter was changed; reload before deleting")
        } else {
            ApiError::missing()
        });
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(OpenApi)]
#[openapi(
    paths(search, list_filters, create_filter, update_filter, delete_filter),
    components(schemas(
        PositionFilterDefinition,
        PositionBrowserSort,
        PositionBrowserDirection,
        PositionAnalysisState,
        PositionBrowserPage,
        PositionBrowserHit,
        PositionBrowserNote,
        PositionDeckMembership,
        PositionNativeError,
        SavedPositionFilter,
        CreatePositionFilter,
        UpdatePositionFilter
    ))
)]
pub struct PositionBrowserDoc;
