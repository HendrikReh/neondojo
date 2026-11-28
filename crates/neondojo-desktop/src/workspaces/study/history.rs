//! Study → Practice History sub-tab:
//! - Mistake Practice History table (`MistakePracticeDay`)
//! - Similar Positions Practice History table (`SimilarPracticeSummary`) with
//!   cursor pagination and native GPUI `canvas()` trend chart
//! - Transfer Practice History (`TransferSummary`)
//! - Clickable drill-down detail view inspecting any selected entry.

use super::common::{
    UiAction, UiCallback, game_plan_from_str, game_plan_label, mistake_kind_from_str,
    mistake_kind_label,
};
use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::banners::{BannerKind, EmptyState, StatusBanner};
use crate::ui::icons::ActionIconName;
use gpui_kit::{
    App, FontWeight, InteractiveElement, IntoElement, ParentElement, PathBuilder, PathStyle,
    SharedString, StatefulInteractiveElement, StrokeOptions, Styled, TestSupportExt,
    base::StyledExt, canvas, div, fill, point, prelude::FluentBuilder, px, quad, size,
};
use neondojo::learning::TransferSummary;
use neondojo::recurring_mistakes::MistakePracticeDay;
use neondojo::similar_positions::SimilarPracticeSummary;
use neondojo_i18n::{
    I18n, format_datetime_short_local, format_decimal, format_int, format_naive_date_medium,
    format_percent,
};
use uuid::Uuid;

/// Identifies the currently inspected history entry in the drill-down detail pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedHistoryEntry {
    MistakeDay(usize),
    SimilarRound(Uuid),
    TransferRound(Uuid),
}

/// Interactive state for Study → Practice History.
pub struct PracticeHistoryState {
    pub mistake_days: Vec<MistakePracticeDay>,
    pub similar_rounds: Vec<SimilarPracticeSummary>,
    pub similar_next_cursor: Option<Uuid>,
    pub transfer_rounds: Vec<TransferSummary>,
    pub loading: bool,
    pub loading_more_similar: bool,
    pub error: Option<String>,
    pub selected: Option<SelectedHistoryEntry>,
}

impl Default for PracticeHistoryState {
    fn default() -> Self {
        Self::new()
    }
}

impl PracticeHistoryState {
    pub fn new() -> Self {
        Self {
            mistake_days: Vec::new(),
            similar_rounds: Vec::new(),
            similar_next_cursor: None,
            transfer_rounds: Vec::new(),
            loading: false,
            loading_more_similar: false,
            error: None,
            selected: None,
        }
    }
}

pub struct HistoryCallbacks {
    pub on_refresh: UiAction,
    pub on_load_more_similar: UiAction,
    pub on_select: UiCallback<Option<SelectedHistoryEntry>>,
}

