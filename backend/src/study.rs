//! Read-only archive of learning reports already durably stored with completed sessions.
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    play::PlaySession,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

const REPORT_SOURCES:&str="WITH report_sources AS (
    SELECT id,session_id,library_match_id,game_no,state,sgf,created_at,updated_at,true AS archived
      FROM study_report_snapshots
    UNION ALL
    SELECT s.id,s.id AS session_id,s.library_match_id,CASE WHEN (s.state->>'match_length')::int=0 THEN (s.state->>'game_no')::int ELSE 0 END,s.state,s.sgf,s.created_at,s.updated_at,false
      FROM play_sessions s WHERE s.state->>'phase'='match_over' AND jsonb_typeof(s.state->'review')='object'
      AND NOT EXISTS(SELECT 1 FROM study_report_snapshots r WHERE r.session_id=s.id)
)";
#[derive(Serialize, ToSchema, FromRow)]
pub struct StudyReportSummary {
    pub id: Uuid,
    pub session_id: Uuid,
    /// Current cumulative session score; never the sum of per-report scores.
    pub session_scores: Vec<i32>,
    pub money_session_ended: bool,
    pub library_match_id: Option<Uuid>,
    pub game_no: Option<i32>,
    /// Library match name, or "<human> · <bot>" when the match has no name.
    #[sqlx(default)]
    pub name: String,
    /// Engine ID the session was played against (`gnubg` or `bgsage`).
    pub engine_id: String,
    /// User-facing name of that engine, for example "NeonGammon".
    #[sqlx(default)]
    pub bot_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub human_player: i32,
    pub human_name: String,
    pub match_length: i32,
    pub scores: Vec<i32>,
    pub play_mode: String,
    pub first_attempt_rating: Option<f64>,
    pub assisted: bool,
    pub analysis_plies: i32,
}
#[derive(Serialize, ToSchema)]
pub struct StudyReportPage {
    pub items: Vec<StudyReportSummary>,
    pub next_cursor: Option<String>,
}
#[derive(Deserialize, IntoParams)]
struct ReportQuery {
    limit: Option<i64>,
    after: Option<String>,
    /// "session" paginates complete money sessions; point reports remain individual.
    group_by: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Cursor {
    updated_at: DateTime<Utc>,
    id: Uuid,
    #[serde(default)]
    grouped: bool,
}
#[derive(FromRow)]
struct ReportRow {
    #[sqlx(flatten)]
    report: StudyReportSummary,
    library_name: Option<String>,
    page_id: Uuid,
    page_updated_at: DateTime<Utc>,
}
impl ReportRow {
    fn into_summary(self) -> StudyReportSummary {
        let mut report = self.report;
        report.bot_name = crate::engines::display_name(&report.engine_id)
            .unwrap_or("GNUbg")
            .to_owned();
        report.name = self
            .library_name
            .unwrap_or_else(|| format!("{} · {}", report.human_name, report.bot_name));
        report
    }
}
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/study/reports", axum::routing::get(list_reports))
        .route("/api/study/reports/{id}", axum::routing::get(get_report))
        .route(
            "/api/study/reports/{id}/history",
            axum::routing::get(report_history),
        )
}
#[utoipa::path(get, path="/api/study/reports", params(ReportQuery), responses((status=200, body=StudyReportPage),(status=422,body=ErrorBody)), tag="Study")]
async fn list_reports(
    State(state): State<AppState>,
    Query(query): Query<ReportQuery>,
) -> ApiResult<Json<StudyReportPage>> {
    let grouped = match query.group_by.as_deref() {
        None => false,
        Some("session") => true,
        _ => return Err(ApiError::invalid("group_by must be session")),
    };
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::invalid("limit must be between 1 and 100"));
    }
    let cursor: Option<Cursor> = query
        .after
        .map(|encoded| {
            if encoded.len() > 512 {
                return Err(ApiError::invalid("Invalid report cursor"));
            }
            let bytes = URL_SAFE_NO_PAD
                .decode(encoded)
                .map_err(|_| ApiError::invalid("Invalid report cursor"))?;
            serde_json::from_slice(&bytes).map_err(|_| ApiError::invalid("Invalid report cursor"))
        })
        .transpose()?;
    if cursor
        .as_ref()
        .is_some_and(|cursor| cursor.grouped != grouped)
    {
        return Err(ApiError::invalid("Report cursor uses a different grouping"));
    }
    // Resolve page boundaries from indexed, small metadata. Report sources are
    // referenced only once below, so PostgreSQL can inline them and fetch JSON
    // only for the selected page instead of materializing every full report/SGF.
    let groups = if grouped {
        "SELECT CASE WHEN money THEN session_id ELSE id END AS id,
                max(updated_at) AS updated_at FROM report_keys GROUP BY 1"
    } else {
        "SELECT id,updated_at FROM report_keys"
    };
    let sql = format!(
        "{REPORT_SOURCES}, report_keys AS (
          SELECT id,session_id,game_no>0 AS money,updated_at FROM study_report_snapshots
          UNION ALL
          SELECT s.id,s.id,(s.state->>'match_length')::int=0,s.updated_at
          FROM play_sessions s WHERE s.state->>'phase'='match_over' AND jsonb_typeof(s.state->'review')='object'
          AND NOT EXISTS(SELECT 1 FROM study_report_snapshots r WHERE r.session_id=s.id)
        ), report_groups AS ({groups}), selected_groups AS (
          SELECT id,updated_at FROM report_groups
          WHERE ($1::timestamptz IS NULL OR (updated_at,id)<($1,$2))
          ORDER BY updated_at DESC,id DESC LIMIT $3
        )
        SELECT s.id,s.session_id,
        ARRAY[(coalesce(live.state,s.state)->'scores'->>0)::int,(coalesce(live.state,s.state)->'scores'->>1)::int] AS session_scores,
        coalesce((live.state->>'money_session_ended')::bool,false) AS money_session_ended,
        s.library_match_id,nullif(s.game_no,0) AS game_no,
        nullif(m.display_name,'') AS library_name,
        coalesce(s.state->'engine'->>'id','gnubg') AS engine_id,
        s.created_at,s.updated_at,(s.state->>'human_player')::int AS human_player,
        s.state->>'human_name' AS human_name,(s.state->>'match_length')::int AS match_length,
        ARRAY[(s.state->'scores'->>0)::int,(s.state->'scores'->>1)::int] AS scores,
        coalesce(s.state->>'play_mode','competition') AS play_mode,
        (s.state->'review'->>'first_attempt_rating')::float8 AS first_attempt_rating,
        coalesce((s.state->'review'->>'assisted')::bool,false) AS assisted,
        (s.state->'review'->>'analysis_plies')::int AS analysis_plies
        ,g.id AS page_id,g.updated_at AS page_updated_at
        FROM selected_groups g JOIN report_sources s
          ON (CASE WHEN $4 AND s.game_no>0 THEN s.session_id ELSE s.id END)=g.id
        LEFT JOIN play_sessions live ON live.id=s.session_id
        LEFT JOIN matches m ON m.id=s.library_match_id
        ORDER BY g.updated_at DESC,g.id DESC,s.game_no ASC,s.id DESC"
    );
    let mut rows: Vec<ReportRow> = sqlx::query_as(&sql)
        .bind(cursor.as_ref().map(|c| c.updated_at))
        .bind(cursor.as_ref().map(|c| c.id))
        .bind(limit + 1)
        .bind(grouped)
        .fetch_all(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;
    let mut group_ids = Vec::new();
    for row in &rows {
        if group_ids.last() != Some(&row.page_id) {
            group_ids.push(row.page_id);
        }
    }
    let has_more = group_ids.len() > limit as usize;
    if has_more {
        let extra_group = group_ids[limit as usize];
        rows.retain(|row| row.page_id != extra_group);
    }
    let next_cursor = if has_more {
        let last = rows.last().expect("positive limit");
        Some(
            URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(&Cursor {
                    updated_at: last.page_updated_at,
                    id: last.page_id,
                    grouped,
                })
                .map_err(anyhow::Error::from)?,
            ),
        )
    } else {
        None
    };
    let items = rows.into_iter().map(ReportRow::into_summary).collect();
    Ok(Json(StudyReportPage { items, next_cursor }))
}
#[utoipa::path(get,path="/api/study/reports/{id}", params(("id"=Uuid,Path)), responses((status=200,body=PlaySession),(status=404,body=ErrorBody)),tag="Study")]
async fn get_report(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<PlaySession>> {
    let (session, _) = report_checkpoint(&state.pool, id).await?;
    Ok(Json(session))
}
pub(crate) async fn report_checkpoint(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> ApiResult<(PlaySession, String)> {
    let sql=format!("{REPORT_SOURCES} SELECT
      CASE WHEN archived THEN
        jsonb_set(jsonb_set(jsonb_set(jsonb_set(state,'{{library_match_id}}',coalesce(to_jsonb(library_match_id),'null'::jsonb)),
        '{{study_report_id}}',to_jsonb(id)), '{{study_report_version}}',state->'version'),
        '{{review_game_no}}',coalesce(to_jsonb(nullif(game_no,0)),'null'::jsonb))
      ELSE jsonb_set(state,'{{library_match_id}}',coalesce(to_jsonb(library_match_id),'null'::jsonb)) END,
      sgf FROM report_sources WHERE id=$1");
    let row: Option<(sqlx::types::Json<PlaySession>, String)> = sqlx::query_as(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(anyhow::Error::from)?;
    row.map(|(state, sgf)| (state.0, sgf))
        .ok_or_else(ApiError::missing)
}
#[derive(Deserialize)]
struct ReportHistoryQuery {
    version: i64,
}
#[utoipa::path(get,path="/api/study/reports/{id}/history",params(("id"=Uuid,Path),("version"=i64,Query)),responses((status=200,body=crate::play::PlayHistorySnapshots),(status=404,body=ErrorBody),(status=409,body=ErrorBody)),tag="Study")]
async fn report_history(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ReportHistoryQuery>,
) -> ApiResult<Json<crate::play::PlayHistorySnapshots>> {
    let (session, sgf) = report_checkpoint(&state.pool, id).await?;
    if session.version != query.version {
        return Err(ApiError(
            axum::http::StatusCode::CONFLICT,
            "Report version differs from saved snapshot".into(),
        ));
    }
    let history =
        tokio::task::spawn_blocking(move || crate::play::build_history_snapshots(&session, &sgf))
            .await
            .map_err(anyhow::Error::from)??;
    Ok(Json(history))
}

#[derive(OpenApi)]
#[openapi(
    paths(list_reports, get_report, report_history),
    components(schemas(StudyReportSummary, StudyReportPage))
)]
pub(crate) struct StudyDoc;
