//! Study → Reports & AI Assessment sub-tab:
//! - Cursor-paginated report archive grouping money games into durable session cards
//! - 2-tab point-match report (`AI Assessment` + `Decisions & Attempts`, `Escape` closes)
//! - `PlayLearningReview`: first-attempt PR, keyboard-navigable decisions, attempt history
//! - `MatchAssessmentPanel`: subscribed to `facade.subscribe_assessment_updates()`,
//!   rendering **structured JSON sections** (NEVER Markdown or WebView!) with clickable
//!   `game.decision` ref chips opening archived board preview + `Open in Analysis`.

use super::common::{
    UiAction, UiCallback, UiCallback3, cube_eval_to_view_data, engine_display_name,
    play_mode_str_label, position_to_board_position, preview_for_notation, topic_name_label,
};
use crate::i18n::current_locale;
use crate::review::cube_details::CubeDecisionDetailsView;
use crate::review::move_explanation::{MoveExplanationView, explain_checker_plays};
use crate::review::preview_controls::PreviewBoardControls;
use crate::review::severity::{SeverityStyle, classify_equity_loss};
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::banners::{BannerKind, EmptyState, StatusBanner};
use crate::ui::icons::ActionIconName;
use crate::ui::text_nav::{TextNavBar, TextNavDensity, TextNavItem};
use gpui_kit::{
    App, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt, base::StyledExt, div,
    prelude::FluentBuilder, px,
};
use neondojo::match_analysis::Decision;
use neondojo::match_assessment::{AssessmentState, AssessmentStatus, Availability, Failure};
use neondojo::model::Position;
use neondojo::play::{PlayHistorySnapshots, PlayReview, PlaySession};
use neondojo::study::StudyReportSummary;
use neondojo_board::{BoardView, BoardViewMode};
use neondojo_i18n::{
    I18n, format_datetime_short_local, format_decimal, format_ply, format_pr, localize_decision,
};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

/// Grouped entry in the Study Reports archive (either a money-game session card
/// or a contiguous block of point-match rows).
#[derive(Clone)]
pub enum StudyReportGroup<'a> {
    MoneySession {
        session_id: Uuid,
        reports: Vec<&'a StudyReportSummary>,
    },
    PointMatches {
        id: Uuid,
        reports: Vec<&'a StudyReportSummary>,
    },
}

/// Groups money games (`match_length == 0`) by durable `session_id` (sorted by
/// `game_no` ascending) while keeping point matches individual (`study-report-groups.ts`).
pub fn group_study_reports<'a>(reports: &'a [StudyReportSummary]) -> Vec<StudyReportGroup<'a>> {
    let mut groups: Vec<StudyReportGroup<'a>> = Vec::new();
    let mut money_idx: HashMap<Uuid, usize> = HashMap::new();

    for report in reports {
        if report.match_length == 0 {
            if let Some(&idx) = money_idx.get(&report.session_id) {
                if let StudyReportGroup::MoneySession { reports: list, .. } = &mut groups[idx] {
                    list.push(report);
                }
            } else {
                let idx = groups.len();
                money_idx.insert(report.session_id, idx);
                groups.push(StudyReportGroup::MoneySession {
                    session_id: report.session_id,
                    reports: vec![report],
                });
            }
        } else if let Some(StudyReportGroup::PointMatches { reports: list, .. }) = groups.last_mut()
        {
            list.push(report);
        } else {
            groups.push(StudyReportGroup::PointMatches {
                id: report.id,
                reports: vec![report],
            });
        }
    }

    for group in &mut groups {
        if let StudyReportGroup::MoneySession { reports: list, .. } = group {
            list.sort_by_key(|r| r.game_no.unwrap_or(0));
        }
    }

    groups
}

/// Canonical decision key `"<game_no>:<decision_no>"` used by backend AI evidence refs.
pub fn review_decision_key(game_no: i32, decision_no: i32) -> String {
    format!("{game_no}:{decision_no}")
}

/// Parses an AI assessment evidence reference (`"2:14"` or `"2.14"`) into `(game_no, decision_no)`.
pub fn parse_evidence_ref(raw: &str) -> Option<(i32, i32)> {
    let trimmed = raw.trim();
    let (g, d) = trimmed
        .split_once(':')
        .or_else(|| trimmed.split_once('.'))?;
    let game_no: i32 = g.trim().parse().ok()?;
    let decision_no: i32 = d.trim().parse().ok()?;
    if game_no < 1 || decision_no < 1 {
        return None;
    }
    Some((game_no, decision_no))
}

/// Formats a backend evidence ref (`"2:14"`) as a compact clickable chip label (`"2.14"`).
pub fn format_evidence_ref_chip(raw: &str) -> String {
    match parse_evidence_ref(raw) {
        Some((g, d)) => format!("{g}.{d}"),
        None => raw.replace(':', "."),
    }
}

/// Single human decision entry inside a [`PlaySession`]'s learning review.
#[derive(Debug, Clone)]
pub struct ReviewDecisionEntry {
    pub key: String,
    pub game_no: i32,
    pub decision: Decision,
}

/// Resolved archived position + history index for a review decision.
#[derive(Debug, Clone)]
pub struct ReviewDecisionPosition {
    pub position: Position,
    pub history_index: usize,
}

/// Extracts all human decisions from `session.review` keyed by `"game_no:decision_no"`.
pub fn human_review_decisions(session: &PlaySession) -> Vec<ReviewDecisionEntry> {
    let Some(review) = &session.review else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for game in &review.games {
        for decision in &game.decisions {
            if decision.player == session.human_player {
                out.push(ReviewDecisionEntry {
                    key: review_decision_key(game.game_no, decision.decision_no),
                    game_no: game.game_no,
                    decision: decision.clone(),
                });
            }
        }
    }
    out
}

