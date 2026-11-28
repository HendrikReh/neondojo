//! FSRS Review (`FsrsReviewView`), Non-FSRS Practice Rounds (`CardPracticeRound`),
//! Special Practice Modals (`Pip Count`, `Cube Score Variants`, `Timed Preview`),
//! and `TransferPractice`.
//!
//! Enforces NeonDojo's core learning invariants:
//! - Checking an answer (`submit_fsrs_attempt`) is strictly distinct from
//!   submitting an FSRS rating (`submit_fsrs_grade`, `1`–`4`).
//! - Idempotent UUID `request_id` per FSRS rating/attempt submission: retries
//!   for the same `(session_id, rating)` reuse the same UUID, while changing
//!   rating or advancing to a new card mints a fresh UUID.
//! - `409 Conflict` reloads the current card cleanly.
//! - Scoped [`AnkiCardFaceView`] is rendered ONLY when `card.kind == "anki"`.

use super::common::{
    ALL_GAME_PLANS, CheckerExerciseController, UiAction, UiCallback, UiCallback2, UiCallback3,
    cube_eval_to_view_data, format_interval, game_plan_label, position_to_board_position,
    preview_for_notation, rating_label, verdict_label,
};
use crate::anki_webview::{AnkiCardFaceRequest, AnkiCardFaceView, AnkiCardSide};
use crate::i18n::current_locale;
use crate::keybindings::CTX_STUDY_EXERCISE;
use crate::review::cube_details::CubeDecisionDetailsView;
use crate::review::move_explanation::{MoveExplanationView, explain_checker_plays};
use crate::review::preview_controls::PreviewBoardControls;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::banners::{BannerKind, EmptyState, StatusBanner};
use crate::ui::icons::ActionIconName;
use crate::ui::modal::{ModalBusyLock, ModalConfig, ModalSize, open_modal_dialog};
use gpui_kit::{
    App, AppContext, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, Styled, TestSupportExt, Window,
    base::StyledExt,
    component::WindowExt,
    component::input::{Input, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::facade::BackendFacade;
use neondojo::game_plan::GamePlan;
use neondojo::learning::{
    AttemptedCard, CardAnswer, CardBrowseItem, CardKind, CardQuestion, NextCard, PipResult,
    PracticeRound, Rating, RevealedCard, ReviewResult, ScorePractice, TransferSummary,
};
use neondojo::practice::evaluation::{
    CubeOfferAction, CubeResponseAction, PracticeAnswer, PracticeEvaluation, PracticeVerdict,
};
use neondojo_board::{BoardView, BoardViewMode};
use neondojo_i18n::{
    I18n, Param, format_datetime_medium_local, format_decimal, format_int, localize_decision,
};
use std::rc::Rc;
use uuid::Uuid;

/// Tracks which move arrows are previewed after checking/revealing an answer.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ArrowComparisonTarget {
    #[default]
    BestPlay,
    YourMove,
}

/// Idempotent request-ID tracker for FSRS review submissions (`review_card`).
///
/// Reuses the same `request_id` when retrying the exact same `(session_id, rating)`,
/// and generates a new UUID whenever either `session_id` or `rating` changes.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FsrsIdempotencyTracker {
    pending_grade: Option<(Uuid, Rating, Uuid)>,
    pending_attempt: Option<(Uuid, i64, Uuid)>,
}

impl FsrsIdempotencyTracker {
    /// Returns the idempotent `request_id` for submitting an FSRS `rating` on `session_id`.
    pub fn grade_request_id(&mut self, session_id: Uuid, rating: Rating) -> Uuid {
        if let Some((s, r, req_id)) = self.pending_grade
            && s == session_id
            && r == rating
        {
            return req_id;
        }
        let fresh = Uuid::new_v4();
        self.pending_grade = Some((session_id, rating, fresh));
        fresh
    }

    /// Returns the idempotent `request_id` for checking an answer attempt on `(card_id, version)`.
    pub fn attempt_request_id(&mut self, card_id: Uuid, version: i64) -> Uuid {
        if let Some((c, v, req_id)) = self.pending_attempt
            && c == card_id
            && v == version
        {
            return req_id;
        }
        let fresh = Uuid::new_v4();
        self.pending_attempt = Some((card_id, version, fresh));
        fresh
    }

    /// Clears pending IDs after a card succeeds or reloads (e.g. after 409 conflict).
    pub fn reset(&mut self) {
        self.pending_grade = None;
        self.pending_attempt = None;
    }
}

/// Complete state machine for an active FSRS study session inside a deck.
pub struct FsrsReviewState {
    pub active: bool,
    pub deck_id: Option<Uuid>,
    pub deck_name: String,
    pub loading: bool,
    pub busy: bool,
    pub attempt_busy: bool,
    pub error: Option<String>,
    pub conflict_notice: Option<String>,
    pub next: Option<NextCard>,
    pub revealed: Option<RevealedCard>,
    pub last_review: Option<ReviewResult>,
    pub idempotency: FsrsIdempotencyTracker,
    pub checker: Option<CheckerExerciseController>,
    pub selected_cube_offer: Option<CubeOfferAction>,
    pub selected_cube_response: Option<CubeResponseAction>,
    pub arrow_target: ArrowComparisonTarget,
    pub preview_mode: BoardViewMode,
    pub board_view: Entity<BoardView>,
    pub anki_face_view: Entity<AnkiCardFaceView>,
}

impl FsrsReviewState {
    pub fn new(board_view: Entity<BoardView>, cx: &mut App) -> Self {
        let anki_face_view = cx.new(|cx| AnkiCardFaceView::new(None, cx));
        Self {
            active: false,
            deck_id: None,
            deck_name: String::new(),
            loading: false,
            busy: false,
            attempt_busy: false,
            error: None,
            conflict_notice: None,
            next: None,
            revealed: None,
            last_review: None,
            idempotency: FsrsIdempotencyTracker::default(),
            checker: None,
            selected_cube_offer: None,
            selected_cube_response: None,
            arrow_target: ArrowComparisonTarget::BestPlay,
            preview_mode: BoardViewMode::Arrows,
            board_view,
            anki_face_view,
        }
    }

    /// Returns the currently active card question, if any.
    pub fn current_card(&self) -> Option<&CardQuestion> {
        self.next.as_ref().and_then(|n| n.card.as_ref())
    }

    /// Returns the active [`RevealedCard`] whether revealed directly or via an attempt check.
    pub fn effective_revealed(&self) -> Option<&RevealedCard> {
        self.revealed.as_ref().or_else(|| {
            self.next
                .as_ref()?
                .active_attempt
                .as_ref()
                .map(|a| &a.revealed)
        })
    }

    /// Returns the active [`AttemptedCard`] if the user checked an answer (distinct from FSRS rating!).
    pub fn active_attempt(&self) -> Option<&AttemptedCard> {
        self.next.as_ref()?.active_attempt.as_ref()
    }

    /// True ONLY when an answer is revealed and ready to accept an FSRS rating (`1`–`4`).
    pub fn can_submit_rating(&self) -> bool {
        self.active && !self.busy && !self.attempt_busy && self.effective_revealed().is_some()
    }

    /// True when the current card is a third-party Anki HTML/JS card.
    pub fn is_anki_card(&self) -> bool {
        self.current_card().is_some_and(|card| card.kind == "anki")
    }

    /// Builds a [`PracticeAnswer`] from the user's current interactive selection, if complete.
    pub fn build_pending_answer(&self) -> Option<PracticeAnswer> {
        let card = self.current_card()?;
        match card.kind.as_str() {
            "checker" => {
                let ctrl = self.checker.as_ref()?;
                ctrl.is_complete().then(|| PracticeAnswer::Checker {
                    steps: ctrl.chosen_steps.clone(),
                })
            }
            "cube_offer" => self
                .selected_cube_offer
                .map(|action| PracticeAnswer::CubeOffer { action }),
            "cube_response" => self
                .selected_cube_response
                .map(|action| PracticeAnswer::CubeResponse { action }),
            _ => None,
        }
    }

    /// Resets per-card state when loading a newly fetched [`NextCard`].
    pub fn apply_next_card(
        &mut self,
        next: NextCard,
        facade: Option<&BackendFacade>,
        cx: &mut App,
    ) {
        self.idempotency.reset();
        self.revealed = None;
        self.selected_cube_offer = None;
        self.selected_cube_response = None;
        self.arrow_target = ArrowComparisonTarget::BestPlay;
        self.preview_mode = BoardViewMode::Arrows;
        self.error = None;
        self.loading = false;

        if let Some(card) = &next.card {
            if card.kind == "checker" {
                self.checker = card
                    .position
                    .as_ref()
                    .map(|pos| CheckerExerciseController::from_position(pos, facade));
            } else {
                self.checker = None;
            }
        } else {
            self.checker = None;
        }
        self.next = Some(next);
        self.sync_visuals(cx);
    }

    /// Applies an [`AttemptedCard`] result (checking an answer without rating FSRS).
    pub fn apply_attempt(&mut self, attempt: AttemptedCard, cx: &mut App) {
        self.attempt_busy = false;
        self.error = None;
        if let Some(next) = self.next.as_mut()
            && next.card.as_ref().is_some_and(|c| {
                c.id == attempt.revealed.card_id && c.version == attempt.revealed.version
            })
        {
            next.active_attempt = Some(attempt);
        }
        self.sync_visuals(cx);
    }

