//! Live Play dialogs and inline Coach feedback bar.
//!
//! Mirrors `GameResultSummary.tsx`, `PlayCubeDecision.tsx`, `PlayResignDialog.tsx`,
//! `PlayDialogs.tsx`, `PlayFeedback.tsx`, `PlayHintView.tsx`, `PlayCoachFeedback.tsx`,
//! and `LearningReviewDialog.tsx`.
//!
//! Reuses `crate::review::*` (`CubeDecisionDetailsView`, `MoveExplanationView`,
//! `PreviewBoardControls`, `SeverityStyle`) plus a secondary preview
//! `neondojo_board::BoardView` for candidate move inspection.

use crate::i18n::current_locale;
use crate::review::cube_details::{
    CubeDecisionDetailsView, CubeEquityUnit, CubeEvaluationData, format_cube_context_line,
};
use crate::review::move_explanation::{MoveExplanationView, explain_checker_plays};
use crate::review::preview_controls::PreviewBoardControls;
use crate::review::severity::SeverityStyle;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::icons::ActionIconName;
use crate::workspaces::play::interruption::{
    PlayInterruption, board_array, is_money_game_finished, parse_play_severity,
};
use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled, TestSupportExt,
    Window, base::StyledExt, div, prelude::FluentBuilder, px,
};
use neondojo::model::{GameResult, GameResultReason};
use neondojo::play::{PlayDecision, PlayMode, PlayReview, PlaySession};
use neondojo_board::{BoardView, BoardViewMode, decode_match_context};
use neondojo_i18n::{
    I18n, Locale, Param, format_decimal, format_equity, format_int, format_percent,
    localize_decision,
};
use std::rc::Rc;
use uuid::Uuid;

/// Resignation level (`1` Single, `2` Gammon, `3` Backgammon).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResignLevel {
    Single = 1,
    Gammon = 2,
    Backgammon = 3,
}

impl ResignLevel {
    pub const ALL: &'static [Self] = &[Self::Single, Self::Gammon, Self::Backgammon];

    pub const fn as_i32(self) -> i32 {
        self as i32
    }

    pub const fn reason_key(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Gammon => "gammon",
            Self::Backgammon => "backgammon",
        }
    }
}

/// Projected score summary for [`render_resign_dialog`] (`PlayResignDialog.tsx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResignProjection {
    pub points: i32,
    pub projected_scores: [i32; 2],
    pub ends_match: bool,
}

/// Pure calculation of `level × cube` award, projected scores, and match-ending status.
pub fn project_resignation(
    level: ResignLevel,
    cube: i32,
    scores: [i32; 2],
    human_player: i32,
    match_length: i32,
) -> ResignProjection {
    let points = level.as_i32() * cube.max(1);
    let mut projected = scores;
    let opponent_idx = if human_player == 0 { 1 } else { 0 };
    projected[opponent_idx] += points;
    let ends_match = match_length > 0 && projected[opponent_idx] >= match_length;
    ResignProjection {
        points,
        projected_scores: projected,
        ends_match,
    }
}

/// Reusable game-result summary card (`GameResultSummary.tsx`).
#[derive(IntoElement)]
pub struct GameResultSummaryView {
    id: ElementId,
    result: GameResult,
    white: SharedString,
    black: SharedString,
    palette: Option<NeonPalette>,
}

impl GameResultSummaryView {
    pub fn new(
        id: impl Into<ElementId>,
        result: GameResult,
        white: impl Into<SharedString>,
        black: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            result,
            white: white.into(),
            black: black.into(),
            palette: None,
        }
    }

    pub fn palette(mut self, palette: NeonPalette) -> Self {
        self.palette = Some(palette);
        self
    }
}

impl RenderOnce for GameResultSummaryView {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = self
            .palette
            .unwrap_or_else(|| NeonTheme::current_palette(cx));
        let winner_name = if self.result.winner == 0 {
            self.white.as_ref()
        } else {
            self.black.as_ref()
        };
        let winner_line = if self.result.points == 1 {
            I18n::format(
                locale,
                "game-result",
                "winnerOne",
                &[("player", Param::from(winner_name))],
            )
        } else {
            I18n::format(
                locale,
                "game-result",
                "winner",
                &[
                    ("player", Param::from(winner_name)),
                    ("points", Param::from(self.result.points)),
                ],
            )
        };
        let reason_key = match self.result.reason {
            GameResultReason::Single => "single",
            GameResultReason::Gammon => "gammon",
            GameResultReason::Backgammon => "backgammon",
            GameResultReason::Drop => "drop",
            GameResultReason::Resignation => "resignation",
            GameResultReason::Recorded => "recorded",
        };
        let reason = I18n::t(locale, "game-result", reason_key);
        let points_label = format!(
            "{} {}",
            format_int(locale, i64::from(self.result.points)),
            I18n::t(
                locale,
                "game-result",
                if self.result.points == 1 {
                    "point"
                } else {
                    "points"
                }
            )
        );
        let cube_label = format!(
            "{} {}",
            I18n::t(locale, "game-result", "cube"),
            format_int(locale, i64::from(self.result.cube))
        );
        let winner_idx = usize::try_from(self.result.winner).unwrap_or(0).min(1);
        let winning_score = self.result.scores_after[winner_idx];
        let calculation = if self.result.reason == GameResultReason::Drop {
            format!("{reason} · {cube_label} = {points_label}")
        } else if let Some(mult) = self.result.multiplier {
            format!(
                "{reason} ({}) × {cube_label} = {points_label}",
                format_int(locale, i64::from(mult))
            )
        } else {
            format!(
                "{reason} · {} − {} = {points_label}",
                format_int(locale, i64::from(winning_score)),
                format_int(locale, i64::from(winning_score - self.result.points))
            )
        };
        let score_line = format!(
            "{}: {} {} : {} {}",
            I18n::t(locale, "game-result", "score"),
            self.white,
            format_int(locale, i64::from(self.result.scores_after[0])),
            format_int(locale, i64::from(self.result.scores_after[1])),
            self.black
        );

