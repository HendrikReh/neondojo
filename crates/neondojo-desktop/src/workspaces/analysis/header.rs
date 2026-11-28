//! Top Analysis header bar:
//! - `AnalysisMatchHeader`: match identity, score, date, PRs, import warnings,
//!   retained-position badge, and Study Report handoff (`WorkspaceEvent::OpenStudyReport`).
//! - `AnalysisCatalogSelect`: Automatic vs GNUbg / NeonGammon / imported analyses,
//!   cursor-paged, zero silent cross-engine fallback, delete with `source_revision`.
//! - `GameSelector`: Prev/Next + dropdown supporting `position_count == 0` cube-only games.
//! - Inline `MatchAnalysisHeader`: Start Analysis, Options popover with live
//!   `AnalysisTimeEstimate`, active jobs strip with Cancel & 1s monotonic ETA.
//! - `TextNavBar` for the 5 inspector detail tabs (`Overview`, `Cube`, `Statistics`, `Notes`, `Cards`).

use super::jobs::{AnalysisLaunchOptions, MatchAnalysisJobEntity};
use super::state::{
    Action2Cb, ActionCb, AnalysisDetailTab, ClickCb, format_eta_ms, is_cube_only_game,
};
use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::icons::ActionIconName;
use crate::ui::modal::{ModalConfig, ModalSize, open_modal_dialog};
use crate::ui::text_nav::{TextNavBar, TextNavDensity, TextNavItem};
use gpui_kit::{
    App, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString, Styled,
    TestSupportExt, Window, base::StyledExt, component::WindowExt, div, prelude::FluentBuilder, px,
};
use neondojo::{
    analysis_catalog::AnalysisCatalogEntry,
    api::MatchAnalysisEngine,
    benchmarks::estimate::{AnalysisTimeEstimate, EstimateUnavailableReason},
    databases::DatabaseMatch,
    model::{MatchGame, MatchSummary},
};
use neondojo_i18n::{I18n, format_naive_date_medium, format_pr};
use uuid::Uuid;

/// Callbacks wired from `AnalysisWorkspace` into `AnalysisHeaderBar`.
#[derive(Clone)]
pub struct HeaderCallbacks {
    pub on_select_tab: ActionCb<AnalysisDetailTab>,
    pub on_select_game: ActionCb<i32>,
    pub on_toggle_game_menu: ClickCb,
    pub on_toggle_catalog_menu: ClickCb,
    pub on_select_analysis: ActionCb<Option<String>>,
    pub on_load_more_catalog: ClickCb,
    pub on_delete_analysis: Action2Cb<String, i64>,
    pub on_toggle_options_popover: ClickCb,
    pub on_start_analysis: ActionCb<bool>,
    pub on_cancel_job: ActionCb<Uuid>,
    pub on_open_study_report: ActionCb<Uuid>,
    pub on_open_welcome_drop: ClickCb,
}

/// Snapshot of header state passed to [`render_analysis_header`].
pub struct HeaderProps<'a> {
    pub match_summary: Option<&'a MatchSummary>,
    pub match_meta: Option<&'a DatabaseMatch>,
    pub games: &'a [MatchGame],
    pub current_game_no: Option<i32>,
    pub active_tab: AnalysisDetailTab,
    pub catalog_items: &'a [AnalysisCatalogEntry],
    pub catalog_selected: Option<&'a AnalysisCatalogEntry>,
    pub catalog_selected_id: Option<&'a str>,
    pub catalog_source_revision: i64,
    pub catalog_next_cursor: Option<&'a str>,
    pub catalog_loading: bool,
    pub game_menu_open: bool,
    pub catalog_menu_open: bool,
    pub options_popover_open: bool,
    pub job_controller: &'a Entity<MatchAnalysisJobEntity>,
    pub callbacks: HeaderCallbacks,
    pub palette: NeonPalette,
}

