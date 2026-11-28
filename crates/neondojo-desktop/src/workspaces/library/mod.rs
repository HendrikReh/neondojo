//! Complete native GPUI Match Library workspace (`MatchDatabase.tsx` parity).
//!
//! Includes:
//! - [`sidebar`]: Database sidebar (`All databases`, user DBs, 4 protected system DBs, CRUD / Empty)
//! - [`search_panels`]: 4 mutually-exclusive accordion search panels (Filters + `AppliedFilterChips`,
//!   Notes search, Position ID search, cursor-paged Duplicate Match Finder)
//! - [`table`]: Dense Library table (15 toggleable / reorderable / resizable columns persisted in
//!   `LocalUiState::library` + 3 dynamic Position-ID occurrence columns, PR warnings, hidden-sort badge,
//!   numbered pagination, paused/unfinished-Competition routing to Play)
//! - [`dialogs`]: Database CRUD, match metadata editor, batch Copy/Move/Tags/Delete/Export, and
//!   Bulk Match Analysis dialogs
//! - [`import_panel`]: Native macOS file & recursive folder pickers, Finder `MatchDropZone`,
//!   cooperative cancel, and `ImportWarningList`

mod dialogs;
mod import_panel;
mod job_watch;
mod search_panels;
mod sidebar;
pub mod state;
mod table;

#[cfg(test)]
mod tests;

use crate::app_context::DesktopAppContext;
use crate::events::WorkspaceEvent;
use crate::i18n::{current_locale, format_domain_error};
use crate::local_state::LocalUiState;
use crate::theme::NeonTheme;
use crate::tokio_bridge::{ReadTask, TokioBridge};
use crate::ui::banners::BannerKind;
use crate::ui::drop_zone::{DroppedMatchBatch, collect_dropped_match_files};
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement, ParentElement,
    PathPromptOptions, Render, SharedString, Styled, Subscription, TestSupportExt, Window,
    base::StyledExt,
    component::input::{InputEvent, InputState},
    div, px,
};
use neondojo::annotations::{AnnotationHit, AnnotationKind};
use neondojo::databases::{ActionResponse, Database, DatabaseMatch};
use neondojo::duplicate_matches::DuplicateMatchQuery;
use neondojo::facade::BackendFacade;
use neondojo::model::Job;
use neondojo_i18n::{I18n, Locale};
use state::{
    BulkAnalysisTracker, ColumnLayoutState, DUPLICATES_PAGE_SIZE, DuplicateFinderState,
    ImportFileRow, ImportFileStatus, ImportRunState, LibraryFilterState, LibrarySelectionState,
    NotesSearchState, OccurrenceColumn, SearchPanelKind, SortDirection, accepts_library_matches,
    can_analyze_match, clone_database_match, database_display_name, is_valid_position_id,
    row_open_event, toggle_search_panel,
};
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, atomic::Ordering};
use uuid::Uuid;

/// Executes a `BackendFacade` async call on Tokio's blocking pool via `handle.block_on`,
/// insulating cross-crate SQLx futures from Rust HRTB `Send` errors.
pub(super) async fn run_facade_async<F, Fut, R>(f: F) -> R
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = R>,
    R: Send + 'static,
{
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || handle.block_on(f()))
        .await
        .expect("facade blocking task failed")
}

/// Persistent `InputState` entities backing the search bar, filters, notes, and position search.
pub(super) struct LibraryInputs {
    pub q: Entity<InputState>,
    pub player: Entity<InputState>,
    pub match_name: Entity<InputState>,
    pub tournament: Entity<InputState>,
    pub round: Entity<InputState>,
    pub date_from: Entity<InputState>,
    pub date_to: Entity<InputState>,
    pub tag: Entity<InputState>,
    pub comment: Entity<InputState>,
    pub match_points_min: Entity<InputState>,
    pub match_points_max: Entity<InputState>,
    pub position_id: Entity<InputState>,
    pub notes_query: Entity<InputState>,
    pub notes_tag: Entity<InputState>,
}

/// Complete native GPUI Match Library workspace entity.
pub struct LibraryWorkspace {
    pub(super) visible: bool,
    pub(super) facade: Option<BackendFacade>,

    // Database sidebar & table data
    pub(super) databases: Vec<Database>,
    pub(super) selected_database_id: Option<Uuid>,
    pub(super) import_database_id: Option<Uuid>,
    pub(super) matches: Vec<DatabaseMatch>,
    pub(super) total_matches: Option<i64>,
    pub(super) page: usize,
    pub(super) sort: String,
    pub(super) direction: SortDirection,
    pub(super) loading_matches: bool,
    pub(super) load_error: Option<SharedString>,

    // Column preferences & picker popover
    pub(super) columns: ColumnLayoutState,
    pub(super) show_column_picker: bool,

