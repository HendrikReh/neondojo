//! Left-pane virtualized move history (`uniform_list`) interleaving cube
//! actions and checker moves (`crate::review::MoveHistoryRow`), severity
//! filters, keyboard Up/Down stepping, and `GameResultSummary` footer.

use super::state::{Action3Cb, ActionCb, ActiveBoardTarget, InterleavedMoveRow, MoveHistoryFilter};
use crate::i18n::current_locale;
use crate::review::move_history_row::{AssessmentSource, MoveAssessmentBadge, MoveHistoryRow};
use crate::review::severity::SeverityStyle;
use crate::theme::{NeonPalette, NeonTypographyExt};
use gpui_kit::{
    App, ClickEvent, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt, UniformListScrollHandle, base::StyledExt,
    div, prelude::FluentBuilder, px, rgb, uniform_list,
};
use neondojo::model::{GameResult, GameResultReason, MatchGame};
use neondojo_i18n::{I18n, format_decimal, localize_decision};
use std::rc::Rc;
use uuid::Uuid;

/// Callbacks wired from `AnalysisWorkspace` into the move history pane.
#[derive(Clone)]
pub struct MoveHistoryCallbacks {
    pub on_select_filter: ActionCb<MoveHistoryFilter>,
    pub on_select_checker: ActionCb<Uuid>,
    pub on_select_cube: Action3Cb<String, i32, Option<Uuid>>,
    pub on_open_move_notes: ActionCb<Option<Uuid>>,
}

