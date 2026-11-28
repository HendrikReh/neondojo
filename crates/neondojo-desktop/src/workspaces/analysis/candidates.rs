//! Overview / Checker Inspector sub-views:
//! 1. `CandidateTable` (10 dense columns + Best/Played/Preview badges + depth,
//!    `MoveExplanationView`, collapsible **Opening Book** supporting both
//!    BackgammonDB checker+cube and XG `.ob` pre-roll evaluations).
//! 2. `PositionComparison` (**Compare moves**: 3-engine XG / GNUbg / NeonGammon
//!    alignment matrix with `|Δ|` sort toggle, rank delta badges, route preview).
//! 3. `DiceEquityView` (**6×6 Matrix** driven by `DiceTabState`: auto-start on
//!    missing, never auto-retry on failure, silent after explicit delete until
//!    manual recalculate, cell click projects hypothetical dice onto `BoardView`).
//! 4. `ObservedRollsView` (**Game / Match Rolls**: 21 unordered pair frequency
//!    bars with weight-1 doubles vs weight-2 non-doubles expected markers).

use super::candidate_data::{
    CheckerCandidate, board_key, build_candidate_compare_rows, comparison_options,
    extract_candidates,
};
use super::state::{
    Action2Cb, ActionCb, CheckerSubTab, ClickCb, CompareEngineSlot, DiceDistributionSnapshot,
    DiceTabState, same_roll,
};
use crate::i18n::current_locale;
use crate::review::move_explanation::{MoveExplanationView, explain_checker_plays};
use crate::theme::{
    FONT_SIZE_MICRO, FONT_SIZE_TABLE, NeonPalette, NeonTheme, NeonTypographyExt,
    tabular_nums_features,
};
use crate::ui::action_button::ActionButton;
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::icons::ActionIconName;
use crate::ui::modal::{ModalConfig, ModalSize, open_modal_dialog};
use crate::ui::text_nav::{TextNavBar, TextNavDensity, TextNavItem};
use gpui_kit::{
    App, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt, Window,
    base::StyledExt,
    component::{WindowExt, tooltip::Tooltip},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::{
    analysis_catalog::{AnalysisCatalogEntry, SelectedPositionAnalysis},
    api::MatchAnalysisEngine,
    model::{DiceStats, Position},
    opening_book::BookLookup,
};
use neondojo_i18n::{
    I18n, format_decimal, format_equity, format_percent, format_ply, localize_decision,
};
use uuid::Uuid;

/// Callbacks wired from `AnalysisWorkspace` into the Overview / Checker pane.
#[derive(Clone)]
pub struct CheckerCallbacks {
    pub on_select_subtab: ActionCb<CheckerSubTab>,
    pub on_preview_candidate: ActionCb<String>,
    pub on_recalculate_position: ClickCb,
    pub on_toggle_opening_book: ClickCb,
    pub on_toggle_compare_sort_delta: ActionCb<CompareEngineSlot>,
    pub on_pick_compare_slot: Action2Cb<CompareEngineSlot, Option<String>>,
    pub on_dice_cell_click: Action2Cb<[u8; 2], Option<String>>,
    pub on_dice_manual_retry: ClickCb,
    pub on_dice_recalculate_dialog: ClickCb,
    pub on_dice_delete: Action2Cb<Uuid, Uuid>,
    pub on_toggle_observed_scope: ActionCb<bool>,
    pub on_retry_observed_rolls: ClickCb,
}

/// Snapshot of Overview / Checker sub-view state.
pub struct CheckerViewProps<'a> {
    pub active_subtab: CheckerSubTab,
    pub position: Option<&'a Position>,
    pub position_result: Option<&'a SelectedPositionAnalysis>,
    pub position_result_loading: bool,
    pub previewed_notation: Option<&'a str>,
    pub active_engine: MatchAnalysisEngine,
    pub opening_book: Option<&'a BookLookup>,
    pub opening_book_expanded: bool,
    pub opening_book_loading: bool,
    pub compare_slots: [&'a [CheckerCandidate]; 3],
    pub compare_slot_ids: [&'a Option<String>; 3],
    pub catalog_items: &'a [AnalysisCatalogEntry],
    pub catalog_selected: Option<&'a AnalysisCatalogEntry>,
    pub compare_sort_delta: Option<CompareEngineSlot>,
    pub dice_state: &'a DiceTabState,
    pub selected_dice_cell: Option<[u8; 2]>,
    pub observed_rolls: Option<&'a DiceStats>,
    pub observed_rolls_game_only: bool,
    pub observed_rolls_error: Option<&'a str>,
    pub callbacks: CheckerCallbacks,
    pub palette: NeonPalette,
}

/// Renders the Overview / Checker inspector panel with its 4 sub-tabs.
pub fn render_checker_inspector(
    props: CheckerViewProps<'_>,
    window: &Window,
    cx: &App,
) -> impl IntoElement {
    let p = props.palette;
    let sub_items = vec![
        TextNavItem::new(CheckerSubTab::Candidates.id(), "Candidates")
            .element_id("checker-subtab-candidates"),
        TextNavItem::new(CheckerSubTab::Compare.id(), "Compare moves")
            .element_id("checker-subtab-compare"),
        TextNavItem::new(CheckerSubTab::Dice.id(), "Dice equity").element_id("checker-subtab-dice"),
        TextNavItem::new(
            CheckerSubTab::Observed.id(),
            if props.observed_rolls_game_only {
                "Game rolls"
            } else {
                "Match rolls"
            },
        )
        .element_id("checker-subtab-observed"),
    ];

    let on_sub = props.callbacks.on_select_subtab.clone();
    let sub_nav = TextNavBar::new("analysis-checker-subtabs", sub_items)
        .active(Some(props.active_subtab.id()))
        .density(TextNavDensity::Compact)
        .palette(p)
        .on_select(move |id, w, cx| {
            if let Some(st) = CheckerSubTab::parse(id) {
                on_sub(st, w, cx);
            }
        });

    let body = match props.active_subtab {
        CheckerSubTab::Candidates => render_candidates_subtab(&props, cx).into_any_element(),
        CheckerSubTab::Compare => render_compare_subtab(&props, window, cx).into_any_element(),
        CheckerSubTab::Dice => render_dice_subtab(&props, cx).into_any_element(),
        CheckerSubTab::Observed => render_observed_rolls_subtab(&props, cx).into_any_element(),
    };

    div()
        .id("analysis-checker-inspector")
        .test_support()
        .v_flex()
        .flex_1()
        .h_full()
        .gap(px(10.0))
        .child(
            div()
                .pb(px(4.0))
                .border_b_1()
                .border_color(p.border)
                .child(sub_nav),
        )
        .child(body)
}

fn render_candidates_subtab(props: &CheckerViewProps<'_>, cx: &App) -> impl IntoElement {
    let locale = current_locale(cx);
    let p = props.palette;
    let on_recalc = props.callbacks.on_recalculate_position.clone();

    let Some(pos) = props.position else {
        return div()
            .id("candidates-empty-position")
            .test_support()
            .text_meta()
            .text_color(p.muted)
            .child("Select a checker move to inspect candidate evaluations.")
            .into_any_element();
    };

    let candidates = props
        .position_result
        .map(|result| extract_candidates(result, pos.player, None))
        .unwrap_or_default();
    let best = candidates.iter().find(|c| c.rank() == 1);
    let mixed_units = candidates
        .first()
        .is_some_and(|first| candidates.iter().any(|c| c.unit != first.unit));
    let played_key = board_key(pos, &pos.notation);

    let header = div()
        .h_flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(
            div()
                .text_meta()
                .text_color(p.muted)
                .child(match props.position_result {
                    Some(res) => format!(
                        "{}{}{}",
                        res.engine,
                        res.depth
                            .as_deref()
                            .map(|d| format!(" · {d}"))
                            .unwrap_or_default(),
                        candidates
                            .first()
                            .map(|c| format!(
                                " · {}",
                                if mixed_units {
                                    I18n::t(locale, "candidate", "mixedUnits")
                                } else {
                                    c.unit_label(locale)
                                }
                            ))
                            .unwrap_or_default()
                    ),
                    None if props.position_result_loading => "Loading evaluation…".to_string(),
                    None => "No evaluation for selected analysis".to_string(),
                }),
        )
        .child(
            ActionButton::new("recalculate-position-btn")
                .label("Recalculate")
                .icon(ActionIconName::Refresh)
                .palette(p)
                .on_click(move |ev, w, cx| on_recalc(ev, w, cx)),
        );

    let table = if candidates.is_empty() {
        div()
            .id("candidates-unavailable-msg")
            .test_support()
            .p(px(12.0))
            .rounded(px(6.0))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .text_meta()
            .text_color(p.muted)
            .child(if props.position_result_loading {
                "Loading candidate moves…"
            } else {
                "No candidate evaluations recorded in the selected analysis. Click Recalculate or run Match Analysis."
            })
            .into_any_element()
    } else {
        let mut container = div()
            .id("analysis-candidate-table")
            .test_support()
            .v_flex()
            .w_full()
            .rounded(px(6.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .overflow_hidden();

        // 10 dense columns header: #, Move, Equity, Loss, W, WG, WBG, L, LG, LBG
        container =
            container.child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(4.0))
                    .px(px(8.0))
                    .py(px(5.0))
                    .bg(p.surface_alt)
                    .border_b_1()
                    .border_color(p.border)
                    .text_table_heading()
                    .text_color(p.muted)
                    .child(div().w(px(22.0)).child("#"))
                    .child(div().flex_1().min_w(px(110.0)).child(I18n::t(
                        locale,
                        "app",
                        "common.move",
                    )))
                    .child(div().w(px(56.0)).text_right().child("Equity"))
                    .child(div().w(px(52.0)).text_right().child(I18n::t(
                        locale,
                        "candidate",
                        "loss",
                    )))
                    .child(div().w(px(42.0)).text_right().child("W"))
                    .child(div().w(px(40.0)).text_right().child("WG"))
                    .child(div().w(px(38.0)).text_right().child("WBG"))
                    .child(div().w(px(42.0)).text_right().child("L"))
                    .child(div().w(px(40.0)).text_right().child("LG"))
                    .child(div().w(px(38.0)).text_right().child("LBG")),
            );

        for (ix, cand) in candidates.iter().enumerate() {
            let loss = cand.loss(best);
            let is_best = cand.rank() == 1;
            let is_played = board_key(pos, cand.preview()) == played_key;
            let is_preview = props
                .previewed_notation
                .is_some_and(|n| n.eq_ignore_ascii_case(cand.preview()));
            let notation_click = cand.preview().to_owned();
            let on_preview = props.callbacks.on_preview_candidate.clone();

            let outcomes = cand.outcomes();
            let prob_cell = |idx: usize| -> String {
                outcomes[idx]
                    .map(|v| format_percent(locale, v, 1))
                    .unwrap_or_else(|| "—".to_string())
            };

            container = container.child(
                div()
                    .id(SharedString::from(format!("candidate-row-{ix}")))
                    .test_support()
                    .h_flex()
                    .items_center()
                    .gap(px(4.0))
                    .px(px(8.0))
                    .py(px(5.0))
                    .border_b_1()
                    .border_color(p.border)
                    .bg(if is_preview {
                        p.selected
                    } else if is_played {
                        p.surface_alt
                    } else {
                        p.surface
                    })
                    .cursor_pointer()
                    .text_table()
                    .tabular_nums()
                    .on_click(move |_, w, cx| on_preview(notation_click.clone(), w, cx))
                    .child(
                        div()
                            .w(px(22.0))
                            .text_color(p.muted)
                            .child(cand.rank().to_string()),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(4.0))
                            .flex_1()
                            .min_w(px(110.0))
                            .flex_wrap()
                            .child(
                                div()
                                    .font_weight(if is_best || is_played {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .text_color(p.text)
                                    .child(cand.notation.clone()),
                            )
                            .when(is_best, |el| {
                                el.child(
                                    div()
                                        .px(px(4.0))
                                        .rounded(px(3.0))
                                        .bg(p.success_bg)
                                        .text_micro()
                                        .text_color(p.success_text)
                                        .child("Best"),
                                )
                            })
                            .when(is_played, |el| {
                                el.child(
                                    div()
                                        .px(px(4.0))
                                        .rounded(px(3.0))
                                        .bg(p.selected)
                                        .text_micro()
                                        .text_color(p.accent)
                                        .child("Played"),
                                )
                            })
                            .when(is_preview, |el| {
                                el.child(
                                    div()
                                        .px(px(4.0))
                                        .rounded(px(3.0))
                                        .bg(p.accent)
                                        .text_micro()
                                        .text_color(p.accent_text)
                                        .child("Preview"),
                                )
                            })
                            .child({
                                div()
                                    .id(SharedString::from(format!("candidate-depth-{ix}")))
                                    .test_support()
                                    .text_micro()
                                    .text_color(p.muted)
                                    .child(cand.depth.clone().unwrap_or_else(|| "—".into()))
                            }),
                    )
                    .child(
                        div()
                            .w(px(56.0))
                            .text_right()
                            .text_color(p.text)
                            .id(SharedString::from(format!("candidate-equity-{ix}")))
                            .test_support()
                            .v_flex()
                            .child(cand.equity_text(locale, 3))
                            .when(mixed_units, |el| {
                                el.child(div().text_micro().child(cand.unit_label(locale)))
                            }),
                    )
                    .child(
                        div()
                            .w(px(52.0))
                            .text_right()
                            .text_color(if loss.is_some_and(|v| v > 0.0005) {
                                p.danger_text
                            } else {
                                p.muted
                            })
                            .id(SharedString::from(format!("candidate-loss-{ix}")))
                            .test_support()
                            .child(cand.difference_text(locale, loss, 3, false)),
                    )
                    .child(
                        div()
                            .w(px(42.0))
                            .text_right()
                            .text_color(p.text)
                            .child(prob_cell(0)),
                    )
                    .child(
                        div()
                            .w(px(40.0))
                            .text_right()
                            .text_color(p.muted)
                            .child(prob_cell(1)),
                    )
                    .child(
                        div()
                            .w(px(38.0))
                            .text_right()
                            .text_color(p.muted)
                            .child(prob_cell(2)),
                    )
                    .child(
                        div()
                            .w(px(42.0))
                            .text_right()
                            .text_color(p.text)
                            .child(prob_cell(3)),
                    )
                    .child(
                        div()
                            .w(px(40.0))
                            .text_right()
                            .text_color(p.muted)
                            .child(prob_cell(4)),
                    )
                    .child(
                        div()
                            .w(px(38.0))
                            .text_right()
                            .text_color(p.muted)
                            .child(prob_cell(5)),
                    ),
            );
        }
        container.into_any_element()
    };

    // Synchronous MoveExplanationView comparing best vs played/previewed play
    let explanation_view = best.and_then(|best| {
        let compare_notation = props.previewed_notation.unwrap_or(pos.notation.as_str());
        if compare_notation
            .trim()
            .eq_ignore_ascii_case(best.preview().trim())
        {
            return None;
        }
        let board_arr: [i32; 26] = pos.board.as_slice().try_into().ok()?;
        let dice_arr: [i32; 2] = pos.dice.as_slice().try_into().ok()?;
        let expl = explain_checker_plays(
            board_arr,
            pos.player,
            dice_arr,
            best.preview(),
            compare_notation,
        )
        .ok()?;
        let best_chances = best.explanation_chances();
        let played_chances: Option<[f64; 6]> = candidates
            .iter()
            .find(|c| {
                c.preview()
                    .trim()
                    .eq_ignore_ascii_case(compare_notation.trim())
            })
            .and_then(CheckerCandidate::explanation_chances);
        Some(
            MoveExplanationView::from_explanation(
                "analysis-move-explanation",
                best.notation.clone(),
                compare_notation.to_string(),
                &expl,
                best_chances,
                played_chances,
            )
            .palette(p),
        )
    });

    // Collapsible Opening Book section (BackgammonDB + XG .ob)
    let opening_book_section = render_opening_book_section(
        props.opening_book,
        props.opening_book_expanded,
        props.opening_book_loading,
        &props.callbacks,
        p,
        cx,
    );

    div()
        .v_flex()
        .gap(px(10.0))
        .child(header)
        .child(table)
        .when_some(explanation_view, |el, ev| el.child(ev))
        .child(opening_book_section)
        .into_any_element()
}

fn render_opening_book_section(
    lookup: Option<&BookLookup>,
    expanded: bool,
    loading: bool,
    callbacks: &CheckerCallbacks,
    p: NeonPalette,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let on_toggle = callbacks.on_toggle_opening_book.clone();
    let hit_badge = match lookup {
        Some(b) if b.hit => "Hit",
        Some(_) => "Miss",
        None if loading => "Checking…",
        None => "—",
    };

    let mut box_el = div()
        .id("analysis-opening-book-section")
        .test_support()
        .v_flex()
        .rounded(px(6.0))
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
        .child(
            div()
                .id("analysis-opening-book-toggle")
                .test_support()
                .h_flex()
                .items_center()
                .justify_between()
                .px(px(10.0))
                .py(px(7.0))
                .cursor_pointer()
                .on_click(move |ev, w, cx| on_toggle(ev, w, cx))
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .child(I18n::t(locale, "analysis-source", "book")),
                )
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().text_micro().text_color(p.muted).child(hit_badge))
                        .child(div().text_micro().text_color(p.accent).child(if expanded {
                            "Hide"
                        } else {
                            "Show"
                        })),
                ),
        );

    if !expanded {
        return box_el;
    }

    let content = match lookup {
        None if loading => div()
            .p(px(10.0))
            .text_meta()
            .text_color(p.muted)
            .child("Looking up opening book…")
            .into_any_element(),
        None => div()
            .p(px(10.0))
            .text_meta()
            .text_color(p.muted)
            .child(I18n::t(locale, "analysis-source", "noBook"))
            .into_any_element(),
        Some(book) => {
            let mut body = div()
                .v_flex()
                .gap(px(8.0))
                .p(px(10.0))
                .border_t_1()
                .border_color(p.border);

            if !book.hit {
                body = body.child(
                    div().text_meta().text_color(p.muted).child(
                        book.reason
                            .clone()
                            .unwrap_or_else(|| "Position not found in opening book.".to_string()),
                    ),
                );
            }

            if let Some(prov) = &book.provenance {
                body = body.child(div().text_micro().text_color(p.muted).child(format!(
                    "{} · {} ({}) · {}",
                    prov.provider, prov.engine, prov.evaluation_kind, prov.context
                )));
            }

            if let Some(checker) = &book.checker {
                let best_eq = checker.candidates.first().map(|c| c.equity).unwrap_or(0.0);
                body = body.child(div().v_flex().gap(px(3.0)).children(
                    checker.candidates.iter().take(8).enumerate().map(|(i, c)| {
                        let loss = (best_eq - c.equity).max(0.0);
                        let notation = c.notation.clone();
                        let on_prev = callbacks.on_preview_candidate.clone();
                        div()
                            .id(SharedString::from(format!("book-cand-{i}")))
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .px(px(6.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .bg(p.surface_alt)
                            .cursor_pointer()
                            .text_table()
                            .tabular_nums()
                            .on_click(move |_, w, cx| on_prev(notation.clone(), w, cx))
                            .child(format!("{}. {}", c.rank, c.notation))
                            .child(format!(
                                "{} (Δ -{})",
                                format_equity(locale, c.equity),
                                format_decimal(locale, loss, 3, 3)
                            ))
                    }),
                ));
            }

            if let Some(cube) = &book.cube {
                body = body.child(
                    div()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .px(px(6.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .bg(p.surface_alt)
                        .text_meta()
                        .tabular_nums()
                        .child(format!("Book cube: {}", cube.verdict))
                        .child(format!(
                            "ND {} · DT {} · DP {}",
                            format_equity(locale, cube.equities.no_double),
                            format_equity(locale, cube.equities.double_take),
                            format_equity(locale, cube.equities.double_pass),
                        )),
                );
            }

            if let Some(xg) = &book.xg_position {
                body = body.child(
                    div()
                        .v_flex()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_micro()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.muted)
                                .child(format!(
                                    "XG Opening Book {} (pre-roll reference)",
                                    xg.book_version
                                )),
                        )
                        .children(xg.evaluations.iter().map(|ev| {
                            let author = if ev.source_author.trim().is_empty() {
                                I18n::t(locale, "analysis-source", "authorUnknown")
                            } else {
                                ev.source_author.as_str()
                            };
                            let rollout_info = ev
                                .rollout
                                .as_ref()
                                .map(|r| format!(" · {} games", r.games))
                                .unwrap_or_default();
                            div()
                                .h_flex()
                                .items_center()
                                .justify_between()
                                .px(px(6.0))
                                .py(px(3.0))
                                .rounded(px(4.0))
                                .bg(p.surface_alt)
                                .text_micro()
                                .tabular_nums()
                                .child(format!(
                                    "{} {} · {author}{rollout_info}",
                                    ev.engine, ev.depth
                                ))
                                .child(format_equity(locale, ev.equity))
                        })),
                );
            }

            body.into_any_element()
        }
    };

    box_el = box_el.child(content);
    box_el
}

// ── 3-Engine Compare Moves Matrix ────────────────────────────────────────────

fn render_compare_subtab(
    props: &CheckerViewProps<'_>,
    window: &Window,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let p = props.palette;

    let Some(pos) = props.position else {
        return div()
            .text_meta()
            .text_color(p.muted)
            .child("Select a checker move to compare engines.")
            .into_any_element();
    };

    let rows = build_candidate_compare_rows(pos, props.compare_slots, props.compare_sort_delta);
    let toolbar = div().h_flex().flex_wrap().gap(px(6.0)).children(
        CompareEngineSlot::ALL
            .into_iter()
            .enumerate()
            .map(|(si, slot)| {
                let matching =
                    comparison_options(props.catalog_items, props.catalog_selected, pos, slot);
                let active_id = props.compare_slot_ids[si].as_deref();
                let next_id = if matching.is_empty() {
                    None
                } else {
                    let cur_idx = active_id
                        .and_then(|id| matching.iter().position(|e| e.id == id))
                        .unwrap_or(0);
                    Some(matching[(cur_idx + 1) % matching.len()].id.clone())
                };
                let current_desc = active_id
                    .and_then(|id| matching.iter().find(|e| e.id == id))
                    .or_else(|| matching.first())
                    .map(|e| {
                        format!(
                            "{}: {}",
                            slot.label(),
                            e.depth.as_deref().unwrap_or(&e.engine)
                        )
                    })
                    .unwrap_or_else(|| format!("{}: —", slot.label()));
                let on_pick = props.callbacks.on_pick_compare_slot.clone();

                ActionButton::new(SharedString::from(format!("compare-slot-pick-{si}")))
                    .label(current_desc)
                    .disabled(matching.len() <= 1)
                    .palette(p)
                    .on_click(move |_, w, cx| on_pick(slot, next_id.clone(), w, cx))
            }),
    );

    if rows.is_empty() {
        return div()
            .id("analysis-compare-empty")
            .test_support()
            .v_flex()
            .gap(px(8.0))
            .child(toolbar)
            .child(
                div()
                    .p(px(12.0))
                    .rounded(px(6.0))
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border)
                    .text_meta()
                    .text_color(p.muted)
                    .child("Run GNUbg, NeonGammon, or import XG evaluations to compare moves side by side."),
            )
            .into_any_element();
    }

    let played = I18n::t(locale, "app", "common.played");
    // Measure the visible notation and its badge in the fonts used below. Unusually
    // long moves can wrap while the numeric columns keep the same start in every row.
    let text_width = |text: &str, size: f32, weight| {
        let mut style = window.text_style();
        style.font_weight = weight;
        style.font_features = tabular_nums_features();
        text.lines()
            .map(|line| {
                window
                    .text_system()
                    .shape_line(
                        line.to_owned().into(),
                        px(size),
                        &[style.to_run(line.len())],
                        None,
                    )
                    .width
            })
            .fold(px(0.), Pixels::max)
    };
    let badge_width = text_width(played, FONT_SIZE_MICRO, FontWeight::NORMAL) + px(12.);
    let move_width = rows
        .iter()
        .map(|row| {
            let weight = if row.played {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            };
            text_width(row.display_notation, FONT_SIZE_TABLE, weight)
                + if row.played { badge_width } else { px(0.) }
        })
        .fold(px(48.), Pixels::max)
        .ceil()
        .min(px(216.));

    let rank_label = I18n::t(locale, "app", "analysis.rank");
    let equity_label = I18n::t(locale, "app", "common.equity");
    let loss_label = I18n::t(locale, "candidate", "loss");
    let depth_label = I18n::t(locale, "app", "analysis.depth");
    let has_xg_comparison = !props.compare_slots[0].is_empty()
        && props.compare_slots[1..].iter().any(|list| !list.is_empty());
    let best = props
        .compare_slots
        .map(|list| list.iter().find(|candidate| candidate.rank() == 1));
    let mixed_units = props.compare_slots.map(|list| {
        list.first()
            .is_some_and(|first| list.iter().any(|c| c.unit != first.unit))
    });
    let engine_width = px(130.);
    let mut table = div()
        .id("analysis-compare-matrix")
        .test_support()
        .v_flex()
        .self_start()
        .w(move_width + engine_width * 3. + px(36.))
        .flex_shrink_0()
        .rounded(px(6.0))
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
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
                        .id("compare-move-header")
                        .test_support()
                        .w(move_width)
                        .flex_shrink_0()
                        .min_w_0()
                        .child(I18n::t(locale, "app", "common.move")),
                )
                .children(
                    CompareEngineSlot::ALL
                        .into_iter()
                        .enumerate()
                        .map(|(si, slot)| {
                            let source = props.compare_slots[si].first().map(|c| {
                                if mixed_units[si] {
                                    I18n::t(locale, "candidate", "mixedUnits")
                                } else {
                                    c.unit_label(locale)
                                }
                            });
                            let on_sort = props.callbacks.on_toggle_compare_sort_delta.clone();
                            div()
                                .w(engine_width)
                                .flex_shrink_0()
                                .v_flex()
                                .child(
                                    div()
                                        .h_flex()
                                        .items_center()
                                        .gap(px(4.))
                                        .child(slot.label())
                                        .when(si != 0, |el| {
                                            el.child(
                                                ActionButton::new(SharedString::from(format!(
                                                    "compare-sort-delta-{si}"
                                                )))
                                                .dense_nav()
                                                .label("|Δ|")
                                                .tooltip(I18n::format(
                                                    locale,
                                                    "candidate",
                                                    "sortByXgDifference",
                                                    &[("engine", slot.label().into())],
                                                ))
                                                .active(props.compare_sort_delta == Some(slot))
                                                .disabled(
                                                    !rows.iter().any(|row| row.delta(si).is_some()),
                                                )
                                                .palette(p)
                                                .on_click(move |_, w, cx| on_sort(slot, w, cx)),
                                            )
                                        }),
                                )
                                .when_some(source, |el, unit| {
                                    el.child(div().text_micro().child(unit))
                                })
                                .child(
                                    div()
                                        .text_micro()
                                        .child(format!("{rank_label} · {equity_label}")),
                                )
                        }),
                ),
        );

    for (ix, row) in rows.iter().enumerate() {
        let on_prev = props.callbacks.on_preview_candidate.clone();
        let default_preview = row
            .slots
            .iter()
            .flatten()
            .next()
            .expect("each row has a candidate")
            .preview()
            .to_owned();
        let ref_rank = row.slots[0].and_then(|candidate| candidate.xg_equity_rank);
        table = table.child(
            div()
                .id(SharedString::from(format!("compare-row-{ix}")))
                .test_support()
                .h_flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(5.0))
                .border_b_1()
                .border_color(p.border)
                .bg(if row.played { p.surface_alt } else { p.surface })
                .text_table()
                .tabular_nums()
                .cursor_pointer()
                .on_click(move |_, w, cx| on_prev(default_preview.clone(), w, cx))
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(4.0))
                        .id(SharedString::from(format!("compare-move-{ix}")))
                        .test_support()
                        .w(move_width)
                        .flex_shrink_0()
                        .min_w_0()
                        .flex_wrap()
                        .child(
                            div()
                                .font_weight(if row.played {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::NORMAL
                                })
                                .text_color(p.text)
                                .child(row.display_notation.to_owned()),
                        )
                        .when(row.played, |el| {
                            el.child(
                                div()
                                    .px(px(4.0))
                                    .rounded(px(3.0))
                                    .bg(p.selected)
                                    .text_micro()
                                    .text_color(p.accent)
                                    .child(played),
                            )
                        }),
                )
                .children(row.slots.iter().enumerate().map(|(si, slot)| {
                    let cell = div()
                        .id(SharedString::from(format!("compare-cell-{ix}-{si}")))
                        .test_support()
                        .v_flex()
                        .w(engine_width)
                        .flex_shrink_0();
                    let Some(candidate) = slot else {
                        return cell
                            .text_color(p.muted)
                            .child("—")
                            .on_click(|_, _, cx| cx.stop_propagation());
                    };
                    let field = |name: &str, text: String| {
                        div()
                            .id(SharedString::from(format!("compare-{name}-{ix}-{si}")))
                            .test_support()
                            .child(text)
                    };
                    let rank = if si == 0 {
                        ref_rank
                    } else {
                        Some(candidate.rank)
                    };
                    let arrow = match (si, ref_rank) {
                        (1 | 2, Some(xg)) if candidate.rank < xg => " ↑",
                        (1 | 2, Some(xg)) if candidate.rank > xg => " ↓",
                        _ => "",
                    };
                    let rank_text = rank.map_or_else(|| "—".into(), |r| format!("#{r}{arrow}"));
                    let on_preview = props.callbacks.on_preview_candidate.clone();
                    let preview = candidate.preview().to_owned();
                    cell.cursor_pointer()
                        .on_click(move |_, w, cx| {
                            cx.stop_propagation();
                            on_preview(preview.clone(), w, cx)
                        })
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap(px(5.))
                                .child(field("rank", rank_text).text_color(p.muted))
                                .child(
                                    field("equity", candidate.equity_text(locale, 6))
                                        .font_weight(FontWeight::MEDIUM),
                                ),
                        )
                        .when(mixed_units[si], |el| {
                            el.child(div().text_micro().child(candidate.unit_label(locale)))
                        })
                        .child(
                            field(
                                "loss",
                                format!(
                                    "{loss_label} {}",
                                    candidate.difference_text(
                                        locale,
                                        candidate.loss(best[si]),
                                        6,
                                        false
                                    )
                                ),
                            )
                            .text_micro()
                            .text_color(p.muted),
                        )
                        .child(
                            field(
                                "depth",
                                candidate.depth.clone().unwrap_or_else(|| "—".into()),
                            )
                            .text_micro()
                            .tooltip(move |w, cx| Tooltip::new(depth_label).build(w, cx)),
                        )
                        .when(si != 0 && has_xg_comparison, |el| {
                            el.child(
                                field(
                                    "delta",
                                    format!(
                                        "Δ {}",
                                        candidate.difference_text(locale, row.delta(si), 6, true)
                                    ),
                                )
                                .text_micro()
                                .text_color(p.muted),
                            )
                        })
                })),
        );
    }

    div()
        .v_flex()
        .gap(px(8.0))
        .child(toolbar)
        .child(
            div()
                .id("analysis-compare-scroll")
                .max_w_full()
                .overflow_x_scroll()
                .child(table),
        )
        .into_any_element()
}

