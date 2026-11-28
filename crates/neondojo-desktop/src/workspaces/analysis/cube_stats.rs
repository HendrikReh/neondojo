//! Cube and Statistics Inspector sub-views:
//! - **Cube tab**: `crate::review::CubeDecisionDetailsView`, filter bar
//!   (`Inaccuracies only`, Player, Severity, Game scope, `Reset filters`),
//!   clickable cube decision list, and on-demand `Calculate winning chances`.
//! - **Statistics tab**: Per-player cards, Total / Checker / Cube grade matrix
//!   with interactive drill-down popover (jumping to filtered Checker or Cube
//!   review), per-game breakdown table (`Open game`, `needs_cube_rule_reanalysis`),
//!   `MatchAnalysisComparison` side-by-side comparison of 2 catalog entries, and
//!   responsive `≥ 1050px` 2-column layout switch.

use super::state::{
    Action2Cb, ActionCb, ClickCb, CubeFilterState, STATS_TWO_COLUMN_MIN_WIDTH, parse_severity,
};
use crate::i18n::current_locale;
use crate::review::cube_details::{CubeDecisionDetailsView, CubeEquityUnit, CubeEvaluationData};
use crate::review::severity::SeverityStyle;
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::icons::ActionIconName;
use gpui_kit::{
    App, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt, base::StyledExt, div,
    prelude::FluentBuilder, px,
};
use neondojo::{
    analysis_catalog::AnalysisCatalogEntry,
    analysis_statistics::{
        ErrorSeverityCounts, PlayerErrorBreakdown,
        catalog::{CatalogGameStatistics, MatchAnalysisStatistics},
    },
    match_analysis::CubeEquityUnit as BackendCubeUnit,
    model::{MatchGame, MoveSeverity},
    review_decisions::ReviewDecision,
};
use neondojo_i18n::{I18n, Locale, format_decimal, format_pr, localize_decision};

// ── Cube Inspector Tab ───────────────────────────────────────────────────────

#[derive(Clone)]
pub struct CubeCallbacks {
    pub on_select_decision: Action2Cb<String, i32>,
    pub on_toggle_errors_only: ClickCb,
    pub on_cycle_player: ClickCb,
    pub on_cycle_severity: ClickCb,
    pub on_cycle_game: ClickCb,
    pub on_reset_filters: ClickCb,
    pub on_calculate_chances: ActionCb<String>,
}

pub struct CubeViewProps<'a> {
    pub decisions: &'a [ReviewDecision],
    pub selected_id: Option<&'a str>,
    pub filter: &'a CubeFilterState,
    pub games: &'a [MatchGame],
    pub players: [SharedString; 2],
    pub loading: bool,
    pub calculating_chances_for: Option<&'a str>,
    pub override_chances: Option<([f64; 6], String, i32)>,
    pub callbacks: CubeCallbacks,
    pub palette: NeonPalette,
}

/// Converts a backend `ReviewDecision` cube evaluation into `CubeEvaluationData`.
pub fn to_cube_evaluation_data(
    decision: &ReviewDecision,
    override_probs: Option<[f64; 6]>,
) -> Option<CubeEvaluationData> {
    let ev = decision.decision.cube_evaluation.as_ref()?;
    Some(CubeEvaluationData {
        no_double: ev.no_double,
        double_take: ev.double_take,
        double_pass: ev.double_pass,
        unit: match ev.unit {
            BackendCubeUnit::Cubeful => CubeEquityUnit::NormalizedCubeful,
            BackendCubeUnit::Mwc => CubeEquityUnit::MatchWinningChance,
        },
        probabilities: override_probs.or(ev.probabilities),
    })
}