/// Resolves native decision ordinals against immutable archived source history
/// (`web/src/workspaces/study/reports/learning-review-positions.ts`).
pub fn build_learning_review_positions(
    review: &PlayReview,
    history: &PlayHistorySnapshots,
) -> BTreeMap<String, ReviewDecisionPosition> {
    let mut result = BTreeMap::new();
    for game in &review.games {
        let events: Vec<_> = history
            .items
            .iter()
            .filter(|item| item.game_no == game.game_no)
            .collect();
        let checkers: Vec<_> = events
            .iter()
            .copied()
            .filter(|item| item.kind == "checker")
            .collect();
        let mut cursor = 0usize;
        for decision in &game.decisions {
            let key = review_decision_key(game.game_no, decision.decision_no);
            let is_checker = decision.kind == "checker";
            let is_no_double =
                decision.kind == "cube" && decision.played.eq_ignore_ascii_case("no double");
            if is_checker || is_no_double {
                let Some(ordinal) = decision.checker_no.filter(|&o| o >= 1) else {
                    break;
                };
                let Some(&event) = checkers.get((ordinal - 1) as usize) else {
                    break;
                };
                if event.before.player != decision.player {
                    break;
                }
                let Some(at) = events.iter().position(|e| e.index == event.index) else {
                    break;
                };
                if at < cursor {
                    break;
                }
                if is_checker {
                    let Some([d0, d1]) = decision.dice else {
                        break;
                    };
                    let mut dec_dice = [d0, d1];
                    dec_dice.sort();
                    let mut ev_dice = event.before.dice.clone();
                    ev_dice.sort();
                    if ev_dice.len() != 2 || ev_dice != dec_dice {
                        break;
                    }
                    result.insert(
                        key,
                        ReviewDecisionPosition {
                            position: event.before.clone(),
                            history_index: event.index,
                        },
                    );
                    cursor = at + 1;
                } else {
                    let mut pos = event.before.clone();
                    pos.dice.clear();
                    result.insert(
                        key,
                        ReviewDecisionPosition {
                            position: pos,
                            history_index: event.index,
                        },
                    );
                    cursor = at;
                }
            } else if decision.kind == "cube" {
                let Some(&event) = events.get(cursor) else {
                    break;
                };
                if event.kind != "cube"
                    || event.before.player != decision.player
                    || !event.before.notation.eq_ignore_ascii_case(&decision.played)
                {
                    break;
                }
                result.insert(
                    key,
                    ReviewDecisionPosition {
                        position: event.before.clone(),
                        history_index: event.index,
                    },
                );
                cursor += 1;
            } else {
                break;
            }
        }
    }
    result
}

/// Active sub-tab inside an open point-match Study report.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ReportDetailTab {
    #[default]
    Assessment,
    Decisions,
}

impl ReportDetailTab {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Assessment => "assessment",
            Self::Decisions => "decisions",
        }
    }
}

/// Interactive state for Study → Reports & AI Assessment.
pub struct StudyReportsState {
    pub reports: Vec<StudyReportSummary>,
    pub next_cursor: Option<String>,
    pub loading: bool,
    pub loading_more: bool,
    pub error: Option<String>,

    // Open report detail view
    pub open_report_id: Option<Uuid>,
    pub open_session: Option<PlaySession>,
    pub open_loading: bool,
    pub open_error: Option<String>,
    pub detail_tab: ReportDetailTab,

    // AI Assessment state (pushed via subscribe_assessment_updates)
    pub assessment: Option<AssessmentState>,
    pub assessment_loading: bool,
    pub assessment_error: Option<String>,
    pub selected_ref_key: Option<String>,

    // Archived positions for board preview (shared by Assessment ref chips & Decisions list)
    pub positions: BTreeMap<String, ReviewDecisionPosition>,
    pub positions_loading: bool,
    pub positions_error: Option<String>,
    pub preview_mode: BoardViewMode,
    pub selected_decision_index: Option<usize>,
    pub expanded_attempt_key: Option<String>,
    pub preview_board_view: Entity<BoardView>,

    // Pending match_id requested via `StudyWorkspace::open_report_for_match`
    pub pending_open_match_id: Option<Uuid>,
}

impl StudyReportsState {
    pub fn new(preview_board_view: Entity<BoardView>) -> Self {
        Self {
            reports: Vec::new(),
            next_cursor: None,
            loading: false,
            loading_more: false,
            error: None,
            open_report_id: None,
            open_session: None,
            open_loading: false,
            open_error: None,
            detail_tab: ReportDetailTab::Assessment,
            assessment: None,
            assessment_loading: false,
            assessment_error: None,
            selected_ref_key: None,
            positions: BTreeMap::new(),
            positions_loading: false,
            positions_error: None,
            preview_mode: BoardViewMode::Arrows,
            selected_decision_index: None,
            expanded_attempt_key: None,
            preview_board_view,
            pending_open_match_id: None,
        }
    }

    pub fn close_open_report(&mut self) {
        self.open_report_id = None;
        self.open_session = None;
        self.open_loading = false;
        self.open_error = None;
        self.assessment = None;
        self.assessment_loading = false;
        self.assessment_error = None;
        self.selected_ref_key = None;
        self.positions.clear();
        self.positions_loading = false;
        self.positions_error = None;
        self.selected_decision_index = None;
        self.expanded_attempt_key = None;
    }

    /// Selects an evidence reference chip (`"game:decision"` or `"game.decision"`) and syncs board preview.
    pub fn select_evidence_ref(&mut self, reference: &str, cx: &mut App) -> bool {
        let canonical = match parse_evidence_ref(reference) {
            Some((g, d)) => review_decision_key(g, d),
            None => reference.to_string(),
        };
        let Some(session) = &self.open_session else {
            return false;
        };
        let entries = human_review_decisions(session);
        let Some(idx) = entries.iter().position(|e| e.key == canonical) else {
            return false;
        };
        self.selected_ref_key = Some(canonical);
        self.selected_decision_index = Some(idx);
        self.sync_preview_board(cx);
        true
    }