        div()
            .id(self.id)
            .test_support()
            .v_flex()
            .gap(px(3.0))
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface_alt)
            .child(
                div()
                    .text_label()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text)
                    .child(winner_line),
            )
            .child(
                div()
                    .text_meta()
                    .tabular_nums()
                    .text_color(p.muted)
                    .child(calculation),
            )
            .child(
                div()
                    .text_meta()
                    .tabular_nums()
                    .text_color(p.text)
                    .child(score_line),
            )
    }
}

/// Formats the player's staged or original decision label (`PlayFeedback.tsx::useDecisionChoice`).
pub fn format_decision_choice(locale: Locale, session: &PlaySession) -> String {
    let Some(decision) = session.pending_decision.as_ref() else {
        return String::new();
    };
    let action = decision
        .action
        .as_deref()
        .or(decision.original_action.as_deref());
    let index = if decision.action.is_some() {
        decision.move_index
    } else {
        decision.original_move_index
    };
    let notation = if decision.action.is_some() {
        decision.notation.as_deref()
    } else {
        decision.original_notation.as_deref()
    };
    match action {
        Some("roll") => I18n::t(locale, "play-mode", "notDoubled").to_string(),
        Some("double") => I18n::t(locale, "play-mode", "doubled").to_string(),
        Some(other) => {
            let text = notation
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    index.and_then(|i| session.legal_moves.get(i).map(|m| m.notation.as_str()))
                })
                .unwrap_or(other);
            localize_decision(locale, text)
        }
        None => String::new(),
    }
}

/// True when `session.pending_decision` has revealed checker alternatives suitable
/// for secondary-board candidate preview.
pub fn has_play_move_preview(session: &PlaySession) -> bool {
    session.pending_decision.as_ref().is_some_and(|d| {
        d.revealed
            && d.alternatives
                .iter()
                .any(|alt| alt.action == "move" && !alt.notation.is_empty())
    })
}

/// Builds optional [`CubeEvaluationData`] from a cube decision's alternatives so
/// `CubeDecisionDetailsView` renders full cube outcomes & take-point math.
fn extract_cube_evaluation(
    decision: &PlayDecision,
    is_response: bool,
) -> Option<CubeEvaluationData> {
    let find_eq = |actions: &[&str]| -> Option<f64> {
        decision
            .alternatives
            .iter()
            .find(|a| actions.contains(&a.action.as_str()))
            .and_then(|a| a.equity)
    };
    let probs = decision
        .alternatives
        .iter()
        .find_map(|a| a.probabilities.as_ref())
        .and_then(|v| {
            if v.len() >= 6 {
                Some([v[0], v[1], v[2], v[3], v[4], v[5]])
            } else {
                None
            }
        });
    if is_response {
        let take = find_eq(&["take"])?;
        let pass = find_eq(&["drop", "pass"])?;
        Some(CubeEvaluationData {
            no_double: 0.0,
            double_take: take,
            double_pass: pass,
            unit: CubeEquityUnit::NormalizedCubeful,
            probabilities: probs,
        })
    } else {
        let no_double = find_eq(&["roll", "no_double"])?;
        let double_eq = find_eq(&["double"])?;
        Some(CubeEvaluationData {
            no_double,
            double_take: double_eq,
            double_pass: 1.0,
            unit: CubeEquityUnit::NormalizedCubeful,
            probabilities: probs,
        })
    }
}

pub type PlayVoidCallback = Rc<dyn Fn(&mut Window, &mut App) + 'static>;
pub type PlayArgCallback<T> = Rc<dyn Fn(T, &mut Window, &mut App) + 'static>;

/// Callbacks for [`render_feedback_body`].
#[derive(Clone)]
pub struct PlayFeedbackCallbacks {
    pub on_retry: PlayVoidCallback,
    pub on_reveal: PlayVoidCallback,
    pub on_continue: PlayVoidCallback,
    pub on_select_candidate: PlayArgCallback<usize>,
    pub on_preview_mode: PlayArgCallback<BoardViewMode>,
    pub on_reset_preview: PlayVoidCallback,
}

