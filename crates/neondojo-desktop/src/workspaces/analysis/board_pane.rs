//! Center Analysis Board Pane embedding `neondojo_board::BoardView`,
//! `PreviewBoardControls` (`Before` / `Arrows` / `After`), synchronous move
//! projection via `neondojo_board::project_move`, hypothetical Dice Equity
//! preview, `MatchErrorNavigation` (`E`/`Shift+E`), Bookmark toggle,
//! Search-this-position, Copy Position ID, `CubeDecisionComparison` banner, and
//! synchronous `GamePlanPanel` (`facade.classify_game_plans` / `game_plan::classify`).

use super::state::{ActionCb, ActiveBoardTarget, ClickCb};
use crate::i18n::current_locale;
use crate::review::preview_controls::PreviewBoardControls;
use crate::review::severity::SeverityStyle;
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::icons::ActionIconName;
use gpui_kit::{
    App, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt, base::StyledExt, div,
    prelude::FluentBuilder, px,
};
use neondojo::{
    game_plan::{GamePlan, GamePlans, classify as classify_plans_sync},
    model::{MoveSeverity, Position},
    review_decisions::ReviewDecision,
};
use neondojo_board::{BoardPosition, BoardView, BoardViewMode, MovePreview, project_move};
use neondojo_i18n::{I18n, Locale, format_decimal, localize_decision};

/// Hypothetical roll preview triggered by clicking a cell in the 6×6 Dice Equity matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HypotheticalDicePreview {
    pub dice: [u8; 2],
    pub best_notation: String,
}

/// Active preview state rendered on `BoardView`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardPreviewState {
    pub mode: BoardViewMode,
    /// Candidate notation currently previewed (defaults to played notation when `None`).
    pub candidate_notation: Option<String>,
    /// Optional hypothetical roll preview from the 6×6 Dice Equity matrix.
    pub hypothetical_dice: Option<HypotheticalDicePreview>,
}

impl Default for BoardPreviewState {
    fn default() -> Self {
        Self {
            mode: BoardViewMode::Before,
            candidate_notation: None,
            hypothetical_dice: None,
        }
    }
}

impl BoardPreviewState {
    pub fn is_custom_preview(&self) -> bool {
        self.candidate_notation.is_some() || self.hypothetical_dice.is_some()
    }

    pub fn reset(&mut self) {
        self.candidate_notation = None;
        self.hypothetical_dice = None;
        self.mode = BoardViewMode::Before;
    }
}

/// Callbacks wired from `AnalysisWorkspace` into the center board pane.
#[derive(Clone)]
pub struct BoardPaneCallbacks {
    pub on_prev_move: ClickCb,
    pub on_next_move: ClickCb,
    pub on_prev_error: ClickCb,
    pub on_next_error: ClickCb,
    pub on_view_mode: ActionCb<BoardViewMode>,
    pub on_reset_preview: ClickCb,
    pub on_toggle_bookmark: ClickCb,
    pub on_search_position: ActionCb<String>,
    pub on_copy_position_id: ActionCb<String>,
    pub on_return_to_checker: ClickCb,
    pub on_open_move_notes: ClickCb,
}

