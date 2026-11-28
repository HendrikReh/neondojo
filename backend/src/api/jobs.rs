use super::{ApiError, ApiResult, AppState, ErrorBody, MatchAnalysisEngine};
use crate::domain_error::DomainError;
use crate::json_errors::{Json, Path};
use crate::{model::*, store};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, convert::Infallible, time::Duration};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PositionJobKind {
    Analysis,
    Dice,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct JobRequest {
    /// Database UUID of the imported position, not its GNUbg ID.
    pub position_id: Uuid,
    /// A ranked candidate analysis or a distribution over 21 weighted rolls.
    pub kind: PositionJobKind,
    /// Provider for this job; omitted uses the saved analysis engine.
    pub engine_id: Option<MatchAnalysisEngine>,
    /// Verified search depth of the selected engine. Omitted uses saved analysis depth.
    #[serde(default)]
    #[schema(minimum = 1, maximum = 5)]
    pub plies: Option<i32>,
}

pub(crate) async fn submit_position_job(
    state: &AppState,
    request: JobRequest,
) -> Result<Job, DomainError> {
    crate::play::require_position_review_available(&state.pool, request.position_id).await?;
    let engine_id = request.engine_id.as_ref().map(MatchAnalysisEngine::id);
    let (settings, snapshot) =
        crate::settings::snapshot_for_analysis(state, request.plies, engine_id)
            .await
            .map_err(|e| DomainError::invalid(format!("Engine unavailable: {e:#}")))?;
    let plies = request.plies.unwrap_or(settings.analysis_plies);
    if !crate::configuration::engine_depths(&snapshot.engine_id, "analysis").contains(&plies) {
        return Err(DomainError::invalid(
            "Depth is not verified for the selected engine",
        ));
    }
    let position = store::get_position(&state.pool, request.position_id)
        .await?
        .ok_or_else(DomainError::missing)?;
    if snapshot.provider() == Some(crate::engines::EngineId::NeonGammon) {
        crate::engines::validate_match_id_away(&position.gnubg_match_id)
            .map_err(|e| DomainError::invalid(format!("{e:#}")))?;
    }
    let job = store::create_job_with_snapshot(
        &state.pool,
        request.position_id,
        match request.kind {
            PositionJobKind::Analysis => "analysis",
            PositionJobKind::Dice => "dice",
        },
        plies,
        Some(&snapshot),
    )
    .await?;
    state.wake.notify_one();
    Ok(job)
}

#[utoipa::path(post, path="/api/jobs", request_body=JobRequest, responses((status=202, body=Job), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Analysis")]
pub(super) async fn create_job(
    State(state): State<AppState>,
    Json(request): Json<JobRequest>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    Ok((
        StatusCode::ACCEPTED,
        Json(submit_position_job(&state, request).await?),
    ))
}

pub(crate) async fn get_job_by_id(state: &AppState, id: Uuid) -> Result<Job, DomainError> {
    let job = store::get_job(&state.pool, id)
        .await?
        .ok_or_else(DomainError::missing)?;
    crate::play::require_job_review_available(&state.pool, &job)
        .await
        .map_err(DomainError::from)?;
    Ok(job)
}

#[utoipa::path(get, path="/api/jobs/{id}", params(("id"=Uuid, Path)), responses((status=200, body=Job), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Analysis")]
pub(super) async fn job(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Job>> {
    Ok(Json(get_job_by_id(&state, id).await?))
}

pub(crate) async fn cancel_job_by_id(state: &AppState, id: Uuid) -> Result<Job, DomainError> {
    let job = store::get_job(&state.pool, id)
        .await?
        .ok_or_else(DomainError::missing)?;
    crate::play::require_job_review_available(&state.pool, &job)
        .await
        .map_err(DomainError::from)?;
    match crate::jobs::cancel(state, id).await? {
        Some((true, job)) => Ok(job),
        Some((false, _)) => Err(DomainError::conflict("The job has already finished")),
        None => Err(DomainError::missing()),
    }
}

/// Cancel a queued or running analysis, dice or match job. The job ends as
/// `failed` with the error "Cancelled by user"; a running engine process is
/// stopped, and games a match analysis completed before stay saved.
#[utoipa::path(post, path="/api/jobs/{id}/cancel", params(("id"=Uuid, Path)), responses((status=200, body=Job, description="The cancelled job"), (status=404, body=ErrorBody), (status=409, body=ErrorBody, description="The job has already finished"), (status=422, body=ErrorBody)), tag="Analysis")]
pub(super) async fn cancel_job(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Job>> {
    Ok(Json(cancel_job_by_id(&state, id).await?))
}

#[derive(Deserialize, ToSchema)]
pub(super) struct JobStatusRequest {
    #[schema(max_items = 100)]
    ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct JobStatusBatch {
    pub jobs: Vec<Job>,
    pub missing: Vec<Uuid>,
    pub restricted: Vec<Uuid>,
}

pub(crate) async fn get_job_status_batch(
    state: &AppState,
    ids: Vec<Uuid>,
) -> Result<JobStatusBatch, DomainError> {
    if ids.len() > 100 {
        return Err(DomainError::invalid(
            "At most 100 job IDs can be read at once",
        ));
    }
    let mut missing: BTreeSet<Uuid> = ids.iter().copied().collect();
    let mut jobs = Vec::new();
    let mut restricted = Vec::new();
    for row in store::jobs_with_review_status(&state.pool, &ids).await? {
        missing.remove(&row.job.id);
        if row.restricted {
            restricted.push(row.job.id);
        } else {
            jobs.push(row.job);
        }
    }
    Ok(JobStatusBatch {
        jobs,
        missing: missing.into_iter().collect(),
        restricted,
    })
}

#[utoipa::path(post, path="/api/jobs/status", request_body=JobStatusRequest,
    responses((status=200, body=JobStatusBatch, description="Each requested ID is represented once as an available job, a missing ID, or an ID restricted by competition play."), (status=422, body=ErrorBody)), tag="Analysis")]
pub(super) async fn job_status_batch(
    State(state): State<AppState>,
    Json(request): Json<JobStatusRequest>,
) -> ApiResult<Json<JobStatusBatch>> {
    Ok(Json(get_job_status_batch(&state, request.ids).await?))
}

impl AppState {
    /// Streams durable persisted [`JobEvent`]s for `id` strictly after `after`,
    /// woken by PostgreSQL `LISTEN neondojo_job_events` until a terminal event,
    /// job deletion, or runtime shutdown.
    pub fn watch_job_events(
        &self,
        id: Uuid,
        mut after: i64,
    ) -> impl Stream<Item = JobEvent> + Send + 'static + use<> {
        let state = self.clone();
        let mut notifications = self.subscribe_job_updates();
        async_stream::stream! {
            loop {
                if state.shutdown.is_cancelled() { return; }
                // The replay sees commits covered by queued hints. Keep listening
                // for any commits made while the replay queries run.
                notifications = notifications.resubscribe();
                match store::events(&state.pool, id, after).await {
                    Ok(events) => {
                        let count = events.len();
                        for event in events {
                            after = event.id;
                            let terminal = event.event == "completed" || event.event == "failed";
                            yield event;
                            if terminal { return; }
                        }
                        if count == 100 { continue; }
                        match store::get_job(&state.pool, id).await {
                            Ok(None) => return,
                            Ok(Some(job)) if matches!(job.status.as_str(), "completed" | "failed") => {
                                // Completion may have committed between the replay and status reads.
                                let last = sqlx::query_scalar::<_, Option<i64>>(
                                    "SELECT max(id) FROM job_events WHERE job_id=$1"
                                ).bind(id).fetch_one(&state.pool).await;
                                match last {
                                    Ok(Some(last)) if last > after => continue,
                                    Ok(_) => return,
                                    Err(error) => tracing::warn!(%error, "SSE final event lookup failed"),
                                }
                            }
                            Ok(Some(_)) => {}
                            Err(error) => tracing::warn!(%error, "SSE job lookup failed"),
                        }
                    }
                    Err(error) => { tracing::warn!(%error, "SSE database read failed"); return; }
                }
                tokio::select! {
                    _ = state.shutdown.cancelled() => return,
                    _ = crate::job_events::wait(&mut notifications, id) => {},
                }
            }
        }
    }
}

pub(crate) async fn watch_job_events_checked(
    state: &AppState,
    id: Uuid,
    after: i64,
) -> Result<impl Stream<Item = JobEvent> + Send + 'static + use<>, DomainError> {
    if after < 0 {
        return Err(DomainError::invalid("Invalid Last-Event-ID"));
    }
    let _job = get_job_by_id(state, id).await?;
    Ok(state.watch_job_events(id, after))
}

