//! Live Play interruption priority resolution, turn status computation,
//! resignation-decline detection, and animation/sound frame extraction.
//!
//! Pure domain helpers mirroring `play-interruption.ts`, `play-turn-status.ts`,
//! `play-animation.ts`, and `play-sound-events.ts`.

use neondojo::domain_error::{DomainError, ErrorKind};
use neondojo::model::{MoveSeverity, Position};
use neondojo::play::{PlayAction, PlayDecision, PlayMode, PlayPhase, PlaySession};
use neondojo_board::{
    ActiveVisual, BoardPosition, DiceFrame, PlayFrame, PlaySoundKind, checker_frame,
    decode_match_context,
};
use neondojo_i18n::{I18n, Locale, Param};

/// The board-anchored dialog that currently stands between the player and the board.
///
/// Resolved strictly in priority order:
/// `blocked` → `resign` → `hint` → inline `coaching` → `cube` → `feedback` → `game_over` → `match_over`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayInterruption {
    Resign,
    Hint,
    Cube,
    Feedback,
    GameOver,
    MatchOver,
}

impl PlayInterruption {
    /// Whether the player may dismiss this interruption with Escape, backdrop click, or Close.
    pub const fn is_dismissible(self) -> bool {
        matches!(self, Self::Resign | Self::Hint)
    }
}

/// Summary of the current pending feedback decision on a [`PlaySession`].
#[derive(Debug, Clone)]
pub struct PlayFeedbackState<'a> {
    pub pending: Option<&'a PlayDecision>,
    pub mode: PlayMode,
    pub competition: bool,
    pub routine_roll: Option<&'a PlayDecision>,
    pub awaiting: bool,
}

/// True when a money-game session has finished its current game and may still
/// continue with another game (or be ended explicitly).
pub fn is_money_game_finished(session: Option<&PlaySession>) -> bool {
    session.is_some_and(|s| {
        s.match_length == 0 && matches!(s.phase, PlayPhase::GameOver | PlayPhase::MatchOver)
    })
}

/// Computes the pending feedback state of a session (`play-interruption.ts::playFeedback`).
pub fn play_feedback_state(session: Option<&PlaySession>) -> PlayFeedbackState<'_> {
    let pending = session.and_then(|s| s.pending_decision.as_ref());
    let mode = session.map_or(PlayMode::Competition, |s| s.play_mode);
    // A roll without a warning continues directly on the next roll click.
    let routine_roll = pending.filter(|p| {
        p.action.as_deref() == Some("roll")
            && !p.warning
            && (mode == PlayMode::Training
                || (mode == PlayMode::Coach && p.severity.as_deref() == Some("none")))
    });
    let awaiting = pending.and_then(|p| p.action.as_ref()).is_some() && routine_roll.is_none();

    PlayFeedbackState {
        pending,
        mode,
        competition: mode == PlayMode::Competition,
        routine_roll,
        awaiting,
    }
}

/// Inputs to [`resolve_play_interruption`].
pub struct PlayInterruptionInput<'a> {
    pub session: Option<&'a PlaySession>,
    /// True when nothing may interrupt (hidden workspace, setup open, paused,
    /// history review active, animation playing, or pending server roll).
    pub blocked: bool,
    pub resign_open: bool,
    pub hint_open: bool,
    /// Coach feedback renders inline above the board and yields to resign/hint only.
    pub coaching: bool,
    /// True after the player closes the finished-match result dialog.
    pub acknowledged: bool,
}

/// Resolves the highest-priority interruption dialog standing over the board.
///
/// Exact priority order:
/// 1. `blocked` → `None`
/// 2. `resign_open` → `Some(PlayInterruption::Resign)`
/// 3. `hint_open && !competition` → `Some(PlayInterruption::Hint)`
/// 4. `coaching` → `None` (inline Coach toolbar owns the decision)
/// 5. `phase == DoubleResponse` → `Some(PlayInterruption::Cube)`
/// 6. `awaiting` feedback → `Some(PlayInterruption::Feedback)`
/// 7. `phase == GameOver` or (`is_money_game_finished && !money_session_ended`) → `Some(PlayInterruption::GameOver)`
/// 8. `phase == MatchOver && !acknowledged` → `Some(PlayInterruption::MatchOver)`
pub fn resolve_play_interruption(input: PlayInterruptionInput<'_>) -> Option<PlayInterruption> {
    if input.blocked {
        return None;
    }
    let fb = play_feedback_state(input.session);
    if input.resign_open {
        return Some(PlayInterruption::Resign);
    }
    if input.hint_open && !fb.competition {
        return Some(PlayInterruption::Hint);
    }
    if input.coaching {
        return None;
    }
    let session = input.session?;
    if matches!(session.phase, PlayPhase::DoubleResponse) {
        return Some(PlayInterruption::Cube);
    }
    if fb.awaiting {
        return Some(PlayInterruption::Feedback);
    }
    if matches!(session.phase, PlayPhase::GameOver) {
        return Some(PlayInterruption::GameOver);
    }
    if is_money_game_finished(Some(session)) && !session.money_session_ended {
        return Some(PlayInterruption::GameOver);
    }
    if matches!(session.phase, PlayPhase::MatchOver) && !input.acknowledged {
        return Some(PlayInterruption::MatchOver);
    }
    None
}

