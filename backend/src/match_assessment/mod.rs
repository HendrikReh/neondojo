//! Assessments own their queue because the source is a saved report, not a library match.
mod evidence;
mod openai;

use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    configuration::ConfigurationLocale,
    play::PlaySession,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use serde::{Deserialize, Serialize};
use sqlx::types::Json as DbJson;
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

const PROMPT_VERSION: i32 = 1;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Assessment {
    summary: String,
    next_match_focus: String,
    topics: Vec<Topic>,
    key_decisions: Vec<KeyDecision>,
    training: Vec<Training>,
    limitations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct Topic {
    topic: TopicName,
    observation: String,
    evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum TopicName {
    Cube,
    Checker,
    Opening,
    BearOff,
    Hit,
    Backgame,
    Race,
    Prime,
    Contact,
    Anchor,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct KeyDecision {
    evidence_ref: String,
    explanation: String,
    next_question: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct Training {
    focus: String,
    exercise: String,
    success_criterion: String,
    evidence_refs: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum Availability {
    Ready,
    MissingAnalysis,
    Unsupported,
    NotConfigured,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum AssessmentStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum Failure {
    Provider,
    InvalidReport,
    Interrupted,
    SourceUnavailable,
    NotConfigured,
}

#[derive(Debug, Serialize, ToSchema)]
struct AssessmentState {
    availability: Availability,
    abbreviated_match: bool,
    job: Option<AssessmentJob>,
}

#[derive(Debug, Serialize, ToSchema)]
struct AssessmentJob {
    status: AssessmentStatus,
    model: String,
    error: Option<Failure>,
    result: Option<Assessment>,
}

#[derive(sqlx::FromRow)]
struct JobRow {
    status: String,
    model: String,
    error_code: Option<String>,
    result: Option<DbJson<Assessment>>,
}

impl From<JobRow> for AssessmentJob {
    fn from(row: JobRow) -> Self {
        Self {
            status: match row.status.as_str() {
                "queued" => AssessmentStatus::Queued,
                "running" => AssessmentStatus::Running,
                "completed" => AssessmentStatus::Completed,
                _ => AssessmentStatus::Failed,
            },
            model: row.model,
            error: row.error_code.as_deref().map(|code| match code {
                "invalid_report" => Failure::InvalidReport,
                "interrupted" => Failure::Interrupted,
                "source_unavailable" => Failure::SourceUnavailable,
                "not_configured" => Failure::NotConfigured,
                _ => Failure::Provider,
            }),
            result: row.result.map(|value| value.0),
        }
    }
}

#[derive(Deserialize, IntoParams)]
struct AssessmentQuery {
    language: ConfigurationLocale,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct StartAssessment {
    report_version: i64,
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route(
        "/api/study/reports/{id}/assessment",
        get(get_assessment).post(start_assessment),
    )
}

fn availability(session: &PlaySession) -> Availability {
    if session.match_length <= 0 {
        Availability::Unsupported
    } else if !session.review.as_ref().is_some_and(|review| {
        review.games.iter().any(|game| {
            game.decisions.iter().any(|decision| {
                decision.player == session.human_player
                    && crate::match_analysis::evaluated_equities(decision).is_some()
            })
        })
    }) {
        Availability::MissingAnalysis
    } else if !openai::configured() {
        Availability::NotConfigured
    } else {
        Availability::Ready
    }
}

#[utoipa::path(get, path="/api/study/reports/{id}/assessment",
    params(("id"=Uuid, Path), AssessmentQuery),
    responses((status=200,body=AssessmentState),(status=404,body=ErrorBody)), tag="Study")]
async fn get_assessment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<AssessmentQuery>,
) -> ApiResult<Json<AssessmentState>> {
    let (session, _) = crate::study::report_checkpoint(&state.pool, id).await?;
    let language = crate::configuration::validation::enum_string(&query.language);
    Ok(Json(load(&state, id, &session, &language).await?))
}

#[utoipa::path(post, path="/api/study/reports/{id}/assessment",
    params(("id"=Uuid, Path), AssessmentQuery),
    request_body=StartAssessment,
    responses((status=200,body=AssessmentState),(status=404,body=ErrorBody),
        (status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=503,body=ErrorBody)), tag="Study")]
async fn start_assessment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<AssessmentQuery>,
    Json(request): Json<StartAssessment>,
) -> ApiResult<Json<AssessmentState>> {
    let (session, _) = crate::study::report_checkpoint(&state.pool, id).await?;
    if session.version != request.report_version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Learning report version changed".into(),
        ));
    }
    let language = crate::configuration::validation::enum_string(&query.language);
    let existing = load(&state, id, &session, &language).await?;
    if existing
        .job
        .as_ref()
        .is_some_and(|job| !matches!(job.status, AssessmentStatus::Failed))
    {
        return Ok(Json(existing));
    }
    match existing.availability {
        Availability::Unsupported => {
            return Err(ApiError::invalid("Assessment requires a point match"));
        }
        Availability::MissingAnalysis => {
            return Err(ApiError::invalid(
                "No evaluated decisions in the learning review",
            ));
        }
        Availability::NotConfigured => {
            return Err(ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "OpenAI is not configured".into(),
            ));
        }
        Availability::Ready => {}
    }
    sqlx::query(
        "INSERT INTO match_assessments
            (id,report_id,report_version,language,prompt_version,model,status)
         VALUES ($1,$2,$3,$4,$5,$6,'queued')
         ON CONFLICT (report_id,report_version,language,prompt_version) DO UPDATE
            SET status='queued',error_code=NULL,model=EXCLUDED.model,created_at=now()
         WHERE match_assessments.status='failed'",
    )
    .bind(Uuid::new_v4())
    .bind(id)
    .bind(session.version)
    .bind(&language)
    .bind(PROMPT_VERSION)
    .bind(openai::model())
    .execute(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    state.assessment_wake.notify_one();
    Ok(Json(load(&state, id, &session, &language).await?))
}

