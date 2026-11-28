//! Similar Positions & Practice sub-view (`SimilarPositions.tsx`,
//! `SimilarPositionPractice.tsx`, `SimilarPracticeHistory.tsx`,
//! `similar-practice-history-api.ts`).
//!
//! Features:
//! - Pip-distance search (up to 50 hits, 25/page pagination, 4 toggles: `same_dice`,
//!   `same_score`, `same_cube`, `include_current_match`) via `facade.find_similar_positions`
//! - Right-hand `BoardView` preview + Open in Analysis (`WorkspaceEvent::OpenMatchInAnalysis`)
//! - Interactive click-to-move practice modal (`BoardView` + synchronous
//!   `neondojo_rules::backgammon` legal turns), **Your Move vs Best Play** arrow
//!   overlay toggle (`BoardViewMode::Arrows`), Skip-while-scoring, and **Retry Missed Moves**
//! - Custom GPUI `canvas()` [`SimilarPracticeChartView`] above the paginated practice history
//! - Persisted failed-save retry queue stored in `LocalUiState::study.pending_similar_practice`
//!   with automatic startup/visibility flush and manual **Retry save** recovery

use super::practice_board::{CheckerPracticeDraft, position_to_board_position};
use super::practice_chart::SimilarPracticeChartView;
use super::{spawn_facade_read, spawn_facade_write};
use crate::app_context::{DesktopAppContext, themed_board};
use crate::events::WorkspaceEvent;
use crate::i18n::{current_locale, format_domain_error};
use crate::local_state::{LocalUiState, PendingSimilarPractice};
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::banners::{BannerKind, EmptyState, StatusBanner};
use crate::ui::icons::ActionIconName;
use crate::ui::modal::{ModalBusyLock, ModalConfig, ModalSize, open_modal_dialog};
use crate::ui::pagination::NumberedSearchPagination;
use chrono::{DateTime, Utc};
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Subscription,
    TestSupportExt, Window, base::StyledExt, component::WindowExt, div, prelude::FluentBuilder, px,
};
use neondojo::domain_error::ErrorKind;
use neondojo::facade::BackendFacade;
use neondojo::model::Position;
use neondojo::similar_positions::{
    CheckerPracticeAnswer, CheckerPracticeMove, CheckerPracticeScore, SaveSimilarPracticeSummary,
    SimilarHit, SimilarPracticeSummary, SimilarQuery,
};
use neondojo_board::{BoardEvent, BoardView, BoardViewMode, decode_match_context};
use neondojo_i18n::{
    I18n, Locale, Param, format_datetime_medium_local, format_decimal, format_equity, format_int,
    localize_decision,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Similar-position results shown per page (backend returns up to 50 nearest hits).
pub const SIMILAR_PAGE_SIZE: usize = 25;

/// Key prefix matching web `localStorage` (`neondojo.similarPractice.pending.<uuid>`).
pub const PENDING_SIMILAR_PREFIX: &str = "neondojo.similarPractice.pending.";

/// Clones a [`SimilarHit`] without modifying the backend crate.
pub fn clone_similar_hit(hit: &SimilarHit) -> SimilarHit {
    SimilarHit {
        position: hit.position.clone(),
        match_name: hit.match_name.clone(),
        white: hit.white.clone(),
        black: hit.black.clone(),
        distance: hit.distance,
        equity_loss: hit.equity_loss,
    }
}

/// Clones a [`SimilarPracticeSummary`] without modifying the backend crate.
pub fn clone_practice_summary(summary: &SimilarPracticeSummary) -> SimilarPracticeSummary {
    SimilarPracticeSummary {
        id: summary.id,
        completed_at: summary.completed_at,
        total_positions: summary.total_positions,
        scored_moves: summary.scored_moves,
        missed_moves: summary.missed_moves,
        total_equity_loss: summary.total_equity_loss,
        average_equity_loss: summary.average_equity_loss,
        is_retry: summary.is_retry,
    }
}

/// Serializable payload mirror of [`SaveSimilarPracticeSummary`] persisted in
/// [`LocalUiState::study.pending_similar_practice`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingPracticeSummaryPayload {
    pub id: Uuid,
    pub completed_at: DateTime<Utc>,
    pub total_positions: i32,
    pub losses: Vec<f64>,
    pub is_retry: bool,
}

impl PendingPracticeSummaryPayload {
    pub fn storage_key(&self) -> String {
        format!("{PENDING_SIMILAR_PREFIX}{}", self.id)
    }

    pub fn to_backend(&self) -> SaveSimilarPracticeSummary {
        SaveSimilarPracticeSummary {
            id: self.id,
            completed_at: self.completed_at,
            total_positions: self.total_positions,
            losses: self.losses.clone(),
            is_retry: self.is_retry,
        }
    }
}

/// Failed-save retry queue synced with [`LocalUiState::study.pending_similar_practice`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PendingSimilarPracticeQueue {
    pub entries: Vec<PendingPracticeSummaryPayload>,
}

impl PendingSimilarPracticeQueue {
    /// Parses all valid pending practice summaries from [`LocalUiState`].
    pub fn load_from_local_state(state: &LocalUiState) -> Self {
        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        for raw in &state.study.pending_similar_practice {
            let Some(expected_id) = raw.key.strip_prefix(PENDING_SIMILAR_PREFIX) else {
                continue;
            };
            let Ok(payload) =
                serde_json::from_str::<PendingPracticeSummaryPayload>(&raw.payload_json)
            else {
                continue;
            };
            if payload.id.to_string() == expected_id && seen.insert(payload.id) {
                entries.push(payload);
            }
        }
        Self { entries }
    }

    /// Writes the current queue into `state.study.pending_similar_practice` and
    /// saves atomically when `path` is provided.
    pub fn sync_to_local_state(
        &self,
        state: &mut LocalUiState,
        path: Option<&Path>,
    ) -> anyhow::Result<()> {
        state.study.pending_similar_practice = self
            .entries
            .iter()
            .filter_map(|entry| {
                serde_json::to_string(entry)
                    .ok()
                    .map(|payload_json| PendingSimilarPractice {
                        key: entry.storage_key(),
                        payload_json,
                    })
            })
            .collect();
        if let Some(path) = path {
            state.save_atomic(path)?;
        }
        Ok(())
    }

    /// Enqueues (or replaces by round ID) a completed round before saving to backend.
    pub fn enqueue(
        &mut self,
        payload: PendingPracticeSummaryPayload,
        state: &mut LocalUiState,
        path: Option<&Path>,
    ) -> anyhow::Result<()> {
        self.entries.retain(|e| e.id != payload.id);
        self.entries.push(payload);
        self.sync_to_local_state(state, path)
    }