    /// Steps selected decision in `PlayLearningReview` via keyboard (`ArrowUp` / `ArrowDown`).
    pub fn step_decision(&mut self, delta: isize, cx: &mut App) -> bool {
        let Some(session) = &self.open_session else {
            return false;
        };
        let entries = human_review_decisions(session);
        if entries.is_empty() {
            return false;
        }
        let next_idx = match self.selected_decision_index {
            Some(cur) => (cur as isize + delta).clamp(0, (entries.len() - 1) as isize) as usize,
            None => 0,
        };
        self.selected_decision_index = Some(next_idx);
        self.selected_ref_key = Some(entries[next_idx].key.clone());
        self.sync_preview_board(cx);
        true
    }

    pub fn sync_preview_board(&self, cx: &mut App) {
        let Some(session) = &self.open_session else {
            return;
        };
        let Some(ref_key) = &self.selected_ref_key else {
            return;
        };
        let Some(resolved) = self.positions.get(ref_key) else {
            return;
        };
        let entry = human_review_decisions(session)
            .into_iter()
            .find(|e| &e.key == ref_key);

        let bot_name = engine_display_name(&session.engine.id);
        let (white, black) = if session.human_player == 0 {
            (session.human_name.clone(), bot_name.to_string())
        } else {
            (bot_name.to_string(), session.human_name.clone())
        };
        let bp = position_to_board_position(&resolved.position);
        let preview = entry
            .as_ref()
            .filter(|e| e.decision.kind == "checker")
            .and_then(|e| preview_for_notation(&resolved.position, &e.decision.best));
        let mode = if preview.is_some() {
            self.preview_mode
        } else {
            BoardViewMode::Before
        };

        self.preview_board_view.update(cx, |bv, cx| {
            bv.set_players(white, black, cx);
            bv.set_show_pip_counts(true, cx);
            bv.set_position(bp, cx);
            bv.set_interaction(None, cx);
            bv.set_preview(preview, mode, cx);
        });
    }
}

/// Callbacks wired from `StudyWorkspace` into the Reports renderer.
pub struct ReportsCallbacks {
    pub on_refresh: UiAction,
    pub on_load_more: UiAction,
    pub on_open_report: UiCallback<Uuid>,
    pub on_close_report: UiAction,
    pub on_select_detail_tab: UiCallback<ReportDetailTab>,
    pub on_start_assessment: UiAction,
    pub on_retry_assessment: UiAction,
    pub on_select_ref: UiCallback<String>,
    pub on_clear_ref: UiAction,
    pub on_preview_mode: UiCallback<BoardViewMode>,
    pub on_toggle_attempts: UiCallback<String>,
    pub on_open_in_analysis: UiCallback3<Uuid, Option<i32>, Option<i32>>,
}

/// Renders Study → Reports (archive list OR open 2-tab point-match report / learning review).
pub fn render_study_reports(
    state: &StudyReportsState,
    palette: NeonPalette,
    cb: ReportsCallbacks,
    cx: &App,
) -> impl IntoElement {
    if state.open_report_id.is_some() {
        return render_open_report(state, palette, cb, cx).into_any_element();
    }
    render_reports_archive(state, palette, cb, cx).into_any_element()
}

