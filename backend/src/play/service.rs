use super::*;

pub(super) fn benchmark_conflict() -> ApiError {
    ApiError(
        StatusCode::CONFLICT,
        "A benchmark is using the engines; retry after it finishes".into(),
    )
}

pub(super) fn native_error(error: anyhow::Error) -> ApiError {
    tracing::warn!(error=?error,"Play action failed; no session changes saved");
    ApiError(
        StatusCode::SERVICE_UNAVAILABLE,
        format!(
            "The engines could not complete this action; the saved session is unchanged. {error}"
        )
        .into(),
    )
}

pub(super) fn check_session_version(
    expected: i64,
    actual: i64,
    conflict_message: &str,
) -> ApiResult<()> {
    if expected != actual {
        return Err(ApiError(StatusCode::CONFLICT, conflict_message.into()));
    }
    if expected < 0 || expected == i64::MAX {
        return Err(ApiError::invalid("Invalid session version"));
    }
    Ok(())
}

pub(super) fn validate_tutor_threshold(threshold: Option<f64>) -> ApiResult<Option<f64>> {
    if let Some(value) = threshold {
        if !crate::settings::valid_tutor_threshold(value) {
            return Err(ApiError::invalid(
                "tutor_threshold must be greater than 0 and at most 1",
            ));
        }
        return Ok(Some(value));
    }
    Ok(None)
}

pub(super) async fn start_session(
    state: AppState,
    request: CreatePlaySession,
) -> ApiResult<PlaySession> {
    if !crate::settings::valid_human_player(request.human_player)
        || !crate::settings::valid_play_match_length(request.match_length)
    {
        return Err(ApiError::invalid(
            "human_player must be 0 or 1 and match_length must be 0 (money game) or 1..25",
        ));
    }
    let (mut settings, error_thresholds) = crate::settings::load_play_settings(&state).await?;
    request.apply_match_choices(&mut settings)?;
    let tutor_threshold =
        validate_tutor_threshold(request.tutor_threshold)?.unwrap_or(error_thresholds.error);
    if crate::benchmarks::has_live_run(&state.pool).await? {
        return Err(benchmark_conflict());
    }
    let pinned_path = resolve_binary_async(&settings.gnubg_path)
        .await
        .map_err(native_error)?;
    let engine = Engine::new(pinned_path.clone()).interactive();
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
    let play_snapshot = async |id: &str,
                               gnubg_threads: Option<i32>,
                               sage_threads: u32,
                               plies: i32|
           -> ApiResult<crate::settings::EngineSnapshot> {
        if id == EngineId::Gnubg.id() {
            return Ok(crate::settings::EngineSnapshot {
                gnubg_threads: gnubg_threads.filter(|&n| n != 1),
                ..rules.clone()
            });
        }
        crate::settings::snapshot_for(&state, id, &pinned_path, gnubg_threads, sage_threads, plies)
            .await
            .map_err(native_error)
    };
    let bot = play_snapshot(
        &settings.play_engine,
        settings.gnubg_play_threads,
        settings.sage_play_threads(),
        settings.play_plies,
    )
    .await?;
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
    let analysis = if let Some(profile) = &bot.open_sage
        && settings.analysis_engine == settings.play_engine
        && settings.analysis_plies == profile.plies
        && settings.sage_analysis_threads() == profile.threads
    {
        bot.clone()
    } else {
        play_snapshot(
            &settings.analysis_engine,
            settings.gnubg_threads,
            settings.sage_analysis_threads(),
            settings.analysis_plies,
        )
        .await?
    };
    let version = bot
        .engine_version
        .clone()
        .context("Playing engine has no verified version")
        .map_err(native_error)?;
    let play_mode = request.play_mode.unwrap_or(settings.default_play_mode);
    let learning_used = play_mode != PlayMode::Competition;
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
        game_learning_used: learning_used,
        library_match_id: None,
        learning_used,
        play_mode,
        tutor_threshold,
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
            cube_enabled: None,
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
    insert_saved_session(&state, &mut session, &pinned_path).await?;
    open_sage::schedule_warmup(&session, &state);
    Ok(session)
}

pub(super) async fn perform_action(
    state: &AppState,
    id: Uuid,
    request: PlayActionRequest,
) -> ApiResult<Json<PlaySession>> {
    let previous = saved(state, id).await?;
    check_session_version(
        request.version,
        previous.state.version,
        "Session changed; reload before playing",
    )?;
    if matches!(request.action, PlayAction::Pause | PlayAction::Resume) {
        if request.move_index.is_some()
            || request.resign_level.is_some()
            || request.decision_id.is_some()
        {
            return Err(ApiError::invalid(
                "Pausing and resuming only require a session version",
            ));
        }
        return pause::transition(state, previous, matches!(request.action, PlayAction::Pause))
            .await;
    }
    if matches!(request.action, PlayAction::ForfeitMatch) {
        return forfeit::finish(state, previous, &request).await;
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
            state,
            &working,
            &previous.sgf,
            &previous.native_id,
            request.version,
            false,
        )
        .await;
    }
    validate_pending_decision(&working, &request, action)?;
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
                state,
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
        state,
        &session,
        &sgf,
        &native_id,
        request.version,
        true,
        completed_report.as_ref(),
    )
    .await
}