    /// Removes a round from the retry queue once saved (or resolved).
    pub fn remove(
        &mut self,
        id: Uuid,
        state: &mut LocalUiState,
        path: Option<&Path>,
    ) -> anyhow::Result<()> {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        if self.entries.len() != before {
            self.sync_to_local_state(state, path)?;
        }
        Ok(())
    }
}

/// Filter toggles for pip-distance similar position search.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SimilarSearchFilters {
    pub same_dice: bool,
    pub same_score: bool,
    pub same_cube: bool,
    pub include_current_match: bool,
}

impl SimilarSearchFilters {
    pub fn to_query(self) -> SimilarQuery {
        SimilarQuery {
            include_current_match: self.include_current_match,
            same_dice: self.same_dice,
            same_score: self.same_score,
            same_cube: self.same_cube,
        }
    }
}

/// Events emitted by [`SimilarPositionsView`] upward to [`super::PositionsWorkspace`].
#[derive(Debug, Clone)]
pub enum SimilarPositionsEvent {
    BackToSaved,
    Workspace(Box<WorkspaceEvent>),
}

/// Complete GPUI Similar Positions & Practice sub-view.
pub struct SimilarPositionsView {
    facade: Option<BackendFacade>,
    persist_path: Option<PathBuf>,
    visible: bool,
    history_loaded: bool,

    // Source position & pip-distance search
    source: Option<(Position, String)>,
    filters: SimilarSearchFilters,
    hits: Vec<SimilarHit>,
    page: usize,
    selected_id: Option<Uuid>,
    loading: bool,
    search_error: Option<SharedString>,
    preview_board: Entity<BoardView>,

    // Practice history, chart & persisted failed-save retry queue
    history: Vec<SimilarPracticeSummary>,
    history_next_cursor: Option<Uuid>,
    loading_history: bool,
    history_error: Option<SharedString>,
    chart: Entity<SimilarPracticeChartView>,
    pending_queue: PendingSimilarPracticeQueue,
    flushing_pending: bool,
    pending_retry_error: Option<SharedString>,
}

impl EventEmitter<SimilarPositionsEvent> for SimilarPositionsView {}