/// Renders Study → Practice History (trend chart + session/day tables + drill-down detail view).
pub fn render_practice_history(
    state: &PracticeHistoryState,
    palette: NeonPalette,
    cb: HistoryCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let on_ref = cb.on_refresh.clone();

    let header_title = match locale {
        neondojo_i18n::Locale::De => "Übungsverlauf",
        neondojo_i18n::Locale::En => "Practice History",
    };

    let mut root = div()
        .id("study-history-panel")
        .test_support()
        .v_flex()
        .gap(px(14.0))
        .w_full()
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_panel()
                        .text_color(palette.text)
                        .child(header_title),
                )
                .child(
                    ActionButton::new("study-history-refresh")
                        .icon(ActionIconName::Refresh)
                        .label(I18n::t(locale, "study", "refresh"))
                        .disabled(state.loading)
                        .palette(palette)
                        .on_click(move |_, window, cx| on_ref(window, cx)),
                ),
        );

    if let Some(err) = &state.error {
        let on_retry = cb.on_refresh.clone();
        root = root.child(
            StatusBanner::new("study-history-err", BannerKind::Error, err.clone())
                .palette(palette)
                .on_retry(I18n::t(locale, "study", "retry"), move |_, window, cx| {
                    on_retry(window, cx)
                }),
        );
    }

    if state.loading
        && state.mistake_days.is_empty()
        && state.similar_rounds.is_empty()
        && state.transfer_rounds.is_empty()
    {
        return root.child(
            div()
                .p(px(20.0))
                .text_body()
                .text_color(palette.muted)
                .child(I18n::t(locale, "study", "loading")),
        );
    }

    let empty_all = state.mistake_days.is_empty()
        && state.similar_rounds.is_empty()
        && state.transfer_rounds.is_empty();

    if empty_all {
        let empty_desc = match locale {
            neondojo_i18n::Locale::De => {
                "Absolvierte Fehler-, Transfer- und Ähnliche-Stellungen-Übungen erscheinen hier."
            }
            neondojo_i18n::Locale::En => {
                "Completed recurring-mistake, transfer, and similar-position practice sessions appear here."
            }
        };
        return root.child(
            EmptyState::new("study-history-empty", header_title, empty_desc)
                .icon(ActionIconName::Cards)
                .palette(palette),
        );
    }

    // Left column: Trend Chart + Tables
    let mut left_col = div().v_flex().gap(px(14.0)).flex_1().min_w(px(380.0));

    // Native GPUI `canvas()` trend chart of recent average equity loss
    let chart_points: Vec<f64> = if !state.similar_rounds.is_empty() {
        state
            .similar_rounds
            .iter()
            .take(20)
            .rev()
            .map(|r| r.average_equity_loss.max(0.0))
            .collect()
    } else {
        state
            .mistake_days
            .iter()
            .take(20)
            .rev()
            .map(|d| d.average_equity_loss.max(0.0))
            .collect()
    };

    if chart_points.len() >= 2 {
        left_col = left_col.child(render_loss_trend_chart(chart_points, palette, cx));
    }

    // 1. Recurring Mistake Practice History Table
    if !state.mistake_days.is_empty() {
        let mut table = div()
            .id("history-mistakes-table")
            .test_support()
            .v_flex()
            .rounded(px(10.0))
            .border_1()
            .border_color(palette.border)
            .bg(palette.surface)
            .overflow_hidden()
            .child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .bg(palette.surface_alt)
                    .border_b_1()
                    .border_color(palette.border)
                    .text_panel()
                    .text_color(palette.text)
                    .child(I18n::t(locale, "study", "mistakes")),
            );

        for (idx, day) in state.mistake_days.iter().take(50).enumerate() {
            let active = state.selected == Some(SelectedHistoryEntry::MistakeDay(idx));
            let on_sel = cb.on_select.clone();
            let hover_bg = palette.surface_alt;
            let plan_txt = game_plan_from_str(&day.game_plan)
                .map(|p| game_plan_label(locale, p))
                .unwrap_or(&day.game_plan);
            let kind_txt = mistake_kind_from_str(&day.mistake_kind)
                .map(|k| mistake_kind_label(locale, k))
                .unwrap_or(&day.mistake_kind);
            let acc = if day.attempts > 0 {
                day.best as f64 / day.attempts as f64
            } else {
                0.0
            };

            table = table.child(
                div()
                    .id(SharedString::from(format!("history-mistake-row-{idx}")))
                    .test_support()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .px(px(12.0))
                    .py(px(7.0))
                    .border_b_1()
                    .border_color(palette.border)
                    .bg(if active {
                        palette.selected
                    } else {
                        palette.surface
                    })
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover_bg))
                    .on_click(move |_, window, cx| {
                        on_sel(Some(SelectedHistoryEntry::MistakeDay(idx)), window, cx)
                    })
                    .text_table()
                    .tabular_nums()
                    .child(
                        div()
                            .w(px(96.0))
                            .text_color(palette.muted)
                            .child(format_naive_date_medium(locale, day.day)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.text)
                            .child(format!("{} · {plan_txt} · {kind_txt}", day.player)),
                    )
                    .child(div().text_color(palette.muted).child(format!(
                        "{}/{} ({})",
                        day.best,
                        day.attempts,
                        format_percent(locale, acc, 0)
                    )))
                    .child(
                        div()
                            .w(px(80.0))
                            .text_right()
                            .text_color(palette.text)
                            .child(format!(
                                "Ø {}",
                                format_decimal(locale, day.average_equity_loss, 3, 3)
                            )),
                    ),
            );
        }
        left_col = left_col.child(table);
    }

    // 2. Similar Positions Practice History Table
    if !state.similar_rounds.is_empty() {
        let mut table = div()
            .id("history-similar-table")
            .test_support()
            .v_flex()
            .rounded(px(10.0))
            .border_1()
            .border_color(palette.border)
            .bg(palette.surface)
            .overflow_hidden()
            .child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .bg(palette.surface_alt)
                    .border_b_1()
                    .border_color(palette.border)
                    .text_panel()
                    .text_color(palette.text)
                    .child(match locale {
                        neondojo_i18n::Locale::De => "Ähnliche Stellungen",
                        neondojo_i18n::Locale::En => "Similar Positions Practice",
                    }),
            );

        for (idx, round) in state.similar_rounds.iter().enumerate() {
            let rid = round.id;
            let active = state.selected == Some(SelectedHistoryEntry::SimilarRound(rid));
            let on_sel = cb.on_select.clone();
            let hover_bg = palette.surface_alt;
            table = table.child(
                div()
                    .id(SharedString::from(format!("history-similar-row-{idx}")))
                    .test_support()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .px(px(12.0))
                    .py(px(7.0))
                    .border_b_1()
                    .border_color(palette.border)
                    .bg(if active {
                        palette.selected
                    } else {
                        palette.surface
                    })
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover_bg))
                    .on_click(move |_, window, cx| {
                        on_sel(Some(SelectedHistoryEntry::SimilarRound(rid)), window, cx)
                    })
                    .text_table()
                    .tabular_nums()
                    .child(
                        div()
                            .flex_1()
                            .text_color(palette.text)
                            .child(format_datetime_short_local(locale, round.completed_at)),
                    )
                    .child(div().text_color(palette.muted).child(format!(
                        "{}/{} · {} err",
                        round.scored_moves, round.total_positions, round.missed_moves
                    )))
                    .child(
                        div()
                            .w(px(90.0))
                            .text_right()
                            .text_color(palette.text)
                            .child(format!(
                                "Ø {}",
                                format_decimal(locale, round.average_equity_loss, 3, 3)
                            )),
                    ),
            );
        }

        if state.similar_next_cursor.is_some() {
            let on_more = cb.on_load_more_similar.clone();
            table = table.child(
                div().p(px(8.0)).h_flex().justify_center().child(
                    ActionButton::new("history-similar-load-more")
                        .label(I18n::t(locale, "study", "more"))
                        .disabled(state.loading_more_similar)
                        .palette(palette)
                        .on_click(move |_, window, cx| on_more(window, cx)),
                ),
            );
        }
        left_col = left_col.child(table);
    }

    // 3. Transfer Practice History Table
    if !state.transfer_rounds.is_empty() {
        let mut table = div()
            .id("history-transfer-table")
            .test_support()
            .v_flex()
            .rounded(px(10.0))
            .border_1()
            .border_color(palette.border)
            .bg(palette.surface)
            .overflow_hidden()
            .child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .bg(palette.surface_alt)
                    .border_b_1()
                    .border_color(palette.border)
                    .text_panel()
                    .text_color(palette.text)
                    .child(I18n::t(locale, "card-practice", "transfer")),
            );
        for (idx, tr) in state.transfer_rounds.iter().enumerate() {
            let tid = tr.id;
            let active = state.selected == Some(SelectedHistoryEntry::TransferRound(tid));
            let on_sel = cb.on_select.clone();
            let hover_bg = palette.surface_alt;
            table = table.child(
                div()
                    .id(SharedString::from(format!("history-transfer-row-{idx}")))
                    .test_support()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .px(px(12.0))
                    .py(px(7.0))
                    .border_b_1()
                    .border_color(palette.border)
                    .bg(if active {
                        palette.selected
                    } else {
                        palette.surface
                    })
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover_bg))
                    .on_click(move |_, window, cx| {
                        on_sel(Some(SelectedHistoryEntry::TransferRound(tid)), window, cx)
                    })
                    .text_table()
                    .tabular_nums()
                    .child(div().flex_1().text_color(palette.text).child(format!(
                        "{} · {}",
                        tr.scope.player,
                        game_plan_label(locale, tr.scope.game_plan)
                    )))
                    .child(
                        div()
                            .text_color(palette.muted)
                            .child(format!("{}/{}", tr.correct, tr.scored)),
                    ),
            );
        }
        left_col = left_col.child(table);
    }

    // Right column: Drill-down detail panel
    let detail_panel = state
        .selected
        .as_ref()
        .map(|sel| render_history_detail_panel(state, sel, palette, cb.on_select.clone(), cx));

    root.child(
        div()
            .h_flex()
            .items_start()
            .gap(px(16.0))
            .w_full()
            .child(left_col)
            .when_some(detail_panel, |this, panel| this.child(panel)),
    )
}

