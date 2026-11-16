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

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/databases", get(list_databases).post(create_database))
        .route(
            "/api/databases/{id}",
            axum::routing::put(update_database).delete(delete_database),
        )
        .route(
            "/api/databases/{id}/matches",
            axum::routing::delete(clear_database),
        )
        .route("/api/library/matches", get(library_matches))
        .route(
            "/api/matches/{id}/metadata",
            get(get_metadata)
                .put(update_metadata)
                .patch(update_metadata),
        )
        .route("/api/library/actions", post(actions))
        .route("/api/library/export", post(export))
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
#[utoipa::path(get,path="/api/databases",responses((status=200,body=[Database])),tag="Databases")]
async fn list_databases(State(s): State<AppState>) -> ApiResult<Json<Vec<Database>>> {
    Ok(Json(
        sqlx::query_as(&format!(
            "{DB_SELECT} WHERE NOT d.collection_only ORDER BY is_default DESC,lower(name),id"
        ))
        .fetch_all(&s.pool)
        .await
        .map_err(anyhow::Error::from)?,
    ))
}
#[utoipa::path(post,path="/api/databases",request_body=DatabaseWrite,responses((status=201,body=Database),(status=422,body=ErrorBody)),tag="Databases")]
async fn create_database(
    State(s): State<AppState>,
    body: Result<Json<DatabaseWrite>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<Database>)> {
    let w = json(body)?;
    validate_database(&w)?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO databases(id,name,description) VALUES($1,$2,$3)")
        .bind(id)
        .bind(w.name.trim())
        .bind(w.description)
        .execute(&s.pool)
        .await
        .map_err(anyhow::Error::from)?;
    Ok((
        StatusCode::CREATED,
        Json(
            sqlx::query_as(&format!("{DB_SELECT} WHERE d.id=$1"))
                .bind(id)
                .fetch_one(&s.pool)
                .await
                .map_err(anyhow::Error::from)?,
        ),
    ))
}
#[utoipa::path(put,path="/api/databases/{id}",params(("id"=Uuid,Path)),request_body=DatabaseWrite,responses((status=200,body=Database),(status=409,body=ErrorBody)),tag="Databases")]
async fn update_database(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<Json<DatabaseWrite>, JsonRejection>,
) -> ApiResult<Json<Database>> {
    let w = json(body)?;
    validate_database(&w)?;
    let version = w
        .version
        .filter(|v| *v > 0)
        .ok_or_else(|| ApiError::invalid("version is required"))?;
    let name = w.name.trim();
    let affected = sqlx::query(
        "UPDATE databases SET name=$2,description=$3,version=version+1,updated_at=now()
         WHERE id=$1 AND version=$4 AND NOT collection_only AND (NOT is_system OR name=$2)",
    )
    .bind(id)
    .bind(name)
    .bind(w.description)
    .bind(version)
    .execute(&s.pool)
    .await
    .map_err(anyhow::Error::from)?
    .rows_affected();
    if affected == 0 {
        let renamed_system: Option<bool> = sqlx::query_scalar(
            "SELECT is_system AND name<>$2 FROM databases WHERE id=$1 AND NOT collection_only",
        )
        .bind(id)
        .bind(name)
        .fetch_optional(&s.pool)
        .await
        .map_err(anyhow::Error::from)?;
        return Err(match renamed_system {
            Some(true) => conflict("System databases cannot be renamed"),
            Some(false) => conflict("Database changed; reload before saving"),
            None => ApiError::missing(),
        });
    }
    Ok(Json(
        sqlx::query_as(&format!("{DB_SELECT} WHERE d.id=$1"))
            .bind(id)
            .fetch_one(&s.pool)
            .await
            .map_err(anyhow::Error::from)?,
    ))
}
pub(crate) async fn require_library_destination(pool: &PgPool, id: Uuid) -> ApiResult<()> {
    if id == PAUSED_MATCHES_DATABASE_ID {
        return Err(conflict(
            "Only paused sessions can be added to Paused Matches",
        ));
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM databases WHERE id=$1 AND NOT collection_only)",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(anyhow::Error::from)?;
    if !exists {
        return Err(ApiError::missing());
    }
    Ok(())
}
#[utoipa::path(delete,path="/api/databases/{id}",params(("id"=Uuid,Path),DatabaseDelete),responses((status=204),(status=409,body=ErrorBody)),tag="Databases")]
async fn delete_database(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    q: Result<Query<DatabaseDelete>, QueryRejection>,
) -> ApiResult<StatusCode> {
    let q = query(q)?;
    remove_database_content(
        &s.pool,
        id,
        q.version,
        DatabaseRemoval::Delete {
            delete_matches: q.delete_matches,
        },
    )
    .await
}

#[utoipa::path(delete,path="/api/databases/{id}/matches",params(("id"=Uuid,Path),DatabaseVersion),responses((status=204),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Databases")]
async fn clear_database(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    q: Result<Query<DatabaseVersion>, QueryRejection>,
) -> ApiResult<StatusCode> {
    remove_database_content(&s.pool, id, query(q)?.version, DatabaseRemoval::Clear).await
}

#[derive(Clone, Copy)]
enum DatabaseRemoval {
    Clear,
    Delete { delete_matches: bool },
}

async fn remove_database_content(
    pool: &PgPool,
    id: Uuid,
    expected_version: i64,
    removal: DatabaseRemoval,
) -> ApiResult<StatusCode> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    let d: Option<(i64, bool)> = sqlx::query_as(
        "SELECT version,is_system FROM databases WHERE id=$1 AND NOT collection_only FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let Some((version, is_system)) = d else {
        return Err(ApiError::missing());
    };
    match removal {
        DatabaseRemoval::Delete { .. } if is_system => {
            return Err(conflict("System databases cannot be deleted"));
        }
        DatabaseRemoval::Clear if !is_system => {
            return Err(conflict("Only system databases can be cleared"));
        }
        DatabaseRemoval::Clear if id == PAUSED_MATCHES_DATABASE_ID => {
            return Err(conflict("Paused Matches cannot be cleared"));
        }
        _ => {}
    }
    if version != expected_version {
        return Err(conflict("Database changed; reload before deleting"));
    }
    // Match locks serialize deletion with bulk operations and job submission FK checks.
    let ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM matches WHERE database_id=$1 ORDER BY id FOR UPDATE")
            .bind(id)
            .fetch_all(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    if !ids.is_empty()
        && matches!(
            removal,
            DatabaseRemoval::Delete {
                delete_matches: false
            }
        )
    {
        return Err(conflict(
            "Database is not empty; delete_matches=true is required",
        ));
    }
    if any_active(&mut tx, &ids).await? {
        return Err(conflict("Database has queued or running analyses"));
    }
    for &match_id in &ids {
        crate::position_retention::preserve(&mut tx, match_id).await?;
    }
    sqlx::query("DELETE FROM matches WHERE database_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let finish = match removal {
        DatabaseRemoval::Clear => {
            "UPDATE databases SET version=version+1,updated_at=now() WHERE id=$1"
        }
        DatabaseRemoval::Delete { .. } => "DELETE FROM databases WHERE id=$1",
    };
    sqlx::query(finish)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}