fn validate_pending_decision(
    working: &PlaySession,
    request: &PlayActionRequest,
    action: &str,
) -> ApiResult<()> {
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
    Ok(())
}

pub(super) async fn present_with_results(
    session: PlaySession,
    sgf: String,
) -> ApiResult<PlaySession> {
    Ok(
        tokio::task::spawn_blocking(move || {
            let mut session = project_game_results(session, &sgf);
            session.legal_turns = None;
            if matches!(session.phase, PlayPhase::Move) {
                match live_checker_turns(&session) {
                    Ok(turns) => session.legal_turns = Some(turns),
                    Err(error) => tracing::warn!(session_id = %session.id, %error, "Cannot project live checker turns"),
                }
            }
            session
        })
            .await
            .map_err(anyhow::Error::from)?,
    )
}

pub(super) fn native_move_board(
    board: &[i32],
    player: i32,
    dice: &[i32],
    choice: &PlayMove,
) -> anyhow::Result<[i32; 26]> {
    let before: [i32; 26] = board.try_into().context("Invalid legal-move board")?;
    let dice: [i32; 2] = dice.try_into().context("Invalid legal-move dice")?;
    let mut pairs = Vec::new();
    for step in &choice.steps {
        let point = |point: i32| -> anyhow::Result<usize> {
            if point == -1 && player == 0 || point == 26 && player == 1 {
                return Ok(0);
            }
            let relative = if player == 0 { point } else { 25 - point };
            ensure!(
                (0..=25).contains(&relative),
                "Invalid native legal move coordinate"
            );
            Ok(relative as usize)
        };
        pairs.push((point(step.from)?, point(step.to)?));
    }
    let mut expanded = before;
    let steps = crate::backgammon::expand_joined_play(&mut expanded, player, dice, &pairs)?;
    crate::backgammon::apply_checker_turn(&before, player, dice, &steps)
}

pub(super) fn live_checker_turns(session: &PlaySession) -> anyhow::Result<Vec<LiveCheckerTurn>> {
    if session.legal_moves.is_empty() {
        return Ok(Vec::new());
    }
    let position = &session.position;
    let board = position
        .board
        .as_slice()
        .try_into()
        .context("Invalid saved board")?;
    let dice = position
        .dice
        .as_slice()
        .try_into()
        .context("Invalid saved roll")?;
    let paths = crate::backgammon::checker_paths(board, position.player, dice)?;
    let mut native_boards = HashMap::new();
    for (index, choice) in session.legal_moves.iter().enumerate() {
        let end = native_move_board(&position.board, position.player, &position.dice, choice)?;
        native_boards.entry(end).or_insert(index);
    }
    let turns = paths
        .into_iter()
        .filter_map(|path| {
            let &move_index = native_boards.get(&path.board)?;
            Some(LiveCheckerTurn {
                turn: CheckerTurn::from_path(path, position.player),
                move_index,
            })
        })
        .collect::<Vec<_>>();
    ensure!(
        !turns.is_empty(),
        "No checker path matches the rule engine's legal moves"
    );
    Ok(turns)
}

fn aligned_history_events(
    history: &[PlayHistory],
    sgf: &str,
) -> anyhow::Result<(
    Option<crate::import::ImportedMatch>,
    Vec<crate::import::sgf::SourceMove>,
)> {
    let (imported, events) = if sgf.trim().is_empty() {
        (None, Vec::new())
    } else {
        let mut events = Vec::new();
        let (_, imported) =
            library::normalize_and_replay_native(sgf.to_owned(), true, Some(&mut events))?;
        (Some(imported), events)
    };
    ensure!(
        events.len() == history.len(),
        "Saved history differs from checkpoint event count"
    );
    for (event, record) in events.iter().zip(history) {
        ensure!(
            event.player == record.player,
            "Saved history differs from checkpoint player"
        );
        let action = record.notation.trim().to_ascii_lowercase();
        let record_kind = match action.as_str() {
            "double" | "take" | "drop" => "cube",
            "resign" => "resign",
            _ => "checker",
        };
        ensure!(
            event.kind == record_kind,
            "Saved history differs from checkpoint action kind"
        );
        if event.kind != "checker" {
            ensure!(
                event.notation.eq_ignore_ascii_case(&action),
                "Saved history differs from checkpoint action"
            );
            ensure!(
                record.dice.is_empty(),
                "Cube/resignation history contains dice"
            );
        } else {
            let mut record_dice = record.dice.clone();
            record_dice.sort_unstable();
            let mut source_dice = event.dice.to_vec();
            source_dice.sort_unstable();
            ensure!(
                record_dice == source_dice,
                "Saved history differs from checkpoint dice"
            );
        }
    }
    Ok((imported, events))
}