impl SimilarPositionsView {
    pub fn new(
        persist_path: Option<PathBuf>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let app_ctx = cx.try_global::<DesktopAppContext>().cloned();
        let facade = app_ctx.as_ref().and_then(|c| c.facade());
        let local_state = cx.try_global::<LocalUiState>().cloned().unwrap_or_default();
        let pending_queue = PendingSimilarPracticeQueue::load_from_local_state(&local_state);

        let preview_board = cx.new(|cx| themed_board("similar-positions-preview-board", cx));
        let chart = cx.new(SimilarPracticeChartView::new);

        Self {
            facade,
            persist_path,
            visible: false,
            history_loaded: false,
            source: None,
            filters: SimilarSearchFilters::default(),
            hits: Vec::new(),
            page: 1,
            selected_id: None,
            loading: false,
            search_error: None,
            preview_board,
            history: Vec::new(),
            history_next_cursor: None,
            loading_history: false,
            history_error: None,
            chart,
            pending_queue,
            flushing_pending: false,
            pending_retry_error: None,
        }
    }

    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        if visible {
            if self.facade.is_none() {
                self.facade = cx
                    .try_global::<DesktopAppContext>()
                    .and_then(|c| c.facade());
            }
            if !self.history_loaded && self.facade.is_some() {
                self.history_loaded = true;
                self.flush_pending_saves(cx);
                self.load_history(None, cx);
            }
        }
        cx.notify();
    }

    /// Sets the target checker occurrence and immediately runs a pip-distance search.
    pub fn set_source(&mut self, position: Position, match_name: String, cx: &mut Context<Self>) {
        self.source = Some((position, match_name));
        self.page = 1;
        self.selected_id = None;
        self.hits.clear();
        self.search_error = None;
        self.run_search(cx);
        cx.notify();
    }

    /// Test & inspection accessors.
    pub fn filters(&self) -> SimilarSearchFilters {
        self.filters
    }

    pub fn pending_queue(&self) -> &PendingSimilarPracticeQueue {
        &self.pending_queue
    }

    pub fn chart(&self) -> &Entity<SimilarPracticeChartView> {
        &self.chart
    }

    pub fn set_hits_for_test(&mut self, hits: Vec<SimilarHit>, cx: &mut Context<Self>) {
        self.selected_id = hits.first().map(|h| h.position.id);
        self.hits = hits;
        self.sync_preview_board(cx);
        cx.notify();
    }

    pub fn set_history_for_test(
        &mut self,
        items: Vec<SimilarPracticeSummary>,
        cx: &mut Context<Self>,
    ) {
        let chart_items: Vec<SimilarPracticeSummary> =
            items.iter().map(clone_practice_summary).collect();
        self.history = items;
        self.chart.update(cx, |c, cx| c.set_items(chart_items, cx));
        cx.notify();
    }

    pub fn enqueue_pending_summary(
        &mut self,
        payload: PendingPracticeSummaryPayload,
        cx: &mut Context<Self>,
    ) {
        let mut state = cx.try_global::<LocalUiState>().cloned().unwrap_or_default();
        let _ = self
            .pending_queue
            .enqueue(payload, &mut state, self.persist_path.as_deref());
        cx.set_global(state);
        cx.notify();
    }

    pub fn remove_pending_summary(&mut self, id: Uuid, cx: &mut Context<Self>) {
        let mut state = cx.try_global::<LocalUiState>().cloned().unwrap_or_default();
        let _ = self
            .pending_queue
            .remove(id, &mut state, self.persist_path.as_deref());
        cx.set_global(state);
        cx.notify();
    }

    pub fn toggle_filter(&mut self, which: &'static str, cx: &mut Context<Self>) {
        match which {
            "same_dice" => self.filters.same_dice = !self.filters.same_dice,
            "same_score" => self.filters.same_score = !self.filters.same_score,
            "same_cube" => self.filters.same_cube = !self.filters.same_cube,
            "include_current_match" => {
                self.filters.include_current_match = !self.filters.include_current_match;
            }
            _ => return,
        }
        self.page = 1;
        self.run_search(cx);
        cx.notify();
    }

    pub fn run_search(&mut self, cx: &mut Context<Self>) {
        let Some((source_pos, _)) = &self.source else {
            return;
        };
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let source_id = source_pos.id;
        let query = self.filters.to_query();
        self.loading = true;
        self.search_error = None;

        let Some(task) = spawn_facade_read(cx, move || async move {
            facade.find_similar_positions(source_id, query).await
        }) else {
            self.loading = false;
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = weak.update(cx, |v, cx| {
                v.loading = false;
                match res {
                    Ok(resp) => {
                        v.hits = resp.items;
                        v.page = 1;
                        v.selected_id = v.hits.first().map(|h| h.position.id);
                        v.sync_preview_board(cx);
                    }
                    Err(err) => {
                        v.search_error = Some(format_domain_error(current_locale(cx), &err).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn load_history(&mut self, before: Option<Uuid>, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        self.loading_history = true;
        self.history_error = None;
        let append = before.is_some();
        let Some(task) = spawn_facade_read(cx, move || async move {
            facade.list_similar_practice_history(before).await
        }) else {
            self.loading_history = false;
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = weak.update(cx, |v, cx| {
                v.loading_history = false;
                match res {
                    Ok(page) => {
                        if append {
                            let existing: HashSet<Uuid> = v.history.iter().map(|i| i.id).collect();
                            for item in page.items {
                                if !existing.contains(&item.id) {
                                    v.history.push(item);
                                }
                            }
                        } else {
                            v.history = page.items;
                        }
                        v.history_next_cursor = page.next_cursor;
                        let chart_items: Vec<SimilarPracticeSummary> =
                            v.history.iter().map(clone_practice_summary).collect();
                        v.chart.update(cx, |c, cx| c.set_items(chart_items, cx));
                    }
                    Err(err) => {
                        v.history_error =
                            Some(format_domain_error(current_locale(cx), &err).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Flushes all persisted pending practice summaries in
    /// `LocalUiState::study.pending_similar_practice` to the backend.
    pub fn flush_pending_saves(&mut self, cx: &mut Context<Self>) {
        if self.flushing_pending || self.pending_queue.entries.is_empty() {
            return;
        }
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let items = self.pending_queue.entries.clone();
        self.flushing_pending = true;
        self.pending_retry_error = None;

        let Some(task) = spawn_facade_write(cx, move || async move {
            let mut succeeded = Vec::new();
            let mut first_error = None;
            for item in items {
                match facade
                    .save_similar_practice_summary(item.to_backend())
                    .await
                {
                    Ok(_) => succeeded.push(item.id),
                    Err(err) if err.kind == ErrorKind::Conflict => {
                        // Round ID already saved previously with different payload: drop stuck entry
                        succeeded.push(item.id);
                    }
                    Err(err) => {
                        if first_error.is_none() {
                            first_error = Some(err);
                        }
                    }
                }
            }
            (succeeded, first_error)
        }) else {
            self.flushing_pending = false;
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let Ok((succeeded, first_error)) = task.await else {
                return;
            };
            let _ = weak.update(cx, |v, cx| {
                v.flushing_pending = false;
                let any_saved = !succeeded.is_empty();
                for id in succeeded {
                    v.remove_pending_summary(id, cx);
                }
                if let Some(err) = first_error {
                    v.pending_retry_error =
                        Some(format_domain_error(current_locale(cx), &err).into());
                }
                if any_saved {
                    v.load_history(None, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn active_preview(&self) -> Option<&SimilarHit> {
        self.selected_id
            .and_then(|id| self.hits.iter().find(|h| h.position.id == id))
            .or_else(|| self.hits.first())
    }

    fn sync_preview_board(&mut self, cx: &mut Context<Self>) {
        let Some((bp, white, black)) = self.active_preview().map(|hit| {
            (
                position_to_board_position(&hit.position, false),
                hit.white.clone(),
                hit.black.clone(),
            )
        }) else {
            return;
        };
        self.preview_board.update(cx, |bv, cx| {
            bv.set_position(bp, cx);
            bv.set_players(white, black, cx);
        });
    }

    fn open_hit_in_analysis(&self, hit: &SimilarHit, cx: &mut Context<Self>) {
        cx.emit(SimilarPositionsEvent::Workspace(Box::new(
            WorkspaceEvent::OpenMatchInAnalysis {
                match_id: hit.position.match_uuid,
                game_no: Some(hit.position.game_no),
                position_id: Some(hit.position.id),
                decision_no: None,
            },
        )));
    }

    fn open_practice_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.hits.is_empty() {
            return;
        }
        let locale = current_locale(cx);
        let busy = ModalBusyLock::new(false);
        let parent = cx.entity();
        let facade = self.facade.clone();
        let hits: Vec<SimilarHit> = self.hits.iter().map(clone_similar_hit).collect();
        let dialog = cx.new(|cx| {
            SimilarPracticeSessionView::new(facade, hits, parent, busy.clone(), window, cx)
        });
        let body = dialog.clone();
        let cfg = ModalConfig::new(I18n::t(locale, "position-browser", "practiceSimilar"))
            .size(ModalSize::ExtraWide)
            .busy_lock(busy);
        open_modal_dialog(
            window,
            cx,
            cfg,
            move |_, _| body.clone().into_any_element(),
            None::<fn(&mut Window, &mut App) -> gpui_kit::AnyElement>,
            None,
        );
    }

    fn render_history_section(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let pb = |k: &'static str| I18n::t(locale, "position-browser", k);
        let ent = cx.entity();

        let mut section = div()
            .id("similar-practice-history-section")
            .test_support()
            .v_flex()
            .gap(px(10.0))
            .pt(px(12.0))
            .border_t_1()
            .border_color(p.border)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_subject()
                            .text_color(p.text)
                            .child(pb("practiceHistory")),
                    )
                    .when(!self.pending_queue.entries.is_empty(), |this| {
                        let ent_fl = ent.clone();
                        this.child(
                            ActionButton::new("similar-pending-retry-btn")
                                .icon(ActionIconName::Refresh)
                                .label(format!(
                                    "{} ({})",
                                    pb("practiceRetrySave"),
                                    self.pending_queue.entries.len()
                                ))
                                .disabled(self.flushing_pending)
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    ent_fl.update(cx, |v, cx| v.flush_pending_saves(cx));
                                }),
                        )
                    }),
            );

        if let Some(err) = self.pending_retry_error.clone() {
            let ent_r = ent.clone();
            section = section.child(
                StatusBanner::new(
                    "similar-pending-save-error",
                    BannerKind::Warning,
                    format!("{} {err}", pb("practiceSaveFailed")),
                )
                .palette(p)
                .on_retry(pb("practiceRetrySave"), move |_, _, cx| {
                    ent_r.update(cx, |v, cx| v.flush_pending_saves(cx));
                }),
            );
        }

        if let Some(err) = self.history_error.clone() {
            let ent_h = ent.clone();
            section = section.child(
                StatusBanner::new("similar-history-error", BannerKind::Error, err)
                    .palette(p)
                    .on_retry(pb("retrySimilar"), move |_, _, cx| {
                        ent_h.update(cx, |v, cx| v.load_history(None, cx));
                    }),
            );
        }

        if self.history.is_empty() && !self.loading_history {
            section = section.child(
                div()
                    .text_body()
                    .text_color(p.muted)
                    .child(pb("practiceHistoryEmpty")),
            );
        } else {
            section = section.child(self.chart.clone());

            let mut hist_table = div()
                .v_flex()
                .rounded(px(8.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .px(px(10.0))
                        .py(px(6.0))
                        .bg(p.surface_alt)
                        .border_b_1()
                        .border_color(p.border)
                        .text_table_heading()
                        .text_color(p.muted)
                        .child(div().w(px(180.0)).child(pb("practiceCompletedAt")))
                        .child(div().w(px(90.0)).child("Type"))
                        .child(div().w(px(110.0)).child(pb("practiceScoredMoves")))
                        .child(div().w(px(90.0)).child("Missed"))
                        .child(div().flex_1().text_right().child(pb("practiceAverageLoss"))),
                );

            for round in &self.history {
                let kind_lbl = pb(if round.is_retry {
                    "practiceRoundRetry"
                } else {
                    "practiceRoundInitial"
                });
                hist_table = hist_table.child(
                    div()
                        .h_flex()
                        .items_center()
                        .px(px(10.0))
                        .py(px(5.0))
                        .border_b_1()
                        .border_color(p.border)
                        .text_table()
                        .tabular_nums()
                        .text_color(p.text)
                        .child(
                            div()
                                .w(px(180.0))
                                .child(format_datetime_medium_local(locale, round.completed_at)),
                        )
                        .child(
                            div()
                                .w(px(90.0))
                                .text_color(if round.is_retry {
                                    p.warning_text
                                } else {
                                    p.accent
                                })
                                .child(kind_lbl),
                        )
                        .child(div().w(px(110.0)).child(format!(
                            "{} / {}",
                            format_int(locale, i64::from(round.scored_moves)),
                            format_int(locale, i64::from(round.total_positions))
                        )))
                        .child(
                            div()
                                .w(px(90.0))
                                .child(format_int(locale, i64::from(round.missed_moves))),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_right()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format_decimal(locale, round.average_equity_loss, 3, 6)),
                        ),
                );
            }
            section = section.child(hist_table);

            if let Some(cursor) = self.history_next_cursor {
                let ent_more = ent.clone();
                section = section.child(
                    div().h_flex().justify_center().child(
                        ActionButton::new("similar-history-load-older")
                            .icon(ActionIconName::Refresh)
                            .label(pb("practiceLoadOlder"))
                            .disabled(self.loading_history)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_more.update(cx, |v, cx| v.load_history(Some(cursor), cx));
                            }),
                    ),
                );
            }
        }

        section
    }
}

impl Render for SimilarPositionsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = NeonTheme::current_palette(cx);
        let pb = |k: &'static str| I18n::t(locale, "position-browser", k);
        let ds = |k: &'static str| I18n::t(locale, "decision-search", k);
        let ent = cx.entity();

        let mut root = div()
            .id("similar-positions-workspace")
            .test_support()
            .v_flex()
            .gap(px(12.0))
            .w_full()
            .flex_1();

        let Some((source_pos, match_name)) = self.source.clone() else {
            let ent_back = ent.clone();
            return root
                .child(
                    EmptyState::new("similar-no-source", pb("similar"), pb("previewHint"))
                        .icon(ActionIconName::Similar)
                        .action(
                            ActionButton::new("similar-go-saved")
                                .icon(ActionIconName::Back)
                                .label(pb("backToSaved"))
                                .primary()
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    ent_back.update(cx, |_, cx| {
                                        cx.emit(SimilarPositionsEvent::BackToSaved);
                                    });
                                }),
                        )
                        .palette(p),
                )
                .child(self.render_history_section(p, locale, cx));
        };

        let source_ctx = decode_match_context(&source_pos.gnubg_match_id);
        let same_score_label = pb(if source_ctx.length > 0 {
            "sameMatchScore"
        } else {
            "sameMoneyGame"
        });

        // Header + 4 filter toggles + Practice results button
        let controls = div()
            .v_flex()
            .gap(px(8.0))
            .p(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .v_flex()
                            .gap(px(2.0))
                            .child(div().text_panel().text_color(p.text).child(pb("similar")))
                            .child(div().text_meta().text_color(p.muted).child(format!(
                                "{match_name} · {} {} · {} {}",
                                pb("gameNumber"),
                                source_pos.game_no,
                                pb("moveNumber"),
                                source_pos.ply
                            ))),
                    )
                    .child(
                        ActionButton::new("similar-back-saved-btn")
                            .icon(ActionIconName::Back)
                            .label(pb("backToSaved"))
                            .palette(p)
                            .on_click({
                                let ent_b = ent.clone();
                                move |_, _, cx| {
                                    ent_b.update(cx, |_, cx| {
                                        cx.emit(SimilarPositionsEvent::BackToSaved);
                                    });
                                }
                            }),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.0))
                    .child({
                        let ent_f = ent.clone();
                        ActionButton::new("similar-toggle-same-dice")
                            .label(pb("sameDice"))
                            .active(self.filters.same_dice)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_f.update(cx, |v, cx| v.toggle_filter("same_dice", cx));
                            })
                    })
                    .child({
                        let ent_f = ent.clone();
                        ActionButton::new("similar-toggle-same-score")
                            .label(same_score_label)
                            .active(self.filters.same_score)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_f.update(cx, |v, cx| v.toggle_filter("same_score", cx));
                            })
                    })
                    .child({
                        let ent_f = ent.clone();
                        ActionButton::new("similar-toggle-same-cube")
                            .label(pb("sameCube"))
                            .active(self.filters.same_cube)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_f.update(cx, |v, cx| v.toggle_filter("same_cube", cx));
                            })
                    })
                    .child({
                        let ent_f = ent.clone();
                        ActionButton::new("similar-toggle-current-match")
                            .label(pb("includeCurrentMatch"))
                            .active(self.filters.include_current_match)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_f.update(cx, |v, cx| {
                                    v.toggle_filter("include_current_match", cx);
                                });
                            })
                    })
                    .child({
                        let ent_p = ent.clone();
                        ActionButton::new("similar-practice-results-btn")
                            .icon(ActionIconName::Cards)
                            .label(pb("practiceResults"))
                            .primary()
                            .disabled(self.hits.is_empty())
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent_p.update(cx, |v, cx| v.open_practice_modal(window, cx));
                            })
                    }),
            );

        root = root.child(controls);

        if let Some(err) = self.search_error.clone() {
            let ent_r = ent.clone();
            root = root.child(
                StatusBanner::new("similar-search-error", BannerKind::Error, err)
                    .palette(p)
                    .on_retry(pb("retrySimilar"), move |_, _, cx| {
                        ent_r.update(cx, |v, cx| v.run_search(cx));
                    }),
            );
        }

        if self.loading {
            root = root.child(div().text_body().text_color(p.muted).child(pb("loading")));
        } else if self.hits.is_empty() {
            root = root.child(
                EmptyState::new("similar-empty", pb("similar"), pb("similarEmpty"))
                    .icon(ActionIconName::Similar)
                    .palette(p),
            );
        } else {
            let last_page = self.hits.len().div_ceil(SIMILAR_PAGE_SIZE).max(1);
            let page = self.page.clamp(1, last_page);
            let start = (page - 1) * SIMILAR_PAGE_SIZE;
            let page_rows = &self.hits[start..(start + SIMILAR_PAGE_SIZE).min(self.hits.len())];
            let active_id = self.active_preview().map(|h| h.position.id);

            let mut table = div().v_flex().flex_1().min_w(px(0.0)).gap(px(8.0)).child(
                div()
                    .id("similar-results-table-scroll")
                    .v_flex()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .overflow_x_scroll()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .px(px(8.0))
                            .py(px(6.0))
                            .bg(p.surface_alt)
                            .border_b_1()
                            .border_color(p.border)
                            .text_table_heading()
                            .text_color(p.muted)
                            .child(div().w(px(170.0)).flex_shrink_0().child(pb("groupMatch")))
                            .child(div().w(px(110.0)).flex_shrink_0().child(pb("player")))
                            .child(div().w(px(56.0)).flex_shrink_0().child(pb("gameNumber")))
                            .child(div().w(px(56.0)).flex_shrink_0().child(pb("moveNumber")))
                            .child(div().w(px(110.0)).flex_shrink_0().child(ds("score")))
                            .child(div().w(px(90.0)).flex_shrink_0().child(ds("cube")))
                            .child(div().w(px(100.0)).flex_shrink_0().child(ds("played")))
                            .child(div().w(px(76.0)).flex_shrink_0().child(ds("loss")))
                            .child(
                                div()
                                    .w(px(80.0))
                                    .flex_shrink_0()
                                    .text_right()
                                    .child(pb("checkerDistance")),
                            ),
                    )
                    .children(page_rows.iter().map(|hit| {
                        let hit_id = hit.position.id;
                        let is_active = active_id == Some(hit_id);
                        let ent_sel = ent.clone();
                        let ctx = decode_match_context(&hit.position.gnubg_match_id);
                        let score_txt = if ctx.length > 0 {
                            let own = usize::try_from(hit.position.player).unwrap_or(0).min(1);
                            I18n::format(
                                locale,
                                "decision-search",
                                "scoreAway",
                                &[
                                    ("own", Param::from(i64::from(ctx.length - ctx.scores[own]))),
                                    (
                                        "opponent",
                                        Param::from(i64::from(ctx.length - ctx.scores[1 - own])),
                                    ),
                                ],
                            )
                        } else {
                            I18n::t(locale, "app", "common.moneyGame").to_string()
                        };
                        let cube_txt = match ctx.owner {
                            None => ctx.cube.to_string(),
                            Some(0) => format!("{} · {}", ctx.cube, hit.white),
                            Some(_) => format!("{} · {}", ctx.cube, hit.black),
                        };
                        let player_name = if hit.position.player == 0 {
                            &hit.white
                        } else {
                            &hit.black
                        };
                        let played = if hit.position.notation.trim().is_empty() {
                            "—".into()
                        } else {
                            localize_decision(locale, &hit.position.notation)
                        };
                        let loss = hit
                            .equity_loss
                            .map(|v| format_decimal(locale, v, 3, 3))
                            .unwrap_or_else(|| "—".into());

                        div()
                            .id(SharedString::from(format!("similar-row-{hit_id}")))
                            .test_support()
                            .h_flex()
                            .items_center()
                            .px(px(8.0))
                            .py(px(5.0))
                            .border_b_1()
                            .border_color(p.border)
                            .bg(if is_active { p.selected } else { p.surface })
                            .text_table()
                            .tabular_nums()
                            .text_color(p.text)
                            .cursor_pointer()
                            .hover(move |s| {
                                s.bg(if is_active { p.selected } else { p.surface_alt })
                            })
                            .on_click(move |_, _, cx| {
                                ent_sel.update(cx, |v, cx| {
                                    v.selected_id = Some(hit_id);
                                    v.sync_preview_board(cx);
                                    cx.notify();
                                });
                            })
                            .child(
                                div()
                                    .w(px(170.0))
                                    .flex_shrink_0()
                                    .truncate()
                                    .child(hit.match_name.clone()),
                            )
                            .child(
                                div()
                                    .w(px(110.0))
                                    .flex_shrink_0()
                                    .truncate()
                                    .child(player_name.clone()),
                            )
                            .child(
                                div()
                                    .w(px(56.0))
                                    .flex_shrink_0()
                                    .child(format_int(locale, i64::from(hit.position.game_no))),
                            )
                            .child(
                                div()
                                    .w(px(56.0))
                                    .flex_shrink_0()
                                    .child(format_int(locale, i64::from(hit.position.ply))),
                            )
                            .child(
                                div()
                                    .w(px(110.0))
                                    .flex_shrink_0()
                                    .truncate()
                                    .child(score_txt),
                            )
                            .child(div().w(px(90.0)).flex_shrink_0().truncate().child(cube_txt))
                            .child(div().w(px(100.0)).flex_shrink_0().truncate().child(played))
                            .child(div().w(px(76.0)).flex_shrink_0().child(loss))
                            .child(
                                div()
                                    .w(px(80.0))
                                    .flex_shrink_0()
                                    .text_right()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format_int(locale, i64::from(hit.distance))),
                            )
                    })),
            );

            let ent_pg = ent.clone();
            table = table.child(
                div().h_flex().justify_end().child(
                    NumberedSearchPagination::new("similar-results-pagination", page, last_page)
                        .palette(p)
                        .on_select(move |target, _, cx| {
                            ent_pg.update(cx, |v, cx| {
                                v.page = target;
                                cx.notify();
                            });
                        }),
                ),
            );

            let preview_pane = self.active_preview().map(clone_similar_hit).map(|prev| {
                let ctx = decode_match_context(&prev.position.gnubg_match_id);
                let match_ctx_str = if ctx.length > 0 {
                    I18n::format(
                        locale,
                        "app",
                        "app.matchLength",
                        &[("count", Param::from(i64::from(ctx.length)))],
                    )
                } else {
                    format!(
                        "{} · Jacoby {}",
                        I18n::t(locale, "app", "common.moneyGame"),
                        if ctx.jacoby { "✓" } else { "✗" }
                    )
                };
                let ent_open = ent.clone();
                div()
                    .v_flex()
                    .gap(px(8.0))
                    .w(px(420.0))
                    .flex_shrink_0()
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
                            .gap(px(8.0))
                            .child(
                                div()
                                    .v_flex()
                                    .gap(px(2.0))
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .text_panel()
                                            .text_color(p.text)
                                            .truncate()
                                            .child(prev.match_name.clone()),
                                    )
                                    .child(
                                        div().text_meta().text_color(p.muted).child(match_ctx_str),
                                    ),
                            )
                            .child(
                                ActionButton::new("similar-preview-open-analysis")
                                    .icon(ActionIconName::Analysis)
                                    .label(ds("openAnalysis"))
                                    .primary()
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        ent_open.update(cx, |v, cx| {
                                            v.open_hit_in_analysis(&prev, cx);
                                        });
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(270.0))
                            .child(self.preview_board.clone()),
                    )
            });

            root = root.child(
                div()
                    .h_flex()
                    .items_start()
                    .gap(px(14.0))
                    .w_full()
                    .child(table)
                    .when_some(preview_pane, |this, pane| this.child(pane)),
            );
        }

        root.child(self.render_history_section(p, locale, cx))
    }
}

