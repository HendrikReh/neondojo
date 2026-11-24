use super::*;

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

pub(super) fn project_game_results(mut session: PlaySession, sgf: &str) -> PlaySession {
    // Project after persistence, never write derived results into legacy state.
    session.game_results = None;
    if !sgf.trim().is_empty()
        && let Ok((imported, events)) = crate::import::sgf::parse_native_sgf_source_moves(sgf)
        && events.len() == session.history.len()
        && events.iter().zip(&session.history).all(|(event, row)| {
            let action = row.notation.trim().to_ascii_lowercase();
            let kind = match action.as_str() {
                "double" | "take" | "drop" => "cube",
                "resign" => "resign",
                _ => "checker",
            };
            let mut dice = row.dice.clone();
            dice.sort_unstable();
            let mut expected = if kind == "checker" {
                event.dice.to_vec()
            } else {
                vec![]
            };
            expected.sort_unstable();
            event.player == row.player
                && event.kind == kind
                && dice == expected
                && (kind == "checker" || event.notation.eq_ignore_ascii_case(&action))
        })
    {
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
    let events = if sgf.trim().is_empty() {
        Vec::new()
    } else {
        let source = library::normalize_native_setup_turn(sgf.to_owned())?;
        crate::import::sgf::parse_native_sgf_source_moves(&source)?.1
    };
    ensure!(
        events.len() == session.history.len(),
        "Saved history differs from checkpoint event count"
    );
    let mut ply = 0;
    let mut game_no = 0;
    let mut items = Vec::with_capacity(events.len());
    for (index, (event, record)) in events.into_iter().zip(&session.history).enumerate() {
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
        if event.game_no != game_no {
            game_no = event.game_no;
            ply = 0;
        }
        if let Some(checker_ply) = event.checker_ply {
            ply = checker_ply;
        }
        let before = Position {
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
