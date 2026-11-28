//! Play match move history panel and version-pinned history review toolbar.
//!
//! Mirrors `PlayHistory.tsx`, `PlayHistoryReviewToolbar.tsx`, and `usePlayReview.ts`:
//! - Reuses `crate::review::move_history_row::MoveHistoryRow` and `PreviewBoardControls`.
//! - Maps `rated_decisions(&session.decisions)` preferring checker decisions over
//!   cube decisions at the same `history_index`.
//! - Renders `GameResultSummaryView` dividers after completed games.
//! - Version-pins history snapshot reads (`session_id`, `version`) so `Escape`
//!   always returns cleanly to the live board preserving uncommitted local steps.

use crate::i18n::current_locale;
use crate::review::move_history_row::{AssessmentSource, MoveAssessmentBadge, MoveHistoryRow};
use crate::review::preview_controls::{parse_view_mode, view_mode_id, view_mode_label};
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::icons::ActionIconName;
use crate::ui::text_nav::{TextNavBar, TextNavDensity, TextNavItem};
use crate::workspaces::play::dialogs::{GameResultSummaryView, PlayArgCallback, PlayVoidCallback};
use crate::workspaces::play::interruption::parse_play_severity;
use gpui_kit::{
    AnyElement, App, ClickEvent, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, TestSupportExt, base::StyledExt, div,
    prelude::FluentBuilder, px,
};
use neondojo::play::{PlayDecision, PlayHistorySnapshot, PlayHistorySnapshots, PlaySession};
use neondojo_board::BoardViewMode;
use neondojo_i18n::{I18n, format_decimal, format_int, format_ply, localize_decision};
use std::collections::HashMap;
use uuid::Uuid;

/// Version-pinned historical turn inspection state (`usePlayReview.ts`).
#[derive(Debug, Clone)]
pub struct PlayHistoryReviewState {
    selected: Option<(Uuid, i64, usize)>,
    view: BoardViewMode,
    cache: Option<PlayHistorySnapshots>,
    loading_key: Option<(Uuid, i64)>,
    error: Option<(Uuid, i64, String)>,
}

impl Default for PlayHistoryReviewState {
    fn default() -> Self {
        Self {
            selected: None,
            view: BoardViewMode::Arrows,
            cache: None,
            loading_key: None,
            error: None,
        }
    }
}

impl PlayHistoryReviewState {
    /// Returns true if a history entry belonging to the current session/version is selected.
    pub fn is_active(&self, session: Option<&PlaySession>) -> bool {
        self.selected_index(session).is_some()
    }

    pub fn selected_index(&self, session: Option<&PlaySession>) -> Option<usize> {
        let session = session?;
        let (id, ver, idx) = self.selected?;
        if id == session.id && ver == session.version && idx < session.history.len() {
            Some(idx)
        } else {
            None
        }
    }

    pub fn view_mode(&self) -> BoardViewMode {
        self.view
    }

    /// Only checker turns support arrow overlays; cube/resign turns fall back to `After`.
    pub fn effective_view_mode(&self, session: Option<&PlaySession>) -> BoardViewMode {
        let is_checker = self
            .entry(session)
            .is_some_and(|snapshot| snapshot.kind == "checker");
        if !is_checker && self.view == BoardViewMode::Arrows {
            BoardViewMode::After
        } else {
            self.view
        }
    }

    pub fn set_view_mode(&mut self, mode: BoardViewMode) {
        self.view = mode;
    }

    pub fn select(&mut self, session: &PlaySession, index: usize) -> bool {
        if index >= session.history.len() {
            return false;
        }
        self.selected = Some((session.id, session.version, index));
        true
    }

    /// Exits history review without touching live uncommitted checker steps.
    pub fn reset(&mut self) -> bool {
        self.selected.take().is_some()
    }

    pub fn has_cached_for(&self, session: &PlaySession) -> bool {
        self.cache.as_ref().is_some_and(|c| {
            c.session_id == session.id
                && c.version == session.version
                && c.items.len() == session.history.len()
        })
    }

    pub fn mark_loading(&mut self, session: &PlaySession) {
        self.loading_key = Some((session.id, session.version));
        self.error = None;
    }

    pub fn set_snapshots(&mut self, snapshots: PlayHistorySnapshots) {
        self.loading_key = None;
        self.error = None;
        self.cache = Some(snapshots);
    }