const MATCH_PLAY_JOIN: &str = " LEFT JOIN play_sessions ps ON ps.library_match_id=m.id";
const MATCH_NOTE_JOIN: &str = " LEFT JOIN annotations a ON a.match_uuid=m.id";
// Select the PR source independently of any active whole-match job.
// Incomplete or imported selected results never borrow another source's rating.
const MATCH_ANALYSIS_JOINS: &str = "
LEFT JOIN LATERAL (
    SELECT a.source,a.job_id FROM effective_analysis_selections a WHERE a.match_id=m.id LIMIT 1
) selected ON true
LEFT JOIN jobs selected_job ON selected_job.id=selected.job_id
LEFT JOIN LATERAL (
    SELECT selected_job.result,selected_job.total_games,selected_job.completed_games
    WHERE selected_job.match_uuid=m.id AND selected_job.source_revision=m.source_revision AND selected_job.kind='match' AND selected_job.game_no IS NULL AND selected_job.status='completed' AND NOT m.collection_only
) rawpr ON true
LEFT JOIN LATERAL (
    SELECT rawpr.result WHERE rawpr.total_games>0 AND rawpr.completed_games=rawpr.total_games
    AND NOT m.play_assisted AND (m.play_mode IS NULL OR (m.play_mode='competition' AND (ps.id IS NULL OR (ps.state->>'phase'='match_over' OR (m.match_length=0 AND ps.state->>'phase'='game_over')))))
    AND rawpr.result->>'total_games'=rawpr.total_games::text
    AND rawpr.result->>'completed_games'=rawpr.total_games::text
    AND rawpr.result->>'analyzed_moves'=rawpr.result->>'total_moves'
    AND rawpr.result #>> '{players,0,unavailable}'='0'
    AND rawpr.result #>> '{players,1,unavailable}'='0'
    AND rawpr.result #>> '{performance_rating,method}'='gnubg_emg_500_v1'
    AND rawpr.result #>> '{performance_rating,games}'=rawpr.total_games::text
    AND rawpr.result #>> '{performance_rating,profile}'=(rawpr.result->>'engine')||'; '||(rawpr.result->>'plies')||'-ply cubeful deterministic'
    AND rawpr.result #>> '{performance_rating,players,0,player}'='0'
    AND rawpr.result #>> '{performance_rating,players,1,player}'='1'
) prj ON true
LEFT JOIN (
    SELECT j.match_uuid,j.source_revision,
           CASE WHEN bool_or(j.status='running') THEN 'running' ELSE 'queued' END AS status
    FROM jobs j
    WHERE j.kind='match' AND j.game_no IS NULL AND j.status IN ('queued','running')
    GROUP BY j.match_uuid,j.source_revision
) active_match_job ON active_match_job.match_uuid=m.id AND active_match_job.source_revision=m.source_revision
    AND NOT m.collection_only
    AND (ps.id IS NULL OR coalesce(ps.state->>'play_mode','competition')<>'competition'
         OR ps.state->>'phase'='match_over' OR (m.match_length=0 AND ps.state->>'phase'='game_over'))
LEFT JOIN LATERAL (
    SELECT coalesce(
        active_match_job.status,
        CASE WHEN selected.source='imported' THEN 'imported'
             WHEN selected_job.status='completed' AND (selected_job.kind='analysis' OR selected_job.game_no IS NOT NULL) THEN 'partial'
             ELSE selected_job.status END,
        (SELECT CASE WHEN c.source='imported' THEN 'imported'
                     WHEN c.scope<>'match' AND c.status='completed' THEN 'partial'
                     ELSE c.status END
         FROM analysis_catalog_sources c WHERE c.match_id=m.id
         ORDER BY (c.status='completed') DESC,c.created_at DESC,c.analysis_id DESC LIMIT 1)
    ) AS status
) analysis ON true";
const MATCH_COLUMNS: &str = "coalesce((SELECT r.id FROM study_report_snapshots r WHERE r.session_id=ps.id ORDER BY r.game_no DESC,r.created_at DESC LIMIT 1),CASE WHEN ps.state->>'phase'='match_over' AND jsonb_typeof(ps.state->'review')='object' THEN ps.id END) AS study_report_id,ps.id AS play_session_id,ps.paused_from_database_id IS NOT NULL AS play_paused,ps.state->>'phase' AS play_phase,m.play_mode,m.play_assisted,m.id,NOT m.collection_only AS source_available,coalesce(m.source_match_no,m.match_no) AS match_no,m.filename,m.format,m.white,m.black,m.match_length,m.position_count,m.created_at,coalesce(m.source_database_id,m.database_id) AS database_id,coalesce(nullif(m.display_name,''),m.filename) AS display_name,m.played_on,m.tournament,m.round,m.place,m.result_state,m.white_score,m.black_score,(prj.result #>> '{performance_rating,players,0,value}')::float8 AS white_pr,(prj.result #>> '{performance_rating,players,1,value}')::float8 AS black_pr,prj.result #>> '{performance_rating,method}' AS pr_method,prj.result #>> '{performance_rating,profile}' AS pr_profile,analysis.status AS analysis_status,(SELECT count(*) FROM match_games g WHERE g.match_uuid=m.id) AS game_count,coalesce(a.tags,'{}'::text[]) AS tags,coalesce(a.comment,'') AS comment,m.metadata_version AS version";
fn sort_spec(sort: &str) -> Option<(&'static str, &'static str)> {
    Some(match sort {
        "occurrence_database" => ("lower(occurrence_db.name)", "text"),
        "occurrence_game" => ("hit.game_no", "integer"),
        "occurrence_move" => ("hit.ply", "integer"),
        "id" | "match_no" => ("m.match_no", "bigint"),
        "created_at" => ("m.created_at", "timestamptz"),
        "played_at" => ("m.played_on", "date"),
        "name" => (
            "lower(coalesce(nullif(m.display_name,''),m.filename))",
            "text",
        ),
        "white" => ("lower(m.white)", "text"),
        "black" => ("lower(m.black)", "text"),
        "match_length" => ("m.match_length", "integer"),
        "tournament" => ("lower(nullif(m.tournament,''))", "text"),
        "format" => ("lower(m.format)", "text"),
        "position_count" => ("m.position_count", "integer"),
        "analysis_status" => ("analysis.status", "text"),
        "tags" => ("lower(nullif(array_to_string(a.tags,', '),''))", "text"),
        "comment" => ("lower(nullif(a.comment,''))", "text"),
        "score" => (
            "CASE WHEN m.white_score IS NOT NULL AND m.black_score IS NOT NULL THEN ARRAY[m.white_score,m.black_score] END",
            "integer[]",
        ),
        "result" => ("m.result_state", "text"),
        "white_pr" => (
            "(prj.result #>> '{performance_rating,players,0,value}')::float8",
            "float8",
        ),
        "black_pr" => (
            "(prj.result #>> '{performance_rating,players,1,value}')::float8",
            "float8",
        ),
        _ => return None,
    })
}
fn bounded_text(value: &Option<String>) -> ApiResult<()> {
    if value.as_ref().is_some_and(|v| v.chars().count() > 240) {
        return Err(ApiError::invalid(
            "Filter text is limited to 240 characters",
        ));
    }
    Ok(())
}
fn valid_score_key(key: &str) -> bool {
    let Some(values) = key.strip_prefix('{').and_then(|v| v.strip_suffix('}')) else {
        return false;
    };
    let values: Vec<_> = values.split(',').collect();
    values.len() == 2
        && values
            .iter()
            .all(|v| v.parse::<i32>().is_ok_and(|n| n >= 0))
}
fn like(q: &mut QueryBuilder<'_, Postgres>, expr: &str, value: &Option<String>) {
    if let Some(v) = value.as_deref().filter(|v| !v.trim().is_empty()) {
        q.push(" AND ")
            .push(expr)
            .push(" LIKE ")
            .push_bind(literal_pattern(v.trim()));
    }
}
const PLAYER_MATCH_FROM: &str = "FROM match_players mp JOIN player_aliases a ON a.id=mp.alias_id WHERE mp.match_id=m.id AND a.profile_id=";