fn render_loss_trend_chart(points: Vec<f64>, palette: NeonPalette, cx: &App) -> impl IntoElement {
    let locale = current_locale(cx);
    let max_val = points.iter().copied().fold(0.05_f64, f64::max);
    let accent = palette.accent;
    let border = palette.border;

    div()
        .id("history-loss-trend-chart")
        .test_support()
        .v_flex()
        .gap(px(6.0))
        .p(px(12.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .child(
            div()
                .h_flex()
                .justify_between()
                .child(
                    div()
                        .text_label()
                        .text_color(palette.text)
                        .child(match locale {
                            neondojo_i18n::Locale::De => "Verlauf Ø Equity-Verlust",
                            neondojo_i18n::Locale::En => "Average Equity Loss Trend",
                        }),
                )
                .child(
                    div()
                        .text_micro()
                        .tabular_nums()
                        .text_color(palette.muted)
                        .child(format!("max {}", format_decimal(locale, max_val, 3, 3))),
                ),
        )
        .child(
            div().h(px(110.0)).w_full().child(
                canvas(
                    move |bounds, _window, _cx| (bounds, points.clone()),
                    move |_bounds, (bounds, pts), window, _cx| {
                        if pts.len() < 2 {
                            return;
                        }
                        let pad = 10.0_f32;
                        let w = (f32::from(bounds.size.width) - pad * 2.0).max(1.0);
                        let h = (f32::from(bounds.size.height) - pad * 2.0).max(1.0);
                        let ox = f32::from(bounds.origin.x) + pad;
                        let oy = f32::from(bounds.origin.y) + pad;

                        // Baseline rule
                        window.paint_quad(quad(
                            gpui_kit::Bounds::new(point(px(ox), px(oy + h)), size(px(w), px(1.0))),
                            gpui_kit::Corners::default(),
                            border,
                            gpui_kit::Edges::default(),
                            border,
                            gpui_kit::BorderStyle::Solid,
                        ));

                        let mut builder = PathBuilder::stroke(px(2.0));
                        let denom = (pts.len() - 1) as f32;
                        let mut coords = Vec::with_capacity(pts.len());
                        for (i, &val) in pts.iter().enumerate() {
                            let x = ox + (i as f32 / denom) * w;
                            let norm = (val / max_val).clamp(0.0, 1.0) as f32;
                            let y = oy + h * (1.0 - norm);
                            let pt = point(px(x), px(y));
                            coords.push(pt);
                            if i == 0 {
                                builder.move_to(pt);
                            } else {
                                builder.line_to(pt);
                            }
                        }
                        if let Ok(path) = builder.build() {
                            window.paint_path(path, accent);
                        }
                        // Data point dots
                        for pt in coords {
                            let dot_bounds = gpui_kit::Bounds::new(
                                point(pt.x - px(3.0), pt.y - px(3.0)),
                                size(px(6.0), px(6.0)),
                            );
                            window.paint_quad(fill(dot_bounds, accent).corner_radii(px(3.0)));
                        }
                        let _ = PathStyle::Stroke(StrokeOptions::default());
                    },
                )
                .size_full(),
            ),
        )
}

fn render_history_detail_panel(
    state: &PracticeHistoryState,
    selected: &SelectedHistoryEntry,
    palette: NeonPalette,
    on_select: UiCallback<Option<SelectedHistoryEntry>>,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let mut card = div()
        .id("study-history-detail-pane")
        .test_support()
        .v_flex()
        .gap(px(10.0))
        .w(px(340.0))
        .flex_shrink_0()
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
                    "play-mode",
                    "details",
                )))
                .child(
                    ActionButton::new("history-detail-close")
                        .icon(ActionIconName::Close)
                        .variant(crate::ui::action_button::ActionButtonVariant::Ghost)
                        .dense_nav()
                        .palette(palette)
                        .on_click(move |_, window, cx| on_select(None, window, cx)),
                ),
        );

    match selected {
        SelectedHistoryEntry::MistakeDay(idx) => {
            if let Some(day) = state.mistake_days.get(*idx) {
                let plan_txt = game_plan_from_str(&day.game_plan)
                    .map(|p| game_plan_label(locale, p))
                    .unwrap_or(&day.game_plan);
                let kind_txt = mistake_kind_from_str(&day.mistake_kind)
                    .map(|k| mistake_kind_label(locale, k))
                    .unwrap_or(&day.mistake_kind);
                let acc = if day.attempts > 0 {
                    day.best as f64 / day.attempts as f64
                } else {
                    0.0
                };
                card = card
                    .child(
                        div()
                            .text_subject()
                            .text_color(palette.text)
                            .child(format!("{plan_txt} · {kind_txt}")),
                    )
                    .child(detail_kv(
                        I18n::t(locale, "study", "date"),
                        format_naive_date_medium(locale, day.day),
                        palette,
                    ))
                    .child(detail_kv(
                        I18n::t(locale, "recurring-mistakes", "player"),
                        day.player.clone(),
                        palette,
                    ))
                    .child(detail_kv(
                        I18n::t(locale, "play-mode", "attempts"),
                        format_int(locale, day.attempts),
                        palette,
                    ))
                    .child(detail_kv(
                        I18n::t(locale, "recurring-mistakes", "best"),
                        format!(
                            "{} ({})",
                            format_int(locale, day.best),
                            format_percent(locale, acc, 1)
                        ),
                        palette,
                    ))
                    .child(detail_kv(
                        I18n::t(locale, "recurring-mistakes", "loss"),
                        format_decimal(locale, day.average_equity_loss, 3, 3),
                        palette,
                    ));
            }
        }
        SelectedHistoryEntry::SimilarRound(id) => {
            if let Some(round) = state.similar_rounds.iter().find(|r| r.id == *id) {
                card = card
                    .child(detail_kv(
                        I18n::t(locale, "study", "date"),
                        format_datetime_short_local(locale, round.completed_at),
                        palette,
                    ))
                    .child(detail_kv(
                        "Scored / Total",
                        format!("{} / {}", round.scored_moves, round.total_positions),
                        palette,
                    ))
                    .child(detail_kv("Missed", round.missed_moves.to_string(), palette))
                    .child(detail_kv(
                        I18n::t(locale, "recurring-mistakes", "totalLoss"),
                        format_decimal(locale, round.total_equity_loss, 3, 3),
                        palette,
                    ))
                    .child(detail_kv(
                        "Average loss",
                        format_decimal(locale, round.average_equity_loss, 3, 3),
                        palette,
                    ));
            }
        }
        SelectedHistoryEntry::TransferRound(id) => {
            if let Some(tr) = state.transfer_rounds.iter().find(|r| r.id == *id) {
                card = card
                    .child(div().text_subject().text_color(palette.text).child(format!(
                        "{} · {}",
                        tr.scope.player,
                        game_plan_label(locale, tr.scope.game_plan)
                    )))
                    .child(detail_kv(
                        I18n::t(locale, "study", "date"),
                        format_datetime_short_local(locale, tr.completed_at),
                        palette,
                    ))
                    .child(detail_kv(
                        "Score",
                        format!("{} / {} ({} revealed)", tr.correct, tr.scored, tr.revealed),
                        palette,
                    ))
                    .child(detail_kv(
                        "Duration",
                        format!("{}s", tr.duration_seconds),
                        palette,
                    ));
                if !tr.lessons.is_empty() {
                    let mut lessons = div()
                        .v_flex()
                        .gap(px(4.0))
                        .pt(px(6.0))
                        .border_t_1()
                        .border_color(palette.border)
                        .child(div().text_label().text_color(palette.muted).child(I18n::t(
                            locale,
                            "card-practice",
                            "lesson",
                        )));
                    for l in &tr.lessons {
                        lessons = lessons.child(
                            div()
                                .text_meta()
                                .text_color(palette.text)
                                .child(format!("• {l}")),
                        );
                    }
                    card = card.child(lessons);
                }
            }
        }
    }

    card
}

fn detail_kv(label: &str, value: String, palette: NeonPalette) -> impl IntoElement {
    div()
        .h_flex()
        .items_center()
        .justify_between()
        .text_table()
        .tabular_nums()
        .child(div().text_color(palette.muted).child(label.to_string()))
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(palette.text)
                .child(value),
        )
}