/// Renders the left Move History pane including severity filters, `uniform_list`
/// of interleaved cube/checker rows, and `GameResultSummary` footer.
#[allow(clippy::too_many_arguments)]
pub fn render_move_history_pane(
    rows: Vec<InterleavedMoveRow>,
    active_target: Option<&ActiveBoardTarget>,
    filter: MoveHistoryFilter,
    current_game: Option<&MatchGame>,
    players: [SharedString; 2],
    loading: bool,
    scroll_handle: &UniformListScrollHandle,
    callbacks: MoveHistoryCallbacks,
    p: NeonPalette,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let total_rows = rows.len();
    let active_checker_id = match active_target {
        Some(ActiveBoardTarget::Checker { position_id }) => Some(*position_id),
        _ => None,
    };
    let active_cube_id = match active_target {
        Some(ActiveBoardTarget::Cube { review_id, .. }) => Some(review_id.clone()),
        _ => None,
    };

    let rows_rc = Rc::new(rows);
    let list_rows = rows_rc.clone();
    let list_callbacks = callbacks.clone();
    let list_players = players.clone();

    let filter_bar = div()
        .id("analysis-move-filter-bar")
        .test_support()
        .h_flex()
        .flex_wrap()
        .gap(px(4.0))
        .px(px(8.0))
        .py(px(6.0))
        .border_b_1()
        .border_color(p.border)
        .children(MoveHistoryFilter::ALL.into_iter().map(|f| {
            let selected = f == filter;
            let on_filter = callbacks.on_select_filter.clone();
            let label = filter_label(locale, f);
            div()
                .id(SharedString::from(format!("move-filter-{}", f.id())))
                .test_support()
                .cursor_pointer()
                .px(px(7.0))
                .py(px(3.0))
                .rounded(px(4.0))
                .text_micro()
                .font_weight(if selected {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::MEDIUM
                })
                .bg(if selected { p.selected } else { p.surface })
                .border_1()
                .border_color(if selected { p.accent } else { p.border })
                .text_color(if selected { p.accent } else { p.muted })
                .on_click(move |_, window, cx| on_filter(f, window, cx))
                .child(label)
        }));

    let header_title = current_game
        .map(|g| format!("Game {} · {} moves", g.game_no, total_rows))
        .unwrap_or_else(|| format!("Moves ({total_rows})"));

    let list_body = if loading && total_rows == 0 {
        div()
            .id("analysis-move-list-loading")
            .test_support()
            .flex_1()
            .p(px(12.0))
            .text_meta()
            .text_color(p.muted)
            .child(I18n::t(locale, "workspace", "loading"))
            .into_any_element()
    } else if total_rows == 0 {
        let empty_msg = if current_game.is_some_and(|g| g.position_count == 0) {
            "Cube-only game — no checker rolls recorded."
        } else if filter != MoveHistoryFilter::All {
            "No moves match the active filter."
        } else {
            "No moves in this game."
        };
        div()
            .id("analysis-move-list-empty")
            .test_support()
            .flex_1()
            .p(px(12.0))
            .text_meta()
            .text_color(p.muted)
            .child(empty_msg)
            .into_any_element()
    } else {
        uniform_list(
            "analysis-move-uniform-list",
            total_rows,
            move |range, _window, cx| {
                let locale = current_locale(cx);
                range
                    .filter_map(|ix| list_rows.get(ix).map(|row| (ix, row)))
                    .map(|(ix, row)| match row {
                        InterleavedMoveRow::Checker {
                            position,
                            assessment,
                        } => {
                            let pid = position.id;
                            let player = u8::try_from(position.player).unwrap_or(0).min(1);
                            let dice: Vec<u8> = position
                                .dice
                                .iter()
                                .filter_map(|&d| u8::try_from(d).ok())
                                .collect();
                            let badge = assessment.as_ref().map(|a| MoveAssessmentBadge {
                                equity_loss: a.equity_loss,
                                severity: a.severity,
                                source: if a.imported {
                                    AssessmentSource::Imported
                                } else {
                                    AssessmentSource::Engine
                                },
                                engine: SharedString::from(a.engine.clone()),
                                depth: a.depth.clone().map(SharedString::from),
                            });
                            let on_click = list_callbacks.on_select_checker.clone();
                            let mut elem = MoveHistoryRow::new(
                                SharedString::from(format!("move-row-{ix}")),
                                format!("{}", position.ply),
                                player,
                                list_players[usize::from(player)].clone(),
                                position.notation.clone(),
                            )
                            .dice(dice)
                            .assessment(badge)
                            .selected(active_checker_id == Some(pid))
                            .palette(p)
                            .on_click(move |_, window, cx| on_click(pid, window, cx));

                            if let Ok(board_arr) = <&[i32; 26]>::try_from(position.board.as_slice())
                            {
                                elem = elem.board_i32(board_arr);
                            }
                            div().w_full().py(px(1.0)).child(elem).into_any_element()
                        }
                        InterleavedMoveRow::CubeAction {
                            move_id,
                            move_no,
                            player,
                            notation,
                            review_id,
                            decision_no,
                            severity,
                            equity_loss,
                            evaluated,
                            return_position_id,
                            ..
                        } => {
                            let player_idx = usize::try_from(*player).unwrap_or(0).min(1);
                            let is_selected = review_id
                                .as_ref()
                                .is_some_and(|id| active_cube_id.as_deref() == Some(id.as_str()));
                            let sev_style = severity.map(|s| SeverityStyle::resolve(s, &p));
                            let action_label = localize_decision(locale, notation);
                            let on_cube = list_callbacks.on_select_cube.clone();
                            let on_notes = list_callbacks.on_open_move_notes.clone();
                            let rid = review_id.clone();
                            let dno = *decision_no;
                            let ret_pos = *return_position_id;
                            let mid = *move_id;

                            let dot_fill = if player_idx == 0 {
                                rgb(0xf4f4f0)
                            } else {
                                rgb(0x23272e)
                            };

                            div()
                                .id(SharedString::from(format!("cube-row-{ix}")))
                                .test_support()
                                .h_flex()
                                .items_center()
                                .justify_between()
                                .gap(px(6.0))
                                .w_full()
                                .px(px(8.0))
                                .py(px(5.0))
                                .rounded(px(5.0))
                                .bg(if is_selected {
                                    p.selected
                                } else {
                                    p.surface_alt
                                })
                                .border_1()
                                .border_color(if is_selected { p.accent } else { p.border })
                                .cursor_pointer()
                                .on_click(move |_ev: &ClickEvent, window, cx| {
                                    if let (Some(id), Some(no)) = (rid.clone(), dno) {
                                        on_cube(id, no, ret_pos, window, cx);
                                    } else {
                                        on_notes(mid, window, cx);
                                    }
                                })
                                .child(
                                    div()
                                        .h_flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .min_w(px(0.0))
                                        .child(
                                            div()
                                                .text_micro()
                                                .tabular_nums()
                                                .text_color(p.muted)
                                                .w(px(24.0))
                                                .child(format!("C{move_no}")),
                                        )
                                        .child(
                                            div()
                                                .w(px(8.0))
                                                .h(px(8.0))
                                                .rounded(px(99.0))
                                                .bg(dot_fill)
                                                .border_1()
                                                .border_color(p.control_border),
                                        )
                                        .child(
                                            div()
                                                .text_label()
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(p.text)
                                                .truncate()
                                                .child(format!(
                                                    "{}: {}",
                                                    list_players[player_idx], action_label
                                                )),
                                        ),
                                )
                                .child(if *evaluated {
                                    let loss_text = equity_loss
                                        .filter(|l| *l > 0.0005)
                                        .map(|l| format!("-{}", format_decimal(locale, l, 3, 3)))
                                        .unwrap_or_else(|| "0.000".to_string());
                                    div()
                                        .h_flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .when_some(sev_style, |el, st| {
                                            el.child(
                                                div()
                                                    .px(px(5.0))
                                                    .py(px(1.0))
                                                    .rounded(px(4.0))
                                                    .bg(st.bg)
                                                    .text_micro()
                                                    .tabular_nums()
                                                    .text_color(st.fg)
                                                    .child(loss_text.clone()),
                                            )
                                        })
                                        .into_any_element()
                                } else {
                                    div()
                                        .text_micro()
                                        .text_color(p.muted)
                                        .child("Not evaluated")
                                        .into_any_element()
                                })
                                .into_any_element()
                        }
                    })
                    .collect()
            },
        )
        .flex_1()
        .h_full()
        .px(px(6.0))
        .py(px(4.0))
        .track_scroll(scroll_handle)
        .into_any_element()
    };

    let footer = current_game
        .and_then(|g| g.result.as_ref())
        .map(|res| render_game_result_footer(res, &players, p, cx));

    div()
        .id("analysis-move-history-pane")
        .test_support()
        .v_flex()
        .w(px(super::state::MOVE_LIST_PANE_WIDTH))
        .h_full()
        .flex_shrink_0()
        .bg(p.surface)
        .border_r_1()
        .border_color(p.border)
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .px(px(10.0))
                .py(px(7.0))
                .border_b_1()
                .border_color(p.border)
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .child(header_title),
                )
                .when(current_game.is_some_and(|g| g.crawford), |el| {
                    el.child(
                        div()
                            .px(px(6.0))
                            .py(px(1.0))
                            .rounded(px(4.0))
                            .bg(p.warning_bg)
                            .text_micro()
                            .text_color(p.warning_text)
                            .child("Crawford"),
                    )
                }),
        )
        .child(filter_bar)
        .child(list_body)
        .when_some(footer, |el, f| el.child(f))
}

