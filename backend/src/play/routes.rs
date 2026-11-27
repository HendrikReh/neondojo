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
        crate::backgammon::checker_paths(&request.board, request.player, request.dice).map(
            |paths| {
                paths
                    .into_iter()
                    .map(|path| CheckerTurn::from_path(path, request.player))
                    .collect()
            },
        )
    })
    .await
    .map_err(anyhow::Error::from)?;
    result
        .map(Json)
        .map_err(|error| ApiError::invalid(error.to_string()))
}

#[utoipa::path(get,path="/api/play/sessions",responses((status=200,body=Vec<PlaySession>)),tag="Playing")]
pub(super) async fn list_sessions(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<PlaySession>>> {
    let rows = recent_sessions(&state.pool).await?;
    let mut sessions = Vec::with_capacity(rows.len());
    for (session, sgf) in rows {
        // Keep the list bounded: only one source replay is queued at a time.
        sessions.push(present_with_results(session, sgf).await?);
    }
    Ok(Json(sessions))
}

#[utoipa::path(get,path="/api/play/sessions/{id}",params(("id"=Uuid,Path)),responses((status=200,body=PlaySession),(status=404,body=ErrorBody)),tag="Playing")]
pub(super) async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<PlaySession>> {
    let row = saved(&state, id).await?;
    let session = present_with_results(row.state.0, row.sgf).await?;
    open_sage::schedule_warmup(&session, &state);
    Ok(Json(session))
}

#[utoipa::path(get,path="/api/play/sessions/{id}/history",params(("id"=Uuid,Path),("version"=i64,Query,description="Saved session revision")),responses((status=200,body=PlayHistorySnapshots),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Playing")]
pub(super) async fn history_snapshots(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<PlayHistorySnapshots>> {
    // Fetch state and its exact raw checkpoint together. No archive lookup or
    // native replay: detached sessions and unavailable engines remain readable.
    let checkpoint = saved(&state, id).await?;
    if checkpoint.state.version != query.version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Session changed; refresh history".into(),
        ));
    }
    let response = tokio::task::spawn_blocking(move || {
        build_history_snapshots(&checkpoint.state, &checkpoint.sgf)
    })
    .await
    .map_err(anyhow::Error::from)??;
    Ok(Json(response))
}

#[utoipa::path(post,path="/api/play/sessions",request_body=CreatePlaySession,responses((status=201,body=PlaySession),(status=409,body=ErrorBody,description="A benchmark is using the engines"),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Playing")]
pub(super) async fn create_session(
    State(state): State<AppState>,
    Json(request): Json<CreatePlaySession>,
) -> ApiResult<(StatusCode, Json<PlaySession>)> {
    let session = crate::engine::try_play_start(start_session(state, request))
        .await
        .ok_or_else(benchmark_conflict)??;
    Ok((StatusCode::CREATED, Json(session)))
}

#[utoipa::path(post,path="/api/play/sessions/{id}/mode",params(("id"=Uuid,Path)),request_body=PlayModeRequest,responses((status=200,body=PlaySession),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Playing")]
pub(super) async fn change_mode(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<PlayModeRequest>,
) -> ApiResult<Json<PlaySession>> {
    let previous = saved(&state, id).await?;
    let mut session = previous.state.0;
    check_session_version(
        request.version,
        session.version,
        "Session changed; reload before changing playing mode",
    )?;
    pause::require_active(&session)?;
    if matches!(session.phase, PlayPhase::MatchOver) || reports::money_finished(&session) {
        return Err(ApiError::invalid(
            "Completed matches keep their playing mode",
        ));
    }
    if let Some(threshold) = validate_tutor_threshold(request.tutor_threshold)? {
        session.tutor_threshold = threshold;
    }
    session.learning_used |=
        session.play_mode != PlayMode::Competition || request.play_mode != PlayMode::Competition;
    session.game_learning_used |=
        session.play_mode != PlayMode::Competition || request.play_mode != PlayMode::Competition;
    session.play_mode = request.play_mode;
    if let Some(pending) = session.pending_decision.as_mut() {
        if session.play_mode == PlayMode::Coach
            || (session.play_mode == PlayMode::Training && session.training_auto_analysis)
        {
            pending.revealed = true;
        }
        pending.warning = session.play_mode == PlayMode::Tutor
            && pending.action.is_some()
            && pending
                .loss
                .is_some_and(|loss| loss >= session.tutor_threshold);
    }
    session.version += 1;
    persist_session(
        &state,
        &session,
        &previous.sgf,
        &previous.native_id,
        request.version,
        false,
    )
    .await
}

#[utoipa::path(post,path="/api/play/sessions/{id}/actions",params(("id"=Uuid,Path)),request_body=PlayActionRequest,responses((status=200,body=PlaySession),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Playing")]
pub(super) async fn act(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<PlayActionRequest>,
) -> ApiResult<Json<PlaySession>> {
    perform_action(&state, id, request).await
}