    // Search panels (mutually exclusive accordion)
    pub(super) active_search_panel: Option<SearchPanelKind>,
    pub(super) filters: LibraryFilterState,
    pub(super) position_draft: String,
    pub(super) position_error: Option<SharedString>,
    pub(super) notes_search: NotesSearchState,
    pub(super) duplicates: DuplicateFinderState,
    pub(super) inputs: LibraryInputs,

    // Cross-page selection & batch notices
    pub(super) selection: LibrarySelectionState,
    pub(super) last_batch_banner: Option<(BannerKind, SharedString)>,

    // Live bulk analysis tracker
    pub(super) bulk_tracker: BulkAnalysisTracker,

    // Native import & Finder drop zone
    pub(super) show_drop_zone: bool,
    pub(super) import_run: Option<ImportRunState>,

    // Deferred window-scoped actions requested from window-less public hooks
    pending_position_input_sync: Option<String>,
    pending_bulk_analysis_ids: Option<Vec<Uuid>>,

    // Monotonic epoch counters & abortable read tasks
    db_epoch: u64,
    matches_epoch: u64,
    pending_job_refresh: bool,
    notes_epoch: u64,
    duplicates_epoch: u64,
    _databases_task: Option<ReadTask<()>>,
    _matches_task: Option<ReadTask<()>>,
    _notes_task: Option<ReadTask<()>>,
    _duplicates_task: Option<ReadTask<()>>,
    job_watch: Option<job_watch::LibraryJobWatch>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<WorkspaceEvent> for LibraryWorkspace {}

impl LibraryWorkspace {
    /// Creates a new [`LibraryWorkspace`], restoring persisted column & database
    /// preferences from [`LocalUiState`] and initiating the initial backend load when connected.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = current_locale(cx);
        let local_state = cx.try_global::<LocalUiState>().cloned().unwrap_or_default();
        let facade = cx
            .try_global::<DesktopAppContext>()
            .and_then(DesktopAppContext::facade);

        let selected_database_id = Uuid::parse_str(local_state.library.database_id.trim()).ok();
        let columns = ColumnLayoutState::from_prefs(&local_state.library);
        let bulk_tracker = BulkAnalysisTracker::from_prefs(&local_state.library);

        let inputs = LibraryInputs {
            q: cx.new(|cx| {
                InputState::new(window, cx).placeholder(I18n::t(
                    locale,
                    "app",
                    "app.searchPlaceholder",
                ))
            }),
            player: cx.new(|cx| InputState::new(window, cx)),
            match_name: cx.new(|cx| InputState::new(window, cx)),
            tournament: cx.new(|cx| InputState::new(window, cx)),
            round: cx.new(|cx| InputState::new(window, cx)),
            date_from: cx.new(|cx| InputState::new(window, cx).placeholder("YYYY-MM-DD")),
            date_to: cx.new(|cx| InputState::new(window, cx).placeholder("YYYY-MM-DD")),
            tag: cx.new(|cx| InputState::new(window, cx)),
            comment: cx.new(|cx| InputState::new(window, cx)),
            match_points_min: cx.new(|cx| InputState::new(window, cx)),
            match_points_max: cx.new(|cx| InputState::new(window, cx)),
            position_id: cx.new(|cx| {
                InputState::new(window, cx).placeholder(I18n::t(
                    locale,
                    "app",
                    "app.positionPlaceholder",
                ))
            }),
            notes_query: cx.new(|cx| {
                InputState::new(window, cx).placeholder(I18n::t(
                    locale,
                    "annotation",
                    "queryPlaceholder",
                ))
            }),
            notes_tag: cx.new(|cx| {
                InputState::new(window, cx).placeholder(I18n::t(
                    locale,
                    "annotation",
                    "tagPlaceholder",
                ))
            }),
        };

        let mut subscriptions = Vec::new();
        Self::subscribe_filter_input(
            &mut subscriptions,
            &inputs.q,
            window,
            cx,
            |this, val| this.filters.q = val,
            true,
        );
        Self::subscribe_filter_input(
            &mut subscriptions,
            &inputs.player,
            window,
            cx,
            |this, val| this.filters.player = val,
            true,
        );
        Self::subscribe_filter_input(
            &mut subscriptions,
            &inputs.match_name,
            window,
            cx,
            |this, val| this.filters.match_name = val,
            true,
        );
        Self::subscribe_filter_input(
            &mut subscriptions,
            &inputs.tournament,
            window,
            cx,
            |this, val| this.filters.tournament = val,
            true,
        );
        Self::subscribe_filter_input(
            &mut subscriptions,
            &inputs.round,
            window,
            cx,
            |this, val| this.filters.round = val,
            true,
        );
        Self::subscribe_filter_input(
            &mut subscriptions,
            &inputs.date_from,
            window,
            cx,
            |this, val| this.filters.date_from = val,
            true,
        );
        Self::subscribe_filter_input(
            &mut subscriptions,
            &inputs.date_to,
            window,
            cx,
            |this, val| this.filters.date_to = val,
            true,
        );
        Self::subscribe_filter_input(
            &mut subscriptions,
            &inputs.tag,
            window,
            cx,
            |this, val| this.filters.tag = val,
            true,
        );
        Self::subscribe_filter_input(
            &mut subscriptions,
            &inputs.comment,
            window,
            cx,
            |this, val| this.filters.comment = val,
            true,
        );
        Self::subscribe_filter_input(
            &mut subscriptions,
            &inputs.match_points_min,
            window,
            cx,
            |this, val| this.filters.match_points_min = val,
            true,
        );
        Self::subscribe_filter_input(
            &mut subscriptions,
            &inputs.match_points_max,
            window,
            cx,
            |this, val| this.filters.match_points_max = val,
            true,
        );