fn render_game_result_footer(
    result: &GameResult,
    players: &[SharedString; 2],
    p: NeonPalette,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let winner_idx = usize::try_from(result.winner).unwrap_or(0).min(1);
    let winner_name = &players[winner_idx];
    let reason = match result.reason {
        GameResultReason::Single => I18n::t(locale, "game-result", "single"),
        GameResultReason::Gammon => I18n::t(locale, "game-result", "gammon"),
        GameResultReason::Backgammon => I18n::t(locale, "game-result", "backgammon"),
        GameResultReason::Drop => I18n::t(locale, "game-result", "drop"),
        GameResultReason::Resignation => I18n::t(locale, "game-result", "resignation"),
        GameResultReason::Recorded => I18n::t(locale, "game-result", "recorded"),
    };

    div()
        .id("analysis-game-result-summary")
        .test_support()
        .v_flex()
        .gap(px(2.0))
        .px(px(10.0))
        .py(px(8.0))
        .bg(p.surface_alt)
        .border_t_1()
        .border_color(p.border)
        .child(
            div()
                .text_label()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text)
                .child(format!("{winner_name} +{} ({reason})", result.points)),
        )
        .child(
            div()
                .text_micro()
                .tabular_nums()
                .text_color(p.muted)
                .child(format!(
                    "Score: {}–{} · Cube {}",
                    result.scores_after[0], result.scores_after[1], result.cube
                )),
        )
}

fn filter_label(locale: neondojo_i18n::Locale, filter: MoveHistoryFilter) -> SharedString {
    match filter {
        MoveHistoryFilter::All => SharedString::from(I18n::t(locale, "workspace", "all")),
        MoveHistoryFilter::Errors => {
            SharedString::from(I18n::t(locale, "review-statistics", "errors"))
        }
        MoveHistoryFilter::Doubtful => SharedString::from("?!"),
        MoveHistoryFilter::Bad => SharedString::from("?"),
        MoveHistoryFilter::Blunder => SharedString::from("??"),
        MoveHistoryFilter::VeryBad => SharedString::from("???"),
    }
}