/// Synchronizes `BoardView` entity state from the active position, optional cube
/// decision, and `BoardPreviewState` using synchronous `neondojo_board::project_move`.
pub fn sync_board_view(
    board_view: &Entity<BoardView>,
    position: Option<&Position>,
    cube_decision: Option<&ReviewDecision>,
    preview_state: &BoardPreviewState,
    players: [&str; 2],
    cx: &mut App,
) {
    let effective_pos = cube_decision
        .and_then(|c| c.review_position.as_ref())
        .or(position);

    let Some(pos) = effective_pos else {
        board_view.update(cx, |view, cx| {
            view.set_position(BoardPosition::default(), cx);
            view.set_preview(None, BoardViewMode::Before, cx);
            view.set_cube_offer_pending(false, cx);
            view.set_players(players[0], players[1], cx);
        });
        return;
    };

    let mut board_arr = [0i32; 26];
    for (dst, &src) in board_arr.iter_mut().zip(pos.board.iter()) {
        *dst = src;
    }
    let player_u8 = u8::try_from(pos.player).unwrap_or(0).min(1);
    let is_cube_view = cube_decision.is_some();

    let (dice, notation, move_preview, view_mode) = if is_cube_view {
        (Vec::new(), String::new(), None, BoardViewMode::Before)
    } else if let Some(hyp) = &preview_state.hypothetical_dice {
        let d = vec![hyp.dice[0], hyp.dice[1]];
        let projected = project_move(&board_arr, player_u8, &d, &hyp.best_notation);
        let mode = if preview_state.mode == BoardViewMode::Before {
            BoardViewMode::Arrows
        } else {
            preview_state.mode
        };
        (d, hyp.best_notation.clone(), Some(projected), mode)
    } else {
        let d: Vec<u8> = pos
            .dice
            .iter()
            .filter_map(|&v| u8::try_from(v).ok())
            .collect();
        let active_notation = preview_state
            .candidate_notation
            .as_deref()
            .unwrap_or(pos.notation.as_str());
        let projected = if !active_notation.trim().is_empty() && d.len() == 2 {
            Some(project_move(&board_arr, player_u8, &d, active_notation))
        } else {
            None
        };
        (
            d,
            active_notation.to_string(),
            projected,
            preview_state.mode,
        )
    };

    let cube_offer_pending = cube_decision
        .is_some_and(|c| matches!(c.decision.played.as_str(), "take" | "drop" | "double"));

    let board_pos = BoardPosition {
        board: board_arr,
        player: player_u8,
        dice,
        gnubg_match_id: pos.gnubg_match_id.clone(),
        notation,
    };

    board_view.update(cx, |view, cx| {
        view.set_players(players[0], players[1], cx);
        view.set_position(board_pos, cx);
        view.set_cube_offer_pending(cube_offer_pending, cx);
        view.set_preview(move_preview, view_mode, cx);
    });
}

/// Parameters for rendering the center board pane.
pub struct BoardPaneProps<'a> {
    pub width_px: f32,
    pub board_view: Entity<BoardView>,
    pub active_target: Option<&'a ActiveBoardTarget>,
    pub position: Option<&'a Position>,
    pub cube_decision: Option<&'a ReviewDecision>,
    pub preview: &'a BoardPreviewState,
    pub players: [SharedString; 2],
    pub bookmarked: bool,
    pub bookmark_busy: bool,
    pub can_prev_move: bool,
    pub can_next_move: bool,
    pub error_index: Option<usize>,
    pub error_total: usize,
    pub copied_feedback: bool,
    pub game_plans: Option<GamePlans>,
    pub callbacks: BoardPaneCallbacks,
    pub palette: NeonPalette,
}

/// Classifies game plans synchronously for a board slice (using `BackendFacade`
/// when connected or direct `neondojo::game_plan::classify` when offline/headless).
pub fn compute_game_plans(
    board: &[i32],
    facade: Option<&neondojo::facade::BackendFacade>,
) -> Option<GamePlans> {
    let arr: &[i32; 26] = board.try_into().ok()?;
    if let Some(f) = facade {
        f.classify_game_plans(arr).ok()
    } else {
        classify_plans_sync(arr).ok()
    }
}

