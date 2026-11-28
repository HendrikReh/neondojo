//! Complete native GPUI Analysis workspace (`AnalysisWorkspace`).
//!
//! Owns the 3-pane resizable desktop layout (`MoveHistoryView` +
//! `AnalysisBoardPane` + 5-tab Inspector), persistent `MatchAnalysisJobEntity`
//! surviving tab/workspace switches, 5 independent per-tab board widths stored
//! in `LocalUiState::analysis_board_widths`, keyboard-adjustable splitter,
//! `DiceTabState` state machine, persistent `NotesDraftCache`, and epoch-guarded
//! async `BackendFacade` loading.

pub mod board_pane;
mod candidate_data;
pub mod candidates;
pub mod cube_stats;
pub mod header;
pub mod jobs;
pub mod move_history;
pub mod notes_cards;
pub mod state;

#[cfg(test)]
mod tests;

use self::board_pane::{
    BoardPaneCallbacks, BoardPaneProps, BoardPreviewState, HypotheticalDicePreview,
    compute_game_plans, render_board_pane, sync_board_view,
};
use self::candidate_data::{CheckerCandidate, comparison_options, extract_candidates};
use self::candidates::{CheckerCallbacks, CheckerViewProps, render_checker_inspector};
use self::cube_stats::{
    CubeCallbacks, CubeViewProps, StatsCallbacks, StatsDrilldownTarget, StatsViewProps,
    render_cube_inspector, render_statistics_inspector,
};
use self::header::{HeaderCallbacks, HeaderProps, render_analysis_header};
use self::jobs::{AnalysisJobEvent, MatchAnalysisJobEntity};
use self::move_history::{MoveHistoryCallbacks, render_move_history_pane};
use self::notes_cards::{
    CardsCallbacks, CardsViewProps, NoteEditorBinding, NotesCallbacks, NotesViewProps,
    open_move_notes_dialog, render_cards_inspector, render_notes_inspector,
};
use self::state::{
    ActiveBoardTarget, AnalysisDetailTab, BoardWidthController, CheckerSubTab, CompareEngineSlot,
    CubeAutoSelectState, CubeFilterState, DiceTabState, DiceTransitionAction, InterleavedMoveRow,
    MOVE_LIST_PANE_WIDTH, MoveAssessmentSnapshot, MoveHistoryFilter, NoteDraft, NoteTargetKey,
    NotesDraftCache, build_interleaved_move_rows, catalog_entry_engine, run_facade,
};
use crate::app_context::DesktopAppContext;
use crate::events::WorkspaceEvent;
use crate::keybindings::{
    AnalyzeGame, AnalyzeMatch, CTX_ANALYSIS, CTX_BOARD_REVIEW, CTX_SPLITTER, CopyPositionId,
    NextError, NextPosition, PrevError, PrevPosition, ResetPaneWidths, ShowFinalBoard,
    ShowInitialBoard, SplitterGrow, SplitterGrowFast, SplitterMax, SplitterMin, SplitterReset,
    SplitterShrink, SplitterShrinkFast, ToggleDiceEquity,
};
use crate::local_state::{LocalUiState, WorkspaceSelection};
use crate::review::preview_controls::step_preview_mode;
use crate::shell::AppNotice;
use crate::theme::NeonTheme;
use crate::tokio_bridge::{ReadTask, TokioBridge};
use crate::ui::banners::BannerKind;
use crate::ui::drop_zone::{DroppedMatchBatch, MatchDropZone};
use gpui_kit::{
    App, AppContext, BorrowAppContext, ClipboardItem, Context, CursorStyle, Entity, EventEmitter,
    FocusHandle, InteractiveElement, IntoElement, KeyContext, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, TestSupportExt, UniformListScrollHandle,
    Window,
    base::StyledExt,
    component::input::{InputEvent, InputState, TextareaState},
    div, px,
};
use neondojo::{
    analysis_catalog::{
        AnalysisCatalogEntry, AnalysisCatalogPage, AnalysisCatalogQuery, SelectedPositionAnalysis,
    },
    analysis_statistics::catalog::{CatalogStatisticsQuery, MatchAnalysisStatistics},
    annotations::{Annotation, AnnotationKind},
    api::{MatchAnalysisEngine, PositionJobKind, PositionQuery},
    databases::DatabaseMatch,
    domain_error::ErrorKind,
    learning::{
        AddCards, CardSuggestions, Deck, SaveSuggestions, SuggestionQuery, SuggestionReference,
    },
    model::{DiceStats, ImportWarning, MatchGame, MatchSummary, MoveSeverity, Position},
    move_assessments::MoveAssessmentQuery,
    move_catalog::StoredMove,
    opening_book::BookLookup,
    review_decisions::{ReviewDecision, ReviewQuery},
};
use neondojo_board::{BoardPosition, BoardView, BoardViewMode};
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use uuid::Uuid;

/// Top-level native GPUI Analysis workspace entity.
pub struct AnalysisWorkspace {
    visible: bool,
    welcome_mode: bool,
    focus_handle: FocusHandle,
    splitter_focus: FocusHandle,
    move_scroll: UniformListScrollHandle,

    // Persistent job controller (NEVER dropped when switching tabs/workspaces)
    pub(crate) job_controller: Entity<MatchAnalysisJobEntity>,
    _job_subscription: Subscription,

    // Board & input child entities
    board_view: Entity<BoardView>,
    tag_input: Entity<InputState>,
    comment_input: Entity<TextareaState>,
    comment_binding: NoteEditorBinding,
    new_deck_input: Entity<InputState>,
    _comment_input_sub: Subscription,

    // Active match & navigation state
    pub(crate) match_id: Option<Uuid>,
    pub(crate) match_summary: Option<MatchSummary>,
    pub(crate) match_meta: Option<DatabaseMatch>,
    pub(crate) games: Vec<MatchGame>,
    pub(crate) current_game_no: Option<i32>,
    pub(crate) current_game_id: Option<Uuid>,
    pub(crate) positions: Vec<Position>,
    pub(crate) source_moves: Vec<StoredMove>,
    pub(crate) assessments: HashMap<Uuid, MoveAssessmentSnapshot>,
    pub(crate) import_warnings: Vec<ImportWarning>,

    // Active selection & board preview
    pub(crate) active_target: Option<ActiveBoardTarget>,
    pub(crate) preview: BoardPreviewState,
    pub(crate) bookmarked: bool,
    pub(crate) bookmark_busy: bool,
    pub(crate) copied_feedback: bool,

    // Move history filter & error navigation
    pub(crate) move_filter: MoveHistoryFilter,
    pub(crate) error_positions: Vec<Uuid>,
    pub(crate) loading_game: bool,

    // Layout & 5-tab persisted board widths
    pub(crate) active_tab: AnalysisDetailTab,
    pub(crate) board_widths: BoardWidthController,
    pub(crate) dragging_splitter: bool,
    pub(crate) drag_start_x: f32,
    pub(crate) drag_start_width: f32,
    pub(crate) last_viewport_width: f32,

    // Header popovers & catalog
    pub(crate) game_menu_open: bool,
    pub(crate) catalog_menu_open: bool,
    pub(crate) options_popover_open: bool,
    pub(crate) catalog_items: Vec<AnalysisCatalogEntry>,
    pub(crate) catalog_selected: Option<AnalysisCatalogEntry>,
    pub(crate) catalog_selected_id: Option<String>,
    pub(crate) catalog_source_revision: i64,
    pub(crate) catalog_next_cursor: Option<String>,
    pub(crate) catalog_loading: bool,
    catalog_epoch: u64,

    // Overview / Checker sub-tabs
    pub(crate) checker_subtab: CheckerSubTab,
    pub(crate) position_result: Option<SelectedPositionAnalysis>,
    pub(crate) position_result_loading: bool,
    pub(crate) opening_book: Option<BookLookup>,
    pub(crate) opening_book_expanded: bool,
    pub(crate) opening_book_loading: bool,
    pub(crate) compare_slots: [Vec<CheckerCandidate>; 3],
    pub(crate) compare_slot_ids: [Option<String>; 3],
    pub(crate) compare_sort_delta: Option<CompareEngineSlot>,
    pub(crate) dice_state: DiceTabState,
    pub(crate) selected_dice_cell: Option<[u8; 2]>,
    pub(crate) observed_rolls: Option<DiceStats>,
    pub(crate) observed_rolls_game_only: bool,
    pub(crate) observed_rolls_error: Option<String>,

    // Cube tab
    pub(crate) cube_filter: CubeFilterState,
    pub(crate) cube_decisions: Vec<ReviewDecision>,
    pub(crate) cube_loading: bool,
    pub(crate) cube_auto_select: CubeAutoSelectState,
    pub(crate) calculating_chances_for: Option<String>,
    pub(crate) calculated_chances: HashMap<String, ([f64; 6], String, i32)>,

    // Statistics tab
    pub(crate) statistics: Option<MatchAnalysisStatistics>,
    pub(crate) statistics_loading: bool,
    pub(crate) stats_drilldown: Option<StatsDrilldownTarget>,
    pub(crate) compare_stats_a_id: Option<String>,
    pub(crate) compare_stats_b_id: Option<String>,
    pub(crate) compare_stats_a: Option<MatchAnalysisStatistics>,
    pub(crate) compare_stats_b: Option<MatchAnalysisStatistics>,

    // Notes tab (persistent drafts across navigation + 409 conflict preservation)
    pub(crate) notes_scope: AnnotationKind,
    pub(crate) notes_drafts: Entity<NotesDraftCache>,
    pub(crate) notes_saving: bool,
    pub(crate) notes_saved_feedback: bool,
    pub(crate) notes_validation_error: Option<String>,

    // Cards tab
    pub(crate) decks: Vec<Deck>,
    pub(crate) selected_deck_id: Option<Uuid>,
    pub(crate) cards_busy: bool,
    pub(crate) cards_status: Option<String>,
    pub(crate) card_suggestions: Option<CardSuggestions>,
    pub(crate) card_suggestions_loading: bool,
    pub(crate) selected_suggestions: BTreeSet<SuggestionReference>,
    pub(crate) suggestion_boards: HashMap<SuggestionReference, Entity<BoardView>>,

    // Welcome import busy flag
    pub(crate) welcome_importing: bool,

    // Epoch & abortable read task guards (protect against stale out-of-order responses)
    match_epoch: u64,
    game_epoch: u64,
    position_epoch: u64,
    match_load_task: Option<ReadTask<()>>,
    game_load_task: Option<ReadTask<()>>,
    position_load_task: Option<ReadTask<()>>,
    stats_load_task: Option<ReadTask<()>>,
    cube_load_task: Option<gpui_kit::Task<()>>,
    rolls_load_task: Option<ReadTask<()>>,
}

impl EventEmitter<WorkspaceEvent> for AnalysisWorkspace {}

impl AnalysisWorkspace {
    /// Creates a new `AnalysisWorkspace` with persistent `MatchAnalysisJobEntity`,
    /// `BoardView`, input entities, and persisted per-tab board widths from `LocalUiState`.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let atlas = cx
            .try_global::<DesktopAppContext>()
            .map(|ctx| ctx.board_atlas.clone())
            .unwrap_or_default();
        let persisted_widths = cx
            .try_global::<LocalUiState>()
            .map(|s| s.analysis_board_widths)
            .unwrap_or_default();
        let saved_selection = cx
            .try_global::<LocalUiState>()
            .and_then(|s| s.workspace_selection);

