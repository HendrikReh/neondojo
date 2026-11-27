use super::matches::{GameScope, limit, validate_game};
use super::{ApiError, ApiResult, AppState, ErrorBody};
use crate::json_errors::{Json, Path, Query};
use crate::{model::*, store};
use axum::{extract::State, http::StatusCode};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MatchAnalysisEngine {
    Gnubg,
    Bgsage,
}

impl MatchAnalysisEngine {
    pub(crate) fn id(&self) -> &'static str {
        crate::engines::EngineId::from(self).id()
    }
}

impl From<&MatchAnalysisEngine> for crate::engines::EngineId {
    fn from(engine: &MatchAnalysisEngine) -> Self {
        match engine {
            MatchAnalysisEngine::Gnubg => Self::Gnubg,
            MatchAnalysisEngine::Bgsage => Self::NeonGammon,
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub(super) struct MatchAnalysisRequest {
    /// Analyze one source game in its original match context; omitted means the whole match.
    game_no: Option<i32>,
    /// Provider for this job; omitted uses the saved analysis engine.
    engine_id: Option<MatchAnalysisEngine>,
    /// Verified checker and cube depth of the selected engine. Omitted uses saved analysis depth.
    #[serde(default)]
    #[schema(minimum = 1, maximum = 5)]
    plies: Option<i32>,
}

#[utoipa::path(post, path="/api/matches/{id}/analysis", params(("id"=Uuid, Path)), request_body=MatchAnalysisRequest, responses((status=202, body=Job),(status=404,body=ErrorBody),(status=422,body=ErrorBody)), tag="Match analysis")]
pub(super) async fn create_match_analysis(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    payload: Result<Json<MatchAnalysisRequest>, ApiError>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    crate::play::require_review_available(&state.pool, id).await?;
    let Json(request) = payload?;
    let source = store::get_match(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    if !source.source_available {
        return Err(ApiError::invalid(
            "The source match was removed; analyze the saved position instead",
        ));
    }
    let engine_id = request.engine_id.as_ref().map(MatchAnalysisEngine::id);
    let (settings, snapshot) =
        crate::settings::snapshot_for_analysis(&state, request.plies, engine_id)
            .await
            .map_err(|e| ApiError::invalid(format!("Engine unavailable: {e:#}")))?;
    let plies = request.plies.unwrap_or(settings.analysis_plies);
    if !crate::configuration::engine_depths(&snapshot.engine_id, "analysis").contains(&plies) {
        return Err(ApiError::invalid(
            "Depth is not verified for the selected engine",
        ));
    }
    let games = validate_game(&state, id, request.game_no).await?;
    let total = if request.game_no.is_some() {
        1
    } else {
        games.len() as i32
    };
    let job = store::create_match_job_scoped_with_snapshot(
        &state.pool,
        id,
        plies,
        total,
        request.game_no,
        Some(&snapshot),
    )
    .await?;
    state.wake.notify_one();
    Ok((StatusCode::ACCEPTED, Json(job)))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct MatchAnalysisQuery {
    /// One-based source game number; omitted means the whole match.
    game_no: Option<i32>,
    /// Provider whose status to show; omitted uses the saved analysis engine.
    engine_id: Option<MatchAnalysisEngine>,
}

#[utoipa::path(get, path="/api/matches/{id}/analysis", params(("id"=Uuid, Path), MatchAnalysisQuery), responses((status=200, body=Option<Job>,description="Latest analysis job for the selected engine and scope, or null if none has been submitted. Includes persisted partial summary during execution."),(status=404,body=ErrorBody),(status=422,body=ErrorBody)), tag="Match analysis")]
pub(super) async fn latest_match_analysis(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    query: Result<Query<MatchAnalysisQuery>, ApiError>,
) -> ApiResult<Json<Option<Job>>> {
    crate::play::require_review_available(&state.pool, id).await?;
    let Query(query) = query?;
    let game_no = query.game_no;
    validate_game(&state, id, game_no).await?;
    Ok(Json(
        store::latest_match_job_scoped(
            &state.pool,
            id,
            game_no,
            query.engine_id.as_ref().map(MatchAnalysisEngine::id),
        )
        .await?,
    ))
}

#[utoipa::path(get, path="/api/matches/{id}/analysis/jobs", params(("id"=Uuid, Path), ("game_no"=Option<i32>, Query, description="One-based source game number; omitted means the whole match.")), responses((status=200, body=Vec<AnalysisComparisonRun>,description="Current analysis jobs for the selected game or whole match, newest first. For a selected game, includes its checkpoint from completed whole-match analyses. Returns the latest job for each provider, depth, status and source scope."),(status=404,body=ErrorBody),(status=422,body=ErrorBody)), tag="Match analysis")]
pub(super) async fn match_analysis_jobs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    query: Result<Query<GameScope>, ApiError>,
) -> ApiResult<Json<Vec<AnalysisComparisonRun>>> {
    crate::play::require_review_available(&state.pool, id).await?;
    let Query(query) = query?;
    if query.game_no.is_some() {
        validate_game(&state, id, query.game_no).await?;
    } else {
        store::get_match(&state.pool, id)
            .await?
            .ok_or_else(ApiError::missing)?;
    }
    let mut runs = store::match_analysis_jobs_for_scope(&state.pool, id, query.game_no).await?;
    if let Some(game_no) = query.game_no {
        for run in runs
            .iter_mut()
            .filter(|run| run.job.game_no.is_none() && run.game_summary.is_none())
        {
            let statistics =
                crate::analysis_statistics::load_statistics(&state, run.job.id).await?;
            run.game_summary = statistics
                .games
                .into_iter()
                .find(|game| {
                    game.game_no == game_no && game.analyzed && game.error_breakdown.is_some()
                })
                .and_then(|game| game.summary);
        }
    }
    Ok(Json(runs))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct MoveAnalysisQuery {
    /// Restrict decisions to one source game.
    game_no: Option<i32>,
    /// Continue after a move-analysis row ID, keeping the same filters.
    after: Option<i64>,
    /// Page size 1–100, defaults to 50.
    limit: Option<i64>,
    /// Exclude forced/unavailable decisions and apply the configured Questionable threshold.
    #[serde(default)]
    errors_only: bool,
    /// 0 = White, 1 = Black.
    player: Option<i32>,
    /// checker or cube; omitted means both.
    kind: Option<String>,
    /// Exact error severity; combined with all other filters.
    severity: Option<MoveSeverity>,
}

#[utoipa::path(get, path="/api/jobs/{id}/moves", params(("id"=Uuid, Path),MoveAnalysisQuery), responses((status=200,body=MoveAnalysisPage),(status=404,body=ErrorBody),(status=422,body=ErrorBody)), tag="Match analysis")]
pub(super) async fn move_analyses(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<MoveAnalysisQuery>,
) -> ApiResult<Json<MoveAnalysisPage>> {
    let job = store::get_job(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    crate::play::require_job_review_available(&state.pool, &job).await?;
    if job.kind != "match" {
        return Err(ApiError::invalid("This job is not a match analysis"));
    }
    if query.after.is_some_and(|n| n < 0)
        || query.player.is_some_and(|n| !(0..=1).contains(&n))
        || query
            .kind
            .as_deref()
            .is_some_and(|k| k != "checker" && k != "cube")
    {
        return Err(ApiError::invalid("Invalid analysis cursor, player or kind"));
    }
    if let Some(game_no) = query.game_no {
        if job.game_no.is_some_and(|n| n != game_no) {
            return Err(ApiError::invalid("game_no does not match this job's scope"));
        }
        validate_game(
            &state,
            job.match_uuid.ok_or_else(ApiError::missing)?,
            Some(game_no),
        )
        .await?;
    }
    let thresholds = crate::configuration::store::error_thresholds(&state.pool).await?;
    if let Some(after) = query.after.filter(|n| *n > 0) {
        let row: Option<(Uuid, i32, i32, String, f64, String)> = sqlx::query_as(
            "SELECT job_id,game_no,player,kind,equity_loss,COALESCE(data->>'evaluation_status','') FROM move_analyses WHERE id=$1",
        )
        .bind(after)
        .fetch_optional(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;
        let Some((job_id, game_no, player, kind, loss, evaluation_status)) = row else {
            return Err(ApiError::invalid("Unknown cursor"));
        };
        let severity = if matches!(evaluation_status.as_str(), "forced" | "unavailable") {
            MoveSeverity::None
        } else {
            thresholds.classify(loss)
        };
        if job_id != id
            || query.game_no.is_some_and(|n| n != game_no)
            || query.player.is_some_and(|n| n != player)
            || query.kind.as_ref().is_some_and(|k| *k != kind)
            || (query.errors_only && severity == MoveSeverity::None)
            || query.severity.is_some_and(|grade| grade != severity)
        {
            return Err(ApiError::invalid("Cursor does not match the filters"));
        }
    }
    Ok(Json(
        store::move_analyses_with_thresholds(
            &state.pool,
            id,
            query.after.unwrap_or(0),
            limit(query.limit)?,
            query.errors_only,
            query.player,
            query.kind.as_deref(),
            query.game_no,
            query.severity.map(MoveSeverity::as_str),
            thresholds,
        )
        .await?,
    ))
}