fn library_filters(
    q: &mut QueryBuilder<'_, Postgres>,
    p: &LibraryQuery,
    date_field: &str,
) -> ApiResult<()> {
    if let Some(id) = p.database_id {
        q.push(" AND m.database_id=").push_bind(id);
    }
    if let Some(v) = p.q.as_deref().filter(|v| !v.trim().is_empty()) {
        let pattern = literal_pattern(v.trim());
        q.push(" AND m.id IN (SELECT id FROM matches WHERE search_text LIKE ").push_bind(pattern.clone()).push(" OR lower(display_name) LIKE ").push_bind(pattern.clone()).push(" OR lower(tournament) LIKE ").push_bind(pattern.clone()).push(" OR lower(round) LIKE ").push_bind(pattern.clone()).push(" UNION SELECT coalesce(n.match_uuid,np.match_uuid,ng.match_uuid,nm.match_uuid) FROM annotations n LEFT JOIN positions np ON np.id=n.position_uuid LEFT JOIN match_games ng ON ng.id=n.game_uuid LEFT JOIN match_moves nm ON nm.id=n.move_uuid WHERE n.search_text LIKE ").push_bind(pattern).push(")");
    }
    if let Some(id) = p.player_id {
        q.push(" AND EXISTS(SELECT 1 ")
            .push(PLAYER_MATCH_FROM)
            .push_bind(id)
            .push(")");
    }
    if let Some(v) = p.player.as_deref().filter(|v| !v.trim().is_empty()) {
        let pattern = literal_pattern(v.trim());
        q.push(" AND (lower(m.white) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR lower(m.black) LIKE ")
            .push_bind(pattern)
            .push(")");
    }
    if date_field == "played" {
        if let Some(v) = p.date_from {
            q.push(" AND m.played_on >= ").push_bind(v);
        }
        if let Some(v) = p.date_to {
            q.push(" AND m.played_on <= ").push_bind(v);
        }
    } else {
        if let Some(v) = p.date_from {
            q.push(" AND m.created_at >= ")
                .push_bind(v.and_hms_opt(0, 0, 0).unwrap().and_utc());
        }
        if let Some(v) = p.date_to {
            let end = v
                .succ_opt()
                .ok_or_else(|| ApiError::invalid("date_to is out of range"))?;
            q.push(" AND m.created_at < ")
                .push_bind(end.and_hms_opt(0, 0, 0).unwrap().and_utc());
        }
    }
    like(q, "lower(m.tournament)", &p.tournament);
    like(q, "lower(m.round)", &p.round);
    like(
        q,
        "lower(coalesce(nullif(m.display_name,''),m.filename))",
        &p.match_name,
    );
    if let Some(v) = p.tag.as_ref() {
        let tag = normalize_tag(v)?;
        q.push(" AND EXISTS(SELECT 1 FROM annotations n LEFT JOIN positions np ON np.id=n.position_uuid LEFT JOIN match_games ng ON ng.id=n.game_uuid LEFT JOIN match_moves nm ON nm.id=n.move_uuid WHERE coalesce(n.match_uuid,np.match_uuid,ng.match_uuid,nm.match_uuid)=m.id AND n.tags @> ").push_bind(vec![tag]).push(")");
    }
    if let Some(v) = p.comment.as_ref() {
        q.push(" AND EXISTS(SELECT 1 FROM annotations n LEFT JOIN positions np ON np.id=n.position_uuid LEFT JOIN match_games ng ON ng.id=n.game_uuid LEFT JOIN match_moves nm ON nm.id=n.move_uuid WHERE coalesce(n.match_uuid,np.match_uuid,ng.match_uuid,nm.match_uuid)=m.id AND lower(n.comment) LIKE ").push_bind(literal_pattern(v)).push(")");
    }
    if let Some(v) = &p.position_id {
        q.push(" AND (ps.id IS NULL OR coalesce(ps.state->>'play_mode','competition')<>'competition' OR ps.state->>'phase'='match_over' OR (m.match_length=0 AND ps.state->>'phase'='game_over')) AND hit.position_id=")
            .push_bind(v.clone());
    }
    if let Some(v) = &p.match_type {
        q.push(if v == "money" {
            " AND m.match_length=0"
        } else {
            " AND m.match_length>0"
        });
    }
    if let Some(v) = &p.result {
        match v.as_str() {
            "white_won" | "black_won" | "draw" => {
                q.push(" AND m.result_state='completed' AND m.white_score IS NOT NULL AND m.black_score IS NOT NULL AND m.white_score")
                    .push(match v.as_str() {"white_won"=>" > ","black_won"=>" < ",_=>" = "})
                    .push("m.black_score");
            }
            _ => {
                q.push(" AND m.result_state=").push_bind(v.clone());
            }
        }
    }
    if let Some(v) = p.match_points_min {
        q.push(" AND m.match_length>=").push_bind(v);
    }
    if let Some(v) = p.match_points_max {
        q.push(" AND m.match_length<=").push_bind(v);
    }
    Ok(())
}
#[utoipa::path(get,path="/api/library/matches",params(LibraryQuery),responses((status=200,body=LibraryPage),(status=422,body=ErrorBody)),tag="Databases")]
async fn library_matches(
    State(s): State<AppState>,
    q: Result<Query<LibraryQuery>, QueryRejection>,
) -> ApiResult<Json<LibraryPage>> {
    Ok(Json(browse(&s.pool, query(q)?).await?))
}
fn occurrence_after(
    q: &mut QueryBuilder<'_, Postgres>,
    id: Uuid,
    occurrence: Option<&OccurrenceCursor>,
) {
    if let Some(o) = occurrence {
        q.push("(m.id,hit.ordinal,hit.id) > (")
            .push_bind(id)
            .push(",")
            .push_bind(o.ordinal)
            .push(",")
            .push_bind(o.id)
            .push(")");
    } else {
        q.push("m.id > ").push_bind(id);
    }
}

fn library_row_projection(q: &mut QueryBuilder<'_, Postgres>, player_id: Option<Uuid>) {
    q.push("SELECT ").push(MATCH_COLUMNS);
    if let Some(id) = player_id {
        q.push(",ARRAY(SELECT mp.side ")
            .push(PLAYER_MATCH_FROM)
            .push_bind(id)
            .push(" ORDER BY mp.side) AS player_sides");
    }
}

