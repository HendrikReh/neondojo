//! Synchronous click-to-move checker practice state machine powered directly by
//! `neondojo_rules::backgammon::checker_paths` and `neondojo_board`.
//!
//! Used by both **Similar Positions & Practice** and **All Analyzed Decisions**
//! checker practice without requiring any backend round-trip to compute legal turns.

use neondojo::model::Position;
use neondojo::play::PlayStep;
use neondojo_board::{
    BoardDiceInteraction, BoardInteractionState, BoardPosition, DieUsage, MovePreview,
    MovePreviewStep, apply_absolute_steps, project_move,
};
use neondojo_rules::backgammon::checker_paths;

/// One complete legal checker turn in absolute board coordinates.
#[derive(Debug, Clone)]
pub struct CheckerTurnOption {
    pub steps: Vec<PlayStep>,
    /// Die used by each step in matching order.
    pub dice: Vec<i32>,
}

/// Converts a backend [`Position`] (`Vec<i32>` board/dice) into a [`BoardPosition`].
pub fn position_to_board_position(pos: &Position, hide_notation: bool) -> BoardPosition {
    let mut board = [0i32; 26];
    for (dst, &src) in board.iter_mut().zip(pos.board.iter()) {
        *dst = src;
    }
    let dice: Vec<u8> = pos
        .dice
        .iter()
        .copied()
        .filter(|&d| (1..=6).contains(&d))
        .map(|d| d as u8)
        .collect();
    BoardPosition {
        board,
        player: u8::try_from(pos.player).unwrap_or(0).min(1),
        dice,
        gnubg_match_id: pos.gnubg_match_id.clone(),
        notation: if hide_notation {
            String::new()
        } else {
            pos.notation.clone()
        },
    }
}

/// Computes all legal complete checker turns synchronously using `neondojo_rules`.
///
/// An empty `Ok(vec![])` indicates a legal forced pass ("Cannot move").
pub fn compute_legal_checker_turns(
    board: &[i32; 26],
    player: i32,
    dice: [i32; 2],
) -> Result<Vec<CheckerTurnOption>, String> {
    let paths = checker_paths(board, player, dice).map_err(|e| e.to_string())?;
    let absolute = |pt: i32| if player == 0 { pt } else { 25 - pt };
    Ok(paths
        .into_iter()
        .map(|path| {
            let mut steps = Vec::with_capacity(path.steps.len());
            let mut step_dice = Vec::with_capacity(path.steps.len());
            for (from, to, die) in path.steps {
                steps.push(PlayStep {
                    from: absolute(from as i32),
                    to: absolute(if to == 0 { -1 } else { to as i32 }),
                });
                step_dice.push(die);
            }
            CheckerTurnOption {
                steps,
                dice: step_dice,
            }
        })
        .collect())
}

/// Interactive checker move builder mirroring `CheckerPracticeBoard.tsx`.
#[derive(Debug, Clone)]
pub struct CheckerPracticeDraft {
    pub initial_board: [i32; 26],
    pub player: u8,
    pub original_dice: [u8; 2],
    pub gnubg_match_id: String,
    pub turns: Vec<CheckerTurnOption>,
    pub turn_error: Option<String>,
    pub steps: Vec<PlayStep>,
    pub first_die: Option<u8>,
    pub hovered_source: Option<i32>,
}

impl CheckerPracticeDraft {
    /// Initializes a checker practice draft from a backend [`Position`].
    pub fn from_position(position: &Position) -> Self {
        let bp = position_to_board_position(position, true);
        let dice_pair = [
            bp.dice.first().copied().unwrap_or(0),
            bp.dice.get(1).copied().unwrap_or(0),
        ];
        let (turns, turn_error) = if dice_pair[0] >= 1 && dice_pair[1] >= 1 {
            match compute_legal_checker_turns(
                &bp.board,
                i32::from(bp.player),
                [i32::from(dice_pair[0]), i32::from(dice_pair[1])],
            ) {
                Ok(t) => (t, None),
                Err(err) => (Vec::new(), Some(err)),
            }
        } else {
            (Vec::new(), Some("Practice requires a checker roll".into()))
        };

        Self {
            initial_board: bp.board,
            player: bp.player,
            original_dice: dice_pair,
            gnubg_match_id: bp.gnubg_match_id,
            turns,
            turn_error,
            steps: Vec::new(),
            first_die: None,
            hovered_source: None,
        }
    }