    pub fn set_error(&mut self, session_id: Uuid, version: i64, message: String) {
        self.loading_key = None;
        self.error = Some((session_id, version, message));
    }

    pub fn is_loading(&self, session: Option<&PlaySession>) -> bool {
        let Some(session) = session else {
            return false;
        };
        self.selected_index(Some(session)).is_some()
            && self.entry(Some(session)).is_none()
            && self.loading_key == Some((session.id, session.version))
    }

    pub fn error_message(&self, session: Option<&PlaySession>) -> Option<&str> {
        let session = session?;
        let (id, ver, msg) = self.error.as_ref()?;
        if *id == session.id && *ver == session.version && self.is_active(Some(session)) {
            Some(msg.as_str())
        } else {
            None
        }
    }

    pub fn entry(&self, session: Option<&PlaySession>) -> Option<&PlayHistorySnapshot> {
        let session = session?;
        let idx = self.selected_index(Some(session))?;
        let cache = self.cache.as_ref()?;
        if cache.session_id == session.id && cache.version == session.version {
            cache.items.get(idx)
        } else {
            None
        }
    }
}

fn is_checker_decision(decision: &PlayDecision) -> bool {
    matches!(
        decision.original_action.as_deref(),
        Some("move") | Some("pass")
    )
}

/// Maps each `history_index` to its rating decision, preferring checker play
/// over cube decision on the same row (`PlayHistory.tsx::ratedDecisions`).
pub fn rated_decisions(decisions: &[PlayDecision]) -> HashMap<usize, &PlayDecision> {
    let mut rated: HashMap<usize, &PlayDecision> = HashMap::new();
    for decision in decisions {
        if decision.first_loss.is_none() {
            continue;
        }
        let idx = decision.history_index;
        let replace = match rated.get(&idx) {
            None => true,
            Some(existing) => is_checker_decision(decision) && !is_checker_decision(existing),
        };
        if replace {
            rated.insert(idx, decision);
        }
    }
    rated
}

/// Short display engine label matching `shortEngineLabel`.
pub fn short_engine_label(engine_version: Option<&str>) -> String {
    let Some(raw) = engine_version else {
        return "GNUbg".to_string();
    };
    let lower = raw.to_ascii_lowercase();
    if lower.contains("neongammon") || lower.contains("bgsage") || lower.contains("sage") {
        "NeonGammon".to_string()
    } else if lower.contains("gnubg") || lower.contains("gnu") {
        "GNUbg".to_string()
    } else {
        raw.split_whitespace().next().unwrap_or("GNUbg").to_string()
    }
}

/// Callbacks for [`render_play_history_panel`].
#[derive(Clone)]
pub struct PlayHistoryCallbacks {
    pub on_select: PlayArgCallback<usize>,
    pub on_export_sgf: PlayVoidCallback,
    pub on_open_learning_review: PlayVoidCallback,
    pub on_open_study_report: PlayArgCallback<Uuid>,
}

