//! All Analyzed Decisions sub-view (`DecisionSearch.tsx`, `DecisionFilterBar.tsx`,
//! `DecisionResultsTable.tsx`, `DecisionPreviewPane.tsx`, `DecisionDeckAction.tsx`,
//! `RecurringMistakePractice.tsx`).
//!
//! Features:
//! - Auto-searches once on first visit for `My player profile` (via `list_recurring_mistakes`
//!   and `search_decisions`)
//! - Filter bar with `DecimalInput` min/max equity loss defaulting to profile Error threshold
//! - Resizable 9-column dense results table with Loss asc/desc sort & server pagination
//! - Right-hand preview pane reusing `BoardView`, `format_cube_context_line`,
//!   `MoveExplanationView`, `SeverityStyle`, and bookmark toggle for checker hits
//! - Emits `WorkspaceEvent::OpenMatchInAnalysis` for BOTH checker (`position_id`) and
//!   cube (`decision_no`) decisions
//! - Single or page-batch (up to 200) Add-to-Deck dialog (`add_decisions_to_deck`)
//! - Launch Practice modal for up to 20 practicable errors on the current page

use super::columns::{DecisionTableColumn, DecisionTableLayout, POSITION_SELECTION_LIMIT};
use super::practice_board::{CheckerPracticeDraft, position_to_board_position};
use super::{
    ALL_GAME_PLANS, ALL_MISTAKE_KINDS, game_plan_label, mistake_kind_label, spawn_facade_read,
    spawn_facade_write,
};
use crate::app_context::{DesktopAppContext, themed_board};
use crate::events::WorkspaceEvent;
use crate::i18n::{current_locale, format_domain_error};
use crate::review::cube_details::{
    CubeDecisionDetailsView, CubeEquityUnit, CubeEvaluationData, format_cube_context_line,
};
use crate::review::move_explanation::{MoveExplanationView, explain_checker_plays};
use crate::review::severity::{SeverityStyle, classify_equity_loss};
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::banners::{BannerKind, EmptyState, StatusBanner};
use crate::ui::decimal_input::{DecimalInput, DecimalInputConfig};
use crate::ui::icons::{ActionIconName, action_icon};
use crate::ui::modal::{
    ModalBusyLock, ModalConfig, ModalSize, open_library_dialog, open_modal_dialog,
};
use crate::ui::pagination::NumberedSearchPagination;
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseMoveEvent, MouseUpEvent,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Subscription,
    TestSupportExt, Window,
    base::StyledExt,
    component::WindowExt,
    component::input::{Input, InputEvent, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::configuration::ErrorThresholds;
use neondojo::databases::Database;
use neondojo::facade::BackendFacade;
use neondojo::game_plan::GamePlan;
use neondojo::learning::{AddDecisionsResult, DecisionAddStatus, Deck};
use neondojo::match_analysis::CubeEquityUnit as BackendCubeEquityUnit;
use neondojo::model::Position;
use neondojo::practice::evaluation::{
    CubeOfferAction, CubeResponseAction, PracticeAnswer, PracticeEvaluation, PracticeVerdict,
};
use neondojo::recurring_mistakes::{
    CubeOwnerFilter, DecisionKind, DecisionSearchPage, DecisionSearchQuery, LossDirection,
    MistakeAnswer, MistakeAttempt, MistakeAttemptRequest, MistakeKind, MistakeReference,
    MistakesQuery,
};
use neondojo_board::{BoardEvent, BoardView, BoardViewMode};
use neondojo_i18n::{
    I18n, Locale, Param, format_decimal, format_equity, format_int, localize_decision,
};
use serde::Deserialize;
use std::rc::Rc;
use uuid::Uuid;

pub const DECISION_PAGE_SIZE: usize = 50;
pub const MAX_PRACTICE_ERRORS: usize = 20;

/// Typed deserialization of `DecisionSearchPage::items` entries (`DecisionHit` with
/// flattened `MistakePracticeQuestion`).
#[derive(Debug, Clone, Deserialize)]
pub struct DecisionHitRow {
    pub reference: MistakeReference,
    /// `"checker"`, `"cube_offer"`, or `"cube_response"`.
    pub kind: String,
    pub position: Position,
    pub players: [String; 2],
    pub engine: String,
    pub plies: Option<i32>,
    #[serde(default)]
    pub depth: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    pub game_plan: GamePlan,
    pub mistake_kind: Option<MistakeKind>,
    pub played: String,
    pub best: String,
    pub equity_loss: f64,
    pub match_length: i32,
    pub away: Option<[i32; 2]>,
    pub crawford: bool,
    pub cube_value: i32,
    pub cube_owner: Option<i32>,
    pub practicable: bool,
}

impl DecisionHitRow {
    /// True when this decision has a stored checker `position.id` (non-nil UUID).
    pub fn has_position(&self) -> bool {
        self.position.id != Uuid::nil()
    }

    /// True when this decision can be opened in Analysis (either checker or cube).
    pub fn is_openable(&self) -> bool {
        self.has_position() || self.position.match_uuid != Uuid::nil()
    }

    /// Builds the exact [`WorkspaceEvent::OpenMatchInAnalysis`] for either a checker
    /// decision (`position_id: Some(...)`) or a cube decision (`decision_no: Some(...)`).
    pub fn to_open_analysis_event(&self) -> Option<WorkspaceEvent> {
        if !self.is_openable() {
            return None;
        }
        if self.kind == "checker" && self.has_position() {
            Some(WorkspaceEvent::OpenMatchInAnalysis {
                match_id: self.position.match_uuid,
                game_no: Some(self.reference.game_no),
                position_id: Some(self.position.id),
                decision_no: None,
            })
        } else {
            Some(WorkspaceEvent::OpenMatchInAnalysis {
                match_id: self.position.match_uuid,
                game_no: Some(self.reference.game_no),
                position_id: None,
                decision_no: Some(self.reference.decision_no),
            })
        }
    }

    /// Stable reference key uniquely identifying a hit within a result page.
    pub fn ref_key(&self) -> String {
        let src = self
            .reference
            .job_id
            .map(|id| format!("j:{id}"))
            .or_else(|| {
                self.reference
                    .imported_analysis_id
                    .map(|id| format!("i:{id}"))
            })
            .unwrap_or_else(|| "none".into());
        format!(
            "{}:{}:{}",
            src, self.reference.game_no, self.reference.decision_no
        )
    }
}

/// Filter draft for All Analyzed Decisions.
#[derive(Debug, Clone, Default)]
pub struct DecisionFilterDraft {
    pub player: String,
    pub database_id: Option<Uuid>,
    pub kind: Option<DecisionKind>,
    pub mistake_kind: Option<MistakeKind>,
    pub game_plan: Option<GamePlan>,
    /// `None` inherits the profile's Error threshold (`error_thresholds.error`).
    pub min_loss: Option<String>,
    pub max_loss: String,
    pub away: String,
    pub opponent_away: String,
    pub cube_value: Option<i32>,
    pub cube_owner: Option<CubeOwnerFilter>,
}

impl DecisionFilterDraft {
    pub fn to_query(
        &self,
        profile_error_loss: f64,
        direction: LossDirection,
        page: usize,
    ) -> Option<DecisionSearchQuery> {
        if self.player.trim().is_empty() {
            return None;
        }
        let parse_opt_f64 = |raw: &str| -> Result<Option<f64>, ()> {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            let v = neondojo_i18n::canonical_decimal(trimmed)
                .parse::<f64>()
                .map_err(|_| ())?;
            if !v.is_finite() || !(0.0..=1000.0).contains(&v) {
                return Err(());
            }
            Ok(Some(v))
        };
        let parse_opt_away = |raw: &str| -> Result<Option<i32>, ()> {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            let v = trimmed.parse::<i32>().map_err(|_| ())?;
            if !(1..=64).contains(&v) {
                return Err(());
            }
            Ok(Some(v))
        };

        let min_loss = match &self.min_loss {
            None => Some(profile_error_loss),
            Some(s) => parse_opt_f64(s).ok()?,
        };
        let max_loss = parse_opt_f64(&self.max_loss).ok()?;
        if let (Some(lo), Some(hi)) = (min_loss, max_loss)
            && hi < lo
        {
            return None;
        }
        let away = parse_opt_away(&self.away).ok()?;
        let opponent_away = parse_opt_away(&self.opponent_away).ok()?;

        Some(DecisionSearchQuery {
            player: self.player.trim().to_string(),
            database_id: self.database_id,
            kind: self.kind,
            mistake_kind: self.mistake_kind,
            game_plan: self.game_plan,
            min_loss,
            max_loss,
            away,
            opponent_away,
            cube_value: self.cube_value,
            cube_owner: self.cube_owner,
            direction: Some(direction),
            page: Some(page.max(1)),
            limit: Some(DECISION_PAGE_SIZE),
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct DecisionResizeDrag {
    column: DecisionTableColumn,
    start_x: f32,
    start_width: u32,
}

/// Complete GPUI All Analyzed Decisions sub-view.
pub struct DecisionSearchView {
    facade: Option<BackendFacade>,
    visible: bool,
    initialized: bool,
    player_manually_chosen: bool,
    auto_search_done: bool,

    // Reference data
    databases: Vec<Database>,
    players: Vec<String>,
    error_thresholds: ErrorThresholds,
    metadata_task: Option<gpui_kit::Task<()>>,
    search_task: Option<gpui_kit::Task<()>>,

    // Filters
    draft: DecisionFilterDraft,
    min_loss_input: Entity<DecimalInput>,
    max_loss_input: Entity<DecimalInput>,
    away_input: Entity<InputState>,
    opponent_away_input: Entity<InputState>,
    more_filters_open: bool,

    // Applied query & results
    applied_query: Option<DecisionFilterDraft>,
    direction: LossDirection,
    page: usize,
    total: usize,
    items: Vec<DecisionHitRow>,
    loading: bool,
    error: Option<SharedString>,
    selected_key: Option<String>,
    preview_board: Entity<BoardView>,
    preview_bookmarked: Option<bool>,
    bookmark_busy: bool,
    bookmark_read: Option<gpui_kit::Task<()>>,

    // Table layout & resize
    table_layout: DecisionTableLayout,
    column_picker_open: bool,
    resizing: Option<DecisionResizeDrag>,

    // Last add-to-deck outcome banner
    deck_outcome_banner: Option<SharedString>,

    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<WorkspaceEvent> for DecisionSearchView {}

impl DecisionSearchView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = current_locale(cx);
        let sep = locale.decimal_separator();
        let app_ctx = cx.try_global::<DesktopAppContext>().cloned();
        let facade = app_ctx.as_ref().and_then(|c| c.facade());
        let thresholds = ErrorThresholds::default();

        let preview_board = cx.new(|cx| themed_board("decision-search-preview-board", cx));

        let weak = cx.weak_entity();
        let ds = |k: &'static str| I18n::t(locale, "decision-search", k);

        let min_loss_input = cx.new(|cx| {
            let w = weak.clone();
            DecimalInput::new(
                DecimalInputConfig::new("decision-min-loss", 0.005)
                    .min(0.0)
                    .max(1000.0)
                    .separator(sep)
                    .placeholder(ds("minLoss"))
                    .width(96.0),
                thresholds.error.to_string(),
                Some(Rc::new(move |val, _, cx| {
                    let s = val.to_string();
                    let _ = w.update(cx, |this, cx| {
                        this.draft.min_loss = Some(s);
                        cx.notify();
                    });
                })),
                window,
                cx,
            )
        });

        let max_loss_input = cx.new(|cx| {
            let w = weak.clone();
            DecimalInput::new(
                DecimalInputConfig::new("decision-max-loss", 0.005)
                    .min(0.0)
                    .max(1000.0)
                    .separator(sep)
                    .placeholder(ds("maxLoss"))
                    .width(96.0),
                "",
                Some(Rc::new(move |val, _, cx| {
                    let s = val.to_string();
                    let _ = w.update(cx, |this, cx| {
                        this.draft.max_loss = s;
                        cx.notify();
                    });
                })),
                window,
                cx,
            )
        });

        let away_input = cx.new(|cx| InputState::new(window, cx).placeholder("1–64"));
        let opponent_away_input = cx.new(|cx| InputState::new(window, cx).placeholder("1–64"));

        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe_in(
            &away_input,
            window,
            |this, state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.draft.away = state.read(cx).value().to_string();
                    cx.notify();
                }
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &opponent_away_input,
            window,
            |this, state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.draft.opponent_away = state.read(cx).value().to_string();
                    cx.notify();
                }
            },
        ));

        Self {
            facade,
            visible: false,
            initialized: false,
            player_manually_chosen: false,
            auto_search_done: false,
            databases: Vec::new(),
            players: Vec::new(),
            error_thresholds: thresholds,
            metadata_task: None,
            search_task: None,
            draft: DecisionFilterDraft::default(),
            min_loss_input,
            max_loss_input,
            away_input,
            opponent_away_input,
            more_filters_open: false,
            applied_query: None,
            direction: LossDirection::Desc,
            page: 1,
            total: 0,
            items: Vec::new(),
            loading: false,
            error: None,
            selected_key: None,
            preview_board,
            preview_bookmarked: None,
            bookmark_busy: false,
            bookmark_read: None,
            table_layout: DecisionTableLayout::default(),
            column_picker_open: false,
            resizing: None,
            deck_outcome_banner: None,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
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
            if !self.initialized && self.metadata_task.is_none() && self.facade.is_some() {
                self.bootstrap_and_autosearch(cx);
            }
        }
        cx.notify();
    }

    /// Test & inspection helpers.
    pub fn set_items_for_test(&mut self, items: Vec<DecisionHitRow>, cx: &mut Context<Self>) {
        self.total = items.len();
        self.selected_key = items.first().map(DecisionHitRow::ref_key);
        self.items = items;
        self.sync_preview_board(cx);
        cx.notify();
    }

    pub fn open_hit_in_analysis(&self, hit: &DecisionHitRow, cx: &mut Context<Self>) {
        if let Some(ev) = hit.to_open_analysis_event() {
            cx.emit(ev);
        }
    }

    pub fn items(&self) -> &[DecisionHitRow] {
        &self.items
    }

    pub fn practicable_page_items(&self) -> Vec<DecisionHitRow> {
        self.items
            .iter()
            .filter(|h| h.practicable)
            .take(MAX_PRACTICE_ERRORS)
            .cloned()
            .collect()
    }

    fn active_preview(&self) -> Option<&DecisionHitRow> {
        self.selected_key
            .as_deref()
            .and_then(|k| self.items.iter().find(|h| h.ref_key() == k))
            .or_else(|| self.items.first())
    }

    fn sync_preview_board(&mut self, cx: &mut Context<Self>) {
        self.bookmark_read = None;
        self.preview_bookmarked = None;
        let Some(hit) = self.active_preview().cloned() else {
            return;
        };
        let bp = position_to_board_position(&hit.position, true);
        let white = hit.players[0].clone();
        let black = hit.players[1].clone();
        let pending_cube = hit.kind == "cube_response";
        self.preview_board.update(cx, |bv, cx| {
            bv.set_position(bp, cx);
            bv.set_players(white, black, cx);
            bv.set_cube_offer_pending(pending_cube, cx);
        });
        self.load_preview_bookmark_state(&hit, cx);
    }

    fn load_preview_bookmark_state(&mut self, hit: &DecisionHitRow, cx: &mut Context<Self>) {
        if !hit.has_position() {
            self.preview_bookmarked = None;
            return;
        }
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let pos_id = hit.position.id;
        let Some(task) = spawn_facade_read(cx, move || async move {
            facade.get_position_bookmark(pos_id).await
        }) else {
            return;
        };
        self.bookmark_read = Some(cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |view, cx| {
                view.bookmark_read = None;
                if view
                    .active_preview()
                    .is_some_and(|hit| hit.position.id == pos_id)
                {
                    view.preview_bookmarked = result
                        .ok()
                        .and_then(Result::ok)
                        .map(|bookmark| bookmark.saved);
                }
                cx.notify();
            });
        }));
    }

    fn toggle_preview_bookmark(&mut self, cx: &mut Context<Self>) {
        let Some(hit) = self.active_preview().cloned() else {
            return;
        };
        if !hit.has_position() || self.bookmark_busy {
            return;
        }
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let Some(currently_saved) = self.preview_bookmarked else {
            self.load_preview_bookmark_state(&hit, cx);
            cx.notify();
            return;
        };
        let pos_id = hit.position.id;
        self.bookmark_busy = true;
        let Some(task) = spawn_facade_write(cx, move || async move {
            if currently_saved {
                facade.unbookmark_position(pos_id).await.map(|()| false)
            } else {
                facade.bookmark_position(pos_id).await.map(|_| true)
            }
        }) else {
            self.bookmark_busy = false;
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let result = task.await;
            let _ = weak.update(cx, |view, cx| {
                view.bookmark_busy = false;
                if view
                    .active_preview()
                    .is_some_and(|hit| hit.position.id == pos_id)
                {
                    view.bookmark_read = None;
                    view.preview_bookmarked = result.ok().and_then(Result::ok);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// The same scoped player list and resolved virtual profile used by the browser.
    fn bootstrap_and_autosearch(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let database_id = self.draft.database_id;
        let Some(task) = spawn_facade_read(cx, move || async move {
            let players = facade
                .list_recurring_mistakes(MistakesQuery {
                    database_id,
                    ..MistakesQuery::default()
                })
                .await?;
            let databases = facade.list_databases().await?;
            let thresholds = match facade.get_configuration().await {
                Ok(cfg) => cfg.current_configuration.analysis.error_thresholds,
                Err(err) if err.body.code.as_deref() == Some("configuration_not_initialized") => {
                    ErrorThresholds::default()
                }
                Err(err) => return Err(err),
            };
            Ok((players, databases, thresholds))
        }) else {
            return;
        };
        self.error = None;
        self.metadata_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |view, cx| {
                view.metadata_task = None;
                let locale = current_locale(cx);
                match result {
                    Ok(Ok((players, databases, thresholds))) => {
                        view.initialized = true;
                        view.databases = databases;
                        view.error_thresholds = thresholds;
                        view.players = players.players;
                        if !view.players.contains(&view.draft.player) {
                            view.draft.player = if view.player_manually_chosen {
                                String::new()
                            } else {
                                players.resolved_player.unwrap_or_default()
                            };
                        }
                        if !view.auto_search_done
                            && !view.player_manually_chosen
                            && !view.draft.player.is_empty()
                        {
                            view.auto_search_done = true;
                            view.submit_search(cx);
                        }
                    }
                    Ok(Err(err)) => {
                        view.initialized = false;
                        view.error = Some(format_domain_error(locale, &err).into());
                    }
                    Err(_) => {
                        view.initialized = false;
                        view.error = Some(I18n::t(locale, "app", "error.invalidResponse").into());
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub fn submit_search(&mut self, cx: &mut Context<Self>) {
        if self
            .draft
            .to_query(self.error_thresholds.error, self.direction, 1)
            .is_none()
        {
            return;
        }
        self.applied_query = Some(self.draft.clone());
        self.page = 1;
        self.execute_search(cx);
    }

    pub fn clear_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let database_changed = self.draft.database_id.is_some();
        let keep_player = self.draft.player.clone();
        self.draft = DecisionFilterDraft {
            player: keep_player,
            ..DecisionFilterDraft::default()
        };
        if database_changed {
            self.bootstrap_and_autosearch(cx);
        }
        let default_loss = self.error_thresholds.error.to_string();
        self.min_loss_input
            .update(cx, |i, cx| i.set_canonical_value(&default_loss, window, cx));
        self.max_loss_input
            .update(cx, |i, cx| i.set_canonical_value("", window, cx));
        self.away_input
            .update(cx, |i, cx| i.set_value(String::new(), window, cx));
        self.opponent_away_input
            .update(cx, |i, cx| i.set_value(String::new(), window, cx));
        if self.applied_query.is_some() {
            self.submit_search(cx);
        }
        cx.notify();
    }

    pub fn toggle_loss_sort(&mut self, cx: &mut Context<Self>) {
        self.direction = match self.direction {
            LossDirection::Desc => LossDirection::Asc,
            LossDirection::Asc => LossDirection::Desc,
        };
        self.page = 1;
        if self.applied_query.is_some() {
            self.execute_search(cx);
        }
        cx.notify();
    }

    pub fn go_to_page(&mut self, page: usize, cx: &mut Context<Self>) {
        self.page = page.max(1);
        if self.applied_query.is_some() {
            self.execute_search(cx);
        }
        cx.notify();
    }

    fn execute_search(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let Some(applied) = &self.applied_query else {
            return;
        };
        let Some(query) = applied.to_query(self.error_thresholds.error, self.direction, self.page)
        else {
            return;
        };
        self.loading = true;
        self.error = None;

        let Some(task) =
            spawn_facade_read(
                cx,
                move || async move { facade.search_decisions(query).await },
            )
        else {
            self.loading = false;
            return;
        };
        self.search_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |view, cx| {
                view.loading = false;
                let locale = current_locale(cx);
                let page = match result {
                    Ok(result) => result.map_err(|e| format_domain_error(locale, &e).into()),
                    Err(_) => Err(I18n::t(locale, "app", "error.invalidResponse").into()),
                };
                view.apply_search_page(page, cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn apply_search_page(
        &mut self,
        page: Result<DecisionSearchPage, SharedString>,
        cx: &mut Context<Self>,
    ) {
        let result = page.and_then(|page| {
            let items = page
                .items
                .into_iter()
                .map(serde_json::from_value)
                .collect::<Result<Vec<DecisionHitRow>, _>>()
                .map_err(|err| {
                    tracing::warn!(%err, "Could not read analyzed decision");
                    SharedString::from(I18n::t(current_locale(cx), "app", "error.invalidResponse"))
                })?;
            Ok((items, page.total, page.page))
        });
        match result {
            Ok((items, total, page)) => {
                self.items = items;
                self.total = total;
                self.page = page;
                self.error = None;
                if !self
                    .items
                    .iter()
                    .any(|item| Some(item.ref_key()) == self.selected_key)
                {
                    self.selected_key = self.items.first().map(DecisionHitRow::ref_key);
                }
            }
            Err(error) => {
                self.items.clear();
                self.selected_key = None;
                self.total = 0;
                self.error = Some(error);
            }
        }
        self.sync_preview_board(cx);
    }

    fn open_add_to_deck_dialog(
        &mut self,
        references: Vec<MistakeReference>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if references.is_empty() {
            return;
        }
        let capped: Vec<MistakeReference> = references
            .into_iter()
            .take(POSITION_SELECTION_LIMIT)
            .collect();
        let locale = current_locale(cx);
        let busy = ModalBusyLock::new(false);
        let parent = cx.entity();
        let facade = self.facade.clone();
        let dialog = cx.new(|cx| {
            DecisionDeckDialogView::new(facade, capped, busy.clone(), parent, window, cx)
        });
        let body = dialog.clone();
        open_library_dialog(
            window,
            cx,
            I18n::t(locale, "position-browser", "addToDeck"),
            busy,
            move |_, _| body.clone().into_any_element(),
            None::<fn(&mut Window, &mut App) -> gpui_kit::AnyElement>,
        );
    }

    fn open_practice_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let questions = self.practicable_page_items();
        if questions.is_empty() {
            return;
        }
        let locale = current_locale(cx);
        let busy = ModalBusyLock::new(false);
        let facade = self.facade.clone();
        let dialog = cx
            .new(|cx| DecisionPracticeDialogView::new(facade, questions, busy.clone(), window, cx));
        let body = dialog.clone();
        let cfg = ModalConfig::new(I18n::t(locale, "recurring-mistakes", "practiceTitle"))
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

    fn handle_table_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.items.is_empty() {
            return;
        }
        let current = self
            .selected_key
            .as_deref()
            .and_then(|k| self.items.iter().position(|h| h.ref_key() == k))
            .unwrap_or(0);
        match event.keystroke.key.as_str() {
            "up" => {
                let next = current.saturating_sub(1);
                self.selected_key = Some(self.items[next].ref_key());
                self.sync_preview_board(cx);
                cx.stop_propagation();
                cx.notify();
            }
            "down" => {
                let next = (current + 1).min(self.items.len() - 1);
                self.selected_key = Some(self.items[next].ref_key());
                self.sync_preview_board(cx);
                cx.stop_propagation();
                cx.notify();
            }
            "home" => {
                self.selected_key = Some(self.items[0].ref_key());
                self.sync_preview_board(cx);
                cx.stop_propagation();
                cx.notify();
            }
            "end" => {
                self.selected_key = Some(self.items[self.items.len() - 1].ref_key());
                self.sync_preview_board(cx);
                cx.stop_propagation();
                cx.notify();
            }
            "enter" => {
                if let Some(hit) = self.items.get(current).cloned() {
                    self.open_hit_in_analysis(&hit, cx);
                    cx.stop_propagation();
                }
            }
            _ => {}
        }
    }

    fn handle_resize_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.resizing else {
            return;
        };
        let delta = (f32::from(event.position.x) - drag.start_x).round() as i32;
        let next = (i64::from(drag.start_width) + i64::from(delta)).max(0) as u32;
        self.table_layout.set_width(drag.column, next);
        cx.notify();
    }

    fn handle_resize_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.resizing.take().is_some() {
            cx.notify();
        }
    }

    fn render_filter_bar(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ent = cx.entity();
        let ds = |k: &'static str| I18n::t(locale, "decision-search", k);
        let rm = |k: &'static str| I18n::t(locale, "recurring-mistakes", k);
        let pb = |k: &'static str| I18n::t(locale, "position-browser", k);

        // Player selector
        let mut player_row = div()
            .id("ds-player-list")
            .max_h(px(90.0))
            .overflow_y_scroll()
            .h_flex()
            .flex_wrap()
            .items_center()
            .gap(px(4.0))
            .child(div().text_label().text_color(p.muted).child(rm("player")));
        if self.players.is_empty() {
            player_row =
                player_row.child(div().text_meta().text_color(p.muted).child(rm("noPlayers")));
        } else {
            for pl in &self.players {
                let pl_name = pl.clone();
                let ent_pl = ent.clone();
                player_row = player_row.child(
                    ActionButton::new(SharedString::from(format!("ds-player-{pl_name}")))
                        .label(pl_name.clone())
                        .active(self.draft.player == pl_name)
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent_pl.update(cx, |v, cx| {
                                v.player_manually_chosen = true;
                                v.draft.player = pl_name.clone();
                                cx.notify();
                            });
                        }),
                );
            }
        }

        // Decision kind selector
        let mut kind_row = div()
            .h_flex()
            .flex_wrap()
            .items_center()
            .gap(px(4.0))
            .child(div().text_label().text_color(p.muted).child(ds("kind")));
        for (opt, lbl) in [
            (None, ds("any")),
            (Some(DecisionKind::Checker), rm("checker")),
            (Some(DecisionKind::CubeOffer), rm("cube_offer")),
            (Some(DecisionKind::CubeResponse), rm("cube_response")),
        ] {
            let ent_k = ent.clone();
            kind_row = kind_row.child(
                ActionButton::new(SharedString::from(format!("ds-kind-{opt:?}")))
                    .label(lbl)
                    .active(self.draft.kind == opt)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_k.update(cx, |v, cx| {
                            v.draft.kind = opt;
                            cx.notify();
                        });
                    }),
            );
        }

        // Mistake type selector
        let mut mistake_row = div()
            .h_flex()
            .flex_wrap()
            .items_center()
            .gap(px(4.0))
            .child(div().text_label().text_color(p.muted).child(ds("mistake")))
            .child({
                let ent_m = ent.clone();
                ActionButton::new("ds-mistake-any")
                    .label(ds("any"))
                    .active(self.draft.mistake_kind.is_none())
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_m.update(cx, |v, cx| {
                            v.draft.mistake_kind = None;
                            cx.notify();
                        });
                    })
            });
        for &mk in ALL_MISTAKE_KINDS {
            let ent_m = ent.clone();
            mistake_row = mistake_row.child(
                ActionButton::new(SharedString::from(format!("ds-mistake-{mk:?}")))
                    .label(mistake_kind_label(locale, mk))
                    .active(self.draft.mistake_kind == Some(mk))
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_m.update(cx, |v, cx| {
                            v.draft.mistake_kind = Some(mk);
                            cx.notify();
                        });
                    }),
            );
        }

        let can_search = self
            .draft
            .to_query(self.error_thresholds.error, self.direction, 1)
            .is_some();

        let action_row = div()
            .h_flex()
            .flex_wrap()
            .items_end()
            .gap(px(8.0))
            .child(
                div()
                    .v_flex()
                    .gap(px(2.0))
                    .child(div().text_meta().text_color(p.muted).child(ds("minLoss")))
                    .child(self.min_loss_input.clone()),
            )
            .child(
                div()
                    .v_flex()
                    .gap(px(2.0))
                    .child(div().text_meta().text_color(p.muted).child(ds("maxLoss")))
                    .child(self.max_loss_input.clone()),
            )
            .child({
                let ent_s = ent.clone();
                ActionButton::new("ds-search-submit")
                    .icon(ActionIconName::Search)
                    .label(ds("search"))
                    .primary()
                    .disabled(!can_search || self.loading)
                    .palette(p)
                    .on_click(move |_, _, cx| ent_s.update(cx, |v, cx| v.submit_search(cx)))
            })
            .child({
                let ent_m = ent.clone();
                ActionButton::new("ds-more-filters-btn")
                    .icon(ActionIconName::Filter)
                    .label(pb("filters"))
                    .active(self.more_filters_open)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_m.update(cx, |v, cx| {
                            v.more_filters_open = !v.more_filters_open;
                            cx.notify();
                        });
                    })
            })
            .child({
                let ent_c = ent.clone();
                ActionButton::new("ds-clear-btn")
                    .icon(ActionIconName::Clear)
                    .tooltip(pb("clear"))
                    .compact()
                    .palette(p)
                    .on_click(move |_, window, cx| {
                        ent_c.update(cx, |v, cx| v.clear_filters(window, cx));
                    })
            })
            .when(self.draft.player.is_empty(), |this| {
                this.child(
                    div()
                        .text_meta()
                        .text_color(p.muted)
                        .child(ds("choosePlayer")),
                )
            });

        // Expandable More Filters (Database, Game Plan, Away, Opp Away, Cube Value, Cube Owner)
        let more_panel = self.more_filters_open.then(|| {
            let mut db_row = div()
                .h_flex()
                .flex_wrap()
                .items_center()
                .gap(px(4.0))
                .child(div().text_meta().text_color(p.muted).child(ds("database")))
                .child({
                    let ent_d = ent.clone();
                    ActionButton::new("ds-db-all")
                        .label(ds("allDatabases"))
                        .active(self.draft.database_id.is_none())
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent_d.update(cx, |v, cx| {
                                v.draft.database_id = None;
                                v.bootstrap_and_autosearch(cx);
                            });
                        })
                });
            for db in &self.databases {
                let db_id = db.id;
                let ent_d = ent.clone();
                db_row = db_row.child(
                    ActionButton::new(SharedString::from(format!("ds-db-{db_id}")))
                        .label(db.name.clone())
                        .active(self.draft.database_id == Some(db_id))
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent_d.update(cx, |v, cx| {
                                v.draft.database_id = Some(db_id);
                                v.bootstrap_and_autosearch(cx);
                            });
                        }),
                );
            }

            let mut plan_row = div()
                .h_flex()
                .flex_wrap()
                .items_center()
                .gap(px(4.0))
                .child(div().text_meta().text_color(p.muted).child(ds("plan")))
                .child({
                    let ent_p = ent.clone();
                    ActionButton::new("ds-plan-any")
                        .label(ds("any"))
                        .active(self.draft.game_plan.is_none())
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent_p.update(cx, |v, cx| {
                                v.draft.game_plan = None;
                                cx.notify();
                            });
                        })
                });
            for &plan in ALL_GAME_PLANS {
                let ent_p = ent.clone();
                plan_row = plan_row.child(
                    ActionButton::new(SharedString::from(format!("ds-plan-{plan:?}")))
                        .label(game_plan_label(locale, plan))
                        .active(self.draft.game_plan == Some(plan))
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent_p.update(cx, |v, cx| {
                                v.draft.game_plan = Some(plan);
                                cx.notify();
                            });
                        }),
                );
            }

            let mut cube_val_row = div()
                .h_flex()
                .flex_wrap()
                .items_center()
                .gap(px(4.0))
                .child(div().text_meta().text_color(p.muted).child(ds("cubeValue")))
                .child({
                    let ent_cv = ent.clone();
                    ActionButton::new("ds-cube-any")
                        .label(ds("any"))
                        .active(self.draft.cube_value.is_none())
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent_cv.update(cx, |v, cx| {
                                v.draft.cube_value = None;
                                cx.notify();
                            });
                        })
                });
            for val in [1, 2, 4, 8, 16, 32, 64] {
                let ent_cv = ent.clone();
                cube_val_row = cube_val_row.child(
                    ActionButton::new(SharedString::from(format!("ds-cube-{val}")))
                        .label(val.to_string())
                        .active(self.draft.cube_value == Some(val))
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent_cv.update(cx, |v, cx| {
                                v.draft.cube_value = Some(val);
                                cx.notify();
                            });
                        }),
                );
            }

            let mut owner_row = div()
                .h_flex()
                .flex_wrap()
                .items_center()
                .gap(px(4.0))
                .child(div().text_meta().text_color(p.muted).child(ds("cubeOwner")));
            for (opt, key) in [
                (None, "any"),
                (Some(CubeOwnerFilter::Own), "ownerSelf"),
                (Some(CubeOwnerFilter::Opponent), "ownerOpponent"),
                (Some(CubeOwnerFilter::Centered), "ownerCentered"),
            ] {
                let ent_o = ent.clone();
                owner_row = owner_row.child(
                    ActionButton::new(SharedString::from(format!("ds-owner-{key}")))
                        .label(ds(key))
                        .active(self.draft.cube_owner == opt)
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent_o.update(cx, |v, cx| {
                                v.draft.cube_owner = opt;
                                cx.notify();
                            });
                        }),
                );
            }

            div()
                .id("decision-more-filters")
                .test_support()
                .v_flex()
                .gap(px(8.0))
                .pt(px(8.0))
                .border_t_1()
                .border_color(p.border)
                .child(db_row)
                .child(plan_row)
                .child(
                    div()
                        .h_flex()
                        .flex_wrap()
                        .items_end()
                        .gap(px(10.0))
                        .child(
                            div()
                                .v_flex()
                                .gap(px(2.0))
                                .child(div().text_meta().text_color(p.muted).child(ds("away")))
                                .child(
                                    div()
                                        .w(px(76.0))
                                        .child(Input::new(&self.away_input).id("ds-away-input")),
                                ),
                        )
                        .child(
                            div()
                                .v_flex()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_meta()
                                        .text_color(p.muted)
                                        .child(ds("opponentAway")),
                                )
                                .child(div().w(px(76.0)).child(
                                    Input::new(&self.opponent_away_input).id("ds-opp-away-input"),
                                )),
                        )
                        .child(cube_val_row)
                        .child(owner_row),
                )
        });

        div()
            .v_flex()
            .gap(px(8.0))
            .p(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(player_row)
            .child(kind_row)
            .child(mistake_row)
            .child(action_row)
            .when_some(more_panel, |this, panel| this.child(panel))
    }

    fn render_table(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ent = cx.entity();
        let ds = |k: &'static str| I18n::t(locale, "decision-search", k);
        let lib = |k: &'static str| I18n::t(locale, "library", k);

        let practicable_count = self.practicable_page_items().len();
        let page_refs: Vec<MistakeReference> = self
            .items
            .iter()
            .take(POSITION_SELECTION_LIMIT)
            .map(|h| h.reference.clone())
            .collect();

        let toolbar = div()
            .h_flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .when(
                        self.applied_query.is_some() && !self.loading && self.error.is_none(),
                        |el| {
                            el.child(div().text_label().tabular_nums().text_color(p.muted).child(
                                I18n::format(
                                    locale,
                                    "decision-search",
                                    "found",
                                    &[("count", Param::from(self.total as i64))],
                                ),
                            ))
                        },
                    )
                    .child({
                        let ent_pr = ent.clone();
                        ActionButton::new("ds-practice-page-btn")
                            .icon(ActionIconName::Cards)
                            .label(ds("practice"))
                            .tooltip(ds("practiceHint"))
                            .disabled(practicable_count == 0)
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent_pr.update(cx, |v, cx| v.open_practice_modal(window, cx));
                            })
                    })
                    .child({
                        let ent_ad = ent.clone();
                        let refs = page_refs;
                        ActionButton::new("ds-add-page-deck-btn")
                            .icon(ActionIconName::Deck)
                            .label(ds("addPage"))
                            .tooltip(ds("addPageHint"))
                            .disabled(refs.is_empty())
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent_ad.update(cx, |v, cx| {
                                    v.open_add_to_deck_dialog(refs.clone(), window, cx);
                                });
                            })
                    }),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .child({
                        let ent_cp = ent.clone();
                        ActionButton::new("ds-column-picker-btn")
                            .icon(ActionIconName::Settings)
                            .label(lib("columns"))
                            .active(self.column_picker_open)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_cp.update(cx, |v, cx| {
                                    v.column_picker_open = !v.column_picker_open;
                                    cx.notify();
                                });
                            })
                    })
                    .child({
                        let ent_rw = ent.clone();
                        ActionButton::new("ds-reset-widths-btn")
                            .icon(ActionIconName::Reset)
                            .tooltip(lib("resetColumnWidths"))
                            .compact()
                            .disabled(!self.table_layout.has_custom_widths())
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_rw.update(cx, |v, cx| {
                                    v.table_layout.reset_widths();
                                    cx.notify();
                                });
                            })
                    }),
            );

        let col_picker = self.column_picker_open.then(|| {
            let mut row = div()
                .h_flex()
                .flex_wrap()
                .gap(px(6.0))
                .p(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface_alt);
            for &col in DecisionTableColumn::ALL {
                let shown = self.table_layout.visible_columns.contains(&col);
                let ent_c = ent.clone();
                row = row.child(
                    ActionButton::new(SharedString::from(format!("ds-col-opt-{}", col.id())))
                        .label(ds(col.id()))
                        .active(shown)
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent_c.update(cx, |v, cx| {
                                v.table_layout.toggle_column(col, !shown);
                                cx.notify();
                            });
                        }),
                );
            }
            row.child({
                let ent_all = ent.clone();
                ActionButton::new("ds-col-show-all")
                    .label(lib("showAllColumns"))
                    .variant(ActionButtonVariant::Ghost)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_all.update(cx, |v, cx| {
                            v.table_layout.show_all();
                            cx.notify();
                        });
                    })
            })
        });

        let mut header_row = div()
            .h_flex()
            .items_center()
            .border_b_1()
            .border_color(p.border)
            .bg(p.surface_alt)
            .py(px(6.0));

        for &col in &self.table_layout.visible_columns {
            let w = px(self.table_layout.width_for(col) as f32);
            let ent_drag = ent.clone();
            let start_w = self.table_layout.width_for(col);
            let label_el = if col == DecisionTableColumn::Loss {
                let ent_sort = ent.clone();
                let icon = match self.direction {
                    LossDirection::Asc => ActionIconName::SortUp,
                    LossDirection::Desc => ActionIconName::SortDown,
                };
                div()
                    .id("ds-sort-loss")
                    .test_support()
                    .h_flex()
                    .items_center()
                    .gap(px(4.0))
                    .cursor_pointer()
                    .text_color(p.accent)
                    .on_click(move |_, _, cx| ent_sort.update(cx, |v, cx| v.toggle_loss_sort(cx)))
                    .child(ds("loss"))
                    .child(action_icon(icon, 12.0, p.accent))
                    .into_any_element()
            } else {
                div().truncate().child(ds(col.id())).into_any_element()
            };

            let handle = div()
                .id(SharedString::from(format!("ds-resize-{}", col.id())))
                .w(px(6.0))
                .h(px(18.0))
                .flex_shrink_0()
                .cursor(gpui_kit::CursorStyle::ResizeLeftRight)
                .hover(move |s| s.bg(p.accent))
                .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                    ent_drag.update(cx, |v, _| {
                        v.resizing = Some(DecisionResizeDrag {
                            column: col,
                            start_x: f32::from(event.position.x),
                            start_width: start_w,
                        });
                    });
                });

            header_row = header_row.child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .w(w)
                    .flex_shrink_0()
                    .px(px(6.0))
                    .text_table_heading()
                    .text_color(p.muted)
                    .child(label_el)
                    .child(handle),
            );
        }

        let active_key = self.active_preview().map(DecisionHitRow::ref_key);
        let mut rows_col = div().v_flex();
        for hit in &self.items {
            let key = hit.ref_key();
            let is_active = active_key.as_deref() == Some(key.as_str());
            let ent_sel = ent.clone();
            let key_click = key.clone();

            let mut row = div()
                .id(SharedString::from(format!("ds-row-{key}")))
                .test_support()
                .h_flex()
                .items_center()
                .min_h(px(34.0))
                .py(px(4.0))
                .border_b_1()
                .border_color(p.border)
                .bg(if is_active { p.selected } else { p.surface })
                .cursor_pointer()
                .hover(move |s| s.bg(if is_active { p.selected } else { p.surface_alt }))
                .on_click(move |_, window, cx| {
                    ent_sel.update(cx, |v, cx| {
                        window.focus(&v.focus_handle, cx);
                        v.selected_key = Some(key_click.clone());
                        v.sync_preview_board(cx);
                        cx.notify();
                    });
                });

            for &col in &self.table_layout.visible_columns {
                let w = px(self.table_layout.width_for(col) as f32);
                let mut cell = div()
                    .w(w)
                    .flex_shrink_0()
                    .px(px(6.0))
                    .text_table()
                    .text_color(p.text)
                    .truncate();
                if col.is_numeric() {
                    cell = cell.tabular_nums();
                }

                match col {
                    DecisionTableColumn::Match => {
                        cell = cell.child(format!("{} – {}", hit.players[0], hit.players[1]));
                    }
                    DecisionTableColumn::Move => {
                        cell = cell.child(format!(
                            "{}.{}",
                            hit.reference.game_no, hit.reference.decision_no
                        ));
                    }
                    DecisionTableColumn::Kind => {
                        let lbl = match hit.mistake_kind {
                            Some(mk) => mistake_kind_label(locale, mk),
                            None => decision_kind_label(locale, &hit.kind),
                        };
                        cell = cell.child(lbl);
                    }
                    DecisionTableColumn::Score => {
                        let base = match hit.away {
                            Some([own, opp]) => I18n::format(
                                locale,
                                "decision-search",
                                "scoreAway",
                                &[
                                    ("own", Param::from(i64::from(own))),
                                    ("opponent", Param::from(i64::from(opp))),
                                ],
                            ),
                            None => "—".into(),
                        };
                        cell = cell.child(if hit.crawford {
                            format!("{base} · Crawford")
                        } else {
                            base
                        });
                    }
                    DecisionTableColumn::Cube => {
                        let owner = hit
                            .cube_owner
                            .and_then(|o| hit.players.get(o as usize))
                            .map(|name| format!(" · {name}"))
                            .unwrap_or_default();
                        cell = cell.child(format!(
                            "{}{owner}",
                            format_int(locale, i64::from(hit.cube_value))
                        ));
                    }
                    DecisionTableColumn::Played => {
                        cell = cell.child(localize_decision(locale, &hit.played));
                    }
                    DecisionTableColumn::Best => {
                        cell = cell.child(localize_decision(locale, &hit.best));
                    }
                    DecisionTableColumn::Loss => {
                        let sev = SeverityStyle::resolve(
                            classify_equity_loss(hit.equity_loss, self.error_thresholds),
                            &p,
                        );
                        cell = cell.child(
                            div()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(px(4.0))
                                .bg(sev.bg)
                                .text_color(sev.fg)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format_decimal(locale, hit.equity_loss, 3, 3)),
                        );
                    }
                    DecisionTableColumn::Source => {
                        let txt = match &hit.depth {
                            Some(d) if !d.is_empty() => format!("{} · {d}", hit.engine),
                            _ => hit.engine.clone(),
                        };
                        cell = cell.text_color(p.muted).child(txt);
                    }
                }
                row = row.child(cell);
            }
            rows_col = rows_col.child(row);
        }

        let last_page = self.total.div_ceil(DECISION_PAGE_SIZE).max(1);
        let ent_pg = ent.clone();

        let mut container = div()
            .v_flex()
            .gap(px(8.0))
            .flex_1()
            .min_w(px(0.0))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_table_key_down))
            .on_mouse_move(cx.listener(Self::handle_resize_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::handle_resize_up))
            .child(toolbar)
            .when_some(col_picker, |this, picker| this.child(picker));

        if let Some(msg) = self.deck_outcome_banner.clone() {
            let ent_d = ent.clone();
            container = container.child(
                StatusBanner::new("ds-deck-outcome-banner", BannerKind::Success, msg)
                    .palette(p)
                    .on_dismiss(move |_, _, cx| {
                        ent_d.update(cx, |v, cx| {
                            v.deck_outcome_banner = None;
                            cx.notify();
                        });
                    }),
            );
        }

        if let Some(err) = self.error.clone() {
            let ent_r = ent.clone();
            container = container.child(
                StatusBanner::new("ds-load-error", BannerKind::Error, err)
                    .palette(p)
                    .on_retry(
                        I18n::t(locale, "recurring-mistakes", "retry"),
                        move |_, _, cx| {
                            ent_r.update(cx, |v, cx| {
                                if v.initialized {
                                    v.execute_search(cx);
                                } else {
                                    v.bootstrap_and_autosearch(cx);
                                }
                            });
                        },
                    ),
            );
        }

        if self.error.is_some() {
            return container;
        }
        if self.loading || self.metadata_task.is_some() {
            container = container.child(div().text_meta().text_color(p.muted).child(ds("loading")));
            if self.items.is_empty() {
                return container;
            }
        }
        if self.items.is_empty() {
            let desc = if self.applied_query.is_none() {
                if self.draft.player.is_empty() {
                    ds("choosePlayer").to_string()
                } else {
                    I18n::format(
                        locale,
                        "decision-search",
                        "idle",
                        &[
                            ("action", Param::from(ds("search"))),
                            ("player", Param::from(self.draft.player.as_str())),
                        ],
                    )
                }
            } else {
                ds("empty").to_string()
            };
            container = container.child(
                EmptyState::new("decision-search-empty", ds("title"), desc)
                    .icon(ActionIconName::Search)
                    .palette(p),
            );
        } else {
            container = container
                .child(
                    div()
                        .id("decision-search-table-scroll")
                        .v_flex()
                        .w_full()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(p.border)
                        .bg(p.surface)
                        .overflow_x_scroll()
                        .child(header_row)
                        .child(rows_col),
                )
                .child(
                    div().h_flex().justify_end().child(
                        NumberedSearchPagination::new(
                            "decision-search-pagination",
                            self.page,
                            last_page,
                        )
                        .disabled(self.loading)
                        .palette(p)
                        .on_select(move |target, _, cx| {
                            ent_pg.update(cx, |v, cx| v.go_to_page(target, cx));
                        }),
                    ),
                );
        }

        container
    }

    fn render_preview_pane(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ds = |k: &'static str| I18n::t(locale, "decision-search", k);
        let ent = cx.entity();

        let Some(hit) = self.active_preview() else {
            return div()
                .id("decision-preview-empty")
                .w(px(430.0))
                .flex_shrink_0()
                .p(px(14.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .child(div().text_body().text_color(p.muted).child(ds("preview")))
                .into_any_element();
        };

        let hit_open = hit.clone();
        let hit_ref = hit.reference.clone();
        let ent_open = ent.clone();
        let ent_deck = ent.clone();
        let ent_bm = ent.clone();

        let offered_by = (hit.kind == "cube_response")
            .then(|| 1usize.saturating_sub(usize::try_from(hit.position.player).unwrap_or(0)));
        let cube_line = format_cube_context_line(
            locale,
            &hit.position.gnubg_match_id,
            [hit.players[0].as_str(), hit.players[1].as_str()],
            offered_by,
            true,
        );
        let sev = SeverityStyle::resolve(
            classify_equity_loss(hit.equity_loss, self.error_thresholds),
            &p,
        );

        // Synchronous in-process MoveExplanation for checker mistakes (reusing crate::review)
        let explanation_view = if hit.kind == "checker" && hit.best != hit.played {
            let bp = position_to_board_position(&hit.position, true);
            if bp.dice.len() >= 2 {
                explain_checker_plays(
                    bp.board,
                    i32::from(bp.player),
                    [i32::from(bp.dice[0]), i32::from(bp.dice[1])],
                    &hit.best,
                    &hit.played,
                )
                .ok()
                .map(|expl| {
                    MoveExplanationView::from_explanation(
                        "ds-preview-explanation",
                        &hit.best,
                        &hit.played,
                        &expl,
                        None,
                        None,
                    )
                    .palette(p)
                })
            } else {
                None
            }
        } else {
            None
        };

        let kind_label = match hit.mistake_kind {
            Some(mk) => mistake_kind_label(locale, mk),
            None => decision_kind_label(locale, &hit.kind),
        };

        div()
            .id("decision-search-preview-pane")
            .test_support()
            .v_flex()
            .gap(px(10.0))
            .w(px(430.0))
            .flex_shrink_0()
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .v_flex()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_panel()
                            .text_color(p.text)
                            .child(format!("{} – {}", hit.players[0], hit.players[1])),
                    )
                    .child(div().text_meta().text_color(p.muted).child(format!(
                        "{} {}.{} · {kind_label}",
                        ds("move"),
                        hit.reference.game_no,
                        hit.reference.decision_no
                    ))),
            )
            .when(hit.is_openable(), |this| {
                this.child(
                    div()
                        .h_flex()
                        .flex_wrap()
                        .gap(px(6.0))
                        .when(hit.has_position(), |row| {
                            let saved = self.preview_bookmarked.unwrap_or(false);
                            row.child(
                                ActionButton::new("ds-preview-bookmark")
                                    .icon(if saved {
                                        ActionIconName::Unbookmark
                                    } else {
                                        ActionIconName::Bookmark
                                    })
                                    .label(I18n::t(
                                        locale,
                                        "position-bookmark",
                                        match self.preview_bookmarked {
                                            Some(true) => "remove",
                                            Some(false) => "save",
                                            None if self.bookmark_read.is_some() => "loading",
                                            None => "retry",
                                        },
                                    ))
                                    .active(saved)
                                    .when(
                                        self.preview_bookmarked.is_none()
                                            && self.bookmark_read.is_none(),
                                        |button| {
                                            button.tooltip(I18n::t(
                                                locale,
                                                "position-bookmark",
                                                "loadError",
                                            ))
                                        },
                                    )
                                    .disabled(self.bookmark_busy || self.bookmark_read.is_some())
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        ent_bm.update(cx, |v, cx| v.toggle_preview_bookmark(cx));
                                    }),
                            )
                        })
                        .child(
                            ActionButton::new("ds-preview-add-deck")
                                .icon(ActionIconName::Deck)
                                .label(I18n::t(locale, "position-browser", "addToDeck"))
                                .palette(p)
                                .on_click(move |_, window, cx| {
                                    ent_deck.update(cx, |v, cx| {
                                        v.open_add_to_deck_dialog(
                                            vec![hit_ref.clone()],
                                            window,
                                            cx,
                                        );
                                    });
                                }),
                        )
                        .child(
                            ActionButton::new("ds-preview-open-analysis")
                                .icon(ActionIconName::Analysis)
                                .label(ds("openAnalysis"))
                                .primary()
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    ent_open
                                        .update(cx, |v, cx| v.open_hit_in_analysis(&hit_open, cx));
                                }),
                        ),
                )
            })
            .child(div().text_meta().text_color(p.muted).child(cube_line))
            .child(
                div()
                    .w_full()
                    .h(px(270.0))
                    .child(self.preview_board.clone()),
            )
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .text_body()
                    .text_color(p.text)
                    .child(format!(
                        "{}: {} · {}: {}",
                        ds("played"),
                        localize_decision(locale, &hit.played),
                        ds("best"),
                        localize_decision(locale, &hit.best)
                    ))
                    .child(
                        div()
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .bg(sev.bg)
                            .text_label()
                            .tabular_nums()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(sev.fg)
                            .child(format!(
                                "{}: {}",
                                ds("loss"),
                                format_decimal(locale, hit.equity_loss, 3, 3)
                            )),
                    ),
            )
            .when_some(explanation_view, |this, expl| this.child(expl))
            .into_any_element()
    }
}