/// Renders the shared decision details (evaluation line, cube details, alternatives table,
/// move explanation, and optional secondary preview `BoardView` + `PreviewBoardControls`).
#[allow(clippy::too_many_arguments)]
pub fn render_decision_details(
    session: &PlaySession,
    decision: &PlayDecision,
    white_name: &str,
    black_name: &str,
    selected_candidate: Option<usize>,
    preview_mode: BoardViewMode,
    preview_board: Option<&Entity<BoardView>>,
    palette: NeonPalette,
    cb: &PlayFeedbackCallbacks,
    cx: &App,
) -> AnyElement {
    let locale = current_locale(cx);
    let p = palette;
    let full = session.play_mode != PlayMode::Coach;
    let has_checker_alts = decision.alternatives.iter().any(|a| a.action == "move");
    let is_cube_decision = !decision.alternatives.is_empty() && !has_checker_alts;
    let is_cube_response = decision
        .alternatives
        .iter()
        .any(|a| a.action == "take" || a.action == "drop");

    let mut details = div().v_flex().gap(px(10.0)).w_full();

    // Severity + equity loss pill
    if let Some(loss) = decision.loss {
        let sev = parse_play_severity(decision.severity.as_deref());
        let style = SeverityStyle::resolve(sev, &p);
        details = details.child(
            div()
                .id("play-feedback-evaluation")
                .test_support()
                .h_flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .bg(style.bg)
                .text_color(style.fg)
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(style.label(locale)),
                )
                .child("·")
                .child(div().text_table().tabular_nums().child(format!(
                    "{}: {}",
                    I18n::t(locale, "play-mode", "loss"),
                    format_decimal(locale, loss, 3, 3)
                ))),
        );
    }

    // Cube context / details reusing `crate::review::cube_details`
    if is_cube_decision {
        if full && let Some(cube_eval) = extract_cube_evaluation(decision, is_cube_response) {
            let decider = usize::try_from(session.human_player).unwrap_or(0).min(1);
            details = details.child(
                CubeDecisionDetailsView::new(
                    "play-feedback-cube-details",
                    decider,
                    is_cube_response,
                    [
                        SharedString::from(white_name.to_string()),
                        SharedString::from(black_name.to_string()),
                    ],
                )
                .match_id(session.position.gnubg_match_id.clone())
                .evaluation(Some(cube_eval))
                .palette(p),
            );
        } else {
            let line = format_cube_context_line(
                locale,
                &session.position.gnubg_match_id,
                [white_name, black_name],
                is_cube_response.then_some(if session.human_player == 0 { 1 } else { 0 }),
                true,
            );
            details = details.child(div().text_meta().text_color(p.muted).child(line));
        }
    }

    // First-attempt line when retrying
    if let Some(orig_action) = decision.original_action.as_deref() {
        let orig_label = decision
            .original_notation
            .as_deref()
            .map(|n| localize_decision(locale, n))
            .unwrap_or_else(|| localize_decision(locale, orig_action));
        let loss_str = decision
            .first_loss
            .map(|l| format!(" · Δ {}", format_decimal(locale, l, 3, 3)))
            .unwrap_or_default();
        let assisted = decision
            .attempts
            .first()
            .map_or(decision.assisted, |a| a.assisted);
        let assist_label = I18n::t(
            locale,
            "play-mode",
            if assisted { "assisted" } else { "independent" },
        );
        details = details.child(div().text_meta().tabular_nums().text_color(p.muted).child(
            format!(
                "{}: {orig_label}{loss_str} · {assist_label}",
                I18n::t(locale, "play-mode", "first")
            ),
        ));
    }

    // Secondary candidate preview board + PreviewBoardControls
    if has_checker_alts && let Some(board_entity) = preview_board {
        let active_alt = selected_candidate
            .and_then(|idx| decision.alternatives.get(idx))
            .or_else(|| decision.alternatives.iter().find(|a| a.action == "move"));
        let move_label = active_alt
            .map(|a| localize_decision(locale, &a.notation))
            .unwrap_or_default();
        let player_label = if session.position.player == 0 {
            white_name
        } else {
            black_name
        };
        let on_mode = cb.on_preview_mode.clone();
        let on_reset = cb.on_reset_preview.clone();
        details = details.child(
            div()
                .id("play-candidate-preview-box")
                .test_support()
                .v_flex()
                .gap(px(8.0))
                .p(px(10.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface_alt)
                .child(
                    PreviewBoardControls::new("play-candidate-preview-controls", preview_mode)
                        .preview_summary(player_label.to_string(), move_label)
                        .can_reset(selected_candidate.is_some())
                        .palette(p)
                        .on_mode(move |m, window, cx| on_mode(m, window, cx))
                        .on_reset(move |_: &ClickEvent, window, cx| on_reset(window, cx)),
                )
                .child(div().w_full().h(px(230.0)).child(board_entity.clone())),
        );
    }

    // Ranked alternatives table
    if !decision.alternatives.is_empty() {
        let mut table = div()
            .id("play-feedback-alternatives")
            .test_support()
            .v_flex()
            .gap(px(4.0))
            .p(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .pb(px(4.0))
                    .border_b_1()
                    .border_color(p.border)
                    .text_table_heading()
                    .text_color(p.muted)
                    .child(
                        div()
                            .flex_1()
                            .child(I18n::t(locale, "play-mode", "selected")),
                    )
                    .child(div().w(px(62.0)).text_right().child("Δ"))
                    .when(full, |this| {
                        this.child(div().w(px(72.0)).text_right().child(I18n::t(
                            locale,
                            "play-mode",
                            "equity",
                        )))
                        .child(
                            div().w(px(148.0)).text_right().child(I18n::t(
                                locale,
                                "play-mode",
                                "probabilities",
                            )),
                        )
                    }),
            );

        for (idx, alt) in decision.alternatives.iter().enumerate() {
            let is_move = alt.action == "move";
            let selected = is_move && selected_candidate == Some(idx);
            let on_select = cb.on_select_candidate.clone();
            let probs_text = alt.probabilities.as_ref().and_then(|v| {
                if v.len() >= 6 {
                    Some(format!(
                        "{} / {} / {}",
                        format_percent(locale, v[0], 1),
                        format_percent(locale, v[1], 1),
                        format_percent(locale, v[2], 1),
                    ))
                } else {
                    None
                }
            });
            table = table.child(
                div()
                    .id(SharedString::from(format!("play-feedback-alt-{idx}")))
                    .test_support()
                    .h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(6.0))
                    .py(px(4.0))
                    .rounded(px(5.0))
                    .bg(if selected { p.selected } else { p.surface })
                    .when(is_move, |this| {
                        this.cursor_pointer()
                            .hover(move |s| s.bg(p.selected))
                            .on_click(move |_: &ClickEvent, window, cx| on_select(idx, window, cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .text_table()
                            .font_weight(if idx == 0 {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(p.text)
                            .truncate()
                            .child(localize_decision(locale, &alt.notation)),
                    )
                    .child(
                        div()
                            .w(px(62.0))
                            .text_right()
                            .text_table()
                            .tabular_nums()
                            .text_color(p.text)
                            .child(format_decimal(locale, alt.loss, 3, 3)),
                    )
                    .when(full, |this| {
                        this.child(
                            div()
                                .w(px(72.0))
                                .text_right()
                                .text_table()
                                .tabular_nums()
                                .text_color(p.muted)
                                .child(
                                    alt.equity
                                        .map(|eq| format_equity(locale, eq))
                                        .unwrap_or_else(|| "—".to_string()),
                                ),
                        )
                        .child(
                            div()
                                .w(px(148.0))
                                .text_right()
                                .text_meta()
                                .tabular_nums()
                                .text_color(p.muted)
                                .child(probs_text.unwrap_or_else(|| "—".to_string())),
                        )
                    }),
            );
        }
        details = details.child(table);
    }

    // Move explanation comparing best play vs chosen play (`crate::review::move_explanation`)
    let best_move = decision
        .alternatives
        .iter()
        .find(|a| a.action == "move" && a.loss.abs() < 1e-9);
    let chosen_move = (decision.action.as_deref() == Some("move"))
        .then_some(decision.notation.as_deref())
        .flatten();
    if let (Some(best), Some(chosen)) = (best_move, chosen_move)
        && best.notation != chosen
        && session.position.dice.len() >= 2
    {
        let board = board_array(&session.position.board);
        let dice = [session.position.dice[0], session.position.dice[1]];
        if let Ok(expl) =
            explain_checker_plays(board, session.position.player, dice, &best.notation, chosen)
        {
            let best_chances = best
                .probabilities
                .as_ref()
                .and_then(|v| (v.len() >= 6).then(|| [v[0], v[1], v[2], v[3], v[4], v[5]]));
            let played_chances = decision
                .alternatives
                .iter()
                .find(|a| a.action == "move" && a.notation == chosen)
                .and_then(|a| a.probabilities.as_ref())
                .and_then(|v| (v.len() >= 6).then(|| [v[0], v[1], v[2], v[3], v[4], v[5]]));
            details = details.child(
                MoveExplanationView::from_explanation(
                    "play-feedback-move-explanation",
                    &best.notation,
                    chosen,
                    &expl,
                    best_chances,
                    played_chances,
                )
                .palette(p),
            );
        }
    }

    details.into_any_element()
}

/// Renders the Training / Tutor / Competition feedback body (`PlayFeedback.tsx`).
#[allow(clippy::too_many_arguments)]
pub fn render_feedback_body(
    session: &PlaySession,
    white_name: &str,
    black_name: &str,
    busy: bool,
    selected_candidate: Option<usize>,
    preview_mode: BoardViewMode,
    preview_board: Option<&Entity<BoardView>>,
    palette: NeonPalette,
    cb: PlayFeedbackCallbacks,
    cx: &App,
) -> Option<AnyElement> {
    let decision = session.pending_decision.as_ref()?;
    let quiet = session.play_mode == PlayMode::Competition;
    if quiet && decision.action.is_none() {
        return None;
    }
    let locale = current_locale(cx);
    let p = palette;
    let choice = format_decision_choice(locale, session);
    let heading = if quiet {
        I18n::t(locale, "play-mode", "unconfirmed").to_string()
    } else if choice.is_empty() {
        I18n::t(locale, "play-mode", "feedback").to_string()
    } else {
        format!("{}: {choice}", I18n::t(locale, "play-mode", "feedback"))
    };
    let tutor_warning = session.play_mode == PlayMode::Tutor && decision.warning;

    let build_actions = || {
        let on_retry = cb.on_retry.clone();
        let on_reveal = cb.on_reveal.clone();
        let on_continue = cb.on_continue.clone();
        div()
            .h_flex()
            .items_center()
            .gap(px(8.0))
            .when(!quiet && decision.action.is_some(), |this| {
                this.child(
                    ActionButton::new("play-feedback-retry")
                        .icon(ActionIconName::Reset)
                        .label(I18n::t(locale, "play-mode", "retry"))
                        .disabled(busy)
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| on_retry(window, cx)),
                )
            })
            .when(!quiet && (!decision.revealed || tutor_warning), |this| {
                this.child(
                    ActionButton::new("play-feedback-reveal")
                        .icon(ActionIconName::Solution)
                        .label(I18n::t(locale, "play-mode", "solution"))
                        .disabled(busy || decision.revealed)
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| on_reveal(window, cx)),
                )
            })
            .when(decision.action.is_some(), |this| {
                let continue_key = if !quiet && decision.warning {
                    "anyway"
                } else {
                    "continue"
                };
                this.child(
                    ActionButton::new("play-feedback-continue")
                        .icon(ActionIconName::Confirm)
                        .label(I18n::t(locale, "play-mode", continue_key))
                        .shortcut("Enter")
                        .primary()
                        .disabled(busy)
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| on_continue(window, cx)),
                )
            })
    };

    let mut panel = div()
        .id("play-feedback-panel")
        .test_support()
        .v_flex()
        .gap(px(10.0))
        .child(div().text_panel().text_color(p.text).child(heading));

    if !quiet && decision.warning && !decision.revealed {
        panel = panel.child(
            div()
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(6.0))
                .bg(p.warning_bg)
                .text_body()
                .text_color(p.warning_text)
                .child(I18n::t(locale, "play-mode", "warning")),
        );
    }

    if tutor_warning {
        panel = panel.child(build_actions());
    }

    if !quiet && decision.revealed {
        panel = panel.child(render_decision_details(
            session,
            decision,
            white_name,
            black_name,
            selected_candidate,
            preview_mode,
            preview_board,
            p,
            &cb,
            cx,
        ));
    }

    if !tutor_warning {
        panel = panel.child(build_actions());
    }

    if !quiet && decision.original_action.is_some() {
        panel = panel.child(div().text_meta().text_color(p.muted).child(I18n::t(
            locale,
            "play-mode",
            "originalHelp",
        )));
    }

    Some(panel.into_any_element())
}