// ── Interactive Similar Position Practice Modal ──────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArrowPreviewChoice {
    Played,
    Best,
}

enum SimilarPracticeFeedback {
    Scored(CheckerPracticeScore),
    Revealed(CheckerPracticeAnswer),
}

struct SimilarPracticeSessionView {
    facade: Option<BackendFacade>,
    parent: Entity<SimilarPositionsView>,
    busy: ModalBusyLock,

    round_id: Uuid,
    is_retry: bool,
    items: Vec<SimilarHit>,
    index: usize,
    scored: Vec<(SimilarHit, f64)>,

    board: Entity<BoardView>,
    draft: Option<CheckerPracticeDraft>,
    feedback: Option<SimilarPracticeFeedback>,
    preview_choice: ArrowPreviewChoice,
    submitting_score: bool,
    submitting_reveal: bool,
    error: Option<SharedString>,

    saving_summary: bool,
    summary_saved: bool,
    summary_save_error: Option<(PendingPracticeSummaryPayload, SharedString)>,
    _board_sub: Subscription,
}

impl SimilarPracticeSessionView {
    fn new(
        facade: Option<BackendFacade>,
        items: Vec<SimilarHit>,
        parent: Entity<SimilarPositionsView>,
        busy: ModalBusyLock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let board = cx.new(|cx| themed_board("similar-practice-board", cx));

        let board_sub = cx.subscribe_in(&board, window, |this, _, event: &BoardEvent, _, cx| {
            let locked = this.feedback.is_some() || this.submitting_score || this.submitting_reveal;
            if locked {
                return;
            }
            let Some(draft) = &mut this.draft else {
                return;
            };
            match *event {
                BoardEvent::PointClicked(pt) => {
                    if draft.click_point(pt, false) {
                        this.sync_board(cx);
                    }
                }
                BoardEvent::SourceHovered(src) => {
                    if draft.hovered_source != src {
                        draft.hovered_source = src;
                        this.sync_board(cx);
                    }
                }
                BoardEvent::DiceClicked => {
                    if draft.swap_dice(false) {
                        this.sync_board(cx);
                    }
                }
                BoardEvent::CubeClicked => {}
            }
        });

        let mut view = Self {
            facade,
            parent,
            busy,
            round_id: Uuid::new_v4(),
            is_retry: false,
            items,
            index: 0,
            scored: Vec::new(),
            board,
            draft: None,
            feedback: None,
            preview_choice: ArrowPreviewChoice::Best,
            submitting_score: false,
            submitting_reveal: false,
            error: None,
            saving_summary: false,
            summary_saved: false,
            summary_save_error: None,
            _board_sub: board_sub,
        };
        view.prepare_question(cx);
        view
    }

