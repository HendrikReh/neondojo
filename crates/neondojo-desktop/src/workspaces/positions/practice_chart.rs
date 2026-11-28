//! Custom GPUI `canvas()` time-series accuracy/equity chart for Similar Position
//! Practice (`SimilarPracticeChart.tsx`).
//!
//! Renders chronological practice rounds on a time-scaled X axis and nice-rounded
//! average-equity-loss Y axis, distinguishes initial vs retry rounds, plots a
//! configurable rolling average (5 / 10 / 20 initial rounds), and supports both
//! mouse hover/click hit-testing and keyboard (`Left` / `Right` / `Home` / `End`)
//! point selection with a live callout tooltip using `NeonPalette` tokens and
//! tabular numerals.

use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt, tabular_nums_features};
use crate::ui::action_button::ActionButton;
use chrono::{DateTime, Utc};
use gpui_kit::{
    App, BorderStyle, Bounds, Context, Corners, FocusHandle, Focusable, Font, FontWeight, Hsla,
    InteractiveElement, IntoElement, KeyDownEvent, LineCap, LineJoin, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement, PathBuilder, PathStyle, Pixels, Point, Render, SharedString,
    StrokeOptions, Styled, TestSupportExt, TextAlign, TextRun, Window, base::StyledExt, canvas,
    div, point, prelude::FluentBuilder, px, quad, size,
};
use neondojo::similar_positions::SimilarPracticeSummary;
use neondojo_i18n::{
    I18n, Locale, Param, format_date_medium_local, format_datetime_medium_local,
    format_datetime_medium_with_seconds, format_decimal, format_int,
};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Supported rolling average window sizes (matching `SimilarPracticeChart.tsx`).
pub const ROLLING_WINDOWS: [usize; 3] = [5, 10, 20];

/// Horizontal gridline fractions from top (`1.0`) to bottom (`0.0`).
pub const Y_TICK_FRACTIONS: [f64; 5] = [1.0, 0.75, 0.5, 0.25, 0.0];

/// Hit-test radius in logical pixels around plotted round markers.
pub const POINT_HIT_RADIUS_PX: f32 = 14.0;

/// Single plotted round inside [`ChartLayout`].
#[derive(Debug, Clone, PartialEq)]
pub struct ChartPoint {
    pub id: Uuid,
    pub index: usize,
    pub completed_at: DateTime<Utc>,
    pub timestamp_ms: i64,
    pub average_equity_loss: f64,
    pub scored_moves: i32,
    pub total_positions: i32,
    pub missed_moves: i32,
    pub is_retry: bool,
    /// Rolling mean across up to `rolling_window` initial rounds (absent for retry rounds).
    pub rolling_average: Option<f64>,
    /// Normalized X position in `[0.0, 1.0]` (`0.5` when all rounds share one timestamp).
    pub norm_x: f32,
    /// Normalized Y position in `[0.0, 1.0]` where `0.0` is top (`axis_max`) and `1.0` is bottom (`0.0`).
    pub norm_y: f32,
}

/// Pure deterministic chart layout derived from newest-first practice summaries.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartLayout {
    pub rolling_window: usize,
    pub axis_max: f64,
    pub points: Vec<ChartPoint>,
    /// Chronological indices into `points` of non-retry rounds (`!is_retry`).
    pub initial_indices: Vec<usize>,
    pub show_trend: bool,
    pub first_timestamp_ms: i64,
    pub last_timestamp_ms: i64,
}