impl Focusable for DecisionSearchView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for DecisionSearchView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.draft.min_loss.is_none() {
            let inherited = self.error_thresholds.error.to_string();
            if self.min_loss_input.read(cx).canonical_value() != inherited {
                self.min_loss_input.update(cx, |input, cx| {
                    input.set_canonical_value(&inherited, window, cx)
                });
            }
        }
        let locale = current_locale(cx);
        let p = NeonTheme::current_palette(cx);

        div()
            .id("decision-search-workspace")
            .test_support()
            .v_flex()
            .gap(px(12.0))
            .w_full()
            .flex_1()
            .child(self.render_filter_bar(p, locale, cx))
            .child(
                div()
                    .h_flex()
                    .items_start()
                    .gap(px(14.0))
                    .w_full()
                    .flex_1()
                    .child(self.render_table(p, locale, cx))
                    .child(self.render_preview_pane(p, locale, cx)),
            )
    }
}

// ── Add Decisions to Deck Dialog ─────────────────────────────────────────────

struct DecisionDeckDialogView {
    facade: Option<BackendFacade>,
    references: Vec<MistakeReference>,
    busy: ModalBusyLock,
    parent: Entity<DecisionSearchView>,
    decks: Vec<Deck>,
    selected_deck_id: Option<Uuid>,
    new_deck_input: Entity<InputState>,
    submitting: bool,
    creating: bool,
    error: Option<SharedString>,
}

