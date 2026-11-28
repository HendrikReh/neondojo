//! Isolated Live Play board stage (`PlayBoardStage`).
//!
//! Architecture invariant:
//! - `PlayBoardStage` observes ONLY `Entity<PlayAnimationEntity>` and
//!   `Entity<PlayTurnEntity>`, isolating 60–120fps animation frame updates
//!   exclusively to the board stage so `PlayWorkspace`, match header, and
//!   `PlayHistory` NEVER re-render during checker flights or dice rolls.
//! - Owns the primary `Entity<BoardView>` (with `audio = None`, since
//!   `PlayAnimationEntity` is the single audio owner).
//! - Wires `BoardEvent::{PointClicked, SourceHovered, DiceClicked, CubeClicked}`
//!   for single-click-to-move (never drag-and-drop), hover destination dots,
//!   die swap/confirm/roll/pass, and cube doubling.

use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::icons::ActionIconName;
use crate::workspaces::play::animation::PlayAnimationEntity;
use crate::workspaces::play::history::PlayHistoryReviewState;
use crate::workspaces::play::interruption::{
    PlayActionKind, board_array, double_unavailable_reason_key, is_money_game_finished,
    play_feedback_state, position_to_board_position,
};
use crate::workspaces::play::turn::PlayTurnEntity;
use gpui_kit::{
    AppContext, ClickEvent, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Subscription, TestSupportExt, Window, base::StyledExt, div,
    prelude::FluentBuilder, px,
};
use neondojo::play::{PlayPhase, PlaySession};
use neondojo_board::{
    ActiveVisual, BearoffDirection, BoardDiceInteraction, BoardEvent, BoardInteractionState,
    BoardPosition, BoardThemeConfig, BoardView, BoardViewMode, DieUsage, MovePreview, PlayFrame,
    PlayOrientation, ThemeAtlasCache, apply_absolute_steps, project_move,
};
use neondojo_i18n::{I18n, Param, format_int};
use neondojo_rules::backgammon::initial_board;
use std::sync::{Arc, Mutex};

/// High-level user interactions emitted by [`PlayBoardStage`] up to `PlayWorkspace`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayStageEvent {
    Roll,
    Double,
    Confirm(usize),
    Pass,
    ContinueDecision,
    RequestHint,
    Undo,
    RetrySession,
}

/// Snapshot of non-animation props pushed from `PlayWorkspace` into [`PlayBoardStage`]
/// only when session, review, mode, theme, or interruption state changes.
#[derive(Clone)]
pub struct PlayStageSnapshot {
    pub session: Option<PlaySession>,
    pub idle_human: u8,
    pub idle_direction: BearoffDirection,
    pub theme_config: BoardThemeConfig,
    pub paused: bool,
    pub busy: bool,
    pub review: PlayHistoryReviewState,
    pub interrupted: bool,
    pub show_board_actions: bool,
    pub can_move: bool,
    pub roll_ready: bool,
    pub can_pass: bool,
    pub can_continue_coach_move: bool,
    pub can_continue_coach_pass: bool,
    pub opening_phase: bool,
    pub white_name: String,
    pub black_name: String,
}

impl Default for PlayStageSnapshot {
    fn default() -> Self {
        Self {
            session: None,
            idle_human: 0,
            idle_direction: BearoffDirection::Right,
            theme_config: BoardThemeConfig::default(),
            paused: false,
            busy: false,
            review: PlayHistoryReviewState::default(),
            interrupted: false,
            show_board_actions: false,
            can_move: false,
            roll_ready: false,
            can_pass: false,
            can_continue_coach_move: false,
            can_continue_coach_pass: false,
            opening_phase: false,
            white_name: "White".to_string(),
            black_name: "Black".to_string(),
        }
    }
}

