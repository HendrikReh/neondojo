//! Custom GPUI `canvas()` Rating Trend chart (`PlayerRatingTrend.tsx`).
//!
//! Draws:
//! - Decision-weighted 10-match rolling total PR solid curve (`palette.accent`)
//! - Dashed 10-match rolling checker PR mean (`palette.success_text`)
//! - Dashed 10-match rolling cube PR mean (`palette.bad_text`)
//! - Per-match PR scatter markers, gridlines, Y-axis tick labels, and active crosshair
//! - Interactive hover (`on_mouse_move`), click (`on_mouse_down`), and keyboard
//!   (`Left` / `Right` / `Home` / `End` / `Escape`) point selection with a
//!   tabular-numeral tooltip card styled with [`NeonPalette`] tokens.

use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt, tabular_nums_features};
use gpui_kit::{
    App, BorderStyle, Bounds, Context, Corners, FocusHandle, Focusable, Font, FontWeight, Hsla,
    InteractiveElement, IntoElement, KeyDownEvent, LineCap, LineJoin, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement, PathBuilder, PathStyle, Pixels, Point, Render, SharedString,
    StrokeOptions, Styled, TestSupportExt, TextAlign, TextRun, Window, base::StyledExt, canvas,
    div, fill, point, prelude::FluentBuilder, px, quad, size,
};
use neondojo::player_directory::PlayerRatingPoint;
use neondojo_i18n::{I18n, Locale, Param, format_int, format_naive_date_medium, format_pr};
use std::sync::{Arc, Mutex};

/// Rolling window size in matches matching `PlayerRatingTrend.tsx`.
pub const RATING_WINDOW: usize = 10;

/// Computes the decision-weighted rolling average PR over up to `window` recent matches.
/// Falls back to an unweighted mean when total decisions in the window is `<= 0`.
pub fn rolling_ratings(points: &[PlayerRatingPoint], window: usize) -> Vec<f64> {
    let win = window.max(1);
    let mut out = Vec::with_capacity(points.len());
    for i in 0..points.len() {
        let start = i.saturating_sub(win - 1);
        let slice = &points[start..=i];
        let total_decisions: i64 = slice.iter().map(|pt| pt.decisions.max(0)).sum();
        let avg = if total_decisions > 0 {
            let weighted: f64 = slice
                .iter()
                .map(|pt| pt.value * (pt.decisions.max(0) as f64))
                .sum();
            weighted / (total_decisions as f64)
        } else {
            let sum: f64 = slice.iter().map(|pt| pt.value).sum();
            sum / (slice.len() as f64)
        };
        out.push(avg);
    }
    out
}

/// Computes the unweighted rolling mean of optional values over up to `window` recent matches.
pub fn rolling_mean(values: &[Option<f64>], window: usize) -> Vec<Option<f64>> {
    let win = window.max(1);
    let mut out = Vec::with_capacity(values.len());
    for i in 0..values.len() {
        let start = i.saturating_sub(win - 1);
        let mut sum = 0.0;
        let mut count = 0usize;
        for v in values[start..=i].iter().flatten() {
            sum += *v;
            count += 1;
        }
        out.push((count > 0).then(|| sum / (count as f64)));
    }
    out
}

/// Pure geometric projection for the rating trend chart, independent of GPUI rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrendChartGeometry {
    pub width: f32,
    pub height: f32,
    pub pad_left: f32,
    pub pad_right: f32,
    pub pad_top: f32,
    pub pad_bottom: f32,
    pub count: usize,
    pub y_max: f64,
}

impl TrendChartGeometry {
    pub const PAD_LEFT: f32 = 42.0;
    pub const PAD_RIGHT: f32 = 16.0;
    pub const PAD_TOP: f32 = 12.0;
    pub const PAD_BOTTOM: f32 = 22.0;

