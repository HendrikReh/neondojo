//! Synchronous local checker-turn engine (`PlayTurnEntity`).
//!
//! Computes legal turns synchronously via `neondojo_rules::backgammon::checker_paths`
//! (or uses `session.legal_turns` when already populated by the backend), supports
//! hover destination dots, single-click step execution (never drag-and-drop),
//! clicking dice to swap die order, half-used doubles die masks, `Z` undo with
//! silent reverse animation, forced pass detection, and turn confirmation.

use crate::workspaces::play::interruption::board_array;
use gpui_kit::Context;
use neondojo::play::{CheckerTurn, LiveCheckerTurn, PlaySession, PlayStep};
use neondojo_board::{
    CheckerFrame, DieUsage, apply_absolute_steps, checker_frame, relative_point, reverse_frame,
};
use neondojo_i18n::{I18n, Locale};
use neondojo_rules::backgammon::checker_paths;
use std::collections::BTreeMap;

/// A single local checker step along with the die face it consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckerStepWithDie {
    pub from: i32,
    pub to: i32,
    pub die: u8,
}

/// Converts a mover-relative `CheckerPath` from `neondojo_rules` into absolute
/// board coordinates matching [`CheckerTurn`].
pub fn checker_turn_from_path(
    path: &neondojo_rules::backgammon::CheckerPath,
    player: i32,
) -> CheckerTurn {
    let absolute = |point: i32| if player == 0 { point } else { 25 - point };
    let mut steps = Vec::with_capacity(path.steps.len());
    let mut dice = Vec::with_capacity(path.steps.len());
    for &(from, to, die) in &path.steps {
        steps.push(PlayStep {
            from: absolute(from as i32),
            to: absolute(if to == 0 { -1 } else { to as i32 }),
        });
        dice.push(die);
    }
    CheckerTurn { steps, dice }
}

/// Computes live checker turns synchronously for `session`, preferring the
/// server's pre-matched `legal_turns` catalog and falling back to
/// `neondojo_rules::backgammon::checker_paths` matched by end-board against
/// `session.legal_moves`.
pub fn resolve_live_checker_turns(session: &PlaySession) -> Vec<LiveCheckerTurn> {
    if session.legal_moves.is_empty() {
        return Vec::new();
    }
    if let Some(turns) = &session.legal_turns
        && !turns.is_empty()
    {
        return turns.clone();
    }
    if session.position.dice.len() < 2 {
        return Vec::new();
    }
    let board = board_array(&session.position.board);
    let player = session.position.player;
    let dice = [session.position.dice[0], session.position.dice[1]];
    let Ok(paths) = checker_paths(&board, player, dice) else {
        return Vec::new();
    };
    let mover = u8::try_from(player).unwrap_or(0).min(1);
    let legal_end_boards: Vec<[i32; 26]> = session
        .legal_moves
        .iter()
        .map(|mv| {
            let pairs: Vec<(i32, i32)> = mv.steps.iter().map(|s| (s.from, s.to)).collect();
            apply_absolute_steps(&board, &pairs, mover)
        })
        .collect();

    let mut matched = Vec::new();
    for path in &paths {
        if let Some(move_index) = legal_end_boards.iter().position(|b| *b == path.board) {
            matched.push(LiveCheckerTurn {
                turn: checker_turn_from_path(path, player),
                move_index,
            });
        }
    }
    matched
}

/// Pure helper computing legal turns directly from `(board, player, dice)`
/// without needing a backend session (used in tests and offline validation).
pub fn synchronous_checker_turns(
    board: &[i32; 26],
    player: i32,
    dice: [i32; 2],
) -> Vec<LiveCheckerTurn> {
    let Ok(paths) = checker_paths(board, player, dice) else {
        return Vec::new();
    };
    let mut end_boards: Vec<[i32; 26]> = Vec::new();
    let mut out = Vec::with_capacity(paths.len());
    for path in &paths {
        let move_index = if let Some(idx) = end_boards.iter().position(|b| *b == path.board) {
            idx
        } else {
            let idx = end_boards.len();
            end_boards.push(path.board);
            idx
        };
        out.push(LiveCheckerTurn {
            turn: checker_turn_from_path(path, player),
            move_index,
        });
    }
    out
}

/// Filters `turns` to those matching every step and die in `prefix`.
pub fn matching_checker_turns<'a>(
    turns: &'a [LiveCheckerTurn],
    prefix: &[CheckerStepWithDie],
) -> Vec<&'a LiveCheckerTurn> {
    turns
        .iter()
        .filter(|live| {
            if live.turn.steps.len() < prefix.len() || live.turn.dice.len() < prefix.len() {
                return false;
            }
            prefix.iter().enumerate().all(|(idx, step)| {
                live.turn.steps[idx].from == step.from
                    && live.turn.steps[idx].to == step.to
                    && live.turn.dice[idx] == i32::from(step.die)
            })
        })
        .collect()
}