        let initial_board_cfg = cx
            .try_global::<DesktopAppContext>()
            .map(|ctx| ctx.board_config.clone())
            .unwrap_or_else(|| neondojo_board::BoardThemeConfig {
                board_theme: cx
                    .try_global::<LocalUiState>()
                    .map(|s| s.board_theme.clone())
                    .unwrap_or_else(|| "forest".into()),
                ..neondojo_board::BoardThemeConfig::default()
            });
        let board_view = cx.new(|cx| {
            let mut bv = BoardView::new("analysis-main-board", atlas);
            bv.set_theme_config(initial_board_cfg, cx);
            bv
        });
        let tag_input = cx.new(|cx| InputState::new(window, cx).placeholder("Add tag…"));
        let comment_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(4)
                .placeholder(neondojo_i18n::I18n::t(
                    crate::i18n::current_locale(cx),
                    "annotation",
                    "commentPlaceholder",
                ))
        });
        let new_deck_input = cx.new(|cx| InputState::new(window, cx).placeholder("New deck name…"));

        let job_controller = cx.new(MatchAnalysisJobEntity::new);
        let job_subscription = cx.subscribe_in(&job_controller, window, Self::on_job_event);

        let notes_drafts = cx.new(|_| NotesDraftCache::default());
        cx.observe_in(&notes_drafts, window, |ws, drafts, window, cx| {
            if ws.active_tab == AnalysisDetailTab::Notes {
                let key = ws.current_note_target_key();
                if ws
                    .comment_binding
                    .sync(&drafts, key, &ws.comment_input, window, cx)
                {
                    ws.notes_validation_error = None;
                    ws.notes_saved_feedback = false;
                }
                cx.notify();
            }
        })
        .detach();
        let comment_input_sub = cx.subscribe(&comment_input, |this, inp, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.notes_validation_error = this
                    .comment_binding
                    .record_input(&inp, &this.notes_drafts, cx)
                    .err();
                this.notes_saved_feedback = false;
                cx.notify();
            }
        });

        let mut ws = Self {
            visible: true,
            welcome_mode: saved_selection.is_none(),
            focus_handle: cx.focus_handle(),
            splitter_focus: cx.focus_handle(),
            move_scroll: UniformListScrollHandle::new(),
            job_controller,
            _job_subscription: job_subscription,
            board_view,
            tag_input,
            comment_input,
            comment_binding: NoteEditorBinding::default(),
            new_deck_input,
            _comment_input_sub: comment_input_sub,
            match_id: None,
            match_summary: None,
            match_meta: None,
            games: Vec::new(),
            current_game_no: None,
            current_game_id: None,
            positions: Vec::new(),
            source_moves: Vec::new(),
            assessments: HashMap::new(),
            import_warnings: Vec::new(),
            active_target: None,
            preview: BoardPreviewState::default(),
            bookmarked: false,
            bookmark_busy: false,
            copied_feedback: false,
            move_filter: MoveHistoryFilter::All,
            error_positions: Vec::new(),
            loading_game: false,
            active_tab: AnalysisDetailTab::Overview,
            board_widths: BoardWidthController::from_persisted(persisted_widths),
            dragging_splitter: false,
            drag_start_x: 0.0,
            drag_start_width: 560.0,
            last_viewport_width: 1440.0,
            game_menu_open: false,
            catalog_menu_open: false,
            options_popover_open: false,
            catalog_items: Vec::new(),
            catalog_selected: None,
            catalog_selected_id: None,
            catalog_source_revision: 0,
            catalog_next_cursor: None,
            catalog_loading: false,
            catalog_epoch: 0,
            checker_subtab: CheckerSubTab::Candidates,
            position_result: None,
            position_result_loading: false,
            opening_book: None,
            opening_book_expanded: false,
            opening_book_loading: false,
            compare_slots: [Vec::new(), Vec::new(), Vec::new()],
            compare_slot_ids: [None, None, None],
            compare_sort_delta: None,
            dice_state: DiceTabState::Idle,
            selected_dice_cell: None,
            observed_rolls: None,
            observed_rolls_game_only: true,
            observed_rolls_error: None,
            cube_filter: CubeFilterState::default(),
            cube_decisions: Vec::new(),
            cube_loading: false,
            cube_auto_select: CubeAutoSelectState::default(),
            calculating_chances_for: None,
            calculated_chances: HashMap::new(),
            statistics: None,
            statistics_loading: false,
            stats_drilldown: None,
            compare_stats_a_id: None,
            compare_stats_b_id: None,
            compare_stats_a: None,
            compare_stats_b: None,
            notes_scope: AnnotationKind::Position,
            notes_drafts,
            notes_saving: false,
            notes_saved_feedback: false,
            notes_validation_error: None,
            decks: Vec::new(),
            selected_deck_id: None,
            cards_busy: false,
            cards_status: None,
            card_suggestions: None,
            card_suggestions_loading: false,
            selected_suggestions: BTreeSet::new(),
            suggestion_boards: HashMap::new(),
            welcome_importing: false,
            match_epoch: 0,
            game_epoch: 0,
            position_epoch: 0,
            match_load_task: None,
            game_load_task: None,
            position_load_task: None,
            stats_load_task: None,
            cube_load_task: None,
            rolls_load_task: None,
        };

        if let Some(sel) = saved_selection {
            ws.open_match(
                sel.match_id,
                Some(sel.game_no),
                sel.position_id,
                None,
                window,
                cx,
            );
        }

        ws
    }

    /// Toggles visibility when switching workspaces. Preserves all state and
    /// NEVER cancels or drops `job_controller` background subscriptions!
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        cx.notify();
    }

    /// Updates the board/checker/cube/dice artwork configuration live.
    pub fn set_theme_config(
        &mut self,
        config: neondojo_board::BoardThemeConfig,
        cx: &mut Context<Self>,
    ) {
        self.board_view
            .update(cx, |bv, cx| bv.set_theme_config(config, cx));
        cx.notify();
    }

    /// Opens a match (and optional game, position, or cube decision) in Analysis.
    pub fn open_match(
        &mut self,
        match_id: Uuid,
        game_no: Option<i32>,
        position_id: Option<Uuid>,
        decision_no: Option<i32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same_match = self.match_id == Some(match_id);
        if position_id.is_some() && self.active_tab == AnalysisDetailTab::Cube {
            self.active_tab = AnalysisDetailTab::Overview;
        }
        self.welcome_mode = false;
        self.game_menu_open = false;
        self.catalog_menu_open = false;
        self.options_popover_open = false;

        if !same_match {
            self.match_epoch = self.match_epoch.wrapping_add(1);
            self.game_epoch = self.game_epoch.wrapping_add(1);
            self.position_epoch = self.position_epoch.wrapping_add(1);
            self.match_load_task = None;
            self.game_load_task = None;
            self.position_load_task = None;
            self.stats_load_task = None;
            self.cube_load_task = None;
            self.rolls_load_task = None;

            self.match_id = Some(match_id);
            self.match_summary = None;
            self.match_meta = None;
            self.games.clear();
            self.current_game_no = game_no;
            self.current_game_id = None;
            self.positions.clear();
            self.source_moves.clear();
            self.assessments.clear();
            self.import_warnings.clear();
            self.active_target =
                position_id.map(|pid| ActiveBoardTarget::Checker { position_id: pid });
            self.preview.reset();
            self.catalog_items.clear();
            self.catalog_selected = None;
            self.catalog_selected_id = None;
            self.catalog_next_cursor = None;
            self.catalog_loading = false;
            self.position_result = None;
            self.opening_book = None;
            self.compare_slots = [Vec::new(), Vec::new(), Vec::new()];
            self.compare_slot_ids = [None, None, None];
            self.dice_state = DiceTabState::Idle;
            self.selected_dice_cell = None;
            self.observed_rolls = None;
            self.observed_rolls_error = None;
            self.cube_decisions.clear();
            self.calculated_chances.clear();
            self.statistics = None;
            self.stats_drilldown = None;
            self.compare_stats_a = None;
            self.compare_stats_b = None;
            self.notes_drafts.as_mut(cx).retain_unsaved();
            self.notes_validation_error = None;
            self.notes_saved_feedback = false;
            self.card_suggestions = None;
            self.selected_suggestions.clear();
            self.suggestion_boards.clear();

            self.job_controller.update(cx, |jobs, cx| {
                jobs.attach_existing_match_jobs(match_id, cx);
                jobs.refresh_estimate(match_id, None, cx);
            });
            self.load_match_initial(match_id, game_no, position_id, decision_no, window, cx);
        } else {
            if let Some(gno) = game_no
                && self.current_game_no != Some(gno)
            {
                self.select_game(gno, position_id, decision_no, window, cx);
            } else if let Some(pid) = position_id {
                self.select_checker_position(pid, window, cx);
            } else if let Some(dno) = decision_no {
                self.select_cube_by_decision_no(dno, window, cx);
            }
        }

        self.persist_workspace_selection(cx);
        cx.notify();
    }

    /// Invalidates cached match data when Play switches a live match to Competition mode
    /// (`WorkspaceEvent::PlaySwitchedToCompetition`).
    pub fn invalidate_match(&mut self, match_id: Uuid, cx: &mut Context<Self>) {
        if self.match_id != Some(match_id) {
            return;
        }
        self.match_epoch = self.match_epoch.wrapping_add(1);
        self.game_epoch = self.game_epoch.wrapping_add(1);
        self.position_epoch = self.position_epoch.wrapping_add(1);
        self.match_load_task = None;
        self.game_load_task = None;
        self.position_load_task = None;
        self.stats_load_task = None;
        self.cube_load_task = None;
        self.rolls_load_task = None;

        self.match_id = None;
        self.match_summary = None;
        self.match_meta = None;
        self.games.clear();
        self.current_game_no = None;
        self.current_game_id = None;
        self.positions.clear();
        self.source_moves.clear();
        self.assessments.clear();
        self.import_warnings.clear();
        self.active_target = None;
        self.preview.reset();
        self.catalog_items.clear();
        self.catalog_selected = None;
        self.catalog_selected_id = None;
        self.position_result = None;
        self.opening_book = None;
        self.dice_state = DiceTabState::Idle;
        self.selected_dice_cell = None;
        self.observed_rolls = None;
        self.cube_decisions.clear();
        self.statistics = None;
        self.notes_drafts.as_mut(cx).retain_unsaved();
        self.welcome_mode = true;

        if cx.has_global::<LocalUiState>() {
            cx.update_global::<LocalUiState, _>(|state, _| {
                if state
                    .workspace_selection
                    .is_some_and(|s| s.match_id == match_id)
                {
                    state.workspace_selection = None;
                }
            });
        }
        cx.notify();
    }

    /// Switches to the Welcome drop zone to import a match directly into Analysis.
    pub fn open_welcome_drop(&mut self, cx: &mut Context<Self>) {
        self.welcome_mode = true;
        self.game_menu_open = false;
        self.catalog_menu_open = false;
        self.options_popover_open = false;
        cx.notify();
    }

    // ── Job Events Handler ───────────────────────────────────────────────────

    fn on_job_event(
        &mut self,
        _entity: &Entity<MatchAnalysisJobEntity>,
        event: &AnalysisJobEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            AnalysisJobEvent::MatchJobUpdated { match_id, job } => {
                cx.emit(WorkspaceEvent::AnalysisJobChanged {
                    match_id: Some(*match_id),
                });
                if self.match_id == Some(*match_id)
                    && matches!(job.status.as_str(), "completed" | "failed")
                {
                    self.refresh_catalog(*match_id, None, window, cx);
                    if let Some(gno) = self.current_game_no {
                        self.load_game_data(*match_id, gno, None, None, window, cx);
                    }
                    if self.active_tab == AnalysisDetailTab::Statistics {
                        self.load_statistics(*match_id, cx);
                    }
                    if self.active_tab == AnalysisDetailTab::Cube {
                        self.load_cube_decisions(*match_id, window, cx);
                    }
                }
            }
            AnalysisJobEvent::PositionAnalysisJobUpdated { position_id, job } => {
                cx.emit(WorkspaceEvent::AnalysisJobChanged {
                    match_id: self.match_id,
                });
                if self.active_checker_position_id() == Some(*position_id)
                    && job.status == "completed"
                {
                    if let Some(mid) = self.match_id {
                        self.refresh_catalog(mid, None, window, cx);
                    }
                    self.load_position_details(*position_id, window, cx);
                }
            }
            AnalysisJobEvent::DiceJobUpdated {
                position_id,
                engine,
                job,
            } => {
                cx.emit(WorkspaceEvent::AnalysisJobChanged {
                    match_id: self.match_id,
                });
                if self.active_checker_position_id() == Some(*position_id) {
                    self.dice_state.on_job_update(*position_id, *engine, job);
                }
            }
            AnalysisJobEvent::Error(msg) => {
                if let (Some(pid), CheckerSubTab::Dice) =
                    (self.active_checker_position_id(), self.checker_subtab)
                    && matches!(self.dice_state, DiceTabState::Submitting { .. })
                {
                    let eng = self.active_engine(cx);
                    self.dice_state.on_failed(pid, eng, msg.clone());
                }
                cx.emit(WorkspaceEvent::ShowNotice(make_notice(
                    "analysis-job-err",
                    BannerKind::Error,
                    msg.clone(),
                )));
            }
        }
        cx.notify();
    }

    // ── Helper Accessors ─────────────────────────────────────────────────────

    pub(crate) fn active_engine(&self, cx: &App) -> MatchAnalysisEngine {
        self.catalog_selected
            .as_ref()
            .and_then(catalog_entry_engine)
            .unwrap_or_else(|| self.job_controller.read(cx).options.engine)
    }

    pub(crate) fn player_names(&self) -> [SharedString; 2] {
        match &self.match_summary {
            Some(m) => [
                SharedString::from(m.white.clone()),
                SharedString::from(m.black.clone()),
            ],
            None => [SharedString::from("White"), SharedString::from("Black")],
        }
    }

    pub(crate) fn active_checker_position_id(&self) -> Option<Uuid> {
        match &self.active_target {
            Some(ActiveBoardTarget::Checker { position_id }) => Some(*position_id),
            Some(ActiveBoardTarget::Cube {
                return_position_id, ..
            }) => *return_position_id,
            None => None,
        }
    }

    pub(crate) fn active_position(&self) -> Option<&Position> {
        let pid = self.active_checker_position_id()?;
        self.positions.iter().find(|p| p.id == pid)
    }

    pub(crate) fn active_cube_decision(&self) -> Option<&ReviewDecision> {
        let ActiveBoardTarget::Cube { review_id, .. } = self.active_target.as_ref()? else {
            return None;
        };
        self.cube_decisions.iter().find(|d| &d.id == review_id)
    }

    pub(crate) fn current_game(&self) -> Option<&MatchGame> {
        let gno = self.current_game_no?;
        self.games.iter().find(|g| g.game_no == gno)
    }

    pub(crate) fn current_note_target_key(&self) -> Option<NoteTargetKey> {
        match self.notes_scope {
            AnnotationKind::Match => self.match_id.map(|id| NoteTargetKey {
                kind: AnnotationKind::Match,
                target_id: id,
            }),
            AnnotationKind::Game => self.current_game_id.map(|id| NoteTargetKey {
                kind: AnnotationKind::Game,
                target_id: id,
            }),
            AnnotationKind::Position | AnnotationKind::Move => {
                self.active_checker_position_id().map(|id| NoteTargetKey {
                    kind: AnnotationKind::Position,
                    target_id: id,
                })
            }
        }
    }

    pub(crate) fn interleaved_rows(&self) -> Vec<InterleavedMoveRow> {
        let current_game_cubes: Vec<&ReviewDecision> = self
            .cube_decisions
            .iter()
            .filter(|c| self.current_game_no.is_none_or(|g| c.game_no == g))
            .collect();
        build_interleaved_move_rows(
            &self.positions,
            &self.source_moves,
            &self.assessments,
            &current_game_cubes,
            self.move_filter,
        )
    }

    fn sync_board(&self, cx: &mut App) {
        let players = self.player_names();
        sync_board_view(
            &self.board_view,
            self.active_position(),
            self.active_cube_decision(),
            &self.preview,
            [players[0].as_ref(), players[1].as_ref()],
            cx,
        );
    }

    fn persist_board_widths(&self, cx: &mut App) {
        if cx.has_global::<LocalUiState>() {
            let widths = self.board_widths.widths;
            cx.update_global::<LocalUiState, _>(|state, _| {
                state.analysis_board_widths = widths;
                #[cfg(not(test))]
                {
                    let _ = state.save_atomic(&LocalUiState::default_path());
                }
            });
        }
    }

    fn persist_workspace_selection(&self, cx: &mut App) {
        let Some(match_id) = self.match_id else {
            return;
        };
        let game_no = self.current_game_no.unwrap_or(1).max(1);
        let position_id = self.active_checker_position_id();
        if cx.has_global::<LocalUiState>() {
            cx.update_global::<LocalUiState, _>(|state, _| {
                state.workspace_selection = Some(WorkspaceSelection {
                    match_id,
                    game_no,
                    position_id,
                });
                #[cfg(not(test))]
                {
                    let _ = state.save_atomic(&LocalUiState::default_path());
                }
            });
        }
    }

    // ── Async Backend Loaders ────────────────────────────────────────────────

    fn load_match_initial(
        &mut self,
        match_id: Uuid,
        requested_game: Option<i32>,
        requested_position: Option<Uuid>,
        requested_decision: Option<i32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        let epoch = self.match_epoch;
        let (catalog_epoch, catalog_query) = self.begin_catalog_read(None);
        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move {
                let summary = facade.get_match(match_id).await?;
                let meta = facade.get_match_metadata(match_id).await.ok();
                let games = facade.match_games(match_id).await.unwrap_or_default();
                let warnings = facade
                    .match_import_warnings(match_id)
                    .await
                    .unwrap_or_default();
                let catalog = facade
                    .list_analysis_catalog(match_id, catalog_query)
                    .await
                    .ok();
                let decks = facade.list_decks().await.unwrap_or_default();
                Ok((summary, meta, games, warnings, catalog, decks))
            })
            .await
        });

        let task = TokioBridge::spawn_read(cx, async move {});
        self.match_load_task = Some(task);

        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok((summary, meta, games, warnings, catalog, decks))) = read.await else {
                return;
            };
            let _ = this.update_in(cx, |ws, window, cx| {
                if ws.match_epoch != epoch || ws.match_id != Some(match_id) {
                    return;
                }
                let retained = !summary.source_available;
                ws.match_summary = Some(summary);
                ws.match_meta = meta;
                ws.import_warnings = warnings;
                ws.decks = decks;
                if ws.selected_deck_id.is_none() {
                    ws.selected_deck_id = ws.decks.first().map(|d| d.id);
                }
                if let Some(cat) = catalog {
                    ws.apply_catalog_page(catalog_epoch, cat, false);
                }
                if retained && !ws.active_tab.available_when_retained() {
                    ws.active_tab = AnalysisDetailTab::Overview;
                }
                ws.games = games;

                let target_game = requested_game
                    .filter(|g| ws.games.iter().any(|mg| mg.game_no == *g))
                    .or_else(|| ws.games.first().map(|g| g.game_no));

                if let Some(gno) = target_game {
                    ws.select_game(gno, requested_position, requested_decision, window, cx);
                } else if let Some(pid) = requested_position {
                    ws.load_single_retained_position(pid, window, cx);
                }
                ws.load_cube_decisions(match_id, window, cx);
                if ws.active_tab == AnalysisDetailTab::Statistics {
                    ws.load_statistics(match_id, cx);
                }
                if ws.active_tab == AnalysisDetailTab::Notes {
                    ws.load_current_note(window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_single_retained_position(
        &mut self,
        position_id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        let epoch = self.game_epoch;
        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move { facade.get_position(position_id).await }).await
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(pos)) = read.await else {
                return;
            };
            let _ = this.update_in(cx, |ws, window, cx| {
                if ws.game_epoch != epoch {
                    return;
                }
                ws.positions = vec![pos];
                ws.select_checker_position(position_id, window, cx);
            });
        })
        .detach();
    }

    fn select_game(
        &mut self,
        game_no: i32,
        requested_position: Option<Uuid>,
        requested_decision: Option<i32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(match_id) = self.match_id else {
            return;
        };
        self.current_game_no = Some(game_no);
        self.game_menu_open = false;
        self.preview.reset();
        self.load_game_data(
            match_id,
            game_no,
            requested_position,
            requested_decision,
            window,
            cx,
        );
        if self.checker_subtab == CheckerSubTab::Observed {
            self.load_observed_rolls(match_id, cx);
        }
        self.persist_workspace_selection(cx);
        cx.notify();
    }

    fn load_game_data(
        &mut self,
        match_id: Uuid,
        game_no: i32,
        requested_position: Option<Uuid>,
        requested_decision: Option<i32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        self.game_epoch = self.game_epoch.wrapping_add(1);
        let epoch = self.game_epoch;
        self.loading_game = true;
        let analysis_id = self.catalog_selected_id.clone();

        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move {
                let mut all_positions = Vec::new();
                let mut cursor: Option<Uuid> = None;
                loop {
                    let page = facade
                        .list_positions(PositionQuery {
                            database_id: None,
                            match_id: Some(match_id),
                            game_no: Some(game_no),
                            position_id: None,
                            after: cursor,
                            limit: Some(100),
                        })
                        .await?;
                    all_positions.extend(page.items);
                    match page.next_cursor {
                        Some(next) => cursor = Some(next),
                        None => break,
                    }
                }
                let catalog_moves = facade.get_move_catalog(match_id, game_no).await.ok();
                let assessments = facade
                    .get_move_assessments(
                        match_id,
                        MoveAssessmentQuery {
                            game_no,
                            job_id: None,
                            analysis_id,
                        },
                    )
                    .await
                    .ok();
                Ok((all_positions, catalog_moves, assessments))
            })
            .await
        });

        self.game_load_task = Some(TokioBridge::spawn_read(cx, async move {}));

        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok((positions, catalog_moves, assessments))) = read.await else {
                let _ = this.update(cx, |ws, cx| {
                    if ws.game_epoch == epoch {
                        ws.loading_game = false;
                        cx.notify();
                    }
                });
                return;
            };
            let _ = this.update_in(cx, |ws, window, cx| {
                if ws.game_epoch != epoch {
                    return;
                }
                ws.loading_game = false;
                ws.positions = positions;
                if let Some(gd) = catalog_moves {
                    ws.current_game_id = Some(gd.game_id);
                    ws.source_moves = gd.items;
                } else {
                    ws.current_game_id = None;
                    ws.source_moves.clear();
                }

                ws.assessments.clear();
                ws.error_positions.clear();
                if let Some(ga) = assessments {
                    for item in &ga.items {
                        let snap = MoveAssessmentSnapshot::from(item);
                        if matches!(
                            snap.severity,
                            MoveSeverity::Doubtful
                                | MoveSeverity::Bad
                                | MoveSeverity::Blunder
                                | MoveSeverity::VeryBad
                        ) {
                            ws.error_positions.push(item.position_id);
                        }
                        ws.assessments.insert(item.position_id, snap);
                    }
                }

                if let Some(dno) = requested_decision {
                    ws.select_cube_by_decision_no(dno, window, cx);
                } else if let Some(pid) = requested_position
                    .filter(|id| ws.positions.iter().any(|p| p.id == *id))
                    .or_else(|| ws.positions.first().map(|p| p.id))
                {
                    ws.select_checker_position(pid, window, cx);
                } else {
                    // Cube-only game (`position_count == 0`)
                    if let Some((cube_id, cube_dno)) = ws
                        .cube_decisions
                        .iter()
                        .find(|c| c.game_no == game_no)
                        .map(|c| (c.id.clone(), c.decision.decision_no))
                    {
                        ws.select_cube_decision(cube_id, cube_dno, None, window, cx);
                    } else {
                        ws.set_board_target(None, window, cx);
                        ws.sync_board(cx);
                    }
                }
                if ws.active_tab == AnalysisDetailTab::Notes {
                    ws.load_current_note(window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn select_checker_position(
        &mut self,
        position_id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_board_target(Some(ActiveBoardTarget::Checker { position_id }), window, cx);
        self.preview.reset();
        self.selected_dice_cell = None;
        self.copied_feedback = false;
        let engine = self.active_engine(cx);
        self.dice_state.reset_for_position(position_id, engine);

        self.sync_board(cx);
        self.persist_workspace_selection(cx);
        self.load_position_details(position_id, window, cx);

        if self.active_tab == AnalysisDetailTab::Notes {
            self.load_current_note(window, cx);
        }
        cx.notify();
    }

    fn select_cube_decision(
        &mut self,
        review_id: String,
        decision_no: i32,
        return_position_id: Option<Uuid>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = ActiveBoardTarget::Cube {
            review_id,
            decision_no,
            return_position_id: return_position_id.or_else(|| self.active_checker_position_id()),
        };
        self.set_board_target(Some(target), window, cx);
        self.preview.reset();
        self.sync_board(cx);
        if self.active_tab == AnalysisDetailTab::Notes {
            self.load_current_note(window, cx);
        }
        cx.notify();
    }

    fn select_cube_by_decision_no(
        &mut self,
        decision_no: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(match_id), Some(game_no)) = (self.match_id, self.current_game_no) else {
            return;
        };
        self.cube_filter.reset();
        self.cube_filter.game_no = Some(game_no);
        self.active_tab = AnalysisDetailTab::Cube;
        self.set_board_target(None, window, cx);
        self.preview.reset();
        self.load_cube_page(match_id, Some(decision_no), window, cx);
    }

    fn set_board_target(
        &mut self,
        target: Option<ActiveBoardTarget>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_position = self.active_checker_position_id();
        self.active_target = target;
        if previous_position != self.active_checker_position_id() {
            // The selection belongs to the match. Only entries and the cursor are position-scoped.
            self.catalog_items.clear();
            self.catalog_next_cursor = None;
            if let Some(match_id) = self.match_id {
                self.refresh_catalog(match_id, None, window, cx);
            }
        }
    }

    fn effective_compare_ids(&self, pos: &Position) -> [Option<String>; 3] {
        CompareEngineSlot::ALL.map(|slot| {
            let options = comparison_options(
                &self.catalog_items,
                self.catalog_selected.as_ref(),
                pos,
                slot,
            );
            let preferred = self.compare_slot_ids[slot as usize].as_deref();
            options
                .iter()
                .find(|e| Some(e.id.as_str()) == preferred)
                .or_else(|| options.first())
                .map(|e| e.id.clone())
        })
    }

    fn load_position_details(
        &mut self,
        position_id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        let Some(pos) = self.active_position().filter(|pos| pos.id == position_id) else {
            return;
        };
        let player = pos.player;
        let compare_ids = self.effective_compare_ids(pos);
        self.position_epoch = self.position_epoch.wrapping_add(1);
        let epoch = self.position_epoch;
        self.position_result_loading = true;
        self.opening_book_loading = true;

        let analysis_id = self.catalog_selected_id.clone();
        let active_engine = self.active_engine(cx);
        self.compare_slots = Default::default();
        self.position_result = None;

        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move {
                let result = facade
                    .get_position_analysis_result(position_id, analysis_id.as_deref())
                    .await
                    .ok()
                    .flatten();
                let bookmarked = facade
                    .get_position_bookmark(position_id)
                    .await
                    .map(|b| b.saved)
                    .unwrap_or(false);
                let book = facade.lookup_opening_book_position(position_id).await.ok();
                let pos_jobs = facade
                    .list_position_jobs(position_id, Some(active_engine))
                    .await
                    .unwrap_or_default();

                // Load 3-engine compare candidates if slot ids exist
                let mut slot_cands: [Vec<CheckerCandidate>; 3] =
                    [Vec::new(), Vec::new(), Vec::new()];
                for (si, cid) in compare_ids.iter().enumerate() {
                    if let Some(id) = cid.as_deref()
                        && let Ok(Some(res)) = facade
                            .get_position_analysis_result(position_id, Some(id))
                            .await
                    {
                        slot_cands[si] =
                            extract_candidates(&res, player, Some(CompareEngineSlot::ALL[si]));
                    }
                }
                Ok((result, bookmarked, book, pos_jobs, slot_cands))
            })
            .await
        });

        self.position_load_task = Some(TokioBridge::spawn_read(cx, async move {}));

        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok((result, bookmarked, book, pos_jobs, slot_cands))) = read.await else {
                let _ = this.update(cx, |ws, cx| {
                    if ws.position_epoch == epoch {
                        ws.position_result_loading = false;
                        ws.opening_book_loading = false;
                        cx.notify();
                    }
                });
                return;
            };
            let _ = this.update_in(cx, |ws, _window, cx| {
                if ws.position_epoch != epoch {
                    return;
                }
                ws.position_result = result;
                ws.position_result_loading = false;
                ws.bookmarked = bookmarked;
                ws.opening_book = book;
                ws.opening_book_loading = false;
                ws.compare_slots = slot_cands;

                // Synchronize DiceTabState with any stored/running dice job for active_engine
                if let Some(dice_job) = pos_jobs.iter().find(|j| j.kind == "dice") {
                    ws.dice_state
                        .on_job_update(position_id, active_engine, dice_job);
                    if matches!(dice_job.status.as_str(), "queued" | "running") {
                        let job_clone = dice_job.clone();
                        ws.job_controller.update(cx, |jc, cx| {
                            jc.track_dice_job(position_id, active_engine, job_clone, cx);
                        });
                    }
                } else if ws.active_tab == AnalysisDetailTab::Overview
                    && ws.checker_subtab == CheckerSubTab::Dice
                {
                    // Auto-start dice calculation when missing (respecting DeletedAwaitingExplicitRequest & Failed!)
                    if let DiceTransitionAction::StartCalculation {
                        position_id: pid,
                        engine,
                    } = ws.dice_state.on_missing_result(position_id, active_engine)
                    {
                        ws.job_controller.update(cx, |jc, cx| {
                            jc.start_position_job(pid, PositionJobKind::Dice, engine, None, cx);
                        });
                    }
                }

                cx.notify();
            });
        })
        .detach();
    }

    fn begin_catalog_read(&mut self, cursor: Option<String>) -> (u64, AnalysisCatalogQuery) {
        self.catalog_epoch = self.catalog_epoch.wrapping_add(1);
        (
            self.catalog_epoch,
            AnalysisCatalogQuery {
                limit: Some(50),
                cursor,
                position_id: self.active_checker_position_id(),
            },
        )
    }

    fn apply_catalog_page(&mut self, epoch: u64, page: AnalysisCatalogPage, append: bool) -> bool {
        if self.catalog_epoch != epoch || self.match_id != Some(page.match_id) {
            return false;
        }
        if append {
            self.catalog_items.extend(page.items);
        } else {
            self.catalog_items = page.items;
        }
        self.catalog_selected = page.selected;
        self.catalog_selected_id = page.selected_id;
        self.catalog_source_revision = page.source_revision;
        self.catalog_next_cursor = page.next_cursor;
        self.catalog_loading = false;
        true
    }

    fn refresh_catalog(
        &mut self,
        match_id: Uuid,
        cursor: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let append = cursor.is_some();
        let (epoch, query) = self.begin_catalog_read(cursor);
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        self.catalog_loading = true;
        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move { facade.list_analysis_catalog(match_id, query).await })
                .await
        });

        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(page)) = read.await else {
                let _ = this.update(cx, |ws, cx| {
                    if ws.match_id == Some(match_id) && ws.catalog_epoch == epoch {
                        ws.catalog_loading = false;
                        cx.notify();
                    }
                });
                return;
            };
            let _ = this.update_in(cx, |ws, window, cx| {
                let previous = ws
                    .active_position()
                    .map(|pos| ws.effective_compare_ids(pos));
                let previous_revision = ws.catalog_source_revision;
                let previous_selection = ws.catalog_selected.as_ref().map(|entry| entry.id.clone());
                if !ws.apply_catalog_page(epoch, page, append) {
                    return;
                }
                if (previous
                    != ws
                        .active_position()
                        .map(|pos| ws.effective_compare_ids(pos))
                    || previous_revision != ws.catalog_source_revision
                    || previous_selection.as_deref()
                        != ws.catalog_selected.as_ref().map(|entry| entry.id.as_str()))
                    && let Some(id) = ws.active_checker_position_id()
                {
                    ws.load_position_details(id, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn select_displayed_analysis(
        &mut self,
        analysis_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(match_id) = self.match_id else {
            return;
        };
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        self.catalog_menu_open = false;
        let write = TokioBridge::spawn_write(cx, async move {
            run_facade(move || async move {
                facade
                    .select_displayed_analysis(match_id, analysis_id)
                    .await
            })
            .await
        });

        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(sel)) = write.await else {
                return;
            };
            let _ = this.update_in(cx, |ws, window, cx| {
                if ws.match_id != Some(match_id) {
                    return;
                }
                ws.catalog_selected_id = sel.analysis_id;
                ws.refresh_catalog(match_id, None, window, cx);
                if let Some(gno) = ws.current_game_no {
                    let keep_pos = ws.active_checker_position_id();
                    ws.load_game_data(match_id, gno, keep_pos, None, window, cx);
                }
                ws.load_cube_decisions(match_id, window, cx);
                if ws.active_tab == AnalysisDetailTab::Statistics {
                    ws.load_statistics(match_id, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn delete_catalog_entry(
        &mut self,
        analysis_id: String,
        source_revision: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(match_id) = self.match_id else {
            return;
        };
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        let write = TokioBridge::spawn_write(cx, async move {
            run_facade(move || async move {
                facade
                    .delete_analysis_entry(match_id, &analysis_id, source_revision)
                    .await
            })
            .await
        });

        cx.spawn_in(window, async move |this, cx| {
            let res = write.await;
            let _ = this.update_in(cx, |ws, window, cx| {
                match res {
                    Ok(Ok(())) => {
                        ws.refresh_catalog(match_id, None, window, cx);
                        if let Some(gno) = ws.current_game_no {
                            let keep_pos = ws.active_checker_position_id();
                            ws.load_game_data(match_id, gno, keep_pos, None, window, cx);
                        }
                        ws.load_cube_decisions(match_id, window, cx);
                        if ws.active_tab == AnalysisDetailTab::Statistics {
                            ws.load_statistics(match_id, cx);
                        }
                    }
                    Ok(Err(err)) => {
                        cx.emit(WorkspaceEvent::ShowNotice(make_notice(
                            "delete-analysis-err",
                            BannerKind::Error,
                            err.to_string(),
                        )));
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_cube_decisions(&mut self, match_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        self.load_cube_page(match_id, None, window, cx);
    }

    fn load_cube_page(
        &mut self,
        match_id: Uuid,
        target: Option<i32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        self.cube_loading = true;
        let mut query = ReviewQuery {
            job_id: None,
            analysis_id: self.catalog_selected_id.clone(),
            game_no: self.cube_filter.game_no,
            player: self.cube_filter.player,
            kind: Some("cube".to_string()),
            severity: self.cube_filter.severity,
            errors_only: self.cube_filter.errors_only,
            limit: Some(100),
            after: None,
            page: target.is_none().then_some(1),
        };
        let epoch = self.game_epoch;
        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move {
                loop {
                    let mut page = facade
                        .list_review_decisions(match_id, query.clone())
                        .await?;
                    if target.is_none_or(|dno| {
                        page.items.iter().any(|row| row.decision.decision_no == dno)
                    }) || page.next_cursor.is_none()
                    {
                        return Ok(page);
                    }
                    query.after = page.next_cursor.take();
                }
            })
            .await
        });
        self.cube_load_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = read.await;
            let _ = this.update_in(cx, |ws, window, cx| {
                ws.cube_loading = false;
                if ws.match_id != Some(match_id)
                    || (target.is_some()
                        && (ws.game_epoch != epoch || ws.active_tab != AnalysisDetailTab::Cube))
                {
                    return;
                }
                let page = match result {
                    Ok(Ok(page)) => page,
                    Ok(Err(err)) => {
                        if target.is_some() || ws.active_tab == AnalysisDetailTab::Cube {
                            let message = crate::i18n::format_domain_error(
                                crate::i18n::current_locale(cx),
                                &err,
                            );
                            cx.emit(WorkspaceEvent::ShowNotice(make_notice(
                                "cube-load",
                                BannerKind::Error,
                                message,
                            )));
                        }
                        cx.notify();
                        return;
                    }
                    Err(_) => {
                        cx.notify();
                        return;
                    }
                };
                ws.cube_decisions = page.items;
                if let Some(dno) = target {
                    if let Some(id) = ws
                        .cube_decisions
                        .iter()
                        .find(|row| row.decision.decision_no == dno)
                        .map(|row| row.id.clone())
                    {
                        ws.select_cube_decision(id, dno, None, window, cx);
                    } else {
                        let message = neondojo_i18n::I18n::t(
                            crate::i18n::current_locale(cx),
                            "app",
                            "error.positionMissing",
                        );
                        cx.emit(WorkspaceEvent::ShowNotice(make_notice(
                            "cube-target-missing",
                            BannerKind::Error,
                            message,
                        )));
                    }
                    cx.notify();
                    return;
                }
                if ws.active_tab == AnalysisDetailTab::Cube {
                    let current_sel = match &ws.active_target {
                        Some(ActiveBoardTarget::Cube { review_id, .. }) => Some(review_id.as_str()),
                        _ => None,
                    };
                    if let Some((pick_id, pick_dno)) = ws
                        .cube_auto_select
                        .next_auto_selection(current_sel, &ws.cube_decisions, ws.current_game_no)
                        .map(|p| (p.id.clone(), p.decision.decision_no))
                    {
                        ws.select_cube_decision(pick_id, pick_dno, None, window, cx);
                    }
                }
                cx.notify();
            });
        }));
    }

    fn load_statistics(&mut self, match_id: Uuid, cx: &mut Context<Self>) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        self.statistics_loading = true;
        let analysis_id = self.catalog_selected_id.clone();
        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move {
                facade
                    .get_match_statistics(
                        match_id,
                        CatalogStatisticsQuery {
                            analysis_id,
                            game_no: None,
                        },
                    )
                    .await
            })
            .await
        });
        self.stats_load_task = Some(TokioBridge::spawn_read(cx, async move {}));

        cx.spawn(async move |this, cx| {
            let Ok(Ok(stats)) = read.await else {
                let _ = this.update(cx, |ws, cx| {
                    ws.statistics_loading = false;
                    cx.notify();
                });
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.statistics_loading = false;
                if ws.match_id == Some(match_id) {
                    ws.statistics = Some(stats);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_compare_statistics(
        &mut self,
        slot_a: bool,
        analysis_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(match_id) = self.match_id else {
            return;
        };
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        if slot_a {
            self.compare_stats_a_id = Some(analysis_id.clone());
        } else {
            self.compare_stats_b_id = Some(analysis_id.clone());
        }
        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move {
                facade
                    .get_match_statistics(
                        match_id,
                        CatalogStatisticsQuery {
                            analysis_id: Some(analysis_id),
                            game_no: None,
                        },
                    )
                    .await
            })
            .await
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(stats)) = read.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                if slot_a {
                    ws.compare_stats_a = Some(stats);
                } else {
                    ws.compare_stats_b = Some(stats);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_observed_rolls(&mut self, match_id: Uuid, cx: &mut Context<Self>) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        self.observed_rolls_error = None;
        let game_no = if self.observed_rolls_game_only {
            self.current_game_no
        } else {
            None
        };
        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move { facade.match_dice(match_id, game_no).await }).await
        });
        self.rolls_load_task = Some(TokioBridge::spawn_read(cx, async move {}));

        cx.spawn(async move |this, cx| {
            let res = read.await;
            let _ = this.update(cx, |ws, cx| {
                if ws.match_id != Some(match_id) {
                    return;
                }
                match res {
                    Ok(Ok(stats)) => {
                        ws.observed_rolls = Some(stats);
                        ws.observed_rolls_error = None;
                    }
                    Ok(Err(err)) => {
                        ws.observed_rolls_error = Some(err.to_string());
                    }
                    Err(err) => {
                        ws.observed_rolls_error = Some(err.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ── Notes Persistence & 409 Conflict Preservation ────────────────────────

    pub(crate) fn load_current_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.current_note_target_key();
        self.comment_binding
            .sync(&self.notes_drafts, key, &self.comment_input, window, cx);
        let Some(key) = self.current_note_target_key() else {
            return;
        };

        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };

        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move { facade.get_annotations(key.kind, key.target_id).await })
                .await
        });

        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(ann)) = read.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.notes_drafts.as_mut(cx).merge_loaded(&ann);
            });
        })
        .detach();
    }

    fn save_current_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.current_note_target_key() else {
            return;
        };
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };

        let comment_val = self.comment_input.read(cx).value().to_string();
        let submitted = self
            .notes_drafts
            .as_mut(cx)
            .ensure_entry(key)
            .prepare_update(comment_val);
        let submitted = match submitted {
            Ok(update) => update,
            Err(err) => {
                self.notes_validation_error = Some(err);
                cx.notify();
                return;
            }
        };

        self.notes_saving = true;
        self.notes_saved_feedback = false;
        self.notes_validation_error = None;
        let match_id = self.match_id;
        cx.notify();

        let write = TokioBridge::spawn_write(cx, state::save_note(facade, key, submitted));

        cx.spawn_in(window, async move |this, cx| {
            let res = write.await;
            let _ = this.update_in(cx, |ws, _window, cx| {
                ws.notes_saving = false;
                match res {
                    Ok(Ok((saved, submitted))) => {
                        ws.notes_drafts.as_mut(cx).mark_saved(&saved, &submitted);
                        ws.notes_saved_feedback = ws.current_note_target_key() == Some(key)
                            && !ws.notes_drafts.read(cx).has_dirty(key);
                        if let Some(mid) = match_id {
                            cx.emit(WorkspaceEvent::AnnotationSaved { match_id: mid });
                        }
                    }
                    Ok(Err(err)) => {
                        if err.kind == ErrorKind::Conflict {
                            ws.notes_drafts.as_mut(cx).mark_conflict(key, &err);
                        } else if ws.current_note_target_key() == Some(key) {
                            ws.notes_validation_error = Some(err.to_string());
                        }
                    }
                    Err(err) => {
                        if ws.current_note_target_key() == Some(key) {
                            ws.notes_validation_error = Some(format!("Save interrupted: {err}"));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn resolve_note_conflict(
        &mut self,
        keep_my_draft: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.current_note_target_key() else {
            return;
        };
        let Some(requested_draft) = self.notes_drafts.read(cx).get(key).cloned() else {
            return;
        };
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };

        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move { facade.get_annotations(key.kind, key.target_id).await })
                .await
        });

        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(latest)) = read.await else {
                return;
            };
            let _ = this.update_in(cx, |ws, window, cx| {
                ws.apply_note_conflict_resolution(
                    key,
                    &requested_draft,
                    &latest,
                    keep_my_draft,
                    window,
                    cx,
                );
            });
        })
        .detach();
    }

    fn apply_note_conflict_resolution(
        &mut self,
        key: NoteTargetKey,
        requested_draft: &NoteDraft,
        latest: &Annotation,
        keep_my_draft: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.current_note_target_key() != Some(key)
            || self.notes_drafts.read(cx).get(key) != Some(requested_draft)
        {
            return;
        }
        if keep_my_draft {
            self.notes_drafts
                .as_mut(cx)
                .rebase_draft_version(key, latest.version);
            self.save_current_note(window, cx);
        } else {
            self.notes_drafts
                .as_mut(cx)
                .accept_server_latest(key, latest);
        }
    }

    // ── Cards Operations ─────────────────────────────────────────────────────

    fn refresh_card_suggestions(&mut self, cx: &mut Context<Self>) {
        let (Some(match_id), Some(deck_id)) = (self.match_id, self.selected_deck_id) else {
            return;
        };
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        self.card_suggestions_loading = true;
        cx.notify();

        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move {
                facade
                    .suggest_cards(
                        deck_id,
                        SuggestionQuery {
                            match_id,
                            player: None,
                            min_loss: 0.04,
                        },
                    )
                    .await
            })
            .await
        });

        cx.spawn(async move |this, cx| {
            let Ok(Ok(suggestions)) = read.await else {
                let _ = this.update(cx, |ws, cx| {
                    ws.card_suggestions_loading = false;
                    cx.notify();
                });
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.card_suggestions_loading = false;
                let atlas = cx
                    .try_global::<DesktopAppContext>()
                    .map(|ctx| ctx.board_atlas.clone())
                    .unwrap_or_default();
                let players = ws.player_names();
                ws.suggestion_boards.clear();
                ws.selected_suggestions.clear();

                for (ix, item) in suggestions.items.iter().enumerate() {
                    if !item.in_deck {
                        ws.selected_suggestions.insert(item.reference);
                    }
                    let pos = &item.position;
                    let mut arr = [0i32; 26];
                    for (dst, &src) in arr.iter_mut().zip(pos.board.iter()) {
                        *dst = src;
                    }
                    let bp = BoardPosition {
                        board: arr,
                        player: u8::try_from(pos.player).unwrap_or(0).min(1),
                        dice: pos
                            .dice
                            .iter()
                            .filter_map(|&d| u8::try_from(d).ok())
                            .collect(),
                        gnubg_match_id: pos.gnubg_match_id.clone(),
                        notation: pos.notation.clone(),
                    };
                    let bv = cx.new(|cx| {
                        let mut view = BoardView::new(
                            SharedString::from(format!("suggestion-mini-board-{ix}")),
                            atlas.clone(),
                        );
                        view.set_players(players[0].as_ref(), players[1].as_ref(), cx);
                        view.set_show_player_bars(false, cx);
                        view.set_show_pip_counts(false, cx);
                        view.set_position(bp, cx);
                        view
                    });
                    ws.suggestion_boards.insert(item.reference, bv);
                }
                ws.card_suggestions = Some(suggestions);
                cx.notify();
            });
        })
        .detach();
    }

    fn create_new_deck(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.new_deck_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        self.cards_busy = true;
        cx.notify();
        let write = TokioBridge::spawn_write(cx, async move {
            run_facade(move || async move { facade.create_deck(name).await }).await
        });

        cx.spawn_in(window, async move |this, cx| {
            let res = write.await;
            let _ = this.update_in(cx, |ws, window, cx| {
                ws.cards_busy = false;
                match res {
                    Ok(Ok(deck)) => {
                        ws.selected_deck_id = Some(deck.id);
                        ws.decks.push(deck);
                        ws.new_deck_input
                            .update(cx, |inp, cx| inp.set_value("", window, cx));
                        ws.cards_status = Some("Deck created".to_string());
                    }
                    Ok(Err(err)) => {
                        ws.cards_status = Some(err.to_string());
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn add_cards_to_selected_deck(&mut self, current_position_only: bool, cx: &mut Context<Self>) {
        let Some(deck_id) = self.selected_deck_id else {
            return;
        };
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        let req = if current_position_only {
            let Some(pid) = self.active_checker_position_id() else {
                return;
            };
            AddCards {
                analysis_id: self.catalog_selected_id.clone(),
                match_id: self.match_id,
                game_no: self.current_game_no,
                position_id: Some(pid),
                job_id: None,
                min_loss: None,
                player: None,
                tags: Vec::new(),
            }
        } else {
            AddCards {
                analysis_id: self.catalog_selected_id.clone(),
                match_id: self.match_id,
                game_no: None,
                position_id: None,
                job_id: None,
                min_loss: Some(0.04),
                player: None,
                tags: Vec::new(),
            }
        };

        self.cards_busy = true;
        cx.notify();
        let write = TokioBridge::spawn_write(cx, async move {
            run_facade(move || async move { facade.add_cards(deck_id, req).await }).await
        });

        cx.spawn(async move |this, cx| {
            let res = write.await;
            let _ = this.update(cx, |ws, cx| {
                ws.cards_busy = false;
                match res {
                    Ok(Ok(r)) => {
                        ws.cards_status = Some(format!(
                            "Added {} cards ({} updated, {} total)",
                            r.added, r.updated, r.total
                        ));
                    }
                    Ok(Err(err)) => {
                        ws.cards_status = Some(err.to_string());
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save_selected_card_suggestions(&mut self, cx: &mut Context<Self>) {
        let (Some(deck_id), Some(match_id)) = (self.selected_deck_id, self.match_id) else {
            return;
        };
        if self.selected_suggestions.is_empty() {
            return;
        }
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        let decisions: Vec<SuggestionReference> =
            self.selected_suggestions.iter().copied().collect();
        self.cards_busy = true;
        cx.notify();
        let write = TokioBridge::spawn_write(cx, async move {
            run_facade(move || async move {
                facade
                    .save_suggested_cards(
                        deck_id,
                        SaveSuggestions {
                            match_id,
                            decisions,
                        },
                    )
                    .await
            })
            .await
        });

        cx.spawn(async move |this, cx| {
            let res = write.await;
            let _ = this.update(cx, |ws, cx| {
                ws.cards_busy = false;
                match res {
                    Ok(Ok(r)) => {
                        ws.cards_status = Some(format!(
                            "Saved {} suggested cards ({} updated)",
                            r.added, r.updated
                        ));
                        ws.refresh_card_suggestions(cx);
                    }
                    Ok(Err(err)) => {
                        ws.cards_status = Some(err.to_string());
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ── Stepping, Error Navigation, Bookmark, Dice, Welcome Import ───────────

    fn step_move(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.interleaved_rows();
        if rows.is_empty() {
            return;
        }
        let current_idx = rows
            .iter()
            .position(|row| match (row, &self.active_target) {
                (
                    InterleavedMoveRow::Checker { position, .. },
                    Some(ActiveBoardTarget::Checker { position_id }),
                ) => position.id == *position_id,
                (
                    InterleavedMoveRow::CubeAction {
                        review_id: Some(rid),
                        ..
                    },
                    Some(ActiveBoardTarget::Cube { review_id, .. }),
                ) => rid == review_id,
                _ => false,
            });

        let next_idx = match current_idx {
            Some(idx) => {
                let target = idx as isize + delta;
                if target < 0 || target >= rows.len() as isize {
                    return;
                }
                target as usize
            }
            None => 0,
        };

        self.move_scroll
            .scroll_to_item(next_idx, gpui_kit::ScrollStrategy::Nearest);

        match &rows[next_idx] {
            InterleavedMoveRow::Checker { position, .. } => {
                self.select_checker_position(position.id, window, cx);
            }
            InterleavedMoveRow::CubeAction {
                review_id: Some(rid),
                decision_no: Some(dno),
                return_position_id,
                ..
            } => {
                self.select_cube_decision(rid.clone(), *dno, *return_position_id, window, cx);
            }
            InterleavedMoveRow::CubeAction { move_id, .. } => {
                self.open_move_notes_for(*move_id, window, cx);
            }
        }
    }

    fn step_error(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.error_positions.is_empty() {
            return;
        }
        let current = self.active_checker_position_id();
        let next_pid =
            match current.and_then(|cid| self.error_positions.iter().position(|p| *p == cid)) {
                Some(idx) => {
                    if forward {
                        self.error_positions[(idx + 1) % self.error_positions.len()]
                    } else {
                        self.error_positions
                            [(idx + self.error_positions.len() - 1) % self.error_positions.len()]
                    }
                }
                None => {
                    if forward {
                        self.error_positions[0]
                    } else {
                        *self.error_positions.last().unwrap()
                    }
                }
            };
        self.select_checker_position(next_pid, window, cx);
    }

    fn step_candidate_preview(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(res) = self.position_result.as_ref() else {
            return;
        };
        let Some(pos) = self.active_position() else {
            return;
        };
        let cands = extract_candidates(res, pos.player, None);
        if cands.is_empty() {
            return;
        }
        let cur_idx = self.preview.candidate_notation.as_deref().and_then(|n| {
            cands
                .iter()
                .position(|c| c.preview().eq_ignore_ascii_case(n))
        });
        let next_idx = match cur_idx {
            Some(i) => (i as isize + delta).clamp(0, (cands.len() - 1) as isize) as usize,
            None => 0,
        };
        self.preview.hypothetical_dice = None;
        self.preview.candidate_notation = Some(cands[next_idx].preview().to_owned());
        if self.preview.mode == BoardViewMode::Before {
            self.preview.mode = BoardViewMode::Arrows;
        }
        self.sync_board(cx);
        cx.notify();
    }

    fn toggle_bookmark(&mut self, cx: &mut Context<Self>) {
        let Some(pid) = self.active_checker_position_id() else {
            return;
        };
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        let currently = self.bookmarked;
        self.bookmark_busy = true;
        cx.notify();
        let write = TokioBridge::spawn_write(cx, async move {
            run_facade(move || async move {
                if currently {
                    facade.unbookmark_position(pid).await.map(|()| false)
                } else {
                    facade.bookmark_position(pid).await.map(|b| b.saved)
                }
            })
            .await
        });
        cx.spawn(async move |this, cx| {
            let res = write.await;
            let _ = this.update(cx, |ws, cx| {
                ws.bookmark_busy = false;
                if let Ok(Ok(saved)) = res
                    && ws.active_checker_position_id() == Some(pid)
                {
                    ws.bookmarked = saved;
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn handle_dice_tab_selected(&mut self, cx: &mut Context<Self>) {
        let Some(pid) = self.active_checker_position_id() else {
            return;
        };
        let engine = self.active_engine(cx);
        if let DiceTransitionAction::StartCalculation {
            position_id,
            engine,
        } = self.dice_state.on_tab_clicked(pid, engine)
        {
            self.job_controller.update(cx, |jc, cx| {
                jc.start_position_job(position_id, PositionJobKind::Dice, engine, None, cx);
            });
        }
    }

    fn delete_dice_job(&mut self, position_id: Uuid, job_id: Uuid, cx: &mut Context<Self>) {
        let engine = self.active_engine(cx);
        // Immediately mark DeletedAwaitingExplicitRequest so no re-render can auto-restart it
        self.dice_state.on_deleted(position_id, engine);
        self.selected_dice_cell = None;
        self.preview.hypothetical_dice = None;
        self.sync_board(cx);
        cx.notify();

        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        TokioBridge::spawn_write(cx, async move {
            let _ =
                run_facade(
                    move || async move { facade.delete_position_job(position_id, job_id).await },
                )
                .await;
        })
        .detach();
    }

    fn calculate_cube_chances(&mut self, review_id: String, cx: &mut Context<Self>) {
        let Some(dec) = self.cube_decisions.iter().find(|d| d.id == review_id) else {
            return;
        };
        let Some(pos) = dec.review_position.clone() else {
            return;
        };
        let response = matches!(dec.decision.played.as_str(), "take" | "drop");
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        self.calculating_chances_for = Some(review_id.clone());
        cx.notify();

        let write = TokioBridge::spawn_write(cx, async move {
            run_facade(move || async move { facade.calculate_cube_chances(&pos, response).await })
                .await
        });

        cx.spawn(async move |this, cx| {
            let res = write.await;
            let _ = this.update(cx, |ws, cx| {
                ws.calculating_chances_for = None;
                match res {
                    Ok(Ok(chances)) => {
                        ws.calculated_chances.insert(
                            review_id,
                            (chances.probabilities, chances.engine, chances.plies),
                        );
                    }
                    Ok(Err(err)) => {
                        cx.emit(WorkspaceEvent::ShowNotice(make_notice(
                            "cube-chances-err",
                            BannerKind::Error,
                            err.to_string(),
                        )));
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_move_notes_for(
        &self,
        initial_move_id: Option<Uuid>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(game_no) = self.current_game_no else {
            return;
        };
        let players = self.player_names();
        open_move_notes_dialog(
            self.notes_drafts.clone(),
            game_no,
            self.source_moves.clone(),
            initial_move_id,
            [players[0].to_string(), players[1].to_string()],
            window,
            cx,
        );
    }

    fn handle_welcome_drop(
        &mut self,
        batch: DroppedMatchBatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(first_path) = batch.accepted.into_iter().next() else {
            cx.emit(WorkspaceEvent::ShowNotice(make_notice(
                "welcome-drop-empty",
                BannerKind::Warning,
                "No supported backgammon match files found in drop.",
            )));
            return;
        };
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        self.welcome_importing = true;
        cx.notify();
        let write = TokioBridge::spawn_write(cx, async move {
            run_facade(move || async move { facade.import_match_path(&first_path, None).await })
                .await
        });

        cx.spawn_in(window, async move |this, cx| {
            let res = write.await;
            let _ = this.update_in(cx, |ws, window, cx| {
                ws.welcome_importing = false;
                match res {
                    Ok(Ok(imported)) => {
                        cx.emit(WorkspaceEvent::LibraryMutated);
                        ws.open_match(imported.r#match.id, Some(1), None, None, window, cx);
                    }
                    Ok(Err(err)) => {
                        cx.emit(WorkspaceEvent::ShowNotice(make_notice(
                            "welcome-import-err",
                            BannerKind::Error,
                            err.to_string(),
                        )));
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn select_detail_tab(
        &mut self,
        tab: AnalysisDetailTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source_available = self
            .match_summary
            .as_ref()
            .is_none_or(|m| m.source_available);
        if !source_available && !tab.available_when_retained() {
            return;
        }
        let prev = self.active_tab;
        if prev == AnalysisDetailTab::Cube && tab != AnalysisDetailTab::Cube {
            self.cube_auto_select.on_leave_cube_tab();
        }
        if prev != AnalysisDetailTab::Cube && tab == AnalysisDetailTab::Cube {
            self.cube_auto_select.on_enter_cube_tab();
        }
        self.active_tab = tab;

        match tab {
            AnalysisDetailTab::Cube => {
                if let Some(mid) = self.match_id {
                    self.load_cube_decisions(mid, window, cx);
                }
            }
            AnalysisDetailTab::Statistics => {
                if let Some(mid) = self.match_id {
                    self.load_statistics(mid, cx);
                }
            }
            AnalysisDetailTab::Notes => {
                self.load_current_note(window, cx);
            }
            AnalysisDetailTab::Cards => {
                if self.card_suggestions.is_none() && self.selected_deck_id.is_some() {
                    self.refresh_card_suggestions(cx);
                }
            }
            AnalysisDetailTab::Overview => {}
        }
        cx.notify();
    }

    // ── Keyboard Action Handlers ─────────────────────────────────────────────

    fn on_prev_position(&mut self, _: &PrevPosition, window: &mut Window, cx: &mut Context<Self>) {
        self.step_move(-1, window, cx);
    }

    fn on_next_position(&mut self, _: &NextPosition, window: &mut Window, cx: &mut Context<Self>) {
        self.step_move(1, window, cx);
    }

    fn on_next_error(&mut self, _: &NextError, window: &mut Window, cx: &mut Context<Self>) {
        self.step_error(true, window, cx);
    }

    fn on_prev_error(&mut self, _: &PrevError, window: &mut Window, cx: &mut Context<Self>) {
        self.step_error(false, window, cx);
    }

    fn on_show_initial_board(
        &mut self,
        _: &ShowInitialBoard,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview.mode = step_preview_mode(false);
        self.sync_board(cx);
        cx.notify();
    }

    fn on_show_final_board(
        &mut self,
        _: &ShowFinalBoard,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview.mode = step_preview_mode(true);
        self.sync_board(cx);
        cx.notify();
    }

    fn on_copy_position_id(
        &mut self,
        _: &CopyPositionId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pid_str) = self
            .active_cube_decision()
            .and_then(|c| c.review_position.as_ref())
            .or_else(|| self.active_position())
            .map(|p| p.position_id.clone())
        {
            cx.write_to_clipboard(ClipboardItem::new_string(pid_str));
            self.copied_feedback = true;
            cx.notify();
        }
    }

    fn on_toggle_dice_equity(
        &mut self,
        _: &ToggleDiceEquity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_detail_tab(AnalysisDetailTab::Overview, window, cx);
        self.checker_subtab = if self.checker_subtab == CheckerSubTab::Dice {
            CheckerSubTab::Candidates
        } else {
            CheckerSubTab::Dice
        };
        if self.checker_subtab == CheckerSubTab::Dice {
            self.handle_dice_tab_selected(cx);
        }
        cx.notify();
    }

    fn on_analyze_game(&mut self, _: &AnalyzeGame, _window: &mut Window, cx: &mut Context<Self>) {
        if let (Some(mid), Some(gno)) = (self.match_id, self.current_game_no) {
            self.job_controller.update(cx, |jc, cx| {
                jc.start_match_analysis(mid, Some(gno), None, None, cx);
            });
        }
    }

    fn on_analyze_match(&mut self, _: &AnalyzeMatch, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(mid) = self.match_id {
            self.job_controller.update(cx, |jc, cx| {
                jc.start_match_analysis(mid, None, None, None, cx);
            });
        }
    }

    fn on_reset_pane_widths(
        &mut self,
        _: &ResetPaneWidths,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.board_widths.reset_all();
        self.persist_board_widths(cx);
        cx.notify();
    }

    fn adjust_splitter(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.board_widths
            .adjust(self.active_tab, delta, self.last_viewport_width);
        self.persist_board_widths(cx);
        cx.notify();
    }

    fn on_splitter_shrink(
        &mut self,
        _: &SplitterShrink,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.adjust_splitter(-16, cx);
    }

    fn on_splitter_grow(&mut self, _: &SplitterGrow, _window: &mut Window, cx: &mut Context<Self>) {
        self.adjust_splitter(16, cx);
    }

    fn on_splitter_shrink_fast(
        &mut self,
        _: &SplitterShrinkFast,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.adjust_splitter(-64, cx);
    }

    fn on_splitter_grow_fast(
        &mut self,
        _: &SplitterGrowFast,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.adjust_splitter(64, cx);
    }

    fn on_splitter_min(&mut self, _: &SplitterMin, _window: &mut Window, cx: &mut Context<Self>) {
        self.board_widths.set_min(self.active_tab);
        self.persist_board_widths(cx);
        cx.notify();
    }

    fn on_splitter_max(&mut self, _: &SplitterMax, _window: &mut Window, cx: &mut Context<Self>) {
        self.board_widths.set_max(self.active_tab);
        self.persist_board_widths(cx);
        cx.notify();
    }

    fn on_splitter_reset(
        &mut self,
        _: &SplitterReset,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.board_widths.reset_tab(self.active_tab);
        self.persist_board_widths(cx);
        cx.notify();
    }
}

impl Render for AnalysisWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            return div()
                .id("analysis-workspace-hidden")
                .test_support()
                .into_any_element();
        }

        let p = NeonTheme::current_palette(cx);
        let viewport_w = f32::from(window.viewport_size().width).max(960.0);
        self.last_viewport_width = viewport_w;

        // Welcome drop screen when no match is open or user requested quick import
        if self.welcome_mode || self.match_id.is_none() {
            let entity = cx.entity().clone();
            return div()
                .id("analysis-welcome-drop-screen")
                .test_support()
                .v_flex()
                .flex_1()
                .size_full()
                .items_center()
                .justify_center()
                .p(px(32.0))
                .bg(p.bg)
                .child(
                    div().w_full().max_w(px(640.0)).child(
                        MatchDropZone::new("analysis-welcome-dropzone")
                            .busy(self.welcome_importing)
                            .palette(p)
                            .on_drop_batch(move |batch, w, cx| {
                                entity.update(cx, |ws, cx| {
                                    ws.handle_welcome_drop(batch, w, cx);
                                });
                            }),
                    ),
                )
                .into_any_element();
        }

        let board_width = self
            .board_widths
            .effective_width(self.active_tab, viewport_w);
        let inspector_width = (viewport_w - MOVE_LIST_PANE_WIDTH - board_width - 8.0).max(300.0);
        let players = self.player_names();
        let source_available = self
            .match_summary
            .as_ref()
            .is_none_or(|m| m.source_available);
        let rows = self.interleaved_rows();

        // Determine BoardReview context shadowing Left/Right when previewing a move/candidate
        let mut key_ctx = KeyContext::default();
        key_ctx.add(CTX_ANALYSIS);
        if self.preview.is_custom_preview()
            || (self.active_tab == AnalysisDetailTab::Overview
                && self.checker_subtab == CheckerSubTab::Candidates
                && self.position_result.is_some())
        {
            key_ctx.add(CTX_BOARD_REVIEW);
        }

        // 1. Header callbacks
        let ent = cx.entity().clone();
        let header_callbacks = HeaderCallbacks {
            on_select_tab: {
                let e = ent.clone();
                Rc::new(move |tab, w, cx| e.update(cx, |ws, cx| ws.select_detail_tab(tab, w, cx)))
            },
            on_select_game: {
                let e = ent.clone();
                Rc::new(move |gno, w, cx| {
                    e.update(cx, |ws, cx| ws.select_game(gno, None, None, w, cx))
                })
            },
            on_toggle_game_menu: {
                let e = ent.clone();
                Rc::new(move |_, _, cx| {
                    e.update(cx, |ws, cx| {
                        ws.game_menu_open = !ws.game_menu_open;
                        ws.catalog_menu_open = false;
                        cx.notify();
                    })
                })
            },
            on_toggle_catalog_menu: {
                let e = ent.clone();
                Rc::new(move |_, _, cx| {
                    e.update(cx, |ws, cx| {
                        ws.catalog_menu_open = !ws.catalog_menu_open;
                        ws.game_menu_open = false;
                        cx.notify();
                    })
                })
            },
            on_select_analysis: {
                let e = ent.clone();
                Rc::new(move |aid, w, cx| {
                    e.update(cx, |ws, cx| ws.select_displayed_analysis(aid, w, cx))
                })
            },
            on_load_more_catalog: {
                let e = ent.clone();
                Rc::new(move |_, w, cx| {
                    e.update(cx, |ws, cx| {
                        if let (Some(mid), Some(cur)) =
                            (ws.match_id, ws.catalog_next_cursor.clone())
                        {
                            ws.refresh_catalog(mid, Some(cur), w, cx);
                        }
                    })
                })
            },
            on_delete_analysis: {
                let e = ent.clone();
                Rc::new(move |aid, rev, w, cx| {
                    e.update(cx, |ws, cx| ws.delete_catalog_entry(aid, rev, w, cx))
                })
            },
            on_toggle_options_popover: {
                let e = ent.clone();
                Rc::new(move |_, _, cx| {
                    e.update(cx, |ws, cx| {
                        ws.options_popover_open = !ws.options_popover_open;
                        if ws.options_popover_open
                            && let Some(mid) = ws.match_id
                        {
                            let gno = ws.current_game_no;
                            ws.job_controller.update(cx, |jc, cx| {
                                let scope_g = if jc.options.scope_current_game {
                                    gno
                                } else {
                                    None
                                };
                                jc.refresh_estimate(mid, scope_g, cx);
                            });
                        }
                        cx.notify();
                    })
                })
            },
            on_start_analysis: {
                let e = ent.clone();
                Rc::new(move |scope_game, _, cx| {
                    e.update(cx, |ws, cx| {
                        if let Some(mid) = ws.match_id {
                            let gno = if scope_game { ws.current_game_no } else { None };
                            ws.options_popover_open = false;
                            ws.job_controller.update(cx, |jc, cx| {
                                jc.start_match_analysis(mid, gno, None, None, cx);
                            });
                            cx.notify();
                        }
                    })
                })
            },
            on_cancel_job: {
                let e = ent.clone();
                Rc::new(move |jid, _, cx| {
                    e.update(cx, |ws, cx| {
                        ws.job_controller
                            .update(cx, |jc, cx| jc.cancel_job(jid, cx));
                    })
                })
            },
            on_open_study_report: {
                let e = ent.clone();
                Rc::new(move |mid, _, cx| {
                    e.update(cx, |_ws, cx| {
                        cx.emit(WorkspaceEvent::OpenStudyReport { match_id: mid });
                    })
                })
            },
            on_open_welcome_drop: {
                let e = ent.clone();
                Rc::new(move |_, _, cx| e.update(cx, |ws, cx| ws.open_welcome_drop(cx)))
            },
        };

        let header_el = render_analysis_header(
            HeaderProps {
                match_summary: self.match_summary.as_ref(),
                match_meta: self.match_meta.as_ref(),
                games: &self.games,
                current_game_no: self.current_game_no,
                active_tab: self.active_tab,
                catalog_items: &self.catalog_items,
                catalog_selected: self.catalog_selected.as_ref(),
                catalog_selected_id: self.catalog_selected_id.as_deref(),
                catalog_source_revision: self.catalog_source_revision,
                catalog_next_cursor: self.catalog_next_cursor.as_deref(),
                catalog_loading: self.catalog_loading,
                game_menu_open: self.game_menu_open,
                catalog_menu_open: self.catalog_menu_open,
                options_popover_open: self.options_popover_open,
                job_controller: &self.job_controller,
                callbacks: header_callbacks,
                palette: p,
            },
            cx,
        );

        // 2. Left pane: MoveHistoryView
        let move_callbacks = MoveHistoryCallbacks {
            on_select_filter: {
                let e = ent.clone();
                Rc::new(move |f, _, cx| {
                    e.update(cx, |ws, cx| {
                        ws.move_filter = f;
                        cx.notify();
                    })
                })
            },
            on_select_checker: {
                let e = ent.clone();
                Rc::new(move |pid, w, cx| {
                    e.update(cx, |ws, cx| ws.select_checker_position(pid, w, cx))
                })
            },
            on_select_cube: {
                let e = ent.clone();
                Rc::new(move |rid, dno, ret, w, cx| {
                    e.update(cx, |ws, cx| {
                        ws.active_tab = AnalysisDetailTab::Cube;
                        ws.select_cube_decision(rid, dno, ret, w, cx);
                    })
                })
            },
            on_open_move_notes: {
                let e = ent.clone();
                Rc::new(move |mid, w, cx| e.update(cx, |ws, cx| ws.open_move_notes_for(mid, w, cx)))
            },
        };

        let left_pane = render_move_history_pane(
            rows,
            self.active_target.as_ref(),
            self.move_filter,
            self.current_game(),
            players.clone(),
            self.loading_game,
            &self.move_scroll,
            move_callbacks,
            p,
            cx,
        );

        // 3. Center pane: AnalysisBoardPane
        let current_pos = self.active_position();
        let active_cube = self.active_cube_decision();
        let facade_opt = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade());
        let game_plans = active_cube
            .and_then(|c| c.review_position.as_ref())
            .or(current_pos)
            .and_then(|pos| compute_game_plans(&pos.board, facade_opt.as_ref()));

        let error_index = self
            .active_checker_position_id()
            .and_then(|cid| self.error_positions.iter().position(|pid| *pid == cid));

        let board_callbacks = BoardPaneCallbacks {
            on_prev_move: {
                let e = ent.clone();
                Rc::new(move |_, w, cx| e.update(cx, |ws, cx| ws.step_move(-1, w, cx)))
            },
            on_next_move: {
                let e = ent.clone();
                Rc::new(move |_, w, cx| e.update(cx, |ws, cx| ws.step_move(1, w, cx)))
            },
            on_prev_error: {
                let e = ent.clone();
                Rc::new(move |_, w, cx| e.update(cx, |ws, cx| ws.step_error(false, w, cx)))
            },
            on_next_error: {
                let e = ent.clone();
                Rc::new(move |_, w, cx| e.update(cx, |ws, cx| ws.step_error(true, w, cx)))
            },
            on_view_mode: {
                let e = ent.clone();
                Rc::new(move |mode, _, cx| {
                    e.update(cx, |ws, cx| {
                        ws.preview.mode = mode;
                        ws.sync_board(cx);
                        cx.notify();
                    })
                })
            },
            on_reset_preview: {
                let e = ent.clone();
                Rc::new(move |_, _, cx| {
                    e.update(cx, |ws, cx| {
                        ws.preview.reset();
                        ws.selected_dice_cell = None;
                        ws.sync_board(cx);
                        cx.notify();
                    })
                })
            },
            on_toggle_bookmark: {
                let e = ent.clone();
                Rc::new(move |_, _, cx| e.update(cx, |ws, cx| ws.toggle_bookmark(cx)))
            },
            on_search_position: {
                let e = ent.clone();
                Rc::new(move |pid_str, _, cx| {
                    e.update(cx, |_ws, cx| {
                        cx.emit(WorkspaceEvent::SearchPositionInLibrary {
                            position_id: pid_str,
                        });
                    })
                })
            },
            on_copy_position_id: {
                let e = ent.clone();
                Rc::new(move |pid_str, _, cx| {
                    e.update(cx, |ws, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(pid_str));
                        ws.copied_feedback = true;
                        cx.notify();
                    })
                })
            },
            on_return_to_checker: {
                let e = ent.clone();
                Rc::new(move |_, w, cx| {
                    e.update(cx, |ws, cx| {
                        if let Some(pid) = ws.active_checker_position_id() {
                            ws.active_tab = AnalysisDetailTab::Overview;
                            ws.select_checker_position(pid, w, cx);
                        }
                    })
                })
            },
            on_open_move_notes: {
                let e = ent.clone();
                Rc::new(move |_, w, cx| {
                    e.update(cx, |ws, cx| {
                        let mid = ws.active_cube_decision().and_then(|c| c.move_id);
                        ws.open_move_notes_for(mid, w, cx);
                    })
                })
            },
        };

        let center_pane = render_board_pane(
            BoardPaneProps {
                width_px: board_width,
                board_view: self.board_view.clone(),
                active_target: self.active_target.as_ref(),
                position: current_pos,
                cube_decision: active_cube,
                preview: &self.preview,
                players: players.clone(),
                bookmarked: self.bookmarked,
                bookmark_busy: self.bookmark_busy,
                can_prev_move: !self.positions.is_empty() || !self.source_moves.is_empty(),
                can_next_move: !self.positions.is_empty() || !self.source_moves.is_empty(),
                error_index,
                error_total: self.error_positions.len(),
                copied_feedback: self.copied_feedback,
                game_plans,
                callbacks: board_callbacks,
                palette: p,
            },
            cx,
        );

        // 4. Focusable keyboard + pointer resizable splitter (`CTX_SPLITTER`)
        let splitter = div()
            .id("analysis-board-splitter")
            .test_support()
            .key_context(CTX_SPLITTER)
            .track_focus(&self.splitter_focus)
            .w(px(6.0))
            .h_full()
            .flex_shrink_0()
            .cursor(CursorStyle::ResizeLeftRight)
            .bg(if self.dragging_splitter {
                p.accent
            } else {
                p.border
            })
            .hover(move |s| s.bg(p.accent))
            .on_action(cx.listener(Self::on_splitter_shrink))
            .on_action(cx.listener(Self::on_splitter_grow))
            .on_action(cx.listener(Self::on_splitter_shrink_fast))
            .on_action(cx.listener(Self::on_splitter_grow_fast))
            .on_action(cx.listener(Self::on_splitter_min))
            .on_action(cx.listener(Self::on_splitter_max))
            .on_action(cx.listener(Self::on_splitter_reset))
            .on_key_down(cx.listener(|ws, ev: &KeyDownEvent, _w, cx| {
                if ev.keystroke.key == "escape" && ws.board_widths.cancel_edit(ws.active_tab) {
                    ws.persist_board_widths(cx);
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, ev: &MouseDownEvent, window, cx| {
                    ws.splitter_focus.focus(window, cx);
                    ws.board_widths.begin_edit(ws.active_tab);
                    ws.dragging_splitter = true;
                    ws.drag_start_x = f32::from(ev.position.x);
                    ws.drag_start_width = board_width;
                    cx.notify();
                }),
            );

        // 5. Right pane: Active Inspector sub-view
        let inspector_body = match self.active_tab {
            AnalysisDetailTab::Overview => {
                let checker_callbacks = CheckerCallbacks {
                    on_select_subtab: {
                        let e = ent.clone();
                        Rc::new(move |st, _, cx| {
                            e.update(cx, |ws, cx| {
                                ws.checker_subtab = st;
                                match st {
                                    CheckerSubTab::Dice => ws.handle_dice_tab_selected(cx),
                                    CheckerSubTab::Observed => {
                                        if let Some(mid) = ws.match_id {
                                            ws.load_observed_rolls(mid, cx);
                                        }
                                    }
                                    _ => {}
                                }
                                cx.notify();
                            })
                        })
                    },
                    on_preview_candidate: {
                        let e = ent.clone();
                        Rc::new(move |notation, _, cx| {
                            e.update(cx, |ws, cx| {
                                ws.preview.hypothetical_dice = None;
                                ws.preview.candidate_notation = Some(notation);
                                if ws.preview.mode == BoardViewMode::Before {
                                    ws.preview.mode = BoardViewMode::Arrows;
                                }
                                ws.sync_board(cx);
                                cx.notify();
                            })
                        })
                    },
                    on_recalculate_position: {
                        let e = ent.clone();
                        Rc::new(move |_, w, cx| {
                            e.update(cx, |ws, cx| {
                                if let Some(pid) = ws.active_checker_position_id() {
                                    let eng = ws.active_engine(cx);
                                    ws.job_controller.update(cx, |jc, cx| {
                                        jc.open_recalculate_dialog(
                                            pid,
                                            PositionJobKind::Analysis,
                                            eng,
                                            w,
                                            cx,
                                        );
                                    });
                                }
                            })
                        })
                    },
                    on_toggle_opening_book: {
                        let e = ent.clone();
                        Rc::new(move |_, _, cx| {
                            e.update(cx, |ws, cx| {
                                ws.opening_book_expanded = !ws.opening_book_expanded;
                                cx.notify();
                            })
                        })
                    },
                    on_toggle_compare_sort_delta: {
                        let e = ent.clone();
                        Rc::new(move |slot, _, cx| {
                            e.update(cx, |ws, cx| {
                                ws.compare_sort_delta =
                                    (ws.compare_sort_delta != Some(slot)).then_some(slot);
                                cx.notify();
                            })
                        })
                    },
                    on_pick_compare_slot: {
                        let e = ent.clone();
                        Rc::new(move |slot, id, w, cx| {
                            e.update(cx, |ws, cx| {
                                ws.compare_slot_ids[slot as usize] = id;
                                ws.preview.reset();
                                ws.sync_board(cx);
                                if let Some(pid) = ws.active_checker_position_id() {
                                    ws.load_position_details(pid, w, cx);
                                }
                                cx.notify();
                            })
                        })
                    },
                    on_dice_cell_click: {
                        let e = ent.clone();
                        Rc::new(move |dice, best_notation, _, cx| {
                            e.update(cx, |ws, cx| {
                                ws.selected_dice_cell = Some(dice);
                                ws.preview.candidate_notation = None;
                                if let Some(best_notation) = best_notation {
                                    ws.preview.hypothetical_dice = Some(HypotheticalDicePreview {
                                        dice,
                                        best_notation,
                                    });
                                    if ws.preview.mode == BoardViewMode::Before {
                                        ws.preview.mode = BoardViewMode::Arrows;
                                    }
                                } else {
                                    ws.preview.reset();
                                }
                                ws.sync_board(cx);
                                cx.notify();
                            })
                        })
                    },
                    on_dice_manual_retry: {
                        let e = ent.clone();
                        Rc::new(move |_, _, cx| {
                            e.update(cx, |ws, cx| {
                                if let Some(pid) = ws.active_checker_position_id() {
                                    let eng = ws.active_engine(cx);
                                    if let DiceTransitionAction::StartCalculation {
                                        position_id,
                                        engine,
                                    } = ws.dice_state.on_manual_retry(pid, eng)
                                    {
                                        ws.job_controller.update(cx, |jc, cx| {
                                            jc.start_position_job(
                                                position_id,
                                                PositionJobKind::Dice,
                                                engine,
                                                None,
                                                cx,
                                            );
                                        });
                                    }
                                    cx.notify();
                                }
                            })
                        })
                    },
                    on_dice_recalculate_dialog: {
                        let e = ent.clone();
                        Rc::new(move |_, w, cx| {
                            e.update(cx, |ws, cx| {
                                if let Some(pid) = ws.active_checker_position_id() {
                                    let eng = ws.active_engine(cx);
                                    ws.job_controller.update(cx, |jc, cx| {
                                        jc.open_recalculate_dialog(
                                            pid,
                                            PositionJobKind::Dice,
                                            eng,
                                            w,
                                            cx,
                                        );
                                    });
                                }
                            })
                        })
                    },
                    on_dice_delete: {
                        let e = ent.clone();
                        Rc::new(move |pid, jid, _, cx| {
                            e.update(cx, |ws, cx| ws.delete_dice_job(pid, jid, cx))
                        })
                    },
                    on_toggle_observed_scope: {
                        let e = ent.clone();
                        Rc::new(move |game_only, _, cx| {
                            e.update(cx, |ws, cx| {
                                ws.observed_rolls_game_only = game_only;
                                if let Some(mid) = ws.match_id {
                                    ws.load_observed_rolls(mid, cx);
                                }
                                cx.notify();
                            })
                        })
                    },
                    on_retry_observed_rolls: {
                        let e = ent.clone();
                        Rc::new(move |_, _, cx| {
                            e.update(cx, |ws, cx| {
                                if let Some(mid) = ws.match_id {
                                    ws.load_observed_rolls(mid, cx);
                                }
                            })
                        })
                    },
                };

                render_checker_inspector(
                    CheckerViewProps {
                        active_subtab: self.checker_subtab,
                        position: current_pos,
                        position_result: self.position_result.as_ref(),
                        position_result_loading: self.position_result_loading,
                        previewed_notation: self.preview.candidate_notation.as_deref(),
                        active_engine: self.active_engine(cx),
                        opening_book: self.opening_book.as_ref(),
                        opening_book_expanded: self.opening_book_expanded,
                        opening_book_loading: self.opening_book_loading,
                        compare_slots: [
                            &self.compare_slots[0],
                            &self.compare_slots[1],
                            &self.compare_slots[2],
                        ],
                        compare_slot_ids: [
                            &self.compare_slot_ids[0],
                            &self.compare_slot_ids[1],
                            &self.compare_slot_ids[2],
                        ],
                        catalog_items: &self.catalog_items,
                        catalog_selected: self.catalog_selected.as_ref(),
                        compare_sort_delta: self.compare_sort_delta,
                        dice_state: &self.dice_state,
                        selected_dice_cell: self.selected_dice_cell,
                        observed_rolls: self.observed_rolls.as_ref(),
                        observed_rolls_game_only: self.observed_rolls_game_only,
                        observed_rolls_error: self.observed_rolls_error.as_deref(),
                        callbacks: checker_callbacks,
                        palette: p,
                    },
                    window,
                    cx,
                )
                .into_any_element()
            }

            AnalysisDetailTab::Cube => {
                let selected_cube_id = match &self.active_target {
                    Some(ActiveBoardTarget::Cube { review_id, .. }) => Some(review_id.as_str()),
                    _ => None,
                };
                let override_chances = selected_cube_id
                    .and_then(|sid| self.calculated_chances.get(sid))
                    .cloned();

                let cube_callbacks = CubeCallbacks {
                    on_select_decision: {
                        let e = ent.clone();
                        Rc::new(move |rid, dno, w, cx| {
                            e.update(cx, |ws, cx| ws.select_cube_decision(rid, dno, None, w, cx))
                        })
                    },
                    on_toggle_errors_only: {
                        let e = ent.clone();
                        Rc::new(move |_, w, cx| {
                            e.update(cx, |ws, cx| {
                                ws.cube_filter.errors_only = !ws.cube_filter.errors_only;
                                if let Some(mid) = ws.match_id {
                                    ws.load_cube_decisions(mid, w, cx);
                                }
                            })
                        })
                    },
                    on_cycle_player: {
                        let e = ent.clone();
                        Rc::new(move |_, w, cx| {
                            e.update(cx, |ws, cx| {
                                ws.cube_filter.player = match ws.cube_filter.player {
                                    None => Some(0),
                                    Some(0) => Some(1),
                                    _ => None,
                                };
                                if let Some(mid) = ws.match_id {
                                    ws.load_cube_decisions(mid, w, cx);
                                }
                            })
                        })
                    },
                    on_cycle_severity: {
                        let e = ent.clone();
                        Rc::new(move |_, w, cx| {
                            e.update(cx, |ws, cx| {
                                ws.cube_filter.severity = match ws.cube_filter.severity {
                                    None => Some(MoveSeverity::Doubtful),
                                    Some(MoveSeverity::Doubtful) => Some(MoveSeverity::Bad),
                                    Some(MoveSeverity::Bad) => Some(MoveSeverity::Blunder),
                                    Some(MoveSeverity::Blunder) => Some(MoveSeverity::VeryBad),
                                    _ => None,
                                };
                                if let Some(mid) = ws.match_id {
                                    ws.load_cube_decisions(mid, w, cx);
                                }
                            })
                        })
                    },
                    on_cycle_game: {
                        let e = ent.clone();
                        Rc::new(move |_, w, cx| {
                            e.update(cx, |ws, cx| {
                                let next = match ws.cube_filter.game_no {
                                    None => ws.games.first().map(|g| g.game_no),
                                    Some(cur) => ws
                                        .games
                                        .iter()
                                        .position(|g| g.game_no == cur)
                                        .and_then(|i| ws.games.get(i + 1))
                                        .map(|g| g.game_no),
                                };
                                ws.cube_filter.game_no = next;
                                if let Some(mid) = ws.match_id {
                                    ws.load_cube_decisions(mid, w, cx);
                                }
                            })
                        })
                    },
                    on_reset_filters: {
                        let e = ent.clone();
                        Rc::new(move |_, w, cx| {
                            e.update(cx, |ws, cx| {
                                ws.cube_filter.reset();
                                if let Some(mid) = ws.match_id {
                                    ws.load_cube_decisions(mid, w, cx);
                                }
                            })
                        })
                    },
                    on_calculate_chances: {
                        let e = ent.clone();
                        Rc::new(move |rid, _, cx| {
                            e.update(cx, |ws, cx| ws.calculate_cube_chances(rid, cx))
                        })
                    },
                };

                render_cube_inspector(
                    CubeViewProps {
                        decisions: &self.cube_decisions,
                        selected_id: selected_cube_id,
                        filter: &self.cube_filter,
                        games: &self.games,
                        players: players.clone(),
                        loading: self.cube_loading,
                        calculating_chances_for: self.calculating_chances_for.as_deref(),
                        override_chances,
                        callbacks: cube_callbacks,
                        palette: p,
                    },
                    cx,
                )
                .into_any_element()
            }

            AnalysisDetailTab::Statistics => {
                let stats_callbacks = StatsCallbacks {
                    on_open_game: {
                        let e = ent.clone();
                        Rc::new(move |gno, w, cx| {
                            e.update(cx, |ws, cx| {
                                ws.active_tab = AnalysisDetailTab::Overview;
                                ws.select_game(gno, None, None, w, cx);
                            })
                        })
                    },
                    on_open_drilldown: {
                        let e = ent.clone();
                        Rc::new(move |dd, _, cx| {
                            e.update(cx, |ws, cx| {
                                ws.stats_drilldown = dd;
                                cx.notify();
                            })
                        })
                    },
                    on_drill_checker: {
                        let e = ent.clone();
                        Rc::new(move |_player, sev, w, cx| {
                            e.update(cx, |ws, cx| {
                                ws.stats_drilldown = None;
                                ws.move_filter = match sev {
                                    MoveSeverity::Doubtful => MoveHistoryFilter::Doubtful,
                                    MoveSeverity::Bad => MoveHistoryFilter::Bad,
                                    MoveSeverity::Blunder => MoveHistoryFilter::Blunder,
                                    MoveSeverity::VeryBad => MoveHistoryFilter::VeryBad,
                                    MoveSeverity::None => MoveHistoryFilter::All,
                                };
                                ws.select_detail_tab(AnalysisDetailTab::Overview, w, cx);
                            })
                        })
                    },
                    on_drill_cube: {
                        let e = ent.clone();
                        Rc::new(move |player, sev, w, cx| {
                            e.update(cx, |ws, cx| {
                                ws.stats_drilldown = None;
                                ws.cube_filter.errors_only = true;
                                ws.cube_filter.player = Some(player);
                                ws.cube_filter.severity = Some(sev);
                                ws.select_detail_tab(AnalysisDetailTab::Cube, w, cx);
                            })
                        })
                    },
                    on_select_compare_a: {
                        let e = ent.clone();
                        Rc::new(move |id, _, cx| {
                            if let Some(aid) = id {
                                e.update(cx, |ws, cx| ws.load_compare_statistics(true, aid, cx));
                            }
                        })
                    },
                    on_select_compare_b: {
                        let e = ent.clone();
                        Rc::new(move |id, _, cx| {
                            if let Some(aid) = id {
                                e.update(cx, |ws, cx| ws.load_compare_statistics(false, aid, cx));
                            }
                        })
                    },
                };

                render_statistics_inspector(
                    StatsViewProps {
                        statistics: self.statistics.as_ref(),
                        compare_a_stats: self.compare_stats_a.as_ref(),
                        compare_b_stats: self.compare_stats_b.as_ref(),
                        compare_a_id: self.compare_stats_a_id.as_deref(),
                        compare_b_id: self.compare_stats_b_id.as_deref(),
                        catalog_items: &self.catalog_items,
                        players: players.clone(),
                        inspector_width_px: inspector_width,
                        loading: self.statistics_loading,
                        active_drilldown: self.stats_drilldown,
                        callbacks: stats_callbacks,
                        palette: p,
                    },
                    cx,
                )
                .into_any_element()
            }

            AnalysisDetailTab::Notes => {
                let target_key = self.current_note_target_key();
                let draft = target_key.and_then(|k| self.notes_drafts.read(cx).get(k));
                let notes_callbacks = NotesCallbacks {
                    on_select_scope: {
                        let e = ent.clone();
                        Rc::new(move |scope, w, cx| {
                            e.update(cx, |ws, cx| {
                                ws.notes_scope = scope;
                                ws.notes_validation_error = None;
                                ws.notes_saved_feedback = false;
                                ws.load_current_note(w, cx);
                                cx.notify();
                            })
                        })
                    },
                    on_add_tag: {
                        let e = ent.clone();
                        Rc::new(move |_, w, cx| {
                            e.update(cx, |ws, cx| {
                                let raw = ws.tag_input.read(cx).value().to_string();
                                if let Some(key) = ws.current_note_target_key() {
                                    let result =
                                        ws.notes_drafts.as_mut(cx).ensure_entry(key).add_tag(&raw);
                                    match result {
                                        Ok(()) => {
                                            ws.notes_validation_error = None;
                                            ws.tag_input.update(cx, |inp, cx| {
                                                inp.set_value("", w, cx);
                                            });
                                        }
                                        Err(err) => {
                                            ws.notes_validation_error = Some(err);
                                        }
                                    }
                                }
                                cx.notify();
                            })
                        })
                    },
                    on_remove_tag: {
                        let e = ent.clone();
                        Rc::new(move |tag, _, cx| {
                            e.update(cx, |ws, cx| {
                                if let Some(key) = ws.current_note_target_key() {
                                    ws.notes_drafts
                                        .as_mut(cx)
                                        .ensure_entry(key)
                                        .remove_tag(&tag);
                                }
                                cx.notify();
                            })
                        })
                    },
                    on_save_note: {
                        let e = ent.clone();
                        Rc::new(move |_, w, cx| e.update(cx, |ws, cx| ws.save_current_note(w, cx)))
                    },
                    on_load_latest_conflict: {
                        let e = ent.clone();
                        Rc::new(move |_, w, cx| {
                            e.update(cx, |ws, cx| ws.resolve_note_conflict(false, w, cx))
                        })
                    },
                    on_rebase_and_save_conflict: {
                        let e = ent.clone();
                        Rc::new(move |_, w, cx| {
                            e.update(cx, |ws, cx| ws.resolve_note_conflict(true, w, cx))
                        })
                    },
                    on_open_move_notes_dialog: {
                        let e = ent.clone();
                        Rc::new(move |_, w, cx| {
                            e.update(cx, |ws, cx| ws.open_move_notes_for(None, w, cx))
                        })
                    },
                };

                render_notes_inspector(
                    NotesViewProps {
                        scope: self.notes_scope,
                        target_key,
                        draft,
                        saving: self.notes_saving,
                        saved_feedback: self.notes_saved_feedback,
                        validation_error: self.notes_validation_error.as_deref(),
                        tag_input: &self.tag_input,
                        comment_input: &self.comment_input,
                        import_warnings: &self.import_warnings,
                        has_game_target: self.current_game_id.is_some(),
                        has_position_target: self.active_checker_position_id().is_some(),
                        source_available,
                        callbacks: notes_callbacks,
                        palette: p,
                    },
                    cx,
                )
                .into_any_element()
            }

            AnalysisDetailTab::Cards => {
                let cards_callbacks = CardsCallbacks {
                    on_select_deck: {
                        let e = ent.clone();
                        Rc::new(move |did, _, cx| {
                            e.update(cx, |ws, cx| {
                                ws.selected_deck_id = Some(did);
                                ws.refresh_card_suggestions(cx);
                                cx.notify();
                            })
                        })
                    },
                    on_create_deck: {
                        let e = ent.clone();
                        Rc::new(move |_, w, cx| e.update(cx, |ws, cx| ws.create_new_deck(w, cx)))
                    },
                    on_add_current_position: {
                        let e = ent.clone();
                        Rc::new(move |_, _, cx| {
                            e.update(cx, |ws, cx| ws.add_cards_to_selected_deck(true, cx))
                        })
                    },
                    on_add_match_errors: {
                        let e = ent.clone();
                        Rc::new(move |_, _, cx| {
                            e.update(cx, |ws, cx| ws.add_cards_to_selected_deck(false, cx))
                        })
                    },
                    on_refresh_suggestions: {
                        let e = ent.clone();
                        Rc::new(move |_, _, cx| {
                            e.update(cx, |ws, cx| ws.refresh_card_suggestions(cx))
                        })
                    },
                    on_toggle_suggestion: {
                        let e = ent.clone();
                        Rc::new(move |r, _, cx| {
                            e.update(cx, |ws, cx| {
                                if !ws.selected_suggestions.remove(&r) {
                                    ws.selected_suggestions.insert(r);
                                }
                                cx.notify();
                            })
                        })
                    },
                    on_save_selected_suggestions: {
                        let e = ent.clone();
                        Rc::new(move |_, _, cx| {
                            e.update(cx, |ws, cx| ws.save_selected_card_suggestions(cx))
                        })
                    },
                };

                render_cards_inspector(
                    CardsViewProps {
                        decks: &self.decks,
                        selected_deck_id: self.selected_deck_id,
                        new_deck_input: &self.new_deck_input,
                        has_position: self.active_checker_position_id().is_some(),
                        source_available,
                        busy: self.cards_busy,
                        status_message: self.cards_status.as_deref(),
                        suggestions: self.card_suggestions.as_ref(),
                        suggestions_loading: self.card_suggestions_loading,
                        selected_suggestions: &self.selected_suggestions,
                        suggestion_boards: &self.suggestion_boards,
                        callbacks: cards_callbacks,
                        palette: p,
                    },
                    cx,
                )
                .into_any_element()
            }
        };

        let right_pane = div()
            .id("analysis-inspector-pane")
            .v_flex()
            .flex_1()
            .min_w(px(300.0))
            .h_full()
            .p(px(10.0))
            .bg(p.bg)
            .overflow_y_scroll()
            .test_support()
            .child(inspector_body);

        // Assemble full Analysis workspace with keyboard & pointer drag listeners
        div()
            .id("analysis-workspace")
            .test_support()
            .key_context(key_ctx)
            .track_focus(&self.focus_handle)
            .v_flex()
            .flex_1()
            .size_full()
            .bg(p.bg)
            .on_action(cx.listener(Self::on_prev_position))
            .on_action(cx.listener(Self::on_next_position))
            .on_action(cx.listener(Self::on_next_error))
            .on_action(cx.listener(Self::on_prev_error))
            .on_action(cx.listener(Self::on_show_initial_board))
            .on_action(cx.listener(Self::on_show_final_board))
            .on_action(cx.listener(Self::on_copy_position_id))
            .on_action(cx.listener(Self::on_toggle_dice_equity))
            .on_action(cx.listener(Self::on_analyze_game))
            .on_action(cx.listener(Self::on_analyze_match))
            .on_action(cx.listener(Self::on_reset_pane_widths))
            .on_key_down(cx.listener(|ws, ev: &KeyDownEvent, window, cx| {
                match ev.keystroke.key.as_str() {
                    "up" => {
                        if ws.active_tab == AnalysisDetailTab::Overview
                            && ws.checker_subtab == CheckerSubTab::Candidates
                            && ws.position_result.is_some()
                            && ev.keystroke.modifiers.alt
                        {
                            ws.step_candidate_preview(-1, cx);
                        } else {
                            ws.step_move(-1, window, cx);
                        }
                    }
                    "down" => {
                        if ws.active_tab == AnalysisDetailTab::Overview
                            && ws.checker_subtab == CheckerSubTab::Candidates
                            && ws.position_result.is_some()
                            && ev.keystroke.modifiers.alt
                        {
                            ws.step_candidate_preview(1, cx);
                        } else {
                            ws.step_move(1, window, cx);
                        }
                    }
                    _ => {}
                }
            }))
            .on_mouse_move(cx.listener(|ws, ev: &MouseMoveEvent, _window, cx| {
                if ws.dragging_splitter {
                    let dx = f32::from(ev.position.x) - ws.drag_start_x;
                    let target = (ws.drag_start_width + dx).round() as i32;
                    ws.board_widths.set_width(ws.active_tab, target);
                    ws.persist_board_widths(cx);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|ws, _ev: &MouseUpEvent, _window, cx| {
                    if ws.dragging_splitter {
                        ws.dragging_splitter = false;
                        ws.board_widths.commit_edit();
                        ws.persist_board_widths(cx);
                        cx.notify();
                    }
                }),
            )
            .child(header_el)
            .child(
                div()
                    .id("analysis-three-pane-body")
                    .test_support()
                    .h_flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .child(left_pane)
                    .child(center_pane)
                    .child(splitter)
                    .child(right_pane),
            )
            .into_any_element()
    }
}

fn make_notice(id: &'static str, kind: BannerKind, message: impl Into<String>) -> AppNotice {
    AppNotice {
        id: SharedString::from(id),
        kind,
        title: None,
        message: message.into().into(),
        dismissible: true,
    }
}