/// Renders the Cube review inspector tab.
pub fn render_cube_inspector(props: CubeViewProps<'_>, cx: &App) -> impl IntoElement {
    let locale = current_locale(cx);
    let p = props.palette;

    let on_err = props.callbacks.on_toggle_errors_only.clone();
    let on_pl = props.callbacks.on_cycle_player.clone();
    let on_sev = props.callbacks.on_cycle_severity.clone();
    let on_game = props.callbacks.on_cycle_game.clone();
    let on_reset = props.callbacks.on_reset_filters.clone();

    let player_lbl = match props.filter.player {
        Some(0) => format!("Player: {}", props.players[0]),
        Some(1) => format!("Player: {}", props.players[1]),
        _ => "Player: All".to_string(),
    };
    let sev_lbl = match props.filter.severity {
        Some(MoveSeverity::Doubtful) => "Severity: ?!",
        Some(MoveSeverity::Bad) => "Severity: ?",
        Some(MoveSeverity::Blunder) => "Severity: ??",
        Some(MoveSeverity::VeryBad) => "Severity: ???",
        _ => "Severity: All",
    };
    let game_lbl = match props.filter.game_no {
        Some(g) => format!("Game {g}"),
        None => format!("All games ({})", props.games.len()),
    };

    let filter_bar = div()
        .id("analysis-cube-filter-bar")
        .test_support()
        .h_flex()
        .items_center()
        .flex_wrap()
        .gap(px(6.0))
        .pb(px(8.0))
        .border_b_1()
        .border_color(p.border)
        .child(
            ActionButton::new("cube-filter-errors-only")
                .label(I18n::t(locale, "review-statistics", "inaccuracies"))
                .active(props.filter.errors_only)
                .palette(p)
                .on_click(move |ev, w, cx| on_err(ev, w, cx)),
        )
        .child(
            ActionButton::new("cube-filter-player")
                .label(player_lbl)
                .active(props.filter.player.is_some())
                .palette(p)
                .on_click(move |ev, w, cx| on_pl(ev, w, cx)),
        )
        .child(
            ActionButton::new("cube-filter-severity")
                .label(sev_lbl)
                .active(props.filter.severity.is_some())
                .palette(p)
                .on_click(move |ev, w, cx| on_sev(ev, w, cx)),
        )
        .child(
            ActionButton::new("cube-filter-game")
                .label(game_lbl)
                .active(props.filter.game_no.is_some())
                .palette(p)
                .on_click(move |ev, w, cx| on_game(ev, w, cx)),
        )
        .when(props.filter.has_active_filters(), |el| {
            el.child(
                ActionButton::new("cube-filter-reset")
                    .label(I18n::t(locale, "review-statistics", "resetFilters"))
                    .ghost()
                    .palette(p)
                    .on_click(move |ev, w, cx| on_reset(ev, w, cx)),
            )
        });

    let selected_decision = props
        .selected_id
        .and_then(|sid| props.decisions.iter().find(|d| d.id == sid));

    let details_panel = if let Some(dec) = selected_decision {
        let response = matches!(dec.decision.played.as_str(), "take" | "drop");
        let player_idx = usize::try_from(dec.decision.player).unwrap_or(0).min(1);
        let override_probs = props.override_chances.as_ref().map(|(probs, _, _)| *probs);
        let eval_data = to_cube_evaluation_data(dec, override_probs);
        let match_id_str = dec
            .review_position
            .as_ref()
            .map(|rp| rp.gnubg_match_id.clone());
        let dec_id = dec.id.clone();
        let calc_cb = props.callbacks.on_calculate_chances.clone();
        let calculating = props.calculating_chances_for == Some(dec.id.as_str());

        let mut view = CubeDecisionDetailsView::new(
            "analysis-cube-details",
            player_idx,
            response,
            props.players.clone(),
        )
        .evaluation(eval_data)
        .calculating_chances(calculating)
        .palette(p)
        .on_calculate_chances(move |_, w, cx| calc_cb(dec_id.clone(), w, cx));

        if let Some(mid) = match_id_str {
            view = view.match_id(mid);
        }

        div()
            .p(px(10.0))
            .rounded(px(6.0))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .child(view)
            .into_any_element()
    } else {
        div()
            .p(px(10.0))
            .rounded(px(6.0))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .text_meta()
            .text_color(p.muted)
            .child(I18n::t(locale, "board-review", "cubeUnselected"))
            .into_any_element()
    };

    let list_section = if props.loading && props.decisions.is_empty() {
        div()
            .text_meta()
            .text_color(p.muted)
            .child("Loading cube decisions…")
            .into_any_element()
    } else if props.decisions.is_empty() {
        div()
            .id("analysis-cube-empty")
            .test_support()
            .p(px(12.0))
            .rounded(px(6.0))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .text_meta()
            .text_color(p.muted)
            .child("No cube decisions match the current filter.")
            .into_any_element()
    } else {
        div()
            .id("analysis-cube-list")
            .test_support()
            .v_flex()
            .gap(px(4.0))
            .children(props.decisions.iter().enumerate().map(|(ix, dec)| {
                let is_sel = props.selected_id == Some(dec.id.as_str());
                let pidx = usize::try_from(dec.decision.player).unwrap_or(0).min(1);
                let sev = parse_severity(&dec.decision.severity).unwrap_or(MoveSeverity::None);
                let st = SeverityStyle::resolve(sev, &p);
                let on_pick = props.callbacks.on_select_decision.clone();
                let rid = dec.id.clone();
                let dno = dec.decision.decision_no;

                div()
                    .id(SharedString::from(format!("cube-list-row-{ix}")))
                    .test_support()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(5.0))
                    .bg(if is_sel { p.selected } else { p.surface })
                    .border_1()
                    .border_color(if is_sel { p.accent } else { p.border })
                    .cursor_pointer()
                    .on_click(move |_, w, cx| on_pick(rid.clone(), dno, w, cx))
                    .child(div().text_table().text_color(p.text).child(format!(
                        "G{} #{} · {} · {} (best: {})",
                        dec.game_no,
                        dec.decision.decision_no,
                        props.players[pidx],
                        localize_decision(locale, &dec.decision.played),
                        localize_decision(locale, &dec.decision.best),
                    )))
                    .child(
                        div()
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .bg(st.bg)
                            .text_micro()
                            .tabular_nums()
                            .text_color(st.fg)
                            .child(format!(
                                "-{}",
                                format_decimal(locale, dec.decision.equity_loss, 3, 3)
                            )),
                    )
            }))
            .into_any_element()
    };

    div()
        .id("analysis-cube-inspector")
        .test_support()
        .v_flex()
        .gap(px(10.0))
        .child(filter_bar)
        .child(details_panel)
        .child(list_section)
}