/// Computes the next clickable source → step map at step `index`, prioritizing
/// `preferred_die` when a checker can legally move with either remaining die
/// (`play-state.ts::nextCheckerSteps`).
pub fn next_checker_steps(
    turns: &[&LiveCheckerTurn],
    index: usize,
    preferred_die: Option<u8>,
) -> BTreeMap<i32, CheckerStepWithDie> {
    let mut choices: BTreeMap<i32, CheckerStepWithDie> = BTreeMap::new();
    for live in turns {
        let Some(step) = live.turn.steps.get(index) else {
            continue;
        };
        let Some(&die_i32) = live.turn.dice.get(index) else {
            continue;
        };
        let die = u8::try_from(die_i32).unwrap_or(0);
        if !choices.contains_key(&step.from) || preferred_die == Some(die) {
            choices.insert(
                step.from,
                CheckerStepWithDie {
                    from: step.from,
                    to: step.to,
                    die,
                },
            );
        }
    }
    choices
}

/// GPUI entity managing the human player's local uncommitted checker turn.
#[derive(Debug, Default)]
pub struct PlayTurnEntity {
    saved_dice: Vec<u8>,
    legal_turns: Vec<LiveCheckerTurn>,
    legal_moves_count: usize,
    steps: Vec<CheckerStepWithDie>,
    hovered: Option<i32>,
    swapped: bool,
}

impl PlayTurnEntity {
    pub fn new() -> Self {
        Self::default()
    }

    /// Synchronizes legal moves & dice from an accepted [`PlaySession`].
    ///
    /// When `preserve_steps` is `true` (e.g. live PlayMode or Tutor threshold
    /// change, or requesting a Hint), uncommitted steps and die-swap state are
    /// preserved intact.
    pub fn sync_session(
        &mut self,
        session: Option<&PlaySession>,
        preserve_steps: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = session {
            self.saved_dice = session
                .position
                .dice
                .iter()
                .filter_map(|&d| u8::try_from(d).ok().filter(|&v| (1..=6).contains(&v)))
                .collect();
            self.legal_moves_count = session.legal_moves.len();
            self.legal_turns = resolve_live_checker_turns(session);
        } else {
            self.saved_dice.clear();
            self.legal_moves_count = 0;
            self.legal_turns.clear();
        }
        if !preserve_steps {
            self.steps.clear();
            self.hovered = None;
            self.swapped = false;
        }
        cx.notify();
    }

    /// Directly loads turns and dice (useful in synchronous unit tests).
    pub fn load_local_position(&mut self, board: &[i32; 26], player: i32, dice: [u8; 2]) {
        self.saved_dice = dice.to_vec();
        self.legal_turns =
            synchronous_checker_turns(board, player, [i32::from(dice[0]), i32::from(dice[1])]);
        self.legal_moves_count = self
            .legal_turns
            .iter()
            .map(|t| t.move_index)
            .max()
            .map_or(0, |m| m + 1);
        self.steps.clear();
        self.hovered = None;
        self.swapped = false;
    }