impl ChartLayout {
    /// Computes the full chart geometry from newest-first `items` (as returned by
    /// `list_similar_practice_history`), reversing into chronological order.
    pub fn compute(items: &[SimilarPracticeSummary], rolling_window: usize) -> Option<Self> {
        if items.is_empty() {
            return None;
        }
        let window_size = rolling_window.max(1);
        let rounds: Vec<&SimilarPracticeSummary> = items.iter().rev().collect();
        let first_ts = rounds[0].completed_at.timestamp_millis();
        let last_ts = rounds[rounds.len() - 1].completed_at.timestamp_millis();
        let duration = (last_ts - first_ts).max(0) as f64;

        let max_loss = rounds.iter().fold(0.001_f64, |acc, r| {
            if r.average_equity_loss.is_finite() {
                acc.max(r.average_equity_loss)
            } else {
                acc
            }
        });
        let unit = 10_f64.powf(max_loss.log10().floor());
        let rounded_max = (max_loss / unit).ceil() * unit;
        let axis_max = if rounded_max.is_finite() && rounded_max > 0.0 {
            max_loss.max(rounded_max)
        } else {
            max_loss.max(0.001)
        };

        let mut points = Vec::with_capacity(rounds.len());
        let mut initial_indices = Vec::new();
        let mut initial_losses: Vec<f64> = Vec::new();

        for (index, round) in rounds.into_iter().enumerate() {
            let ts = round.completed_at.timestamp_millis();
            let norm_x = if duration > 0.0 {
                ((ts - first_ts) as f64 / duration).clamp(0.0, 1.0) as f32
            } else {
                0.5
            };
            let loss = if round.average_equity_loss.is_finite() {
                round.average_equity_loss.max(0.0)
            } else {
                0.0
            };
            let norm_y = (1.0 - (loss / axis_max)).clamp(0.0, 1.0) as f32;

            let rolling_average = if !round.is_retry {
                initial_indices.push(index);
                initial_losses.push(loss);
                let start = initial_losses.len().saturating_sub(window_size);
                let slice = &initial_losses[start..];
                let mean = slice.iter().sum::<f64>() / slice.len() as f64;
                Some(mean)
            } else {
                None
            };

            points.push(ChartPoint {
                id: round.id,
                index,
                completed_at: round.completed_at,
                timestamp_ms: ts,
                average_equity_loss: loss,
                scored_moves: round.scored_moves,
                total_positions: round.total_positions,
                missed_moves: round.missed_moves,
                is_retry: round.is_retry,
                rolling_average,
                norm_x,
                norm_y,
            });
        }

        let show_trend = initial_indices.len() > 1;
        Some(Self {
            rolling_window: window_size,
            axis_max,
            points,
            initial_indices,
            show_trend,
            first_timestamp_ms: first_ts,
            last_timestamp_ms: last_ts,
        })
    }

    /// Normalized Y position (`0.0` top, `1.0` bottom) for an arbitrary loss value.
    pub fn norm_y_for_loss(&self, loss: f64) -> f32 {
        (1.0 - (loss / self.axis_max)).clamp(0.0, 1.0) as f32
    }

    /// Formats the Y-axis tick label at `fraction` in `[0.0, 1.0]`.
    pub fn format_y_tick(&self, locale: Locale, fraction: f64) -> String {
        let val = self.axis_max * fraction;
        if self.axis_max >= 1000.0 && fraction > 0.0 {
            format!("{val:.2e}")
        } else if self.axis_max < 0.01 {
            format_decimal(locale, val, 4, 4)
        } else {
            format_decimal(locale, val, 3, 3)
        }
    }

    /// Formats start/end X-axis date ticks (collapsed to 1 tick when equal).
    pub fn date_ticks(&self, locale: Locale) -> Vec<(f32, String)> {
        let Some(first) = self.points.first() else {
            return Vec::new();
        };
        let Some(last) = self.points.last() else {
            return Vec::new();
        };
        let first_day = format_date_medium_local(locale, first.completed_at);
        let last_day = format_date_medium_local(locale, last.completed_at);
        let duration_ms = (self.last_timestamp_ms - self.first_timestamp_ms).max(0);
        let fmt_ts = |dt: DateTime<Utc>| {
            if first_day == last_day {
                if duration_ms < 60_000 {
                    format_datetime_medium_with_seconds(locale, dt)
                } else {
                    format_datetime_medium_local(locale, dt)
                }
            } else {
                format_date_medium_local(locale, dt)
            }
        };
        let a = fmt_ts(first.completed_at);
        let b = fmt_ts(last.completed_at);
        if a == b {
            vec![(first.norm_x, a)]
        } else {
            vec![(0.0, a), (1.0, b)]
        }
    }