/// Renders the inline Coach feedback toolbar above the board (`PlayCoachFeedback.tsx`).
#[allow(clippy::too_many_arguments)]
pub fn render_coach_toolbar(
    session: &PlaySession,
    decision: &PlayDecision,
    busy: bool,
    error_banner: Option<AnyElement>,
    palette: NeonPalette,
    on_continue: PlayVoidCallback,
    on_open_details: PlayVoidCallback,
    cx: &App,
) -> AnyElement {
    let locale = current_locale(cx);
    let p = palette;
    let choice = format_decision_choice(locale, session);
    let is_pass = decision.action.as_deref() == Some("pass");
    let is_checker = matches!(decision.action.as_deref(), Some("move") | Some("pass"));
    let sev = if decision.revealed && decision.loss.is_some() {
        parse_play_severity(decision.severity.as_deref())
    } else {
        neondojo::model::MoveSeverity::None
    };
    let style = SeverityStyle::resolve(sev, &p);

    let main_icon = if is_pass {
        ActionIconName::Pass
    } else {
        ActionIconName::Confirm
    };
    let main_label = if is_checker {
        I18n::t(locale, "play", if is_pass { "passTurn" } else { "confirm" })
    } else {
        I18n::t(locale, "play-mode", "continue")
    };

    div()
        .id("play-coach-toolbar")
        .test_support()
        .v_flex()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(p.border_strong)
        .bg(p.surface)
        .child(
            div()
                .h_flex()
                .items_center()
                .flex_wrap()
                .gap(px(10.0))
                .child(div().text_panel().text_color(p.text).child(I18n::t(
                    locale,
                    "play-mode",
                    "coach",
                )))
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded(px(6.0))
                        .bg(style.bg)
                        .text_color(style.fg)
                        .child(
                            div()
                                .text_label()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(choice),
                        )
                        .when(decision.revealed && decision.loss.is_some(), |this| {
                            let loss = decision.loss.unwrap_or(0.0);
                            this.child("·")
                                .child(div().text_meta().tabular_nums().child(format!(
                                    "{} ({}: {})",
                                    style.label(locale),
                                    I18n::t(locale, "play-mode", "loss"),
                                    format_decimal(locale, loss, 3, 3)
                                )))
                        }),
                )
                .child(
                    ActionButton::new("play-coach-continue")
                        .icon(main_icon)
                        .label(main_label)
                        .shortcut("Enter")
                        .primary()
                        .disabled(busy)
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| on_continue(window, cx)),
                )
                .child(
                    ActionButton::new("play-coach-details")
                        .icon(ActionIconName::Analysis)
                        .label(I18n::t(locale, "play-mode", "details"))
                        .disabled(busy)
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| on_open_details(window, cx)),
                )
                .when(is_checker, |this| {
                    this.child(div().text_meta().text_color(p.muted).child(I18n::t(
                        locale,
                        "play",
                        if is_pass { "passReady" } else { "ready" },
                    )))
                }),
        )
        .when_some(error_banner, |this, err| this.child(err))
        .into_any_element()
}