    /// Applies a direct reveal (`Show answer` without submitting a practice attempt).
    pub fn apply_revealed(&mut self, revealed: RevealedCard, cx: &mut App) {
        self.busy = false;
        self.error = None;
        self.revealed = Some(revealed);
        self.sync_visuals(cx);
    }

    /// Handles a `409 Conflict` by clearing stale state and flagging reload recovery.
    pub fn handle_conflict_reload(&mut self, message: String, cx: &mut App) {
        self.busy = false;
        self.attempt_busy = false;
        self.revealed = None;
        self.next = None;
        self.checker = None;
        self.idempotency.reset();
        self.conflict_notice = Some(message);
        self.anki_face_view
            .update(cx, |v, cx| v.set_request(None, cx));
    }

    /// Tears down the active FSRS session and hides any mounted `WKWebView`.
    pub fn stop(&mut self, cx: &mut App) {
        self.active = false;
        self.loading = false;
        self.busy = false;
        self.attempt_busy = false;
        self.next = None;
        self.revealed = None;
        self.checker = None;
        self.idempotency.reset();
        self.anki_face_view.update(cx, |v, cx| {
            v.set_request(None, cx);
            v.set_visible(false, cx);
        });
    }

    /// Synchronizes `BoardView` (interactive state or Your Move vs Best Play arrows)
    /// and strictly scoped `AnkiCardFaceView` (`card.kind == "anki"` only!).
    pub fn sync_visuals(&self, cx: &mut App) {
        let Some(card) = self.current_card() else {
            self.anki_face_view
                .update(cx, |v, cx| v.set_request(None, cx));
            return;
        };

        let is_revealed = self.effective_revealed().is_some();

        // Strictly mount AnkiCardFaceView ONLY for third-party HTML/JS Anki cards.
        if card.kind == "anki" {
            let side = if is_revealed {
                AnkiCardSide::Back
            } else {
                AnkiCardSide::Front
            };
            let req = AnkiCardFaceRequest::new(card.id, side);
            self.anki_face_view.update(cx, |v, cx| {
                v.set_visible(self.active, cx);
                v.set_request(Some(req), cx);
            });
            return;
        }

        // Non-Anki card: ensure WKWebView is torn down.
        self.anki_face_view
            .update(cx, |v, cx| v.set_request(None, cx));

        let Some(pos) = &card.position else {
            return;
        };
        let mut bp = position_to_board_position(pos);
        let white = card.players[0].clone();
        let black = card.players[1].clone();
        let cube_pending = card.kind == "cube_response";

        if !is_revealed && card.kind == "checker" {
            if let Some(ctrl) = &self.checker {
                bp.board = ctrl.current_board();
                let interaction = ctrl.interaction_state();
                self.board_view.update(cx, |bv, cx| {
                    bv.set_players(white, black, cx);
                    bv.set_cube_offer_pending(false, cx);
                    bv.set_position(bp, cx);
                    bv.set_preview(None, BoardViewMode::Before, cx);
                    bv.set_interaction(Some(interaction), cx);
                });
            }
        } else {
            // Revealed or cube decision: show Your Move vs Best Play arrows for checker cards.
            let preview = if is_revealed && card.kind == "checker" {
                let notation = match self.arrow_target {
                    ArrowComparisonTarget::YourMove => self
                        .active_attempt()
                        .map(|a| a.evaluation.chosen.as_str())
                        .or_else(|| self.effective_revealed().map(|r| r.answer.played.as_str())),
                    ArrowComparisonTarget::BestPlay => self
                        .active_attempt()
                        .map(|a| a.evaluation.best.as_str())
                        .or_else(|| {
                            self.effective_revealed()
                                .and_then(|r| r.answer.solution.as_ref())
                                .and_then(|s| {
                                    s.decision.as_ref().map(|d| d.best.as_str()).or_else(|| {
                                        s.candidates.first().map(|c| c.notation.as_str())
                                    })
                                })
                        }),
                };
                notation.and_then(|n| preview_for_notation(pos, n))
            } else {
                None
            };
            let mode = if preview.is_some() {
                self.preview_mode
            } else {
                BoardViewMode::Before
            };
            self.board_view.update(cx, |bv, cx| {
                bv.set_players(white, black, cx);
                bv.set_cube_offer_pending(cube_pending, cx);
                bv.set_position(bp, cx);
                bv.set_interaction(None, cx);
                bv.set_preview(preview, mode, cx);
            });
        }
    }
}

/// Callbacks wired from `StudyWorkspace` into `render_fsrs_review`.
pub struct FsrsReviewCallbacks {
    pub on_back: UiAction,
    pub on_reveal: UiAction,
    pub on_check_answer: UiAction,
    pub on_undo_checker: UiAction,
    pub on_select_cube_offer: UiCallback<CubeOfferAction>,
    pub on_select_cube_response: UiCallback<CubeResponseAction>,
    pub on_rate: UiCallback<Rating>,
    pub on_arrow_target: UiCallback<ArrowComparisonTarget>,
    pub on_preview_mode: UiCallback<BoardViewMode>,
    pub on_reload: UiAction,
}