#[utoipa::path(post,path="/api/play/sessions/{id}/feedback",params(("id"=Uuid,Path)),request_body=PlayFeedbackRequest,responses((status=200,body=PlaySession),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Playing")]
pub(super) async fn feedback(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<PlayFeedbackRequest>,
) -> ApiResult<Json<PlaySession>> {
    let previous = saved(&state, id).await?;
    let mut session = previous.state.0;
    check_session_version(
        request.version,
        session.version,
        "Session changed; reload before requesting feedback",
    )?;
    pause::require_active(&session)?;
    if session.play_mode == PlayMode::Competition {
        return Err(ApiError::invalid(
            "Feedback is available after a competition match",
        ));
    }
    if !(matches!(session.phase, PlayPhase::Move | PlayPhase::DoubleResponse)
        || matches!(session.phase, PlayPhase::Roll) && session.can_double)
    {
        return Err(ApiError::invalid(
            "No human decision available for feedback",
        ));
    }
    if let Some(id) = request.decision_id
        && session.pending_decision.as_ref().is_none_or(|d| d.id != id)
    {
        return Err(ApiError::invalid(
            "Feedback decision does not match pending decision",
        ));
    }
    session.game_learning_used = true;
    match request.operation {
        FeedbackOperation::Hint => {
            if session.pending_decision.is_none() {
                let alternatives = analyze_decision(&session, &previous.gnubg_path)
                    .await
                    .map_err(native_error)?;
                let mut decision = feedback::empty_decision(alternatives);
                decision.game_no = session.game_no;
                decision.history_index = session.history.len();
                session.pending_decision = Some(decision);
            }
            let pending = session
                .pending_decision
                .as_mut()
                .context("Hint analysis did not create a pending decision")?;
            pending.revealed = true;
            pending.assisted = true;
        }
        FeedbackOperation::Reveal => {
            let pending = session
                .pending_decision
                .as_mut()
                .ok_or_else(|| ApiError::invalid("No decision to reveal"))?;
            pending.revealed = true;
            pending.assisted = true;
        }
        FeedbackOperation::Retry => {
            let pending = session
                .pending_decision
                .as_mut()
                .ok_or_else(|| ApiError::invalid("No decision to retry"))?;
            if pending.action.is_none() {
                return Err(ApiError::invalid("Choose a move before retrying"));
            }
            pending.action = None;
            pending.move_index = None;
            pending.retry_count += 1;
            pending.assisted = true;
            pending.warning = false;
        }
    }
    session.version += 1;
    persist_session(
        &state,
        &session,
        &previous.sgf,
        &previous.native_id,
        request.version,
        false,
    )
    .await
}

#[utoipa::path(post,path="/api/play/sessions/{id}/review",params(("id"=Uuid,Path)),request_body=PlayReviewRequest,responses((status=200,body=PlaySession),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Playing")]
pub(super) async fn review(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<PlayReviewRequest>,
) -> ApiResult<Json<PlaySession>> {
    let previous = saved(&state, id).await?;
    let mut session = previous.state.0;
    check_session_version(
        request.version,
        session.version,
        "Session changed; reload before reviewing",
    )?;
    pause::require_active(&session)?;
    if !matches!(session.phase, PlayPhase::MatchOver) && !reports::money_finished(&session) {
        return Err(ApiError::invalid(
            "Complete the match or money game before reviewing",
        ));
    }
    if session.review.is_some() {
        return Ok(Json(present_with_results(session, previous.sgf).await?));
    }
    reports::build(&mut session, &previous.sgf, &previous.gnubg_path)
        .await
        .map_err(native_error)?;

    session.version += 1;
    persist_session(
        &state,
        &session,
        &previous.sgf,
        &previous.native_id,
        request.version,
        false,
    )
    .await
}

#[utoipa::path(get,path="/api/play/sessions/{id}/sgf",params(("id"=Uuid,Path)),responses((status=200,body=String,content_type="application/x-go-sgf"),(status=404,body=ErrorBody)),tag="Playing")]
pub(super) async fn export_sgf(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<([(header::HeaderName, String); 2], String)> {
    let row = saved(&state, id).await?;
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
        forfeit::annotate(
            &row.state,
            named_export(
                &if row.sgf.is_empty() {
                    format!(
                        "(;FF[4]GM[6]MI[length:{}][game:0][ws:0][bs:0]PW[White]PB[Black]RU[Crawford])\n",
                        row.state.match_length
                    )
                } else {
                    row.sgf
                },
                &row.state.human_name,
                row.state.human_player,
                row.state.bot_name(),
            ),
        ),
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