    /// Displayed dice order, accounting for user dice swap.
    pub fn displayed_dice(&self) -> [u8; 2] {
        let preferred = self.first_die.or_else(|| {
            self.turns
                .first()
                .and_then(|t| t.dice.first())
                .map(|&d| d as u8)
        });
        if preferred == Some(self.original_dice[1])
            && self.original_dice[0] != self.original_dice[1]
        {
            [self.original_dice[1], self.original_dice[0]]
        } else {
            self.original_dice
        }
    }

    /// Legal turns matching the active first die.
    fn active_turns(&self) -> Vec<&CheckerTurnOption> {
        let active_die = i32::from(self.displayed_dice()[0]);
        self.turns
            .iter()
            .filter(|turn| turn.dice.first().copied() == Some(active_die))
            .collect()
    }

    /// Legal turns whose prefix matches `self.steps`.
    fn matching_turns(&self) -> Vec<&CheckerTurnOption> {
        self.active_turns()
            .into_iter()
            .filter(|turn| {
                turn.steps.len() >= self.steps.len()
                    && self
                        .steps
                        .iter()
                        .zip(turn.steps.iter())
                        .all(|(a, b)| a.from == b.from && a.to == b.to)
            })
            .collect()
    }

    /// Next legal points: when `source` is `None`, returns clickable source points;
    /// when `source` is `Some(from)`, returns destination points from `from`.
    pub fn next_points(&self, source: Option<i32>) -> Vec<i32> {
        let mut pts = Vec::new();
        for turn in self.matching_turns() {
            let Some(step) = turn.steps.get(self.steps.len()) else {
                continue;
            };
            let pt = match source {
                None => step.from,
                Some(from) if step.from == from => step.to,
                Some(_) => continue,
            };
            if !pts.contains(&pt) {
                pts.push(pt);
            }
        }
        pts
    }

    pub fn is_forced_pass(&self) -> bool {
        self.turn_error.is_none() && self.turns.is_empty()
    }

    pub fn can_swap_dice(&self, busy: bool) -> bool {
        let dice = self.displayed_dice();
        !busy
            && self.steps.is_empty()
            && dice[0] != dice[1]
            && self
                .turns
                .iter()
                .any(|turn| turn.dice.first().copied() == Some(i32::from(dice[1])))
    }

    pub fn can_submit(&self) -> bool {
        self.turn_error.is_none()
            && (self.is_forced_pass()
                || self
                    .matching_turns()
                    .iter()
                    .any(|turn| turn.steps.len() == self.steps.len()))
    }

    pub fn can_undo(&self) -> bool {
        !self.steps.is_empty()
    }

    /// Board after applying the currently drafted partial steps.
    pub fn current_board(&self) -> [i32; 26] {
        let pairs: Vec<(i32, i32)> = self.steps.iter().map(|s| (s.from, s.to)).collect();
        apply_absolute_steps(&self.initial_board, &pairs, self.player)
    }

    /// Current [`BoardPosition`] to pass to [`neondojo_board::BoardView`].
    pub fn board_position(&self) -> BoardPosition {
        let dice = self.displayed_dice();
        BoardPosition {
            board: self.current_board(),
            player: self.player,
            dice: dice.into_iter().filter(|&d| (1..=6).contains(&d)).collect(),
            gnubg_match_id: self.gnubg_match_id.clone(),
            notation: String::new(),
        }
    }