impl DecisionDeckDialogView {
    fn new(
        facade: Option<BackendFacade>,
        references: Vec<MistakeReference>,
        busy: ModalBusyLock,
        parent: Entity<DecisionSearchView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let locale = current_locale(cx);
        let new_deck_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(locale, "position-browser", "deckName"))
        });
        let mut view = Self {
            facade,
            references,
            busy,
            parent,
            decks: Vec::new(),
            selected_deck_id: None,
            new_deck_input,
            submitting: false,
            creating: false,
            error: None,
        };
        view.load_decks(cx);
        view
    }

    fn load_decks(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let Some(task) = spawn_facade_read(cx, move || async move { facade.list_decks().await })
        else {
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let Ok(Ok(decks)) = task.await else {
                return;
            };
            let _ = weak.update(cx, |v, cx| {
                if v.selected_deck_id.is_none() {
                    v.selected_deck_id = decks.first().map(|d| d.id);
                }
                v.decks = decks;
                cx.notify();
            });
        })
        .detach();
    }

    fn create_deck(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.new_deck_input.read(cx).value().trim().to_string();
        if name.is_empty() || self.creating || self.submitting {
            return;
        }
        let Some(facade) = self.facade.clone() else {
            return;
        };
        self.creating = true;
        self.busy.set_busy(true);
        self.error = None;
        let Some(task) =
            spawn_facade_write(cx, move || async move { facade.create_deck(name).await })
        else {
            self.creating = false;
            self.busy.set_busy(false);
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |v, window, cx| {
                v.creating = false;
                v.busy.set_busy(false);
                match res {
                    Ok(deck) => {
                        v.selected_deck_id = Some(deck.id);
                        v.decks.push(deck);
                        v.new_deck_input
                            .update(cx, |s, cx| s.set_value(String::new(), window, cx));
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

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(deck_id) = self.selected_deck_id else {
            return;
        };
        if self.submitting || self.creating {
            return;
        }
        let Some(facade) = self.facade.clone() else {
            window.close_dialog(cx);
            return;
        };
        self.submitting = true;
        self.busy.set_busy(true);
        self.error = None;
        let refs = self.references.clone();
        let Some(task) = spawn_facade_write(cx, move || async move {
            facade.add_decisions_to_deck(deck_id, refs).await
        }) else {
            self.submitting = false;
            self.busy.set_busy(false);
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |v, window, cx| {
                v.submitting = false;
                v.busy.set_busy(false);
                let locale = current_locale(cx);
                match res {
                    Ok(outcome) => {
                        let summary = format_decision_deck_outcome(locale, &outcome);
                        v.parent.update(cx, |parent, cx| {
                            parent.deck_outcome_banner = Some(summary.into());
                            cx.notify();
                        });
                        window.close_dialog(cx);
                    }
                    Err(err) => {
                        v.error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for DecisionDeckDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = NeonTheme::current_palette(cx);
        let pb = |k: &'static str| I18n::t(locale, "position-browser", k);
        let ds = |k: &'static str| I18n::t(locale, "decision-search", k);
        let disabled = self.submitting || self.creating;
        let ent = cx.entity();

        let mut decks_row = div().h_flex().flex_wrap().gap(px(6.0));
        for deck in &self.decks {
            let id = deck.id;
            let ent_d = ent.clone();
            decks_row = decks_row.child(
                ActionButton::new(SharedString::from(format!("ds-deck-pick-{id}")))
                    .label(format!("{} ({})", deck.name, deck.card_count))
                    .active(self.selected_deck_id == Some(id))
                    .disabled(disabled)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_d.update(cx, |v, cx| {
                            v.selected_deck_id = Some(id);
                            cx.notify();
                        });
                    }),
            );
        }

        let add_lbl = if self.submitting {
            ds("addingDecisions").to_string()
        } else {
            I18n::format(
                locale,
                "decision-search",
                "addDecisions",
                &[("count", Param::from(self.references.len() as i64))],
            )
        };

        div()
            .v_flex()
            .gap(px(12.0))
            .child(
                div()
                    .v_flex()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_label()
                            .text_color(p.text)
                            .child(pb("chooseDeck")),
                    )
                    .child(decks_row),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(&self.new_deck_input).id("ds-new-deck-in")),
                    )
                    .child({
                        let ent_c = ent.clone();
                        ActionButton::new("ds-create-deck-btn")
                            .icon(ActionIconName::Add)
                            .label(pb("createDeck"))
                            .disabled(
                                disabled || self.new_deck_input.read(cx).value().trim().is_empty(),
                            )
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent_c.update(cx, |v, cx| v.create_deck(window, cx));
                            })
                    }),
            )
            .when_some(self.error.clone(), |this, err| {
                this.child(StatusBanner::new("ds-deck-err", BannerKind::Error, err).palette(p))
            })
            .child(div().h_flex().justify_end().child({
                let ent_s = ent.clone();
                ActionButton::new("ds-deck-submit")
                    .icon(ActionIconName::Add)
                    .label(add_lbl)
                    .primary()
                    .disabled(disabled || self.selected_deck_id.is_none())
                    .palette(p)
                    .on_click(move |_, window, cx| {
                        ent_s.update(cx, |v, cx| v.submit(window, cx));
                    })
            }))
    }
}

