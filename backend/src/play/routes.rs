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
    payload: Result<Json<CheckerTurnsRequest>, JsonRejection>,
) -> ApiResult<Json<Vec<CheckerTurn>>> {
    let Json(request) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    let result = tokio::task::spawn_blocking(move || {
        crate::import::sgf::checker_paths(&request.board, request.player, request.dice).map(
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
    let rows: Vec<(sqlx::types::Json<PlaySession>, String)> = sqlx::query_as(&format!(
        "SELECT {SAVED_STATE},sgf FROM play_sessions ORDER BY updated_at DESC,id DESC LIMIT 100"
    ))
    .fetch_all(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    let mut sessions = Vec::with_capacity(rows.len());
    for (session, sgf) in rows {
        // Keep the list bounded: only one source replay is queued at a time.
        sessions.push(present_with_results(session.0, sgf).await?);
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
    open_sage::schedule_warmup(&session, &state.shutdown);
    Ok(Json(session))
}

#[utoipa::path(get,path="/api/play/sessions/{id}/history",params(("id"=Uuid,Path),("version"=i64,Query,description="Saved session revision")),responses((status=200,body=PlayHistorySnapshots),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Playing")]
pub(super) async fn history_snapshots(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    crate::json_errors::Query(query): crate::json_errors::Query<HistoryQuery>,
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

#[utoipa::path(post,path="/api/play/sessions",request_body=CreatePlaySession,responses((status=201,body=PlaySession),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Playing")]
pub(super) async fn create_session(
    State(state): State<AppState>,
    crate::json_errors::Json(request): crate::json_errors::Json<CreatePlaySession>,
) -> ApiResult<(StatusCode, Json<PlaySession>)> {
    if !(0..=1).contains(&request.human_player) || !(0..=25).contains(&request.match_length) {
        return Err(ApiError::invalid(
            "human_player must be 0 or 1 and match_length must be 0 (money game) or 1..25",
        ));
    }
    let (mut settings, error_thresholds) = crate::settings::load_play_settings(&state).await?;
    request.apply_match_choices(&mut settings)?;
    if request
        .tutor_threshold
        .is_some_and(|v| !v.is_finite() || v <= 0.0 || v > 1.0)
    {
        return Err(ApiError::invalid(
            "tutor_threshold must be greater than 0 and at most 1",
        ));
    }
    let pinned_path = resolve_binary(&settings.gnubg_path).map_err(native_error)?;
    let engine = Engine::new(pinned_path.clone());
    let rules_version = engine.version().await.map_err(native_error)?;
    let rules = crate::settings::EngineSnapshot {
        engine_id: EngineId::Gnubg.id().into(),
        gnubg_path: pinned_path.clone(),
        engine_version: Some(rules_version),
        // The rules process plays the bot's turns itself when the bot is GNUbg.
        gnubg_threads: settings.gnubg_play_threads.filter(|_| {
            EngineId::parse(&settings.play_engine).is_some_and(EngineId::is_rules_engine)
        }),
        gnubg_move_filter: None,
        open_sage: None,
    };
    let bot = crate::settings::snapshot_for(
        &state,
        &settings.play_engine,
        &pinned_path,
        settings.gnubg_play_threads,
        settings.sage_play_threads(),
        settings.play_plies,
    )
    .await
    .map_err(native_error)?;
    if request.play_difficulty.is_some()
        && !bot
            .open_sage
            .as_ref()
            .is_some_and(crate::engines::SageSnapshot::supports_difficulty_presets)
    {
        return Err(ApiError::invalid(
            "NeonGammon difficulty presets require NeonGammon 3.1.0 or newer",
        ));
    }
    let analysis = crate::settings::snapshot_for(
        &state,
        &settings.analysis_engine,
        &pinned_path,
        settings.gnubg_threads,
        settings.sage_analysis_threads(),
        settings.analysis_plies,
    )
    .await
    .map_err(native_error)?;
    let version = bot
        .engine_version
        .clone()
        .context("Playing engine has no verified version")
        .map_err(native_error)?;
    let id = Uuid::new_v4();
    let mut session = PlaySession {
        paused: false,
        engine_snapshots: Some(PlayEngineSnapshots {
            rules,
            bot,
            analysis,
        }),
        money_session_ended: false,
        match_forfeited: false,
        game_results: None,
        study_report_id: None,
        study_report_version: None,
        review_game_no: None,
        game_learning_used: request.play_mode.unwrap_or(settings.default_play_mode)
            != PlayMode::Competition,
        library_match_id: None,
        learning_used: request.play_mode.unwrap_or(settings.default_play_mode)
            != PlayMode::Competition,
        play_mode: request.play_mode.unwrap_or(settings.default_play_mode),
        tutor_threshold: request.tutor_threshold.unwrap_or(error_thresholds.error),
        error_thresholds,
        training_auto_analysis: request
            .training_auto_analysis
            .unwrap_or(settings.training_auto_analysis),
        analysis_plies: settings.analysis_plies,
        pending_decision: None,
        review: None,
        decisions: vec![],
        id,
        version: 0,
        human_player: request.human_player,
        human_name: if settings.player_name.is_empty() {
            "Human".into()
        } else {
            settings.player_name
        },
        match_length: request.match_length,
        bearoff_direction: settings.bearoff_direction,
        engine: PlayEngine {
            id: settings.play_engine,
            version,
            plies: settings.play_plies,
            top_two: settings.play_top_two,
            difficulty: request.play_difficulty,
        },
        position: Position {
            id,
            match_uuid: id,
            ordinal: 0,
            game_no: 1,
            ply: 0,
            player: request.human_player,
            dice: vec![],
            board: vec![],
            position_id: String::new(),
            gnubg_match_id: String::new(),
            notation: String::new(),
        },
        phase: PlayPhase::OpeningRoll,
        can_double: false,
        legal_moves: vec![],
        legal_turns: None,
        history: vec![],
        scores: [0, 0],
        game_no: 1,
        winner: None,
        opening: None,
    };
    probe_native(&session.rules_engine(&pinned_path)?)
        .await
        .map_err(native_error)?;
    stage_opening(&mut session, false)?;
    let sgf = String::new();
    let native_id = String::new();
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    library::lock_bot_database(&mut tx).await?;
    sqlx::query(
        "INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(id)
    .bind(sqlx::types::Json(&session))
    .bind(pinned_path)
    .bind(&sgf)
    .bind(native_id)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    session.library_match_id = Some(
        library::sync(
            &mut tx,
            &session,
            &sgf,
            library::ArchiveTarget::NewIn(BOT_DATABASE_ID),
        )
        .await?,
    );
    sqlx::query("UPDATE play_sessions SET state=$2 WHERE id=$1")
        .bind(id)
        .bind(sqlx::types::Json(&session))
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    open_sage::schedule_warmup(&session, &state.shutdown);
    Ok((StatusCode::CREATED, Json(session)))
}

pub(super) fn native_error(error: anyhow::Error) -> ApiError {
    tracing::warn!(error=?error,"Play action failed; no session changes saved");
    ApiError(
        StatusCode::SERVICE_UNAVAILABLE,
        format!(
            "The engines could not complete this action; the saved session is unchanged. {error}"
        ),
    )
}

#[utoipa::path(post,path="/api/play/sessions/{id}/mode",params(("id"=Uuid,Path)),request_body=PlayModeRequest,responses((status=200,body=PlaySession),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Playing")]
pub(super) async fn change_mode(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    crate::json_errors::Json(request): crate::json_errors::Json<PlayModeRequest>,
) -> ApiResult<Json<PlaySession>> {
    let previous = saved(&state, id).await?;
    let mut session = previous.state.0;
    if request.version != session.version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Session changed; reload before changing playing mode".into(),
        ));
    }
    if request.version < 0 || request.version == i64::MAX {
        return Err(ApiError::invalid("Invalid session version"));
    }
    pause::require_active(&session)?;
    if matches!(session.phase, PlayPhase::MatchOver) || reports::money_finished(&session) {
        return Err(ApiError::invalid(
            "Completed matches keep their playing mode",
        ));
    }
    if let Some(threshold) = request.tutor_threshold {
        if !threshold.is_finite() || threshold <= 0.0 || threshold > 1.0 {
            return Err(ApiError::invalid(
                "tutor_threshold must be greater than 0 and at most 1",
            ));
        }
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
    crate::json_errors::Json(request): crate::json_errors::Json<PlayActionRequest>,
) -> ApiResult<Json<PlaySession>> {
    let previous = saved(&state, id).await?;
    if request.version != previous.state.version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Session changed; reload before playing".into(),
        ));
    }
    if request.version < 0 || request.version == i64::MAX {
        return Err(ApiError::invalid("Invalid session version"));
    }
    if matches!(request.action, PlayAction::Pause | PlayAction::Resume) {
        if request.move_index.is_some()
            || request.resign_level.is_some()
            || request.decision_id.is_some()
        {
            return Err(ApiError::invalid(
                "Pausing and resuming only require a session version",
            ));
        }
        return pause::transition(
            &state,
            previous,
            matches!(request.action, PlayAction::Pause),
        )
        .await;
    }
    if matches!(request.action, PlayAction::ForfeitMatch) {
        return forfeit::finish(&state, previous, &request).await;
    }
    let (action, notation) = validate_action(&previous.state, &request)?;
    let mut working = previous.state.0.clone();
    working.learning_used |= working.play_mode != PlayMode::Competition;
    working.game_learning_used |= working.play_mode != PlayMode::Competition;
    if matches!(request.action, PlayAction::EndSession) {
        if working.review.is_none() {
            reports::build(&mut working, &previous.sgf, &previous.gnubg_path)
                .await
                .map_err(native_error)?;
        }
        working.phase = PlayPhase::MatchOver;
        working.money_session_ended = true;
        working.version += 1;
        return persist_session(
            &state,
            &working,
            &previous.sgf,
            &previous.native_id,
            request.version,
            false,
        )
        .await;
    }
    if let Some(pending) = &working.pending_decision {
        if let Some(choice) = &pending.action {
            if request.decision_id != Some(pending.id)
                || choice != action
                || pending.move_index != request.move_index
            {
                return Err(ApiError::invalid(
                    "Continue or retry the pending decision before playing",
                ));
            }
        } else if request.decision_id.is_some() {
            return Err(ApiError::invalid("Choose a decision before continuing"));
        }
    } else if request.decision_id.is_some() {
        return Err(ApiError::invalid("No pending decision to continue"));
    }
    let is_decision = matches!(
        request.action,
        PlayAction::Move
            | PlayAction::Pass
            | PlayAction::Double
            | PlayAction::Take
            | PlayAction::Drop
    ) || (matches!(request.action, PlayAction::Roll) && working.can_double);
    if is_decision
        && request.decision_id.is_none()
        && (working.play_mode != PlayMode::Competition || working.pending_decision.is_some())
    {
        let alternatives = if let Some(pending) = &working.pending_decision {
            pending.alternatives.clone()
        } else {
            analyze_decision(&working, &previous.gnubg_path)
                .await
                .map_err(native_error)?
        };
        stage_choice(&mut working, action, request.move_index, alternatives)?;
        if should_pause_for_feedback(&working, &request.action) {
            working.version += 1;
            return persist_session(
                &state,
                &working,
                &previous.sgf,
                &previous.native_id,
                request.version,
                false,
            )
            .await;
        }
    }
    // Native action is the sole commit point. A failed process leaves the
    // original pending checkpoint and its version untouched.
    let mut completed_report = None;
    let (mut session, sgf, native_id) = if matches!(request.action, PlayAction::NextGame) {
        if reports::money_finished(&working) {
            if working.review.is_none() {
                reports::build(&mut working, &previous.sgf, &previous.gnubg_path)
                    .await
                    .map_err(native_error)?;
            }
            completed_report = Some(working.clone());
            working.review = None;
            working.study_report_id = None;
            working.study_report_version = None;
            working.review_game_no = None;
            working.game_learning_used = working.play_mode != PlayMode::Competition;
        }
        working.version += 1;
        stage_opening(&mut working, true)?;
        (working, previous.sgf, previous.native_id)
    } else {
        let checkpoint = if previous.sgf.is_empty() {
            None
        } else {
            Some((previous.sgf.as_str(), previous.native_id.as_str()))
        };
        let (snapshot, sgf) = native(
            &working,
            &previous.gnubg_path,
            checkpoint,
            action,
            notation,
            request.resign_level,
        )
        .await
        .map_err(native_error)?;
        if snapshot.resignation_declined {
            return Err(ApiError::invalid(
                "The bot declined this resignation. Choose a higher level or continue playing; the saved game is unchanged",
            ));
        }
        working.version += 1;
        let native_id = apply_snapshot(&mut working, snapshot)?;
        (working, sgf, native_id)
    };
    // Resignation abandons a hint/retry checkpoint; its checker/cube choice
    // was never played and must not become a committed learning decision.
    if let Some(mut decision) = session.pending_decision.take()
        && !matches!(request.action, PlayAction::Resign)
    {
        decision.committed = true;
        session.decisions.push(decision);
    }
    persist_session_with_report(
        &state,
        &session,
        &sgf,
        &native_id,
        request.version,
        true,
        completed_report.as_ref(),
    )
    .await
}

#[utoipa::path(post,path="/api/play/sessions/{id}/feedback",params(("id"=Uuid,Path)),request_body=PlayFeedbackRequest,responses((status=200,body=PlaySession),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Playing")]
pub(super) async fn feedback(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    crate::json_errors::Json(request): crate::json_errors::Json<PlayFeedbackRequest>,
) -> ApiResult<Json<PlaySession>> {
    let previous = saved(&state, id).await?;
    let mut session = previous.state.0;
    if request.version != session.version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Session changed; reload before requesting feedback".into(),
        ));
    }
    if request.version < 0 || request.version == i64::MAX {
        return Err(ApiError::invalid("Invalid session version"));
    }
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
            let pending = session.pending_decision.as_mut().unwrap();
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
    crate::json_errors::Json(request): crate::json_errors::Json<PlayReviewRequest>,
) -> ApiResult<Json<PlaySession>> {
    let previous = saved(&state, id).await?;
    let mut session = previous.state.0;
    if request.version != session.version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Session changed; reload before reviewing".into(),
        ));
    }
    if request.version < 0 || request.version == i64::MAX {
        return Err(ApiError::invalid("Invalid session version"));
    }
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