/// Renders the scrollable left-hand move history column.
#[allow(clippy::too_many_arguments)]
pub fn render_play_history_panel(
    session: &PlaySession,
    white_name: &str,
    black_name: &str,
    selected_index: Option<usize>,
    disabled: bool,
    show_decisions: bool,
    show_review_actions: bool,
    paused: bool,
    palette: NeonPalette,
    cb: PlayHistoryCallbacks,
    cx: &App,
) -> AnyElement {
    let locale = current_locale(cx);
    let p = palette;
    let current = selected_index.or_else(|| session.history.len().checked_sub(1));
    let rated = if show_decisions {
        rated_decisions(&session.decisions)
    } else {
        HashMap::new()
    };
    let results: HashMap<usize, &neondojo::model::GameResult> = session
        .game_results
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|entry| (entry.history_index, &entry.result))
        .collect();
    let engine_label = short_engine_label(
        session
            .engine_snapshots
            .as_ref()
            .and_then(|s| s.analysis.engine_version.as_deref()),
    );
    let depth_label = format_ply(locale, session.analysis_plies.max(1) as u32);

    let mut list = div()
        .id("play-history-list")
        .test_support()
        .v_flex()
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .gap(px(4.0))
        .pr(px(4.0));

    for (index, turn) in session.history.iter().enumerate() {
        let decision = rated.get(&index).copied();
        let assessment = decision.and_then(|d| {
            let loss = d.loss.or(d.first_loss)?;
            let sev_str = d.severity.as_deref().or(d.first_severity.as_deref())?;
            Some(MoveAssessmentBadge {
                equity_loss: loss,
                severity: parse_play_severity(Some(sev_str)),
                source: AssessmentSource::Engine,
                engine: SharedString::from(engine_label.clone()),
                depth: Some(SharedString::from(depth_label.clone())),
            })
        });
        let player_u8 = u8::try_from(turn.player).unwrap_or(0).min(1);
        let pname = if player_u8 == 0 {
            white_name
        } else {
            black_name
        };
        let dice: Vec<u8> = turn
            .dice
            .iter()
            .filter_map(|&d| u8::try_from(d).ok().filter(|&v| (1..=6).contains(&v)))
            .collect();
        let on_select = cb.on_select.clone();
        let mut row = MoveHistoryRow::new(
            SharedString::from(format!("play-history-row-{index}")),
            format!("{}", index + 1),
            player_u8,
            pname.to_string(),
            turn.notation.clone(),
        )
        .dice(dice)
        .assessment(assessment)
        .selected(current == Some(index))
        .palette(p);

        if !disabled {
            row = row.on_click(move |_: &ClickEvent, window, cx| on_select(index, window, cx));
        }

        let corrected_note = decision.and_then(|d| {
            let orig = d.original_notation.as_deref()?;
            let final_n = d.notation.as_deref()?;
            if orig == final_n {
                return None;
            }
            let loss_part = d
                .first_loss
                .map(|l| format!(" · {}", format_decimal(locale, l, 3, 3)))
                .unwrap_or_default();
            Some(format!(
                "{}: {}{}",
                I18n::t(locale, "play-mode", "first"),
                localize_decision(locale, orig),
                loss_part
            ))
        });

        let game_result = results.get(&index).copied();

        let item = div()
            .v_flex()
            .gap(px(4.0))
            .child(row)
            .when_some(corrected_note, |this, note| {
                this.child(
                    div()
                        .pl(px(36.0))
                        .text_micro()
                        .tabular_nums()
                        .text_color(p.muted)
                        .child(note),
                )
            })
            .when_some(game_result, |this, res| {
                this.child(
                    GameResultSummaryView::new(
                        SharedString::from(format!("play-history-result-{index}")),
                        res.clone(),
                        white_name.to_string(),
                        black_name.to_string(),
                    )
                    .palette(p),
                )
            });

        list = list.child(item);
    }

    let on_export = cb.on_export_sgf.clone();
    let on_review = cb.on_open_learning_review.clone();
    let on_study = cb.on_open_study_report.clone();
    let study_match_id = session.library_match_id;

    div()
        .id("play-history-panel")
        .test_support()
        .v_flex()
        .w(px(310.0))
        .h_full()
        .flex_shrink_0()
        .gap(px(8.0))
        .p(px(12.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .pb(px(6.0))
                .border_b_1()
                .border_color(p.border)
                .child(
                    div()
                        .text_panel()
                        .text_color(p.text)
                        .child(I18n::t(locale, "play", "history")),
                )
                .child(
                    div()
                        .text_meta()
                        .tabular_nums()
                        .text_color(p.muted)
                        .child(format_int(locale, session.history.len() as i64)),
                ),
        )
        .child(list)
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(6.0))
                .pt(px(8.0))
                .border_t_1()
                .border_color(p.border)
                .child(
                    ActionButton::new("play-history-export-sgf")
                        .icon(ActionIconName::Export)
                        .tooltip(I18n::t(locale, "play", "export"))
                        .compact()
                        .disabled(disabled)
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| on_export(window, cx)),
                )
                .when(show_review_actions, |this| {
                    this.child(
                        ActionButton::new("play-history-learning-review")
                            .icon(ActionIconName::Analysis)
                            .tooltip(I18n::t(locale, "play-mode", "reviewTooltip"))
                            .label(I18n::t(locale, "play-mode", "review"))
                            .disabled(disabled || (paused && session.review.is_none()))
                            .palette(p)
                            .on_click(move |_: &ClickEvent, window, cx| on_review(window, cx)),
                    )
                    .when_some(study_match_id, |inner, mid| {
                        inner.child(
                            ActionButton::new("play-history-open-study-report")
                                .icon(ActionIconName::Open)
                                .tooltip(I18n::t(locale, "study", "open"))
                                .compact()
                                .disabled(disabled)
                                .palette(p)
                                .on_click(move |_: &ClickEvent, window, cx| {
                                    on_study(mid, window, cx);
                                }),
                        )
                    })
                }),
        )
        .into_any_element()
}