pub(super) fn project_game_results(mut session: PlaySession, sgf: &str) -> PlaySession {
    // Project after persistence, never write derived results into legacy state.
    session.game_results = None;
    if let Ok((Some(imported), events)) = aligned_history_events(&session.history, sgf) {
        let results = imported
            .games
            .into_iter()
            .filter_map(|game| {
                let result = game.result?;
                let history_index = events
                    .iter()
                    .rposition(|event| event.game_no == game.game_no)?;
                Some(PlayGameResult {
                    game_no: game.game_no,
                    history_index,
                    result,
                })
            })
            .collect::<Vec<_>>();
        session.game_results = Some(results);
    }
    present(session)
}

pub(crate) fn build_history_snapshots(
    session: &PlaySession,
    sgf: &str,
) -> anyhow::Result<PlayHistorySnapshots> {
    let (_, events) = aligned_history_events(&session.history, sgf)?;
    let mut ply = 0;
    let mut game_no = 0;
    let mut items = Vec::with_capacity(events.len());
    for (index, (event, record)) in events.into_iter().zip(&session.history).enumerate() {
        if event.game_no != game_no {
            game_no = event.game_no;
            ply = 0;
        }
        if let Some(checker_ply) = event.checker_ply {
            ply = checker_ply;
        }
        let before = Position {
            cube_enabled: None,
            id: session.id,
            match_uuid: session.id,
            ordinal: index.try_into()?,
            game_no: event.game_no,
            ply,
            player: event.player,
            dice: record.dice.clone(),
            board: event.board,
            position_id: event.position_id,
            gnubg_match_id: event.match_id,
            notation: event.notation,
        };
        let after_board: [i32; 26] = event.after_board.as_slice().try_into()?;
        // IDs encode checkers relative to player-on-roll, while the review
        // perspective remains the action's actor (also true for cube replies).
        let after_on_roll = crate::backgammon::ids::on_roll(&event.after_match_id)
            .context("Invalid GNUbg Match ID")?;
        let after = Position {
            board: event.after_board,
            position_id: crate::backgammon::ids::position_id(&after_board, after_on_roll),
            gnubg_match_id: event.after_match_id,
            dice: vec![],
            notation: String::new(),
            ..before.clone()
        };
        items.push(PlayHistorySnapshot {
            index,
            game_no: event.game_no,
            move_no: event.move_no,
            kind: event.kind.into(),
            before,
            after,
        });
    }
    Ok(PlayHistorySnapshots {
        session_id: session.id,
        version: session.version,
        items,
    })
}

// Staging does not consume RNG or begin a native game. For later games the
// previous native checkpoint already contains NextTurn's Crawford transition.
pub(super) fn stage_opening(session: &mut PlaySession, next: bool) -> anyhow::Result<()> {
    use crate::backgammon::ids::{Field, MatchId};
    let crawford = next
        && MatchId::decode(&session.position.gnubg_match_id)
            .is_some_and(|id| id.get(Field::CRAWFORD) == 1);
    if next {
        session.game_no += 1;
    }
    let board = crate::backgammon::initial_board();
    session.position = Position {
        cube_enabled: None,
        id: session.id,
        match_uuid: session.id,
        ordinal: session.version.try_into()?,
        game_no: session.game_no,
        ply: session.history.len().try_into()?,
        player: session.human_player,
        dice: vec![],
        board: board.to_vec(),
        position_id: crate::backgammon::ids::position_id(&board, session.human_player),
        gnubg_match_id: crate::backgammon::ids::match_id(
            session.match_length,
            session.scores,
            1,
            -1,
            crawford,
            session.match_length == 0,
            session.human_player,
            [0, 0],
        ),
        notation: String::new(),
    };
    session.phase = PlayPhase::OpeningRoll;
    session.opening = None;
    session.winner = None;
    session.legal_moves.clear();
    session.can_double = false;
    Ok(())
}