/// Renders the bot-offered double decision dialog (`PlayCubeDecision.tsx` & `PlayCubeDialog`).
#[allow(clippy::too_many_arguments)]
pub fn render_cube_dialog_body(
    session: &PlaySession,
    busy: bool,
    awaiting_feedback: bool,
    competition: bool,
    can_forfeit: bool,
    error_banner: Option<AnyElement>,
    feedback_panel: Option<AnyElement>,
    palette: NeonPalette,
    on_take: PlayVoidCallback,
    on_drop: PlayVoidCallback,
    on_hint: PlayVoidCallback,
    on_pause: PlayVoidCallback,
    on_forfeit: PlayVoidCallback,
    cx: &App,
) -> AnyElement {
    let locale = current_locale(cx);
    let p = palette;
    let current_cube = decode_match_context(&session.position.gnubg_match_id)
        .cube
        .max(1);
    let offered_cube = current_cube * 2;

    div()
        .id("play-cube-dialog-body")
        .test_support()
        .v_flex()
        .gap(px(12.0))
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(10.0))
                .px(px(12.0))
                .py(px(10.0))
                .rounded(px(8.0))
                .bg(p.surface_alt)
                .border_1()
                .border_color(p.border)
                .child(div().text_body().text_color(p.muted).child(format!(
                    "{} {}",
                    I18n::t(locale, "play", "cubeBefore"),
                    format_int(locale, i64::from(current_cube))
                )))
                .child(div().text_subject().text_color(p.muted).child("→"))
                .child(
                    div()
                        .text_subject()
                        .tabular_nums()
                        .text_color(p.accent)
                        .child(format!(
                            "{}: {}",
                            I18n::t(locale, "play", "cubeOffered"),
                            format_int(locale, i64::from(offered_cube))
                        )),
                ),
        )
        .child(div().text_body().text_color(p.muted).child(I18n::t(
            locale,
            "play",
            "cubeDecisionRequired",
        )))
        .when_some(error_banner, |this, err| this.child(err))
        .when_some(feedback_panel, |this, fb| this.child(fb))
        .when(!awaiting_feedback, |this| {
            this.child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        ActionButton::new("play-cube-take")
                            .icon(ActionIconName::Take)
                            .label(I18n::t(locale, "play", "take"))
                            .primary()
                            .disabled(busy)
                            .palette(p)
                            .on_click(move |_: &ClickEvent, window, cx| on_take(window, cx)),
                    )
                    .child(
                        ActionButton::new("play-cube-drop")
                            .icon(ActionIconName::Drop)
                            .label(I18n::t(locale, "play", "drop"))
                            .disabled(busy)
                            .palette(p)
                            .on_click(move |_: &ClickEvent, window, cx| on_drop(window, cx)),
                    )
                    .when(!competition, |inner| {
                        inner.child(
                            ActionButton::new("play-cube-hint")
                                .icon(ActionIconName::Hint)
                                .label(I18n::t(locale, "play-mode", "hint"))
                                .disabled(busy)
                                .palette(p)
                                .on_click(move |_: &ClickEvent, window, cx| on_hint(window, cx)),
                        )
                    }),
            )
        })
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(8.0))
                .pt(px(8.0))
                .border_t_1()
                .border_color(p.border)
                .child(
                    ActionButton::new("play-cube-pause")
                        .icon(ActionIconName::Pause)
                        .label(I18n::t(locale, "play", "pauseMatch"))
                        .disabled(busy)
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| on_pause(window, cx)),
                )
                .when(can_forfeit, |this| {
                    this.child(
                        ActionButton::new("play-cube-forfeit")
                            .icon(ActionIconName::Resign)
                            .label(I18n::t(locale, "play-resign", "forfeitMatch"))
                            .danger()
                            .disabled(busy)
                            .palette(p)
                            .on_click(move |_: &ClickEvent, window, cx| on_forfeit(window, cx)),
                    )
                }),
        )
        .into_any_element()
}