    /// Returns the index of the nearest point within `radius_px` of `pos` inside `plot_bounds`.
    pub fn hit_test_point(
        &self,
        plot_bounds: Bounds<Pixels>,
        pos: Point<Pixels>,
        radius_px: f32,
    ) -> Option<usize> {
        let max_dist_sq = radius_px * radius_px;
        let mut best: Option<(usize, f32)> = None;
        for pt in &self.points {
            let screen = map_norm_point(plot_bounds, pt.norm_x, pt.norm_y);
            let dx = f32::from(pos.x) - f32::from(screen.x);
            let dy = f32::from(pos.y) - f32::from(screen.y);
            let dist_sq = dx * dx + dy * dy;
            if dist_sq <= max_dist_sq && best.is_none_or(|(_, prev)| dist_sq < prev) {
                best = Some((pt.index, dist_sq));
            }
        }
        best.map(|(idx, _)| idx)
    }
}

/// Maps normalized plot coordinates `(nx, ny)` in `[0, 1]` to screen-space pixels.
pub fn map_norm_point(plot_bounds: Bounds<Pixels>, nx: f32, ny: f32) -> Point<Pixels> {
    point(
        px(f32::from(plot_bounds.origin.x) + nx * f32::from(plot_bounds.size.width)),
        px(f32::from(plot_bounds.origin.y) + ny * f32::from(plot_bounds.size.height)),
    )
}

/// Computes the inner plot rectangle inside the full canvas bounds (leaving margins
/// for Y-axis ticks on the left and date ticks at the bottom).
pub fn inner_plot_bounds(canvas_bounds: Bounds<Pixels>) -> Bounds<Pixels> {
    let left_pad = 64.0_f32;
    let right_pad = 16.0_f32;
    let top_pad = 14.0_f32;
    let bottom_pad = 26.0_f32;
    let w = (f32::from(canvas_bounds.size.width) - left_pad - right_pad).max(20.0);
    let h = (f32::from(canvas_bounds.size.height) - top_pad - bottom_pad).max(20.0);
    Bounds {
        origin: point(
            px(f32::from(canvas_bounds.origin.x) + left_pad),
            px(f32::from(canvas_bounds.origin.y) + top_pad),
        ),
        size: size(px(w), px(h)),
    }
}

/// Keyboard navigation step inside [`SimilarPracticeChartView`].
pub fn step_selected_index(len: usize, current: usize, key: &str) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let idx = current.min(len - 1);
    match key {
        "left" => Some(idx.saturating_sub(1)),
        "right" => Some((idx + 1).min(len - 1)),
        "home" => Some(0),
        "end" => Some(len - 1),
        _ => None,
    }
}

/// Stateful GPUI `canvas()` chart view for Similar Position Practice history.
pub struct SimilarPracticeChartView {
    items: Vec<SimilarPracticeSummary>,
    rolling_window: usize,
    selected_id: Option<Uuid>,
    hovered_id: Option<Uuid>,
    focus_handle: FocusHandle,
    last_plot_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
}

impl SimilarPracticeChartView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            items: Vec::new(),
            rolling_window: ROLLING_WINDOWS[0],
            selected_id: None,
            hovered_id: None,
            focus_handle: cx.focus_handle(),
            last_plot_bounds: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_items(&mut self, items: Vec<SimilarPracticeSummary>, cx: &mut Context<Self>) {
        self.items = items;
        if let Some(sel) = self.selected_id
            && !self.items.iter().any(|r| r.id == sel)
        {
            self.selected_id = None;
        }
        if let Some(hov) = self.hovered_id
            && !self.items.iter().any(|r| r.id == hov)
        {
            self.hovered_id = None;
        }
        cx.notify();
    }

    pub fn set_rolling_window(&mut self, window_size: usize, cx: &mut Context<Self>) {
        if ROLLING_WINDOWS.contains(&window_size) && self.rolling_window != window_size {
            self.rolling_window = window_size;
            cx.notify();
        }
    }

    pub fn rolling_window(&self) -> usize {
        self.rolling_window
    }

    pub fn selected_round_id(&self) -> Option<Uuid> {
        let layout = self.layout()?;
        self.active_point(&layout).map(|p| p.id)
    }

    pub fn layout(&self) -> Option<ChartLayout> {
        ChartLayout::compute(&self.items, self.rolling_window)
    }

    fn active_point<'a>(&self, layout: &'a ChartLayout) -> Option<&'a ChartPoint> {
        if let Some(hov) = self.hovered_id
            && let Some(pt) = layout.points.iter().find(|p| p.id == hov)
        {
            return Some(pt);
        }
        if let Some(sel) = self.selected_id
            && let Some(pt) = layout.points.iter().find(|p| p.id == sel)
        {
            return Some(pt);
        }
        layout.points.last()
    }

    pub fn select_by_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let Some(layout) = self.layout() else {
            return false;
        };
        let current = self
            .selected_id
            .and_then(|id| layout.points.iter().position(|p| p.id == id))
            .unwrap_or(layout.points.len().saturating_sub(1));
        let Some(next) = step_selected_index(layout.points.len(), current, key) else {
            return false;
        };
        self.selected_id = Some(layout.points[next].id);
        self.hovered_id = None;
        cx.notify();
        true
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.select_by_key(event.keystroke.key.as_str(), cx) {
            cx.stop_propagation();
        }
    }

    fn handle_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.last_plot_bounds.lock().ok().and_then(|g| *g) else {
            return;
        };
        let Some(layout) = self.layout() else {
            return;
        };
        let next_hover = layout
            .hit_test_point(bounds, event.position, POINT_HIT_RADIUS_PX)
            .map(|idx| layout.points[idx].id);
        if self.hovered_id != next_hover {
            self.hovered_id = next_hover;
            cx.notify();
        }
    }

    fn handle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        window.focus(&self.focus_handle, cx);
        let Some(bounds) = self.last_plot_bounds.lock().ok().and_then(|g| *g) else {
            return;
        };
        let Some(layout) = self.layout() else {
            return;
        };
        if let Some(idx) = layout.hit_test_point(bounds, event.position, POINT_HIT_RADIUS_PX) {
            self.selected_id = Some(layout.points[idx].id);
            self.hovered_id = Some(layout.points[idx].id);
            cx.notify();
        }
    }
}