    fn current(&self) -> Option<&SimilarHit> {
        self.items.get(self.index)
    }

    fn start_retry_missed(&mut self, cx: &mut Context<Self>) {
        let missed: Vec<SimilarHit> = self
            .scored
            .iter()
            .filter(|(_, loss)| *loss > 0.0)
            .map(|(hit, _)| clone_similar_hit(hit))
            .collect();
        if missed.is_empty() {
            return;
        }
        self.round_id = Uuid::new_v4();
        self.is_retry = true;
        self.items = missed;
        self.index = 0;
        self.scored.clear();
        self.saving_summary = false;
        self.summary_saved = false;
        self.summary_save_error = None;
        self.prepare_question(cx);
    }

    fn prepare_question(&mut self, cx: &mut Context<Self>) {
        self.feedback = None;
        self.preview_choice = ArrowPreviewChoice::Best;
        self.submitting_score = false;
        self.submitting_reveal = false;
        self.error = None;
        if let Some(hit) = self.current().map(clone_similar_hit) {
            self.draft = Some(CheckerPracticeDraft::from_position(&hit.position));
            self.sync_board(cx);
        } else {
            self.draft = None;
        }
        cx.notify();
    }

    fn sync_board(&mut self, cx: &mut Context<Self>) {
        let Some(hit) = self.current().map(clone_similar_hit) else {
            return;
        };
        let Some(draft) = &self.draft else {
            return;
        };
        let loading = self.submitting_score || self.submitting_reveal;
        let (bp, interaction, preview, mode) = match &self.feedback {
            None => (
                draft.board_position(),
                Some(draft.interaction_state(loading)),
                None,
                BoardViewMode::Before,
            ),
            Some(fb) => {
                let preview = match (fb, self.preview_choice) {
                    (SimilarPracticeFeedback::Scored(_), ArrowPreviewChoice::Played) => {
                        draft.project_draft_preview()
                    }
                    (SimilarPracticeFeedback::Scored(s), ArrowPreviewChoice::Best) => {
                        draft.project_notation_preview(&s.best)
                    }
                    (SimilarPracticeFeedback::Revealed(a), _) => {
                        draft.project_notation_preview(&a.best)
                    }
                };
                (
                    position_to_board_position(&hit.position, true),
                    None,
                    Some(preview),
                    BoardViewMode::Arrows,
                )
            }
        };

        let white = hit.white.clone();
        let black = hit.black.clone();
        self.board.update(cx, |bv, cx| {
            bv.set_position(bp, cx);
            bv.set_players(white, black, cx);
            bv.set_interaction(interaction, cx);
            bv.set_preview(preview, mode, cx);
        });
        cx.notify();
    }