/// Returns the `play` i18n key explaining why doubling is currently unavailable.
pub fn double_unavailable_reason_key(session: Option<&PlaySession>) -> &'static str {
    let Some(session) = session else {
        return "cubeUnavailable";
    };
    let cube_ctx = decode_match_context(&session.position.gnubg_match_id);
    if cube_ctx.crawford {
        return "doubleCrawford";
    }
    if let Some(owner) = cube_ctx.owner
        && i32::from(owner) != session.human_player
    {
        return "doubleOpponentCube";
    }
    let human_idx = usize::try_from(session.human_player).unwrap_or(0).min(1);
    if session.match_length > 0 && cube_ctx.cube >= session.match_length - session.scores[human_idx]
    {
        return "doubleMatchCovered";
    }
    "cubeUnavailable"
}

/// Inputs for [`compute_play_turn_status`].
pub struct PlayTurnStatusContext<'a> {
    pub session: Option<&'a PlaySession>,
    pub review_active: bool,
    pub paused: bool,
    pub visual: Option<&'a ActiveVisual>,
    pub opening_phase: bool,
    pub pending_roll: Option<PlayActionKind>,
    pub busy: bool,
    pub hint_open: bool,
    pub awaiting_feedback: bool,
    pub forced_pass: bool,
}

/// Lightweight copyable mirror of [`PlayAction`] used for animation/pending state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayActionKind {
    OpeningRoll,
    StartOpeningMove,
    Roll,
    Move,
    Pass,
    Double,
    Take,
    Drop,
    Resign,
    ForfeitMatch,
    NextGame,
    EndSession,
    Pause,
    Resume,
}

impl PlayActionKind {
    pub const fn to_backend(self) -> PlayAction {
        match self {
            Self::OpeningRoll => PlayAction::OpeningRoll,
            Self::StartOpeningMove => PlayAction::StartOpeningMove,
            Self::Roll => PlayAction::Roll,
            Self::Move => PlayAction::Move,
            Self::Pass => PlayAction::Pass,
            Self::Double => PlayAction::Double,
            Self::Take => PlayAction::Take,
            Self::Drop => PlayAction::Drop,
            Self::Resign => PlayAction::Resign,
            Self::ForfeitMatch => PlayAction::ForfeitMatch,
            Self::NextGame => PlayAction::NextGame,
            Self::EndSession => PlayAction::EndSession,
            Self::Pause => PlayAction::Pause,
            Self::Resume => PlayAction::Resume,
        }
    }

    pub fn from_action_str(value: &str) -> Option<Self> {
        match value {
            "opening_roll" => Some(Self::OpeningRoll),
            "start_opening_move" => Some(Self::StartOpeningMove),
            "roll" => Some(Self::Roll),
            "move" => Some(Self::Move),
            "pass" => Some(Self::Pass),
            "double" => Some(Self::Double),
            "take" => Some(Self::Take),
            "drop" => Some(Self::Drop),
            "resign" => Some(Self::Resign),
            "forfeit_match" => Some(Self::ForfeitMatch),
            "next_game" => Some(Self::NextGame),
            "end_session" => Some(Self::EndSession),
            "pause" => Some(Self::Pause),
            "resume" => Some(Self::Resume),
            _ => None,
        }
    }
}