async fn load(
    state: &AppState,
    id: Uuid,
    session: &PlaySession,
    language: &str,
) -> ApiResult<AssessmentState> {
    let row = sqlx::query_as::<_, JobRow>(
        "SELECT status,model,error_code,result FROM match_assessments
         WHERE report_id=$1 AND report_version=$2 AND language=$3 AND prompt_version=$4",
    )
    .bind(id)
    .bind(session.version)
    .bind(language)
    .bind(PROMPT_VERSION)
    .fetch_optional(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    Ok(AssessmentState {
        availability: availability(session),
        abbreviated_match: session.match_forfeited,
        job: row.map(Into::into),
    })
}

#[derive(sqlx::FromRow)]
struct Claimed {
    id: Uuid,
    report_id: Uuid,
    report_version: i64,
    language: String,
    model: String,
}

pub async fn worker(state: AppState) {
    while !state.shutdown.is_cancelled() {
        let result = tokio::select! {
            _ = state.shutdown.cancelled() => break,
            result = async {
                let Ok(_access) = state.backup.requests.try_read() else { return Ok(false) };
                run_one(&state).await
            } => result,
        };
        match result {
            Ok(true) => continue,
            Ok(false) => {}
            Err(error) => tracing::error!(%error, "Assessment worker database error"),
        }
        tokio::select! {
            _ = state.shutdown.cancelled() => break,
            _ = state.assessment_wake.notified() => {},
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {},
        }
    }
}

async fn run_one(state: &AppState) -> anyhow::Result<bool> {
    // An expired request may already have been billed. Retry only on explicit user action.
    sqlx::query(
        "UPDATE match_assessments SET status='failed',error_code='interrupted',
             lease_token=NULL,lease_until=NULL WHERE status='running' AND lease_until<=now()",
    )
    .execute(&state.pool)
    .await?;
    let token = Uuid::new_v4();
    let job = sqlx::query_as::<_, Claimed>(
        "UPDATE match_assessments SET status='running',lease_token=$1,
             lease_until=now()+interval '120 seconds' WHERE id=(
             SELECT id FROM match_assessments WHERE status='queued'
             ORDER BY created_at,id LIMIT 1 FOR UPDATE SKIP LOCKED)
         RETURNING id,report_id,report_version,language,model",
    )
    .bind(token)
    .fetch_optional(&state.pool)
    .await?;
    let Some(job) = job else { return Ok(false) };
    let (status, result, error) = match execute(state, &job).await {
        Ok(output) => ("completed", Some(DbJson(output)), None),
        Err(code) => ("failed", None, Some(code)),
    };
    sqlx::query(
        "UPDATE match_assessments SET status=$3,result=$4,error_code=$5,
             lease_token=NULL,lease_until=NULL
         WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now()",
    )
    .bind(job.id)
    .bind(token)
    .bind(status)
    .bind(result)
    .bind(error)
    .execute(&state.pool)
    .await?;
    Ok(true)
}

async fn execute(state: &AppState, job: &Claimed) -> Result<Assessment, &'static str> {
    if !openai::configured() {
        return Err("not_configured");
    }
    let (session, sgf) = crate::study::report_checkpoint(&state.pool, job.report_id)
        .await
        .map_err(|_| "source_unavailable")?;
    if session.version != job.report_version {
        return Err("source_unavailable");
    }
    let language = job.language.clone();
    let input = tokio::task::spawn_blocking(move || evidence::build(&session, &sgf, &language))
        .await
        .map_err(|_| "source_unavailable")?
        .map_err(|_| "source_unavailable")?;
    let output = openai::generate(&job.model, &input).await?;
    evidence::validate(&output, &input)
        .then_some(output)
        .ok_or("invalid_report")
}

#[derive(OpenApi)]
#[openapi(
    paths(get_assessment, start_assessment),
    components(schemas(
        AssessmentState,
        AssessmentJob,
        Availability,
        AssessmentStatus,
        Failure,
        Assessment,
        Topic,
        TopicName,
        KeyDecision,
        Training,
        StartAssessment
    ))
)]
pub(crate) struct AssessmentDoc;