/// Renders the center Analysis board column.
pub fn render_board_pane(props: BoardPaneProps<'_>, cx: &App) -> impl IntoElement {
    let locale = current_locale(cx);
    let p = props.palette;
    let effective_pos = props
        .cube_decision
        .and_then(|c| c.review_position.as_ref())
        .or(props.position);

    let pos_id_text = effective_pos.map(|pos| pos.position_id.clone());
    let player_idx = effective_pos
        .map(|pos| usize::try_from(pos.player).unwrap_or(0).min(1))
        .unwrap_or(0);
    let preview_move_label = if let Some(hyp) = &props.preview.hypothetical_dice {
        Some(format!(
            "{}–{}: {}",
            hyp.dice[0], hyp.dice[1], hyp.best_notation
        ))
    } else if let Some(cand) = &props.preview.candidate_notation {
        Some(cand.clone())
    } else {
        effective_pos
            .filter(|_| props.cube_decision.is_none())
            .map(|pos| pos.notation.clone())
    };

    let board_height = (props.width_px * 0.695).clamp(240.0, 740.0);

    let mut root = div()
        .id("analysis-board-pane")
        .v_flex()
        .w(px(props.width_px))
        .h_full()
        .flex_shrink_0()
        .gap(px(8.0))
        .p(px(10.0))
        .bg(p.bg)
        .overflow_y_scroll()
        .test_support();

    // Cube decision comparison banner when reviewing a cube decision.
    if let Some(cube) = props.cube_decision {
        let on_back = props.callbacks.on_return_to_checker.clone();
        let on_notes = props.callbacks.on_open_move_notes.clone();
        let played_lbl = localize_decision(locale, &cube.decision.played);
        let best_lbl = localize_decision(locale, &cube.decision.best);
        let sev =
            super::state::parse_severity(&cube.decision.severity).unwrap_or(MoveSeverity::None);
        let st = SeverityStyle::resolve(sev, &p);

        root =
            root.child(
                div()
                    .id("analysis-cube-comparison-banner")
                    .test_support()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .py(px(7.0))
                    .rounded(px(6.0))
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.accent)
                    .child(
                        div()
                            .v_flex()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text)
                                    .child(format!(
                                        "Cube decision #{} · Played: {} · Best: {}",
                                        cube.decision.decision_no, played_lbl, best_lbl
                                    )),
                            )
                            .child(div().text_micro().tabular_nums().text_color(st.fg).child(
                                format!(
                                    "Δ -{}",
                                    format_decimal(locale, cube.decision.equity_loss, 3, 3)
                                ),
                            )),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap(px(6.0))
                            .child(
                                ActionButton::new("cube-banner-notes")
                                    .label("Move notes")
                                    .icon(ActionIconName::Notes)
                                    .palette(p)
                                    .on_click(move |ev, w, cx| on_notes(ev, w, cx)),
                            )
                            .child(
                                ActionButton::new("cube-banner-back")
                                    .label(I18n::t(locale, "board-review", "returnToMove"))
                                    .icon(ActionIconName::Back)
                                    .palette(p)
                                    .on_click(move |ev, w, cx| on_back(ev, w, cx)),
                            ),
                    ),
            );
    }

    // Top controls row: Move steppers + Error navigation (E / Shift+E) + utility actions.
    let on_prev = props.callbacks.on_prev_move.clone();
    let on_next = props.callbacks.on_next_move.clone();
    let on_prev_err = props.callbacks.on_prev_error.clone();
    let on_next_err = props.callbacks.on_next_error.clone();
    let on_bookmark = props.callbacks.on_toggle_bookmark.clone();
    let on_search = props.callbacks.on_search_position.clone();
    let on_copy = props.callbacks.on_copy_position_id.clone();

    let error_caption = if props.error_total > 0 {
        match props.error_index {
            Some(idx) => format!("Error {}/{}", idx + 1, props.error_total),
            None => format!("{} errors", props.error_total),
        }
    } else {
        "0 errors".to_string()
    };

    let nav_row = div()
        .id("analysis-board-nav-bar")
        .test_support()
        .h_flex()
        .items_center()
        .justify_between()
        .flex_wrap()
        .gap(px(6.0))
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(4.0))
                .child(
                    ActionButton::new("board-prev-move")
                        .icon(ActionIconName::Back)
                        .dense_nav()
                        .tooltip(I18n::t(locale, "board-review", "previousDecision"))
                        .shortcut("Left")
                        .disabled(!props.can_prev_move)
                        .palette(p)
                        .on_click(move |ev, w, cx| on_prev(ev, w, cx)),
                )
                .child(
                    ActionButton::new("board-next-move")
                        .icon(ActionIconName::Next)
                        .dense_nav()
                        .tooltip(I18n::t(locale, "board-review", "nextDecision"))
                        .shortcut("Right")
                        .disabled(!props.can_next_move)
                        .palette(p)
                        .on_click(move |ev, w, cx| on_next(ev, w, cx)),
                )
                .child(div().w(px(6.0)))
                .child(
                    ActionButton::new("board-prev-error")
                        .icon(ActionIconName::First)
                        .dense_nav()
                        .tooltip(I18n::t(locale, "board-review", "previousError"))
                        .shortcut("Shift+E")
                        .disabled(props.error_total == 0)
                        .palette(p)
                        .on_click(move |ev, w, cx| on_prev_err(ev, w, cx)),
                )
                .child(
                    div()
                        .id("analysis-error-nav-counter")
                        .test_support()
                        .text_micro()
                        .tabular_nums()
                        .text_color(p.muted)
                        .px(px(4.0))
                        .child(error_caption),
                )
                .child(
                    ActionButton::new("board-next-error")
                        .icon(ActionIconName::Last)
                        .dense_nav()
                        .tooltip(I18n::t(locale, "workspace", "nextError"))
                        .shortcut("E")
                        .disabled(props.error_total == 0)
                        .palette(p)
                        .on_click(move |ev, w, cx| on_next_err(ev, w, cx)),
                ),
        )
        .child(div().h_flex().items_center().gap(px(4.0)).when_some(
            pos_id_text.clone(),
            |el, pid_str| {
                let search_id = pid_str.clone();
                let copy_id = pid_str.clone();
                let s_cb = on_search.clone();
                let c_cb = on_copy.clone();
                let b_cb = on_bookmark.clone();
                el.child(
                    ActionButton::new("board-bookmark-btn")
                        .icon(if props.bookmarked {
                            ActionIconName::Unbookmark
                        } else {
                            ActionIconName::Bookmark
                        })
                        .dense_nav()
                        .active(props.bookmarked)
                        .disabled(props.bookmark_busy || props.position.is_none())
                        .tooltip(if props.bookmarked {
                            I18n::t(locale, "position-bookmark", "remove")
                        } else {
                            I18n::t(locale, "position-bookmark", "save")
                        })
                        .palette(p)
                        .on_click(move |ev, w, cx| b_cb(ev, w, cx)),
                )
                .child(
                    ActionButton::new("board-search-pos-btn")
                        .icon(ActionIconName::Search)
                        .dense_nav()
                        .tooltip(I18n::t(locale, "workspace", "findPosition"))
                        .palette(p)
                        .on_click(move |_, w, cx| s_cb(search_id.clone(), w, cx)),
                )
                .child(
                    ActionButton::new("board-copy-pos-id-btn")
                        .icon(ActionIconName::Copy)
                        .dense_nav()
                        .tooltip(if props.copied_feedback {
                            "Copied Position ID"
                        } else {
                            "Copy Position ID"
                        })
                        .shortcut("Cmd+Shift+C")
                        .variant(if props.copied_feedback {
                            ActionButtonVariant::Primary
                        } else {
                            ActionButtonVariant::Default
                        })
                        .palette(p)
                        .on_click(move |_, w, cx| c_cb(copy_id.clone(), w, cx)),
                )
            },
        ));

    root = root.child(nav_row);

    // Embedded GPU BoardView canvas
    root = root.child(
        div()
            .id("analysis-board-canvas-frame")
            .test_support()
            .w_full()
            .h(px(board_height))
            .rounded(px(8.0))
            .overflow_hidden()
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(props.board_view),
    );

    // Preview controls (Initial / Arrows / Final + Reset preview) when inspecting checker move.
    if props.cube_decision.is_none() && effective_pos.is_some() {
        let on_mode = props.callbacks.on_view_mode.clone();
        let on_reset = props.callbacks.on_reset_preview.clone();
        let mut controls =
            PreviewBoardControls::new("analysis-preview-controls", props.preview.mode)
                .can_reset(props.preview.is_custom_preview())
                .palette(p)
                .on_mode(move |mode, w, cx| on_mode(mode, w, cx))
                .on_reset(move |ev, w, cx| on_reset(ev, w, cx));
        if let Some(mv_label) = preview_move_label {
            controls = controls.preview_summary(props.players[player_idx].clone(), mv_label);
        }
        root = root.child(controls);
    }

    // Position ID & Match ID metadata line
    if let Some(pos) = effective_pos {
        root = root.child(
            div()
                .id("analysis-board-ids")
                .test_support()
                .h_flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(5.0))
                .bg(p.surface)
                .border_1()
                .border_color(p.border)
                .child(
                    div()
                        .mono_font()
                        .text_micro()
                        .text_color(p.muted)
                        .child(format!("PID: {}", pos.position_id)),
                )
                .child(
                    div()
                        .mono_font()
                        .text_micro()
                        .text_color(p.muted)
                        .child(format!("MID: {}", pos.gnubg_match_id)),
                ),
        );
    }

    // Synchronous GamePlanPanel
    if let Some(plans) = props.game_plans {
        root = root.child(render_game_plan_panel(&plans, &props.players, p, locale));
    }

    // Validate projection errors subtly if any
    if let Some(pos) = effective_pos
        && props.cube_decision.is_none()
        && let Some(cand) = props.preview.candidate_notation.as_deref()
        && let Ok(board_arr) = <&[i32; 26]>::try_from(pos.board.as_slice())
    {
        let d: Vec<u8> = pos
            .dice
            .iter()
            .filter_map(|&v| u8::try_from(v).ok())
            .collect();
        let player_u8 = u8::try_from(pos.player).unwrap_or(0).min(1);
        if d.len() == 2
            && let MovePreview::Unavailable(reason) = project_move(board_arr, player_u8, &d, cand)
        {
            root = root.child(
                div()
                    .text_micro()
                    .text_color(p.warning_text)
                    .child(format!("Preview unavailable: {reason}")),
            );
        }
    }

    root
}