/// Resolves the live status message for the active turn, animation, or phase.
pub fn compute_play_turn_status(
    locale: Locale,
    ctx: &PlayTurnStatusContext<'_>,
    player_name: impl Fn(i32) -> String,
) -> String {
    let Some(session) = ctx.session else {
        return String::new();
    };
    if ctx.review_active {
        return I18n::t(locale, "play", "reviewing").to_string();
    }
    if ctx.paused {
        return I18n::t(locale, "play", "matchPaused").to_string();
    }
    if let Some(ActiveVisual::Checker { player, .. }) = ctx.visual {
        return format!(
            "{} · {}",
            player_name(i32::from(*player)),
            I18n::t(locale, "play", "watchMove")
        );
    }
    if let Some(ActiveVisual::Dice {
        opening: true,
        rolling: false,
        tied,
        player,
        ..
    }) = ctx.visual
    {
        return if *tied {
            I18n::t(locale, "play", "openingTie").to_string()
        } else {
            I18n::format(
                locale,
                "app",
                "app.toMove",
                &[("player", Param::from(player_name(i32::from(*player))))],
            )
        };
    }
    if ctx.pending_roll.is_some() || matches!(ctx.visual, Some(ActiveVisual::Dice { .. })) {
        if ctx.opening_phase {
            return I18n::t(locale, "play", "openingRolling").to_string();
        }
        let rolling_player = match ctx.visual {
            Some(ActiveVisual::Dice { player, .. }) => i32::from(*player),
            _ => session.human_player,
        };
        return format!(
            "{} · {}",
            player_name(rolling_player),
            I18n::t(locale, "play", "rolling")
        );
    }
    if ctx.busy && !ctx.hint_open {
        let key = if ctx.opening_phase {
            "openingPreparing"
        } else {
            "thinking"
        };
        return I18n::t(locale, "play", key).to_string();
    }
    if ctx.awaiting_feedback {
        return I18n::t(locale, "play-mode", "unconfirmed").to_string();
    }
    if session.money_session_ended {
        return I18n::t(locale, "play", "sessionEnded").to_string();
    }
    if session.match_forfeited {
        return I18n::t(locale, "play-resign", "matchForfeited").to_string();
    }
    if ctx.forced_pass {
        return I18n::t(locale, "play", "pass").to_string();
    }
    match session.phase {
        PlayPhase::Roll => I18n::t(locale, "play", "yourTurn").to_string(),
        PlayPhase::Move => {
            if session.position.player == session.human_player {
                I18n::t(locale, "play", "yourTurn").to_string()
            } else {
                I18n::format(
                    locale,
                    "app",
                    "app.toMove",
                    &[("player", Param::from(player_name(session.position.player)))],
                )
            }
        }
        PlayPhase::OpeningRoll => I18n::t(locale, "play", "opening_roll").to_string(),
        PlayPhase::OpeningResult => I18n::t(locale, "play", "opening_result").to_string(),
        PlayPhase::DoubleResponse => I18n::t(locale, "play", "double_response").to_string(),
        PlayPhase::GameOver => I18n::t(locale, "play", "game_over").to_string(),
        PlayPhase::MatchOver => I18n::t(locale, "play", "match_over").to_string(),
    }
}

/// Detects the backend's 422 bot resignation-decline error cleanly (`PlayDialogStack.tsx:272`).
pub fn is_resignation_declined(err: &DomainError) -> bool {
    err.kind == ErrorKind::UnprocessableEntity
        && (err
            .body
            .error
            .starts_with("GNUbg declined this resignation.")
            || err
                .body
                .error
                .starts_with("The bot declined this resignation."))
}

/// Maps backend severity string (`"none"`, `"doubtful"`, `"bad"`, `"blunder"`, `"very_bad"`)
/// into [`MoveSeverity`].
pub fn parse_play_severity(value: Option<&str>) -> MoveSeverity {
    match value {
        Some("doubtful") => MoveSeverity::Doubtful,
        Some("bad") => MoveSeverity::Bad,
        Some("blunder") => MoveSeverity::Blunder,
        Some("very_bad") => MoveSeverity::VeryBad,
        _ => MoveSeverity::None,
    }
}

/// Converts a backend `Vec<i32>` board into a fixed `[i32; 26]` array.
pub fn board_array(slice: &[i32]) -> [i32; 26] {
    let mut out = [0i32; 26];
    for (dst, &src) in out.iter_mut().zip(slice.iter()) {
        *dst = src;
    }
    out
}

/// Converts a backend [`Position`] into a [`BoardPosition`] for `neondojo_board::BoardView`.
pub fn position_to_board_position(pos: &Position) -> BoardPosition {
    BoardPosition {
        board: board_array(&pos.board),
        player: u8::try_from(pos.player).unwrap_or(0).min(1),
        dice: pos
            .dice
            .iter()
            .filter_map(|&d| u8::try_from(d).ok().filter(|&v| (1..=6).contains(&v)))
            .collect(),
        gnubg_match_id: pos.gnubg_match_id.clone(),
        notation: pos.notation.clone(),
    }
}