fn format_decision_deck_outcome(locale: Locale, outcome: &AddDecisionsResult) -> String {
    let duplicate = outcome
        .items
        .iter()
        .filter(|i| i.status == DecisionAddStatus::Duplicate)
        .count() as i64;
    let unavailable = outcome
        .items
        .iter()
        .filter(|i| {
            matches!(
                i.status,
                DecisionAddStatus::Unavailable | DecisionAddStatus::NotFound
            )
        })
        .count() as i64;
    I18n::format(
        locale,
        "decision-search",
        "deckOutcome",
        &[
            ("added", Param::from(outcome.added)),
            ("updated", Param::from(outcome.updated)),
            ("duplicate", Param::from(duplicate)),
            ("unavailable", Param::from(unavailable)),
        ],
    )
}

// ── Launch Practice Modal (Up to 20 Errors from Current Page) ────────────────

enum DecisionPracticeOutcome {
    Attempt(Box<MistakeAttempt>),
    Revealed(MistakeAnswer),
}

struct DecisionPracticeDialogView {
    facade: Option<BackendFacade>,
    questions: Vec<DecisionHitRow>,
    index: usize,
    board: Entity<BoardView>,
    checker_draft: Option<CheckerPracticeDraft>,
    outcome: Option<DecisionPracticeOutcome>,
    busy: ModalBusyLock,
    submitting: bool,
    error: Option<SharedString>,
    _board_sub: Subscription,
}