// ── Statistics Inspector Tab ─────────────────────────────────────────────────

/// Active drill-down popover target inside the Statistics grade matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsDrilldownTarget {
    pub player: i32,
    pub severity: MoveSeverity,
    pub checker_count: i32,
    pub cube_count: i32,
}

#[derive(Clone)]
pub struct StatsCallbacks {
    pub on_open_game: ActionCb<i32>,
    pub on_open_drilldown: ActionCb<Option<StatsDrilldownTarget>>,
    pub on_drill_checker: Action2Cb<i32, MoveSeverity>,
    pub on_drill_cube: Action2Cb<i32, MoveSeverity>,
    pub on_select_compare_a: ActionCb<Option<String>>,
    pub on_select_compare_b: ActionCb<Option<String>>,
}

pub struct StatsViewProps<'a> {
    pub statistics: Option<&'a MatchAnalysisStatistics>,
    pub compare_a_stats: Option<&'a MatchAnalysisStatistics>,
    pub compare_b_stats: Option<&'a MatchAnalysisStatistics>,
    pub compare_a_id: Option<&'a str>,
    pub compare_b_id: Option<&'a str>,
    pub catalog_items: &'a [AnalysisCatalogEntry],
    pub players: [SharedString; 2],
    pub inspector_width_px: f32,
    pub loading: bool,
    pub active_drilldown: Option<StatsDrilldownTarget>,
    pub callbacks: StatsCallbacks,
    pub palette: NeonPalette,
}

/// Returns true when the Statistics inspector should switch to its responsive 2-column layout.
pub fn stats_uses_two_columns(inspector_width_px: f32) -> bool {
    inspector_width_px >= STATS_TWO_COLUMN_MIN_WIDTH
}