fn render_reports_archive(
    state: &StudyReportsState,
    palette: NeonPalette,
    cb: ReportsCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let on_ref = cb.on_refresh.clone();

    let mut root = div()
        .id("study-reports-archive")
        .test_support()
        .v_flex()
        .gap(px(12.0))
        .w_full()
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .v_flex()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_panel()
                                .text_color(palette.text)
                                .child(I18n::t(locale, "study", "archive")),
                        )
                        .child(div().text_meta().text_color(palette.muted).child(I18n::t(
                            locale,
                            "study",
                            "description",
                        ))),
                )
                .child(
                    ActionButton::new("study-reports-refresh")
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
            StatusBanner::new("study-reports-err", BannerKind::Error, err.clone())
                .palette(palette)
                .on_retry(I18n::t(locale, "study", "retry"), move |_, window, cx| {
                    on_retry(window, cx)
                }),
        );
    }

    if state.loading && state.reports.is_empty() {
        return root.child(
            div()
                .p(px(20.0))
                .text_body()
                .text_color(palette.muted)
                .child(I18n::t(locale, "study", "loading")),
        );
    }

    if state.reports.is_empty() {
        return root.child(
            EmptyState::new(
                "study-reports-empty",
                I18n::t(locale, "study", "empty"),
                I18n::t(locale, "study", "emptyHelp"),
            )
            .icon(ActionIconName::Analysis)
            .palette(palette),
        );
    }

    for group in group_study_reports(&state.reports) {
        match group {
            StudyReportGroup::MoneySession {
                session_id,
                reports,
            } => {
                let first = reports[0];
                let score_txt = format!(
                    "{}–{}",
                    first.session_scores.first().copied().unwrap_or(0),
                    first.session_scores.get(1).copied().unwrap_or(0)
                );
                let mut card = div()
                    .id(SharedString::from(format!("money-session-{session_id}")))
                    .test_support()
                    .v_flex()
                    .gap(px(8.0))
                    .p(px(12.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(palette.border)
                    .bg(palette.surface)
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(div().text_panel().text_color(palette.text).child(
                                        format!(
                                            "{} · {}",
                                            I18n::t(locale, "study", "moneySession"),
                                            first.name
                                        ),
                                    ))
                                    .child(
                                        div()
                                            .text_meta()
                                            .px(px(6.0))
                                            .py(px(2.0))
                                            .rounded(px(4.0))
                                            .bg(palette.surface_alt)
                                            .text_color(palette.muted)
                                            .child(I18n::t(
                                                locale,
                                                "study",
                                                if first.money_session_ended {
                                                    "ended"
                                                } else {
                                                    "ongoing"
                                                },
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .text_meta()
                                    .tabular_nums()
                                    .text_color(palette.muted)
                                    .child(format!(
                                        "{}: {score_txt} · {} {}",
                                        I18n::t(locale, "study", "totalScore"),
                                        reports.len(),
                                        I18n::t(locale, "study", "gameReports")
                                    )),
                            ),
                    );

                for rep in reports {
                    card = card.child(render_report_row(
                        rep,
                        palette,
                        cb.on_open_report.clone(),
                        cx,
                    ));
                }
                root = root.child(card);
            }
            StudyReportGroup::PointMatches { id, reports } => {
                let mut table = div()
                    .id(SharedString::from(format!("point-reports-{id}")))
                    .test_support()
                    .v_flex()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(palette.border)
                    .bg(palette.surface)
                    .overflow_hidden();

                for rep in reports {
                    table = table.child(render_report_row(
                        rep,
                        palette,
                        cb.on_open_report.clone(),
                        cx,
                    ));
                }
                root = root.child(table);
            }
        }
    }

    if state.next_cursor.is_some() {
        let on_more = cb.on_load_more.clone();
        root = root.child(
            div().h_flex().justify_center().child(
                ActionButton::new("study-reports-load-more")
                    .label(I18n::t(locale, "study", "more"))
                    .disabled(state.loading_more)
                    .palette(palette)
                    .on_click(move |_, window, cx| on_more(window, cx)),
            ),
        );
    }

    root
}

fn render_report_row(
    rep: &StudyReportSummary,
    palette: NeonPalette,
    on_open: UiCallback<Uuid>,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let rid = rep.id;
    let score = format!(
        "{}–{}",
        rep.scores.first().copied().unwrap_or(0),
        rep.scores.get(1).copied().unwrap_or(0)
    );
    let pr = rep
        .first_attempt_rating
        .map(|v| format_pr(locale, v))
        .unwrap_or_else(|| "—".into());
    let title = if let Some(g) = rep.game_no {
        format!("{} · #{}", rep.name, g)
    } else {
        format!("{} ({} pt)", rep.name, rep.match_length)
    };
    let hover_bg = palette.surface_alt;

    div()
        .id(SharedString::from(format!("study-report-row-{rid}")))
        .test_support()
        .h_flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .px(px(12.0))
        .py(px(8.0))
        .border_b_1()
        .border_color(palette.border)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .on_click(move |_, window, cx| on_open(rid, window, cx))
        .child(
            div()
                .v_flex()
                .gap(px(2.0))
                .flex_1()
                .min_w(px(0.0))
                .child(
                    div()
                        .text_body()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(palette.text)
                        .truncate()
                        .child(title),
                )
                .child(
                    div()
                        .text_meta()
                        .tabular_nums()
                        .text_color(palette.muted)
                        .child(format!(
                            "{} · {} · {} · {} · {}",
                            format_datetime_short_local(locale, rep.updated_at),
                            score,
                            play_mode_str_label(locale, &rep.play_mode),
                            I18n::t(
                                locale,
                                "play-mode",
                                if rep.assisted {
                                    "assisted"
                                } else {
                                    "independent"
                                }
                            ),
                            format_ply(locale, u32::try_from(rep.analysis_plies).unwrap_or(0)),
                        )),
                ),
        )
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .text_table()
                        .tabular_nums()
                        .text_color(palette.text)
                        .child(format!("PR {pr}")),
                )
                .child(
                    div()
                        .text_label()
                        .text_color(palette.accent)
                        .child(I18n::t(locale, "study", "open")),
                ),
        )
}

fn render_open_report(
    state: &StudyReportsState,
    palette: NeonPalette,
    cb: ReportsCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let on_close = cb.on_close_report.clone();

    let mut header = div()
        .h_flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .pb(px(10.0))
        .border_b_1()
        .border_color(palette.border)
        .child(
            ActionButton::new("study-report-back")
                .icon(ActionIconName::Back)
                .label(I18n::t(locale, "match-assessment", "back"))
                .shortcut("Escape")
                .palette(palette)
                .on_click(move |_, window, cx| on_close(window, cx)),
        );

    if let Some(session) = &state.open_session
        && let Some(match_id) = session.library_match_id
    {
        let on_open_analysis = cb.on_open_in_analysis.clone();
        header = header.child(
            ActionButton::new("study-report-open-analysis")
                .icon(ActionIconName::Analysis)
                .label(I18n::t(locale, "workspace", "analysis"))
                .palette(palette)
                .on_click(move |_, window, cx| on_open_analysis(match_id, None, None, window, cx)),
        );
    }

    let mut root = div()
        .id("study-report-detail")
        .test_support()
        .v_flex()
        .gap(px(12.0))
        .w_full()
        .child(header);

    if state.open_loading {
        return root.child(
            div()
                .p(px(20.0))
                .text_body()
                .text_color(palette.muted)
                .child(I18n::t(locale, "study", "loadingReport")),
        );
    }
    if let Some(err) = &state.open_error {
        return root.child(
            StatusBanner::new("study-report-open-err", BannerKind::Error, err.clone())
                .palette(palette),
        );
    }

    let Some(session) = &state.open_session else {
        return root;
    };

    let is_point_match = session.match_length > 0;
    if is_point_match {
        let on_tab = cb.on_select_detail_tab.clone();
        let tab_items = vec![
            TextNavItem::new(
                ReportDetailTab::Assessment.id(),
                I18n::t(locale, "match-assessment", "tab"),
            )
            .element_id("report-tab-assessment"),
            TextNavItem::new(
                ReportDetailTab::Decisions.id(),
                I18n::t(locale, "study", "decisions"),
            )
            .element_id("report-tab-decisions"),
        ];
        root = root.child(
            TextNavBar::new("study-report-detail-tabs", tab_items)
                .active(Some(state.detail_tab.id()))
                .density(TextNavDensity::Inline)
                .palette(palette)
                .on_select(move |id, window, cx| {
                    let tab = if id == ReportDetailTab::Decisions.id() {
                        ReportDetailTab::Decisions
                    } else {
                        ReportDetailTab::Assessment
                    };
                    on_tab(tab, window, cx);
                }),
        );
    }

    if is_point_match && state.detail_tab == ReportDetailTab::Assessment {
        root.child(render_match_assessment_panel(
            state, session, palette, cb, cx,
        ))
    } else {
        root.child(render_play_learning_review(state, session, palette, cb, cx))
    }
}