/// Renders the full FSRS review interface (`FsrsReviewView`).
pub fn render_fsrs_review(
    state: &FsrsReviewState,
    palette: NeonPalette,
    cb: FsrsReviewCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);

    let header = div()
        .h_flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .pb(px(10.0))
        .border_b_1()
        .border_color(palette.border)
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(10.0))
                .child({
                    let on_back = cb.on_back.clone();
                    ActionButton::new("fsrs-back-btn")
                        .icon(ActionIconName::Back)
                        .label(I18n::t(locale, "learning", "back"))
                        .palette(palette)
                        .on_click(move |_, window, cx| on_back(window, cx))
                })
                .child(
                    div()
                        .v_flex()
                        .child(
                            div()
                                .text_subject()
                                .text_color(palette.text)
                                .child(state.deck_name.clone()),
                        )
                        .child(div().text_meta().text_color(palette.muted).child(I18n::t(
                            locale,
                            "learning",
                            "retention",
                        ))),
                ),
        )
        .when_some(state.next.as_ref().map(|n| &n.counts), |this, counts| {
            this.child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(10.0))
                    .text_meta()
                    .tabular_nums()
                    .text_color(palette.muted)
                    .child(I18n::format(
                        locale,
                        "learning",
                        "due",
                        &[("count", Param::from(format_int(locale, counts.due)))],
                    ))
                    .child("·")
                    .child(I18n::format(
                        locale,
                        "learning",
                        "newCards",
                        &[("count", Param::from(format_int(locale, counts.new)))],
                    ))
                    .child(format!(
                        "({})",
                        I18n::format(
                            locale,
                            "learning",
                            "newToday",
                            &[(
                                "count",
                                Param::from(format_int(locale, counts.new_available))
                            )],
                        )
                    )),
            )
        });

    let mut root = div()
        .id("study-fsrs-review")
        .test_support()
        .key_context(CTX_STUDY_EXERCISE)
        .v_flex()
        .gap(px(14.0))
        .w_full()
        .child(header);

    if let Some(notice) = &state.conflict_notice {
        root = root.child(
            StatusBanner::new("fsrs-conflict-banner", BannerKind::Warning, notice.clone())
                .palette(palette),
        );
    }
    if let Some(err) = &state.error {
        let on_reload = cb.on_reload.clone();
        root = root.child(
            StatusBanner::new("fsrs-error-banner", BannerKind::Error, err.clone())
                .palette(palette)
                .on_retry(I18n::t(locale, "study", "retry"), move |_, window, cx| {
                    on_reload(window, cx)
                }),
        );
    }

    if let Some(last) = &state.last_review {
        root = root.child(
            div()
                .text_meta()
                .text_color(palette.success_text)
                .child(I18n::format(
                    locale,
                    "learning",
                    "nextDue",
                    &[(
                        "date",
                        Param::from(format_datetime_medium_local(locale, last.due_at)),
                    )],
                )),
        );
    }

    if state.loading {
        return root.child(
            div()
                .p(px(24.0))
                .text_body()
                .text_color(palette.muted)
                .child(I18n::t(locale, "learning", "loading")),
        );
    }

    let Some(card) = state.current_card() else {
        let desc = if state
            .next
            .as_ref()
            .is_some_and(|n| n.counts.new > 0 && n.counts.new_available == 0)
        {
            I18n::t(locale, "learning", "dailyLimitReached")
        } else {
            I18n::t(locale, "learning", "noDue")
        };
        return root.child(
            EmptyState::new(
                "fsrs-no-due-cards",
                I18n::t(locale, "learning", "study"),
                desc,
            )
            .icon(ActionIconName::Cards)
            .palette(palette),
        );
    };

    let revealed = state.effective_revealed();
    let attempt = state.active_attempt();
    let question_heading = match card.kind.as_str() {
        "checker" => I18n::t(locale, "learning", "questionChecker"),
        "cube_offer" => I18n::t(locale, "learning", "questionOffer"),
        "cube_response" => I18n::t(locale, "learning", "questionResponse"),
        _ => card.prompt.as_str(),
    };

    // Left column: BoardView (native cards) OR AnkiCardFaceView (ONLY when kind == "anki")
    let mut left_col = div().v_flex().gap(px(10.0)).flex_1().min_w(px(360.0));
    if card.kind == "anki" {
        left_col = left_col.child(state.anki_face_view.clone());
    } else {
        left_col = left_col.child(div().h(px(420.0)).w_full().child(state.board_view.clone()));
        if revealed.is_some() && card.kind == "checker" {
            let on_target_best = cb.on_arrow_target.clone();
            let on_target_yours = cb.on_arrow_target.clone();
            let on_mode = cb.on_preview_mode.clone();
            left_col = left_col
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            ActionButton::new("fsrs-arrow-best")
                                .label(I18n::t(locale, "learning", "best"))
                                .active(state.arrow_target == ArrowComparisonTarget::BestPlay)
                                .palette(palette)
                                .on_click(move |_, window, cx| {
                                    on_target_best(ArrowComparisonTarget::BestPlay, window, cx)
                                }),
                        )
                        .child(
                            ActionButton::new("fsrs-arrow-yours")
                                .label(I18n::t(locale, "learning", "chosenAnswer"))
                                .active(state.arrow_target == ArrowComparisonTarget::YourMove)
                                .palette(palette)
                                .on_click(move |_, window, cx| {
                                    on_target_yours(ArrowComparisonTarget::YourMove, window, cx)
                                }),
                        ),
                )
                .child(
                    PreviewBoardControls::new("fsrs-preview-controls", state.preview_mode)
                        .palette(palette)
                        .on_mode(move |mode, window, cx| on_mode(mode, window, cx)),
                );
        }
    }

    // Right column: question controls, answer evaluation, explanation, and FSRS rating buttons
    let mut right_col = div()
        .v_flex()
        .gap(px(12.0))
        .w(px(400.0))
        .p(px(14.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .child(
            div()
                .text_question()
                .text_color(palette.text)
                .child(question_heading.to_string()),
        )
        .when(!card.prompt.is_empty() && card.kind != "anki", |this| {
            this.child(
                div()
                    .text_meta()
                    .text_color(palette.muted)
                    .child(card.prompt.clone()),
            )
        });

    if revealed.is_none() {
        // Answer interaction controls (distinct from FSRS rating!)
        match card.kind.as_str() {
            "checker" => {
                let can_undo = state
                    .checker
                    .as_ref()
                    .is_some_and(|c| !c.chosen_steps.is_empty());
                let can_check = state.build_pending_answer().is_some() && !state.attempt_busy;
                let on_undo = cb.on_undo_checker.clone();
                let on_check = cb.on_check_answer.clone();
                right_col = right_col.child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            ActionButton::new("fsrs-check-move")
                                .icon(ActionIconName::Confirm)
                                .label(I18n::t(locale, "learning", "checkMove"))
                                .shortcut("Enter")
                                .primary()
                                .disabled(!can_check)
                                .palette(palette)
                                .on_click(move |_, window, cx| on_check(window, cx)),
                        )
                        .child(
                            ActionButton::new("fsrs-undo-step")
                                .icon(ActionIconName::Undo)
                                .label(I18n::t(locale, "app", "play.undo"))
                                .shortcut("Z")
                                .disabled(!can_undo || state.attempt_busy)
                                .palette(palette)
                                .on_click(move |_, window, cx| on_undo(window, cx)),
                        ),
                );
            }
            "cube_offer" => {
                let on_nd = cb.on_select_cube_offer.clone();
                let on_d = cb.on_select_cube_offer.clone();
                let on_check = cb.on_check_answer.clone();
                let can_check = state.selected_cube_offer.is_some() && !state.attempt_busy;
                right_col = right_col
                    .child(
                        div()
                            .h_flex()
                            .gap(px(8.0))
                            .child(
                                ActionButton::new("fsrs-cube-no-double")
                                    .label(localize_decision(locale, "no double"))
                                    .shortcut("N")
                                    .active(matches!(
                                        state.selected_cube_offer,
                                        Some(CubeOfferAction::NoDouble)
                                    ))
                                    .palette(palette)
                                    .on_click(move |_, window, cx| {
                                        on_nd(CubeOfferAction::NoDouble, window, cx)
                                    }),
                            )
                            .child(
                                ActionButton::new("fsrs-cube-double")
                                    .icon(ActionIconName::Double)
                                    .label(localize_decision(locale, "double"))
                                    .shortcut("D")
                                    .active(matches!(
                                        state.selected_cube_offer,
                                        Some(CubeOfferAction::Double)
                                    ))
                                    .palette(palette)
                                    .on_click(move |_, window, cx| {
                                        on_d(CubeOfferAction::Double, window, cx)
                                    }),
                            ),
                    )
                    .child(
                        ActionButton::new("fsrs-check-cube-offer")
                            .icon(ActionIconName::Confirm)
                            .label(I18n::t(locale, "learning", "checkAnswer"))
                            .shortcut("Enter")
                            .primary()
                            .disabled(!can_check)
                            .palette(palette)
                            .on_click(move |_, window, cx| on_check(window, cx)),
                    );
            }
            "cube_response" => {
                let on_take = cb.on_select_cube_response.clone();
                let on_drop = cb.on_select_cube_response.clone();
                let on_check = cb.on_check_answer.clone();
                let can_check = state.selected_cube_response.is_some() && !state.attempt_busy;
                right_col = right_col
                    .child(
                        div()
                            .h_flex()
                            .gap(px(8.0))
                            .child(
                                ActionButton::new("fsrs-cube-take")
                                    .icon(ActionIconName::Take)
                                    .label(localize_decision(locale, "take"))
                                    .shortcut("T")
                                    .active(matches!(
                                        state.selected_cube_response,
                                        Some(CubeResponseAction::Take)
                                    ))
                                    .palette(palette)
                                    .on_click(move |_, window, cx| {
                                        on_take(CubeResponseAction::Take, window, cx)
                                    }),
                            )
                            .child(
                                ActionButton::new("fsrs-cube-drop")
                                    .icon(ActionIconName::Drop)
                                    .label(localize_decision(locale, "drop"))
                                    .shortcut("P")
                                    .active(matches!(
                                        state.selected_cube_response,
                                        Some(CubeResponseAction::Drop)
                                    ))
                                    .palette(palette)
                                    .on_click(move |_, window, cx| {
                                        on_drop(CubeResponseAction::Drop, window, cx)
                                    }),
                            ),
                    )
                    .child(
                        ActionButton::new("fsrs-check-cube-response")
                            .icon(ActionIconName::Confirm)
                            .label(I18n::t(locale, "learning", "checkAnswer"))
                            .shortcut("Enter")
                            .primary()
                            .disabled(!can_check)
                            .palette(palette)
                            .on_click(move |_, window, cx| on_check(window, cx)),
                    );
            }
            _ => {}
        }

        let on_reveal = cb.on_reveal.clone();
        right_col = right_col.child(
            ActionButton::new("fsrs-reveal-btn")
                .icon(ActionIconName::Solution)
                .label(I18n::t(locale, "learning", "reveal"))
                .shortcut("A")
                .disabled(state.busy || state.attempt_busy)
                .palette(palette)
                .on_click(move |_, window, cx| on_reveal(window, cx)),
        );
    } else if let Some(rev) = revealed {
        // Show PracticeEvaluation if checked
        if let Some(att) = attempt {
            right_col = right_col.child(render_practice_evaluation_card(
                "fsrs-attempt-eval",
                &att.evaluation,
                palette,
                cx,
            ));
        }

        // Show Answer details (best / played / notes / explanation / cube details)
        if let Some(solution) = &rev.answer.solution {
            let best_str = solution
                .decision
                .as_ref()
                .map(|d| d.best.as_str())
                .or_else(|| solution.candidates.first().map(|c| c.notation.as_str()))
                .unwrap_or("");
            if !best_str.is_empty() {
                right_col = right_col.child(
                    div()
                        .v_flex()
                        .gap(px(4.0))
                        .p(px(10.0))
                        .rounded(px(8.0))
                        .bg(palette.surface_alt)
                        .child(
                            div()
                                .text_label()
                                .text_color(palette.muted)
                                .child(I18n::t(locale, "learning", "best")),
                        )
                        .child(
                            div()
                                .text_subject()
                                .text_color(palette.text)
                                .child(localize_decision(locale, best_str)),
                        )
                        .when(!rev.answer.played.is_empty(), |this| {
                            this.child(div().text_meta().text_color(palette.muted).child(format!(
                                "{}: {}",
                                I18n::t(locale, "learning", "played"),
                                localize_decision(locale, &rev.answer.played)
                            )))
                        }),
                );
            }

            // In-process MoveExplanationView for checker plays
            if card.kind == "checker"
                && let Some(pos) = &card.position
                && pos.board.len() == 26
                && pos.dice.len() == 2
            {
                let mut board = [0i32; 26];
                board.copy_from_slice(&pos.board);
                let played_ref = attempt
                    .map(|a| a.evaluation.chosen.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&rev.answer.played);
                if !best_str.is_empty()
                    && !played_ref.is_empty()
                    && best_str != played_ref
                    && let Ok(expl) = explain_checker_plays(
                        board,
                        pos.player,
                        [pos.dice[0], pos.dice[1]],
                        best_str,
                        played_ref,
                    )
                {
                    right_col = right_col.child(
                        MoveExplanationView::from_explanation(
                            "fsrs-move-explanation",
                            best_str,
                            played_ref,
                            &expl,
                            None,
                            None,
                        )
                        .palette(palette),
                    );
                }
            }

            // CubeDecisionDetailsView for cube offers & responses
            if matches!(card.kind.as_str(), "cube_offer" | "cube_response")
                && let Some(pos) = &card.position
            {
                let cube_eval = attempt
                    .and_then(|a| a.evaluation.cube_evaluation.as_ref())
                    .or_else(|| {
                        solution
                            .decision
                            .as_ref()
                            .and_then(|d| d.cube_evaluation.as_ref())
                    })
                    .map(cube_eval_to_view_data);
                right_col = right_col.child(
                    CubeDecisionDetailsView::new(
                        "fsrs-cube-details",
                        usize::try_from(pos.player).unwrap_or(0),
                        card.kind == "cube_response",
                        [
                            SharedString::from(card.players[0].clone()),
                            SharedString::from(card.players[1].clone()),
                        ],
                    )
                    .match_id(pos.gnubg_match_id.clone())
                    .evaluation(cube_eval)
                    .palette(palette),
                );
            }
        }

        if !rev.answer.notes.trim().is_empty() {
            right_col = right_col.child(
                div()
                    .v_flex()
                    .gap(px(4.0))
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(palette.border)
                    .child(
                        div()
                            .text_label()
                            .text_color(palette.muted)
                            .child(I18n::t(locale, "learning", "notes")),
                    )
                    .child(
                        div()
                            .text_body()
                            .text_color(palette.text)
                            .child(rev.answer.notes.clone()),
                    ),
            );
        }

        // FSRS Rating Buttons (`1` Again, `2` Hard, `3` Good, `4` Easy) — strictly separate from Check Answer!
        let intervals = &rev.intervals;
        let ratings = [
            (
                Rating::Again,
                "1",
                intervals.again,
                ActionButtonVariant::Danger,
            ),
            (
                Rating::Hard,
                "2",
                intervals.hard,
                ActionButtonVariant::Default,
            ),
            (
                Rating::Good,
                "3",
                intervals.good,
                ActionButtonVariant::Primary,
            ),
            (
                Rating::Easy,
                "4",
                intervals.easy,
                ActionButtonVariant::Default,
            ),
        ];
        let mut rating_bar = div()
            .id("fsrs-rating-bar")
            .test_support()
            .h_flex()
            .flex_wrap()
            .gap(px(6.0))
            .pt(px(8.0))
            .border_t_1()
            .border_color(palette.border);
        for (rating, shortcut, secs, variant) in ratings {
            let on_rate = cb.on_rate.clone();
            let label = format!(
                "{} · {}",
                rating_label(locale, rating),
                format_interval(locale, secs)
            );
            rating_bar = rating_bar.child(
                ActionButton::new(SharedString::from(format!("fsrs-rate-{shortcut}")))
                    .label(label)
                    .shortcut(shortcut)
                    .variant(variant)
                    .disabled(state.busy)
                    .palette(palette)
                    .on_click(move |_, window, cx| on_rate(rating, window, cx)),
            );
        }
        right_col = right_col.child(rating_bar);
    }

    root.child(
        div()
            .h_flex()
            .items_start()
            .gap(px(16.0))
            .w_full()
            .child(left_col)
            .child(right_col),
    )
}

