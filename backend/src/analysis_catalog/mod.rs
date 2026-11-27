//! One persisted choice over immutable imported and native analysis histories.
mod results;
mod selection;

pub use results::{PositionJobsQuery, PositionResultQuery, SelectedPositionAnalysis};
pub(crate) use results::{list_position_jobs, load_position_result, remove_position_job};
pub(crate) use selection::{SelectedAnalysis, resolve_selection, resolve_selection_on};

use crate::domain_error::DomainError;
use crate::json_errors::{Json, Path, Query};
use crate::{
    api::{ApiResult, AppState, ErrorBody},
    model::Job,
};
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    routing::{delete, get, put},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisSource {
    Native,
    Imported,
}
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisScope {
    Match,
    Game,
    Position,
}

#[derive(Debug, Serialize, FromRow, ToSchema)]
pub struct AnalysisCatalogEntry {
    #[sqlx(rename = "analysis_id")]
    pub id: String,
    #[schema(value_type = AnalysisSource)]
    pub source: String,
    #[schema(value_type = AnalysisScope)]
    pub scope: String,
    pub engine: String,
    pub depth: Option<String>,
    pub game_no: Option<i32>,
    pub position_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub status: String,
    /// True only when the native scope has completed durable coverage.
    pub complete: bool,
    pub checker_count: i64,
    pub cube_count: i64,
    #[sqlx(json(nullable))]
    pub job: Option<Job>,
}
#[derive(Serialize, ToSchema)]
pub struct AnalysisCatalogPage {
    pub match_id: Uuid,
    pub source_revision: i64,
    pub items: Vec<AnalysisCatalogEntry>,
    /// Effective selection, also returned when it is outside the current page.
    pub selected: Option<AnalysisCatalogEntry>,
    /// Explicit persisted identity; null means the automatic policy is active.
    pub selected_id: Option<String>,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AnalysisCatalogQuery {
    /// 1–100; default 50.
    pub limit: Option<i64>,
    /// Opaque continuation bound to this match and position filter.
    pub cursor: Option<String>,
    /// Limit individual-position jobs, while retaining all match/game analyses.
    pub position_id: Option<Uuid>,
}
#[derive(Serialize, Deserialize)]
struct Cursor {
    match_id: Uuid,
    position_id: Option<Uuid>,
    revision: i64,
    created_at: DateTime<Utc>,
    id: String,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalysisSelectionUpdate {
    /// Null returns the match to the shared automatic policy.
    pub analysis_id: Option<String>,
}
#[derive(Serialize, ToSchema)]
pub struct AnalysisSelection {
    pub match_id: Uuid,
    pub analysis_id: Option<String>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct DeleteAnalysisQuery {
    /// The match source revision shown when the deletion was confirmed.
    #[param(minimum = 0)]
    source_revision: i64,
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/matches/{id}/analyses", get(catalog))
        .route(
            "/api/matches/{id}/analyses/{analysis_id}",
            delete(delete_analysis),
        )
        .route(
            "/api/matches/{id}/analysis-selection",
            put(update_selection),
        )
        .route(
            "/api/positions/{id}/analysis-result",
            get(results::position_result),
        )
        .route("/api/positions/{id}/jobs", get(results::position_jobs))
        .route(
            "/api/positions/{id}/jobs/{job_id}",
            delete(results::delete_position_job),
        )
}

async fn entry_on(
    connection: &mut PgConnection,
    match_id: Uuid,
    analysis_id: &str,
) -> ApiResult<Option<AnalysisCatalogEntry>> {
    Ok(sqlx::query_as("SELECT a.*,to_jsonb(j) AS job FROM analysis_catalog_sources a LEFT JOIN jobs j ON j.id=a.job_id WHERE a.match_id=$1 AND a.analysis_id=$2")
        .bind(match_id).bind(analysis_id).fetch_optional(connection).await.map_err(anyhow::Error::from)?)
}

pub(crate) async fn list_catalog(
    state: &AppState,
    id: Uuid,
    query: AnalysisCatalogQuery,
) -> Result<AnalysisCatalogPage, DomainError> {
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(DomainError::invalid("limit must be between 1 and 100"));
    }
    let mut tx = crate::database_pool::begin_read_snapshot(&state.pool).await?;
    crate::play::require_review_available_on(&mut *tx, id).await?;
    let (revision, imported_loaded): (i64, bool) =
        sqlx::query_as("SELECT source_revision,imported_analysis_loaded FROM matches WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?
            .ok_or_else(DomainError::missing)?;
    if !imported_loaded {
        return Err(crate::browse_indexes::preparing(state, "Imported analysis").into());
    }
    if let Some(position) = query.position_id {
        let belongs: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM positions WHERE id=$1 AND match_uuid=$2)",
        )
        .bind(position)
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
        if !belongs {
            return Err(DomainError::invalid(
                "Position does not belong to this match",
            ));
        }
    }
    let cursor = query
        .cursor
        .as_deref()
        .map(|encoded| -> Result<Cursor, DomainError> {
            let cursor: Cursor = crate::cursor::decode(
                encoded,
                crate::cursor::DEFAULT_MAX_LEN,
                "Invalid catalog cursor",
            )?;
            if cursor.match_id != id
                || cursor.position_id != query.position_id
                || cursor.revision != revision
            {
                return Err(DomainError::invalid(
                    "Catalog cursor does not match the current source and filters",
                ));
            }
            Ok(cursor)
        })
        .transpose()?;
    let selected_id: Option<String> =
        sqlx::query_scalar("SELECT analysis_id FROM analysis_selections WHERE match_id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    let effective_id: Option<String> = sqlx::query_scalar(
        "SELECT analysis_id FROM effective_analysis_selections WHERE match_id=$1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let selected = if let Some(key) = effective_id {
        entry_on(&mut tx, id, &key).await?
    } else {
        None
    };
    let mut items:Vec<AnalysisCatalogEntry>=sqlx::query_as("SELECT a.*,to_jsonb(j) AS job FROM analysis_catalog_sources a LEFT JOIN jobs j ON j.id=a.job_id WHERE a.match_id=$1 AND ($2::uuid IS NULL OR a.scope<>'position' OR a.position_id=$2) AND ($3::timestamptz IS NULL OR (a.created_at,a.analysis_id)<($3,$4)) ORDER BY a.created_at DESC,a.analysis_id DESC LIMIT $5")
        .bind(id).bind(query.position_id).bind(cursor.as_ref().map(|c|c.created_at)).bind(cursor.as_ref().map(|c|&c.id)).bind(limit+1).fetch_all(&mut *tx).await.map_err(anyhow::Error::from)?;
    let next_cursor = if items.len() > limit as usize {
        items.pop();
        items
            .last()
            .map(|last| {
                crate::cursor::encode(&Cursor {
                    match_id: id,
                    position_id: query.position_id,
                    revision,
                    created_at: last.created_at,
                    id: last.id.clone(),
                })
            })
            .transpose()?
    } else {
        None
    };
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(AnalysisCatalogPage {
        match_id: id,
        source_revision: revision,
        items,
        selected,
        selected_id,
        next_cursor,
    })
}

#[utoipa::path(get, path="/api/matches/{id}/analyses", params(("id"=Uuid,Path),AnalysisCatalogQuery),responses((status=200,body=AnalysisCatalogPage),(status=404,body=ErrorBody),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Analysis catalog")]
async fn catalog(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<AnalysisCatalogQuery>,
) -> ApiResult<Json<AnalysisCatalogPage>> {
    Ok(Json(list_catalog(&state, id, query).await?))
}

pub(crate) async fn delete_analysis_entry(
    state: &AppState,
    id: Uuid,
    analysis_id: &str,
    source_revision: i64,
) -> Result<(), DomainError> {
    if source_revision < 0 {
        return Err(DomainError::invalid("Source revision must be nonnegative"));
    }
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let revision: i64 =
        sqlx::query_scalar("SELECT source_revision FROM matches WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?
            .ok_or_else(DomainError::missing)?;
    crate::play::require_review_available_on(&mut *tx, id).await?;
    if revision != source_revision {
        return Err(DomainError::conflict("The match source has changed"));
    }
    let job_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT job_id FROM analysis_catalog_sources WHERE match_id=$1 AND analysis_id=$2",
    )
    .bind(id)
    .bind(analysis_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?
    .ok_or_else(DomainError::missing)?;
    if let Some(job_id) = job_id {
        let deleted =
            sqlx::query("DELETE FROM jobs WHERE id=$1 AND status IN ('completed','failed')")
                .bind(job_id)
                .execute(&mut *tx)
                .await
                .map_err(anyhow::Error::from)?;
        if deleted.rows_affected() == 0 {
            return Err(DomainError::conflict(
                "The analysis is no longer available for deletion",
            ));
        }
    } else {
        // The original artifact and its loaded marker remain available for export.
        // Keeping the marker prevents lazy catalog reads from restoring deleted values.
        sqlx::query("DELETE FROM imported_analyses WHERE match_uuid=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    }
    sqlx::query("DELETE FROM analysis_selections WHERE match_id=$1 AND analysis_id=$2")
        .bind(id)
        .bind(analysis_id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    state.browse_wake.notify_one();
    Ok(())
}

#[utoipa::path(delete,path="/api/matches/{id}/analyses/{analysis_id}",params(("id"=Uuid,Path),("analysis_id"=String,Path),DeleteAnalysisQuery),responses((status=204,description="Saved analysis deleted"),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Analysis catalog")]
async fn delete_analysis(
    State(state): State<AppState>,
    Path((id, analysis_id)): Path<(Uuid, String)>,
    Query(query): Query<DeleteAnalysisQuery>,
) -> ApiResult<StatusCode> {
    delete_analysis_entry(&state, id, &analysis_id, query.source_revision).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn select_analysis(
    state: &AppState,
    id: Uuid,
    request: AnalysisSelectionUpdate,
) -> Result<AnalysisSelection, DomainError> {
    crate::play::require_review_available(&state.pool, id).await?;
    if !crate::imported_analysis::ensure_catalog(&state.pool, id).await? {
        return Err(DomainError::missing());
    }
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM matches WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    if exists.is_none() {
        return Err(DomainError::missing());
    }
    crate::play::require_review_available_on(&mut *tx, id).await?;
    if let Some(key) = request.analysis_id.as_deref() {
        resolve_selection_on(&mut tx, id, Some(key)).await?;
        sqlx::query("INSERT INTO analysis_selections(match_id,analysis_id,source_revision) SELECT id,$2,source_revision FROM matches WHERE id=$1 ON CONFLICT(match_id) DO UPDATE SET analysis_id=excluded.analysis_id,source_revision=excluded.source_revision,updated_at=now()")
            .bind(id).bind(key).execute(&mut *tx).await.map_err(anyhow::Error::from)?;
    } else {
        sqlx::query("DELETE FROM analysis_selections WHERE match_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    state.browse_wake.notify_one();
    Ok(AnalysisSelection {
        match_id: id,
        analysis_id: request.analysis_id,
    })
}

#[utoipa::path(put, path="/api/matches/{id}/analysis-selection", params(("id"=Uuid,Path)),request_body=AnalysisSelectionUpdate,responses((status=200,body=AnalysisSelection),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Analysis catalog")]
async fn update_selection(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<AnalysisSelectionUpdate>,
) -> ApiResult<Json<AnalysisSelection>> {
    Ok(Json(select_analysis(&state, id, request).await?))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        catalog,
        delete_analysis,
        update_selection,
        results::position_result,
        results::position_jobs,
        results::delete_position_job
    ),
    components(schemas(
        AnalysisSource,
        AnalysisScope,
        AnalysisCatalogEntry,
        AnalysisCatalogPage,
        AnalysisSelectionUpdate,
        AnalysisSelection,
        results::SelectedPositionAnalysis
    ))
)]
pub struct AnalysisCatalogDoc;