    /// Computes a clean 5-unit rounded ceiling `max(5.0, ceil(max_val / 5.0) * 5.0)`
    /// across raw values, rolling averages, checker means, and cube means.
    pub fn compute_y_max(
        points: &[PlayerRatingPoint],
        averages: &[f64],
        checker: &[Option<f64>],
        cube: &[Option<f64>],
    ) -> f64 {
        let mut peak = 1.0_f64;
        for pt in points {
            if pt.value.is_finite() {
                peak = peak.max(pt.value);
            }
        }
        for &v in averages {
            if v.is_finite() {
                peak = peak.max(v);
            }
        }
        for v in checker.iter().chain(cube.iter()).flatten() {
            if v.is_finite() {
                peak = peak.max(*v);
            }
        }
        ((peak / 5.0).ceil() * 5.0).max(5.0)
    }

    pub fn new(width: f32, height: f32, count: usize, y_max: f64) -> Self {
        Self {
            width: width.max(Self::PAD_LEFT + Self::PAD_RIGHT + 10.0),
            height: height.max(Self::PAD_TOP + Self::PAD_BOTTOM + 10.0),
            pad_left: Self::PAD_LEFT,
            pad_right: Self::PAD_RIGHT,
            pad_top: Self::PAD_TOP,
            pad_bottom: Self::PAD_BOTTOM,
            count,
            y_max: y_max.max(5.0),
        }
    }

    pub fn plot_width(&self) -> f32 {
        (self.width - self.pad_left - self.pad_right).max(1.0)
    }

    pub fn plot_height(&self) -> f32 {
        (self.height - self.pad_top - self.pad_bottom).max(1.0)
    }

    /// Maps point index `0..count` into local X pixel coordinate.
    pub fn x_at(&self, index: usize) -> f32 {
        if self.count <= 1 {
            return self.pad_left + self.plot_width() / 2.0;
        }
        let frac = (index.min(self.count - 1) as f32) / ((self.count - 1) as f32);
        self.pad_left + frac * self.plot_width()
    }

    /// Maps PR value `0.0..=y_max` into local Y pixel coordinate (0 at bottom, y_max at top).
    pub fn y_at(&self, value: f64) -> f32 {
        let clamped = value.clamp(0.0, self.y_max);
        let frac = (clamped / self.y_max) as f32;
        self.pad_top + (1.0 - frac) * self.plot_height()
    }

    /// Resolves the nearest point index for a local X coordinate inside the chart.
    pub fn nearest_point_index(&self, local_x: f32) -> Option<usize> {
        if self.count == 0 {
            return None;
        }
        if self.count == 1 {
            return Some(0);
        }
        let rel = ((local_x - self.pad_left) / self.plot_width()).clamp(0.0, 1.0);
        let idx = (rel * ((self.count - 1) as f32)).round() as usize;
        Some(idx.min(self.count - 1))
    }
}

/// Splits a polyline into `(start, end)` dash segments along cumulative arc length.
pub fn dashed_polyline_segments(
    pts: &[(f32, f32)],
    dash_len: f32,
    gap_len: f32,
) -> Vec<((f32, f32), (f32, f32))> {
    if pts.len() < 2 || dash_len <= 0.0 {
        return Vec::new();
    }
    let pattern = dash_len + gap_len.max(0.0);
    let mut out = Vec::new();
    let mut phase = 0.0_f32;

    for pair in pts.windows(2) {
        let (x0, y0) = pair[0];
        let (x1, y1) = pair[1];
        let dx = x1 - x0;
        let dy = y1 - y0;
        let seg_len = dx.hypot(dy);
        if seg_len <= 1e-4 {
            continue;
        }
        let ux = dx / seg_len;
        let uy = dy / seg_len;
        let mut cursor = 0.0_f32;

        while cursor < seg_len {
            let in_dash = phase < dash_len;
            let remaining_in_phase = if in_dash {
                dash_len - phase
            } else {
                pattern - phase
            };
            let step = remaining_in_phase.min(seg_len - cursor);
            if in_dash && step > 1e-3 {
                let a = (x0 + ux * cursor, y0 + uy * cursor);
                let b = (x0 + ux * (cursor + step), y0 + uy * (cursor + step));
                out.push((a, b));
            }
            cursor += step;
            phase = (phase + step) % pattern;
        }
    }
    out
}