/// Renders a shared practice answer evaluation card (used across FSRS, Practice Rounds,
/// Recurring Mistake Practice, and Cube Score Variants).
pub fn render_practice_evaluation_card(
    id: impl Into<SharedString>,
    eval: &PracticeEvaluation,
    palette: NeonPalette,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let (fg, bg) = match eval.verdict {
        PracticeVerdict::Best | PracticeVerdict::Forced => {
            (palette.success_text, palette.success_bg)
        }
        PracticeVerdict::Playable => (palette.warning_text, palette.warning_bg),
        PracticeVerdict::Error => (palette.danger_text, palette.danger_bg),
    };

    div()
        .id(id.into())
        .test_support()
        .v_flex()
        .gap(px(6.0))
        .p(px(10.0))
        .rounded(px(8.0))
        .bg(bg)
        .text_color(fg)
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_panel()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(verdict_label(locale, eval.verdict)),
                )
                .child(div().text_meta().tabular_nums().child(format!(
                    "Δ {} EMG",
                    format_decimal(locale, eval.equity_loss, 3, 3)
                ))),
        )
        .child(div().text_body().child(format!(
            "{}: {} → {}: {}",
            I18n::t(locale, "learning", "chosenAnswer"),
            localize_decision(locale, &eval.chosen),
            I18n::t(locale, "learning", "best"),
            localize_decision(locale, &eval.best),
        )))
}

// ── Non-FSRS Card Practice Round & Transfer Practice ─────────────────────────

/// Reflection diagnosis categories for practice rounds (`CardPracticeRound.tsx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundDiagnosis {
    Pattern,
    Principle,
    Calculation,
}

impl RoundDiagnosis {
    pub const ALL: &'static [Self] = &[Self::Pattern, Self::Principle, Self::Calculation];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pattern => "pattern",
            Self::Principle => "principle",
            Self::Calculation => "calculation",
        }
    }
}

/// Interactive state for a non-FSRS Practice Round (10/20 cards) or Transfer Round.
pub struct PracticeRoundState {
    pub round: Option<PracticeRound>,
    pub loading: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub round_size: i64,
    pub confidence: Option<bool>,
    pub diagnosis: Option<RoundDiagnosis>,
    pub lesson_input: Entity<InputState>,
    pub checker: Option<CheckerExerciseController>,
    pub selected_cube_offer: Option<CubeOfferAction>,
    pub selected_cube_response: Option<CubeResponseAction>,
    pub request_id: Uuid,
    pub board_view: Entity<BoardView>,
    pub anki_face_view: Entity<AnkiCardFaceView>,
    // Transfer practice controls & history
    pub transfer_game_plan: GamePlan,
    pub transfer_kind: CardKind,
    pub transfer_follow_up: bool,
    pub transfer_history: Vec<TransferSummary>,
    pub transfer_loading: bool,
}

impl PracticeRoundState {
    pub fn new(board_view: Entity<BoardView>, window: &mut Window, cx: &mut App) -> Self {
        let lesson_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(
                current_locale(cx),
                "card-practice",
                "lesson",
            ))
        });
        let anki_face_view = cx.new(|cx| AnkiCardFaceView::new(None, cx));
        Self {
            round: None,
            loading: false,
            busy: false,
            error: None,
            round_size: 10,
            confidence: None,
            diagnosis: None,
            lesson_input,
            checker: None,
            selected_cube_offer: None,
            selected_cube_response: None,
            request_id: Uuid::new_v4(),
            board_view,
            anki_face_view,
            transfer_game_plan: GamePlan::Opening,
            transfer_kind: CardKind::Checker,
            transfer_follow_up: false,
            transfer_history: Vec::new(),
            transfer_loading: false,
        }
    }

    pub fn apply_round(
        &mut self,
        round: PracticeRound,
        facade: Option<&BackendFacade>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.busy = false;
        self.loading = false;
        self.error = None;
        if round.result.is_none() {
            self.confidence = None;
            self.diagnosis = None;
            self.selected_cube_offer = None;
            self.selected_cube_response = None;
            self.request_id = Uuid::new_v4();
            self.lesson_input
                .update(cx, |s, cx| s.set_value(String::new(), window, cx));
            self.checker = round.card.as_ref().and_then(|c| {
                (c.kind == "checker")
                    .then_some(c.position.as_ref())
                    .flatten()
                    .map(|p| CheckerExerciseController::from_position(p, facade))
            });
        }
        self.round = Some(round);
        self.sync_visuals(cx);
    }

    pub fn build_pending_answer(&self) -> Option<PracticeAnswer> {
        let card = self.round.as_ref()?.card.as_ref()?;
        match card.kind.as_str() {
            "checker" => {
                self.checker
                    .as_ref()
                    .filter(|c| c.is_complete())
                    .map(|c| PracticeAnswer::Checker {
                        steps: c.chosen_steps.clone(),
                    })
            }
            "cube_offer" => self
                .selected_cube_offer
                .map(|action| PracticeAnswer::CubeOffer { action }),
            "cube_response" => self
                .selected_cube_response
                .map(|action| PracticeAnswer::CubeResponse { action }),
            _ => None,
        }
    }

    pub fn sync_visuals(&self, cx: &mut App) {
        let Some(round) = &self.round else {
            self.anki_face_view
                .update(cx, |v, cx| v.set_request(None, cx));
            return;
        };
        let Some(card) = &round.card else {
            self.anki_face_view
                .update(cx, |v, cx| v.set_request(None, cx));
            return;
        };
        if card.kind == "anki" {
            let side = if round.result.is_some() {
                AnkiCardSide::Back
            } else {
                AnkiCardSide::Front
            };
            self.anki_face_view.update(cx, |v, cx| {
                v.set_visible(true, cx);
                v.set_request(Some(AnkiCardFaceRequest::new(card.id, side)), cx);
            });
            return;
        }
        self.anki_face_view
            .update(cx, |v, cx| v.set_request(None, cx));
        if let Some(pos) = &card.position {
            let mut bp = position_to_board_position(pos);
            let interaction = if round.result.is_none() && card.kind == "checker" {
                if let Some(ctrl) = &self.checker {
                    bp.board = ctrl.current_board();
                    Some(ctrl.interaction_state())
                } else {
                    None
                }
            } else {
                None
            };
            let preview = if round.result.is_some() && card.kind == "checker" {
                round
                    .result
                    .as_ref()
                    .and_then(|r| r.evaluation.as_ref())
                    .and_then(|e| preview_for_notation(pos, &e.best))
            } else {
                None
            };
            let mode = if preview.is_some() {
                BoardViewMode::Arrows
            } else {
                BoardViewMode::Before
            };
            self.board_view.update(cx, |bv, cx| {
                bv.set_players(card.players[0].clone(), card.players[1].clone(), cx);
                bv.set_cube_offer_pending(card.kind == "cube_response", cx);
                bv.set_position(bp, cx);
                bv.set_interaction(interaction, cx);
                bv.set_preview(preview, mode, cx);
            });
        }
    }
}