pub async fn browse(pool: &PgPool, mut p: LibraryQuery) -> ApiResult<LibraryPage> {
    for v in [
        &p.q,
        &p.player,
        &p.tag,
        &p.comment,
        &p.position_id,
        &p.tournament,
        &p.round,
        &p.match_name,
    ] {
        bounded_text(v)?
    }
    let sort = p.sort.clone().unwrap_or("created_at".into());
    if p.position_id.is_none()
        && ["occurrence_database", "occurrence_game", "occurrence_move"].contains(&sort.as_str())
    {
        return Err(ApiError::invalid(
            "Occurrence sorting requires a position filter",
        ));
    }
    let (expr, cast) = sort_spec(&sort).ok_or_else(|| ApiError::invalid("Unknown sort field"))?;
    let direction = p.direction.clone().unwrap_or("desc".into());
    if !["asc", "desc"].contains(&direction.as_str()) {
        return Err(ApiError::invalid("direction must be asc or desc"));
    }
    let date_field = p.date_field.clone().unwrap_or("played".into());
    if !["played", "imported"].contains(&date_field.as_str()) {
        return Err(ApiError::invalid("date_field must be played or imported"));
    }
    if p.date_from.zip(p.date_to).is_some_and(|(a, b)| a > b) {
        return Err(ApiError::invalid("date_from must not exceed date_to"));
    }
    if p.match_points_min.is_some_and(|v| v < 0)
        || p.match_points_max.is_some_and(|v| v < 0)
        || p.match_points_min
            .zip(p.match_points_max)
            .is_some_and(|(a, b)| a > b)
    {
        return Err(ApiError::invalid("Invalid match points range"));
    }
    if p.match_type
        .as_deref()
        .is_some_and(|v| !["match", "money"].contains(&v))
    {
        return Err(ApiError::invalid("match_type must be match or money"));
    }
    if p.result.as_deref().is_some_and(|v| {
        ![
            "completed",
            "ongoing",
            "unknown",
            "white_won",
            "black_won",
            "draw",
        ]
        .contains(&v)
    }) {
        return Err(ApiError::invalid("Invalid result state"));
    }
    let limit = p.limit.unwrap_or(50);
    if !(1..=200).contains(&limit) {
        return Err(ApiError::invalid("limit must be between 1 and 200"));
    }
    let after = p.after.take();
    let offset = crate::search::page_offset(p.page.take(), after.is_some(), limit)?;
    p.limit = None;
    let scope = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&p).map_err(anyhow::Error::from)?)
    );
    let occurrences = p.position_id.is_some();
    let occurrence_join = if occurrences {
        " JOIN positions hit ON hit.match_uuid=m.id JOIN databases occurrence_db ON occurrence_db.id=m.database_id"
    } else {
        ""
    };
    let occurrence_columns = if occurrences {
        ",hit.id AS occurrence_id,hit.ordinal AS occurrence_ordinal,hit.game_no AS occurrence_game_no,hit.ply AS occurrence_move_no,occurrence_db.name AS occurrence_database_name"
    } else {
        ""
    };
    let projected_occurrence_columns = if occurrences {
        ",ranked.occurrence_id,ranked.occurrence_ordinal,ranked.occurrence_game_no,ranked.occurrence_move_no,ranked.occurrence_database_name"
    } else {
        ""
    };
    let compact = ["tags", "comment"].contains(&sort.as_str());
    let sort_uses_analysis = ["white_pr", "black_pr", "analysis_status"].contains(&sort.as_str());
    let mut cursor = after
        .map(|v| {
            if v.len() > 4096 {
                return Err(ApiError::invalid("Cursor is too long"));
            }
            URL_SAFE_NO_PAD
                .decode(v)
                .ok()
                .and_then(|b| serde_json::from_slice::<LibraryCursor>(&b).ok())
                .filter(|c| c.scope == scope)
                .ok_or_else(|| {
                    ApiError::invalid("Cursor is invalid or belongs to different filters")
                })
        })
        .transpose()?;
    if let Some(c) = &mut cursor {
        if c.anchor != compact
            || (c.anchor && c.key.is_some())
            || c.occurrence.is_some() != occurrences
            || c.occurrence.as_ref().is_some_and(|o| o.ordinal < 0)
        {
            return Err(ApiError::invalid("Cursor contains an invalid anchor"));
        }
        if c.anchor {
            // Use the same filters as the page, so a deleted anchor or one that
            // has left the current scope produces a validation error.
            let play_join = if occurrences { MATCH_PLAY_JOIN } else { "" };
            let mut anchor = QueryBuilder::<Postgres>::new(format!(
                "SELECT ({expr})::text AS cursor_key FROM matches m{MATCH_NOTE_JOIN}{play_join}{occurrence_join} WHERE NOT m.collection_only"
            ));
            library_filters(&mut anchor, &p, &date_field)?;
            anchor.push(" AND m.id=").push_bind(c.id);
            if let Some(o) = &c.occurrence {
                anchor
                    .push(" AND hit.id=")
                    .push_bind(o.id)
                    .push(" AND hit.ordinal=")
                    .push_bind(o.ordinal);
            }
            let row = anchor
                .build()
                .fetch_optional(pool)
                .await
                .map_err(anyhow::Error::from)?
                .ok_or_else(|| {
                    ApiError::invalid("Cursor anchor is missing from the current filters")
                })?;
            c.key = row.try_get("cursor_key").map_err(anyhow::Error::from)?;
        }
        let valid = match c.key.as_deref() {
            None => [
                "played_at",
                "white_pr",
                "black_pr",
                "tournament",
                "analysis_status",
                "tags",
                "comment",
                "score",
            ]
            .contains(&sort.as_str()),
            Some(key) => match cast {
                "integer" => key.parse::<i32>().is_ok(),
                "bigint" => key.parse::<i64>().is_ok(),
                "integer[]" => valid_score_key(key),
                "float8" => key.parse::<f64>().is_ok_and(f64::is_finite),
                "date" => NaiveDate::parse_from_str(key, "%Y-%m-%d").is_ok(),
                "timestamptz" => DateTime::parse_from_str(key, "%Y-%m-%d %H:%M:%S%.f%#z").is_ok(),
                _ => c.anchor || key.chars().count() <= 2048,
            },
        };
        if !valid {
            return Err(ApiError::invalid("Cursor contains an invalid sort value"));
        }
    }
    let mut q = QueryBuilder::<Postgres>::new("WITH ");
    if offset.is_some() {
        q.push("totals AS (SELECT count(*) AS total FROM matches m");
        if occurrences {
            q.push(MATCH_PLAY_JOIN).push(occurrence_join);
        }
        q.push(" WHERE NOT m.collection_only");
        library_filters(&mut q, &p, &date_field)?;
        q.push("), ");
    }
    if sort_uses_analysis {
        q.push("page AS (");
        library_row_projection(&mut q, p.player_id);
    } else {
        // Sort and limit narrow rows before fetching notes, games and analysis.
        q.push("ranked AS MATERIALIZED (SELECT m.id");
    }
    q.push(occurrence_columns).push(format!(
        ",({expr})::text AS cursor_key,({expr}) AS page_sort FROM matches m"
    ));
    if occurrences || sort_uses_analysis {
        q.push(MATCH_PLAY_JOIN);
    }
    if compact || sort_uses_analysis {
        q.push(MATCH_NOTE_JOIN);
    }
    if sort_uses_analysis {
        q.push(MATCH_ANALYSIS_JOINS);
    }
    q.push(occurrence_join).push(" WHERE NOT m.collection_only");
    library_filters(&mut q, &p, &date_field)?;
    if let Some(c) = cursor {
        q.push(" AND (");
        if let Some(key) = c.key {
            q.push(expr)
                .push(if direction == "asc" { " > " } else { " < " })
                .push_bind(key.clone())
                .push("::")
                .push(cast)
                .push(" OR ")
                .push(expr)
                .push(" IS NULL OR (")
                .push(expr)
                .push(" = ")
                .push_bind(key)
                .push("::")
                .push(cast)
                .push(" AND ");
            occurrence_after(&mut q, c.id, c.occurrence.as_ref());
            q.push("))");
        } else {
            q.push(expr).push(" IS NULL AND ");
            occurrence_after(&mut q, c.id, c.occurrence.as_ref());
            q.push(")");
        }
    }
    let sort_order = if direction == "asc" {
        " ASC NULLS LAST"
    } else {
        " DESC NULLS LAST"
    };
    q.push(" ORDER BY ")
        .push(expr)
        .push(sort_order)
        .push(",m.id ASC");
    if occurrences {
        q.push(",hit.ordinal ASC,hit.id ASC");
    }
    q.push(" LIMIT ").push_bind(limit + 1);
    if let Some(offset) = offset {
        q.push(" OFFSET ").push_bind(offset);
    }
    q.push(")");
    if !sort_uses_analysis {
        q.push(", page AS (");
        library_row_projection(&mut q, p.player_id);
        q.push(",ranked.cursor_key,ranked.page_sort")
            .push(projected_occurrence_columns)
            .push(" FROM ranked JOIN matches m ON m.id=ranked.id")
            .push(MATCH_PLAY_JOIN)
            .push(MATCH_NOTE_JOIN)
            .push(MATCH_ANALYSIS_JOINS)
            .push(")");
    }
    if offset.is_some() {
        q.push(" SELECT page.*,totals.total FROM totals LEFT JOIN page ON true");
    } else {
        q.push(" SELECT page.* FROM page");
    }
    q.push(" ORDER BY page_sort")
        .push(sort_order)
        .push(",id ASC");
    if occurrences {
        q.push(",occurrence_ordinal ASC,occurrence_id ASC");
    }
    let mut rows = q
        .build()
        .fetch_all(pool)
        .await
        .map_err(anyhow::Error::from)?;
    let total = if offset.is_some() {
        let total = rows[0].get::<i64, _>("total");
        rows.retain(|row| row.get::<Option<Uuid>, _>("id").is_some());
        Some(total)
    } else {
        None
    };
    let more = rows.len() > limit as usize;
    let mut items = Vec::new();
    let mut last = None;
    for row in rows.iter().take(limit as usize) {
        let mut m = <DatabaseMatch as sqlx::FromRow<sqlx::postgres::PgRow>>::from_row(row)
            .map_err(anyhow::Error::from)?;
        if p.player_id.is_some() {
            m.player_sides = Some(row.try_get("player_sides").map_err(anyhow::Error::from)?);
        }
        let occurrence = if occurrences {
            let id = row.try_get("occurrence_id").map_err(anyhow::Error::from)?;
            m.occurrence = Some(LibraryOccurrence {
                id,
                game_no: row
                    .try_get("occurrence_game_no")
                    .map_err(anyhow::Error::from)?,
                move_no: row
                    .try_get("occurrence_move_no")
                    .map_err(anyhow::Error::from)?,
                database_name: row
                    .try_get("occurrence_database_name")
                    .map_err(anyhow::Error::from)?,
            });
            Some(OccurrenceCursor {
                id,
                ordinal: row
                    .try_get("occurrence_ordinal")
                    .map_err(anyhow::Error::from)?,
            })
        } else {
            None
        };
        last = Some(LibraryCursor {
            occurrence,
            scope: scope.clone(),
            key: if compact {
                None
            } else {
                row.try_get("cursor_key").map_err(anyhow::Error::from)?
            },
            id: m.id,
            anchor: compact,
        });
        items.push(m);
    }
    let next_cursor = if more {
        last.map(|c| serde_json::to_vec(&c).map(|v| URL_SAFE_NO_PAD.encode(v)))
            .transpose()
            .map_err(anyhow::Error::from)?
    } else {
        None
    };
    Ok(LibraryPage {
        items,
        next_cursor,
        total,
    })
}
async fn database_match(pool: &PgPool, id: Uuid) -> ApiResult<DatabaseMatch> {
    sqlx::query_as(&format!(
        "SELECT {MATCH_COLUMNS} FROM matches m{MATCH_PLAY_JOIN}{MATCH_NOTE_JOIN}{MATCH_ANALYSIS_JOINS} WHERE m.id=$1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(anyhow::Error::from)?
    .ok_or_else(ApiError::missing)
}
#[utoipa::path(get,path="/api/matches/{id}/metadata",params(("id"=Uuid,Path)),responses((status=200,body=DatabaseMatch),(status=404,body=ErrorBody)),tag="Databases")]
async fn get_metadata(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<DatabaseMatch>> {
    Ok(Json(database_match(&s.pool, id).await?))
}
#[utoipa::path(put,path="/api/matches/{id}/metadata",params(("id"=Uuid,Path)),request_body=MetadataWrite,responses((status=200,body=DatabaseMatch),(status=409,body=ErrorBody)),tag="Databases")]
async fn update_metadata(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<Json<MetadataWrite>, JsonRejection>,
) -> ApiResult<Json<DatabaseMatch>> {
    let w = json(body)?;
    if w.version < 0
        || w.display_name.chars().count() > 240
        || w.tournament
            .as_ref()
            .is_some_and(|v| v.chars().count() > 240)
        || w.round.as_ref().is_some_and(|v| v.chars().count() > 120)
        || w.place.as_ref().is_some_and(|v| v.chars().count() > 240)
        || !["unknown", "ongoing", "completed"].contains(&w.result_state.as_str())
        || w.white_score.is_some_and(|v| v < 0)
        || w.black_score.is_some_and(|v| v < 0)
    {
        return Err(ApiError::invalid("Invalid match metadata"));
    }
    let affected=sqlx::query("UPDATE matches SET display_name=$2,played_on=$3,tournament=$4,round=$5,place=$6,result_state=$7,white_score=$8,black_score=$9,metadata_version=metadata_version+1,source_metadata_loaded=true,native_match_context_pending=false WHERE id=$1 AND metadata_version=$10 AND NOT collection_only").bind(id).bind(w.display_name.trim()).bind(w.played_on).bind(w.tournament).bind(w.round).bind(w.place).bind(w.result_state).bind(w.white_score).bind(w.black_score).bind(w.version).execute(&s.pool).await.map_err(anyhow::Error::from)?.rows_affected();
    if affected == 0 {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM matches WHERE id=$1 AND NOT collection_only)",
        )
        .bind(id)
        .fetch_one(&s.pool)
        .await
        .map_err(anyhow::Error::from)?;
        return Err(if exists {
            conflict("Metadata changed; reload before saving")
        } else {
            ApiError::missing()
        });
    }
    Ok(Json(database_match(&s.pool, id).await?))
}

async fn any_active(tx: &mut sqlx::Transaction<'_, Postgres>, ids: &[Uuid]) -> ApiResult<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM jobs j LEFT JOIN positions p ON p.id=j.position_uuid WHERE coalesce(j.match_uuid,p.match_uuid)=ANY($1) AND j.status IN ('queued','running'))").bind(ids).fetch_one(&mut **tx).await.map_err(anyhow::Error::from)?)
}
#[utoipa::path(post,path="/api/library/actions",request_body=LibraryAction,responses((status=200,body=ActionResponse),(status=409,body=ErrorBody,description="Target is reserved for paused sessions"),(status=422,body=ErrorBody)),tag="Databases")]
async fn actions(
    State(s): State<AppState>,
    body: Result<Json<LibraryAction>, JsonRejection>,
) -> ApiResult<Json<ActionResponse>> {
    let mut a = json(body)?;
    if a.match_ids.is_empty() || a.match_ids.len() > MAX_ACTIONS {
        return Err(ApiError::invalid("Select between 1 and 100 matches"));
    }
    if !["copy", "move", "delete", "tags"].contains(&a.action.as_str()) {
        return Err(ApiError::invalid("Unknown library action"));
    }
    if ["copy", "move"].contains(&a.action.as_str()) {
        let db = a
            .target_database_id
            .ok_or_else(|| ApiError::invalid("target_database_id is required"))?;
        require_library_destination(&s.pool, db).await?;
    }
    a.add_tags = a
        .add_tags
        .iter()
        .map(|s| normalize_tag(s))
        .collect::<ApiResult<Vec<_>>>()?
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    a.remove_tags = a
        .remove_tags
        .iter()
        .map(|s| normalize_tag(s))
        .collect::<ApiResult<Vec<_>>>()?;
    let mut seen = HashSet::new();
    let mut results = Vec::new();
    for &id in &a.match_ids {
        if !seen.insert(id) {
            results.push(action_item(id, "duplicate_request", None, None));
            continue;
        }
        results.push(action_one(&s.pool, &a, id).await?)
    }
    Ok(Json(ActionResponse { results }))
}
fn action_item(
    id: Uuid,
    status: &str,
    created: Option<Uuid>,
    existing: Option<Uuid>,
) -> ActionItem {
    ActionItem {
        match_id: id,
        status: status.into(),
        created_id: created,
        existing_id: existing,
        error: None,
    }
}
async fn action_one(pool: &PgPool, a: &LibraryAction, id: Uuid) -> ApiResult<ActionItem> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    // Lock database ownership before the source; deletion uses the same order.
    let source_db: Option<Uuid> =
        sqlx::query_scalar("SELECT database_id FROM matches WHERE id=$1 AND NOT collection_only")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    let Some(source_db) = source_db else {
        return Ok(action_item(id, "not_found", None, None));
    };
    let mut lock_dbs = vec![source_db];
    if let Some(target) = a.target_database_id {
        lock_dbs.push(target)
    }
    lock_dbs.sort();
    lock_dbs.dedup();
    sqlx::query("SELECT id FROM databases WHERE id=ANY($1) ORDER BY id FOR KEY SHARE")
        .bind(&lock_dbs)
        .fetch_all(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let source: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT database_id,sha256 FROM matches WHERE id=$1 AND NOT collection_only FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let Some((source_db, hash)) = source else {
        return Ok(action_item(id, "not_found", None, None));
    };
    if a.database_id.is_some_and(|scope| scope != source_db) {
        return Ok(action_item(id, "wrong_database", None, None));
    }
    if source_db == PAUSED_MATCHES_DATABASE_ID && a.action != "tags" {
        return Ok(ActionItem {
            error: Some("Resume the paused match before copying, moving or deleting it".into()),
            ..action_item(id, "invalid", None, None)
        });
    }
    let item = match a.action.as_str() {
        "copy" | "move" => {
            let target = a.target_database_id.unwrap();
            if source_db == target {
                return Ok(action_item(id, "same_database", None, Some(id)));
            }
            // The unique constraint is the final arbiter for concurrent imports/copies.
            let existing: Option<Uuid> =
                sqlx::query_scalar("SELECT id FROM matches WHERE database_id=$1 AND sha256=$2")
                    .bind(target)
                    .bind(&hash)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(anyhow::Error::from)?;
            if let Some(existing) = existing {
                return Ok(action_item(id, "duplicate", None, Some(existing)));
            }
            if a.action == "move" {
                let moved = sqlx::query("UPDATE matches SET database_id=$2 WHERE id=$1")
                    .bind(id)
                    .bind(target)
                    .execute(&mut *tx)
                    .await;
                match moved {
                    Ok(_) => action_item(id, "moved", None, None),
                    Err(e)
                        if e.as_database_error()
                            .is_some_and(|e| e.is_unique_violation()) =>
                    {
                        tx.rollback().await.map_err(anyhow::Error::from)?;
                        let existing = sqlx::query_scalar(
                            "SELECT id FROM matches WHERE database_id=$1 AND sha256=$2",
                        )
                        .bind(target)
                        .bind(hash)
                        .fetch_optional(pool)
                        .await
                        .map_err(anyhow::Error::from)?;
                        return Ok(action_item(id, "duplicate", None, existing));
                    }
                    Err(e) => return Err(anyhow::Error::from(e).into()),
                }
            } else {
                let new_id = Uuid::new_v4();
                let inserted=sqlx::query_scalar::<_,Uuid>("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf,created_at,database_id,display_name,played_on,tournament,round,place,result_state,white_score,black_score,metadata_version,moves_cataloged,source_metadata_loaded,native_match_context_pending,source_revision,play_mode,play_assisted,player_source_platform) SELECT $2,filename,format,sha256,white,black,match_length,position_count,original,sgf,created_at,$3,display_name,played_on,tournament,round,place,result_state,white_score,black_score,metadata_version,moves_cataloged,source_metadata_loaded,native_match_context_pending,source_revision,play_mode,play_assisted,player_source_platform FROM matches WHERE id=$1 ON CONFLICT(database_id,sha256) DO NOTHING RETURNING id").bind(id).bind(new_id).bind(target).fetch_optional(&mut *tx).await.map_err(anyhow::Error::from)?;
                if inserted.is_none() {
                    let existing = sqlx::query_scalar(
                        "SELECT id FROM matches WHERE database_id=$1 AND sha256=$2",
                    )
                    .bind(target)
                    .bind(hash)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(anyhow::Error::from)?;
                    return Ok(action_item(id, "duplicate", None, existing));
                }
                copy_children(&mut tx, id, new_id).await?;
                action_item(id, "copied", Some(new_id), None)
            }
        }
        "delete" => {
            if any_active(&mut tx, &[id]).await? {
                return Ok(action_item(id, "busy", None, None));
            }
            crate::position_retention::preserve(&mut tx, id).await?;
            sqlx::query("DELETE FROM matches WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(anyhow::Error::from)?;
            action_item(id, "deleted", None, None)
        }
        "tags" => {
            let old: Option<(Uuid, Vec<String>)> =
                sqlx::query_as("SELECT id,tags FROM annotations WHERE match_uuid=$1 FOR UPDATE")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(anyhow::Error::from)?;
            let mut tags = old
                .as_ref()
                .map(|(_, tags)| tags.iter().cloned().collect::<BTreeSet<_>>())
                .unwrap_or_default();
            for tag in &a.remove_tags {
                tags.remove(tag);
            }
            tags.extend(a.add_tags.iter().cloned());
            if tags.len() > 30 {
                return Ok(ActionItem {
                    error: Some("At most 30 tags per match".into()),
                    ..action_item(id, "invalid", None, None)
                });
            }
            let tags = tags.into_iter().collect::<Vec<_>>();
            if let Some((note_id, _)) = old {
                sqlx::query(
                    "UPDATE annotations SET tags=$2,version=version+1,updated_at=now() WHERE id=$1",
                )
                .bind(note_id)
                .bind(tags)
                .execute(&mut *tx)
                .await
                .map_err(anyhow::Error::from)?;
            } else {
                sqlx::query(
                    "INSERT INTO annotations(id,match_uuid,tags,version) VALUES($1,$2,$3,1)",
                )
                .bind(Uuid::new_v4())
                .bind(id)
                .bind(tags)
                .execute(&mut *tx)
                .await
                .map_err(anyhow::Error::from)?;
            }
            action_item(id, "tagged", None, None)
        }
        _ => unreachable!(),
    };
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(item)
}
async fn copy_children(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    source: Uuid,
    target: Uuid,
) -> ApiResult<()> {
    let position_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 ORDER BY ordinal")
            .bind(source)
            .fetch_all(&mut **tx)
            .await
            .map_err(anyhow::Error::from)?;
    let mut positions = HashMap::new();
    for old in position_ids {
        let id = Uuid::new_v4();
        positions.insert(old, id);
        sqlx::query("INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation) SELECT $2,$3,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation FROM positions WHERE id=$1").bind(old).bind(id).bind(target).execute(&mut **tx).await.map_err(anyhow::Error::from)?;
    }
    let mut games = HashMap::new();
    let rows: Vec<(Uuid, i32)> =
        sqlx::query_as("SELECT id,game_no FROM match_games WHERE match_uuid=$1")
            .bind(source)
            .fetch_all(&mut **tx)
            .await
            .map_err(anyhow::Error::from)?;
    for (old, _) in rows {
        let id = Uuid::new_v4();
        games.insert(old, id);
        sqlx::query("INSERT INTO match_games(id,match_uuid,game_no,position_count,white_score,black_score,crawford) SELECT $2,$3,game_no,position_count,white_score,black_score,crawford FROM match_games WHERE id=$1").bind(old).bind(id).bind(target).execute(&mut **tx).await.map_err(anyhow::Error::from)?;
    }
    crate::imported_analysis::copy_records(tx, source, target, &positions).await?;
    let mut moves = HashMap::new();
    let rows: Vec<(Uuid, Option<Uuid>)> = sqlx::query_as(
        "SELECT id,position_uuid FROM match_moves WHERE match_uuid=$1 ORDER BY game_no,move_no",
    )
    .bind(source)
    .fetch_all(&mut **tx)
    .await
    .map_err(anyhow::Error::from)?;
    for (old, position) in rows {
        let id = Uuid::new_v4();
        moves.insert(old, id);
        sqlx::query("INSERT INTO match_moves(id,match_uuid,game_no,move_no,kind,player,notation,position_uuid,board,dice,position_id,gnubg_match_id) SELECT $2,$3,game_no,move_no,kind,player,notation,$4,board,dice,position_id,gnubg_match_id FROM match_moves WHERE id=$1").bind(old).bind(id).bind(target).bind(position.and_then(|id|positions.get(&id).copied())).execute(&mut **tx).await.map_err(anyhow::Error::from)?;
    }
    type AnnotationTargets = (Uuid, Option<Uuid>, Option<Uuid>, Option<Uuid>, Option<Uuid>);
    let annotations:Vec<AnnotationTargets>=sqlx::query_as("SELECT a.id,a.match_uuid,a.position_uuid,a.game_uuid,a.move_uuid FROM annotations a LEFT JOIN positions p ON p.id=a.position_uuid LEFT JOIN match_games g ON g.id=a.game_uuid LEFT JOIN match_moves m ON m.id=a.move_uuid WHERE coalesce(a.match_uuid,p.match_uuid,g.match_uuid,m.match_uuid)=$1").bind(source).fetch_all(&mut **tx).await.map_err(anyhow::Error::from)?;
    for (old, mat, pos, game, mov) in annotations {
        sqlx::query("INSERT INTO annotations(id,match_uuid,position_uuid,game_uuid,move_uuid,tags,comment,version,updated_at) SELECT $2,$3,$4,$5,$6,tags,comment,version,updated_at FROM annotations WHERE id=$1").bind(old).bind(Uuid::new_v4()).bind(mat.map(|_|target)).bind(pos.and_then(|id|positions.get(&id).copied())).bind(game.and_then(|id|games.get(&id).copied())).bind(mov.and_then(|id|moves.get(&id).copied())).execute(&mut **tx).await.map_err(anyhow::Error::from)?;
    }
    // Completed snapshots only: queued/running/failed work and learning state stay with the source.
    let jobs:Vec<(Uuid,Option<Uuid>,Option<Uuid>)>=sqlx::query_as("SELECT j.id,j.match_uuid,j.position_uuid FROM jobs j LEFT JOIN positions p ON p.id=j.position_uuid WHERE coalesce(j.match_uuid,p.match_uuid)=$1 AND j.status='completed' ORDER BY j.created_at,j.id").bind(source).fetch_all(&mut **tx).await.map_err(anyhow::Error::from)?;
    let mut copied_jobs = HashMap::new();
    for (old, mat, pos) in jobs {
        let new_id = Uuid::new_v4();
        copied_jobs.insert(old, new_id);
        sqlx::query("INSERT INTO jobs SELECT (jsonb_populate_record(NULL::jobs,to_jsonb(j)||jsonb_build_object('id',$2::uuid,'match_uuid',$3::uuid,'position_uuid',$4::uuid,'lease_token',NULL,'lease_until',NULL))).* FROM jobs j WHERE id=$1").bind(old).bind(new_id).bind(mat.map(|_|target)).bind(pos.and_then(|id|positions.get(&id).copied())).execute(&mut **tx).await.map_err(anyhow::Error::from)?;
        sqlx::query("INSERT INTO job_events(job_id,event,data) SELECT id,'completed',jsonb_build_object('status','completed','progress',progress,'error',NULL,'completed_games',completed_games,'total_games',total_games) FROM jobs WHERE id=$1")
            .bind(new_id).execute(&mut **tx).await.map_err(anyhow::Error::from)?;
        sqlx::query("INSERT INTO match_analysis_games(job_id,game_no,summary) SELECT $2,game_no,summary FROM match_analysis_games WHERE job_id=$1").bind(old).bind(new_id).execute(&mut **tx).await.map_err(anyhow::Error::from)?;
        type CopiedDecision = (Option<Uuid>, i32, i32, i32, String, f64, serde_json::Value);
        let decisions:Vec<CopiedDecision>=sqlx::query_as("SELECT position_uuid,game_no,decision_no,player,kind,equity_loss,data FROM move_analyses WHERE job_id=$1 ORDER BY id").bind(old).fetch_all(&mut **tx).await.map_err(anyhow::Error::from)?;
        for (pos, game, decision, player, kind, loss, data) in decisions {
            sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(new_id).bind(pos.and_then(|id|positions.get(&id).copied())).bind(game).bind(decision).bind(player).bind(kind).bind(loss).bind(data).execute(&mut **tx).await.map_err(anyhow::Error::from)?;
        }
    }
    let selected: Option<(String, i64)> = sqlx::query_as(
        "SELECT analysis_id,source_revision FROM analysis_selections WHERE match_id=$1",
    )
    .bind(source)
    .fetch_optional(&mut **tx)
    .await
    .map_err(anyhow::Error::from)?;
    if let Some((selected, source_revision)) = selected {
        let copied_selection = if selected == format!("imported:{source}") {
            format!("imported:{target}")
        } else {
            selected
                .strip_prefix("job:")
                .and_then(|id| Uuid::parse_str(id).ok())
                .and_then(|id| copied_jobs.get(&id))
                .map(|id| format!("job:{id}"))
                .unwrap_or(selected)
        };
        // Retain stale selection metadata too: copying cannot silently promote
        // an old imported context or switch to another completed result.
        sqlx::query("INSERT INTO analysis_selections(match_id,analysis_id,source_revision) VALUES($1,$2,$3)")
            .bind(target).bind(copied_selection).bind(source_revision)
            .execute(&mut **tx).await.map_err(anyhow::Error::from)?;
    }
    Ok(())
}
#[utoipa::path(post,path="/api/library/export",request_body=ExportRequest,responses((status=200,description="Original/SGF file or ZIP for multiple matches",content_type="application/octet-stream"),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Databases")]
async fn export(
    State(s): State<AppState>,
    body: Result<Json<ExportRequest>, JsonRejection>,
) -> ApiResult<Response> {
    let r = json(body)?;
    if r.match_ids.is_empty() || r.match_ids.len() > MAX_EXPORTS {
        return Err(ApiError::invalid("Select between 1 and 50 matches"));
    }
    if !["original", "sgf"].contains(&r.format.as_str()) {
        return Err(ApiError::invalid("format must be original or sgf"));
    }
    let mut seen = HashSet::new();
    if r.match_ids.iter().any(|id| !seen.insert(*id)) {
        return Err(ApiError::invalid("Duplicate match IDs are not allowed"));
    }
    let mut tx = s.pool.begin().await.map_err(anyhow::Error::from)?;
    let sizes:Vec<(Uuid,i64)>=sqlx::query_as(if r.format=="original"{"SELECT id,octet_length(original)::bigint FROM matches WHERE id=ANY($1) AND NOT collection_only ORDER BY id FOR SHARE"}else{"SELECT id,octet_length(sgf)::bigint FROM matches WHERE id=ANY($1) AND NOT collection_only ORDER BY id FOR SHARE"}).bind(&r.match_ids).fetch_all(&mut *tx).await.map_err(anyhow::Error::from)?;
    if sizes.len() != r.match_ids.len() {
        return Err(ApiError::missing());
    }
    if sizes.iter().map(|(_, n)| n).sum::<i64>() > MAX_EXPORT_BYTES {
        return Err(ApiError::invalid("Export exceeds 64 MiB"));
    }
    let files: Vec<(Uuid, String, Vec<u8>)> = sqlx::query_as(if r.format == "original" {
        "SELECT id,filename,original FROM matches WHERE id=ANY($1) ORDER BY id"
    } else {
        "SELECT id,filename,convert_to(sgf,'UTF8') FROM matches WHERE id=ANY($1) ORDER BY id"
    })
    .bind(&r.match_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    let safe_name = |id: Uuid, name: &str| {
        let name = name.rsplit(['/', '\\']).next().unwrap_or("match");
        let clean: String = name
            .chars()
            .map(|c| {
                if c.is_control() || "\"<>:|?*".contains(c) {
                    '_'
                } else {
                    c
                }
            })
            .take(180)
            .collect();
        let clean = if clean.trim_matches('.').is_empty() {
            "match".to_string()
        } else {
            clean
        };
        if r.format == "sgf" {
            let stem = clean.rsplit_once('.').map(|(s, _)| s).unwrap_or(&clean);
            format!("{stem}-{id}.sgf")
        } else {
            format!("{id}-{clean}")
        }
    };
    let (name, bytes, content_type) = if files.len() == 1 {
        let (id, name, data) = files.into_iter().next().unwrap();
        (safe_name(id, &name), data, "application/octet-stream")
    } else {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (id, name, data) in files {
            zip.start_file(
                safe_name(id, &name),
                zip::write::SimpleFileOptions::default(),
            )
            .map_err(anyhow::Error::from)?;
            zip.write_all(&data).map_err(anyhow::Error::from)?;
        }
        let bytes = zip.finish().map_err(anyhow::Error::from)?.into_inner();
        ("matches.zip".into(), bytes, "application/zip")
    };
    let disposition = format!(
        "attachment; filename=\"{}\"",
        name.chars()
            .map(|c| if c.is_ascii() { c } else { '_' })
            .collect::<String>()
    );
    Ok((
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        bytes,
    )
        .into_response())
}

#[derive(OpenApi)]
#[openapi(paths(list_databases,create_database,update_database,delete_database,clear_database,library_matches,get_metadata,update_metadata,actions,export),components(schemas(Database,DatabaseWrite,DatabaseMatch,LibraryOccurrence,LibraryPage,MetadataWrite,LibraryAction,ActionItem,ActionResponse,ExportRequest)),tags((name="Databases",description="Independent match databases and library management")))]
pub struct DatabaseDoc;

#[derive(Debug)]
pub struct SourceMetadata {
    pub played_on: Option<NaiveDate>,
    pub tournament: Option<String>,
    pub round: Option<String>,
    pub place: Option<String>,
    pub result_state: String,
    pub white_score: Option<i32>,
    pub black_score: Option<i32>,
}
/// Only explicit SGF properties are trusted. A game RE alone is not a match result.
pub fn source_metadata(sgf: &str) -> SourceMetadata {
    let mut out = SourceMetadata {
        played_on: None,
        tournament: None,
        round: None,
        place: None,
        result_state: "unknown".into(),
        white_score: None,
        black_score: None,
    };
    let Ok(trees) = crate::import::sgf::parse_trees(sgf) else {
        return out;
    };
    let Some(first) = trees.first().and_then(|t| t.first()) else {
        return out;
    };
    let prop = |key: &str, limit: usize| {
        first
            .get(key)
            .and_then(|v| v.first())
            .filter(|v| !v.trim().is_empty() && v.chars().count() <= limit)
            .cloned()
    };
    out.played_on = prop("DT", 10)
        .filter(|v| v.len() == 10)
        .and_then(|v| NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok());
    out.tournament = prop("EV", 240);
    out.round = prop("RO", 120);
    out.place = prop("PC", 240);
    let Some(last) = trees.last().and_then(|t| t.first()) else {
        return out;
    };
    let mi = |key: &str| {
        last.get("MI")
            .and_then(|v| v.iter().find_map(|v| v.strip_prefix(key)))
            .and_then(|v| v.parse::<i32>().ok())
            .filter(|v| *v >= 0)
    };
    let (Some(length), Some(mut white), Some(mut black)) = (mi("length:"), mi("ws:"), mi("bs:"))
    else {
        return out;
    };
    if length == 0 {
        return out;
    }
    if let Some(result) = last.get("RE").and_then(|v| v.first()) {
        let Some((side, points)) = result.split_once('+') else {
            return out;
        };
        let points = points.strip_suffix('R').unwrap_or(points);
        let Ok(points) = points.parse::<i32>() else {
            return out;
        };
        if points <= 0 {
            return out;
        }
        match side {
            "W" => {
                let Some(n) = white.checked_add(points) else {
                    return out;
                };
                white = n
            }
            "B" => {
                let Some(n) = black.checked_add(points) else {
                    return out;
                };
                black = n
            }
            _ => return out,
        }
    }
    if prop("AP", 32).as_deref() == Some(crate::import::ogxm::SGF_APPLICATION) {
        // OGXM §5.4.6 caps the match score at the target, even if the
        // final game's award was larger than the remaining points.
        white = white.min(length);
        black = black.min(length);
    }
    if white < length && black < length {
        // A BG Tutor forfeit awards the remaining match points without a game result.
        match prop(crate::import::sgf::MATCH_FORFEIT_PROPERTY, 1).as_deref() {
            Some("W") => black = length,
            Some("B") => white = length,
            _ => {}
        }
    }
    out.white_score = Some(white);
    out.black_score = Some(black);
    out.result_state = if white >= length || black >= length {
        "completed"
    } else {
        "ongoing"
    }
    .into();
    out
}
/// Bounded startup backfill; repeat until it returns zero. Manual metadata is preserved.
pub async fn backfill_source_metadata(pool: &PgPool) -> anyhow::Result<usize> {
    let mut tx = pool.begin().await?;
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM matches WHERE NOT source_metadata_loaded OR native_match_context_pending
         ORDER BY id LIMIT 100 FOR UPDATE SKIP LOCKED",
    )
    .fetch_all(&mut *tx)
    .await?;

    for id in &ids {
        // Fetch sources one at a time: a batch must not retain 100 large originals.
        let (sgf, format, original, version, source_loaded, context_pending):
            (String, String, Vec<u8>, i64, bool, bool) = sqlx::query_as(
                "SELECT CASE WHEN NOT source_metadata_loaded AND metadata_version=0 THEN sgf ELSE '' END,
                        format, CASE WHEN metadata_version=0 AND format IN ('xg','bgf')
                                     THEN original ELSE ''::bytea END,
                        metadata_version, source_metadata_loaded, native_match_context_pending
                 FROM matches WHERE id=$1",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

        if version == 0 {
            let match_id = *id;
            let (legacy, context) = tokio::task::spawn_blocking(move || {
                let legacy = (!source_loaded).then(|| {
                    let mut metadata = source_metadata(&sgf);
                    // Recover outcomes missing from older canonical SGF.
                    let canonical = match format.as_str() {
                        "xg" => crate::import::xg_to_sgf(&original).ok(),
                        "bgf" => crate::import::bgf_to_sgf(&original).ok(),
                        _ => None,
                    };
                    if let Some(canonical) = canonical {
                        let source = source_metadata(&canonical);
                        metadata.result_state = source.result_state;
                        metadata.white_score = source.white_score;
                        metadata.black_score = source.black_score;
                    }
                    metadata
                });
                let mut context = if context_pending {
                    crate::import::match_context::from_original(&format, &original)
                        .unwrap_or_else(|error| {
                            tracing::warn!(%match_id, %format, %error, "Could not recover match context from original");
                            Default::default()
                        })
                } else {
                    Default::default()
                };
                if let Some(metadata) = &legacy {
                    context.played_on = context.played_on.or(metadata.played_on);
                    context.event = context.event.or_else(|| metadata.tournament.clone());
                    context.round = context.round.or_else(|| metadata.round.clone());
                    context.place = context.place.or_else(|| metadata.place.clone());
                }
                (legacy, context)
            })
            .await?;

            sqlx::query(
                "UPDATE matches
                 SET played_on=COALESCE(played_on,$2),tournament=COALESCE(tournament,$3),
                     round=COALESCE(round,$4),place=COALESCE(place,$5),
                     result_state=COALESCE($6,result_state),
                     white_score=CASE WHEN $6 IS NULL THEN white_score ELSE $7 END,
                     black_score=CASE WHEN $6 IS NULL THEN black_score ELSE $8 END,
                     metadata_version=metadata_version+CASE WHEN $9 AND (
                         (played_on IS NULL AND $2 IS NOT NULL)
                         OR (tournament IS NULL AND $3 IS NOT NULL)
                         OR (round IS NULL AND $4 IS NOT NULL)
                         OR (place IS NULL AND $5 IS NOT NULL)) THEN 1 ELSE 0 END,
                     source_metadata_loaded=true,native_match_context_pending=false
                 WHERE id=$1",
            )
            .bind(id)
            .bind(context.played_on)
            .bind(context.event)
            .bind(context.round)
            .bind(context.place)
            .bind(legacy.as_ref().map(|m| m.result_state.as_str()))
            .bind(legacy.as_ref().and_then(|m| m.white_score))
            .bind(legacy.as_ref().and_then(|m| m.black_score))
            .bind(context_pending)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query("UPDATE matches SET source_metadata_loaded=true,native_match_context_pending=false WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(ids.len())
}