/// Computes next selected point index for keyboard navigation (`left`, `right`, `home`, `end`, `escape`).
pub fn step_selection_index(current: Option<usize>, count: usize, key: &str) -> Option<usize> {
    if count == 0 {
        return None;
    }
    match key {
        "left" => Some(match current {
            Some(i) => i.saturating_sub(1),
            None => count - 1,
        }),
        "right" => Some(match current {
            Some(i) => (i + 1).min(count - 1),
            None => 0,
        }),
        "home" => Some(0),
        "end" => Some(count - 1),
        "escape" => None,
        _ => current,
    }
}

/// Interactive GPUI `canvas()` chart showing rolling PR trend (total, checker, cube)
/// with mouse hover and keyboard (`Left`/`Right`) point inspection.
pub struct PlayerRatingTrendView {
    id: SharedString,
    profile_label: String,
    points: Vec<PlayerRatingPoint>,
    averages: Vec<f64>,
    checker_averages: Vec<Option<f64>>,
    cube_averages: Vec<Option<f64>>,
    y_max: f64,
    selected_index: Option<usize>,
    hovered_index: Option<usize>,
    focus_handle: FocusHandle,
    last_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
}

impl PlayerRatingTrendView {
    pub fn new(
        id: impl Into<SharedString>,
        profile_label: String,
        points: Vec<PlayerRatingPoint>,
        cx: &mut Context<Self>,
    ) -> Self {
        let averages = rolling_ratings(&points, RATING_WINDOW);
        let checker_raw: Vec<Option<f64>> = points.iter().map(|p| p.checker_value).collect();
        let cube_raw: Vec<Option<f64>> = points.iter().map(|p| p.cube_value).collect();
        let checker_averages = rolling_mean(&checker_raw, RATING_WINDOW);
        let cube_averages = rolling_mean(&cube_raw, RATING_WINDOW);
        let y_max = TrendChartGeometry::compute_y_max(
            &points,
            &averages,
            &checker_averages,
            &cube_averages,
        );

        Self {
            id: id.into(),
            profile_label,
            points,
            averages,
            checker_averages,
            cube_averages,
            y_max,
            selected_index: None,
            hovered_index: None,
            focus_handle: cx.focus_handle(),
            last_bounds: Arc::new(Mutex::new(None)),
        }
    }

    pub fn active_index(&self) -> Option<usize> {
        self.hovered_index.or(self.selected_index)
    }

    pub fn set_selected_index(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        self.selected_index = index.map(|i| i.min(self.points.len().saturating_sub(1)));
        cx.notify();
    }