/// Renders the Statistics inspector tab with grade matrix, drill-down popover,
/// per-game table, `MatchAnalysisComparison`, and responsive ≥1050px 2-column switch.
pub fn render_statistics_inspector(props: StatsViewProps<'_>, cx: &App) -> impl IntoElement {
    let locale = current_locale(cx);
    let p = props.palette;
    let two_col = stats_uses_two_columns(props.inspector_width_px);

    if props.loading && props.statistics.is_none() {
        return div()
            .id("analysis-stats-loading")
            .test_support()
            .text_meta()
            .text_color(p.muted)
            .child(I18n::t(locale, "review-statistics", "loading"))
            .into_any_element();
    }

    let Some(stats) = props.statistics else {
        return div()
            .id("analysis-stats-empty")
            .test_support()
            .p(px(12.0))
            .rounded(px(6.0))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .text_meta()
            .text_color(p.muted)
            .child(I18n::t(locale, "review-statistics", "noSummary"))
            .into_any_element();
    };

    let Some(summary) = stats.summary.as_ref() else {
        return div()
            .p(px(12.0))
            .rounded(px(6.0))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .text_meta()
            .text_color(p.muted)
            .child(I18n::t(locale, "review-statistics", "noSummary"))
            .into_any_element();
    };

    let mut top_section =
        div()
            .v_flex()
            .gap(px(10.0))
            .when(stats.needs_cube_rule_reanalysis, |el| {
                el.child(
                    div()
                        .px(px(10.0))
                        .py(px(6.0))
                        .rounded(px(6.0))
                        .bg(p.warning_bg)
                        .text_meta()
                        .text_color(p.warning_text)
                        .child(I18n::t(locale, "review-statistics", "reanalyzeCubeless")),
                )
            });

    // Player summary cards (Total / Checker / Cube + grade matrix)
    let player_cards = (0..2usize).map(|idx| {
        let ps = &summary.players[idx];
        let pr_val = summary
            .performance_rating
            .as_ref()
            .and_then(|r| r.players.get(idx))
            .and_then(|p| p.value);
        let breakdown: Option<&PlayerErrorBreakdown> =
            stats.error_breakdown.as_ref().and_then(|arr| arr.get(idx));

        render_player_stats_card(
            idx as i32,
            &props.players[idx],
            ps,
            pr_val,
            breakdown,
            props.active_drilldown,
            &props.callbacks,
            p,
            cx,
        )
    });

    if two_col {
        top_section = top_section.child(
            div()
                .id("analysis-stats-two-col-grid")
                .test_support()
                .h_flex()
                .items_start()
                .gap(px(10.0))
                .children(player_cards.map(|card| div().flex_1().child(card))),
        );
    } else {
        top_section = top_section.child(
            div()
                .id("analysis-stats-single-col")
                .test_support()
                .v_flex()
                .gap(px(10.0))
                .children(player_cards),
        );
    }

    // Active drill-down popover
    if let Some(dd) = props.active_drilldown {
        let on_checker = props.callbacks.on_drill_checker.clone();
        let on_cube = props.callbacks.on_drill_cube.clone();
        let on_close = props.callbacks.on_open_drilldown.clone();
        let pname = props.players[usize::try_from(dd.player).unwrap_or(0).min(1)].clone();

        top_section = top_section.child(
            div()
                .id("analysis-stats-drilldown-popover")
                .test_support()
                .h_flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(6.0))
                .bg(p.selected)
                .border_1()
                .border_color(p.accent)
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .child(format!("{pname} · {:?} breakdown", dd.severity)),
                )
                .child(
                    div()
                        .h_flex()
                        .gap(px(6.0))
                        .child(
                            ActionButton::new("drill-checker-btn")
                                .label(format!("Checker ({})", dd.checker_count))
                                .disabled(dd.checker_count == 0)
                                .palette(p)
                                .on_click(move |_, w, cx| {
                                    on_checker(dd.player, dd.severity, w, cx)
                                }),
                        )
                        .child(
                            ActionButton::new("drill-cube-btn")
                                .label(format!("Cube ({})", dd.cube_count))
                                .disabled(dd.cube_count == 0)
                                .palette(p)
                                .on_click(move |_, w, cx| on_cube(dd.player, dd.severity, w, cx)),
                        )
                        .child(
                            ActionButton::new("drill-close-btn")
                                .icon(ActionIconName::Close)
                                .dense_nav()
                                .palette(p)
                                .on_click(move |_, w, cx| on_close(None, w, cx)),
                        ),
                ),
        );
    }

    // Per-game statistics table
    let games_table = render_per_game_table(&stats.games, &props.callbacks, p, cx);

    // MatchAnalysisComparison (compare 2 catalog entries)
    let comparison_section = render_match_analysis_comparison(
        props.catalog_items,
        props.compare_a_id,
        props.compare_b_id,
        props.compare_a_stats,
        props.compare_b_stats,
        &props.players,
        &props.callbacks,
        p,
        cx,
    );

    div()
        .id("analysis-statistics-inspector")
        .test_support()
        .v_flex()
        .gap(px(12.0))
        .child(top_section)
        .child(games_table)
        .child(comparison_section)
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_player_stats_card(
    player_idx: i32,
    player_name: &SharedString,
    summary: &neondojo::model::PlayerAnalysisSummary,
    pr_val: Option<f64>,
    breakdown: Option<&PlayerErrorBreakdown>,
    active_drilldown: Option<StatsDrilldownTarget>,
    callbacks: &StatsCallbacks,
    p: NeonPalette,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let total_loss = summary.checker_equity_loss + summary.cube_equity_loss;

    let sev_rows: [(MoveSeverity, &str, i32, i32, i32); 4] = [
        (
            MoveSeverity::Doubtful,
            "?!",
            summary.doubtful,
            breakdown.map(|b| b.checker.doubtful).unwrap_or(0),
            breakdown.map(|b| b.cube.doubtful).unwrap_or(0),
        ),
        (
            MoveSeverity::Bad,
            "?",
            summary.bad,
            breakdown.map(|b| b.checker.bad).unwrap_or(0),
            breakdown.map(|b| b.cube.bad).unwrap_or(0),
        ),
        (
            MoveSeverity::Blunder,
            "??",
            summary.blunder,
            breakdown.map(|b| b.checker.blunder).unwrap_or(0),
            breakdown.map(|b| b.cube.blunder).unwrap_or(0),
        ),
        (
            MoveSeverity::VeryBad,
            "???",
            summary.very_bad,
            breakdown.map(|b| b.checker.very_bad).unwrap_or(0),
            breakdown.map(|b| b.cube.very_bad).unwrap_or(0),
        ),
    ];

    div()
        .id(SharedString::from(format!(
            "stats-player-card-{player_idx}"
        )))
        .test_support()
        .v_flex()
        .gap(px(8.0))
        .p(px(10.0))
        .rounded(px(6.0))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_panel()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .child(player_name.clone()),
                )
                .child(
                    div()
                        .text_label()
                        .tabular_nums()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.accent)
                        .child(
                            pr_val
                                .map(|v| format!("PR {}", format_pr(locale, v)))
                                .unwrap_or_else(|| "PR —".to_string()),
                        ),
                ),
        )
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .text_micro()
                .tabular_nums()
                .text_color(p.muted)
                .child(format!(
                    "Checker: {} (-{})",
                    summary.checker_decisions,
                    format_decimal(locale, summary.checker_equity_loss, 3, 3)
                ))
                .child(format!(
                    "Cube: {} (-{})",
                    summary.cube_decisions,
                    format_decimal(locale, summary.cube_equity_loss, 3, 3)
                ))
                .child(format!(
                    "Total: -{}",
                    format_decimal(locale, total_loss, 3, 3)
                )),
        )
        .child(
            // Grade matrix: Severity | Total | Checker | Cube
            div()
                .v_flex()
                .gap(px(2.0))
                .child(
                    div()
                        .h_flex()
                        .px(px(4.0))
                        .text_table_heading()
                        .text_color(p.muted)
                        .child(div().w(px(52.0)).child("Grade"))
                        .child(div().flex_1().text_right().child("Total"))
                        .child(div().flex_1().text_right().child("Checker"))
                        .child(div().flex_1().text_right().child("Cube")),
                )
                .children(sev_rows.into_iter().map(|(sev, glyph, tot, chk, cub)| {
                    let st = SeverityStyle::resolve(sev, &p);
                    let target = StatsDrilldownTarget {
                        player: player_idx,
                        severity: sev,
                        checker_count: chk,
                        cube_count: cub,
                    };
                    let is_active = active_drilldown == Some(target);
                    let on_dd = callbacks.on_open_drilldown.clone();
                    div()
                        .id(SharedString::from(format!(
                            "stats-grade-{player_idx}-{}",
                            sev.as_str()
                        )))
                        .test_support()
                        .h_flex()
                        .items_center()
                        .px(px(4.0))
                        .py(px(3.0))
                        .rounded(px(4.0))
                        .bg(if is_active { p.selected } else { p.surface_alt })
                        .cursor_pointer()
                        .text_table()
                        .tabular_nums()
                        .on_click(move |_, w, cx| {
                            on_dd(if is_active { None } else { Some(target) }, w, cx);
                        })
                        .child(
                            div()
                                .w(px(52.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(st.fg)
                                .child(glyph),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_right()
                                .text_color(p.text)
                                .child(format!("{tot}")),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_right()
                                .text_color(p.muted)
                                .child(format!("{chk}")),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_right()
                                .text_color(p.muted)
                                .child(format!("{cub}")),
                        )
                })),
        )
}

pub(super) fn statistics_game_labels(locale: Locale, game_no: i32) -> (String, String) {
    let game = &[("game", game_no.into())];
    (
        I18n::format(locale, "review-statistics", "game", game),
        I18n::format(locale, "review-statistics", "openGame", game),
    )
}

fn render_per_game_table(
    games: &[CatalogGameStatistics],
    callbacks: &StatsCallbacks,
    p: NeonPalette,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    if games.is_empty() {
        return div().into_any_element();
    }

    let mut table = div()
        .id("analysis-stats-games-table")
        .test_support()
        .v_flex()
        .rounded(px(6.0))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .overflow_hidden()
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(5.0))
                .bg(p.surface_alt)
                .border_b_1()
                .border_color(p.border)
                .text_table_heading()
                .text_color(p.muted)
                .child(
                    div()
                        .w(px(72.0))
                        .child(I18n::t(locale, "review-statistics", "games")),
                )
                .child(div().w(px(60.0)).child("Score"))
                .child(div().flex_1().text_right().child("W PR"))
                .child(div().flex_1().text_right().child("B PR"))
                .child(div().w(px(116.0)).text_right().child("Action")),
        );

    for g in games {
        let gno = g.game_no;
        let (game_label, open_label) = statistics_game_labels(locale, gno);
        let on_open = callbacks.on_open_game.clone();
        let (wpr, bpr) = g
            .summary
            .as_ref()
            .and_then(|s| s.performance_rating.as_ref())
            .map(|r| (r.players[0].value, r.players[1].value))
            .unwrap_or((None, None));

        table =
            table.child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .py(px(5.0))
                    .border_b_1()
                    .border_color(p.border)
                    .text_table()
                    .tabular_nums()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(4.0))
                            .w(px(72.0))
                            .text_color(p.text)
                            .child(game_label)
                            .when(g.needs_cube_rule_reanalysis, |el| {
                                el.child(div().text_micro().text_color(p.warning_text).child("!"))
                            }),
                    )
                    .child(div().w(px(60.0)).text_color(p.muted).child(format!(
                        "{}–{}{}",
                        g.white_score,
                        g.black_score,
                        if g.crawford { " Cr" } else { "" }
                    )))
                    .child(div().flex_1().text_right().text_color(p.text).child(
                        wpr.map(|v| format_pr(locale, v)).unwrap_or_else(|| {
                            if g.analyzed { "—" } else { "Pending" }.to_string()
                        }),
                    ))
                    .child(
                        div().flex_1().text_right().text_color(p.text).child(
                            bpr.map(|v| format_pr(locale, v))
                                .unwrap_or_else(|| "—".to_string()),
                        ),
                    )
                    .child(
                        div().w(px(116.0)).h_flex().justify_end().child(
                            ActionButton::new(SharedString::from(format!("stats-open-game-{gno}")))
                                .label(open_label)
                                .palette(p)
                                .on_click(move |_, w, cx| on_open(gno, w, cx)),
                        ),
                    ),
            );
    }

    table.into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_match_analysis_comparison(
    catalog_items: &[AnalysisCatalogEntry],
    compare_a_id: Option<&str>,
    compare_b_id: Option<&str>,
    stats_a: Option<&MatchAnalysisStatistics>,
    stats_b: Option<&MatchAnalysisStatistics>,
    players: &[SharedString; 2],
    callbacks: &StatsCallbacks,
    p: NeonPalette,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let comparable: Vec<&AnalysisCatalogEntry> = catalog_items
        .iter()
        .filter(|e| e.scope == "match" || e.scope == "game")
        .collect();
    if comparable.len() < 2 {
        return div().into_any_element();
    }

    let next_for = |current: Option<&str>| -> Option<String> {
        let idx = current
            .and_then(|id| comparable.iter().position(|e| e.id == id))
            .unwrap_or(0);
        Some(comparable[(idx + 1) % comparable.len()].id.clone())
    };
    let label_for = |current: Option<&str>| -> String {
        current
            .and_then(|id| comparable.iter().find(|e| e.id == id))
            .map(|e| format!("{} ({})", e.engine, e.depth.as_deref().unwrap_or("—")))
            .unwrap_or_else(|| "Select run".to_string())
    };

    let next_a = next_for(compare_a_id);
    let next_b = next_for(compare_b_id);
    let on_a = callbacks.on_select_compare_a.clone();
    let on_b = callbacks.on_select_compare_b.clone();

    let mut card = div()
        .id("analysis-match-comparison")
        .test_support()
        .v_flex()
        .gap(px(8.0))
        .p(px(10.0))
        .rounded(px(6.0))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .flex_wrap()
                .gap(px(8.0))
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .child("Match analysis comparison"),
                )
                .child(
                    div()
                        .h_flex()
                        .gap(px(6.0))
                        .child(
                            ActionButton::new("stats-compare-pick-a")
                                .label(format!("A: {}", label_for(compare_a_id)))
                                .palette(p)
                                .on_click(move |_, w, cx| on_a(next_a.clone(), w, cx)),
                        )
                        .child(
                            ActionButton::new("stats-compare-pick-b")
                                .label(format!("B: {}", label_for(compare_b_id)))
                                .palette(p)
                                .on_click(move |_, w, cx| on_b(next_b.clone(), w, cx)),
                        ),
                ),
        );

    if let (Some(sa), Some(sb)) = (
        stats_a.and_then(|s| s.summary.as_ref()),
        stats_b.and_then(|s| s.summary.as_ref()),
    ) {
        for idx in 0..2 {
            let pa = &sa.players[idx];
            let pb = &sb.players[idx];
            let pra = sa
                .performance_rating
                .as_ref()
                .and_then(|r| r.players[idx].value)
                .map(|v| format_pr(locale, v))
                .unwrap_or_else(|| "—".to_string());
            let prb = sb
                .performance_rating
                .as_ref()
                .and_then(|r| r.players[idx].value)
                .map(|v| format_pr(locale, v))
                .unwrap_or_else(|| "—".to_string());
            card = card.child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .px(px(6.0))
                    .py(px(4.0))
                    .rounded(px(4.0))
                    .bg(p.surface_alt)
                    .text_table()
                    .tabular_nums()
                    .child(players[idx].clone())
                    .child(format!(
                        "A: PR {pra} (chk -{}, cube -{})",
                        format_decimal(locale, pa.checker_equity_loss, 3, 3),
                        format_decimal(locale, pa.cube_equity_loss, 3, 3)
                    ))
                    .child(format!(
                        "B: PR {prb} (chk -{}, cube -{})",
                        format_decimal(locale, pb.checker_equity_loss, 3, 3),
                        format_decimal(locale, pb.cube_equity_loss, 3, 3)
                    )),
            );
        }
    }

    card.into_any_element()
}

/// Helper summing total errors in an `ErrorSeverityCounts`.
#[allow(dead_code)]
pub fn total_severity_errors(counts: &ErrorSeverityCounts) -> i32 {
    counts.doubtful + counts.bad + counts.blunder + counts.very_bad
}