impl DecisionPracticeDialogView {
    fn new(
        facade: Option<BackendFacade>,
        questions: Vec<DecisionHitRow>,
        busy: ModalBusyLock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let board = cx.new(|cx| themed_board("decision-practice-board", cx));

        let board_sub = cx.subscribe_in(&board, window, |this, _, event: &BoardEvent, _, cx| {
            if this.outcome.is_some() || this.submitting {
                return;
            }
            let Some(draft) = &mut this.checker_draft else {
                return;
            };
            match *event {
                BoardEvent::PointClicked(pt) => {
                    if draft.click_point(pt, this.submitting) {
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
                    if draft.swap_dice(this.submitting) {
                        this.sync_board(cx);
                    }
                }
                BoardEvent::CubeClicked => {}
            }
        });

        let mut view = Self {
            facade,
            questions,
            index: 0,
            board,
            checker_draft: None,
            outcome: None,
            busy,
            submitting: false,
            error: None,
            _board_sub: board_sub,
        };
        view.prepare_current_question(cx);
        view
    }

    fn current(&self) -> Option<&DecisionHitRow> {
        self.questions.get(self.index)
    }

    fn prepare_current_question(&mut self, cx: &mut Context<Self>) {
        self.outcome = None;
        self.error = None;
        if let Some(q) = self.current().cloned() {
            if q.kind == "checker" {
                self.checker_draft = Some(CheckerPracticeDraft::from_position(&q.position));
            } else {
                self.checker_draft = None;
            }
            self.sync_board(cx);
        }
    }

    fn sync_board(&mut self, cx: &mut Context<Self>) {
        let Some(q) = self.current().cloned() else {
            return;
        };
        let (bp, interaction, preview) = if let Some(draft) = &self.checker_draft {
            let preview = self.outcome.as_ref().map(|o| {
                let best = match o {
                    DecisionPracticeOutcome::Attempt(a) => &a.evaluation.best,
                    DecisionPracticeOutcome::Revealed(r) => &r.best,
                };
                draft.project_notation_preview(best)
            });
            let inter = if self.outcome.is_none() {
                Some(draft.interaction_state(self.submitting))
            } else {
                None
            };
            (draft.board_position(), inter, preview)
        } else {
            (position_to_board_position(&q.position, true), None, None)
        };

        let pending_cube = q.kind == "cube_response";
        let white = q.players[0].clone();
        let black = q.players[1].clone();
        self.board.update(cx, |bv, cx| {
            bv.set_position(bp, cx);
            bv.set_players(white, black, cx);
            bv.set_cube_offer_pending(pending_cube, cx);
            bv.set_interaction(interaction, cx);
            let mode = if preview.is_some() {
                BoardViewMode::Arrows
            } else {
                BoardViewMode::Before
            };
            bv.set_preview(preview, mode, cx);
        });
        cx.notify();
    }

    fn submit_answer(&mut self, answer: PracticeAnswer, cx: &mut Context<Self>) {
        let Some(q) = self.current().cloned() else {
            return;
        };
        let Some(facade) = self.facade.clone() else {
            return;
        };
        if self.submitting {
            return;
        }
        self.submitting = true;
        self.busy.set_busy(true);
        self.error = None;
        let req = MistakeAttemptRequest {
            reference: q.reference,
            answer,
        };
        let Some(task) = spawn_facade_write(cx, move || async move {
            facade.attempt_recurring_mistake(req).await
        }) else {
            self.submitting = false;
            self.busy.set_busy(false);
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = weak.update(cx, |v, cx| {
                v.submitting = false;
                v.busy.set_busy(false);
                match res {
                    Ok(attempt) => {
                        v.outcome = Some(DecisionPracticeOutcome::Attempt(Box::new(attempt)));
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

    fn reveal(&mut self, cx: &mut Context<Self>) {
        let Some(q) = self.current().cloned() else {
            return;
        };
        let Some(facade) = self.facade.clone() else {
            return;
        };
        if self.submitting {
            return;
        }
        self.submitting = true;
        self.busy.set_busy(true);
        self.error = None;
        let reference = q.reference;
        let Some(task) = spawn_facade_read(cx, move || async move {
            facade.reveal_recurring_mistake(&reference).await
        }) else {
            self.submitting = false;
            self.busy.set_busy(false);
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = weak.update(cx, |v, cx| {
                v.submitting = false;
                v.busy.set_busy(false);
                match res {
                    Ok(ans) => {
                        v.outcome = Some(DecisionPracticeOutcome::Revealed(ans));
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

    fn next_question(&mut self, cx: &mut Context<Self>) {
        self.index += 1;
        self.prepare_current_question(cx);
    }
}

impl Render for DecisionPracticeDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = NeonTheme::current_palette(cx);
        let rm = |k: &'static str| I18n::t(locale, "recurring-mistakes", k);
        let ent = cx.entity();

        let Some(q) = self.current().cloned() else {
            return div()
                .v_flex()
                .gap(px(12.0))
                .child(
                    div()
                        .text_subject()
                        .text_color(p.text)
                        .child(rm("complete")),
                )
                .child(
                    div().h_flex().justify_end().child(
                        ActionButton::new("ds-practice-close-done")
                            .icon(ActionIconName::Close)
                            .label(rm("close"))
                            .primary()
                            .palette(p)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
                )
                .into_any_element();
        };

        let progress = format!("{} / {}", self.index + 1, self.questions.len());
        let loc = I18n::format(
            locale,
            "recurring-mistakes",
            "location",
            &[
                ("game", Param::from(i64::from(q.reference.game_no))),
                ("decision", Param::from(i64::from(q.reference.decision_no))),
            ],
        );

        let mut controls = div().h_flex().flex_wrap().gap(px(8.0));
        if self.outcome.is_none() {
            if q.kind == "checker" {
                let can_undo = self
                    .checker_draft
                    .as_ref()
                    .is_some_and(CheckerPracticeDraft::can_undo);
                let can_submit = self
                    .checker_draft
                    .as_ref()
                    .is_some_and(CheckerPracticeDraft::can_submit);
                let steps = self
                    .checker_draft
                    .as_ref()
                    .map(|d| d.steps.clone())
                    .unwrap_or_default();
                let ent_u = ent.clone();
                let ent_c = ent.clone();
                controls = controls
                    .child(
                        ActionButton::new("ds-prac-undo")
                            .icon(ActionIconName::Undo)
                            .label(I18n::t(locale, "play", "undo"))
                            .shortcut("Z")
                            .disabled(self.submitting || !can_undo)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_u.update(cx, |v, cx| {
                                    if let Some(d) = &mut v.checker_draft
                                        && d.undo_step()
                                    {
                                        v.sync_board(cx);
                                    }
                                });
                            }),
                    )
                    .child(
                        ActionButton::new("ds-prac-check")
                            .icon(ActionIconName::Confirm)
                            .label(I18n::t(locale, "position-browser", "practiceCheckMove"))
                            .shortcut("Enter")
                            .primary()
                            .disabled(self.submitting || !can_submit)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_c.update(cx, |v, cx| {
                                    v.submit_answer(
                                        PracticeAnswer::Checker {
                                            steps: steps.clone(),
                                        },
                                        cx,
                                    );
                                });
                            }),
                    );
            } else if q.kind == "cube_offer" {
                let ent_nd = ent.clone();
                let ent_d = ent.clone();
                controls = controls
                    .child(
                        ActionButton::new("ds-prac-no-double")
                            .label(localize_decision(locale, "no double"))
                            .shortcut("N")
                            .disabled(self.submitting)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_nd.update(cx, |v, cx| {
                                    v.submit_answer(
                                        PracticeAnswer::CubeOffer {
                                            action: CubeOfferAction::NoDouble,
                                        },
                                        cx,
                                    );
                                });
                            }),
                    )
                    .child(
                        ActionButton::new("ds-prac-double")
                            .icon(ActionIconName::Double)
                            .label(localize_decision(locale, "double"))
                            .shortcut("D")
                            .primary()
                            .disabled(self.submitting)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_d.update(cx, |v, cx| {
                                    v.submit_answer(
                                        PracticeAnswer::CubeOffer {
                                            action: CubeOfferAction::Double,
                                        },
                                        cx,
                                    );
                                });
                            }),
                    );
            } else {
                let ent_t = ent.clone();
                let ent_p = ent.clone();
                controls = controls
                    .child(
                        ActionButton::new("ds-prac-take")
                            .icon(ActionIconName::Take)
                            .label(localize_decision(locale, "take"))
                            .shortcut("T")
                            .primary()
                            .disabled(self.submitting)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_t.update(cx, |v, cx| {
                                    v.submit_answer(
                                        PracticeAnswer::CubeResponse {
                                            action: CubeResponseAction::Take,
                                        },
                                        cx,
                                    );
                                });
                            }),
                    )
                    .child(
                        ActionButton::new("ds-prac-drop")
                            .icon(ActionIconName::Drop)
                            .label(localize_decision(locale, "drop"))
                            .shortcut("P")
                            .disabled(self.submitting)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_p.update(cx, |v, cx| {
                                    v.submit_answer(
                                        PracticeAnswer::CubeResponse {
                                            action: CubeResponseAction::Drop,
                                        },
                                        cx,
                                    );
                                });
                            }),
                    );
            }

            let ent_rev = ent.clone();
            controls = controls.child(
                ActionButton::new("ds-prac-reveal")
                    .icon(ActionIconName::View)
                    .label(rm("reveal"))
                    .disabled(self.submitting)
                    .palette(p)
                    .on_click(move |_, _, cx| ent_rev.update(cx, |v, cx| v.reveal(cx))),
            );
        } else {
            let ent_nxt = ent.clone();
            let last = self.index + 1 >= self.questions.len();
            controls = controls.child(
                ActionButton::new("ds-prac-next")
                    .icon(ActionIconName::Next)
                    .label(rm(if last { "finish" } else { "next" }))
                    .primary()
                    .palette(p)
                    .on_click(move |_, _, cx| ent_nxt.update(cx, |v, cx| v.next_question(cx))),
            );
        }

        let feedback = self.outcome.as_ref().map(|outcome| match outcome {
            DecisionPracticeOutcome::Attempt(attempt) => {
                render_attempt_feedback(locale, p, &q, &attempt.evaluation)
            }
            DecisionPracticeOutcome::Revealed(ans) => div()
                .v_flex()
                .gap(px(4.0))
                .p(px(10.0))
                .rounded(px(8.0))
                .bg(p.surface_alt)
                .border_1()
                .border_color(p.border)
                .child(div().text_body().text_color(p.text).child(format!(
                    "{}: {} · {}: {} · {}: {}",
                    rm("best"),
                    localize_decision(locale, &ans.best),
                    rm("played"),
                    localize_decision(locale, &ans.played),
                    rm("loss"),
                    format_decimal(locale, ans.equity_loss, 3, 3)
                )))
                .into_any_element(),
        });

        div()
            .v_flex()
            .gap(px(12.0))
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
                                "{progress} · {} – {} · {loc}",
                                q.players[0], q.players[1]
                            )),
                    )
                    .child(
                        div()
                            .text_meta()
                            .text_color(p.muted)
                            .child(rm("practiceHelp")),
                    ),
            )
            .child(div().w_full().h(px(360.0)).child(self.board.clone()))
            .when_some(self.error.clone(), |this, err| {
                this.child(StatusBanner::new("ds-prac-err", BannerKind::Error, err).palette(p))
            })
            .when_some(feedback, |this, fb| this.child(fb))
            .child(controls)
            .into_any_element()
    }
}