    pub fn handle_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let normalized = key.to_ascii_lowercase();
        let mapped = match normalized.as_str() {
            "left" | "arrowleft" => "left",
            "right" | "arrowright" => "right",
            "home" => "home",
            "end" => "end",
            "escape" => "escape",
            _ => return false,
        };
        let base = self.selected_index.or(self.hovered_index);
        self.hovered_index = None;
        self.selected_index = step_selection_index(base, self.points.len(), mapped);
        cx.notify();
        true
    }

    fn resolve_mouse_index(&self, position: Point<Pixels>) -> Option<usize> {
        let bounds = (*self.last_bounds.lock().ok()?)?;
        if !bounds.contains(&position) {
            return None;
        }
        let geom = TrendChartGeometry::new(
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            self.points.len(),
            self.y_max,
        );
        let local_x = f32::from(position.x - bounds.origin.x);
        geom.nearest_point_index(local_x)
    }

    fn render_tooltip(&self, p: NeonPalette, locale: Locale) -> impl IntoElement {
        let active = self
            .active_index()
            .or_else(|| (!self.points.is_empty()).then_some(self.points.len() - 1));
        let Some(idx) = active else {
            return div().into_any_element();
        };
        let pt = &self.points[idx];
        let avg = self.averages[idx];
        let chk_avg = self.checker_averages[idx];
        let cube_avg = self.cube_averages[idx];
        let is_pinned = self.active_index().is_some();

        let metric_pill = |label: SharedString, value: String, color: Hsla| {
            div()
                .h_flex()
                .items_center()
                .gap(px(5.0))
                .px(px(8.0))
                .py(px(3.0))
                .rounded(px(4.0))
                .bg(p.surface)
                .border_1()
                .border_color(p.border)
                .child(div().text_meta().text_color(p.muted).child(label))
                .child(
                    div()
                        .text_label()
                        .tabular_nums()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color)
                        .child(value),
                )
        };

        let mut row = div()
            .id(SharedString::from(format!("{}-tooltip", self.id)))
            .test_support()
            .h_flex()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            .flex_wrap()
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(if is_pinned { p.accent } else { p.border })
            .bg(p.surface_alt)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_label()
                            .tabular_nums()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text)
                            .child(format!(
                                "#{} · {}",
                                idx + 1,
                                format_naive_date_medium(locale, pt.date)
                            )),
                    )
                    .child(
                        div()
                            .text_meta()
                            .tabular_nums()
                            .text_color(p.muted)
                            .child(format!("{} dec.", format_int(locale, pt.decisions))),
                    ),
            );

        let mut metrics = div().h_flex().items_center().gap(px(6.0)).flex_wrap();
        metrics = metrics
            .child(metric_pill(
                I18n::t(locale, "player-directory", "pr").into(),
                format_pr(locale, pt.value),
                p.text,
            ))
            .child(metric_pill(
                I18n::t(locale, "player-directory", "prTrendTotal").into(),
                format_pr(locale, avg),
                p.accent,
            ));

        if let Some(c) = chk_avg.or(pt.checker_value) {
            metrics = metrics.child(metric_pill(
                I18n::t(locale, "player-directory", "prTrendChecker").into(),
                format_pr(locale, c),
                p.success_text,
            ));
        }
        if let Some(cu) = cube_avg.or(pt.cube_value) {
            metrics = metrics.child(metric_pill(
                I18n::t(locale, "player-directory", "prTrendCube").into(),
                format_pr(locale, cu),
                p.bad_text,
            ));
        }

        row = row.child(metrics);
        row.into_any_element()
    }
}

impl Focusable for PlayerRatingTrendView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[derive(Clone)]
struct ChartPaintState {
    points: Vec<PlayerRatingPoint>,
    averages: Vec<f64>,
    checker_averages: Vec<Option<f64>>,
    cube_averages: Vec<Option<f64>>,
    y_max: f64,
    active_index: Option<usize>,
    palette: NeonPalette,
    locale: Locale,
}