/// Returns the bot's most recent dice roll in the current game so it stays
/// dimmed on the bot's half of the board until the human rolls (`PlayBoardStage.tsx:71`).
pub fn last_bot_roll(session: &PlaySession) -> Option<(u8, [u8; 2])> {
    if !matches!(session.phase, PlayPhase::Roll) {
        return None;
    }
    let last_result = session
        .game_results
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .find(|g| g.game_no == session.game_no - 1);
    if session.game_no > 1 && last_result.is_none() {
        return None;
    }
    let first = last_result.map_or(0, |r| r.history_index + 1);
    for turn in session.history.get(first..).unwrap_or(&[]).iter().rev() {
        if turn.player != session.human_player && turn.dice.len() == 2 {
            let d0 = u8::try_from(turn.dice[0]).ok()?;
            let d1 = u8::try_from(turn.dice[1]).ok()?;
            if (1..=6).contains(&d0) && (1..=6).contains(&d1) {
                return Some((u8::try_from(turn.player).unwrap_or(1).min(1), [d0, d1]));
            }
        }
    }
    None
}

/// Isolated GPUI view rendering the Live Play turn toolbar + GPU `BoardView`.
pub struct PlayBoardStage {
    board_view: Entity<BoardView>,
    animation: Entity<PlayAnimationEntity>,
    turn: Entity<PlayTurnEntity>,
    snapshot: PlayStageSnapshot,
    last_forwarded_batch_seq: u64,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PlayStageEvent> for PlayBoardStage {}

impl PlayBoardStage {
    pub fn new(
        atlas: Arc<Mutex<ThemeAtlasCache>>,
        animation: Entity<PlayAnimationEntity>,
        turn: Entity<PlayTurnEntity>,
        cx: &mut Context<Self>,
    ) -> Self {
        let board_view = cx.new(|_cx| {
            let mut bv = BoardView::new("play-live-board", atlas);
            // Audio is owned exclusively by PlayAnimationEntity (no duplicate cues).
            bv.set_audio(None);
            bv
        });

        let sub_anim = cx.observe(&animation, |this, anim, cx| {
            let seq = anim.read(cx).batch_seq();
            if seq != this.last_forwarded_batch_seq {
                this.last_forwarded_batch_seq = seq;
                let batch = anim.read(cx).last_enqueued_batch().to_vec();
                this.board_view.update(cx, |bv, cx| {
                    if batch.is_empty() {
                        bv.cancel_animation(cx);
                    } else {
                        bv.enqueue_animation(batch, cx);
                    }
                });
            }
            cx.notify();
        });

        let sub_turn = cx.observe(&turn, |_this, _turn, cx| {
            cx.notify();
        });

        let sub_board = cx.subscribe(&board_view, |this, _bv, event: &BoardEvent, cx| {
            this.handle_board_event(*event, cx);
        });

        Self {
            board_view,
            animation,
            turn,
            snapshot: PlayStageSnapshot::default(),
            last_forwarded_batch_seq: 0,
            _subscriptions: vec![sub_anim, sub_turn, sub_board],
        }
    }

    pub fn board_view(&self) -> &Entity<BoardView> {
        &self.board_view
    }

    /// Updates stage props from `PlayWorkspace` on session/mode/review changes.
    pub fn update_snapshot(&mut self, snapshot: PlayStageSnapshot, cx: &mut Context<Self>) {
        self.snapshot = snapshot;
        cx.notify();
    }