/// Renders the game resignation dialog (`PlayResignDialog.tsx`).
#[allow(clippy::too_many_arguments)]
pub fn render_resign_dialog_body(
    session: &PlaySession,
    white_name: &str,
    black_name: &str,
    selected_level: ResignLevel,
    busy: bool,
    can_forfeit: bool,
    resignation_declined: bool,
    error_banner: Option<AnyElement>,
    palette: NeonPalette,
    on_select_level: PlayArgCallback<ResignLevel>,
    on_confirm: PlayArgCallback<ResignLevel>,
    on_forfeit: PlayVoidCallback,
    on_cancel: PlayVoidCallback,
    cx: &App,
) -> AnyElement {
    let locale = current_locale(cx);
    let p = palette;
    let cube = decode_match_context(&session.position.gnubg_match_id)
        .cube
        .max(1);
    let proj = project_resignation(
        selected_level,
        cube,
        session.scores,
        session.human_player,
        session.match_length,
    );
    let opponent = if session.human_player == 0 {
        black_name
    } else {
        white_name
    };
    let points_word = I18n::t(
        locale,
        "game-result",
        if proj.points == 1 { "point" } else { "points" },
    );
    let award_line = I18n::format(
        locale,
        "play-resign",
        "award",
        &[
            ("player", Param::from(opponent)),
            (
                "points",
                Param::from(format!(
                    "{} {points_word}",
                    format_int(locale, i64::from(proj.points))
                )),
            ),
        ],
    );
    let outcome_key = if proj.ends_match {
        "matchEnds"
    } else if session.match_length > 0 {
        "matchContinues"
    } else {
        "sessionContinues"
    };
    let outcome_line = I18n::format(
        locale,
        "play-resign",
        outcome_key,
        &[("player", Param::from(opponent))],
    );

    let mut level_row = div().h_flex().gap(px(8.0));
    for &lvl in ResignLevel::ALL {
        let on_sel = on_select_level.clone();
        level_row = level_row.child(
            ActionButton::new(SharedString::from(format!(
                "play-resign-level-{}",
                lvl.as_i32()
            )))
            .label(I18n::t(locale, "game-result", lvl.reason_key()))
            .active(selected_level == lvl)
            .disabled(busy)
            .palette(p)
            .on_click(move |_: &ClickEvent, window, cx| on_sel(lvl, window, cx)),
        );
    }

    div()
        .id("play-resign-dialog-body")
        .test_support()
        .v_flex()
        .gap(px(12.0))
        .child(
            div()
                .text_body()
                .text_color(p.muted)
                .child(I18n::t(locale, "play-resign", "help")),
        )
        .when(resignation_declined, |this| {
            this.child(
                div()
                    .id("play-resign-declined-alert")
                    .test_support()
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(px(6.0))
                    .bg(p.warning_bg)
                    .text_body()
                    .text_color(p.warning_text)
                    .child(I18n::t(locale, "play-resign", "declined")),
            )
        })
        .when(!resignation_declined, |this| {
            this.when_some(error_banner, |inner, err| inner.child(err))
        })
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .child(div().text_label().text_color(p.muted).child(I18n::t(
                    locale,
                    "play-resign",
                    "level",
                )))
                .child(level_row),
        )
        .child(
            div()
                .v_flex()
                .gap(px(4.0))
                .p(px(10.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(if proj.ends_match {
                    p.danger_border
                } else {
                    p.border
                })
                .bg(if proj.ends_match {
                    p.danger_bg
                } else {
                    p.surface_alt
                })
                .child(
                    div()
                        .text_panel()
                        .tabular_nums()
                        .text_color(p.text)
                        .child(format!(
                            "{} × {} {} = {}",
                            format_int(locale, i64::from(selected_level.as_i32())),
                            I18n::t(locale, "game-result", "cube"),
                            format_int(locale, i64::from(cube)),
                            format_int(locale, i64::from(proj.points))
                        )),
                )
                .child(div().text_body().text_color(p.text).child(award_line))
                .child(
                    div()
                        .text_meta()
                        .tabular_nums()
                        .text_color(p.muted)
                        .child(format!(
                            "{}: {} {} : {} {} · {}",
                            I18n::t(
                                locale,
                                "play-resign",
                                if session.match_length > 0 {
                                    "score"
                                } else {
                                    "moneyScore"
                                }
                            ),
                            white_name,
                            format_int(locale, i64::from(proj.projected_scores[0])),
                            format_int(locale, i64::from(proj.projected_scores[1])),
                            black_name,
                            if session.match_length > 0 {
                                I18n::format(
                                    locale,
                                    "play-resign",
                                    "target",
                                    &[("points", Param::from(session.match_length))],
                                )
                            } else {
                                I18n::t(locale, "app", "common.moneyGame").to_string()
                            }
                        )),
                )
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if proj.ends_match {
                            p.danger_text
                        } else {
                            p.text
                        })
                        .child(outcome_line),
                ),
        )
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_end()
                .gap(px(8.0))
                .child(
                    ActionButton::new("play-resign-cancel")
                        .icon(ActionIconName::Cancel)
                        .label(I18n::t(locale, "play-resign", "cancel"))
                        .disabled(busy)
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| on_cancel(window, cx)),
                )
                .when(can_forfeit && !proj.ends_match, |this| {
                    this.child(
                        ActionButton::new("play-resign-forfeit-instead")
                            .icon(ActionIconName::Resign)
                            .label(I18n::t(locale, "play-resign", "forfeitMatch"))
                            .danger()
                            .disabled(busy)
                            .palette(p)
                            .on_click(move |_: &ClickEvent, window, cx| on_forfeit(window, cx)),
                    )
                })
                .child(
                    ActionButton::new("play-resign-confirm")
                        .icon(ActionIconName::Resign)
                        .label(I18n::t(
                            locale,
                            "play-resign",
                            if proj.ends_match && session.match_length > 0 {
                                "confirmMatch"
                            } else {
                                "confirmGame"
                            },
                        ))
                        .danger_solid()
                        .disabled(busy)
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| {
                            on_confirm(selected_level, window, cx);
                        }),
                ),
        )
        .into_any_element()
}