impl Render for PlayerRatingTrendView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let has_checker = self.checker_averages.iter().any(|v| v.is_some());
        let has_cube = self.cube_averages.iter().any(|v| v.is_some());

        let paint_state = ChartPaintState {
            points: self.points.clone(),
            averages: self.averages.clone(),
            checker_averages: self.checker_averages.clone(),
            cube_averages: self.cube_averages.clone(),
            y_max: self.y_max,
            active_index: self.active_index(),
            palette: p,
            locale,
        };
        let bounds_slot = Arc::clone(&self.last_bounds);

        let chart_canvas = canvas(
            move |bounds, _window, _cx| {
                if let Ok(mut guard) = bounds_slot.lock() {
                    *guard = Some(bounds);
                }
                paint_state
            },
            move |bounds, state, window, cx| {
                paint_rating_trend_canvas(bounds, &state, window, cx);
            },
        )
        .size_full();

        let entity = cx.entity().clone();
        let ent_move = entity.clone();
        let ent_down = entity.clone();
        let ent_key = entity;
        let focus = self.focus_handle.clone();

        let legend_item = |label: SharedString, color: Hsla, dashed: bool| {
            let swatch = if dashed {
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(div().w(px(5.0)).h(px(2.0)).bg(color))
                    .child(div().w(px(5.0)).h(px(2.0)).bg(color))
                    .into_any_element()
            } else {
                div()
                    .w(px(14.0))
                    .h(px(2.5))
                    .rounded(px(1.0))
                    .bg(color)
                    .into_any_element()
            };
            div()
                .h_flex()
                .items_center()
                .gap(px(6.0))
                .child(swatch)
                .child(div().text_meta().text_color(p.muted).child(label))
        };

        div()
            .id(self.id.clone())
            .test_support()
            .v_flex()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            // Title + legend bar
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .flex_wrap()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text)
                                    .child(format!(
                                        "{} · {}",
                                        I18n::t(locale, "player-directory", "prTrend"),
                                        self.profile_label
                                    )),
                            )
                            .child(div().text_meta().text_color(p.muted).child(I18n::format(
                                locale,
                                "player-directory",
                                "prTrendHint",
                                &[("window", Param::from(RATING_WINDOW as i64))],
                            ))),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(12.0))
                            .child(legend_item(
                                I18n::t(locale, "player-directory", "prTrendTotal").into(),
                                p.accent,
                                false,
                            ))
                            .when(has_checker, |this| {
                                this.child(legend_item(
                                    I18n::t(locale, "player-directory", "prTrendChecker").into(),
                                    p.success_text,
                                    true,
                                ))
                            })
                            .when(has_cube, |this| {
                                this.child(legend_item(
                                    I18n::t(locale, "player-directory", "prTrendCube").into(),
                                    p.bad_text,
                                    true,
                                ))
                            }),
                    ),
            )
            // Interactive canvas area
            .child(
                div()
                    .id(SharedString::from(format!("{}-canvas", self.id)))
                    .track_focus(&focus)
                    .w_full()
                    .h(px(184.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface_alt)
                    .cursor_pointer()
                    .on_mouse_move(move |event: &MouseMoveEvent, _window, cx| {
                        ent_move.update(cx, |chart, cx| {
                            let next = chart.resolve_mouse_index(event.position);
                            if chart.hovered_index != next {
                                chart.hovered_index = next;
                                cx.notify();
                            }
                        });
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        move |event: &MouseDownEvent, window, cx| {
                            ent_down.update(cx, |chart, cx| {
                                window.focus(&chart.focus_handle, cx);
                                if let Some(idx) = chart.resolve_mouse_index(event.position) {
                                    chart.selected_index = Some(idx);
                                    chart.hovered_index = Some(idx);
                                    cx.notify();
                                }
                            });
                        },
                    )
                    .on_key_down(move |event: &KeyDownEvent, _window, cx| {
                        let key = event.keystroke.key.clone();
                        ent_key.update(cx, |chart, cx| {
                            if chart.handle_key(&key, cx) {
                                cx.stop_propagation();
                            }
                        });
                    })
                    .child(chart_canvas),
            )
            // Tabular-numeral tooltip card
            .child(self.render_tooltip(p, locale))
    }
}