    fn handle_board_event(&mut self, event: BoardEvent, cx: &mut Context<Self>) {
        let Some(session) = self.snapshot.session.clone() else {
            return;
        };
        let review_active = self.snapshot.review.is_active(Some(&session));
        let animating = self.animation.read(cx).is_animating();
        let ui_busy = self.snapshot.busy || animating;
        if review_active {
            return;
        }

        match event {
            BoardEvent::SourceHovered(pt) => {
                self.turn.update(cx, |t, cx| t.set_hover(pt, cx));
            }
            BoardEvent::PointClicked(pt) => {
                if !self.snapshot.can_move || ui_busy {
                    return;
                }
                let base_board = board_array(&session.position.board);
                let human = u8::try_from(session.human_player).unwrap_or(0).min(1);
                let frame = self.turn.update(cx, |t, cx| {
                    t.click_point(pt, &base_board, human, self.snapshot.can_move, false, cx)
                });
                if let Some(checker) = frame {
                    self.animation.update(cx, |anim, cx| {
                        anim.enqueue(vec![PlayFrame::Checker(checker)], None, cx);
                    });
                }
            }
            BoardEvent::CubeClicked => {
                let fb = play_feedback_state(Some(&session));
                if self.snapshot.roll_ready && session.can_double && fb.routine_roll.is_none() {
                    cx.emit(PlayStageEvent::Double);
                }
            }
            BoardEvent::DiceClicked => {
                if ui_busy {
                    return;
                }
                let prev_roll = last_bot_roll(&session);
                if self.snapshot.roll_ready && prev_roll.is_some() {
                    cx.emit(PlayStageEvent::Roll);
                    return;
                }
                if self.snapshot.can_continue_coach_pass {
                    cx.emit(PlayStageEvent::ContinueDecision);
                    return;
                }
                if self.snapshot.can_pass {
                    cx.emit(PlayStageEvent::Pass);
                    return;
                }
                if self.snapshot.can_continue_coach_move {
                    cx.emit(PlayStageEvent::ContinueDecision);
                    return;
                }
                let completed = self
                    .turn
                    .read(cx)
                    .completed_move_index(self.snapshot.can_move);
                if let Some(move_index) = completed {
                    cx.emit(PlayStageEvent::Confirm(move_index));
                    return;
                }
                let can_move = self.snapshot.can_move;
                self.turn.update(cx, |t, cx| {
                    t.swap_dice(can_move, animating, false, cx);
                });
            }
        }
    }