/// Renders the full Analysis top header bar (Match summary + controls + popovers + running jobs).
pub fn render_analysis_header(props: HeaderProps<'_>, cx: &App) -> impl IntoElement {
    let locale = current_locale(cx);
    let p = props.palette;
    let source_available = props.match_summary.is_none_or(|m| m.source_available);
    let match_id = props.match_summary.map(|m| m.id);

    let jobs_state = props.job_controller.read(cx);
    let active_jobs = match_id
        .map(|mid| jobs_state.active_jobs_for_match(mid))
        .unwrap_or_default();
    let launch_opts = jobs_state.options.clone();
    let estimate = jobs_state.estimate.as_ref();
    let estimate_loading = jobs_state.estimate_loading;
    let busy_submitting = jobs_state.busy_submitting;

    // 1. Top match summary row
    let title_text = props
        .match_summary
        .map(|m| format!("{} vs {}", m.white, m.black))
        .unwrap_or_else(|| "Analysis".to_string());

    let mut meta_parts: Vec<String> = Vec::new();
    if let Some(meta) = props.match_meta {
        meta_parts.push(format!("#{}", meta.match_no));
        if meta.match_length > 0 {
            meta_parts.push(format!("{} pt", meta.match_length));
        } else {
            meta_parts.push(I18n::t(locale, "app", "common.moneyGame").to_string());
        }
        if let (Some(ws), Some(bs)) = (meta.white_score, meta.black_score) {
            meta_parts.push(format!("{ws}–{bs}"));
        }
        if let Some(date) = meta.played_on {
            meta_parts.push(format_naive_date_medium(locale, date));
        }
        if let Some(t) = meta.tournament.as_deref().filter(|s| !s.trim().is_empty()) {
            meta_parts.push(t.to_string());
        }
        if let Some(r) = meta.round.as_deref().filter(|s| !s.trim().is_empty()) {
            meta_parts.push(r.to_string());
        }
        if let Some(pl) = meta.place.as_deref().filter(|s| !s.trim().is_empty()) {
            meta_parts.push(pl.to_string());
        }
        if let Some(wpr) = meta.white_pr {
            meta_parts.push(format!("W PR {}", format_pr(locale, wpr)));
        }
        if let Some(bpr) = meta.black_pr {
            meta_parts.push(format!("B PR {}", format_pr(locale, bpr)));
        }
    } else if let Some(m) = props.match_summary {
        if m.match_length > 0 {
            meta_parts.push(format!("{} pt", m.match_length));
        } else {
            meta_parts.push(I18n::t(locale, "app", "common.moneyGame").to_string());
        }
        meta_parts.push(format!("{} pos", m.position_count));
    }

    let import_warnings_count = props
        .match_meta
        .map(|m| m.import_warning_count)
        .unwrap_or(0);

    let on_study = props.callbacks.on_open_study_report.clone();
    let on_welcome = props.callbacks.on_open_welcome_drop.clone();

    let match_header_row = div()
        .id("analysis-match-header")
        .test_support()
        .h_flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .px(px(12.0))
        .py(px(8.0))
        .border_b_1()
        .border_color(p.border)
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(10.0))
                .min_w(px(0.0))
                .child(
                    div()
                        .text_panel()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .child(title_text),
                )
                .when(!source_available, |el| {
                    el.child(
                        div()
                            .id("analysis-retained-badge")
                            .test_support()
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .bg(p.warning_bg)
                            .text_micro()
                            .text_color(p.warning_text)
                            .child(I18n::t(locale, "analysis-match-header", "retainedPosition")),
                    )
                })
                .when(import_warnings_count > 0, |el| {
                    el.child(
                        div()
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .bg(p.warning_bg)
                            .text_micro()
                            .text_color(p.warning_text)
                            .child(format!("{import_warnings_count} import notes")),
                    )
                })
                .child(
                    div()
                        .text_meta()
                        .tabular_nums()
                        .text_color(p.muted)
                        .truncate()
                        .child(meta_parts.join(" · ")),
                ),
        )
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(6.0))
                .when_some(match_id.filter(|_| source_available), |el, mid| {
                    let cb = on_study.clone();
                    el.child(
                        ActionButton::new("analysis-open-study-report")
                            .label("Study report")
                            .icon(ActionIconName::Cards)
                            .palette(p)
                            .on_click(move |_, w, cx| cb(mid, w, cx)),
                    )
                })
                .child(
                    ActionButton::new("analysis-import-drop-btn")
                        .icon(ActionIconName::Import)
                        .dense_nav()
                        .tooltip("Import match…")
                        .palette(p)
                        .on_click(move |ev, w, cx| on_welcome(ev, w, cx)),
                ),
        );

    // 2. Controls row: GameSelector + AnalysisCatalogSelect + Analyze actions + Inspector TextNavBar
    let game_selector = render_game_selector(
        props.games,
        props.current_game_no,
        props.game_menu_open,
        &props.callbacks,
        p,
    );

    let catalog_selector = render_catalog_selector(
        props.catalog_selected,
        props.catalog_selected_id,
        props.catalog_items,
        props.catalog_source_revision,
        props.catalog_next_cursor,
        props.catalog_loading,
        props.catalog_menu_open,
        &props.callbacks,
        p,
        cx,
    );

    let on_start_match = props.callbacks.on_start_analysis.clone();
    let on_toggle_opts = props.callbacks.on_toggle_options_popover.clone();
    let analyze_controls = div().h_flex().items_center().gap(px(4.0)).when(
        source_available && match_id.is_some(),
        |el| {
            let start_cb = on_start_match.clone();
            let opts_cb = on_toggle_opts.clone();
            let label = format!(
                "Analyze ({} {}-ply)",
                match launch_opts.engine {
                    MatchAnalysisEngine::Gnubg => "GNUbg",
                    MatchAnalysisEngine::Bgsage => "NeonGammon",
                },
                launch_opts.plies
            );
            el.child(
                ActionButton::new("analysis-start-btn")
                    .label(label)
                    .icon(ActionIconName::Analysis)
                    .primary()
                    .disabled(busy_submitting)
                    .palette(p)
                    .on_click(move |_, w, cx| start_cb(launch_opts.scope_current_game, w, cx)),
            )
            .child(
                ActionButton::new("analysis-options-btn")
                    .icon(ActionIconName::Settings)
                    .dense_nav()
                    .active(props.options_popover_open)
                    .tooltip("Analysis options")
                    .palette(p)
                    .on_click(move |ev, w, cx| opts_cb(ev, w, cx)),
            )
        },
    );

    let tab_items: Vec<TextNavItem> = AnalysisDetailTab::ALL
        .into_iter()
        .filter(|tab| source_available || tab.available_when_retained())
        .map(|tab| {
            let label = match tab {
                AnalysisDetailTab::Overview => "Overview",
                AnalysisDetailTab::Cube => I18n::t(locale, "analysis-source", "cube"),
                AnalysisDetailTab::Statistics => "Statistics",
                AnalysisDetailTab::Notes => I18n::t(locale, "workspace", "notes"),
                AnalysisDetailTab::Cards => I18n::t(locale, "workspace", "cards"),
            };
            TextNavItem::new(tab.id(), label)
                .element_id(SharedString::from(format!("analysis-tab-{}", tab.id())))
        })
        .collect();

    let on_tab = props.callbacks.on_select_tab.clone();
    let detail_nav = TextNavBar::new("analysis-detail-tabs", tab_items)
        .active(Some(props.active_tab.id()))
        .density(TextNavDensity::Inline)
        .palette(p)
        .on_select(move |id, w, cx| {
            if let Some(tab) = AnalysisDetailTab::parse(id) {
                on_tab(tab, w, cx);
            }
        });

    let controls_row = div()
        .h_flex()
        .items_center()
        .justify_between()
        .flex_wrap()
        .gap(px(10.0))
        .px(px(12.0))
        .py(px(6.0))
        .bg(p.surface)
        .border_b_1()
        .border_color(p.border)
        .child(
            div()
                .h_flex()
                .items_center()
                .flex_wrap()
                .gap(px(8.0))
                .child(game_selector)
                .child(catalog_selector)
                .child(analyze_controls),
        )
        .child(detail_nav);

    // 3. Optional inline popovers (Game picker, Catalog picker, Analysis Options)
    let mut root = div()
        .id("analysis-header-bar")
        .test_support()
        .v_flex()
        .w_full()
        .flex_shrink_0()
        .bg(p.surface)
        .child(match_header_row)
        .child(controls_row);

    if props.options_popover_open {
        root = root.child(render_options_popover(
            props.job_controller,
            match_id,
            props.current_game_no,
            &launch_opts,
            estimate,
            estimate_loading,
            &props.callbacks,
            p,
        ));
    }

    if !active_jobs.is_empty() {
        root = root.child(render_active_jobs_strip(
            &active_jobs,
            jobs_state,
            &props.callbacks,
            p,
        ));
    }

    if let Some(err) = jobs_state.last_error.as_deref() {
        root = root.child(
            div()
                .id("analysis-job-error-bar")
                .test_support()
                .px(px(12.0))
                .py(px(4.0))
                .bg(p.danger_bg)
                .text_meta()
                .text_color(p.danger_text)
                .child(err.to_string()),
        );
    }

    root
}