/// Renders the match forfeit confirmation dialog (`PlayForfeitDialog`).
pub fn render_forfeit_dialog_body(
    busy: bool,
    error_banner: Option<AnyElement>,
    palette: NeonPalette,
    on_cancel: PlayVoidCallback,
    on_forfeit: PlayVoidCallback,
    cx: &App,
) -> AnyElement {
    let locale = current_locale(cx);
    let p = palette;
    div()
        .id("play-forfeit-dialog-body")
        .test_support()
        .v_flex()
        .gap(px(12.0))
        .child(div().text_body().text_color(p.muted).child(I18n::t(
            locale,
            "play-resign",
            "forfeitHelp",
        )))
        .when_some(error_banner, |this, err| this.child(err))
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_end()
                .gap(px(8.0))
                .child(
                    ActionButton::new("play-forfeit-cancel")
                        .icon(ActionIconName::Cancel)
                        .label(I18n::t(locale, "play-resign", "cancel"))
                        .disabled(busy)
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| on_cancel(window, cx)),
                )
                .child(
                    ActionButton::new("play-forfeit-confirm")
                        .icon(ActionIconName::Resign)
                        .label(I18n::t(locale, "play-resign", "forfeitMatch"))
                        .danger_solid()
                        .disabled(busy)
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| on_forfeit(window, cx)),
                ),
        )
        .into_any_element()
}

/// Renders the pause confirmation warning when uncommitted steps would be lost (`PlayPauseDialog`).
pub fn render_pause_warning_body(
    busy: bool,
    error_banner: Option<AnyElement>,
    palette: NeonPalette,
    on_cancel: PlayVoidCallback,
    on_pause: PlayVoidCallback,
    cx: &App,
) -> AnyElement {
    let locale = current_locale(cx);
    let p = palette;
    div()
        .id("play-pause-warning-body")
        .test_support()
        .v_flex()
        .gap(px(12.0))
        .child(div().text_body().text_color(p.muted).child(I18n::t(
            locale,
            "play",
            "pauseDraftHelp",
        )))
        .when_some(error_banner, |this, err| this.child(err))
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_end()
                .gap(px(8.0))
                .child(
                    ActionButton::new("play-pause-cancel")
                        .icon(ActionIconName::Close)
                        .label(I18n::t(locale, "library", "cancel"))
                        .disabled(busy)
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| on_cancel(window, cx)),
                )
                .child(
                    ActionButton::new("play-pause-confirm")
                        .icon(ActionIconName::Pause)
                        .label(I18n::t(locale, "play", "pauseMatch"))
                        .primary()
                        .disabled(busy)
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| on_pause(window, cx)),
                ),
        )
        .into_any_element()
}

/// Callbacks for [`render_result_dialog_body`].
#[derive(Clone)]
pub struct PlayResultCallbacks {
    pub on_next_game: PlayVoidCallback,
    pub on_end_session: PlayVoidCallback,
    pub on_pause: PlayVoidCallback,
    pub on_forfeit: PlayVoidCallback,
    pub on_new_match: PlayVoidCallback,
    pub on_back_to_board: PlayVoidCallback,
    pub on_learning_review: PlayVoidCallback,
    pub on_open_study_report: PlayArgCallback<Uuid>,
}