// ── Dice Equity 6×6 Matrix ───────────────────────────────────────────────────

fn render_dice_subtab(props: &CheckerViewProps<'_>, cx: &App) -> impl IntoElement {
    let p = props.palette;
    let on_retry = props.callbacks.on_dice_manual_retry.clone();
    let on_recalc = props.callbacks.on_dice_recalculate_dialog.clone();

    let Some(pos) = props.position else {
        return div()
            .text_meta()
            .text_color(p.muted)
            .child("Select a checker position to compute the 6×6 dice equity matrix.")
            .into_any_element();
    };

    let engine_name = match props.active_engine {
        MatchAnalysisEngine::Gnubg => "GNUbg",
        MatchAnalysisEngine::Bgsage => "NeonGammon",
    };

    let header = div()
        .h_flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(
            div()
                .text_meta()
                .text_color(p.muted)
                .child(format!("Dice Equity (6×6) · {engine_name}")),
        )
        .child(
            div().h_flex().gap(px(6.0)).child(
                ActionButton::new("dice-recalc-dialog-btn")
                    .label("Recalculate")
                    .icon(ActionIconName::Dice)
                    .palette(p)
                    .on_click(move |ev, w, cx| on_recalc(ev, w, cx)),
            ),
        );

    let body = match props.dice_state {
        DiceTabState::Idle => div()
            .id("dice-state-idle")
            .test_support()
            .p(px(12.0))
            .rounded(px(6.0))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .text_meta()
            .text_color(p.muted)
            .child("Preparing dice equity calculation…")
            .into_any_element(),

        DiceTabState::DeletedAwaitingExplicitRequest { .. } => {
            let retry_cb = on_retry.clone();
            div()
                .id("dice-state-deleted")
                .test_support()
                .v_flex()
                .gap(px(8.0))
                .p(px(12.0))
                .rounded(px(6.0))
                .bg(p.surface)
                .border_1()
                .border_color(p.border)
                .child(div().text_body().text_color(p.muted).child(
                    "Saved dice equity calculation was deleted. Click below to calculate again.",
                ))
                .child(
                    ActionButton::new("dice-calculate-after-delete-btn")
                        .label("Calculate dice equity")
                        .primary()
                        .palette(p)
                        .on_click(move |ev, w, cx| retry_cb(ev, w, cx)),
                )
                .into_any_element()
        }

        DiceTabState::Submitting { .. } => div()
            .id("dice-state-submitting")
            .test_support()
            .p(px(12.0))
            .rounded(px(6.0))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .text_meta()
            .text_color(p.muted)
            .child("Starting 21-roll dice equity job…")
            .into_any_element(),

        DiceTabState::Running {
            progress, status, ..
        } => div()
            .id("dice-state-running")
            .test_support()
            .p(px(12.0))
            .rounded(px(6.0))
            .bg(p.selected)
            .border_1()
            .border_color(p.border)
            .text_meta()
            .tabular_nums()
            .text_color(p.text)
            .child(format!(
                "Calculating 21 dice rolls ({status}) · {progress}%"
            ))
            .into_any_element(),

        DiceTabState::Failed { error, .. } => {
            let retry_cb = on_retry.clone();
            StatusBanner::new("dice-state-failed", BannerKind::Error, error.clone())
                .title("Dice equity failed")
                .palette(p)
                .on_retry("Retry", move |ev, w, cx| retry_cb(ev, w, cx))
                .into_any_element()
        }

        DiceTabState::Ready {
            position_id,
            job_id,
            distribution,
            ..
        } => render_dice_matrix(
            pos,
            *position_id,
            *job_id,
            distribution,
            props.selected_dice_cell,
            &props.callbacks,
            p,
            cx,
        )
        .into_any_element(),
    };

    div()
        .id("analysis-dice-equity-view")
        .test_support()
        .v_flex()
        .gap(px(8.0))
        .when(
            !matches!(props.dice_state, DiceTabState::Ready { .. }),
            |el| el.child(header),
        )
        .child(body)
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_dice_matrix(
    position: &Position,
    position_id: Uuid,
    job_id: Option<Uuid>,
    dist: &DiceDistributionSnapshot,
    selected_cell: Option<[u8; 2]>,
    callbacks: &CheckerCallbacks,
    p: NeonPalette,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let actual_dice = <[i32; 2]>::try_from(position.dice.as_slice())
        .ok()
        .filter(|dice| dice.iter().all(|d| (1..=6).contains(d)));
    let selected = selected_cell
        .map(|dice| dice.map(i32::from))
        .or(actual_dice)
        .unwrap_or([1, 1]);
    let on_recalc = callbacks.on_dice_recalculate_dialog.clone();

    let summary_bar = div()
        .h_flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(px(6.0))
        .text_meta()
        .text_color(p.muted)
        .child(format!(
            "{} · {}",
            dist.engine,
            format_ply(locale, dist.plies as u32)
        ))
        .child(I18n::t(locale, "app", "analysis.combinations"))
        .child(
            div()
                .text_table()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text)
                .child(format!("Ø {}", format_equity(locale, dist.mean_equity))),
        )
        .child(
            div()
                .h_flex()
                .gap(px(4.0))
                .child(
                    ActionButton::new("dice-recalc-dialog-btn")
                        .icon(ActionIconName::Refresh)
                        .dense_nav()
                        .tooltip(I18n::t(locale, "app", "analysis.recalculateDice"))
                        .palette(p)
                        .on_click(move |ev, w, cx| on_recalc(ev, w, cx)),
                )
                .when_some(job_id, |el, jid| {
                    let on_del = callbacks.on_dice_delete.clone();
                    el.child(
                        ActionButton::new("dice-delete-btn")
                            .icon(ActionIconName::Delete)
                            .dense_nav()
                            .danger()
                            .tooltip(I18n::t(locale, "app", "analysis.deleteDice"))
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                open_delete_dice_confirm(
                                    position_id,
                                    jid,
                                    on_del.clone(),
                                    window,
                                    cx,
                                )
                            }),
                    )
                }),
        );

    let rolled_label = I18n::t(locale, "app", "analysis.rolled");
    let mut grid = div()
        .id("dice-equity-6x6-grid")
        .test_support()
        .v_flex()
        .w_full()
        .gap(px(3.0));

    for a in 1..=6 {
        let mut row = div().h_flex().gap(px(3.0));
        for b in 1..=6 {
            let roll = dist.find_roll(a, b);
            let actual = actual_dice.is_some_and(|dice| same_roll(dice, [a, b]));
            let selected = same_roll(selected, [a, b]);
            let (bg, fg) = match roll {
                Some(r) if r.best.equity >= dist.mean_equity => (p.success_bg, p.success_text),
                Some(_) => (p.warning_bg, p.warning_text),
                None => (p.surface_alt, p.muted),
            };
            let equity = roll
                .map(|r| format_equity(locale, r.best.equity))
                .unwrap_or_else(|| "—".into());
            let notation = roll
                .map(|r| localize_decision(locale, &r.best.notation))
                .unwrap_or_else(|| "—".into());
            let preview = roll.map(|r| r.best.path.as_ref().unwrap_or(&r.best.notation).clone());
            let mut title = I18n::format(
                locale,
                "app",
                "analysis.rollTitle",
                &[
                    ("first", a.into()),
                    ("second", b.into()),
                    ("move", notation.into()),
                ],
            );
            if actual {
                title.push_str(&format!(" · {rolled_label}"));
            }
            let on_select = callbacks.on_dice_cell_click.clone();
            row = row.child(
                div()
                    .id(SharedString::from(format!("dice-cell-{a}-{b}")))
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .p(px(1.0))
                    .rounded(px(5.0))
                    .border_1()
                    .border_dashed()
                    .border_color(if actual {
                        p.text
                    } else {
                        gpui_kit::transparent_black()
                    })
                    .cursor_pointer()
                    .role(gpui_kit::accesskit::Role::Button)
                    .aria_toggled(selected.into())
                    .aria_label(format!("{title} · {equity}"))
                    .tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx))
                    .on_click(move |_, w, cx| on_select([a as u8, b as u8], preview.clone(), w, cx))
                    .child(
                        div()
                            .v_flex()
                            .items_center()
                            .py(px(4.0))
                            .gap(px(1.0))
                            .bg(bg)
                            .text_color(fg)
                            .rounded(px(3.0))
                            .border_1()
                            .border_color(if selected { p.accent } else { bg })
                            .child(div().text_micro().child(format!("{a} · {b}")))
                            .child(
                                div()
                                    .text_table()
                                    .font_weight(FontWeight::BOLD)
                                    .child(equity),
                            ),
                    ),
            );
        }
        grid = grid.child(row);
    }

    let detail = dist.find_roll(selected[0], selected[1]).map(|entry| {
        let [a, b] = entry.dice;
        let probability = format_percent(locale, entry.probability, 2);
        div()
            .id("dice-selected-roll-caption")
            .test_support()
            .h_flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.0))
            .text_table()
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .child(format!("{a}–{b}")),
            )
            .child(localize_decision(locale, &entry.best.notation))
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .child(format_equity(locale, entry.best.equity)),
            )
            .when(
                actual_dice.is_some_and(|dice| same_roll(dice, selected)),
                |el| el.child(div().text_micro().text_color(p.muted).child(rolled_label)),
            )
            .child(div().text_micro().text_color(p.muted).child(I18n::format(
                locale,
                "app",
                "analysis.probability",
                &[("value", probability.into())],
            )))
    });
    let mut legend = I18n::format(
        locale,
        "app",
        "analysis.distributionMethod",
        &[("plies", dist.plies.into())],
    );
    if actual_dice.is_some() {
        legend.push(' ');
        legend.push_str(I18n::t(locale, "app", "analysis.rolledLegend"));
    }
    div()
        .v_flex()
        .w_full()
        .max_w(px(660.0))
        .self_center()
        .gap(px(9.0))
        .child(summary_bar)
        .child(grid)
        .children(detail)
        .child(div().text_meta().text_color(p.muted).child(legend))
}