fn paint_rating_trend_canvas(
    bounds: Bounds<Pixels>,
    s: &ChartPaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    if w < 20.0 || h < 20.0 || s.points.is_empty() {
        return;
    }
    let geom = TrendChartGeometry::new(w, h, s.points.len(), s.y_max);
    let p = s.palette;
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let to_pt = |lx: f32, ly: f32| point(px(ox + lx), px(oy + ly));

    // Horizontal gridlines & Y-axis labels (0, y_max/2, y_max)
    let mut grid_color = p.border;
    grid_color.a = 0.7;
    for frac in [0.0_f64, 0.5, 1.0] {
        let val = s.y_max * frac;
        let ly = geom.y_at(val);
        let line_bounds = Bounds {
            origin: to_pt(geom.pad_left, ly),
            size: size(px(geom.plot_width()), px(1.0)),
        };
        window.paint_quad(fill(line_bounds, grid_color));

        let label = format!("{val:.0}");
        paint_axis_text(
            window,
            cx,
            &label,
            to_pt(geom.pad_left - 6.0, ly),
            px(10.0),
            p.muted,
            true,
        );
    }

    // X-axis first/last date labels
    if let (Some(first), Some(last)) = (s.points.first(), s.points.last()) {
        let first_label = format_naive_date_medium(s.locale, first.date);
        let last_label = format_naive_date_medium(s.locale, last.date);
        let label_y = geom.height - 9.0;
        paint_axis_text(
            window,
            cx,
            &first_label,
            to_pt(geom.pad_left, label_y),
            px(10.0),
            p.muted,
            false,
        );
        paint_axis_text(
            window,
            cx,
            &last_label,
            to_pt(geom.width - geom.pad_right, label_y),
            px(10.0),
            p.muted,
            true,
        );
    }

    // Per-match scatter dots
    let mut dot_color = p.muted;
    dot_color.a = 0.45;
    for (idx, pt) in s.points.iter().enumerate() {
        let cx_pt = to_pt(geom.x_at(idx), geom.y_at(pt.value));
        paint_dot(window, cx_pt, px(2.6), dot_color, None);
    }

    // Dashed checker PR rolling mean
    paint_dashed_optional_series(
        window,
        &geom,
        ox,
        oy,
        &s.checker_averages,
        p.success_text,
        1.75,
    );
    // Dashed cube PR rolling mean
    paint_dashed_optional_series(window, &geom, ox, oy, &s.cube_averages, p.bad_text, 1.75);

    // Solid decision-weighted 10-match rolling total PR curve
    if s.averages.len() >= 2 {
        let mut builder =
            PathBuilder::stroke(px(2.25)).with_style(PathStyle::Stroke(StrokeOptions {
                line_width: 2.25,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                miter_limit: 4.0,
            }));
        for (idx, &avg) in s.averages.iter().enumerate() {
            let pt = to_pt(geom.x_at(idx), geom.y_at(avg));
            if idx == 0 {
                builder.move_to(pt);
            } else {
                builder.line_to(pt);
            }
        }
        if let Ok(path) = builder.build() {
            window.paint_path(path, p.accent);
        }
    }

    // Active hover/selected crosshair + halo markers
    if let Some(idx) = s.active_index
        && idx < s.points.len()
    {
        let lx = geom.x_at(idx);
        let mut guide = p.accent;
        guide.a = 0.35;
        let guide_bounds = Bounds {
            origin: to_pt(lx, geom.pad_top),
            size: size(px(1.0), px(geom.plot_height())),
        };
        window.paint_quad(fill(guide_bounds, guide));

        // Raw match PR highlight
        let raw_pt = to_pt(lx, geom.y_at(s.points[idx].value));
        paint_dot(window, raw_pt, px(3.5), p.text, Some((p.surface, px(1.0))));

        // Checker rolling highlight
        if let Some(c) = s.checker_averages[idx] {
            paint_dot(
                window,
                to_pt(lx, geom.y_at(c)),
                px(3.5),
                p.success_text,
                Some((p.surface, px(1.0))),
            );
        }
        // Cube rolling highlight
        if let Some(cu) = s.cube_averages[idx] {
            paint_dot(
                window,
                to_pt(lx, geom.y_at(cu)),
                px(3.5),
                p.bad_text,
                Some((p.surface, px(1.0))),
            );
        }
        // Total rolling highlight with halo
        let avg_pt = to_pt(lx, geom.y_at(s.averages[idx]));
        let mut halo = p.accent;
        halo.a = 0.25;
        paint_dot(window, avg_pt, px(7.5), halo, None);
        paint_dot(
            window,
            avg_pt,
            px(4.2),
            p.accent,
            Some((p.surface, px(1.5))),
        );
    }
}