/// Renders `GameOver` or `MatchOver` result dialog (`PlayResultDialog`).
#[allow(clippy::too_many_arguments)]
pub fn render_result_dialog_body(
    kind: PlayInterruption,
    session: &PlaySession,
    white_name: &str,
    black_name: &str,
    busy: bool,
    failed: bool,
    can_forfeit: bool,
    error_banner: Option<AnyElement>,
    feedback_panel: Option<AnyElement>,
    palette: NeonPalette,
    cb: PlayResultCallbacks,
    cx: &App,
) -> AnyElement {
    let locale = current_locale(cx);
    let p = palette;
    let money_finished = is_money_game_finished(Some(session));
    let current_result = session
        .game_results
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .find(|entry| entry.game_no == session.game_no)
        .map(|entry| entry.result.clone());

    let mut root = div()
        .id("play-result-dialog-body")
        .test_support()
        .v_flex()
        .gap(px(12.0))
        .when_some(error_banner, |this, err| this.child(err))
        .when_some(feedback_panel, |this, fb| this.child(fb));

    if session.match_forfeited {
        root = root.child(div().text_body().text_color(p.danger_text).child(I18n::t(
            locale,
            "play-resign",
            "matchForfeited",
        )));
    } else if let Some(res) = current_result {
        root = root.child(
            GameResultSummaryView::new(
                "play-result-summary",
                res,
                white_name.to_string(),
                black_name.to_string(),
            )
            .palette(p),
        );
    } else if let Some(winner) = session.winner {
        let winner_name = if winner == 0 { white_name } else { black_name };
        root = root.child(div().text_panel().text_color(p.text).child(format!(
            "{}: {winner_name}",
            I18n::t(locale, "play", "winner")
        )));
    }

    let mut actions = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0));
    if kind == PlayInterruption::GameOver {
        let next_disabled = busy || (money_finished && session.review.is_none() && !failed);
        let on_next = cb.on_next_game.clone();
        let on_pause = cb.on_pause.clone();
        actions = actions
            .child(
                ActionButton::new("play-result-next-game")
                    .icon(ActionIconName::NewGame)
                    .label(I18n::t(locale, "play", "next_game"))
                    .primary()
                    .disabled(next_disabled)
                    .palette(p)
                    .on_click(move |_: &ClickEvent, window, cx| on_next(window, cx)),
            )
            .child(
                ActionButton::new("play-result-pause")
                    .icon(ActionIconName::Pause)
                    .label(I18n::t(locale, "play", "pauseMatch"))
                    .disabled(busy)
                    .palette(p)
                    .on_click(move |_: &ClickEvent, window, cx| on_pause(window, cx)),
            );
        if money_finished {
            let on_end = cb.on_end_session.clone();
            actions = actions.child(
                ActionButton::new("play-result-end-session")
                    .icon(ActionIconName::Confirm)
                    .label(I18n::t(locale, "play", "endSession"))
                    .disabled(busy)
                    .palette(p)
                    .on_click(move |_: &ClickEvent, window, cx| on_end(window, cx)),
            );
        }
        if can_forfeit {
            let on_forfeit = cb.on_forfeit.clone();
            actions = actions.child(
                ActionButton::new("play-result-forfeit")
                    .icon(ActionIconName::Resign)
                    .label(I18n::t(locale, "play-resign", "forfeitMatch"))
                    .danger()
                    .disabled(busy)
                    .palette(p)
                    .on_click(move |_: &ClickEvent, window, cx| on_forfeit(window, cx)),
            );
        }
    } else {
        let on_new = cb.on_new_match.clone();
        let on_back = cb.on_back_to_board.clone();
        actions = actions
            .child(
                ActionButton::new("play-result-new-match")
                    .icon(ActionIconName::NewGame)
                    .label(I18n::t(locale, "play", "newMatch"))
                    .primary()
                    .disabled(busy)
                    .palette(p)
                    .on_click(move |_: &ClickEvent, window, cx| on_new(window, cx)),
            )
            .child(
                ActionButton::new("play-result-back-to-board")
                    .icon(ActionIconName::Back)
                    .label(I18n::t(locale, "play", "backToBoard"))
                    .disabled(busy)
                    .palette(p)
                    .on_click(move |_: &ClickEvent, window, cx| on_back(window, cx)),
            );
    }

    if let Some(match_id) = session.library_match_id
        && (kind == PlayInterruption::MatchOver || money_finished)
    {
        let on_study = cb.on_open_study_report.clone();
        actions = actions.child(
            ActionButton::new("play-result-open-study-report")
                .icon(ActionIconName::Open)
                .label(I18n::t(locale, "study", "open"))
                .disabled(busy)
                .palette(p)
                .on_click(move |_: &ClickEvent, window, cx| on_study(match_id, window, cx)),
        );
    }

    if money_finished || kind == PlayInterruption::MatchOver {
        let on_review = cb.on_learning_review.clone();
        actions = actions.child(
            ActionButton::new("play-result-learning-review")
                .icon(ActionIconName::Analysis)
                .label(I18n::t(locale, "play-mode", "reviewTooltip"))
                .disabled(busy || (money_finished && session.review.is_none()))
                .palette(p)
                .on_click(move |_: &ClickEvent, window, cx| on_review(window, cx)),
        );
    }

    root.child(actions).into_any_element()
}

/// Renders the in-workspace Learning Review summary modal for a finished game/match.
pub fn render_learning_review_body(
    session: &PlaySession,
    review: &PlayReview,
    palette: NeonPalette,
    on_open_study_report: PlayArgCallback<Uuid>,
    cx: &App,
) -> AnyElement {
    let locale = current_locale(cx);
    let p = palette;
    let pr_text = review
        .first_attempt_rating
        .map(|pr| format_decimal(locale, pr, 2, 2))
        .unwrap_or_else(|| "—".to_string());
    let total_loss = format_decimal(locale, review.first_attempt_loss, 3, 3);
    let final_loss = format_decimal(locale, review.final_choice_loss, 3, 3);

    div()
        .id("play-learning-review-body")
        .test_support()
        .v_flex()
        .gap(px(12.0))
        .child(
            div()
                .h_flex()
                .flex_wrap()
                .gap(px(12.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface_alt)
                .child(metric_tile(
                    I18n::t(locale, "play-mode", "firstPr"),
                    pr_text,
                    p,
                ))
                .child(metric_tile(
                    I18n::t(locale, "play-mode", "totalLoss"),
                    total_loss,
                    p,
                ))
                .child(metric_tile(
                    I18n::t(locale, "play-mode", "played"),
                    final_loss,
                    p,
                ))
                .child(metric_tile(
                    I18n::t(locale, "play-mode", "attempts"),
                    format!(
                        "{} ({} / {})",
                        format_int(locale, review.first_attempt_count as i64),
                        format_int(locale, review.independent_first_attempt_count as i64),
                        format_int(locale, review.assisted_first_attempt_count as i64),
                    ),
                    p,
                )),
        )
        .child(div().text_meta().text_color(p.muted).child(I18n::t(
            locale,
            "play-mode",
            "firstPrHelp",
        )))
        .child(
            div()
                .text_meta()
                .text_color(p.muted)
                .child(I18n::t(locale, "study", "saved")),
        )
        .when_some(session.library_match_id, |this, mid| {
            this.child(
                div().h_flex().justify_end().child(
                    ActionButton::new("play-learning-review-open-study")
                        .icon(ActionIconName::Open)
                        .label(I18n::t(locale, "study", "open"))
                        .primary()
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| {
                            on_open_study_report(mid, window, cx);
                        }),
                ),
            )
        })
        .into_any_element()
}

fn metric_tile(label: &str, value: String, p: NeonPalette) -> AnyElement {
    div()
        .v_flex()
        .gap(px(2.0))
        .min_w(px(110.0))
        .child(
            div()
                .text_meta()
                .text_color(p.muted)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_subject()
                .tabular_nums()
                .text_color(p.text)
                .child(value),
        )
        .into_any_element()
}