fn render_game_selector(
    games: &[MatchGame],
    current_game_no: Option<i32>,
    menu_open: bool,
    callbacks: &HeaderCallbacks,
    p: NeonPalette,
) -> impl IntoElement {
    let idx = current_game_no
        .and_then(|gno| games.iter().position(|g| g.game_no == gno))
        .unwrap_or(0);
    let current = games.get(idx);
    let prev_game = idx
        .checked_sub(1)
        .and_then(|i| games.get(i))
        .map(|g| g.game_no);
    let next_game = games.get(idx + 1).map(|g| g.game_no);

    let label = current
        .map(|g| {
            let cube_only = if is_cube_only_game(g) {
                " · cube only"
            } else {
                ""
            };
            let crawford = if g.crawford { " · Cr" } else { "" };
            format!(
                "Game {}/{} ({}–{}{}{})",
                g.game_no,
                games.len(),
                g.white_score,
                g.black_score,
                crawford,
                cube_only
            )
        })
        .unwrap_or_else(|| "No games".to_string());

    let on_prev = callbacks.on_select_game.clone();
    let on_next = callbacks.on_select_game.clone();
    let on_toggle = callbacks.on_toggle_game_menu.clone();

    div()
        .id("analysis-game-selector")
        .test_support()
        .v_flex()
        .gap(px(4.0))
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(4.0))
                .child(
                    ActionButton::new("game-prev-btn")
                        .icon(ActionIconName::Back)
                        .dense_nav()
                        .disabled(prev_game.is_none())
                        .tooltip("Previous game")
                        .palette(p)
                        .on_click(move |_, w, cx| {
                            if let Some(g) = prev_game {
                                on_prev(g, w, cx);
                            }
                        }),
                )
                .child(
                    ActionButton::new("game-dropdown-trigger")
                        .label(label)
                        .active(menu_open)
                        .disabled(games.is_empty())
                        .palette(p)
                        .on_click(move |ev, w, cx| on_toggle(ev, w, cx)),
                )
                .child(
                    ActionButton::new("game-next-btn")
                        .icon(ActionIconName::Next)
                        .dense_nav()
                        .disabled(next_game.is_none())
                        .tooltip("Next game")
                        .palette(p)
                        .on_click(move |_, w, cx| {
                            if let Some(g) = next_game {
                                on_next(g, w, cx);
                            }
                        }),
                ),
        )
        .when(menu_open && !games.is_empty(), |el| {
            el.child(
                div()
                    .id("analysis-game-dropdown-list")
                    .test_support()
                    .h_flex()
                    .flex_wrap()
                    .gap(px(4.0))
                    .p(px(6.0))
                    .rounded(px(6.0))
                    .bg(p.surface_alt)
                    .border_1()
                    .border_color(p.border)
                    .children(games.iter().map(|g| {
                        let gno = g.game_no;
                        let selected = current_game_no == Some(gno);
                        let cb = callbacks.on_select_game.clone();
                        let tag = if is_cube_only_game(g) {
                            format!("G{gno} ({}–{}, cube-only)", g.white_score, g.black_score)
                        } else {
                            format!(
                                "G{gno} ({}–{}, {}m)",
                                g.white_score, g.black_score, g.position_count
                            )
                        };
                        ActionButton::new(SharedString::from(format!("game-pick-{gno}")))
                            .label(tag)
                            .active(selected)
                            .palette(p)
                            .on_click(move |_, w, cx| cb(gno, w, cx))
                    })),
            )
        })
}

