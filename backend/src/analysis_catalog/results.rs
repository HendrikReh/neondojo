use super::{
    entry_on,
    selection::{SelectedAnalysis, resolve_selection_on},
};
use crate::json_errors::{Json, Path, Query};
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody, MatchAnalysisEngine},
    engine::Analysis,
    imported_analysis::ImportedAnalysisRecord,
    match_analysis::Decision,
    model::{Job, Position},
};
use axum::{extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in=Query)]
pub(super) struct PositionResultQuery {
    /// Explicit identity; omission resolves the match's persisted or automatic selection.
    analysis_id: Option<String>,
}
#[derive(Serialize, ToSchema)]
pub struct SelectedPositionAnalysis {
    pub analysis_id: String,
    pub position_id: Uuid,
    pub engine: String,
    pub depth: Option<String>,
    /// Original imported candidates, cube values, units and perspective.
    pub records: Vec<ImportedAnalysisRecord>,
    /// Native match/game decision with best, played, and alternatives when recorded.
    pub decision: Option<Decision>,
    /// Original candidates from a saved individual-position calculation.
    pub analysis: Option<Analysis>,
}

#[utoipa::path(get,path="/api/positions/{id}/analysis-result",params(("id"=Uuid,Path),PositionResultQuery),responses((status=200,body=Option<SelectedPositionAnalysis>),(status=404,body=ErrorBody),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Analysis catalog")]
pub(super) async fn position_result(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<PositionResultQuery>,
) -> ApiResult<Json<Option<SelectedPositionAnalysis>>> {
    let mut tx = crate::database_pool::begin_read_snapshot(&state.pool).await?;
    let position: Position = sqlx::query_as("SELECT * FROM positions WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    crate::play::require_review_available_on(&mut *tx, position.match_uuid).await?;
    let imported_loaded: bool =
        sqlx::query_scalar("SELECT imported_analysis_loaded FROM matches WHERE id=$1")
            .bind(position.match_uuid)
            .fetch_optional(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?
            .ok_or_else(ApiError::missing)?;
    if !imported_loaded {
        return Err(crate::browse_indexes::preparing(
            &state,
            "Imported analysis",
        ));
    }
    let Some(selected) =
        resolve_selection_on(&mut tx, position.match_uuid, query.analysis_id.as_deref()).await?
    else {
        return Ok(Json(None));
    };
    let key = selected.id();
    let Some(entry) = entry_on(&mut tx, position.match_uuid, &key).await? else {
        return Ok(Json(None));
    };
    let mut response = SelectedPositionAnalysis {
        analysis_id: key,
        position_id: id,
        engine: entry.engine,
        depth: entry.depth,
        records: vec![],
        decision: None,
        analysis: None,
    };
    match selected {
        SelectedAnalysis::Imported { match_id } => {
            response.records =
                crate::imported_analysis::selected_position_records(&mut *tx, match_id, id).await?;
            if response.records.is_empty() {
                return Ok(Json(None));
            }
        }
        SelectedAnalysis::Job(job) if job.kind == "analysis" => {
            if job.position_uuid != Some(id) {
                return Ok(Json(None));
            }
            response.analysis = job
                .result
                .and_then(|data| serde_json::from_value::<Analysis>(data).ok())
                .filter(|a| !a.candidates.is_empty());
            if response.analysis.is_none() {
                return Ok(Json(None));
            }
        }
        SelectedAnalysis::Job(job) => {
            let row:Option<(Value,f64)>=sqlx::query_as("SELECT a.data,a.equity_loss FROM move_analyses a JOIN match_analysis_games c ON c.job_id=a.job_id AND c.game_no=a.game_no WHERE a.job_id=$1 AND a.position_uuid=$2 AND a.game_no=$3 AND a.player=$4 AND a.kind='checker' AND ($5::integer IS NULL OR a.game_no=$5) ORDER BY a.id LIMIT 1")
                .bind(job.id).bind(id).bind(position.game_no).bind(position.player).bind(job.game_no).fetch_optional(&mut *tx).await.map_err(anyhow::Error::from)?;
            let Some((data, loss)) = row else {
                return Ok(Json(None));
            };
            if crate::move_assessments::native_checker_loss(&position, &data, loss).is_none() {
                return Ok(Json(None));
            }
            response.decision = serde_json::from_value(data).ok();
            if response.decision.is_none() {
                return Ok(Json(None));
            }
        }
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(Some(response)))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in=Query)]
pub(super) struct PositionJobsQuery {
    /// Calculation monitoring only; saved analysis selection never uses this filter.
    engine_id: Option<MatchAnalysisEngine>,
}
#[utoipa::path(get,path="/api/positions/{id}/jobs",params(("id"=Uuid,Path),PositionJobsQuery),responses((status=200,body=Vec<Job>),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Jobs")]
pub(super) async fn position_jobs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<PositionJobsQuery>,
) -> ApiResult<Json<Vec<Job>>> {
    let position = crate::store::get_position(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    crate::play::require_review_available(&state.pool, position.match_uuid).await?;
    let jobs=sqlx::query_as("SELECT DISTINCT ON (j.kind) j.* FROM jobs j JOIN positions p ON p.id=j.position_uuid JOIN matches m ON m.id=p.match_uuid AND m.source_revision=j.source_revision WHERE p.id=$1 AND j.kind IN ('analysis','dice') AND ($2::text IS NULL OR coalesce(j.engine_snapshot->>'engine_id','gnubg')=$2) ORDER BY j.kind,j.created_at DESC,j.id DESC LIMIT 2")
        .bind(id).bind(query.engine_id.as_ref().map(MatchAnalysisEngine::id)).fetch_all(&state.pool).await.map_err(anyhow::Error::from)?;
    Ok(Json(jobs))
}

/// Delete one finished dice equity calculation of a position. Checker analyses
/// belong to the analysis catalog and are deleted there.
#[utoipa::path(delete,path="/api/positions/{id}/jobs/{job_id}",params(("id"=Uuid,Path),("job_id"=Uuid,Path)),responses((status=204,description="Dice equity deleted"),(status=404,body=ErrorBody),(status=409,body=ErrorBody,description="The calculation is still queued or running"),(status=422,body=ErrorBody)),tag="Jobs")]
pub(super) async fn delete_position_job(
    State(state): State<AppState>,
    Path((id, job_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    // Hold the owning match like job submission so a match cascade cannot interleave.
    let owner = crate::position_retention::lock_owner(&mut tx, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    crate::play::require_review_available_on(&mut *tx, owner).await?;
    let job: Option<(String, String)> =
        sqlx::query_as("SELECT kind,status FROM jobs WHERE id=$1 AND position_uuid=$2 FOR UPDATE")
            .bind(job_id)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    let (kind, status) = job.ok_or_else(ApiError::missing)?;
    if kind != "dice" {
        return Err(ApiError::invalid(
            "Only dice equity can be deleted here; delete checker analyses in the analysis catalog",
        ));
    }
    if status != "completed" && status != "failed" {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "The dice equity calculation is still queued or running".into(),
        ));
    }
    sqlx::query("DELETE FROM jobs WHERE id=$1")
        .bind(job_id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}