/// Parses mover-relative native history notation (e.g. `"bar/22 6/5"`, `"3/off 1/off"`)
/// into absolute `(from, to)` step pairs (`play-sound-events.ts::nativeSteps`).
pub fn native_steps(notation: &str, player: i32) -> Vec<(i32, i32)> {
    let mut steps = Vec::new();
    for part in notation.split_whitespace() {
        let Some((lhs, rhs)) = part.split_once('/') else {
            continue;
        };
        let from = if lhs == "bar" {
            Some(25)
        } else {
            lhs.parse::<i32>().ok().filter(|&n| (1..=24).contains(&n))
        };
        let to = if rhs == "off" {
            Some(-1)
        } else {
            rhs.parse::<i32>().ok().filter(|&n| (1..=24).contains(&n))
        };
        if let (Some(f), Some(t)) = (from, to) {
            let abs_from = if player == 0 { f } else { 25 - f };
            let abs_to = if player == 0 { t } else { 25 - t };
            steps.push((abs_from, abs_to));
        }
    }
    steps
}

/// Builds the ordered dice and bot-checker animation frames for a committed
/// server transition (`play-animation.ts::actionFrames`).
pub fn action_frames(
    before: &PlaySession,
    after: &PlaySession,
    action: PlayActionKind,
) -> Vec<PlayFrame> {
    if before.id != after.id || after.version <= before.version {
        return Vec::new();
    }
    let before_board = board_array(&before.position.board);
    if action == PlayActionKind::OpeningRoll {
        return after
            .opening
            .as_ref()
            .map(|opening| {
                opening
                    .rolls
                    .iter()
                    .map(|&dice| {
                        let d0 = u8::try_from(dice[0]).unwrap_or(1);
                        let d1 = u8::try_from(dice[1]).unwrap_or(1);
                        PlayFrame::Dice(DiceFrame {
                            before: before_board,
                            player: if d0 > d1 { 0 } else { 1 },
                            dice: vec![d0, d1],
                            roll: true,
                            opening: true,
                            tied: d0 == d1,
                            sound: Some(PlaySoundKind::Roll),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    let mut frames = Vec::new();
    if action == PlayActionKind::Roll {
        let dice: Vec<u8> = after
            .position
            .dice
            .iter()
            .filter_map(|&d| u8::try_from(d).ok().filter(|&v| (1..=6).contains(&v)))
            .collect();
        frames.push(PlayFrame::Dice(DiceFrame {
            before: before_board,
            player: u8::try_from(before.human_player).unwrap_or(0).min(1),
            dice,
            roll: true,
            opening: false,
            tied: false,
            sound: Some(PlaySoundKind::Roll),
        }));
    }

    let mut board = before_board;
    let new_records = after.history.get(before.history.len()..).unwrap_or(&[]);
    for (index, record) in new_records.iter().enumerate() {
        let steps = native_steps(&record.notation, record.player);
        let is_bot = record.player != before.human_player;
        let rec_player = u8::try_from(record.player).unwrap_or(0).min(1);
        let rec_dice: Vec<u8> = record
            .dice
            .iter()
            .filter_map(|&d| u8::try_from(d).ok().filter(|&v| (1..=6).contains(&v)))
            .collect();

        if is_bot && rec_dice.len() == 2 && (!steps.is_empty() || record.notation == "pass") {
            let roll = !(action == PlayActionKind::StartOpeningMove && index == 0);
            frames.push(PlayFrame::Dice(DiceFrame {
                before: board,
                player: rec_player,
                dice: rec_dice.clone(),
                roll,
                opening: false,
                tied: false,
                sound: roll.then_some(PlaySoundKind::Roll),
            }));
        }
        for (from, to) in steps {
            let frame = checker_frame(&board, from, to, rec_player, rec_dice.clone());
            board = frame.after;
            if is_bot {
                frames.push(PlayFrame::Checker(frame));
            }
        }
    }
    frames
}

/// True when a server response introduces a new bot double offer
/// (`play-sound-events.ts::isNewCubeOffer`).
pub fn is_new_cube_offer(before: &PlaySession, after: &PlaySession) -> bool {
    if before.id != after.id
        || after.version <= before.version
        || matches!(before.phase, PlayPhase::DoubleResponse)
        || !matches!(after.phase, PlayPhase::DoubleResponse)
    {
        return false;
    }
    after
        .history
        .get(before.history.len()..)
        .unwrap_or(&[])
        .iter()
        .any(|row| row.player != before.human_player && row.notation.eq_ignore_ascii_case("double"))
}