#[allow(clippy::too_many_arguments)]
fn render_catalog_selector(
    selected: Option<&AnalysisCatalogEntry>,
    selected_id: Option<&str>,
    items: &[AnalysisCatalogEntry],
    source_revision: i64,
    next_cursor: Option<&str>,
    loading: bool,
    menu_open: bool,
    callbacks: &HeaderCallbacks,
    p: NeonPalette,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let current_label = match (selected_id, selected) {
        (None, Some(eff)) => format!(
            "Auto ({}{})",
            eff.engine,
            eff.depth
                .as_deref()
                .map(|d| format!(" · {d}"))
                .unwrap_or_default()
        ),
        (None, None) => format!("{}: None", I18n::t(locale, "analysis-catalog", "automatic")),
        (Some(_), Some(eff)) => format!(
            "{}{}",
            eff.engine,
            eff.depth
                .as_deref()
                .map(|d| format!(" · {d}"))
                .unwrap_or_default()
        ),
        (Some(id), None) => id.to_string(),
    };

    let on_toggle = callbacks.on_toggle_catalog_menu.clone();
    let on_select_auto = callbacks.on_select_analysis.clone();
    let on_more = callbacks.on_load_more_catalog.clone();

    div()
        .id("analysis-catalog-select")
        .test_support()
        .v_flex()
        .gap(px(4.0))
        .child(
            ActionButton::new("catalog-dropdown-trigger")
                .label(current_label)
                .icon(ActionIconName::Filter)
                .active(menu_open)
                .palette(p)
                .on_click(move |ev, w, cx| on_toggle(ev, w, cx)),
        )
        .when(menu_open, |el| {
            el.child(
                div()
                    .id("analysis-catalog-menu")
                    .test_support()
                    .v_flex()
                    .gap(px(4.0))
                    .p(px(8.0))
                    .rounded(px(6.0))
                    .bg(p.surface_alt)
                    .border_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .text_micro()
                            .text_color(p.muted)
                            .child("Displayed analysis (zero cross-engine fallback)"),
                    )
                    .child(
                        ActionButton::new("catalog-opt-auto")
                            .label(I18n::t(locale, "analysis-catalog", "automatic"))
                            .active(selected_id.is_none())
                            .palette(p)
                            .on_click(move |_, w, cx| on_select_auto(None, w, cx)),
                    )
                    .children(items.iter().map(|entry| {
                        let eid = entry.id.clone();
                        let del_id = entry.id.clone();
                        let label = format!(
                            "{} · {} · {} ({}c / {}q)",
                            entry.engine,
                            entry.depth.as_deref().unwrap_or("—"),
                            entry.scope,
                            entry.checker_count,
                            entry.cube_count
                        );
                        let is_sel = selected_id == Some(entry.id.as_str());
                        let can_delete = entry.source == "native";
                        let on_pick = callbacks.on_select_analysis.clone();
                        let on_del = callbacks.on_delete_analysis.clone();
                        let entry_title = label.clone();

                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .gap(px(6.0))
                            .child(
                                ActionButton::new(SharedString::from(format!(
                                    "catalog-pick-{eid}"
                                )))
                                .label(label)
                                .active(is_sel)
                                .palette(p)
                                .on_click(move |_, w, cx| on_pick(Some(eid.clone()), w, cx)),
                            )
                            .when(can_delete, |row| {
                                row.child(
                                    ActionButton::new(SharedString::from(format!(
                                        "catalog-del-{del_id}"
                                    )))
                                    .icon(ActionIconName::Delete)
                                    .dense_nav()
                                    .danger()
                                    .tooltip("Delete analysis")
                                    .palette(p)
                                    .on_click(
                                        move |_, window, cx| {
                                            open_delete_analysis_confirm(
                                                del_id.clone(),
                                                entry_title.clone(),
                                                source_revision,
                                                on_del.clone(),
                                                window,
                                                cx,
                                            );
                                        },
                                    ),
                                )
                            })
                    }))
                    .when(next_cursor.is_some(), |box_el| {
                        box_el.child(
                            ActionButton::new("catalog-load-more")
                                .label(if loading {
                                    "Loading…"
                                } else {
                                    I18n::t(locale, "analysis-catalog", "more")
                                })
                                .disabled(loading)
                                .palette(p)
                                .on_click(move |ev, w, cx| on_more(ev, w, cx)),
                        )
                    }),
            )
        })
}