fn paint_dashed_optional_series(
    window: &mut Window,
    geom: &TrendChartGeometry,
    ox: f32,
    oy: f32,
    series: &[Option<f64>],
    color: Hsla,
    width: f32,
) {
    let mut run: Vec<(f32, f32)> = Vec::new();
    let flush = |window: &mut Window, run: &mut Vec<(f32, f32)>| {
        if run.len() >= 2 {
            for ((x0, y0), (x1, y1)) in dashed_polyline_segments(run, 6.0, 4.0) {
                let mut builder =
                    PathBuilder::stroke(px(width)).with_style(PathStyle::Stroke(StrokeOptions {
                        line_width: width,
                        line_cap: LineCap::Round,
                        line_join: LineJoin::Round,
                        miter_limit: 4.0,
                    }));
                builder.move_to(point(px(ox + x0), px(oy + y0)));
                builder.line_to(point(px(ox + x1), px(oy + y1)));
                if let Ok(path) = builder.build() {
                    window.paint_path(path, color);
                }
            }
        }
        run.clear();
    };

    for (idx, val) in series.iter().enumerate() {
        if let Some(v) = val {
            run.push((geom.x_at(idx), geom.y_at(*v)));
        } else {
            flush(window, &mut run);
        }
    }
    flush(window, &mut run);
}