    fn check_move(&mut self, cx: &mut Context<Self>) {
        let Some(hit) = self.current().map(clone_similar_hit) else {
            return;
        };
        let Some(draft) = &self.draft else {
            return;
        };
        let Some(facade) = self.facade.clone() else {
            return;
        };
        if self.submitting_score || self.submitting_reveal || !draft.can_submit() {
            return;
        }
        self.submitting_score = true;
        self.error = None;
        let pos_id = hit.position.id;
        let input = CheckerPracticeMove {
            steps: draft.steps.clone(),
        };
        let req_index = self.index;
        let Some(task) = spawn_facade_write(cx, move || async move {
            facade.evaluate_similar_practice(pos_id, input).await
        }) else {
            self.submitting_score = false;
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = weak.update(cx, |v, cx| {
                if v.index != req_index {
                    return;
                }
                v.submitting_score = false;
                match res {
                    Ok(score) => {
                        v.feedback = Some(SimilarPracticeFeedback::Scored(score));
                        v.preview_choice = ArrowPreviewChoice::Best;
                        v.sync_board(cx);
                    }
                    Err(err) => {
                        v.error = Some(format_domain_error(current_locale(cx), &err).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn reveal_answer(&mut self, cx: &mut Context<Self>) {
        let Some(hit) = self.current().map(clone_similar_hit) else {
            return;
        };
        let Some(facade) = self.facade.clone() else {
            return;
        };
        if self.submitting_score || self.submitting_reveal {
            return;
        }
        self.submitting_reveal = true;
        self.error = None;
        let pos_id = hit.position.id;
        let req_index = self.index;
        let Some(task) = spawn_facade_read(cx, move || async move {
            facade.reveal_similar_practice_answer(pos_id).await
        }) else {
            self.submitting_reveal = false;
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = weak.update(cx, |v, cx| {
                if v.index != req_index {
                    return;
                }
                v.submitting_reveal = false;
                match res {
                    Ok(ans) => {
                        v.feedback = Some(SimilarPracticeFeedback::Revealed(ans));
                        v.preview_choice = ArrowPreviewChoice::Best;
                        v.sync_board(cx);
                    }
                    Err(err) => {
                        v.error = Some(format_domain_error(current_locale(cx), &err).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn advance(&mut self, cx: &mut Context<Self>) {
        let Some(hit) = self.current().map(clone_similar_hit) else {
            return;
        };
        if let Some(SimilarPracticeFeedback::Scored(score)) = &self.feedback {
            self.scored.push((hit, score.equity_loss));
        }
        self.index += 1;
        if self.index >= self.items.len() {
            self.draft = None;
            self.feedback = None;
            if !self.scored.is_empty() {
                let payload = PendingPracticeSummaryPayload {
                    id: self.round_id,
                    completed_at: Utc::now(),
                    total_positions: self.items.len() as i32,
                    losses: self.scored.iter().map(|(_, l)| *l).collect(),
                    is_retry: self.is_retry,
                };
                self.persist_summary(payload, cx);
            }
        } else {
            self.prepare_question(cx);
        }
        cx.notify();
    }

    fn persist_summary(&mut self, payload: PendingPracticeSummaryPayload, cx: &mut Context<Self>) {
        // Persist into LocalUiState::study.pending_similar_practice BEFORE network/DB write
        // so crashes or transient failures never drop a completed round.
        self.parent.update(cx, |parent, cx| {
            parent.enqueue_pending_summary(payload.clone(), cx);
        });
        let Some(facade) = self.facade.clone() else {
            return;
        };
        self.saving_summary = true;
        self.summary_save_error = None;
        self.busy.set_busy(true);
        let backend_input = payload.to_backend();
        let round_id = payload.id;
        let Some(task) = spawn_facade_write(cx, move || async move {
            facade.save_similar_practice_summary(backend_input).await
        }) else {
            self.saving_summary = false;
            self.busy.set_busy(false);
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = weak.update(cx, |v, cx| {
                v.saving_summary = false;
                v.busy.set_busy(false);
                match res {
                    Ok(_) => {
                        v.summary_saved = true;
                        v.summary_save_error = None;
                        v.parent.update(cx, |parent, cx| {
                            parent.remove_pending_summary(round_id, cx);
                            parent.load_history(None, cx);
                        });
                    }
                    Err(err) => {
                        let msg = SharedString::from(format_domain_error(current_locale(cx), &err));
                        v.summary_save_error = Some((payload, msg));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for SimilarPracticeSessionView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = NeonTheme::current_palette(cx);
        let pb = |k: &'static str| I18n::t(locale, "position-browser", k);
        let rm = |k: &'static str| I18n::t(locale, "recurring-mistakes", k);
        let ent = cx.entity();

        let Some(hit) = self.current().map(clone_similar_hit) else {
            let missed_count = self.scored.iter().filter(|(_, l)| *l > 0.0).count();
            let avg_loss = if self.scored.is_empty() {
                None
            } else {
                Some(self.scored.iter().map(|(_, l)| *l).sum::<f64>() / self.scored.len() as f64)
            };

            let mut done = div().v_flex().gap(px(12.0)).child(
                div()
                    .text_subject()
                    .text_color(p.text)
                    .child(rm("complete")),
            );

            if let Some(avg) = avg_loss {
                done = done
                    .child(
                        div()
                            .text_body()
                            .tabular_nums()
                            .text_color(p.text)
                            .child(format!(
                                "{}: {}",
                                pb("practiceAverageLoss"),
                                format_decimal(locale, avg, 3, 6)
                            )),
                    )
                    .child(
                        div()
                            .text_body()
                            .tabular_nums()
                            .text_color(p.text)
                            .child(format!(
                                "{}: {} / {}",
                                pb("practiceScoredMoves"),
                                format_int(locale, self.scored.len() as i64),
                                format_int(locale, self.items.len() as i64)
                            )),
                    );

                if let Some((payload, err)) = self.summary_save_error.clone() {
                    let ent_retry = ent.clone();
                    done = done.child(
                        StatusBanner::new(
                            "sim-prac-save-failed",
                            BannerKind::Error,
                            format!("{} {err}", pb("practiceSaveFailed")),
                        )
                        .palette(p)
                        .on_retry(
                            pb("practiceRetrySave"),
                            move |_, _, cx| {
                                ent_retry
                                    .update(cx, |v, cx| v.persist_summary(payload.clone(), cx));
                            },
                        ),
                    );
                } else if self.saving_summary {
                    done = done.child(
                        div()
                            .text_meta()
                            .text_color(p.muted)
                            .child(pb("practiceSavingSummary")),
                    );
                } else if self.summary_saved {
                    done = done.child(
                        StatusBanner::new(
                            "sim-prac-saved-ok",
                            BannerKind::Success,
                            pb("practiceSavedSummary"),
                        )
                        .palette(p),
                    );
                }
            } else {
                done = done.child(
                    div()
                        .text_body()
                        .text_color(p.muted)
                        .child(pb("practiceNoScores")),
                );
            }

            return done
                .child(
                    div()
                        .h_flex()
                        .justify_end()
                        .gap(px(8.0))
                        .when(missed_count > 0, |this| {
                            let ent_retry = ent.clone();
                            this.child(
                                ActionButton::new("sim-prac-retry-missed")
                                    .icon(ActionIconName::Reset)
                                    .label(pb("practiceRetryMissed"))
                                    .primary()
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        ent_retry.update(cx, |v, cx| v.start_retry_missed(cx));
                                    }),
                            )
                        })
                        .child(
                            ActionButton::new("sim-prac-close-done")
                                .icon(ActionIconName::Close)
                                .label(rm("close"))
                                .palette(p)
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        ),
                )
                .into_any_element();
        };

        let loading = self.submitting_score || self.submitting_reveal;
        let can_undo = self
            .draft
            .as_ref()
            .is_some_and(CheckerPracticeDraft::can_undo);
        let can_submit = self
            .draft
            .as_ref()
            .is_some_and(CheckerPracticeDraft::can_submit);
        let forced_pass = self
            .draft
            .as_ref()
            .is_some_and(CheckerPracticeDraft::is_forced_pass);
        let is_last = self.index + 1 >= self.items.len();

        let mut root = div()
            .v_flex()
            .gap(px(10.0))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_panel()
                            .tabular_nums()
                            .text_color(p.text)
                            .child(format!(
                                "{} / {} · {} · {} {} · {} {}",
                                self.index + 1,
                                self.items.len(),
                                hit.match_name,
                                pb("gameNumber"),
                                hit.position.game_no,
                                pb("moveNumber"),
                                hit.position.ply
                            )),
                    )
                    .child(
                        div()
                            .text_meta()
                            .text_color(p.muted)
                            .child(pb("practicePrompt")),
                    ),
            )
            .child(div().w_full().h(px(360.0)).child(self.board.clone()));

        if loading {
            root = root.child(
                div()
                    .text_meta()
                    .text_color(p.muted)
                    .child(pb("loadingPracticeAnswer")),
            );
        }

        if let Some(err) = self.error.clone() {
            root = root.child(StatusBanner::new("sim-prac-err", BannerKind::Error, err).palette(p));
        }

        if let Some(fb) = &self.feedback {
            let mut fb_box = div()
                .v_flex()
                .gap(px(6.0))
                .p(px(10.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface_alt);

            match fb {
                SimilarPracticeFeedback::Scored(score) => {
                    let ent_pl = ent.clone();
                    let ent_bp = ent.clone();
                    fb_box =
                        fb_box
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        ActionButton::new("sim-prac-arrows-played")
                                            .label(pb("practiceYourMove"))
                                            .active(
                                                self.preview_choice == ArrowPreviewChoice::Played,
                                            )
                                            .palette(p)
                                            .on_click(move |_, _, cx| {
                                                ent_pl.update(cx, |v, cx| {
                                                    v.preview_choice = ArrowPreviewChoice::Played;
                                                    v.sync_board(cx);
                                                });
                                            }),
                                    )
                                    .child(
                                        ActionButton::new("sim-prac-arrows-best")
                                            .label(pb("practiceBestPlay"))
                                            .active(self.preview_choice == ArrowPreviewChoice::Best)
                                            .palette(p)
                                            .on_click(move |_, _, cx| {
                                                ent_bp.update(cx, |v, cx| {
                                                    v.preview_choice = ArrowPreviewChoice::Best;
                                                    v.sync_board(cx);
                                                });
                                            }),
                                    ),
                            )
                            .child(div().text_body().tabular_nums().text_color(p.text).child(
                                format!(
                            "{}: {} · {}: {} ({}) · {}: {} ({})",
                            rm("loss"),
                            format_decimal(locale, score.equity_loss, 3, 6),
                            pb("practiceYourMove"),
                            localize_decision(locale, &score.played),
                            score
                                .played_equity
                                .map(|e| format_equity(locale, e))
                                .unwrap_or_else(|| "—".into()),
                            rm("best"),
                            localize_decision(locale, &score.best),
                            score
                                .best_equity
                                .map(|e| format_equity(locale, e))
                                .unwrap_or_else(|| "—".into())
                        ),
                            ));
                }
                SimilarPracticeFeedback::Revealed(ans) => {
                    fb_box = fb_box.child(div().text_body().text_color(p.text).child(format!(
                        "{}: {} ({}{})",
                        rm("best"),
                        localize_decision(locale, &ans.best),
                        ans.engine,
                        ans.depth
                            .as_deref()
                            .map(|d| format!(" · {d}"))
                            .unwrap_or_default()
                    )));
                }
            }

            root = root.child(fb_box);
        }

        // Action strip
        let mut actions = div().h_flex().flex_wrap().justify_between().gap(px(8.0));
        let mut left_actions = div().h_flex().gap(px(8.0));
        if self.feedback.is_none() {
            let ent_u = ent.clone();
            let ent_c = ent.clone();
            let ent_r = ent.clone();
            left_actions = left_actions
                .child(
                    ActionButton::new("sim-prac-undo")
                        .icon(ActionIconName::Undo)
                        .label(I18n::t(locale, "play", "undo"))
                        .shortcut("Z")
                        .disabled(loading || !can_undo)
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent_u.update(cx, |v, cx| {
                                if let Some(d) = &mut v.draft
                                    && d.undo_step()
                                {
                                    v.sync_board(cx);
                                }
                            });
                        }),
                )
                .child(
                    ActionButton::new("sim-prac-check")
                        .icon(ActionIconName::Confirm)
                        .label(if forced_pass {
                            I18n::t(locale, "play", "pass")
                        } else {
                            pb("practiceCheckMove")
                        })
                        .shortcut("Enter")
                        .primary()
                        .disabled(loading || !can_submit)
                        .palette(p)
                        .on_click(move |_, _, cx| ent_c.update(cx, |v, cx| v.check_move(cx))),
                )
                .child(
                    ActionButton::new("sim-prac-reveal")
                        .icon(ActionIconName::View)
                        .label(rm("reveal"))
                        .disabled(loading)
                        .palette(p)
                        .on_click(move |_, _, cx| ent_r.update(cx, |v, cx| v.reveal_answer(cx))),
                );
        }

        let ent_adv = ent.clone();
        let next_label = if self.submitting_score {
            pb("practiceSkipScoring")
        } else {
            rm(if is_last { "finish" } else { "next" })
        };
        let right_actions = div()
            .h_flex()
            .gap(px(8.0))
            .child(
                ActionButton::new("sim-prac-advance")
                    .icon(ActionIconName::Next)
                    .label(next_label)
                    .variant(if self.feedback.is_some() {
                        ActionButtonVariant::Primary
                    } else {
                        ActionButtonVariant::Default
                    })
                    .palette(p)
                    .on_click(move |_, _, cx| ent_adv.update(cx, |v, cx| v.advance(cx))),
            )
            .child(
                ActionButton::new("sim-prac-close")
                    .icon(ActionIconName::Close)
                    .label(rm("close"))
                    .palette(p)
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            );

        actions = actions.child(left_actions).child(right_actions);
        root.child(actions).into_any_element()
    }
}