    fn sync_board_view(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.snapshot.session.as_ref() else {
            // Idle board shown behind empty-state PlaySetupDialog.
            let idle_pos = BoardPosition {
                board: initial_board(),
                player: self.snapshot.idle_human,
                dice: Vec::new(),
                gnubg_match_id: "MAAAAAAAAAAA".to_string(),
                notation: String::new(),
            };
            let orientation = PlayOrientation {
                human: self.snapshot.idle_human,
                direction: self.snapshot.idle_direction,
            };
            let theme_cfg = self.snapshot.theme_config.clone();
            let white = self.snapshot.white_name.clone();
            let black = self.snapshot.black_name.clone();
            self.board_view.update(cx, |bv, cx| {
                bv.set_theme_config(theme_cfg, cx);
                bv.set_play_orientation(Some(orientation), cx);
                bv.set_players(white, black, cx);
                bv.set_position(idle_pos, cx);
                bv.set_preview(None, BoardViewMode::Before, cx);
                bv.set_cube_offer_pending(false, cx);
                bv.set_show_pip_counts(false, cx);
                bv.set_show_player_bars(false, cx);
                bv.set_interaction(None, cx);
                bv.set_review_outline(None, cx);
            });
            return;
        };

        let anim_ref = self.animation.read(cx);
        let animating = anim_ref.is_animating();
        let pending_roll = anim_ref.pending_roll();
        let visual = anim_ref.current_visual().cloned();

        let fb = play_feedback_state(Some(session));
        let review_active = self.snapshot.review.is_active(Some(session));
        let historical = self.snapshot.review.entry(Some(session));
        let history_view = self.snapshot.review.effective_view_mode(Some(session));

        let turn_ref = self.turn.read(cx);
        let human_u8 = u8::try_from(session.human_player).unwrap_or(0).min(1);
        let selected_steps: Vec<(i32, i32)> = if !self.snapshot.paused
            && fb.pending.and_then(|d| d.action.as_deref()) == Some("move")
            && let Some(idx) = fb.pending.and_then(|d| d.move_index)
            && let Some(mv) = session.legal_moves.get(idx)
        {
            mv.steps.iter().map(|s| (s.from, s.to)).collect()
        } else {
            turn_ref.step_pairs()
        };
        let draft_dice = turn_ref.dice();
        let sources = turn_ref.sources(self.snapshot.can_move && !animating);
        let destinations = turn_ref.destinations(self.snapshot.can_move && !animating);
        let hovered = turn_ref.hovered();
        let completed = turn_ref.completed_move_index(self.snapshot.can_move);
        let can_swap = turn_ref.can_swap(self.snapshot.can_move, animating);
        let used_dice = turn_ref.used_dice();

        let base_board = board_array(&session.position.board);
        let live_board = apply_absolute_steps(&base_board, &selected_steps, human_u8);

        let (position, preview, view_mode) = if let Some(snap) = historical {
            let pos = if snap.kind == "checker" || history_view == BoardViewMode::Before {
                position_to_board_position(&snap.before)
            } else {
                position_to_board_position(&snap.after)
            };
            let prev = if snap.kind == "checker" {
                let before_b = board_array(&snap.before.board);
                let after_b = board_array(&snap.after.board);
                let player_u8 = u8::try_from(snap.before.player).unwrap_or(0).min(1);
                let dice_u8: Vec<u8> = snap
                    .before
                    .dice
                    .iter()
                    .filter_map(|&d| u8::try_from(d).ok())
                    .collect();
                let proj = project_move(&before_b, player_u8, &dice_u8, &snap.before.notation);
                Some(MovePreview::Ok {
                    board: after_b,
                    steps: proj.steps().map(|s| s.to_vec()).unwrap_or_default(),
                    forced: matches!(proj, MovePreview::Ok { forced: true, .. }),
                })
            } else {
                None
            };
            (pos, prev, history_view)
        } else {
            let dice = if self.snapshot.opening_phase {
                Vec::new()
            } else {
                draft_dice.clone()
            };
            let mut pos = position_to_board_position(&session.position);
            pos.board = live_board;
            pos.dice = dice;
            (pos, None, BoardViewMode::Before)
        };

        let opening_values = if self.snapshot.opening_phase {
            match &visual {
                Some(ActiveVisual::Dice {
                    opening: true,
                    dice,
                    ..
                }) => Some([dice.first().copied(), dice.get(1).copied()]),
                _ if pending_roll == Some(PlayActionKind::OpeningRoll) => Some([Some(3), Some(5)]),
                _ if matches!(session.phase, PlayPhase::OpeningResult) => {
                    session.opening.as_ref().map(|op| {
                        [
                            u8::try_from(op.white_die).ok(),
                            u8::try_from(op.black_die).ok(),
                        ]
                    })
                }
                _ => Some([None, None]),
            }
        } else {
            None
        };

        let prev_roll = if !review_active && !animating && pending_roll.is_none() {
            last_bot_roll(session)
        } else {
            None
        };

        let can_confirm =
            (self.snapshot.can_move && !self.snapshot.busy && !animating && completed.is_some())
                || self.snapshot.can_continue_coach_move;
        let may_double = session.can_double && fb.routine_roll.is_none();

        let interaction = if review_active {
            None
        } else {
            Some(BoardInteractionState {
                enabled_points: sources.clone(),
                source_points: sources,
                destination_points: destinations,
                hovered_source: hovered,
                dice: Some(BoardDiceInteraction {
                    can_swap,
                    can_confirm,
                    can_roll: self.snapshot.roll_ready && prev_roll.is_some(),
                    can_pass: self.snapshot.can_pass || self.snapshot.can_continue_coach_pass,
                    used: if animating || pending_roll.is_some() {
                        [DieUsage::Unused, DieUsage::Unused]
                    } else if prev_roll.is_some() {
                        [DieUsage::Used, DieUsage::Used]
                    } else {
                        used_dice
                    },
                    previous_roll: prev_roll,
                    opening: opening_values,
                    rolling: matches!(visual, Some(ActiveVisual::Dice { rolling: true, .. }))
                        || matches!(
                            pending_roll,
                            Some(PlayActionKind::Roll | PlayActionKind::OpeningRoll)
                        ),
                }),
                can_double: self.snapshot.roll_ready && may_double,
                cube_interactive: self.snapshot.roll_ready,
            })
        };

        let direction = if session.bearoff_direction == "left" {
            BearoffDirection::Left
        } else {
            BearoffDirection::Right
        };
        let orientation = PlayOrientation {
            human: human_u8,
            direction,
        };
        let show_pips = !fb.competition
            || matches!(session.phase, PlayPhase::MatchOver)
            || is_money_game_finished(Some(session));
        let theme_cfg = self.snapshot.theme_config.clone();
        let white = self.snapshot.white_name.clone();
        let black = self.snapshot.black_name.clone();
        let review_outline = review_active.then(|| NeonTheme::current_palette(cx).accent);
        let cube_pending = matches!(session.phase, PlayPhase::DoubleResponse);

        self.board_view.update(cx, |bv, cx| {
            bv.set_theme_config(theme_cfg, cx);
            bv.set_play_orientation(Some(orientation), cx);
            bv.set_players(white, black, cx);
            bv.set_position(position, cx);
            bv.set_preview(preview, view_mode, cx);
            bv.set_cube_offer_pending(cube_pending, cx);
            bv.set_show_pip_counts(show_pips, cx);
            bv.set_show_player_bars(true, cx);
            bv.set_interaction(interaction, cx);
            bv.set_review_outline(review_outline, cx);
        });
        let _ = window;
    }