        subscriptions.push(cx.subscribe_in(
            &inputs.position_id,
            window,
            |this, state, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    this.position_draft = state.read(cx).value().to_string();
                    this.position_error = None;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    this.submit_position_search_input(cx);
                }
                _ => {}
            },
        ));

        subscriptions.push(cx.subscribe_in(
            &inputs.notes_query,
            window,
            |this, state, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    this.notes_search.query = state.read(cx).value().to_string();
                    this.notes_search.error = None;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    this.run_notes_search(1, cx);
                }
                _ => {}
            },
        ));

        subscriptions.push(cx.subscribe_in(
            &inputs.notes_tag,
            window,
            |this, state, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    this.notes_search.exact_tag = state.read(cx).value().to_string();
                    this.notes_search.error = None;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    this.run_notes_search(1, cx);
                }
                _ => {}
            },
        ));

        let mut workspace = Self {
            visible: true,
            facade,
            databases: Vec::new(),
            selected_database_id,
            import_database_id: None,
            matches: Vec::new(),
            total_matches: None,
            page: 1,
            sort: "created_at".to_string(),
            direction: SortDirection::Desc,
            loading_matches: false,
            load_error: None,
            columns,
            show_column_picker: false,
            active_search_panel: None,
            filters: LibraryFilterState::default(),
            position_draft: String::new(),
            position_error: None,
            notes_search: NotesSearchState::new(),
            duplicates: DuplicateFinderState::default(),
            inputs,
            selection: LibrarySelectionState::default(),
            last_batch_banner: None,
            bulk_tracker,
            show_drop_zone: false,
            import_run: None,
            pending_position_input_sync: None,
            pending_bulk_analysis_ids: None,
            db_epoch: 0,
            matches_epoch: 0,
            pending_job_refresh: false,
            notes_epoch: 0,
            duplicates_epoch: 0,
            _databases_task: None,
            _matches_task: None,
            _notes_task: None,
            _duplicates_task: None,
            job_watch: None,
            _subscriptions: subscriptions,
        };

        if workspace.facade.is_some() {
            workspace.reload_all(cx);
            workspace.sync_job_watcher(cx);
        }

        workspace
    }

    fn subscribe_filter_input<F>(
        subs: &mut Vec<Subscription>,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
        mut setter: F,
        submit_on_enter: bool,
    ) where
        F: FnMut(&mut Self, String) + 'static,
    {
        subs.push(cx.subscribe_in(
            input,
            window,
            move |this, state, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    setter(this, state.read(cx).value().to_string());
                    cx.notify();
                }
                InputEvent::PressEnter { .. } if submit_on_enter => {
                    this.apply_current_filters(cx);
                }
                _ => {}
            },
        ));
    }

    // ── Required Public Cross-Workspace Hooks ────────────────────────────────

    /// Called by `AppShell` when the Match Library tab is shown or hidden.
    /// Pauses live job watchers while hidden and refreshes when shown again.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        self.refresh_facade_from_global(cx);
        if visible {
            if self.facade.is_some() {
                self.reload_all(cx);
                self.sync_job_watcher(cx);
            }
        } else {
            self.job_watch = None;
        }
        cx.notify();
    }

    /// Applies an exact 14-character GNUbg Position ID search (for example triggered
    /// from Analysis via `WorkspaceEvent::SearchPositionInLibrary`).
    pub fn apply_position_search(&mut self, position_id: String, cx: &mut Context<Self>) {
        let trimmed = position_id.trim().to_string();
        self.active_search_panel = Some(SearchPanelKind::Position);
        self.position_draft = trimmed.clone();
        self.pending_position_input_sync = Some(trimmed.clone());

        if trimmed.is_empty() {
            self.position_error = None;
            self.filters.position_id.clear();
            self.page = 1;
            self.load_matches(cx);
            cx.notify();
            return;
        }

        if !is_valid_position_id(&trimmed) {
            let locale = current_locale(cx);
            self.position_error = Some(SharedString::from(I18n::t(
                locale,
                "library",
                "invalidPositionId",
            )));
            cx.notify();
            return;
        }

        self.position_error = None;
        self.filters.position_id = trimmed;
        self.page = 1;
        self.load_matches(cx);
        cx.notify();
    }

    /// Queues a Bulk Match Analysis dialog for the supplied match IDs (for example
    /// triggered from Player Directory via `WorkspaceEvent::StartBulkAnalysis`).
    pub fn start_bulk_analysis(&mut self, match_ids: Vec<Uuid>, cx: &mut Context<Self>) {
        if match_ids.is_empty() {
            return;
        }
        self.refresh_facade_from_global(cx);
        self.pending_bulk_analysis_ids = Some(match_ids);
        cx.notify();
    }

    /// Imports files and/or recursive folders (for example from Finder drop or menu actions).
    pub fn import_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }
        let batch = collect_dropped_match_files(&paths);
        self.start_import_batch(batch, cx);
    }

    /// Refreshes databases, match table, and active duplicate results after an external mutation.
    pub fn refresh_on_library_mutated(&mut self, cx: &mut Context<Self>) {
        self.refresh_facade_from_global(cx);
        if self.facade.is_some() {
            self.reload_all(cx);
            if self.active_search_panel == Some(SearchPanelKind::Duplicates)
                && self.duplicates.searched
            {
                self.load_duplicates(false, cx);
            }
        }
    }

    // ── Internal State & Navigation Helpers ──────────────────────────────────

    fn refresh_facade_from_global(&mut self, cx: &App) {
        if self.facade.is_none() {
            self.facade = cx
                .try_global::<DesktopAppContext>()
                .and_then(DesktopAppContext::facade);
        }
    }

    pub(super) fn effective_import_database_id(&self) -> Option<Uuid> {
        if let Some(id) = self.import_database_id
            && accepts_library_matches(id)
            && self.databases.iter().any(|d| d.id == id)
        {
            return Some(id);
        }
        if let Some(id) = self.selected_database_id
            && accepts_library_matches(id)
            && self.databases.iter().any(|d| d.id == id)
        {
            return Some(id);
        }
        self.databases
            .iter()
            .find(|d| d.is_default && accepts_library_matches(d.id))
            .or_else(|| {
                self.databases
                    .iter()
                    .find(|d| accepts_library_matches(d.id))
            })
            .map(|d| d.id)
    }

    pub(super) fn persist_library_prefs(&self, cx: &mut Context<Self>) {
        let mut state = cx.try_global::<LocalUiState>().cloned().unwrap_or_default();
        state.library.database_id = self
            .selected_database_id
            .map(|id| id.to_string())
            .unwrap_or_default();
        self.columns.write_to_prefs(&mut state.library);
        self.bulk_tracker.write_to_prefs(&mut state.library);
        cx.set_global(state.clone());

        #[cfg(not(test))]
        {
            let path = LocalUiState::default_path();
            if let Err(err) = state.save_atomic(&path) {
                tracing::warn!("failed to save local library UI preferences: {err:#}");
            }
        }
    }

    pub(super) fn select_database(&mut self, db_id: Option<Uuid>, cx: &mut Context<Self>) {
        if self.selected_database_id == db_id {
            return;
        }
        self.selected_database_id = db_id;
        self.page = 1;
        self.selection.clear();
        self.duplicates.clear();
        self.persist_library_prefs(cx);
        self.load_matches(cx);
        if self.active_search_panel == Some(SearchPanelKind::Duplicates) {
            self.load_duplicates(false, cx);
        }
        cx.notify();
    }

    pub(super) fn toggle_panel(&mut self, panel: SearchPanelKind, cx: &mut Context<Self>) {
        toggle_search_panel(&mut self.active_search_panel, panel);
        if self.active_search_panel == Some(SearchPanelKind::Duplicates)
            && !self.duplicates.searched
            && !self.duplicates.loading
            && self.facade.is_some()
        {
            self.load_duplicates(false, cx);
        }
        cx.notify();
    }

    pub(super) fn apply_current_filters(&mut self, cx: &mut Context<Self>) {
        self.page = 1;
        self.load_matches(cx);
        cx.notify();
    }

    pub(super) fn submit_position_search_input(&mut self, cx: &mut Context<Self>) {
        let draft = self.position_draft.clone();
        self.apply_position_search(draft, cx);
    }

    pub(super) fn remove_filter_chip(
        &mut self,
        chip_id: &str,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if !self.filters.remove_filter(chip_id) {
            return;
        }
        if chip_id == "position_id" {
            self.position_draft.clear();
            self.position_error = None;
            if OccurrenceColumn::ALL
                .iter()
                .any(|c| c.sort_key() == self.sort)
            {
                self.sort = "created_at".to_string();
                self.direction = SortDirection::Desc;
            }
        }
        if let Some(window) = window {
            self.sync_filter_inputs_to_window(window, cx);
        } else if chip_id == "position_id" {
            self.pending_position_input_sync = Some(String::new());
        }
        self.page = 1;
        self.load_matches(cx);
        cx.notify();
    }

    pub(super) fn clear_all_filters(
        &mut self,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        self.filters.clear();
        self.position_draft.clear();
        self.position_error = None;
        if OccurrenceColumn::ALL
            .iter()
            .any(|c| c.sort_key() == self.sort)
        {
            self.sort = "created_at".to_string();
            self.direction = SortDirection::Desc;
        }
        if let Some(window) = window {
            self.sync_filter_inputs_to_window(window, cx);
        } else {
            self.pending_position_input_sync = Some(String::new());
        }
        self.page = 1;
        self.load_matches(cx);
        cx.notify();
    }

    fn sync_filter_inputs_to_window(&self, window: &mut Window, cx: &mut Context<Self>) {
        let pairs: [(&Entity<InputState>, &str); 12] = [
            (&self.inputs.q, &self.filters.q),
            (&self.inputs.player, &self.filters.player),
            (&self.inputs.match_name, &self.filters.match_name),
            (&self.inputs.tournament, &self.filters.tournament),
            (&self.inputs.round, &self.filters.round),
            (&self.inputs.date_from, &self.filters.date_from),
            (&self.inputs.date_to, &self.filters.date_to),
            (&self.inputs.tag, &self.filters.tag),
            (&self.inputs.comment, &self.filters.comment),
            (
                &self.inputs.match_points_min,
                &self.filters.match_points_min,
            ),
            (
                &self.inputs.match_points_max,
                &self.filters.match_points_max,
            ),
            (&self.inputs.position_id, &self.position_draft),
        ];
        for (input, val) in pairs {
            let owned = val.to_string();
            input.update(cx, |state, cx| {
                if state.value().as_ref() != owned {
                    state.set_value(owned, window, cx);
                }
            });
        }
    }

    pub(super) fn toggle_sort(&mut self, sort_key: &str, cx: &mut Context<Self>) {
        if self.sort == sort_key {
            self.direction = self.direction.toggle();
        } else {
            self.sort = sort_key.to_string();
            self.direction = SortDirection::Desc;
        }
        self.page = 1;
        self.load_matches(cx);
        cx.notify();
    }

    pub(super) fn go_to_page(&mut self, target_page: usize, cx: &mut Context<Self>) {
        let next = target_page.max(1);
        if self.page == next {
            return;
        }
        self.page = next;
        self.load_matches(cx);
        cx.notify();
    }

    pub(super) fn open_library_match(&mut self, m: &DatabaseMatch, cx: &mut Context<Self>) {
        cx.emit(row_open_event(m));
    }

    pub(super) fn emit_resume_play(&mut self, session_id: Uuid, cx: &mut Context<Self>) {
        cx.emit(WorkspaceEvent::OpenPlaySession {
            session_id,
            resume: true,
        });
    }

    pub(super) fn select_duplicate_copies(&mut self, cx: &mut Context<Self>) {
        self.selection
            .select_duplicate_copies(&self.duplicates.groups);
        cx.notify();
    }

    pub(super) fn apply_batch_action_response(
        &mut self,
        response: &ActionResponse,
        cx: &mut Context<Self>,
    ) {
        let locale = current_locale(cx);
        let summary = self.selection.retain_failed(response);
        let msg = summary.format_message(locale);
        let kind = if summary.has_failures() {
            BannerKind::Warning
        } else {
            BannerKind::Success
        };
        if !msg.is_empty() {
            self.last_batch_banner = Some((kind, SharedString::from(msg)));
        }
        self.on_library_mutation_completed(cx);
    }

    pub(super) fn on_library_mutation_completed(&mut self, cx: &mut Context<Self>) {
        self.reload_all(cx);
        if self.active_search_panel == Some(SearchPanelKind::Duplicates) && self.duplicates.searched
        {
            self.load_duplicates(false, cx);
        }
        cx.emit(WorkspaceEvent::LibraryMutated);
        cx.notify();
    }

    pub(super) fn track_bulk_analysis_jobs(
        &mut self,
        jobs: &[(Job, Uuid)],
        continues_batch: bool,
        cx: &mut Context<Self>,
    ) {
        self.bulk_tracker.track_jobs(jobs, continues_batch);
        self.persist_library_prefs(cx);
        self.load_matches(cx);
        self.sync_job_watcher(cx);
        cx.notify();
    }

    // ── Backend Async Queries (TokioBridge + Epoch Guards) ───────────────────

    pub(super) fn reload_all(&mut self, cx: &mut Context<Self>) {
        self.load_databases(cx);
        self.load_matches(cx);
    }

    fn load_databases(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        self.db_epoch = self.db_epoch.wrapping_add(1);
        let epoch = self.db_epoch;
        let locale = current_locale(cx);

        let read = TokioBridge::spawn_read(cx, async move {
            run_facade_async(move || async move { facade.list_databases().await }).await
        });

        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let wrapper = TokioBridge::spawn_read(cx, async move {
            let _ = done_tx.send(());
        });
        drop(done_rx);
        self._databases_task = Some(wrapper);

        cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |ws, cx| {
                if ws.db_epoch != epoch {
                    return;
                }
                match result {
                    Ok(Ok(dbs)) => {
                        if let Some(sel) = ws.selected_database_id
                            && !dbs.iter().any(|d| d.id == sel)
                        {
                            ws.selected_database_id = None;
                            ws.persist_library_prefs(cx);
                        }
                        ws.databases = dbs;
                        cx.notify();
                    }
                    Ok(Err(err)) => {
                        ws.load_error = Some(SharedString::from(format_domain_error(locale, &err)));
                        cx.notify();
                    }
                    Err(_) => {}
                }
            });
        })
        .detach();
    }

    fn load_matches(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            self.loading_matches = false;
            return;
        };
        self.matches_epoch = self.matches_epoch.wrapping_add(1);
        let epoch = self.matches_epoch;
        let query = self.filters.to_library_query(
            self.selected_database_id,
            &self.sort,
            self.direction,
            self.page,
        );
        let locale = current_locale(cx);
        self.pending_job_refresh = false;
        self.loading_matches = true;
        self.load_error = None;
        cx.notify();

        let (tx, rx) = tokio::sync::oneshot::channel();
        let read_task = TokioBridge::spawn_read(cx, async move {
            let res =
                run_facade_async(move || async move { facade.search_matches(query).await }).await;
            let _ = tx.send(res);
        });
        self._matches_task = Some(read_task);

        cx.spawn(async move |this, cx| {
            let Ok(result) = rx.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                if ws.matches_epoch != epoch {
                    return;
                }
                ws.loading_matches = false;
                match result {
                    Ok(page) => {
                        ws.selection.sync_visible_page(&page.items);
                        ws.matches = page.items;
                        ws.total_matches = page.total;
                        ws.load_error = None;
                        ws.sync_job_watcher(cx);
                    }
                    Err(err) => {
                        ws.load_error = Some(SharedString::from(format_domain_error(locale, &err)));
                    }
                }
                if ws.visible && ws.pending_job_refresh {
                    ws.load_matches(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn search_notes_by_exact_tag(
        &mut self,
        tag: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.notes_search.exact_tag = tag.clone();
        self.inputs.notes_tag.update(cx, |state, cx| {
            state.set_value(tag, window, cx);
        });
        self.run_notes_search(1, cx);
    }

    pub(super) fn run_notes_search(&mut self, page: usize, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        if let Err(err) = self.notes_search.validate(locale) {
            self.notes_search.error = Some(err);
            cx.notify();
            return;
        }
        let Some(facade) = self.facade.clone() else {
            return;
        };
        self.notes_epoch = self.notes_epoch.wrapping_add(1);
        let epoch = self.notes_epoch;
        self.notes_search.page = page.max(1);
        self.notes_search.loading = true;
        self.notes_search.error = None;
        let query = self.notes_search.to_query(self.selected_database_id);
        cx.notify();

        let (tx, rx) = tokio::sync::oneshot::channel();
        let read_task = TokioBridge::spawn_read(cx, async move {
            let res =
                run_facade_async(move || async move { facade.search_annotations(query).await })
                    .await;
            let _ = tx.send(res);
        });
        self._notes_task = Some(read_task);

        cx.spawn(async move |this, cx| {
            let Ok(result) = rx.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                if ws.notes_epoch != epoch {
                    return;
                }
                ws.notes_search.loading = false;
                ws.notes_search.searched = true;
                match result {
                    Ok(page) => {
                        ws.notes_search.hits = page.items;
                        ws.notes_search.total = page.total;
                        ws.notes_search.error = None;
                    }
                    Err(err) => {
                        ws.notes_search.error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_annotation_hit(&mut self, hit: AnnotationHit, cx: &mut Context<Self>) {
        match hit.target_kind {
            AnnotationKind::Match | AnnotationKind::Game => {
                cx.emit(WorkspaceEvent::OpenMatchInAnalysis {
                    match_id: hit.match_uuid,
                    game_no: hit.game_no,
                    position_id: None,
                    decision_no: None,
                });
            }
            AnnotationKind::Position => {
                cx.emit(WorkspaceEvent::OpenMatchInAnalysis {
                    match_id: hit.match_uuid,
                    game_no: hit.game_no,
                    position_id: Some(hit.target_id),
                    decision_no: None,
                });
            }
            AnnotationKind::Move => {
                let Some(facade) = self.facade.clone() else {
                    cx.emit(WorkspaceEvent::OpenMatchInAnalysis {
                        match_id: hit.match_uuid,
                        game_no: hit.game_no,
                        position_id: None,
                        decision_no: None,
                    });
                    return;
                };
                let move_id = hit.target_id;
                let fallback_match = hit.match_uuid;
                let fallback_game = hit.game_no;
                let task = TokioBridge::spawn_read(cx, async move {
                    run_facade_async(move || async move { facade.get_move(move_id).await }).await
                });
                cx.spawn(async move |this, cx| {
                    let res = task.await;
                    let _ = this.update(cx, |_, cx| match res {
                        Ok(Ok(mv)) => {
                            cx.emit(WorkspaceEvent::OpenMatchInAnalysis {
                                match_id: mv.match_uuid,
                                game_no: Some(mv.game_no),
                                position_id: mv.position_uuid,
                                decision_no: Some(mv.move_no),
                            });
                        }
                        _ => {
                            cx.emit(WorkspaceEvent::OpenMatchInAnalysis {
                                match_id: fallback_match,
                                game_no: fallback_game,
                                position_id: None,
                                decision_no: None,
                            });
                        }
                    });
                })
                .detach();
            }
        }
    }

    pub(super) fn load_duplicates(&mut self, append: bool, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let after = if append {
            match self.duplicates.next_cursor.clone() {
                Some(cursor) => Some(cursor),
                None => return,
            }
        } else {
            None
        };

        self.duplicates_epoch = self.duplicates_epoch.wrapping_add(1);
        let epoch = self.duplicates_epoch;
        let locale = current_locale(cx);
        if append {
            self.duplicates.loading_more = true;
        } else {
            self.duplicates.loading = true;
            self.duplicates.error = None;
        }
        let query = DuplicateMatchQuery {
            database_id: self.selected_database_id,
            after,
            limit: Some(DUPLICATES_PAGE_SIZE as i64),
        };
        cx.notify();

        let (tx, rx) = tokio::sync::oneshot::channel();
        let read_task = TokioBridge::spawn_read(cx, async move {
            let res =
                run_facade_async(move || async move { facade.find_duplicates(query).await }).await;
            let _ = tx.send(res);
        });
        self._duplicates_task = Some(read_task);

        cx.spawn(async move |this, cx| {
            let Ok(result) = rx.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                if ws.duplicates_epoch != epoch {
                    return;
                }
                ws.duplicates.loading = false;
                ws.duplicates.loading_more = false;
                ws.duplicates.searched = true;
                match result {
                    Ok(page) => {
                        if append {
                            ws.duplicates.groups.extend(page.groups);
                        } else {
                            ws.duplicates.groups = page.groups;
                        }
                        ws.duplicates.next_cursor = page.next_cursor;
                        ws.duplicates.total_groups = page.total_groups;
                        ws.duplicates.total_matches = page.total_matches;
                        ws.duplicates.error = None;
                    }
                    Err(err) => {
                        ws.duplicates.error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ── Native File / Folder Import Flow ─────────────────────────────────────

    pub(super) fn prompt_import_files(&mut self, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(SharedString::from(I18n::t(
                locale,
                "app",
                "app.importPlain",
            ))),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update(cx, |ws, cx| {
                    ws.import_paths(paths, cx);
                });
            }
        })
        .detach();
    }

    pub(super) fn prompt_import_folder(&mut self, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(I18n::t(
                locale,
                "library",
                "importFolder",
            ))),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update(cx, |ws, cx| {
                    ws.import_paths(paths, cx);
                });
            }
        })
        .detach();
    }

    pub(super) fn start_import_batch(&mut self, batch: DroppedMatchBatch, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        let target_db_id = self.effective_import_database_id();
        let dest_name = target_db_id
            .and_then(|id| self.databases.iter().find(|d| d.id == id))
            .map(|db| database_display_name(db, locale))
            .unwrap_or_else(|| "Unsorted".to_string());

        if batch.accepted.is_empty() {
            let mut run = ImportRunState::start(
                0,
                batch.skipped_unsupported,
                batch.ignored_hidden,
                dest_name,
            );
            run.error = Some(I18n::t(locale, "library", "importFolderEmpty").to_string());
            self.import_run = Some(run);
            cx.notify();
            return;
        }

        let Some(facade) = self.facade.clone() else {
            let mut run = ImportRunState::start(
                batch.accepted.len(),
                batch.skipped_unsupported,
                batch.ignored_hidden,
                dest_name,
            );
            run.finish(false);
            run.error = Some("Database connection is not ready.".to_string());
            self.import_run = Some(run);
            cx.notify();
            return;
        };

        let run = ImportRunState::start(
            batch.accepted.len(),
            batch.skipped_unsupported,
            batch.ignored_hidden,
            dest_name,
        );
        let cancel_flag = Arc::clone(&run.cancel_flag);
        self.import_run = Some(run);
        cx.notify();

        let files = batch.accepted;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(Option<ImportFileRow>, bool)>();

        TokioBridge::spawn_write(cx, async move {
            let mut stopped = false;
            for path in files {
                if cancel_flag.load(Ordering::SeqCst) {
                    stopped = true;
                    break;
                }
                let display_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                let f = facade.clone();
                let p = path.clone();
                let res =
                    run_facade_async(
                        move || async move { f.import_match_path(&p, target_db_id).await },
                    )
                    .await;

                let status = match res {
                    Ok(resp) if resp.duplicate => ImportFileStatus::Duplicate {
                        match_id: resp.r#match.id,
                        warnings: resp.warnings,
                    },
                    Ok(resp) => ImportFileStatus::Imported {
                        match_id: resp.r#match.id,
                        position_count: resp.r#match.position_count,
                        warnings: resp.warnings,
                    },
                    Err(err) => ImportFileStatus::Failed(format_domain_error(locale, &err)),
                };
                if tx
                    .send((
                        Some(ImportFileRow {
                            path,
                            display_name,
                            status,
                        }),
                        false,
                    ))
                    .is_err()
                {
                    return;
                }
            }
            let _ = tx.send((None, stopped));
        })
        .detach();

        cx.spawn(async move |this, cx| {
            while let Some((maybe_row, stopped)) = rx.recv().await {
                let alive = this
                    .update(cx, |ws, cx| match maybe_row {
                        Some(row) => {
                            if let Some(run) = &mut ws.import_run {
                                run.record_row(row);
                            }
                            cx.notify();
                        }
                        None => {
                            let imported_any = ws
                                .import_run
                                .as_ref()
                                .is_some_and(|r| r.imported > 0 || r.duplicates > 0);
                            if let Some(run) = &mut ws.import_run {
                                run.finish(stopped);
                            }
                            if imported_any {
                                ws.on_library_mutation_completed(cx);
                            } else {
                                cx.notify();
                            }
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
    }

    // ── Deferred Pending Operations (Window-Dependent) ───────────────────────

    fn flush_pending_window_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(val) = self.pending_position_input_sync.take() {
            self.inputs.position_id.update(cx, |state, cx| {
                if state.value().as_ref() != val {
                    state.set_value(val, window, cx);
                }
            });
        }

        if let Some(match_ids) = self.pending_bulk_analysis_ids.take() {
            let mut resolved = Vec::new();
            let mut missing_ids = Vec::new();
            for id in match_ids {
                if let Some(found) = self
                    .matches
                    .iter()
                    .chain(self.selection.items().iter())
                    .find(|m| m.id == id)
                {
                    resolved.push(clone_database_match(found));
                } else {
                    missing_ids.push(id);
                }
            }

            let entity = cx.entity();
            if missing_ids.is_empty() {
                let analyzable: Vec<_> = resolved
                    .iter()
                    .filter(|m| can_analyze_match(m))
                    .map(clone_database_match)
                    .collect();
                let unavailable = resolved.len().saturating_sub(analyzable.len());
                if !analyzable.is_empty() {
                    window.defer(cx, move |window, cx| {
                        entity.update(cx, |ws, cx| {
                            ws.open_bulk_analysis_modal(analyzable, unavailable, window, cx);
                        });
                    });
                }
            } else if let Some(facade) = self.facade.clone() {
                let window_handle = window.window_handle();
                let task = TokioBridge::spawn_read(cx, async move {
                    run_facade_async(move || async move {
                        let mut fetched = resolved;
                        for id in missing_ids {
                            if let Ok(m) = facade.get_match_metadata(id).await {
                                fetched.push(m);
                            }
                        }
                        fetched
                    })
                    .await
                });
                cx.spawn(async move |this, cx| {
                    if let Ok(fetched) = task.await {
                        let analyzable: Vec<_> = fetched
                            .iter()
                            .filter(|m| can_analyze_match(m))
                            .map(clone_database_match)
                            .collect();
                        let unavailable = fetched.len().saturating_sub(analyzable.len());
                        if !analyzable.is_empty() {
                            let _ = window_handle.update(cx, |_, window, cx| {
                                let _ = this.update(cx, |ws, cx| {
                                    ws.open_bulk_analysis_modal(
                                        analyzable,
                                        unavailable,
                                        window,
                                        cx,
                                    );
                                });
                            });
                        }
                    }
                })
                .detach();
            }
        }
    }

    // ── Test-Only Inspection & Seeding Helpers ───────────────────────────────

    #[cfg(test)]
    pub(super) fn seed_test_data(&mut self, databases: Vec<Database>, matches: Vec<DatabaseMatch>) {
        self.databases = databases;
        self.total_matches = Some(matches.len() as i64);
        self.matches = matches;
        self.loading_matches = false;
        self.load_error = None;
    }
}

impl Render for LibraryWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.flush_pending_window_actions(window, cx);

        let p = NeonTheme::current_palette(cx);
        let locale: Locale = current_locale(cx);

        div()
            .id("library-workspace")
            .test_support()
            .h_flex()
            .size_full()
            .min_h(px(0.0))
            .bg(p.bg)
            .text_color(p.text)
            .child(self.render_database_sidebar(p, locale, cx))
            .child(
                div()
                    .id("library-main-pane")
                    .v_flex()
                    .flex_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .h_full()
                    .p(px(14.0))
                    .gap(px(10.0))
                    .child(self.render_import_header(p, locale, cx))
                    .child(self.render_search_section(p, locale, cx))
                    .child(self.render_toolbar_and_batch_bar(p, locale, cx))
                    .child(self.render_match_table_section(p, locale, cx)),
            )
    }
}