/// Renders structured JSON AI Match Assessment (`MatchAssessmentPanel`) — NEVER Markdown!
fn render_match_assessment_panel(
    state: &StudyReportsState,
    session: &PlaySession,
    palette: NeonPalette,
    cb: ReportsCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let entries: HashMap<String, ReviewDecisionEntry> = human_review_decisions(session)
        .into_iter()
        .map(|e| (e.key.clone(), e))
        .collect();

    let mut left = div()
        .id("study-match-assessment-panel")
        .test_support()
        .v_flex()
        .gap(px(14.0))
        .flex_1()
        .min_w(px(360.0));

    if session.match_forfeited {
        left = left.child(
            div()
                .text_meta()
                .text_color(palette.warning_text)
                .child(I18n::t(locale, "match-assessment", "forfeit")),
        );
    }

    if let Some(err) = &state.assessment_error {
        let on_retry = cb.on_retry_assessment.clone();
        left = left.child(
            StatusBanner::new("assessment-err", BannerKind::Error, err.clone())
                .palette(palette)
                .on_retry(I18n::t(locale, "study", "retry"), move |_, window, cx| {
                    on_retry(window, cx)
                }),
        );
    } else if state.assessment_loading && state.assessment.is_none() {
        left = left.child(div().text_body().text_color(palette.muted).child(I18n::t(
            locale,
            "match-assessment",
            "loading",
        )));
    } else if let Some(asmt_state) = &state.assessment {
        let active_job = asmt_state.job.as_ref().is_some_and(|j| {
            matches!(
                j.status,
                AssessmentStatus::Queued | AssessmentStatus::Running
            )
        });
        if active_job {
            let status_key = match asmt_state.job.as_ref().map(|j| &j.status) {
                Some(AssessmentStatus::Queued) => "queued",
                _ => "running",
            };
            left = left.child(
                StatusBanner::new(
                    "assessment-active-status",
                    BannerKind::Info,
                    I18n::t(locale, "match-assessment", status_key),
                )
                .palette(palette),
            );
        } else if let Some(output) = asmt_state.job.as_ref().and_then(|j| j.result.as_ref()) {
            // ── Structured JSON Sections (NOT Markdown) ─────────────────────
            let model_name = asmt_state
                .job
                .as_ref()
                .map(|j| j.model.as_str())
                .unwrap_or("");
            let ply_suffix = session
                .review
                .as_ref()
                .map(|r| {
                    format!(
                        " · {}",
                        format_ply(locale, u32::try_from(r.analysis_plies).unwrap_or(0))
                    )
                })
                .unwrap_or_default();
            left = left
                .child(div().text_meta().text_color(palette.muted).child(format!(
                    "{} · {model_name}{ply_suffix}",
                    I18n::t(locale, "match-assessment", "basis")
                )))
                .child(
                    div()
                        .id("assessment-summary")
                        .test_support()
                        .p(px(14.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(palette.border)
                        .bg(palette.surface)
                        .text_body()
                        .text_color(palette.text)
                        .child(output.summary.clone()),
                )
                .child(
                    div()
                        .id("assessment-next-focus")
                        .test_support()
                        .v_flex()
                        .gap(px(4.0))
                        .p(px(14.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(palette.border)
                        .bg(palette.surface)
                        .child(div().text_panel().text_color(palette.text).child(I18n::t(
                            locale,
                            "match-assessment",
                            "focus",
                        )))
                        .child(
                            div()
                                .text_body()
                                .text_color(palette.text)
                                .child(output.next_match_focus.clone()),
                        ),
                );

            // Topics section with clickable `game.decision` ref chips
            if !output.topics.is_empty() {
                let mut topics_box = div()
                    .id("assessment-topics")
                    .test_support()
                    .v_flex()
                    .gap(px(8.0))
                    .p(px(14.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(palette.border)
                    .bg(palette.surface)
                    .child(div().text_panel().text_color(palette.text).child(I18n::t(
                        locale,
                        "match-assessment",
                        "themes",
                    )));
                for (t_idx, topic) in output.topics.iter().enumerate() {
                    let mut chips = div().h_flex().flex_wrap().gap(px(6.0));
                    for (r_idx, ref_str) in topic.evidence_refs.iter().enumerate() {
                        if entries.contains_key(ref_str) {
                            let rk = ref_str.clone();
                            let on_ref = cb.on_select_ref.clone();
                            let active =
                                state.selected_ref_key.as_deref() == Some(ref_str.as_str());
                            chips = chips.child(
                                ActionButton::new(SharedString::from(format!(
                                    "assessment-topic-ref-{t_idx}-{r_idx}"
                                )))
                                .label(format_evidence_ref_chip(ref_str))
                                .active(active)
                                .palette(palette)
                                .on_click(move |_, window, cx| on_ref(rk.clone(), window, cx)),
                            );
                        }
                    }
                    topics_box = topics_box.child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .p(px(8.0))
                            .rounded(px(6.0))
                            .bg(palette.surface_alt)
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(palette.text)
                                    .child(topic_name_label(locale, topic.topic)),
                            )
                            .child(
                                div()
                                    .text_body()
                                    .text_color(palette.text)
                                    .child(topic.observation.clone()),
                            )
                            .child(chips),
                    );
                }
                left = left.child(topics_box);
            }

            // Key Decisions section
            if !output.key_decisions.is_empty() {
                let mut kd_box = div()
                    .id("assessment-key-decisions")
                    .test_support()
                    .v_flex()
                    .gap(px(8.0))
                    .p(px(14.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(palette.border)
                    .bg(palette.surface)
                    .child(div().text_panel().text_color(palette.text).child(I18n::t(
                        locale,
                        "match-assessment",
                        "examples",
                    )));
                for (idx, item) in output.key_decisions.iter().enumerate() {
                    let Some(details) = entries.get(&item.evidence_ref) else {
                        continue;
                    };
                    let d = &details.decision;
                    let rk = item.evidence_ref.clone();
                    let on_ref = cb.on_select_ref.clone();
                    kd_box = kd_box.child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .p(px(10.0))
                            .rounded(px(6.0))
                            .bg(palette.surface_alt)
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_label()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(palette.text)
                                            .child(format!(
                                                "{}.{} · {}",
                                                details.game_no,
                                                d.decision_no,
                                                I18n::t(
                                                    locale,
                                                    "match-assessment",
                                                    if d.kind == "cube" {
                                                        "cube"
                                                    } else {
                                                        "checker"
                                                    }
                                                )
                                            )),
                                    )
                                    .child(
                                        ActionButton::new(SharedString::from(format!(
                                            "assessment-kd-preview-{idx}"
                                        )))
                                        .icon(ActionIconName::View)
                                        .label(I18n::t(locale, "match-assessment", "preview"))
                                        .palette(palette)
                                        .on_click(
                                            move |_, window, cx| on_ref(rk.clone(), window, cx),
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .text_meta()
                                    .tabular_nums()
                                    .text_color(palette.muted)
                                    .child(format!(
                                        "{} → {} · Δ {} EMG",
                                        localize_decision(locale, &d.played),
                                        localize_decision(locale, &d.best),
                                        format_decimal(locale, d.equity_loss, 3, 3)
                                    )),
                            )
                            .child(
                                div()
                                    .text_body()
                                    .text_color(palette.text)
                                    .child(item.explanation.clone()),
                            )
                            .child(
                                div()
                                    .text_meta()
                                    .text_color(palette.muted)
                                    .child(item.next_question.clone()),
                            ),
                    );
                }
                left = left.child(kd_box);
            }

            // Training Exercises section
            if !output.training.is_empty() {
                let mut tr_box = div()
                    .id("assessment-training")
                    .test_support()
                    .v_flex()
                    .gap(px(8.0))
                    .p(px(14.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(palette.border)
                    .bg(palette.surface)
                    .child(div().text_panel().text_color(palette.text).child(I18n::t(
                        locale,
                        "match-assessment",
                        "training",
                    )));
                for (t_idx, tr) in output.training.iter().enumerate() {
                    let mut chips = div().h_flex().flex_wrap().gap(px(6.0));
                    for (r_idx, ref_str) in tr.evidence_refs.iter().enumerate() {
                        if entries.contains_key(ref_str) {
                            let rk = ref_str.clone();
                            let on_ref = cb.on_select_ref.clone();
                            chips = chips.child(
                                ActionButton::new(SharedString::from(format!(
                                    "assessment-training-ref-{t_idx}-{r_idx}"
                                )))
                                .label(format_evidence_ref_chip(ref_str))
                                .palette(palette)
                                .on_click(move |_, window, cx| on_ref(rk.clone(), window, cx)),
                            );
                        }
                    }
                    tr_box = tr_box.child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .p(px(8.0))
                            .rounded(px(6.0))
                            .bg(palette.surface_alt)
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(palette.text)
                                    .child(tr.focus.clone()),
                            )
                            .child(
                                div()
                                    .text_body()
                                    .text_color(palette.text)
                                    .child(tr.exercise.clone()),
                            )
                            .child(
                                div()
                                    .text_meta()
                                    .text_color(palette.muted)
                                    .child(tr.success_criterion.clone()),
                            )
                            .child(chips),
                    );
                }
                left = left.child(tr_box);
            }

            // Limitations section
            if !output.limitations.is_empty() {
                let mut lim_box = div()
                    .id("assessment-limitations")
                    .test_support()
                    .v_flex()
                    .gap(px(4.0))
                    .p(px(14.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(palette.border)
                    .bg(palette.surface)
                    .child(div().text_panel().text_color(palette.text).child(I18n::t(
                        locale,
                        "match-assessment",
                        "limitations",
                    )));
                for item in &output.limitations {
                    lim_box = lim_box.child(
                        div()
                            .text_meta()
                            .text_color(palette.muted)
                            .child(format!("• {item}")),
                    );
                }
                left = left.child(lim_box);
            }
        } else {
            // Idle / failed / unavailable state
            if let Some(fail) = asmt_state
                .job
                .as_ref()
                .filter(|j| matches!(j.status, AssessmentStatus::Failed))
                .and_then(|j| j.error.as_ref())
            {
                let key = match fail {
                    Failure::InvalidReport => "invalid",
                    Failure::Interrupted => "interrupted",
                    Failure::SourceUnavailable => "source",
                    Failure::NotConfigured => "configuration",
                    Failure::Provider => "failed",
                };
                left = left.child(
                    StatusBanner::new(
                        "assessment-failed-banner",
                        BannerKind::Error,
                        I18n::t(locale, "match-assessment", key),
                    )
                    .palette(palette),
                );
            }
            match asmt_state.availability {
                Availability::Ready => {
                    let on_start = cb.on_start_assessment.clone();
                    left = left.child(
                        ActionButton::new("assessment-create-btn")
                            .icon(ActionIconName::Analysis)
                            .label(I18n::t(locale, "match-assessment", "create"))
                            .primary()
                            .palette(palette)
                            .on_click(move |_, window, cx| on_start(window, cx)),
                    );
                }
                Availability::MissingAnalysis => {
                    left = left.child(div().text_body().text_color(palette.muted).child(I18n::t(
                        locale,
                        "match-assessment",
                        "missing",
                    )));
                }
                Availability::Unsupported => {
                    left = left.child(div().text_body().text_color(palette.muted).child(I18n::t(
                        locale,
                        "match-assessment",
                        "unsupported",
                    )));
                }
                Availability::NotConfigured => {
                    left = left.child(div().text_body().text_color(palette.muted).child(I18n::t(
                        locale,
                        "match-assessment",
                        "configuration",
                    )));
                }
            }
        }
    }

    let preview_panel = state.selected_ref_key.as_ref().and_then(|key| {
        entries
            .get(key)
            .map(|entry| render_archived_decision_preview(state, session, entry, palette, &cb, cx))
    });

    div()
        .h_flex()
        .items_start()
        .gap(px(16.0))
        .w_full()
        .child(left)
        .when_some(preview_panel, |this, panel| this.child(panel))
}

/// Renders Tab 2: `PlayLearningReview` (first-attempt PR, keyboard-navigable decisions, attempt history).
fn render_play_learning_review(
    state: &StudyReportsState,
    session: &PlaySession,
    palette: NeonPalette,
    cb: ReportsCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let entries = human_review_decisions(session);
    let review = session.review.as_ref();

    let mut summary_strip = div()
        .id("play-learning-review-summary")
        .test_support()
        .h_flex()
        .flex_wrap()
        .gap(px(12.0))
        .p(px(12.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface);

    if let Some(rev) = review {
        let pr_str = rev
            .first_attempt_rating
            .map(|v| format_pr(locale, v))
            .unwrap_or_else(|| "—".into());
        summary_strip = summary_strip
            .child(metric_pill(
                I18n::t(locale, "play-mode", "firstPr"),
                pr_str,
                palette,
            ))
            .child(metric_pill(
                I18n::t(locale, "play-mode", "totalLoss"),
                format_decimal(locale, rev.first_attempt_loss, 3, 3),
                palette,
            ))
            .child(metric_pill(
                I18n::t(locale, "play-mode", "attempts"),
                format!("{} (↻ {})", rev.first_attempt_count, rev.retry_count),
                palette,
            ))
            .child(metric_pill(
                I18n::t(locale, "play-mode", "analysisDepth"),
                format_ply(locale, u32::try_from(rev.analysis_plies).unwrap_or(0)),
                palette,
            ));
    }

    let mut decision_list = div()
        .id("play-learning-review-decisions")
        .test_support()
        .v_flex()
        .flex_1()
        .min_w(px(360.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .overflow_hidden();

    for (idx, entry) in entries.iter().enumerate() {
        let d = &entry.decision;
        let is_selected = state.selected_ref_key.as_deref() == Some(entry.key.as_str());
        let sev = classify_equity_loss(d.equity_loss, session.error_thresholds);
        let sev_style = SeverityStyle::resolve(sev, &palette);
        let rk = entry.key.clone();
        let on_sel = cb.on_select_ref.clone();
        let hover_bg = palette.surface_alt;

        // Match corresponding recorded PlayDecision attempts if present
        let matching_attempts = session.decisions.iter().find(|pd| {
            pd.game_no == entry.game_no
                && pd
                    .notation
                    .as_deref()
                    .is_some_and(|n| n.eq_ignore_ascii_case(&d.played))
                && !pd.attempts.is_empty()
        });
        let expanded = state.expanded_attempt_key.as_deref() == Some(entry.key.as_str());
        let rk_att = entry.key.clone();
        let on_toggle_att = cb.on_toggle_attempts.clone();

        let mut row = div()
            .id(SharedString::from(format!("learning-review-row-{idx}")))
            .test_support()
            .v_flex()
            .gap(px(4.0))
            .px(px(10.0))
            .py(px(7.0))
            .border_b_1()
            .border_color(palette.border)
            .bg(if is_selected {
                palette.selected
            } else {
                palette.surface
            })
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .on_click(move |_, window, cx| on_sel(rk.clone(), window, cx))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_meta()
                                    .tabular_nums()
                                    .text_color(palette.muted)
                                    .child(format!("{}.{}", entry.game_no, d.decision_no)),
                            )
                            .child(div().text_table().text_color(palette.text).child(format!(
                                "{} → {}",
                                localize_decision(locale, &d.played),
                                localize_decision(locale, &d.best)
                            ))),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(6.0))
                            .when(sev_style.is_error(), |this| {
                                this.child(
                                    div()
                                        .text_micro()
                                        .px(px(5.0))
                                        .py(px(1.0))
                                        .rounded(px(4.0))
                                        .bg(sev_style.bg)
                                        .text_color(sev_style.fg)
                                        .child(sev_style.label(locale)),
                                )
                            })
                            .child(
                                div()
                                    .text_table()
                                    .tabular_nums()
                                    .text_color(palette.muted)
                                    .child(format!(
                                        "−{}",
                                        format_decimal(locale, d.equity_loss, 3, 3)
                                    )),
                            )
                            .when_some(matching_attempts, |this, pd| {
                                this.child(
                                    ActionButton::new(SharedString::from(format!(
                                        "review-attempts-toggle-{idx}"
                                    )))
                                    .label(format!(
                                        "{} ({})",
                                        I18n::t(locale, "play-mode", "attempts"),
                                        pd.attempts.len()
                                    ))
                                    .variant(ActionButtonVariant::Ghost)
                                    .palette(palette)
                                    .on_click(
                                        move |_, window, cx| {
                                            cx.stop_propagation();
                                            on_toggle_att(rk_att.clone(), window, cx);
                                        },
                                    ),
                                )
                            }),
                    ),
            );

        if expanded && let Some(pd) = matching_attempts {
            let mut att_list = div().v_flex().gap(px(3.0)).pl(px(18.0)).pt(px(4.0));
            for (a_idx, att) in pd.attempts.iter().enumerate() {
                att_list = att_list.child(
                    div()
                        .text_meta()
                        .tabular_nums()
                        .text_color(palette.muted)
                        .child(format!(
                            "#{}: {} ({})",
                            a_idx + 1,
                            localize_decision(locale, &att.notation),
                            att.loss
                                .map(|l| format!("−{}", format_decimal(locale, l, 3, 3)))
                                .unwrap_or_else(|| "—".into())
                        )),
                );
            }
            row = row.child(att_list);
        }

        decision_list = decision_list.child(row);
    }

    let selected_entry = state
        .selected_ref_key
        .as_ref()
        .and_then(|key| entries.iter().find(|e| &e.key == key));

    div()
        .id("study-play-learning-review")
        .test_support()
        .v_flex()
        .gap(px(12.0))
        .w_full()
        .child(summary_strip)
        .child(
            div()
                .h_flex()
                .items_start()
                .gap(px(16.0))
                .w_full()
                .child(decision_list)
                .when_some(selected_entry, |this, entry| {
                    this.child(render_archived_decision_preview(
                        state, session, entry, palette, &cb, cx,
                    ))
                }),
        )
}

fn render_archived_decision_preview(
    state: &StudyReportsState,
    session: &PlaySession,
    entry: &ReviewDecisionEntry,
    palette: NeonPalette,
    cb: &ReportsCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let d = &entry.decision;
    let on_clear = cb.on_clear_ref.clone();
    let on_mode = cb.on_preview_mode.clone();

    let mut panel = div()
        .id("study-report-decision-preview")
        .test_support()
        .v_flex()
        .gap(px(10.0))
        .w(px(440.0))
        .flex_shrink_0()
        .p(px(12.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(div().text_panel().text_color(palette.text).child(format!(
                    "{} · {}.{}",
                    I18n::t(locale, "study", "decisionPreview"),
                    entry.game_no,
                    d.decision_no
                )))
                .child(
                    div()
                        .h_flex()
                        .gap(px(6.0))
                        .when_some(session.library_match_id, |this, match_id| {
                            let on_analysis = cb.on_open_in_analysis.clone();
                            let gno = entry.game_no;
                            let dno = d.decision_no;
                            this.child(
                                ActionButton::new("preview-open-in-analysis")
                                    .icon(ActionIconName::Analysis)
                                    .label(I18n::t(locale, "workspace", "analysis"))
                                    .palette(palette)
                                    .on_click(move |_, window, cx| {
                                        on_analysis(match_id, Some(gno), Some(dno), window, cx)
                                    }),
                            )
                        })
                        .child(
                            ActionButton::new("preview-close")
                                .icon(ActionIconName::Close)
                                .variant(ActionButtonVariant::Ghost)
                                .dense_nav()
                                .palette(palette)
                                .on_click(move |_, window, cx| on_clear(window, cx)),
                        ),
                ),
        );

    if state.positions_loading {
        panel = panel.child(div().text_meta().text_color(palette.muted).child(I18n::t(
            locale,
            "study",
            "loadingPosition",
        )));
    } else if let Some(resolved) = state.positions.get(&entry.key) {
        panel = panel
            .child(
                div()
                    .h(px(300.0))
                    .w_full()
                    .child(state.preview_board_view.clone()),
            )
            .when(d.kind == "checker", |this| {
                this.child(
                    PreviewBoardControls::new("report-preview-controls", state.preview_mode)
                        .palette(palette)
                        .on_mode(move |mode, window, cx| on_mode(mode, window, cx)),
                )
            });

        if d.kind == "checker"
            && resolved.position.board.len() == 26
            && resolved.position.dice.len() == 2
            && d.best != d.played
        {
            let mut board = [0i32; 26];
            board.copy_from_slice(&resolved.position.board);
            if let Ok(expl) = explain_checker_plays(
                board,
                resolved.position.player,
                [resolved.position.dice[0], resolved.position.dice[1]],
                &d.best,
                &d.played,
            ) {
                panel = panel.child(
                    MoveExplanationView::from_explanation(
                        "report-move-explanation",
                        &d.best,
                        &d.played,
                        &expl,
                        None,
                        None,
                    )
                    .palette(palette),
                );
            }
        } else if d.kind == "cube" {
            let bot_name = engine_display_name(&session.engine.id);
            let players = if session.human_player == 0 {
                [
                    SharedString::from(session.human_name.clone()),
                    SharedString::from(bot_name),
                ]
            } else {
                [
                    SharedString::from(bot_name),
                    SharedString::from(session.human_name.clone()),
                ]
            };
            let is_response = matches!(d.played.as_str(), "take" | "drop");
            panel = panel.child(
                CubeDecisionDetailsView::new(
                    "report-cube-details",
                    usize::try_from(d.player).unwrap_or(0),
                    is_response,
                    players,
                )
                .match_id(resolved.position.gnubg_match_id.clone())
                .evaluation(d.cube_evaluation.as_ref().map(cube_eval_to_view_data))
                .palette(palette),
            );
        }
    } else {
        panel = panel.child(div().text_meta().text_color(palette.muted).child(I18n::t(
            locale,
            "study",
            "positionUnavailable",
        )));
    }

    panel
}

fn metric_pill(label: &str, value: String, palette: NeonPalette) -> impl IntoElement {
    div()
        .v_flex()
        .gap(px(2.0))
        .px(px(10.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .bg(palette.surface_alt)
        .child(
            div()
                .text_micro()
                .text_color(palette.muted)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_subject()
                .tabular_nums()
                .text_color(palette.text)
                .child(value),
        )
}