    fn render_turn_toolbar(
        &self,
        p: NeonPalette,
        cx: &mut Context<Self>,
    ) -> Option<gpui_kit::AnyElement> {
        let session = self.snapshot.session.as_ref()?;
        if !self.snapshot.show_board_actions {
            return None;
        }
        let locale = current_locale(cx);
        let anim_busy = self.animation.read(cx).is_animating();
        let ui_busy = self.snapshot.busy || anim_busy;
        let fb = play_feedback_state(Some(session));
        let before_roll = matches!(session.phase, PlayPhase::Roll);
        let turn = self.turn.read(cx);
        let forced_pass = turn.forced_pass(self.snapshot.can_move);
        let steps_len = turn.steps().len();
        let completed_idx = turn.completed_move_index(self.snapshot.can_move);
        let missing_paths = turn.missing_checker_paths(self.snapshot.can_move);
        let human_u8 = u8::try_from(session.human_player).unwrap_or(0).min(1);
        let notation = turn.notation_summary(locale, human_u8);

        let heading_key = if before_roll {
            "yourTurn"
        } else if forced_pass {
            "pass"
        } else {
            "move"
        };
        let can_confirm = self.snapshot.can_move && !ui_busy && completed_idx.is_some();
        let only_move = (self.snapshot.can_move
            && !before_roll
            && !forced_pass
            && session.legal_moves.len() == 1)
            .then(|| session.legal_moves[0].notation.clone());

        let main_btn = if before_roll {
            let disabled = !self.snapshot.roll_ready || ui_busy;
            ActionButton::new("play-action-roll")
                .icon(ActionIconName::Roll)
                .label(I18n::t(locale, "play", "roll"))
                .shortcut("Enter")
                .primary()
                .disabled(disabled)
                .palette(p)
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(PlayStageEvent::Roll)))
        } else if forced_pass {
            let disabled = !self.snapshot.can_pass || ui_busy;
            ActionButton::new("play-action-pass")
                .icon(ActionIconName::Pass)
                .label(I18n::t(locale, "play", "passTurn"))
                .shortcut("Enter")
                .primary()
                .disabled(disabled)
                .palette(p)
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(PlayStageEvent::Pass)))
        } else {
            let move_idx = completed_idx;
            ActionButton::new("play-action-confirm")
                .icon(ActionIconName::Confirm)
                .label(I18n::t(locale, "play", "confirm"))
                .shortcut("Enter")
                .primary()
                .disabled(!can_confirm)
                .palette(p)
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    if let Some(idx) = move_idx {
                        cx.emit(PlayStageEvent::Confirm(idx));
                    }
                }))
        };

        let show_hint = !fb.competition
            && fb.routine_roll.is_none()
            && !forced_pass
            && (!before_roll || session.can_double);

        let double_reason = I18n::t(locale, "play", double_unavailable_reason_key(Some(session)));

        let help_text = if !before_roll {
            if forced_pass {
                Some(I18n::t(locale, "play", "passReady").to_string())
            } else if completed_idx.is_some() {
                Some(I18n::t(locale, "play", "ready").to_string())
            } else if let Some(mv) = only_move {
                Some(I18n::format(
                    locale,
                    "play",
                    "onlyMove",
                    &[("move", Param::from(mv))],
                ))
            } else {
                Some(I18n::t(locale, "play", "select").to_string())
            }
        } else {
            None
        };

        Some(
            div()
                .id("play-board-toolbar")
                .test_support()
                .v_flex()
                .gap(px(6.0))
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .child(
                    div()
                        .h_flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().text_panel().text_color(p.text).child(I18n::t(
                            locale,
                            "play",
                            heading_key,
                        )))
                        .child(main_btn)
                        .when(before_roll && fb.routine_roll.is_none(), |this| {
                            this.child(
                                ActionButton::new("play-action-double")
                                    .icon(ActionIconName::Double)
                                    .label(I18n::t(locale, "play", "double"))
                                    .shortcut("D")
                                    .disabled(
                                        !self.snapshot.roll_ready || ui_busy || !session.can_double,
                                    )
                                    .palette(p)
                                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                        cx.emit(PlayStageEvent::Double);
                                    })),
                            )
                        })
                        .when(show_hint, |this| {
                            this.child(
                                ActionButton::new("play-action-hint")
                                    .icon(ActionIconName::Hint)
                                    .label(I18n::t(locale, "play-mode", "hint"))
                                    .shortcut("H")
                                    .disabled(ui_busy)
                                    .palette(p)
                                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                        cx.emit(PlayStageEvent::RequestHint);
                                    })),
                            )
                        })
                        .when(!before_roll && !forced_pass, |this| {
                            let undo_label = if steps_len > 0 {
                                format!(
                                    "{} ({})",
                                    I18n::t(locale, "play", "undo"),
                                    format_int(locale, steps_len as i64)
                                )
                            } else {
                                I18n::t(locale, "play", "undo").to_string()
                            };
                            this.child(
                                ActionButton::new("play-action-undo")
                                    .icon(ActionIconName::Undo)
                                    .label(undo_label)
                                    .shortcut("Z")
                                    .disabled(ui_busy || !self.snapshot.can_move || steps_len == 0)
                                    .palette(p)
                                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                        cx.emit(PlayStageEvent::Undo);
                                    })),
                            )
                        })
                        .when(!notation.is_empty(), |this| {
                            this.child(
                                div()
                                    .id("play-selected-steps")
                                    .test_support()
                                    .px(px(8.0))
                                    .py(px(3.0))
                                    .rounded(px(5.0))
                                    .bg(p.surface_alt)
                                    .border_1()
                                    .border_color(p.border)
                                    .text_table()
                                    .mono_font()
                                    .text_color(p.text)
                                    .child(notation),
                            )
                        }),
                )
                .when(before_roll && !session.can_double, |this| {
                    this.child(div().text_meta().text_color(p.muted).child(double_reason))
                })
                .when(missing_paths, |this| {
                    this.child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(px(6.0))
                            .bg(p.danger_bg)
                            .text_color(p.danger_text)
                            .child(div().text_meta().child(I18n::t(
                                locale,
                                "play",
                                "checkerTurnsError",
                            )))
                            .child(
                                ActionButton::new("play-paths-retry")
                                    .icon(ActionIconName::Refresh)
                                    .label(I18n::t(locale, "play", "retry"))
                                    .palette(p)
                                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                        cx.emit(PlayStageEvent::RetrySession);
                                    })),
                            ),
                    )
                })
                .when_some(help_text, |this, text| {
                    this.child(div().text_meta().text_color(p.muted).child(text))
                })
                .into_any_element(),
        )
    }
}

impl Render for PlayBoardStage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_board_view(window, cx);
        let p = NeonTheme::current_palette(cx);
        let toolbar = self.render_turn_toolbar(p, cx);

        let is_idle = self.snapshot.session.is_none();

        div()
            .id("play-board-stage")
            .test_support()
            .v_flex()
            .flex_1()
            .size_full()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .gap(px(8.0))
            .when_some(toolbar, |this, tb| this.child(tb))
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .min_h(px(280.0))
                    .when(is_idle, |this| this.opacity(0.55))
                    .child(self.board_view.clone()),
            )
    }
}