pub struct PracticeRoundCallbacks {
    pub on_close: UiAction,
    pub on_set_confidence: UiCallback<bool>,
    pub on_set_diagnosis: UiCallback<Option<RoundDiagnosis>>,
    pub on_submit: UiCallback<bool>, // true = check answer, false = reveal
    pub on_toggle_mark: UiAction,
    pub on_advance: UiAction,
    pub on_retry_pass2: UiAction,
    pub on_select_cube_offer: UiCallback<CubeOfferAction>,
    pub on_select_cube_response: UiCallback<CubeResponseAction>,
    pub on_undo_checker: UiAction,
}

/// Renders an active non-FSRS `CardPracticeRound` (10/20 cards or Transfer Practice).
pub fn render_practice_round(
    state: &PracticeRoundState,
    palette: NeonPalette,
    cb: PracticeRoundCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let Some(round) = &state.round else {
        return div().id("study-practice-round-empty").test_support();
    };

    let header = div()
        .h_flex()
        .items_center()
        .justify_between()
        .pb(px(10.0))
        .border_b_1()
        .border_color(palette.border)
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(10.0))
                .child({
                    let on_close = cb.on_close.clone();
                    ActionButton::new("round-back-btn")
                        .icon(ActionIconName::Back)
                        .label(I18n::t(locale, "learning", "back"))
                        .palette(palette)
                        .on_click(move |_, window, cx| on_close(window, cx))
                })
                .child(
                    div()
                        .v_flex()
                        .child(div().text_subject().text_color(palette.text).child(
                            if round.transfer {
                                I18n::t(locale, "card-practice", "transfer")
                            } else if round.pass > 1 {
                                I18n::t(locale, "card-practice", "retryTitle")
                            } else {
                                I18n::t(locale, "card-practice", "start")
                            },
                        ))
                        .child(div().text_meta().text_color(palette.muted).child(I18n::t(
                            locale,
                            "card-practice",
                            "mode",
                        ))),
                ),
        )
        .child(
            div()
                .text_meta()
                .tabular_nums()
                .text_color(palette.muted)
                .child(I18n::format(
                    locale,
                    "card-practice",
                    "progress",
                    &[
                        ("index", Param::from((round.index + 1).min(round.total))),
                        ("total", Param::from(round.total)),
                    ],
                )),
        );

    let mut root = div()
        .id("study-practice-round")
        .test_support()
        .key_context(CTX_STUDY_EXERCISE)
        .v_flex()
        .gap(px(14.0))
        .w_full()
        .child(header);

    if let Some(err) = &state.error {
        root = root.child(
            StatusBanner::new("round-err-banner", BannerKind::Error, err.clone()).palette(palette),
        );
    }

    // Completed round summary + Pass-2 retry
    if round.card.is_none() && !round.missing {
        let s = &round.summary;
        let on_retry = cb.on_retry_pass2.clone();
        let on_close = cb.on_close.clone();
        return root.child(
            div()
                .id("round-complete-summary")
                .test_support()
                .v_flex()
                .gap(px(12.0))
                .p(px(20.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(palette.border)
                .bg(palette.surface)
                .child(div().text_dialog().text_color(palette.text).child(I18n::t(
                    locale,
                    "card-practice",
                    "complete",
                )))
                .child(
                    div()
                        .text_body()
                        .tabular_nums()
                        .text_color(palette.muted)
                        .child(I18n::format(
                            locale,
                            "card-practice",
                            "summary",
                            &[
                                ("correct", Param::from(s.correct)),
                                ("errors", Param::from(s.errors)),
                                ("revealed", Param::from(s.revealed)),
                                ("skipped", Param::from(s.skipped)),
                            ],
                        )),
                )
                .child(
                    div()
                        .h_flex()
                        .gap(px(8.0))
                        .when(s.retry_count > 0 && round.pass == 1, |this| {
                            this.child(
                                ActionButton::new("round-retry-pass2")
                                    .icon(ActionIconName::Refresh)
                                    .label(I18n::format(
                                        locale,
                                        "card-practice",
                                        "retry",
                                        &[("count", Param::from(s.retry_count))],
                                    ))
                                    .primary()
                                    .disabled(state.busy)
                                    .palette(palette)
                                    .on_click(move |_, window, cx| on_retry(window, cx)),
                            )
                        })
                        .child(
                            ActionButton::new("round-done-close")
                                .label(I18n::t(locale, "recurring-mistakes", "close"))
                                .palette(palette)
                                .on_click(move |_, window, cx| on_close(window, cx)),
                        ),
                ),
        );
    }

    let Some(card) = &round.card else {
        let on_adv = cb.on_advance.clone();
        return root.child(
            EmptyState::new(
                "round-missing-card",
                I18n::t(locale, "card-practice", "missing"),
                I18n::t(locale, "card-practice", "next"),
            )
            .action(
                ActionButton::new("round-skip-missing")
                    .label(I18n::t(locale, "card-practice", "next"))
                    .primary()
                    .palette(palette)
                    .on_click(move |_, window, cx| on_adv(window, cx)),
            )
            .palette(palette),
        );
    };

    let left_col = div()
        .v_flex()
        .flex_1()
        .min_w(px(360.0))
        .child(if card.kind == "anki" {
            state.anki_face_view.clone().into_any_element()
        } else {
            div()
                .h(px(400.0))
                .w_full()
                .child(state.board_view.clone())
                .into_any_element()
        });

    let mut right_col = div()
        .v_flex()
        .gap(px(12.0))
        .w(px(400.0))
        .p(px(14.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface);

    if let Some(res) = &round.result {
        if let Some(eval) = &res.evaluation {
            right_col = right_col.child(render_practice_evaluation_card(
                "round-item-eval",
                eval,
                palette,
                cx,
            ));
        }

        // Reflection: diagnosis chips + lesson input + mark for review + Next card
        let mut diag_row = div().h_flex().flex_wrap().gap(px(6.0));
        for diag in RoundDiagnosis::ALL {
            let active = state.diagnosis == Some(*diag);
            let on_diag = cb.on_set_diagnosis.clone();
            let d = *diag;
            diag_row = diag_row.child(
                ActionButton::new(SharedString::from(format!("round-diag-{}", d.as_str())))
                    .label(I18n::t(locale, "card-practice", d.as_str()))
                    .active(active)
                    .palette(palette)
                    .on_click(move |_, window, cx| {
                        on_diag(if active { None } else { Some(d) }, window, cx)
                    }),
            );
        }

        let on_mark = cb.on_toggle_mark.clone();
        let on_adv = cb.on_advance.clone();
        right_col = right_col
            .child(
                div()
                    .v_flex()
                    .gap(px(6.0))
                    .child(div().text_label().text_color(palette.muted).child(I18n::t(
                        locale,
                        "card-practice",
                        "diagnosis",
                    )))
                    .child(diag_row),
            )
            .child(
                div()
                    .v_flex()
                    .gap(px(4.0))
                    .child(div().text_label().text_color(palette.muted).child(I18n::t(
                        locale,
                        "card-practice",
                        "lesson",
                    )))
                    .child(Input::new(&state.lesson_input).id("round-lesson-input")),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        ActionButton::new("round-mark-btn")
                            .icon(ActionIconName::Bookmark)
                            .label(I18n::t(locale, "card-practice", "mark"))
                            .active(round.marked)
                            .disabled(state.busy)
                            .palette(palette)
                            .on_click(move |_, window, cx| on_mark(window, cx)),
                    )
                    .child(
                        ActionButton::new("round-next-btn")
                            .icon(ActionIconName::Next)
                            .label(I18n::t(locale, "card-practice", "next"))
                            .shortcut("Enter")
                            .primary()
                            .disabled(state.busy)
                            .palette(palette)
                            .on_click(move |_, window, cx| on_adv(window, cx)),
                    ),
            );
    } else {
        // Confidence rating (Sure / Unsure)
        let on_sure = cb.on_set_confidence.clone();
        let on_unsure = cb.on_set_confidence.clone();
        right_col = right_col.child(
            div()
                .v_flex()
                .gap(px(6.0))
                .child(div().text_label().text_color(palette.muted).child(I18n::t(
                    locale,
                    "card-practice",
                    "confidence",
                )))
                .child(
                    div()
                        .h_flex()
                        .gap(px(6.0))
                        .child(
                            ActionButton::new("round-conf-sure")
                                .label(I18n::t(locale, "card-practice", "sure"))
                                .active(state.confidence == Some(true))
                                .palette(palette)
                                .on_click(move |_, window, cx| on_sure(true, window, cx)),
                        )
                        .child(
                            ActionButton::new("round-conf-unsure")
                                .label(I18n::t(locale, "card-practice", "unsure"))
                                .active(state.confidence == Some(false))
                                .palette(palette)
                                .on_click(move |_, window, cx| on_unsure(false, window, cx)),
                        ),
                ),
        );

        match card.kind.as_str() {
            "cube_offer" => {
                let on_nd = cb.on_select_cube_offer.clone();
                let on_d = cb.on_select_cube_offer.clone();
                right_col = right_col.child(
                    div()
                        .h_flex()
                        .gap(px(8.0))
                        .child(
                            ActionButton::new("round-cube-no-double")
                                .label(localize_decision(locale, "no double"))
                                .shortcut("N")
                                .active(matches!(
                                    state.selected_cube_offer,
                                    Some(CubeOfferAction::NoDouble)
                                ))
                                .palette(palette)
                                .on_click(move |_, window, cx| {
                                    on_nd(CubeOfferAction::NoDouble, window, cx)
                                }),
                        )
                        .child(
                            ActionButton::new("round-cube-double")
                                .label(localize_decision(locale, "double"))
                                .shortcut("D")
                                .active(matches!(
                                    state.selected_cube_offer,
                                    Some(CubeOfferAction::Double)
                                ))
                                .palette(palette)
                                .on_click(move |_, window, cx| {
                                    on_d(CubeOfferAction::Double, window, cx)
                                }),
                        ),
                );
            }
            "cube_response" => {
                let on_t = cb.on_select_cube_response.clone();
                let on_p = cb.on_select_cube_response.clone();
                right_col = right_col.child(
                    div()
                        .h_flex()
                        .gap(px(8.0))
                        .child(
                            ActionButton::new("round-cube-take")
                                .label(localize_decision(locale, "take"))
                                .shortcut("T")
                                .active(matches!(
                                    state.selected_cube_response,
                                    Some(CubeResponseAction::Take)
                                ))
                                .palette(palette)
                                .on_click(move |_, window, cx| {
                                    on_t(CubeResponseAction::Take, window, cx)
                                }),
                        )
                        .child(
                            ActionButton::new("round-cube-drop")
                                .label(localize_decision(locale, "drop"))
                                .shortcut("P")
                                .active(matches!(
                                    state.selected_cube_response,
                                    Some(CubeResponseAction::Drop)
                                ))
                                .palette(palette)
                                .on_click(move |_, window, cx| {
                                    on_p(CubeResponseAction::Drop, window, cx)
                                }),
                        ),
                );
            }
            "checker" => {
                let on_undo = cb.on_undo_checker.clone();
                right_col = right_col.child(
                    ActionButton::new("round-undo-checker")
                        .icon(ActionIconName::Undo)
                        .label(I18n::t(locale, "app", "play.undo"))
                        .shortcut("Z")
                        .disabled(
                            state
                                .checker
                                .as_ref()
                                .is_none_or(|c| c.chosen_steps.is_empty()),
                        )
                        .palette(palette)
                        .on_click(move |_, window, cx| on_undo(window, cx)),
                );
            }
            _ => {}
        }

        let on_check = cb.on_submit.clone();
        let on_reveal = cb.on_submit.clone();
        let can_check = state.build_pending_answer().is_some() && !state.busy;
        right_col = right_col.child(
            div()
                .h_flex()
                .gap(px(8.0))
                .when(card.kind != "anki", |this| {
                    this.child(
                        ActionButton::new("round-check-btn")
                            .icon(ActionIconName::Confirm)
                            .label(I18n::t(locale, "learning", "checkAnswer"))
                            .shortcut("Enter")
                            .primary()
                            .disabled(!can_check)
                            .palette(palette)
                            .on_click(move |_, window, cx| on_check(true, window, cx)),
                    )
                })
                .child(
                    ActionButton::new("round-reveal-btn")
                        .icon(ActionIconName::Solution)
                        .label(I18n::t(locale, "learning", "reveal"))
                        .shortcut("A")
                        .disabled(state.busy)
                        .palette(palette)
                        .on_click(move |_, window, cx| on_reveal(false, window, cx)),
                ),
        );
    }

    root.child(
        div()
            .h_flex()
            .items_start()
            .gap(px(16.0))
            .w_full()
            .child(left_col)
            .child(right_col),
    )
}

/// Callbacks wired from `StudyWorkspace` into `render_transfer_practice_section`.
pub struct TransferPracticeCallbacks {
    pub on_set_plan: UiCallback<GamePlan>,
    pub on_set_kind: UiCallback<CardKind>,
    pub on_toggle_follow_up: UiAction,
    pub on_start: UiAction,
}

/// Renders the Transfer Practice panel (baseline / follow-up selector + past summaries).
pub fn render_transfer_practice_section(
    state: &PracticeRoundState,
    selected_player: &str,
    palette: NeonPalette,
    cb: TransferPracticeCallbacks,
    cx: &App,
) -> impl IntoElement {
    let TransferPracticeCallbacks {
        on_set_plan,
        on_set_kind,
        on_toggle_follow_up,
        on_start,
    } = cb;
    let locale = current_locale(cx);
    let mut plan_row = div().h_flex().flex_wrap().gap(px(4.0));
    for &plan in ALL_GAME_PLANS {
        let on_p = on_set_plan.clone();
        plan_row = plan_row.child(
            ActionButton::new(SharedString::from(format!("transfer-plan-{plan:?}")))
                .label(game_plan_label(locale, plan))
                .active(state.transfer_game_plan == plan)
                .palette(palette)
                .on_click(move |_, window, cx| on_p(plan, window, cx)),
        );
    }

    let kind_choices = [
        (CardKind::Checker, "checker"),
        (CardKind::CubeOffer, "cube_offer"),
        (CardKind::CubeResponse, "cube_response"),
    ];
    let mut kind_row = div().h_flex().flex_wrap().gap(px(4.0));
    for (kind, key) in kind_choices {
        let on_k = on_set_kind.clone();
        let active = matches!(
            (state.transfer_kind, kind),
            (CardKind::Checker, CardKind::Checker)
                | (CardKind::CubeOffer, CardKind::CubeOffer)
                | (CardKind::CubeResponse, CardKind::CubeResponse)
        );
        kind_row = kind_row.child(
            ActionButton::new(SharedString::from(format!("transfer-kind-{key}")))
                .label(I18n::t(locale, "card-search", key))
                .active(active)
                .palette(palette)
                .on_click(move |_, window, cx| on_k(kind, window, cx)),
        );
    }

    let mut panel = div()
        .id("study-transfer-practice")
        .test_support()
        .v_flex()
        .gap(px(10.0))
        .p(px(14.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(div().text_panel().text_color(palette.text).child(I18n::t(
                    locale,
                    "card-practice",
                    "transfer",
                )))
                .child(
                    div()
                        .h_flex()
                        .gap(px(6.0))
                        .child(
                            ActionButton::new("transfer-baseline-toggle")
                                .label(if state.transfer_follow_up {
                                    I18n::t(locale, "card-practice", "followUp")
                                } else {
                                    I18n::t(locale, "card-practice", "baseline")
                                })
                                .active(state.transfer_follow_up)
                                .palette(palette)
                                .on_click(move |_, window, cx| on_toggle_follow_up(window, cx)),
                        )
                        .child(
                            ActionButton::new("transfer-start-btn")
                                .icon(ActionIconName::Play)
                                .label(I18n::t(locale, "card-practice", "transfer"))
                                .primary()
                                .disabled(selected_player.trim().is_empty() || state.loading)
                                .palette(palette)
                                .on_click(move |_, window, cx| on_start(window, cx)),
                        ),
                ),
        )
        .child(plan_row)
        .child(kind_row);

    for (idx, summary) in state.transfer_history.iter().take(5).enumerate() {
        let loss = summary
            .mean_loss
            .map(|v| format_decimal(locale, v, 3, 3))
            .unwrap_or_else(|| "—".into());
        panel = panel.child(
            div()
                .id(SharedString::from(format!("transfer-history-{idx}")))
                .h_flex()
                .items_center()
                .justify_between()
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .bg(palette.surface_alt)
                .child(div().text_label().text_color(palette.text).child(format!(
                    "{} · {} ({})",
                    summary.scope.player,
                    game_plan_label(locale, summary.scope.game_plan),
                    if summary.scope.follow_up {
                        I18n::t(locale, "card-practice", "followUp")
                    } else {
                        I18n::t(locale, "card-practice", "baseline")
                    }
                )))
                .child(
                    div()
                        .text_meta()
                        .tabular_nums()
                        .text_color(palette.muted)
                        .child(I18n::format(
                            locale,
                            "card-practice",
                            "transferSummary",
                            &[
                                ("scored", Param::from(summary.scored)),
                                ("correct", Param::from(summary.correct)),
                                ("revealed", Param::from(summary.revealed)),
                                ("loss", Param::from(loss.as_str())),
                            ],
                        )),
                ),
        );
    }

    panel
}

// ── 3 Special Practice Modals (`CardSpecialPractice`) ────────────────────────

/// State entity for the Pip Count Practice modal.
pub struct PipPracticeModalState {
    pub card: CardQuestion,
    pub white_input: Entity<InputState>,
    pub black_input: Entity<InputState>,
    pub board_view: Entity<BoardView>,
    pub result: Option<PipResult>,
    pub busy: bool,
    pub error: Option<String>,
}

/// Opens Modal 1: Pip Count Practice (`facade.check_card_pips`).
pub fn open_pip_practice_modal(
    card: CardQuestion,
    board_view: Entity<BoardView>,
    on_submit: UiCallback3<Uuid, [i32; 2], Entity<PipPracticeModalState>>,
    window: &mut Window,
    cx: &mut App,
) {
    let locale = current_locale(cx);
    if let Some(pos) = &card.position {
        let bp = position_to_board_position(pos);
        board_view.update(cx, |bv, cx| {
            bv.set_players(card.players[0].clone(), card.players[1].clone(), cx);
            bv.set_show_pip_counts(false, cx);
            bv.set_position(bp, cx);
            bv.set_interaction(None, cx);
            bv.set_preview(None, BoardViewMode::Before, cx);
        });
    }

    let white_input = cx.new(|cx| InputState::new(window, cx).placeholder("167"));
    let black_input = cx.new(|cx| InputState::new(window, cx).placeholder("167"));
    let state = cx.new(|_| PipPracticeModalState {
        card,
        white_input,
        black_input,
        board_view,
        result: None,
        busy: false,
        error: None,
    });

    let state_body = state.clone();
    let state_footer = state;
    let config = ModalConfig::new(I18n::t(locale, "card-practice", "pips"))
        .size(ModalSize::Wide)
        .busy_lock(ModalBusyLock::new(false));

    open_modal_dialog(
        window,
        cx,
        config,
        move |_, cx| {
            let p = NeonTheme::current_palette(cx);
            let locale = current_locale(cx);
            let st = state_body.read(cx);
            let mut body = div()
                .id("special-practice-pips-modal")
                .test_support()
                .v_flex()
                .gap(px(12.0))
                .child(div().h(px(340.0)).w_full().child(st.board_view.clone()))
                .child(
                    div()
                        .h_flex()
                        .gap(px(12.0))
                        .child(
                            div()
                                .v_flex()
                                .gap(px(4.0))
                                .flex_1()
                                .child(div().text_label().text_color(p.text).child(format!(
                                    "{} ({})",
                                    I18n::t(locale, "game-plan", "white"),
                                    st.card.players[0]
                                )))
                                .child(Input::new(&st.white_input).id("pips-white-input")),
                        )
                        .child(
                            div()
                                .v_flex()
                                .gap(px(4.0))
                                .flex_1()
                                .child(div().text_label().text_color(p.text).child(format!(
                                    "{} ({})",
                                    I18n::t(locale, "game-plan", "black"),
                                    st.card.players[1]
                                )))
                                .child(Input::new(&st.black_input).id("pips-black-input")),
                        ),
                );
            if let Some(err) = &st.error {
                body = body.child(
                    StatusBanner::new("pips-modal-err", BannerKind::Error, err.clone()).palette(p),
                );
            }
            if let Some(res) = &st.result {
                body = body.child(
                    div()
                        .h_flex()
                        .gap(px(12.0))
                        .p(px(10.0))
                        .rounded(px(8.0))
                        .bg(p.surface_alt)
                        .child(
                            div()
                                .text_body()
                                .tabular_nums()
                                .text_color(if res.correct[0] {
                                    p.success_text
                                } else {
                                    p.danger_text
                                })
                                .child(format!(
                                    "{}: {} ({})",
                                    I18n::t(locale, "game-plan", "white"),
                                    res.counts[0],
                                    I18n::t(
                                        locale,
                                        "card-practice",
                                        if res.correct[0] {
                                            "correct"
                                        } else {
                                            "incorrect"
                                        }
                                    )
                                )),
                        )
                        .child(
                            div()
                                .text_body()
                                .tabular_nums()
                                .text_color(if res.correct[1] {
                                    p.success_text
                                } else {
                                    p.danger_text
                                })
                                .child(format!(
                                    "{}: {} ({})",
                                    I18n::t(locale, "game-plan", "black"),
                                    res.counts[1],
                                    I18n::t(
                                        locale,
                                        "card-practice",
                                        if res.correct[1] {
                                            "correct"
                                        } else {
                                            "incorrect"
                                        }
                                    )
                                )),
                        ),
                );
            }
            body.into_any_element()
        },
        Some(move |_: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let locale = current_locale(cx);
            let st_ref = state_footer.clone();
            let submit_cb = on_submit.clone();
            let busy = state_footer.read(cx).busy;
            div()
                .h_flex()
                .gap(px(8.0))
                .child(
                    ActionButton::new("pips-close")
                        .label(I18n::t(locale, "recurring-mistakes", "close"))
                        .palette(p)
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .child(
                    ActionButton::new("pips-check-submit")
                        .icon(ActionIconName::Confirm)
                        .label(I18n::t(locale, "card-practice", "checkPips"))
                        .primary()
                        .disabled(busy)
                        .palette(p)
                        .on_click(move |_, window, cx| {
                            let (card_id, w_txt, b_txt) = {
                                let s = st_ref.read(cx);
                                (
                                    s.card.id,
                                    s.white_input.read(cx).value().to_string(),
                                    s.black_input.read(cx).value().to_string(),
                                )
                            };
                            let (Ok(w), Ok(b)) =
                                (w_txt.trim().parse::<i32>(), b_txt.trim().parse::<i32>())
                            else {
                                st_ref.update(cx, |s, cx| {
                                    s.error = Some("Enter integer pip counts (0–375).".into());
                                    cx.notify();
                                });
                                return;
                            };
                            submit_cb(
                                card_id,
                                [w.clamp(0, 375), b.clamp(0, 375)],
                                st_ref.clone(),
                                window,
                                cx,
                            );
                        }),
                )
                .into_any_element()
        }),
        None,
    );
}

/// State entity for the Cube Score Variants modal.
pub struct ScoreVariantsModalState {
    pub card_id: Uuid,
    pub practice: Option<ScorePractice>,
    pub index: usize,
    pub evaluation: Option<PracticeEvaluation>,
    pub loading: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub board_view: Entity<BoardView>,
}

impl ScoreVariantsModalState {
    pub fn sync_board(&self, cx: &mut App) {
        if let Some(q) = self
            .practice
            .as_ref()
            .and_then(|p| p.questions.get(self.index))
            && let Some(pos) = &q.card.position
        {
            let bp = position_to_board_position(pos);
            let white = q.card.players[0].clone();
            let black = q.card.players[1].clone();
            let pending = q.card.kind == "cube_response";
            self.board_view.update(cx, |bv, cx| {
                bv.set_players(white, black, cx);
                bv.set_show_pip_counts(true, cx);
                bv.set_cube_offer_pending(pending, cx);
                bv.set_position(bp, cx);
                bv.set_interaction(None, cx);
                bv.set_preview(None, BoardViewMode::Before, cx);
            });
        }
    }
}

/// Opens Modal 2: Cube Score Variants (`start_score_variants` + `score_practice_variants`).
pub fn open_score_variants_modal(
    state: Entity<ScoreVariantsModalState>,
    on_answer: UiCallback2<Entity<ScoreVariantsModalState>, PracticeAnswer>,
    window: &mut Window,
    cx: &mut App,
) {
    let locale = current_locale(cx);
    let state_body = state.clone();
    let state_footer = state;
    let config = ModalConfig::new(I18n::t(locale, "card-practice", "score")).size(ModalSize::Wide);

    open_modal_dialog(
        window,
        cx,
        config,
        move |_, cx| {
            let p = NeonTheme::current_palette(cx);
            let locale = current_locale(cx);
            let st = state_body.read(cx);

            let mut body = div()
                .id("special-practice-score-variants-modal")
                .test_support()
                .v_flex()
                .gap(px(12.0));

            if st.loading {
                return body
                    .child(
                        div()
                            .text_body()
                            .text_color(p.muted)
                            .child(I18n::t(locale, "learning", "loading")),
                    )
                    .into_any_element();
            }
            if let Some(err) = &st.error {
                body = body.child(
                    StatusBanner::new("score-variants-err", BannerKind::Error, err.clone())
                        .palette(p),
                );
            }

            if let Some(practice) = &st.practice
                && let Some(q) = practice.questions.get(st.index)
            {
                let total = practice.questions.len();
                let is_response = q.card.kind == "cube_response";
                body = body
                    .child(
                        div()
                            .h_flex()
                            .justify_between()
                            .child(
                                div()
                                    .text_label()
                                    .text_color(p.text)
                                    .child(q.card.prompt.clone()),
                            )
                            .child(
                                div()
                                    .text_meta()
                                    .tabular_nums()
                                    .text_color(p.muted)
                                    .child(format!("{} / {total}", st.index + 1)),
                            ),
                    )
                    .child(div().h(px(340.0)).w_full().child(st.board_view.clone()));

                let ans_cb1 = on_answer.clone();
                let ans_cb2 = on_answer.clone();
                let st1 = state_body.clone();
                let st2 = state_body.clone();
                body = body.child(
                    div()
                        .h_flex()
                        .gap(px(8.0))
                        .child(if is_response {
                            ActionButton::new("score-var-take")
                                .label(localize_decision(locale, "take"))
                                .disabled(st.busy)
                                .palette(p)
                                .on_click(move |_, window, cx| {
                                    ans_cb1(
                                        st1.clone(),
                                        PracticeAnswer::CubeResponse {
                                            action: CubeResponseAction::Take,
                                        },
                                        window,
                                        cx,
                                    )
                                })
                        } else {
                            ActionButton::new("score-var-nodouble")
                                .label(localize_decision(locale, "no double"))
                                .disabled(st.busy)
                                .palette(p)
                                .on_click(move |_, window, cx| {
                                    ans_cb1(
                                        st1.clone(),
                                        PracticeAnswer::CubeOffer {
                                            action: CubeOfferAction::NoDouble,
                                        },
                                        window,
                                        cx,
                                    )
                                })
                        })
                        .child(if is_response {
                            ActionButton::new("score-var-drop")
                                .label(localize_decision(locale, "drop"))
                                .disabled(st.busy)
                                .palette(p)
                                .on_click(move |_, window, cx| {
                                    ans_cb2(
                                        st2.clone(),
                                        PracticeAnswer::CubeResponse {
                                            action: CubeResponseAction::Drop,
                                        },
                                        window,
                                        cx,
                                    )
                                })
                        } else {
                            ActionButton::new("score-var-double")
                                .label(localize_decision(locale, "double"))
                                .disabled(st.busy)
                                .palette(p)
                                .on_click(move |_, window, cx| {
                                    ans_cb2(
                                        st2.clone(),
                                        PracticeAnswer::CubeOffer {
                                            action: CubeOfferAction::Double,
                                        },
                                        window,
                                        cx,
                                    )
                                })
                        }),
                );

                if let Some(eval) = &st.evaluation {
                    body = body.child(render_practice_evaluation_card(
                        "score-variant-eval",
                        eval,
                        p,
                        cx,
                    ));
                }
            }

            body.into_any_element()
        },
        Some(move |_: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let locale = current_locale(cx);
            let st_next = state_footer.clone();
            let has_next = state_footer
                .read(cx)
                .practice
                .as_ref()
                .is_some_and(|pr| state_footer.read(cx).index + 1 < pr.questions.len());
            div()
                .h_flex()
                .gap(px(8.0))
                .when(has_next, |this| {
                    this.child(
                        ActionButton::new("score-var-next")
                            .icon(ActionIconName::Next)
                            .label(I18n::t(locale, "recurring-mistakes", "next"))
                            .primary()
                            .palette(p)
                            .on_click(move |_, _window, cx| {
                                st_next.update(cx, |s, cx| {
                                    s.index += 1;
                                    s.evaluation = None;
                                    s.sync_board(cx);
                                    cx.notify();
                                });
                            }),
                    )
                })
                .child(
                    ActionButton::new("score-var-close")
                        .label(I18n::t(locale, "recurring-mistakes", "close"))
                        .palette(p)
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .into_any_element()
        }),
        None,
    );
}

/// State entity for Modal 3: Timed Preview.
pub struct TimedPreviewModalState {
    pub cards: Vec<CardBrowseItem>,
    pub index: usize,
    pub showing_answer: bool,
    pub running: bool,
    pub viewed_count: usize,
    pub question_seconds: u64,
    pub answer_seconds: u64,
    pub current_answer: Option<CardAnswer>,
    pub board_view: Entity<BoardView>,
    pub anki_face_view: Entity<AnkiCardFaceView>,
}

impl TimedPreviewModalState {
    pub fn sync_visuals(&self, cx: &mut App) {
        let Some(item) = self.cards.get(self.index) else {
            return;
        };
        if item.card.kind == "anki" {
            let side = if self.showing_answer {
                AnkiCardSide::Back
            } else {
                AnkiCardSide::Front
            };
            self.anki_face_view.update(cx, |v, cx| {
                v.set_visible(true, cx);
                v.set_request(Some(AnkiCardFaceRequest::new(item.card.id, side)), cx);
            });
            return;
        }
        self.anki_face_view
            .update(cx, |v, cx| v.set_request(None, cx));
        if let Some(pos) = &item.card.position {
            let bp = position_to_board_position(pos);
            let preview = if self.showing_answer && item.card.kind == "checker" {
                self.current_answer
                    .as_ref()
                    .and_then(|a| a.solution.as_ref())
                    .and_then(|s| {
                        s.decision
                            .as_ref()
                            .map(|d| d.best.as_str())
                            .or_else(|| s.candidates.first().map(|c| c.notation.as_str()))
                    })
                    .and_then(|n| preview_for_notation(pos, n))
            } else {
                None
            };
            let mode = if preview.is_some() {
                BoardViewMode::Arrows
            } else {
                BoardViewMode::Before
            };
            self.board_view.update(cx, |bv, cx| {
                bv.set_players(
                    item.card.players[0].clone(),
                    item.card.players[1].clone(),
                    cx,
                );
                bv.set_show_pip_counts(true, cx);
                bv.set_position(bp, cx);
                bv.set_interaction(None, cx);
                bv.set_preview(preview, mode, cx);
            });
        }
    }
}

/// Opens Modal 3: Timed Preview (configurable question/answer seconds, play/pause, card counter).
pub fn open_timed_preview_modal(
    state: Entity<TimedPreviewModalState>,
    on_toggle_run: UiCallback<Entity<TimedPreviewModalState>>,
    on_step_next: UiCallback<Entity<TimedPreviewModalState>>,
    window: &mut Window,
    cx: &mut App,
) {
    let locale = current_locale(cx);
    let state_body = state.clone();
    let state_footer = state.clone();
    let state_close = state;
    let config =
        ModalConfig::new(I18n::t(locale, "card-practice", "preview")).size(ModalSize::Wide);

    open_modal_dialog(
        window,
        cx,
        config,
        move |_, cx| {
            let p = NeonTheme::current_palette(cx);
            let locale = current_locale(cx);
            let st = state_body.read(cx);
            let is_anki = st
                .cards
                .get(st.index)
                .is_some_and(|c| c.card.kind == "anki");
            let best_text = st
                .current_answer
                .as_ref()
                .and_then(|a| a.solution.as_ref())
                .and_then(|s| {
                    s.decision
                        .as_ref()
                        .map(|d| d.best.as_str())
                        .or_else(|| s.candidates.first().map(|c| c.notation.as_str()))
                })
                .map(|b| localize_decision(locale, b));

            div()
                .id("special-practice-timed-preview-modal")
                .test_support()
                .v_flex()
                .gap(px(12.0))
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_meta()
                                .tabular_nums()
                                .text_color(p.muted)
                                .child(format!(
                                    "{}: {}s · {}: {}s",
                                    I18n::t(locale, "card-practice", "questionSeconds"),
                                    st.question_seconds,
                                    I18n::t(locale, "card-practice", "answerSeconds"),
                                    st.answer_seconds
                                )),
                        )
                        .child(div().text_meta().tabular_nums().text_color(p.muted).child(
                            I18n::format(
                                locale,
                                "card-practice",
                                "previewSeen",
                                &[("count", Param::from(st.viewed_count as i64))],
                            ),
                        )),
                )
                .child(if is_anki {
                    st.anki_face_view.clone().into_any_element()
                } else {
                    div()
                        .h(px(340.0))
                        .w_full()
                        .child(st.board_view.clone())
                        .into_any_element()
                })
                .when(st.showing_answer, |this| {
                    this.child(
                        div()
                            .p(px(10.0))
                            .rounded(px(8.0))
                            .bg(p.selected)
                            .text_subject()
                            .text_color(p.text)
                            .child(best_text.unwrap_or_else(|| {
                                I18n::t(locale, "learning", "solution").to_string()
                            })),
                    )
                })
                .into_any_element()
        },
        Some(move |_: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let locale = current_locale(cx);
            let running = state_footer.read(cx).running;
            let st_toggle = state_footer.clone();
            let st_step = state_footer.clone();
            let cb_toggle = on_toggle_run.clone();
            let cb_step = on_step_next.clone();
            div()
                .h_flex()
                .gap(px(8.0))
                .child(
                    ActionButton::new("timed-preview-play-pause")
                        .icon(if running {
                            ActionIconName::Pause
                        } else {
                            ActionIconName::Play
                        })
                        .label(I18n::t(
                            locale,
                            "card-practice",
                            if running { "pause" } else { "play" },
                        ))
                        .primary()
                        .palette(p)
                        .on_click(move |_, window, cx| cb_toggle(st_toggle.clone(), window, cx)),
                )
                .child(
                    ActionButton::new("timed-preview-next")
                        .icon(ActionIconName::Next)
                        .label(I18n::t(locale, "card-practice", "next"))
                        .palette(p)
                        .on_click(move |_, window, cx| cb_step(st_step.clone(), window, cx)),
                )
                .child(
                    ActionButton::new("timed-preview-close")
                        .label(I18n::t(locale, "recurring-mistakes", "close"))
                        .palette(p)
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .into_any_element()
        }),
        Some(Rc::new(move |_, _, cx| {
            state_close.update(cx, |s, cx| {
                s.running = false;
                s.anki_face_view.update(cx, |v, cx| v.set_request(None, cx));
            });
        })),
    );
}