fn open_delete_analysis_confirm(
    analysis_id: String,
    label: String,
    source_revision: i64,
    on_confirm: Action2Cb<String, i64>,
    window: &mut Window,
    cx: &mut App,
) {
    let cfg = ModalConfig::new("Delete saved analysis").size(ModalSize::Compact);
    open_modal_dialog(
        window,
        cx,
        cfg,
        move |_w: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            div()
                .v_flex()
                .gap(px(8.0))
                .child(
                    div()
                        .text_body()
                        .text_color(p.text)
                        .child(format!("Permanently delete \"{label}\"?")),
                )
                .child(
                    div()
                        .text_meta()
                        .text_color(p.muted)
                        .child(format!("Match revision #{source_revision}")),
                )
                .into_any_element()
        },
        Some(move |_w: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let cb = on_confirm.clone();
            let id = analysis_id.clone();
            div()
                .h_flex()
                .gap(px(8.0))
                .justify_end()
                .child(
                    ActionButton::new("confirm-del-analysis-cancel")
                        .label("Cancel")
                        .palette(p)
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .child(
                    ActionButton::new("confirm-del-analysis-ok")
                        .label("Delete")
                        .danger_solid()
                        .palette(p)
                        .on_click(move |_, window, cx| {
                            cb(id.clone(), source_revision, window, cx);
                            window.close_dialog(cx);
                        }),
                )
                .into_any_element()
        }),
        None,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_options_popover(
    job_controller: &Entity<MatchAnalysisJobEntity>,
    match_id: Option<Uuid>,
    current_game_no: Option<i32>,
    options: &AnalysisLaunchOptions,
    estimate: Option<&AnalysisTimeEstimate>,
    estimate_loading: bool,
    callbacks: &HeaderCallbacks,
    p: NeonPalette,
) -> impl IntoElement {
    let allowed_plies = match options.engine {
        MatchAnalysisEngine::Gnubg => vec![1, 2, 3, 4, 5],
        MatchAnalysisEngine::Bgsage => vec![1, 2, 3, 4],
    };

    let est_text = if estimate_loading {
        "Estimating runtime…".to_string()
    } else if let Some(est) = estimate {
        if let Some(ms) = est.estimated_ms {
            format!(
                "Estimated ~{} ({} moves)",
                format_eta_ms(ms),
                est.position_count
            )
        } else {
            match est.unavailable_reason {
                Some(EstimateUnavailableReason::NoMatchingBenchmark) => {
                    "No matching benchmark for this engine/depth".to_string()
                }
                Some(EstimateUnavailableReason::EngineUnavailable) => {
                    "Selected engine is unavailable".to_string()
                }
                Some(EstimateUnavailableReason::NoCheckerMoves) => {
                    "No checker moves in scope".to_string()
                }
                None => "Estimate unavailable".to_string(),
            }
        }
    } else {
        "Select engine & depth".to_string()
    };

    let jc_gnubg = job_controller.clone();
    let jc_sage = job_controller.clone();
    let jc_whole = job_controller.clone();
    let jc_game = job_controller.clone();
    let on_start = callbacks.on_start_analysis.clone();
    let cur_plies = options.plies;
    let cur_engine = options.engine;
    let cur_scope_game = options.scope_current_game;

    div()
        .id("analysis-options-popover")
        .test_support()
        .h_flex()
        .items_center()
        .justify_between()
        .flex_wrap()
        .gap(px(12.0))
        .px(px(12.0))
        .py(px(8.0))
        .bg(p.surface_alt)
        .border_b_1()
        .border_color(p.border)
        .child(
            div()
                .h_flex()
                .items_center()
                .flex_wrap()
                .gap(px(8.0))
                .child(div().text_meta().text_color(p.muted).child("Engine:"))
                .child(
                    ActionButton::new("opt-engine-gnubg")
                        .label("GNUbg")
                        .active(cur_engine == MatchAnalysisEngine::Gnubg)
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            jc_gnubg.update(cx, |j, cx| {
                                j.set_options(
                                    MatchAnalysisEngine::Gnubg,
                                    cur_plies,
                                    cur_scope_game,
                                    match_id,
                                    current_game_no,
                                    cx,
                                );
                            });
                        }),
                )
                .child(
                    ActionButton::new("opt-engine-bgsage")
                        .label("NeonGammon")
                        .active(cur_engine == MatchAnalysisEngine::Bgsage)
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            jc_sage.update(cx, |j, cx| {
                                j.set_options(
                                    MatchAnalysisEngine::Bgsage,
                                    cur_plies,
                                    cur_scope_game,
                                    match_id,
                                    current_game_no,
                                    cx,
                                );
                            });
                        }),
                )
                .child(div().text_meta().text_color(p.muted).child("Depth:"))
                .children(allowed_plies.into_iter().map(|ply| {
                    let jc = job_controller.clone();
                    ActionButton::new(SharedString::from(format!("opt-ply-{ply}")))
                        .label(SharedString::from(format!("{ply}-ply")))
                        .active(cur_plies == ply)
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            jc.update(cx, |j, cx| {
                                j.set_options(
                                    cur_engine,
                                    ply,
                                    cur_scope_game,
                                    match_id,
                                    current_game_no,
                                    cx,
                                );
                            });
                        })
                }))
                .child(div().text_meta().text_color(p.muted).child("Scope:"))
                .child(
                    ActionButton::new("opt-scope-match")
                        .label("Whole match")
                        .active(!cur_scope_game)
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            jc_whole.update(cx, |j, cx| {
                                j.set_options(
                                    cur_engine,
                                    cur_plies,
                                    false,
                                    match_id,
                                    current_game_no,
                                    cx,
                                );
                            });
                        }),
                )
                .child(
                    ActionButton::new("opt-scope-game")
                        .label(
                            current_game_no
                                .map(|g| format!("Game {g}"))
                                .unwrap_or_else(|| "Current game".to_string()),
                        )
                        .active(cur_scope_game)
                        .disabled(current_game_no.is_none())
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            jc_game.update(cx, |j, cx| {
                                j.set_options(
                                    cur_engine,
                                    cur_plies,
                                    true,
                                    match_id,
                                    current_game_no,
                                    cx,
                                );
                            });
                        }),
                ),
        )
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .id("analysis-time-estimate")
                        .test_support()
                        .text_meta()
                        .tabular_nums()
                        .text_color(p.muted)
                        .child(est_text),
                )
                .child(
                    ActionButton::new("opt-start-now")
                        .label("Start")
                        .primary()
                        .palette(p)
                        .on_click(move |_, w, cx| on_start(cur_scope_game, w, cx)),
                ),
        )
}