impl Focusable for SimilarPracticeChartView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

struct ChartPaintSnapshot {
    layout: ChartLayout,
    palette: NeonPalette,
    locale: Locale,
    active_id: Option<Uuid>,
    last_plot_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
}

impl Render for SimilarPracticeChartView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(layout) = self.layout() else {
            return div().id("similar-practice-chart-empty").into_any_element();
        };
        let locale = current_locale(cx);
        let p = NeonTheme::current_palette(cx);
        let active_pt = self.active_point(&layout).cloned();
        let active_id = active_pt.as_ref().map(|pt| pt.id);
        let trend_label = I18n::format(
            locale,
            "position-browser",
            "practiceRollingAverage",
            &[("count", Param::from(layout.rolling_window as i64))],
        );

        let ent = cx.entity();
        let mut window_controls = div().h_flex().items_center().gap(px(4.0));
        if layout.show_trend {
            window_controls = window_controls.child(
                div()
                    .text_meta()
                    .text_color(p.muted)
                    .mr(px(4.0))
                    .child(I18n::t(locale, "position-browser", "practiceRollingWindow")),
            );
            for &win_size in &ROLLING_WINDOWS {
                let ent_btn = ent.clone();
                let lbl = I18n::format(
                    locale,
                    "position-browser",
                    "practiceRollingWindowRounds",
                    &[("count", Param::from(win_size as i64))],
                );
                window_controls = window_controls.child(
                    ActionButton::new(SharedString::from(format!(
                        "similar-chart-window-{win_size}"
                    )))
                    .label(lbl)
                    .active(self.rolling_window == win_size)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_btn.update(cx, |view, cx| view.set_rolling_window(win_size, cx));
                    }),
                );
            }
        }

        let legend = div()
            .h_flex()
            .flex_wrap()
            .items_center()
            .gap(px(14.0))
            .text_meta()
            .text_color(p.muted)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().w(px(9.0)).h(px(9.0)).rounded_full().bg(p.accent))
                    .child(I18n::t(locale, "position-browser", "practiceRoundInitial")),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .w(px(9.0))
                            .h(px(9.0))
                            .rounded_full()
                            .border_2()
                            .border_color(p.warning_text)
                            .bg(p.surface),
                    )
                    .child(I18n::t(locale, "position-browser", "practiceRoundRetry")),
            )
            .when(layout.show_trend, |this| {
                this.child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .w(px(14.0))
                                .h(px(2.5))
                                .rounded(px(2.0))
                                .bg(p.success_text),
                        )
                        .child(trend_label.clone()),
                )
            });

        let snapshot = ChartPaintSnapshot {
            layout: layout.clone(),
            palette: p,
            locale,
            active_id,
            last_plot_bounds: Arc::clone(&self.last_plot_bounds),
        };

        let chart_canvas = canvas(
            move |bounds, _window, _cx| {
                let plot = inner_plot_bounds(bounds);
                if let Ok(mut guard) = snapshot.last_plot_bounds.lock() {
                    *guard = Some(plot);
                }
                (snapshot, plot)
            },
            move |_bounds, (snapshot, plot), window, cx| {
                paint_practice_chart(&snapshot, plot, window, cx);
            },
        )
        .w_full()
        .h(px(204.0));

        // Tooltip / keyboard selection callout strip with tabular numerals
        let callout = active_pt.map(|pt| {
            let kind = I18n::t(
                locale,
                "position-browser",
                if pt.is_retry {
                    "practiceRoundRetry"
                } else {
                    "practiceRoundInitial"
                },
            );
            let ts = format_datetime_medium_with_seconds(locale, pt.completed_at);
            let loss_str = format_decimal(locale, pt.average_equity_loss, 3, 6);
            let moves_str = format!(
                "{}/{}",
                format_int(locale, i64::from(pt.scored_moves)),
                format_int(locale, i64::from(pt.total_positions))
            );
            let mut summary = format!("{kind} · {ts} · {loss_str} · {moves_str}");
            if layout.show_trend
                && let Some(avg) = pt.rolling_average
            {
                summary.push_str(&format!(
                    " · {} · {}",
                    trend_label,
                    format_decimal(locale, avg, 3, 6)
                ));
            }
            div()
                .id("similar-practice-chart-callout")
                .test_support()
                .h_flex()
                .items_center()
                .justify_between()
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .bg(p.surface_alt)
                .border_1()
                .border_color(p.border)
                .text_label()
                .tabular_nums()
                .text_color(p.text)
                .child(summary)
                .child(div().text_meta().text_color(p.muted).child(format!(
                    "← / → · {} / {}",
                    pt.index + 1,
                    layout.points.len()
                )))
        });

        div()
            .id("similar-practice-chart")
            .test_support()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .v_flex()
            .gap(px(10.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(div().text_panel().text_color(p.text).child(I18n::t(
                        locale,
                        "position-browser",
                        "practiceLossOverTime",
                    )))
                    .child(window_controls),
            )
            .child(legend)
            .child(
                div()
                    .id("similar-practice-chart-canvas")
                    .test_support()
                    .w_full()
                    .h(px(204.0))
                    .cursor_pointer()
                    .on_mouse_move(cx.listener(Self::handle_mouse_move))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_mouse_down))
                    .child(chart_canvas),
            )
            .when_some(callout, |this, el| this.child(el))
            .into_any_element()
    }
}