fn open_delete_dice_confirm(
    position_id: Uuid,
    job_id: Uuid,
    on_confirm: Action2Cb<Uuid, Uuid>,
    window: &mut Window,
    cx: &mut App,
) {
    let cfg = ModalConfig::new("Delete dice equity").size(ModalSize::Compact);
    open_modal_dialog(
        window,
        cx,
        cfg,
        move |_w: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            div()
                .text_body()
                .text_color(p.text)
                .child("Delete the stored 21-roll dice equity result for this position? It will stay cleared until you manually calculate again.")
                .into_any_element()
        },
        Some(move |_w: &mut Window, cx: &mut App| {
            let p = NeonTheme::current_palette(cx);
            let cb = on_confirm.clone();
            div()
                .h_flex()
                .gap(px(8.0))
                .justify_end()
                .child(
                    ActionButton::new("confirm-del-dice-cancel")
                        .label("Cancel")
                        .palette(p)
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .child(
                    ActionButton::new("confirm-del-dice-ok")
                        .label("Delete")
                        .danger_solid()
                        .palette(p)
                        .on_click(move |_, window, cx| {
                            cb(position_id, job_id, window, cx);
                            window.close_dialog(cx);
                        }),
                )
                .into_any_element()
        }),
        None,
    );
}

// ── Observed Game / Match Rolls (21 unordered pairs) ─────────────────────────