fn render_active_jobs_strip(
    active_jobs: &[&neondojo::model::Job],
    jobs_state: &MatchAnalysisJobEntity,
    callbacks: &HeaderCallbacks,
    p: NeonPalette,
) -> impl IntoElement {
    div()
        .id("analysis-running-jobs-bar")
        .test_support()
        .v_flex()
        .gap(px(4.0))
        .px(px(12.0))
        .py(px(6.0))
        .bg(p.selected)
        .border_b_1()
        .border_color(p.border)
        .children(active_jobs.iter().map(|job| {
            let jid = job.id;
            let engine_label = job
                .engine_snapshot
                .as_ref()
                .map(|s| {
                    if s.engine_id == "bgsage" {
                        "NeonGammon"
                    } else {
                        "GNUbg"
                    }
                })
                .unwrap_or("GNUbg");
            let scope = job
                .game_no
                .map(|g| format!("Game {g}"))
                .unwrap_or_else(|| format!("{}/{} games", job.completed_games, job.total_games));
            let eta = jobs_state
                .remaining_label(jid)
                .map(|s| format!(" · {s}"))
                .unwrap_or_default();
            let cancelling = jobs_state
                .cancelling_jobs
                .get(&jid)
                .copied()
                .unwrap_or(false);
            let on_cancel = callbacks.on_cancel_job.clone();

            div()
                .h_flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .text_meta()
                        .tabular_nums()
                        .text_color(p.text)
                        .child(format!(
                            "Analyzing · {engine_label} {}-ply · {scope} · {}%{eta}",
                            job.plies, job.progress
                        )),
                )
                .child(
                    ActionButton::new(SharedString::from(format!("cancel-job-{jid}")))
                        .label(if cancelling {
                            "Cancelling…"
                        } else {
                            "Cancel"
                        })
                        .icon(ActionIconName::Cancel)
                        .danger()
                        .disabled(cancelling)
                        .palette(p)
                        .on_click(move |_, w, cx| on_cancel(jid, w, cx)),
                )
        }))
}