fn render_game_plan_panel(
    plans: &GamePlans,
    players: &[SharedString; 2],
    p: NeonPalette,
    locale: Locale,
) -> impl IntoElement {
    let white_label = game_plan_label(locale, plans.white);
    let black_label = game_plan_label(locale, plans.black);
    div()
        .id("analysis-game-plan-panel")
        .test_support()
        .h_flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .px(px(10.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .text_meta()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.muted)
                .child(I18n::t(locale, "game-plan", "title")),
        )
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_micro()
                                .text_color(p.muted)
                                .child(format!("{}:", players[0])),
                        )
                        .child(
                            div()
                                .px(px(6.0))
                                .py(px(1.0))
                                .rounded(px(4.0))
                                .bg(p.surface_alt)
                                .text_micro()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(p.text)
                                .child(white_label),
                        ),
                )
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_micro()
                                .text_color(p.muted)
                                .child(format!("{}:", players[1])),
                        )
                        .child(
                            div()
                                .px(px(6.0))
                                .py(px(1.0))
                                .rounded(px(4.0))
                                .bg(p.surface_alt)
                                .text_micro()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(p.text)
                                .child(black_label),
                        ),
                ),
        )
}

fn game_plan_label(locale: Locale, plan: GamePlan) -> &'static str {
    let key = match plan {
        GamePlan::Opening => "opening",
        GamePlan::PureRace => "pure_race",
        GamePlan::Racing => "racing",
        GamePlan::Attacking => "attacking",
        GamePlan::Priming => "priming",
        GamePlan::Anchoring => "anchoring",
    };
    I18n::t(locale, "game-plan", key)
}
