use super::*;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/play/checker-turns", post(checker_turns))
        .route(
            "/api/play/sessions",
            get(list_sessions).post(create_session),
        )
        .route("/api/play/sessions/{id}", get(get_session))
        .route("/api/play/sessions/{id}/actions", post(act))
        .route("/api/play/sessions/{id}/mode", post(change_mode))
        .route("/api/play/sessions/{id}/feedback", post(feedback))
        .route("/api/play/sessions/{id}/review", post(review))
        .route("/api/play/sessions/{id}/sgf", get(export_sgf))
        .route("/api/play/sessions/{id}/history", get(history_snapshots))
}

#[utoipa::path(post,path="/api/play/checker-turns",request_body=CheckerTurnsRequest,responses((status=200,body=Vec<CheckerTurn>,description="Complete paths in absolute coordinates. An empty list means a forced pass."),(status=422,body=ErrorBody)),tag="Playing")]
pub(super) async fn checker_turns(
    Json(request): Json<CheckerTurnsRequest>,
) -> ApiResult<Json<Vec<CheckerTurn>>> {
    let result = tokio::task::spawn_blocking(move || {
        compute_checker_turns(&request.board, request.player, request.dice)
    })
    .await
    .map_err(anyhow::Error::from)?;
    Ok(Json(result?))
}

#[utoipa::path(get,path="/api/play/sessions",responses((status=200,body=Vec<PlaySession>)),tag="Playing")]
pub(super) async fn list_sessions(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<PlaySession>>> {
    Ok(Json(list_sessions_service(&state).await?))
}

#[utoipa::path(get,path="/api/play/sessions/{id}",params(("id"=Uuid,Path)),responses((status=200,body=PlaySession),(status=404,body=ErrorBody)),tag="Playing")]
pub(super) async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<PlaySession>> {
    Ok(Json(get_session_service(&state, id).await?))
}

#[utoipa::path(get,path="/api/play/sessions/{id}/history",params(("id"=Uuid,Path),("version"=i64,Query,description="Saved session revision")),responses((status=200,body=PlayHistorySnapshots),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Playing")]
pub(super) async fn history_snapshots(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<PlayHistorySnapshots>> {
    Ok(Json(
        history_snapshots_service(&state, id, query.version).await?,
    ))
}

#[utoipa::path(post,path="/api/play/sessions",request_body=CreatePlaySession,responses((status=201,body=PlaySession),(status=409,body=ErrorBody,description="A benchmark is using the engines"),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Playing")]
pub(super) async fn create_session(
    State(state): State<AppState>,
    Json(request): Json<CreatePlaySession>,
) -> ApiResult<(StatusCode, Json<PlaySession>)> {
    Ok((
        StatusCode::CREATED,
        Json(create_session_service(&state, request).await?),
    ))
}

#[utoipa::path(post,path="/api/play/sessions/{id}/mode",params(("id"=Uuid,Path)),request_body=PlayModeRequest,responses((status=200,body=PlaySession),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Playing")]
pub(super) async fn change_mode(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<PlayModeRequest>,
) -> ApiResult<Json<PlaySession>> {
    Ok(Json(change_mode_service(&state, id, request).await?))
}

#[utoipa::path(post,path="/api/play/sessions/{id}/actions",params(("id"=Uuid,Path)),request_body=PlayActionRequest,responses((status=200,body=PlaySession),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Playing")]
pub(super) async fn act(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<PlayActionRequest>,
) -> ApiResult<Json<PlaySession>> {
    Ok(Json(act_in_session_service(&state, id, request).await?))
}

#[utoipa::path(post,path="/api/play/sessions/{id}/feedback",params(("id"=Uuid,Path)),request_body=PlayFeedbackRequest,responses((status=200,body=PlaySession),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Playing")]
pub(super) async fn feedback(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<PlayFeedbackRequest>,
) -> ApiResult<Json<PlaySession>> {
    Ok(Json(update_feedback_service(&state, id, request).await?))
}

#[utoipa::path(post,path="/api/play/sessions/{id}/review",params(("id"=Uuid,Path)),request_body=PlayReviewRequest,responses((status=200,body=PlaySession),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Playing")]
pub(super) async fn review(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<PlayReviewRequest>,
) -> ApiResult<Json<PlaySession>> {
    Ok(Json(review_session_service(&state, id, request).await?))
}

#[utoipa::path(get,path="/api/play/sessions/{id}/sgf",params(("id"=Uuid,Path)),responses((status=200,body=String,content_type="application/x-go-sgf"),(status=404,body=ErrorBody)),tag="Playing")]
pub(super) async fn export_sgf(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<([(header::HeaderName, String); 2], String)> {
    let sgf = export_session_sgf_service(&state, id).await?;
    Ok((
        [
            (
                header::CONTENT_TYPE,
                "application/x-go-sgf; charset=utf-8".into(),
            ),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"neondojo-{id}.sgf\""),
            ),
        ],
        sgf,
    ))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        checker_turns,
        list_sessions,
        get_session,
        history_snapshots,
        create_session,
        act,
        change_mode,
        feedback,
        review,
        export_sgf
    ),
    components(schemas(
        PlaySession,
        PlayGameResult,
        crate::model::GameResult,
        crate::model::GameResultReason,
        PlayEngine,
        PlayMove,
        PlayStep,
        CheckerTurnsRequest,
        CheckerTurn,
        LiveCheckerTurn,
        PlayHistory,
        PlayHistorySnapshot,
        PlayHistorySnapshots,
        PlayPhase,
        OpeningRoll,
        CreatePlaySession,
        PlayActionRequest,
        PlayModeRequest,
        PlayAction,
        PlayMode,
        PlayAlternative,
        PlayAttempt,
        PlayDecision,
        PlayFeedbackRequest,
        FeedbackOperation,
        PlayReview,
        PlayReviewRequest
    ))
)]
pub(crate) struct PlayDoc;