fn render_observed_rolls_subtab(props: &CheckerViewProps<'_>, cx: &App) -> impl IntoElement {
    let locale = current_locale(cx);
    let p = props.palette;
    let on_scope_game = props.callbacks.on_toggle_observed_scope.clone();
    let on_scope_match = props.callbacks.on_toggle_observed_scope.clone();
    let on_retry = props.callbacks.on_retry_observed_rolls.clone();

    let scope_bar = div()
        .h_flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(
            div()
                .h_flex()
                .gap(px(6.0))
                .child(
                    ActionButton::new("observed-scope-game")
                        .label("This game")
                        .active(props.observed_rolls_game_only)
                        .palette(p)
                        .on_click(move |_, w, cx| on_scope_game(true, w, cx)),
                )
                .child(
                    ActionButton::new("observed-scope-match")
                        .label("Whole match")
                        .active(!props.observed_rolls_game_only)
                        .palette(p)
                        .on_click(move |_, w, cx| on_scope_match(false, w, cx)),
                ),
        )
        .when_some(props.observed_rolls, |el, stats| {
            el.child(
                div()
                    .text_meta()
                    .tabular_nums()
                    .text_color(p.muted)
                    .child(format!("{} total rolls", stats.total)),
            )
        });

    if let Some(err) = props.observed_rolls_error {
        return div()
            .v_flex()
            .gap(px(8.0))
            .child(scope_bar)
            .child(
                StatusBanner::new("observed-rolls-error", BannerKind::Error, err.to_string())
                    .palette(p)
                    .on_retry("Retry", move |ev, w, cx| on_retry(ev, w, cx)),
            )
            .into_any_element();
    }

    let Some(stats) = props.observed_rolls else {
        return div()
            .v_flex()
            .gap(px(8.0))
            .child(scope_bar)
            .child(
                div()
                    .text_meta()
                    .text_color(p.muted)
                    .child("Loading observed dice frequencies…"),
            )
            .into_any_element();
    };

    let max_count = stats
        .rolls
        .iter()
        .map(|r| r.count)
        .max()
        .unwrap_or(1)
        .max(1) as f32;

    let bars =
        div()
            .id("analysis-observed-rolls-bars")
            .test_support()
            .v_flex()
            .gap(px(3.0))
            .p(px(8.0))
            .rounded(px(6.0))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .children(stats.rolls.iter().map(|r| {
                let is_double = r.dice[0] == r.dice[1];
                let weight = if is_double { 1 } else { 2 };
                let expected_count = (stats.total as f64) * r.expected_probability;
                let bar_frac = (r.count as f32 / max_count).clamp(0.0, 1.0);
                let bar_width = (bar_frac * 180.0).max(if r.count > 0 { 4.0 } else { 0.0 });
                let actual_pct = if stats.total > 0 {
                    r.count as f64 / stats.total as f64
                } else {
                    0.0
                };

                div()
                    .h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_table()
                    .tabular_nums()
                    .child(
                        div()
                            .w(px(34.0))
                            .font_weight(if is_double {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(p.text)
                            .child(format!("{}–{}", r.dice[0], r.dice[1])),
                    )
                    .child(
                        div()
                            .w(px(28.0))
                            .text_micro()
                            .text_color(p.muted)
                            .child(format!("{weight}/36")),
                    )
                    .child(
                        div()
                            .w(px(180.0))
                            .h(px(10.0))
                            .rounded(px(3.0))
                            .bg(p.surface_alt)
                            .overflow_hidden()
                            .child(div().h_full().w(px(bar_width)).rounded(px(3.0)).bg(
                                if is_double {
                                    p.accent
                                } else {
                                    p.control_border
                                },
                            )),
                    )
                    .child(
                        div()
                            .w(px(28.0))
                            .text_right()
                            .text_color(p.text)
                            .child(format!("{}", r.count)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_right()
                            .text_micro()
                            .text_color(p.muted)
                            .child(format!(
                                "{} (exp {:.1})",
                                format_percent(locale, actual_pct, 1),
                                expected_count
                            )),
                    )
            }));

    div()
        .v_flex()
        .gap(px(8.0))
        .child(scope_bar)
        .child(bars)
        .into_any_element()
}