fn paint_dot(
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

fn paint_axis_text(
    window: &mut Window,
    cx: &mut App,
    text: &str,
    anchor: Point<Pixels>,
    font_size: Pixels,
    color: Hsla,
    right_align: bool,
) {
    if text.is_empty() {
        return;
    }
    let shared: SharedString = text.to_string().into();
    let font = Font {
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
    let line_h = font_size * 1.15;
    let x = if right_align {
        anchor.x - shaped.width
    } else {
        anchor.x
    };
    let origin = point(x, anchor.y - line_h / 2.0);
    let _ = shaped.paint(origin, line_h, TextAlign::Left, None, window, cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure_app;
    use crate::local_state::LocalUiState;
    use chrono::NaiveDate;
    use gpui_kit::{AppContext, WindowOptions, test::TestWindowExt};
    use uuid::Uuid;

    fn pt(
        day: u32,
        value: f64,
        decisions: i64,
        checker: Option<f64>,
        cube: Option<f64>,
    ) -> PlayerRatingPoint {
        PlayerRatingPoint {
            match_id: Uuid::new_v4(),
            date: NaiveDate::from_ymd_opt(2026, 3, day).unwrap(),
            profile: "gnubg-2ply".into(),
            value,
            decisions,
            checker_value: checker,
            cube_value: cube,
        }
    }

    #[test]
    fn rolling_ratings_weights_by_decision_count_over_window() {
        let pts = vec![
            pt(1, 2.0, 100, Some(1.5), Some(3.0)),
            pt(2, 6.0, 300, Some(5.5), None),
            pt(3, 10.0, 100, Some(9.0), Some(11.0)),
        ];
        // Window of 2:
        // idx 0: 2.0
        // idx 1: (2*100 + 6*300)/400 = 5.0
        // idx 2: (6*300 + 10*100)/400 = 7.0
        let avg2 = rolling_ratings(&pts, 2);
        assert!((avg2[0] - 2.0).abs() < 1e-6);
        assert!((avg2[1] - 5.0).abs() < 1e-6);
        assert!((avg2[2] - 7.0).abs() < 1e-6);

        // Window of 10 covers all 3: (200 + 1800 + 1000) / 500 = 6.0
        let avg10 = rolling_ratings(&pts, RATING_WINDOW);
        assert!((avg10[2] - 6.0).abs() < 1e-6);

        // Zero-decision fallback uses plain mean.
        let zero = vec![pt(1, 4.0, 0, None, None), pt(2, 8.0, 0, None, None)];
        let z_avg = rolling_ratings(&zero, 10);
        assert!((z_avg[1] - 6.0).abs() < 1e-6);
    }

    #[test]
    fn rolling_mean_skips_missing_optional_values() {
        let values = vec![Some(2.0), None, Some(6.0), None];
        let means = rolling_mean(&values, 2);
        assert_eq!(means[0], Some(2.0));
        assert_eq!(means[1], Some(2.0));
        assert_eq!(means[2], Some(6.0));
        assert_eq!(means[3], Some(6.0));

        let all_none = rolling_mean(&[None, None], 10);
        assert_eq!(all_none, vec![None, None]);
    }

    #[test]
    fn geometry_projects_axes_dashes_and_nearest_point_cleanly() {
        let pts = vec![
            pt(1, 3.2, 50, Some(2.5), Some(4.0)),
            pt(2, 11.8, 60, Some(10.0), Some(14.2)),
            pt(3, 5.0, 80, Some(4.8), Some(5.2)),
        ];
        let avg = rolling_ratings(&pts, RATING_WINDOW);
        let chk = rolling_mean(
            &pts.iter().map(|p| p.checker_value).collect::<Vec<_>>(),
            RATING_WINDOW,
        );
        let cub = rolling_mean(
            &pts.iter().map(|p| p.cube_value).collect::<Vec<_>>(),
            RATING_WINDOW,
        );
        let y_max = TrendChartGeometry::compute_y_max(&pts, &avg, &chk, &cub);
        assert_eq!(y_max, 15.0);

        let geom = TrendChartGeometry::new(400.0, 200.0, 3, y_max);
        assert!((geom.x_at(0) - TrendChartGeometry::PAD_LEFT).abs() < 1e-4);
        assert!((geom.x_at(2) - (400.0 - TrendChartGeometry::PAD_RIGHT)).abs() < 1e-4);
        assert!((geom.y_at(y_max) - TrendChartGeometry::PAD_TOP).abs() < 1e-4);
        assert!((geom.y_at(0.0) - (200.0 - TrendChartGeometry::PAD_BOTTOM)).abs() < 1e-4);

        assert_eq!(geom.nearest_point_index(geom.x_at(0)), Some(0));
        assert_eq!(geom.nearest_point_index(geom.x_at(1) + 2.0), Some(1));
        assert_eq!(geom.nearest_point_index(999.0), Some(2));

        let dashes = dashed_polyline_segments(&[(0.0, 0.0), (25.0, 0.0)], 6.0, 4.0);
        assert_eq!(dashes.len(), 3); // [0..6], [10..16], [20..25]
        assert!((dashes[0].1.0 - 6.0).abs() < 1e-3);
        assert!((dashes[1].0.0 - 10.0).abs() < 1e-3);
        assert!((dashes[2].1.0 - 25.0).abs() < 1e-3);
    }

    #[test]
    fn keyboard_navigation_steps_and_clears_selection() {
        assert_eq!(step_selection_index(None, 5, "left"), Some(4));
        assert_eq!(step_selection_index(None, 5, "right"), Some(0));
        assert_eq!(step_selection_index(Some(2), 5, "left"), Some(1));
        assert_eq!(step_selection_index(Some(0), 5, "left"), Some(0));
        assert_eq!(step_selection_index(Some(4), 5, "right"), Some(4));
        assert_eq!(step_selection_index(Some(2), 5, "home"), Some(0));
        assert_eq!(step_selection_index(Some(2), 5, "end"), Some(4));
        assert_eq!(step_selection_index(Some(2), 5, "escape"), None);
    }

    #[gpui_kit::test]
    async fn rating_trend_renders_and_updates_on_keyboard_navigation(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(LocalUiState::default(), false, cx);
        });
        let pts = vec![
            pt(1, 4.5, 100, Some(3.8), Some(6.0)),
            pt(2, 6.2, 120, Some(5.5), Some(7.5)),
            pt(3, 3.9, 90, Some(3.4), Some(5.0)),
        ];
        let (window, view) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, move |_window, cx| {
                    cx.new(|cx| PlayerRatingTrendView::new("test-trend", "2-ply".into(), pts, cx))
                })
            })
            .expect("open trend window");

        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("test-trend").visible());
            assert!(window.find("test-trend-tooltip").visible());

            view.update(cx, |chart, cx| {
                assert!(chart.handle_key("right", cx));
                assert_eq!(chart.active_index(), Some(0));
                assert!(chart.handle_key("right", cx));
                assert_eq!(chart.active_index(), Some(1));
                assert!(chart.handle_key("left", cx));
                assert_eq!(chart.active_index(), Some(0));
                assert!(chart.handle_key("end", cx));
                assert_eq!(chart.active_index(), Some(2));
                assert!(chart.handle_key("escape", cx));
                assert_eq!(chart.active_index(), None);
            });
            window.render_frame(cx);
        })
        .expect("update trend window");
    }
}