#[utoipa::path(get, path="/api/jobs/{id}/events", params(("id"=Uuid, Path),("Last-Event-ID"=Option<i64>, Header, description="Resume after this persisted event ID")), responses((status=200, body=String, content_type="text/event-stream", description="SSE wire format: id: <persisted integer>\\nevent: <queued|running|progress|completed|failed>\\ndata: <JobProgress JSON>\\n\\n. Each data field conforms to components.schemas.JobProgress. Replay ends when the job disappears, its terminal history is exhausted, or the service shuts down. On disconnect, read the job status. Reconnect to an active job with Last-Event-ID."),(status=404, body=ErrorBody),(status=422,body=ErrorBody)), tag="Analysis")]
pub(super) async fn events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let after = headers
        .get("last-event-id")
        .map(|h| h.to_str().ok().and_then(|s| s.parse::<i64>().ok()))
        .unwrap_or(Some(0))
        .filter(|n| *n >= 0)
        .ok_or_else(|| ApiError::invalid("Invalid Last-Event-ID"))?;
    let stream = watch_job_events_checked(&state, id, after)
        .await?
        .map(|event| {
            Ok(Event::default()
                .id(event.id.to_string())
                .event(event.event)
                .data(event.data.to_string()))
        });
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keepalive"),
    ))
}