fn render_attempt_feedback(
    locale: Locale,
    p: NeonPalette,
    q: &DecisionHitRow,
    eval: &PracticeEvaluation,
) -> gpui_kit::AnyElement {
    let rm = |k: &'static str| I18n::t(locale, "recurring-mistakes", k);
    let (verdict_bg, verdict_fg) = match eval.verdict {
        PracticeVerdict::Best | PracticeVerdict::Forced => (p.success_bg, p.success_text),
        PracticeVerdict::Playable => (p.warning_bg, p.warning_text),
        PracticeVerdict::Error => (p.danger_bg, p.danger_text),
    };

    let mut box_el = div()
        .v_flex()
        .gap(px(8.0))
        .p(px(10.0))
        .rounded(px(8.0))
        .bg(p.surface_alt)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .h_flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .px(px(7.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .bg(verdict_bg)
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(verdict_fg)
                        .child(format!("{:?}", eval.verdict)),
                )
                .child(
                    div()
                        .text_body()
                        .tabular_nums()
                        .text_color(p.text)
                        .child(format!(
                            "{}: {} ({}) · {}: {} ({}) · {}: {}",
                            I18n::t(locale, "position-browser", "practiceYourMove"),
                            localize_decision(locale, &eval.chosen),
                            eval.chosen_equity
                                .map(|e| format_equity(locale, e))
                                .unwrap_or_else(|| "—".into()),
                            rm("best"),
                            localize_decision(locale, &eval.best),
                            eval.best_equity
                                .map(|e| format_equity(locale, e))
                                .unwrap_or_else(|| "—".into()),
                            rm("loss"),
                            format_decimal(locale, eval.equity_loss, 3, 3)
                        )),
                ),
        );

    if let Some(cube_eval) = &eval.cube_evaluation {
        let unit = match cube_eval.unit {
            BackendCubeEquityUnit::Mwc => CubeEquityUnit::MatchWinningChance,
            BackendCubeEquityUnit::Cubeful => CubeEquityUnit::NormalizedCubeful,
        };
        let details = CubeDecisionDetailsView::new(
            "ds-prac-cube-details",
            usize::try_from(q.position.player).unwrap_or(0),
            q.kind == "cube_response",
            [
                SharedString::from(q.players[0].clone()),
                SharedString::from(q.players[1].clone()),
            ],
        )
        .match_id(q.position.gnubg_match_id.clone())
        .evaluation(Some(CubeEvaluationData {
            no_double: cube_eval.no_double,
            double_take: cube_eval.double_take,
            double_pass: cube_eval.double_pass,
            unit,
            probabilities: cube_eval.probabilities,
        }))
        .palette(p);
        box_el = box_el.child(details);
    }

    box_el.into_any_element()
}

fn decision_kind_label(locale: Locale, kind: &str) -> &'static str {
    let key = match kind {
        "cube_offer" => "cube_offer",
        "cube_response" => "cube_response",
        _ => "checker",
    };
    I18n::t(locale, "recurring-mistakes", key)
}