    /// Clears all uncommitted steps, hover state, and die-swap state.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        if self.steps.is_empty() && self.hovered.is_none() && !self.swapped {
            return;
        }
        self.steps.clear();
        self.hovered = None;
        self.swapped = false;
        cx.notify();
    }

    /// Active dice in display order (reversed when swapped).
    pub fn dice(&self) -> Vec<u8> {
        if self.swapped {
            self.saved_dice.iter().copied().rev().collect()
        } else {
            self.saved_dice.clone()
        }
    }

    pub fn steps(&self) -> &[CheckerStepWithDie] {
        &self.steps
    }

    pub fn step_pairs(&self) -> Vec<(i32, i32)> {
        self.steps.iter().map(|s| (s.from, s.to)).collect()
    }

    pub fn hovered(&self) -> Option<i32> {
        self.hovered
    }

    pub fn is_swapped(&self) -> bool {
        self.swapped
    }

    /// True when the human can move, has legal moves, yet no paths were resolved.
    pub fn missing_checker_paths(&self, can_move: bool) -> bool {
        can_move && self.legal_turns.is_empty() && self.legal_moves_count > 0
    }

    /// True when the human is on move with zero legal checker plays.
    pub fn forced_pass(&self, can_move: bool) -> bool {
        can_move && self.legal_moves_count == 0
    }

    /// True when clicking the dice pair swaps their priority order.
    pub fn can_swap(&self, can_move: bool, animating: bool) -> bool {
        let dice = self.dice();
        can_move
            && !self.forced_pass(can_move)
            && !animating
            && self.steps.is_empty()
            && dice.len() == 2
            && dice[0] != dice[1]
    }

    /// Consumed-die mask (`Unused` / `Half` / `Used`) for the two displayed dice.
    pub fn used_dice(&self) -> [DieUsage; 2] {
        let dice = self.dice();
        let played: Vec<u8> = self.steps.iter().map(|s| s.die).collect();
        DieUsage::for_turn(&dice, &played)
    }

    fn continuations(&self, can_move: bool) -> Vec<&LiveCheckerTurn> {
        if !can_move {
            return Vec::new();
        }
        matching_checker_turns(&self.legal_turns, &self.steps)
    }

    fn choices(&self, can_move: bool) -> BTreeMap<i32, CheckerStepWithDie> {
        let continuations = self.continuations(can_move);
        let dice = self.dice();
        let preferred = dice.get(self.steps.len()).copied();
        next_checker_steps(&continuations, self.steps.len(), preferred)
    }

    /// Points holding checkers that can legally perform the next step.
    pub fn sources(&self, can_move: bool) -> Vec<i32> {
        self.choices(can_move).keys().copied().collect()
    }

    /// Destination dot(s) for the currently hovered checker source.
    pub fn destinations(&self, can_move: bool) -> Vec<i32> {
        let Some(src) = self.hovered else {
            return Vec::new();
        };
        self.choices(can_move)
            .get(&src)
            .map(|step| vec![step.to])
            .unwrap_or_default()
    }

    /// Returns the matching legal move index once all required dice have been played.
    pub fn completed_move_index(&self, can_move: bool) -> Option<usize> {
        if self.steps.is_empty() {
            return None;
        }
        self.continuations(can_move)
            .into_iter()
            .find(|t| t.turn.steps.len() == self.steps.len())
            .map(|t| t.move_index)
    }

    /// Updates hovered source point for live destination dot preview.
    pub fn set_hover(&mut self, point: Option<i32>, cx: &mut Context<Self>) {
        if self.hovered != point {
            self.hovered = point;
            cx.notify();
        }
    }

    /// Non-GPUI helper for synchronous unit tests.
    pub fn set_hover_local(&mut self, point: Option<i32>) {
        self.hovered = point;
    }

    /// Swaps the priority order of two non-double dice before any step is played.
    pub fn swap_dice_local(
        &mut self,
        can_move: bool,
        animating: bool,
        review_active: bool,
    ) -> bool {
        if !self.can_swap(can_move, animating) || review_active {
            return false;
        }
        self.swapped = !self.swapped;
        self.hovered = None;
        true
    }

    pub fn swap_dice(
        &mut self,
        can_move: bool,
        animating: bool,
        review_active: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.swap_dice_local(can_move, animating, review_active) {
            cx.notify();
            true
        } else {
            false
        }
    }

    /// Executes a single-click checker step from `point` using the preferred die
    /// and returns a [`CheckerFrame`] for immediate board animation + sound.
    pub fn click_point_local(
        &mut self,
        point: i32,
        base_board: &[i32; 26],
        human_player: u8,
        can_move: bool,
        review_active: bool,
    ) -> Option<CheckerFrame> {
        if review_active {
            return None;
        }
        let step = *self.choices(can_move).get(&point)?;
        let before = apply_absolute_steps(base_board, &self.step_pairs(), human_player);
        let frame = checker_frame(&before, step.from, step.to, human_player, self.dice());
        self.steps.push(step);
        self.hovered = Some(point);
        Some(frame)
    }

    pub fn click_point(
        &mut self,
        point: i32,
        base_board: &[i32; 26],
        human_player: u8,
        can_move: bool,
        review_active: bool,
        cx: &mut Context<Self>,
    ) -> Option<CheckerFrame> {
        let frame =
            self.click_point_local(point, base_board, human_player, can_move, review_active)?;
        cx.notify();
        Some(frame)
    }

    /// Undoes the last uncommitted step (`Z`), returning a **silent** reverse
    /// animation frame (`sound: None`) including restoring any hit blot.
    pub fn undo_local(
        &mut self,
        base_board: &[i32; 26],
        human_player: u8,
        locked: bool,
        animating: bool,
        review_active: bool,
    ) -> Option<CheckerFrame> {
        if self.steps.is_empty() || locked || animating || review_active {
            return None;
        }
        let popped = self.steps.pop()?;
        let board_before_popped =
            apply_absolute_steps(base_board, &self.step_pairs(), human_player);
        let forward = checker_frame(
            &board_before_popped,
            popped.from,
            popped.to,
            human_player,
            self.dice(),
        );
        self.hovered = None;
        Some(reverse_frame(&forward))
    }

    pub fn undo(
        &mut self,
        base_board: &[i32; 26],
        human_player: u8,
        locked: bool,
        animating: bool,
        review_active: bool,
        cx: &mut Context<Self>,
    ) -> Option<CheckerFrame> {
        let frame = self.undo_local(base_board, human_player, locked, animating, review_active)?;
        cx.notify();
        Some(frame)
    }

    /// Formats the currently staged steps in mover-relative notation (e.g. `"8/5 6/5"`).
    pub fn notation_summary(&self, locale: Locale, human_player: u8) -> String {
        let bar = I18n::t(locale, "play", "bar");
        let off = I18n::t(locale, "play", "off");
        self.steps
            .iter()
            .map(|step| {
                let from = if step.from == 0 || step.from == 25 {
                    bar.to_string()
                } else {
                    relative_point(step.from, human_player).to_string()
                };
                let to = if step.to < 0 || step.to > 25 {
                    off.to_string()
                } else {
                    relative_point(step.to, human_player).to_string()
                };
                format!("{from}/{to}")
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}