    /// Builds [`BoardInteractionState`] for [`neondojo_board::BoardView`].
    pub fn interaction_state(&self, busy: bool) -> BoardInteractionState {
        let sources = if busy {
            Vec::new()
        } else {
            self.next_points(None)
        };
        let destinations = match self.hovered_source {
            Some(from) if sources.contains(&from) => self.next_points(Some(from)),
            _ => Vec::new(),
        };
        let dice = self.displayed_dice();
        let played_dice: Vec<u8> = self
            .matching_turns()
            .first()
            .map(|turn| {
                turn.dice
                    .iter()
                    .take(self.steps.len())
                    .map(|&d| d as u8)
                    .collect()
            })
            .unwrap_or_default();
        let used = DieUsage::for_turn(&dice, &played_dice);
        let can_swap = self.can_swap_dice(busy);

        BoardInteractionState {
            enabled_points: sources.clone(),
            source_points: sources,
            destination_points: destinations,
            hovered_source: self.hovered_source,
            dice: Some(BoardDiceInteraction {
                can_swap,
                can_confirm: false,
                can_roll: false,
                can_pass: false,
                used,
                previous_roll: None,
                opening: None,
                rolling: false,
            }),
            can_double: false,
            cube_interactive: false,
        }
    }

    /// Clicks a checker source point, advancing one step along the active die.
    pub fn click_point(&mut self, from: i32, busy: bool) -> bool {
        if busy {
            return false;
        }
        let sources = self.next_points(None);
        if !sources.contains(&from) {
            return false;
        }
        let dests = self.next_points(Some(from));
        let Some(&to) = dests.first() else {
            return false;
        };
        self.steps.push(PlayStep { from, to });
        self.hovered_source = None;
        true
    }

    /// Swaps the active first die when no steps have been made yet.
    pub fn swap_dice(&mut self, busy: bool) -> bool {
        if !self.can_swap_dice(busy) {
            return false;
        }
        let dice = self.displayed_dice();
        self.first_die = Some(dice[1]);
        self.hovered_source = None;
        true
    }

    /// Pops the last played checker step.
    pub fn undo_step(&mut self) -> bool {
        if self.steps.pop().is_some() {
            self.hovered_source = None;
            true
        } else {
            false
        }
    }

    /// Resets the draft to the initial board.
    pub fn reset(&mut self) {
        self.steps.clear();
        self.hovered_source = None;
    }

    /// Projects the user's drafted steps into a [`MovePreview`] for the
    /// "Your move" arrow overlay (`BoardViewMode::Arrows`).
    pub fn project_draft_preview(&self) -> MovePreview {
        if self.steps.is_empty() {
            return MovePreview::Ok {
                board: self.initial_board,
                steps: Vec::new(),
                forced: self.is_forced_pass(),
            };
        }
        let matching = self.matching_turns();
        let step_dice = matching.first().map(|t| t.dice.as_slice()).unwrap_or(&[]);

        let mut sim = self.initial_board;
        let sign = if self.player == 0 { 1 } else { -1 };
        let opp_bar = if self.player == 0 { 0usize } else { 25usize };
        let mut preview_steps = Vec::with_capacity(self.steps.len());

        for (idx, step) in self.steps.iter().enumerate() {
            let hit = (1..=24).contains(&step.to) && sim[step.to as usize] == -sign;
            if (0..=25).contains(&step.from) {
                sim[step.from as usize] -= sign;
            }
            if (1..=24).contains(&step.to) {
                let target = step.to as usize;
                if hit {
                    sim[target] = 0;
                    sim[opp_bar] -= sign;
                }
                sim[target] += sign;
            }
            let die = step_dice
                .get(idx)
                .copied()
                .unwrap_or_else(|| (step.from - step.to).abs());
            preview_steps.push(MovePreviewStep {
                from: step.from,
                to: step.to,
                die,
                hit,
            });
        }

        MovePreview::Ok {
            board: sim,
            steps: preview_steps,
            forced: false,
        }
    }

    /// Projects an engine best-move notation string onto the initial position for
    /// the "Best play" arrow overlay (`BoardViewMode::Arrows`).
    pub fn project_notation_preview(&self, notation: &str) -> MovePreview {
        project_move(
            &self.initial_board,
            self.player,
            &self.original_dice,
            notation,
        )
    }
}