// Internal checkpoints keep native names so old GNUbg's 31-byte name buffer
// cannot truncate a user's UTF-8 name during restore. Export full saved names
// by replacing root player properties outside escaped SGF values.
pub(crate) fn named_export(sgf: &str, human: &str, player: i32, bot: &str) -> String {
    let names = if player == 0 {
        [
            crate::import::sgf::escape_value(human),
            crate::import::sgf::escape_value(bot),
        ]
    } else {
        [
            crate::import::sgf::escape_value(bot),
            crate::import::sgf::escape_value(human),
        ]
    };
    let bytes = sgf.as_bytes();
    let mut replacements = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_uppercase() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_uppercase() {
                i += 1;
            }
            let key = &sgf[start..i];
            while i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b']' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                if let Some(index) = match key {
                    "PW" => Some(0),
                    "PB" => Some(1),
                    _ => None,
                } {
                    replacements.push((start, i, names[index].as_str()));
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    let mut result = sgf.to_owned();
    for (start, end, name) in replacements.into_iter().rev() {
        result.replace_range(start..end, name);
    }
    result
}

pub(super) fn validate_action<'a>(
    session: &'a PlaySession,
    request: &PlayActionRequest,
) -> ApiResult<(&'static str, Option<&'a str>)> {
    pause::require_active(session)?;
    if !matches!(request.action, PlayAction::Move) && request.move_index.is_some() {
        return Err(ApiError::invalid("move is only valid for checker moves"));
    }
    if matches!(request.action, PlayAction::Resign) {
        if !request
            .resign_level
            .is_some_and(|level| (1..=3).contains(&level))
        {
            return Err(ApiError::invalid("Choose resignation level 1, 2, or 3"));
        }
    } else if request.resign_level.is_some() {
        return Err(ApiError::invalid(
            "resign_level is only valid for resignation",
        ));
    }
    if session.money_session_ended {
        return Err(ApiError::invalid("This money session has ended"));
    }
    let action = match (&session.phase, &request.action) {
        (PlayPhase::OpeningRoll, PlayAction::OpeningRoll) => "opening_roll",
        (PlayPhase::OpeningResult, PlayAction::StartOpeningMove) => "start_opening_move",
        (PlayPhase::Roll | PlayPhase::Move, PlayAction::Resign) => "resign",
        (PlayPhase::Roll, PlayAction::Roll) => "roll",
        (PlayPhase::Roll, PlayAction::Double) if session.can_double => "double",
        (PlayPhase::Move, PlayAction::Move) => {
            let selected = request
                .move_index
                .and_then(|index| session.legal_moves.get(index))
                .ok_or_else(|| ApiError::invalid("Choose a legal move index"))?;
            return Ok(("move", Some(&selected.notation)));
        }
        (PlayPhase::Move, PlayAction::Pass) if session.legal_moves.is_empty() => "pass",
        (PlayPhase::DoubleResponse, PlayAction::Take) => "take",
        (PlayPhase::DoubleResponse, PlayAction::Drop) => "drop",
        (PlayPhase::GameOver | PlayPhase::MatchOver, PlayAction::EndSession)
            if reports::money_finished(session) =>
        {
            "end_session"
        }
        (PlayPhase::GameOver, PlayAction::NextGame) => "next_game",
        (PlayPhase::MatchOver, PlayAction::NextGame) if session.match_length == 0 => "next_game",
        _ => {
            return Err(ApiError::invalid(
                "Action is not available at this decision",
            ));
        }
    };
    Ok((action, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_game_results_normalizes_black_first_native_setup_turns() {
        // Native SETBOARD export says PL[W] despite a Black first action.
        let sgf = "(;GM[6]FF[4]MI[length:5][ws:0][bs:0]RE[W+1R];PL[W]AE[a:y]AW[e]AB[b];B[31beef])\n(;GM[6]FF[4]MI[length:5][ws:1][bs:0];PL[W]AE[a:y]AW[y]AB[s][s][t][t][u][u][v][v][w][w][x][x];W[61])";
        let id = Uuid::new_v4();
        let session: PlaySession = serde_json::from_value(serde_json::json!({
            "id": id,
            "version": 3,
            "human_player": 0,
            "human_name": "Human",
            "match_length": 5,
            "bearoff_direction": "right",
            "engine": {"id": "gnubg", "version": "fixture", "plies": 1},
            "position": {
                "id": id,
                "match_uuid": id,
                "ordinal": 0,
                "game_no": 2,
                "ply": 1,
                "player": 0,
                "dice": [],
                "board": vec![0; 26],
                "position_id": "",
                "gnubg_match_id": "",
                "notation": ""
            },
            "phase": "roll",
            "can_double": false,
            "legal_moves": [],
            "history": [
                {"player": 1, "dice": [3, 1], "notation": "23/20*/19"},
                {"player": 1, "dice": [], "notation": "resign"},
                {"player": 0, "dice": [6, 1], "notation": "pass"}
            ],
            "scores": [1, 0],
            "game_no": 2,
            "winner": null
        }))
        .unwrap();

        let projected = project_game_results(session, sgf);
        let results = projected
            .game_results
            .expect("setup turn should align with history and project results");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].game_no, 1);
        assert_eq!(results[0].history_index, 1);
        assert_eq!(results[0].result.winner, 0);
        assert_eq!(results[0].result.points, 1);
    }
}