fn paint_practice_chart(
    snap: &ChartPaintSnapshot,
    plot: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let p = snap.palette;
    let layout = &snap.layout;

    // 1. Subtle plot background
    window.paint_quad(quad(
        plot,
        Corners::all(px(4.0)),
        p.surface_alt,
        px(1.0),
        p.border,
        BorderStyle::Solid,
    ));

    // 2. Horizontal gridlines & Y-axis tabular-numeral labels
    for &fraction in &Y_TICK_FRACTIONS {
        let ny = (1.0 - fraction) as f32;
        let y = f32::from(plot.origin.y) + ny * f32::from(plot.size.height);
        let line_bounds = Bounds {
            origin: point(plot.origin.x, px(y)),
            size: size(plot.size.width, px(1.0)),
        };
        window.paint_quad(quad(
            line_bounds,
            Corners::default(),
            p.border,
            px(0.0),
            Hsla::transparent_black(),
            BorderStyle::Solid,
        ));

        let label = layout.format_y_tick(snap.locale, fraction);
        paint_chart_text(
            window,
            cx,
            &label,
            point(px(f32::from(plot.origin.x) - 8.0), px(y - 6.0)),
            px(11.0),
            FontWeight::NORMAL,
            p.muted,
            TextAlign::Right,
        );
    }

    // 3. Initial-round polyline (subtle muted/accent stroke)
    if layout.initial_indices.len() > 1 {
        let mut stroke_color = p.accent;
        stroke_color.a = 0.55;
        let pts: Vec<Point<Pixels>> = layout
            .initial_indices
            .iter()
            .map(|&idx| {
                let pt = &layout.points[idx];
                map_norm_point(plot, pt.norm_x, pt.norm_y)
            })
            .collect();
        paint_polyline(window, &pts, stroke_color, px(1.75));
    }

    // 4. Rolling average trend polyline (initial rounds only)
    if layout.show_trend {
        let trend_pts: Vec<Point<Pixels>> = layout
            .initial_indices
            .iter()
            .filter_map(|&idx| {
                let pt = &layout.points[idx];
                let avg = pt.rolling_average?;
                Some(map_norm_point(plot, pt.norm_x, layout.norm_y_for_loss(avg)))
            })
            .collect();
        if trend_pts.len() > 1 {
            paint_polyline(window, &trend_pts, p.success_text, px(2.5));
        }
    }

    // 5. Point markers (distinct filled circle for initial vs outlined circle for retry)
    for pt in &layout.points {
        let center = map_norm_point(plot, pt.norm_x, pt.norm_y);
        let is_active = snap.active_id == Some(pt.id);
        if is_active {
            let mut halo = p.accent;
            halo.a = 0.28;
            paint_marker_circle(window, center, px(9.5), halo, Some((p.accent, px(1.5))));
        }
        if pt.is_retry {
            paint_marker_circle(
                window,
                center,
                px(4.5),
                p.surface,
                Some((p.warning_text, px(2.0))),
            );
        } else {
            paint_marker_circle(
                window,
                center,
                px(4.5),
                p.accent,
                Some((p.surface, px(1.2))),
            );
        }
    }

    // 6. X-axis date ticks
    let ticks = layout.date_ticks(snap.locale);
    let label_y = px(f32::from(plot.origin.y) + f32::from(plot.size.height) + 6.0);
    for (i, (nx, text)) in ticks.iter().enumerate() {
        let x = px(f32::from(plot.origin.x) + nx * f32::from(plot.size.width));
        let align = if ticks.len() == 1 {
            TextAlign::Center
        } else if i == 0 {
            TextAlign::Left
        } else {
            TextAlign::Right
        };
        paint_chart_text(
            window,
            cx,
            text,
            point(x, label_y),
            px(11.0),
            FontWeight::NORMAL,
            p.muted,
            align,
        );
    }
}