/// Renders the version-pinned board-top review toolbar (`PlayHistoryReviewToolbar.tsx`).
#[allow(clippy::too_many_arguments)]
pub fn render_play_history_review_toolbar(
    review: &PlayHistoryReviewState,
    session: &PlaySession,
    palette: NeonPalette,
    on_return: PlayVoidCallback,
    on_mode: PlayArgCallback<BoardViewMode>,
    on_retry: PlayVoidCallback,
    cx: &App,
) -> Option<AnyElement> {
    let index = review.selected_index(Some(session))?;
    let locale = current_locale(cx);
    let p = palette;
    let entry = review.entry(Some(session));
    let view = review.effective_view_mode(Some(session));
    let loading = review.is_loading(Some(session));
    let error = review.error_message(Some(session)).map(str::to_string);

    let mut heading = format!(
        "{} {}",
        I18n::t(locale, "play", "reviewing"),
        format_int(locale, (index + 1) as i64)
    );
    if let Some(snap) = entry {
        heading.push_str(&format!(
            " · {} {}",
            I18n::t(locale, "play", "game"),
            format_int(locale, i64::from(snap.game_no))
        ));
    }

    let mode_strip = entry.map(|snap| {
        let mut items = vec![
            TextNavItem::new("before", view_mode_label(locale, BoardViewMode::Before))
                .element_id("play-review-mode-before")
                .tooltip(format!(
                    "{} (←)",
                    view_mode_label(locale, BoardViewMode::Before)
                )),
        ];
        if snap.kind == "checker" {
            items.push(
                TextNavItem::new("arrows", view_mode_label(locale, BoardViewMode::Arrows))
                    .element_id("play-review-mode-arrows"),
            );
        }
        items.push(
            TextNavItem::new("after", view_mode_label(locale, BoardViewMode::After))
                .element_id("play-review-mode-after")
                .tooltip(format!(
                    "{} (→)",
                    view_mode_label(locale, BoardViewMode::After)
                )),
        );
        let on_mode = on_mode.clone();
        TextNavBar::new("play-history-review-modes", items)
            .active(Some(view_mode_id(view)))
            .density(TextNavDensity::Compact)
            .palette(p)
            .on_select(move |id, window, cx| {
                if let Some(next) = parse_view_mode(id) {
                    on_mode(next, window, cx);
                }
            })
    });

    Some(
        div()
            .id("play-history-review-toolbar")
            .test_support()
            .v_flex()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.accent)
            .bg(p.surface)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_panel()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text)
                            .child(heading),
                    )
                    .child(
                        ActionButton::new("play-review-return-live")
                            .icon(ActionIconName::Resume)
                            .label(I18n::t(locale, "play", "returnToGame"))
                            .shortcut("Escape")
                            .primary()
                            .palette(p)
                            .on_click(move |_: &ClickEvent, window, cx| on_return(window, cx)),
                    ),
            )
            .when(loading, |this| {
                this.child(div().text_meta().text_color(p.muted).child(I18n::t(
                    locale,
                    "play",
                    "loadingHistory",
                )))
            })
            .when_some(error, |this, err| {
                let on_retry = on_retry.clone();
                this.child(
                    div()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .px(px(10.0))
                        .py(px(6.0))
                        .rounded(px(6.0))
                        .bg(p.danger_bg)
                        .text_color(p.danger_text)
                        .child(
                            div().text_meta().child(format!(
                                "{} {err}",
                                I18n::t(locale, "play", "historyError")
                            )),
                        )
                        .child(
                            ActionButton::new("play-review-retry")
                                .icon(ActionIconName::Refresh)
                                .label(I18n::t(locale, "play", "retry"))
                                .palette(p)
                                .on_click(move |_: &ClickEvent, window, cx| on_retry(window, cx)),
                        ),
                )
            })
            .when_some(mode_strip, |this, strip| this.child(strip))
            .into_any_element(),
    )
}