fn paint_polyline(window: &mut Window, points: &[Point<Pixels>], color: Hsla, width: Pixels) {
    if points.len() < 2 {
        return;
    }
    let mut builder = PathBuilder::stroke(width).with_style(PathStyle::Stroke(StrokeOptions {
        line_width: f32::from(width),
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        miter_limit: 4.0,
    }));
    builder.move_to(points[0]);
    for &pt in &points[1..] {
        builder.line_to(pt);
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

fn paint_marker_circle(
    window: &mut Window,
    center: Point<Pixels>,
    radius: Pixels,
    fill_color: Hsla,
    stroke: Option<(Hsla, Pixels)>,
) {
    let d = radius * 2.0;
    let bounds = Bounds {
        origin: point(center.x - radius, center.y - radius),
        size: size(d, d),
    };
    let (border_color, border_width) = stroke.unwrap_or((Hsla::transparent_black(), px(0.0)));
    window.paint_quad(quad(
        bounds,
        Corners::all(radius),
        fill_color,
        border_width,
        border_color,
        BorderStyle::Solid,
    ));
}

#[allow(clippy::too_many_arguments)]
fn paint_chart_text(
    window: &mut Window,
    cx: &mut App,
    text: &str,
    anchor: Point<Pixels>,
    font_size: Pixels,
    weight: FontWeight,
    color: Hsla,
    align: TextAlign,
) {
    if text.is_empty() {
        return;
    }
    let shared: SharedString = text.to_string().into();
    let font = Font {
        weight,
        features: tabular_nums_features(),
        ..Default::default()
    };
    let run = TextRun {
        len: shared.len(),
        font,
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line(shared, font_size, &[run], None);
    let line_h = font_size * 1.25;
    let x = match align {
        TextAlign::Left => anchor.x,
        TextAlign::Center => anchor.x - shaped.width / 2.0,
        TextAlign::Right => anchor.x - shaped.width,
    };
    let _ = shaped.paint(
        point(x, anchor.y),
        line_h,
        TextAlign::Left,
        None,
        window,
        cx,
    );
}
