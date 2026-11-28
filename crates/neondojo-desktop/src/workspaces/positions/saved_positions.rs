//! Saved Positions sub-view (`PositionBrowser.tsx`, `PositionBrowserControls.tsx`,
//! `PositionBrowserResults.tsx`, `PositionBrowserDecks.tsx`, `SavedPositionSearches.tsx`).
//!
//! Features:
//! - 4 filter groups (Match, Position, Analysis, Tags & Comments) + `AppliedFilterChips`
//! - Named Saved Searches with Save / Update / Delete, Undo delete, missing-DB alert, and
//!   explicit 409 revision conflict banner with Reload trigger
//! - 13 sortable, hideable, mouse-resizable columns persisted in `LocalUiState::positions`
//! - Cross-page selection capped strictly at 200 items (`POSITION_SELECTION_LIMIT`)
//! - Batch bookmark removal + Undo restoration banner
//! - Right-hand `BoardView` preview + retained-position badge + metadata + notes/tags
//! - Analysis readiness pre-check + Deck Transfer dialog with inline Create Deck and
//!   per-position outcome breakdown

use super::columns::{
    POSITION_SELECTION_LIMIT, SavedDataColumn, SavedPositionColumn, SavedTableLayout,
    effective_sort_direction, toggle_position_selection, toggled_sort_direction,
};
use super::practice_board::position_to_board_position;
use super::{
    ALL_GAME_PLANS, board_theme_from_local_state, game_plan_label, spawn_facade_read,
    spawn_facade_write,
};
use crate::app_context::DesktopAppContext;
use crate::i18n::{current_locale, format_domain_error};
use crate::local_state::LocalUiState;
use crate::review::severity::SeverityStyle;
use crate::shell::AppNotice;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::banners::{BannerKind, EmptyState, StatusBanner};
use crate::ui::decimal_input::{DecimalInput, DecimalInputConfig};
use crate::ui::filter_chips::{AppliedFilterChip, AppliedFilterChips};
use crate::ui::icons::{ActionIconName, action_icon};
use crate::ui::modal::{ModalBusyLock, open_library_dialog};
use crate::ui::pagination::NumberedSearchPagination;
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FontWeight, InteractiveElement, IntoElement,
    MouseButton, MouseMoveEvent, MouseUpEvent, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Window,
    base::StyledExt,
    component::WindowExt,
    component::input::{Input, InputEvent, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::databases::Database;
use neondojo::domain_error::{DomainError, ErrorKind};
use neondojo::facade::BackendFacade;
use neondojo::game_plan::GamePlan;
use neondojo::learning::{AddPositionsResult, Deck, PositionAddStatus};
use neondojo::model::Position;
use neondojo::position_browser::{
    CreatePositionFilter, PositionAnalysisReadiness, PositionAnalysisState,
    PositionBrowserDirection, PositionBrowserHit, PositionBrowserSort, PositionFilterDefinition,
    SavedPositionFilter, UpdatePositionFilter,
};
use neondojo_board::{BoardView, ThemeAtlasCache};
use neondojo_i18n::{I18n, Locale, Param, format_decimal, format_int, localize_decision};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Page size matching `POSITION_PAGE_SIZE` in `position-browser-api.ts`.
pub const SAVED_PAGE_SIZE: i64 = 25;

/// Events emitted by [`SavedPositionsView`] to [`super::PositionsWorkspace`].
#[derive(Debug, Clone)]
pub enum SavedPositionsEvent {
    OpenInAnalysis {
        match_id: Uuid,
        game_no: i32,
        position_id: Uuid,
    },
    FindSimilar {
        position: Position,
        match_name: String,
    },
    ShowNotice(AppNotice),
}

/// Editable text/select draft for all 4 Saved Positions filter groups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SavedFilterDraft {
    pub database_id: Option<Uuid>,
    pub q: String,
    pub player: String,
    // Group 1: Match
    pub match_name: String,
    pub tournament: String,
    pub round: String,
    pub match_points_min: String,
    pub match_points_max: String,
    // Group 2: Position
    pub game_plan: Option<GamePlan>,
    pub die1: String,
    pub die2: String,
    pub white_score: String,
    pub black_score: String,
    pub crawford: Option<bool>,
    pub cube_value: String,
    pub position_id: String,
    // Group 3: Analysis
    pub analysis_state: Option<PositionAnalysisState>,
    pub min_loss: String,
    pub max_loss: String,
    // Group 4: Tags & Notes
    pub tag: String,
    pub comment: String,
    // Sort
    pub sort: PositionBrowserSort,
    pub direction: Option<PositionBrowserDirection>,
}

impl SavedFilterDraft {
    pub fn from_definition(def: &PositionFilterDefinition) -> Self {
        Self {
            database_id: def.database_id,
            q: def.q.clone().unwrap_or_default(),
            player: def.player.clone().unwrap_or_default(),
            match_name: def.match_name.clone().unwrap_or_default(),
            tournament: def.tournament.clone().unwrap_or_default(),
            round: def.round.clone().unwrap_or_default(),
            match_points_min: def
                .match_points_min
                .map(|v| v.to_string())
                .unwrap_or_default(),
            match_points_max: def
                .match_points_max
                .map(|v| v.to_string())
                .unwrap_or_default(),
            game_plan: def.game_plan,
            die1: def.die1.map(|v| v.to_string()).unwrap_or_default(),
            die2: def.die2.map(|v| v.to_string()).unwrap_or_default(),
            white_score: def.white_score.map(|v| v.to_string()).unwrap_or_default(),
            black_score: def.black_score.map(|v| v.to_string()).unwrap_or_default(),
            crawford: def.crawford,
            cube_value: def.cube_value.map(|v| v.to_string()).unwrap_or_default(),
            position_id: def.position_id.clone().unwrap_or_default(),
            analysis_state: def.analysis_state,
            min_loss: def.min_loss.map(|v| v.to_string()).unwrap_or_default(),
            max_loss: def.max_loss.map(|v| v.to_string()).unwrap_or_default(),
            tag: def.tag.clone().unwrap_or_default(),
            comment: def.comment.clone().unwrap_or_default(),
            sort: def.sort,
            direction: def.direction,
        }
    }

    /// Validates and compiles the draft into a [`PositionFilterDefinition`].
    pub fn to_definition(&self) -> Option<PositionFilterDefinition> {
        let opt_text = |s: &str| {
            let trimmed = s.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        };
        let opt_i32 = |s: &str, min: i32, max: i32| -> Option<Option<i32>> {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Some(None);
            }
            let parsed = trimmed.parse::<i32>().ok()?;
            if !(min..=max).contains(&parsed) {
                return None;
            }
            Some(Some(parsed))
        };
        let opt_f64 = |s: &str| -> Option<Option<f64>> {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Some(None);
            }
            let canonical = neondojo_i18n::canonical_decimal(trimmed);
            let parsed = canonical.parse::<f64>().ok()?;
            if !parsed.is_finite() || !(0.0..=1000.0).contains(&parsed) {
                return None;
            }
            Some(Some(parsed))
        };

        let match_points_min = opt_i32(&self.match_points_min, 0, 32767)?;
        let match_points_max = opt_i32(&self.match_points_max, 0, 32767)?;
        if let (Some(lo), Some(hi)) = (match_points_min, match_points_max)
            && lo > hi
        {
            return None;
        }

        let mut die1 = opt_i32(&self.die1, 1, 6)?;
        let mut die2 = opt_i32(&self.die2, 1, 6)?;
        if let (Some(a), Some(b)) = (die1, die2)
            && a > b
        {
            std::mem::swap(&mut die1, &mut die2);
        }

        let white_score = opt_i32(&self.white_score, 0, 32767)?;
        let black_score = opt_i32(&self.black_score, 0, 32767)?;
        let cube_value = opt_i32(&self.cube_value, 1, 32768)?;
        if let Some(cv) = cube_value
            && !cv.unsigned_abs().is_power_of_two()
        {
            return None;
        }

        let min_loss = opt_f64(&self.min_loss)?;
        let max_loss = opt_f64(&self.max_loss)?;
        if let (Some(lo), Some(hi)) = (min_loss, max_loss)
            && lo > hi
        {
            return None;
        }

        let position_id = opt_text(&self.position_id);
        if let Some(pid) = &position_id
            && pid.len() != 14
        {
            return None;
        }

        Some(PositionFilterDefinition {
            schema_version: 1,
            game_plan: self.game_plan,
            database_id: self.database_id,
            q: opt_text(&self.q),
            player: opt_text(&self.player),
            match_name: opt_text(&self.match_name),
            tournament: opt_text(&self.tournament),
            round: opt_text(&self.round),
            tag: opt_text(&self.tag),
            comment: opt_text(&self.comment),
            position_id,
            match_points_min,
            match_points_max,
            die1,
            die2,
            white_score,
            black_score,
            crawford: self.crawford,
            cube_value,
            analysis_state: self.analysis_state,
            min_loss,
            max_loss,
            sort: self.sort,
            direction: self.direction,
        })
    }

    pub fn remove_key(&mut self, key: &str) {
        match key {
            "database_id" => self.database_id = None,
            "q" => self.q.clear(),
            "player" => self.player.clear(),
            "match_name" => self.match_name.clear(),
            "tournament" => self.tournament.clear(),
            "round" => self.round.clear(),
            "match_points_min" => self.match_points_min.clear(),
            "match_points_max" => self.match_points_max.clear(),
            "game_plan" => self.game_plan = None,
            "dice" | "die1" | "die2" => {
                self.die1.clear();
                self.die2.clear();
            }
            "white_score" => self.white_score.clear(),
            "black_score" => self.black_score.clear(),
            "crawford" => self.crawford = None,
            "cube_value" => self.cube_value.clear(),
            "position_id" => self.position_id.clear(),
            "analysis_state" => self.analysis_state = None,
            "min_loss" => self.min_loss.clear(),
            "max_loss" => self.max_loss.clear(),
            "tag" => self.tag.clear(),
            "comment" => self.comment.clear(),
            _ => {}
        }
    }

    /// Builds active [`AppliedFilterChip`]s for the currently applied definition.
    pub fn build_chips(&self, locale: Locale, databases: &[Database]) -> Vec<AppliedFilterChip> {
        let p = |k: &'static str| I18n::t(locale, "position-browser", k);
        let mut chips = Vec::new();
        if let Some(db_id) = self.database_id {
            let val = databases
                .iter()
                .find(|d| d.id == db_id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| format!("{} ({db_id})", p("missing")));
            chips.push(AppliedFilterChip::new("database_id", p("database")).value(val));
        }
        let push_text = |chips: &mut Vec<AppliedFilterChip>,
                         id: &'static str,
                         label: &'static str,
                         val: &str| {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                chips.push(AppliedFilterChip::new(id, label).value(trimmed.to_string()));
            }
        };
        push_text(&mut chips, "q", p("q"), &self.q);
        push_text(&mut chips, "player", p("player"), &self.player);
        push_text(&mut chips, "match_name", p("matchName"), &self.match_name);
        push_text(&mut chips, "tournament", p("tournament"), &self.tournament);
        push_text(&mut chips, "round", p("round"), &self.round);
        push_text(
            &mut chips,
            "match_points_min",
            p("minPoints"),
            &self.match_points_min,
        );
        push_text(
            &mut chips,
            "match_points_max",
            p("maxPoints"),
            &self.match_points_max,
        );
        if let Some(plan) = self.game_plan {
            chips.push(
                AppliedFilterChip::new("game_plan", I18n::t(locale, "game-plan", "title"))
                    .value(game_plan_label(locale, plan)),
            );
        }
        if !self.die1.trim().is_empty() || !self.die2.trim().is_empty() {
            let parts: Vec<&str> = [self.die1.trim(), self.die2.trim()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect();
            chips.push(AppliedFilterChip::new("dice", p("dice")).value(parts.join("–")));
        }
        push_text(
            &mut chips,
            "white_score",
            p("whiteScore"),
            &self.white_score,
        );
        push_text(
            &mut chips,
            "black_score",
            p("blackScore"),
            &self.black_score,
        );
        if let Some(crawford) = self.crawford {
            chips.push(
                AppliedFilterChip::new("crawford", p("crawford")).value(p(if crawford {
                    "yes"
                } else {
                    "no"
                })),
            );
        }
        push_text(&mut chips, "cube_value", p("cube"), &self.cube_value);
        push_text(&mut chips, "position_id", p("checkerId"), &self.position_id);
        if let Some(state) = self.analysis_state {
            let val = match state {
                PositionAnalysisState::Ready => p("ready"),
                PositionAnalysisState::Missing => p("missing"),
            };
            chips.push(AppliedFilterChip::new("analysis_state", p("analysis")).value(val));
        }
        push_text(&mut chips, "min_loss", p("minLoss"), &self.min_loss);
        push_text(&mut chips, "max_loss", p("maxLoss"), &self.max_loss);
        push_text(&mut chips, "tag", p("tag"), &self.tag);
        push_text(&mut chips, "comment", p("comment"), &self.comment);
        chips
    }
}

/// Checks whether two [`PositionFilterDefinition`] values are semantically equal.
pub fn same_position_definition(
    a: &PositionFilterDefinition,
    b: &PositionFilterDefinition,
) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

/// Named Saved Searches state with optimistic concurrency (409 conflict) & Undo delete.
#[derive(Debug, Default)]
pub struct SavedSearchesState {
    pub items: Vec<SavedPositionFilter>,
    pub active_id: Option<Uuid>,
    pub deleted_undo: Option<SavedPositionFilter>,
    pub busy: bool,
    pub conflict_error: Option<SharedString>,
    pub error: Option<SharedString>,
    pub notice: Option<SharedString>,
}

impl SavedSearchesState {
    pub fn active(&self) -> Option<&SavedPositionFilter> {
        let id = self.active_id?;
        self.items.iter().find(|item| item.id == id)
    }

    pub fn is_dirty(&self, draft: &SavedFilterDraft) -> bool {
        let Some(active) = self.active() else {
            return false;
        };
        match draft.to_definition() {
            Some(def) => !same_position_definition(&active.definition, &def),
            None => true,
        }
    }

    /// Records a backend mutation error, separating 409 revision conflicts so the UI
    /// renders an explicit conflict banner with a Reload action.
    pub fn record_error(&mut self, locale: Locale, err: &DomainError) {
        let msg = SharedString::from(format_domain_error(locale, err));
        if err.kind == ErrorKind::Conflict {
            self.conflict_error = Some(msg);
            self.error = None;
        } else {
            self.error = Some(msg);
            self.conflict_error = None;
        }
    }

    /// Applies a locally completed deletion, saving the item for one-click Undo.
    pub fn apply_deleted(&mut self, removed: SavedPositionFilter, locale: Locale) {
        self.items.retain(|i| i.id != removed.id);
        if self.active_id == Some(removed.id) {
            self.active_id = None;
        }
        self.deleted_undo = Some(removed);
        self.conflict_error = None;
        self.error = None;
        self.notice = Some(SharedString::from(I18n::t(
            locale,
            "position-browser",
            "deleted",
        )));
    }

    /// Upserts a saved search into the alphabetically sorted catalog.
    pub fn upsert_saved(&mut self, saved: SavedPositionFilter, locale: Locale) {
        self.items.retain(|i| i.id != saved.id);
        let id = saved.id;
        self.items.push(saved);
        self.items.sort_by_key(|a| a.name.to_lowercase());
        self.active_id = Some(id);
        self.deleted_undo = None;
        self.conflict_error = None;
        self.error = None;
        self.notice = Some(SharedString::from(I18n::t(
            locale,
            "position-browser",
            "saved",
        )));
    }
}

/// Clones a [`SavedPositionFilter`] without modifying the backend crate.
pub fn clone_saved_filter(filter: &SavedPositionFilter) -> SavedPositionFilter {
    SavedPositionFilter {
        id: filter.id,
        name: filter.name.clone(),
        definition: filter.definition.clone(),
        version: filter.version,
        created_at: filter.created_at,
        updated_at: filter.updated_at,
        database_missing: filter.database_missing,
    }
}

/// Compact metadata retained across pages for selected positions (up to 200 items).
#[derive(Debug, Clone)]
struct SelectedPositionMeta {
    match_id: Uuid,
    game_no: i32,
    ply: i32,
    position_id: Uuid,
    database_name: String,
    display_name: String,
}

impl SelectedPositionMeta {
    fn from_hit(hit: &PositionBrowserHit) -> Self {
        Self {
            match_id: hit.position.match_uuid,
            game_no: hit.position.game_no,
            ply: hit.position.ply,
            position_id: hit.position.id,
            database_name: hit.database_name.clone(),
            display_name: hit.display_name.clone(),
        }
    }
}

/// Active mouse drag state for resizing a Saved Positions column.
#[derive(Debug, Clone, Copy)]
struct ColumnResizeDrag {
    column: SavedPositionColumn,
    start_x: f32,
    start_width: u32,
}

/// Text inputs bound to [`SavedFilterDraft`].
struct SavedFilterInputs {
    q: Entity<InputState>,
    player: Entity<InputState>,
    match_name: Entity<InputState>,
    tournament: Entity<InputState>,
    round: Entity<InputState>,
    match_points_min: Entity<InputState>,
    match_points_max: Entity<InputState>,
    die1: Entity<InputState>,
    die2: Entity<InputState>,
    white_score: Entity<InputState>,
    black_score: Entity<InputState>,
    cube_value: Entity<InputState>,
    position_id: Entity<InputState>,
    min_loss: Entity<DecimalInput>,
    max_loss: Entity<DecimalInput>,
    tag: Entity<InputState>,
    comment: Entity<InputState>,
}

/// Complete GPUI Saved Positions sub-view.
pub struct SavedPositionsView {
    facade: Option<BackendFacade>,
    persist_path: Option<PathBuf>,
    visible: bool,
    loaded_once: bool,

    // Filters & saved searches
    definition: PositionFilterDefinition,
    draft: SavedFilterDraft,
    inputs: SavedFilterInputs,
    filters_expanded: bool,
    invalid_filter: bool,
    databases: Vec<Database>,
    saved_searches: SavedSearchesState,

    // Results & pagination
    page: i64,
    total: i64,
    items: Vec<PositionBrowserHit>,
    loading: bool,
    load_error: Option<SharedString>,
    preview_id: Option<Uuid>,
    preview_details_expanded: bool,
    preview_board: Entity<BoardView>,

    // 13-column layout & drag resize
    table_layout: SavedTableLayout,
    column_picker_open: bool,
    resizing: Option<ColumnResizeDrag>,

    // Cross-page selection (max 200), batch remove + Undo
    selected_ids: Vec<Uuid>,
    selected_cache: HashMap<Uuid, SelectedPositionMeta>,
    removing: bool,
    restoring: bool,
    remove_error: Option<SharedString>,
    last_removed_ids: Vec<Uuid>,

    // Last deck transfer outcome displayed inline
    transfer_outcome: Option<(AddPositionsResult, HashMap<Uuid, String>)>,

    // In-flight abortable read tasks (replacing drops prior ReadTask -> AbortHandle::abort)
    search_task: Option<gpui_kit::Task<()>>,
    meta_task: Option<gpui_kit::Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SavedPositionsEvent> for SavedPositionsView {}

impl SavedPositionsView {
    pub fn new(persist_path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let locale = current_locale(cx);
        let sep = locale.decimal_separator();
        let app_ctx = cx.try_global::<DesktopAppContext>().cloned();
        let facade = app_ctx.as_ref().and_then(|c| c.facade());
        let local_state = cx.try_global::<LocalUiState>().cloned().unwrap_or_default();
        let table_layout = SavedTableLayout::from_prefs(&local_state.positions);
        let atlas = app_ctx
            .as_ref()
            .map(|c| Arc::clone(&c.board_atlas))
            .unwrap_or_else(|| Arc::new(Mutex::new(ThemeAtlasCache::default())));
        let board_theme = board_theme_from_local_state(&local_state);

        let preview_board = cx.new(|cx| {
            let mut bv = BoardView::new("saved-positions-preview-board", atlas);
            bv.set_theme_config(board_theme, cx);
            bv
        });

        let make_input =
            |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
                cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
            };

        let p = |k: &'static str| I18n::t(locale, "position-browser", k);
        let q = make_input(p("q"), window, cx);
        let player = make_input(p("player"), window, cx);
        let match_name = make_input(p("matchName"), window, cx);
        let tournament = make_input(p("tournament"), window, cx);
        let round = make_input(p("round"), window, cx);
        let match_points_min = make_input(p("minPoints"), window, cx);
        let match_points_max = make_input(p("maxPoints"), window, cx);
        let die1 = make_input("1–6", window, cx);
        let die2 = make_input("1–6", window, cx);
        let white_score = make_input(p("whiteScore"), window, cx);
        let black_score = make_input(p("blackScore"), window, cx);
        let cube_value = make_input("1, 2, 4…", window, cx);
        let position_id = make_input("14 chars", window, cx);
        let tag = make_input(p("tag"), window, cx);
        let comment = make_input(p("comment"), window, cx);

        let weak = cx.weak_entity();
        let min_loss = cx.new(|cx| {
            let weak_min = weak.clone();
            DecimalInput::new(
                DecimalInputConfig::new("saved-filter-min-loss", 0.005)
                    .min(0.0)
                    .max(1000.0)
                    .separator(sep)
                    .placeholder(p("minLoss"))
                    .width(104.0),
                "",
                Some(Rc::new(move |val, _, cx| {
                    let s = val.to_string();
                    let _ = weak_min.update(cx, |this, cx| {
                        this.draft.min_loss = s;
                        cx.notify();
                    });
                })),
                window,
                cx,
            )
        });
        let max_loss = cx.new(|cx| {
            let weak_max = weak.clone();
            DecimalInput::new(
                DecimalInputConfig::new("saved-filter-max-loss", 0.005)
                    .min(0.0)
                    .max(1000.0)
                    .separator(sep)
                    .placeholder(p("maxLoss"))
                    .width(104.0),
                "",
                Some(Rc::new(move |val, _, cx| {
                    let s = val.to_string();
                    let _ = weak_max.update(cx, |this, cx| {
                        this.draft.max_loss = s;
                        cx.notify();
                    });
                })),
                window,
                cx,
            )
        });

        let mut subscriptions = Vec::new();
        macro_rules! bind_text {
            ($ent:expr, $field:ident) => {
                subscriptions.push(cx.subscribe_in(
                    &$ent,
                    window,
                    |this, state, event: &InputEvent, _window, cx| {
                        if matches!(event, InputEvent::Change) {
                            this.draft.$field = state.read(cx).value().to_string();
                            cx.notify();
                        }
                    },
                ));
            };
        }
        bind_text!(q, q);
        bind_text!(player, player);
        bind_text!(match_name, match_name);
        bind_text!(tournament, tournament);
        bind_text!(round, round);
        bind_text!(match_points_min, match_points_min);
        bind_text!(match_points_max, match_points_max);
        bind_text!(die1, die1);
        bind_text!(die2, die2);
        bind_text!(white_score, white_score);
        bind_text!(black_score, black_score);
        bind_text!(cube_value, cube_value);
        bind_text!(position_id, position_id);
        bind_text!(tag, tag);
        bind_text!(comment, comment);

        let inputs = SavedFilterInputs {
            q,
            player,
            match_name,
            tournament,
            round,
            match_points_min,
            match_points_max,
            die1,
            die2,
            white_score,
            black_score,
            cube_value,
            position_id,
            min_loss,
            max_loss,
            tag,
            comment,
        };

        Self {
            facade,
            persist_path,
            visible: false,
            loaded_once: false,
            definition: PositionFilterDefinition::default(),
            draft: SavedFilterDraft::default(),
            inputs,
            filters_expanded: false,
            invalid_filter: false,
            databases: Vec::new(),
            saved_searches: SavedSearchesState::default(),
            page: 1,
            total: 0,
            items: Vec::new(),
            loading: false,
            load_error: None,
            preview_id: None,
            preview_details_expanded: false,
            preview_board,
            table_layout,
            column_picker_open: false,
            resizing: None,
            selected_ids: Vec::new(),
            selected_cache: HashMap::new(),
            removing: false,
            restoring: false,
            remove_error: None,
            last_removed_ids: Vec::new(),
            transfer_outcome: None,
            search_task: None,
            meta_task: None,
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
            if !self.loaded_once && self.facade.is_some() {
                self.loaded_once = true;
                self.load_metadata(cx);
                self.load_page(1, cx);
            }
        }
        cx.notify();
    }

    /// Public getters/mutators for headless tests and workspace coordination.
    pub fn table_layout(&self) -> &SavedTableLayout {
        &self.table_layout
    }

    pub fn selected_ids(&self) -> &[Uuid] {
        &self.selected_ids
    }

    pub fn saved_searches(&self) -> &SavedSearchesState {
        &self.saved_searches
    }

    pub fn saved_searches_mut(&mut self) -> &mut SavedSearchesState {
        &mut self.saved_searches
    }

    pub fn set_items_for_test(&mut self, items: Vec<PositionBrowserHit>, cx: &mut Context<Self>) {
        self.total = items.len() as i64;
        self.items = items;
        self.sync_preview_board(cx);
        cx.notify();
    }

    pub fn set_filters_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        self.filters_expanded = expanded;
        cx.notify();
    }

    pub fn toggle_column_visibility(
        &mut self,
        col: SavedDataColumn,
        visible: bool,
        cx: &mut Context<Self>,
    ) {
        if self.table_layout.toggle_column(col, visible) {
            self.persist_table_prefs(cx);
            cx.notify();
        }
    }

    pub fn set_show_played(&mut self, show: bool, cx: &mut Context<Self>) {
        if self.table_layout.show_played != show {
            self.table_layout.show_played = show;
            self.persist_table_prefs(cx);
            self.sync_preview_board(cx);
            cx.notify();
        }
    }

    pub fn set_column_width(
        &mut self,
        col: SavedPositionColumn,
        width: u32,
        cx: &mut Context<Self>,
    ) {
        self.table_layout.set_width(col, width);
        self.persist_table_prefs(cx);
        cx.notify();
    }

    pub fn reset_column_widths(&mut self, cx: &mut Context<Self>) {
        self.table_layout.reset_widths();
        self.persist_table_prefs(cx);
        cx.notify();
    }

    pub fn toggle_selection(&mut self, ids: &[Uuid], checked: bool, cx: &mut Context<Self>) {
        for hit in &self.items {
            self.selected_cache
                .insert(hit.position.id, SelectedPositionMeta::from_hit(hit));
        }
        self.selected_ids =
            toggle_position_selection(&self.selected_ids, ids, checked, POSITION_SELECTION_LIMIT);
        let active: std::collections::HashSet<Uuid> = self.selected_ids.iter().copied().collect();
        self.selected_cache.retain(|id, _| active.contains(id));
        cx.notify();
    }

    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selected_ids.clear();
        self.selected_cache.clear();
        cx.notify();
    }

    fn persist_table_prefs(&self, cx: &mut App) {
        let mut state = cx.try_global::<LocalUiState>().cloned().unwrap_or_default();
        let _ = self
            .table_layout
            .save_to_local_state(&mut state, self.persist_path.as_deref());
        cx.set_global(state);
    }

    fn sync_inputs_from_draft(&self, window: &mut Window, cx: &mut Context<Self>) {
        macro_rules! set_val {
            ($ent:expr, $val:expr) => {
                let v = $val.clone();
                $ent.update(cx, |s, cx| s.set_value(v, window, cx));
            };
        }
        set_val!(self.inputs.q, self.draft.q);
        set_val!(self.inputs.player, self.draft.player);
        set_val!(self.inputs.match_name, self.draft.match_name);
        set_val!(self.inputs.tournament, self.draft.tournament);
        set_val!(self.inputs.round, self.draft.round);
        set_val!(self.inputs.match_points_min, self.draft.match_points_min);
        set_val!(self.inputs.match_points_max, self.draft.match_points_max);
        set_val!(self.inputs.die1, self.draft.die1);
        set_val!(self.inputs.die2, self.draft.die2);
        set_val!(self.inputs.white_score, self.draft.white_score);
        set_val!(self.inputs.black_score, self.draft.black_score);
        set_val!(self.inputs.cube_value, self.draft.cube_value);
        set_val!(self.inputs.position_id, self.draft.position_id);
        set_val!(self.inputs.tag, self.draft.tag);
        set_val!(self.inputs.comment, self.draft.comment);
        let min_l = self.draft.min_loss.clone();
        self.inputs
            .min_loss
            .update(cx, |d, cx| d.set_canonical_value(&min_l, window, cx));
        let max_l = self.draft.max_loss.clone();
        self.inputs
            .max_loss
            .update(cx, |d, cx| d.set_canonical_value(&max_l, window, cx));
    }

    pub fn apply_draft(&mut self, cx: &mut Context<Self>) {
        match self.draft.to_definition() {
            Some(def) => {
                self.invalid_filter = false;
                self.definition = def;
                self.selected_ids.clear();
                self.selected_cache.clear();
                self.preview_id = None;
                self.load_page(1, cx);
            }
            None => {
                self.invalid_filter = true;
            }
        }
        cx.notify();
    }

    pub fn clear_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.draft = SavedFilterDraft::default();
        self.sync_inputs_from_draft(window, cx);
        self.apply_draft(cx);
    }

    pub fn remove_filter_chip(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.draft = SavedFilterDraft::from_definition(&self.definition);
        self.draft.remove_key(key);
        self.sync_inputs_from_draft(window, cx);
        self.apply_draft(cx);
    }

    pub fn apply_sort(&mut self, clicked: PositionBrowserSort, cx: &mut Context<Self>) {
        if self.removing || self.restoring {
            return;
        }
        let next_dir =
            toggled_sort_direction(self.definition.sort, self.definition.direction, clicked);
        self.draft.sort = clicked;
        self.draft.direction = Some(next_dir);
        self.definition.sort = clicked;
        self.definition.direction = Some(next_dir);
        self.load_page(1, cx);
        cx.notify();
    }

    pub fn choose_saved_search(
        &mut self,
        id: Option<Uuid>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.saved_searches.active_id = id;
        self.saved_searches.notice = None;
        self.saved_searches.conflict_error = None;
        self.saved_searches.error = None;
        if let Some(def) = self
            .saved_searches
            .active()
            .map(|saved| saved.definition.clone())
        {
            self.draft = SavedFilterDraft::from_definition(&def);
            self.sync_inputs_from_draft(window, cx);
            self.apply_draft(cx);
        }
        cx.notify();
    }

    fn active_preview(&self) -> Option<&PositionBrowserHit> {
        self.preview_id
            .and_then(|id| self.items.iter().find(|h| h.position.id == id))
            .or_else(|| self.items.first())
    }

    fn sync_preview_board(&mut self, cx: &mut Context<Self>) {
        let hide_notation = !self.table_layout.show_played;
        let Some((bp, white, black)) = self.active_preview().map(|hit| {
            (
                position_to_board_position(&hit.position, hide_notation),
                hit.match_info.white.clone(),
                hit.match_info.black.clone(),
            )
        }) else {
            return;
        };
        self.preview_board.update(cx, |bv, cx| {
            bv.set_position(bp, cx);
            bv.set_players(white, black, cx);
        });
    }

    pub fn load_metadata(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let Some(task) = spawn_facade_read(cx, move || async move {
            let dbs = facade.list_databases().await;
            let filters = facade.list_saved_searches().await;
            (dbs, filters)
        }) else {
            return;
        };
        let weak = cx.weak_entity();
        self.meta_task = Some(cx.spawn(async move |_this, cx| {
            let Ok((dbs_res, filters_res)) = task.await else {
                return;
            };
            let _ = weak.update(cx, |view, cx| {
                let locale = current_locale(cx);
                if let Ok(dbs) = dbs_res {
                    view.databases = dbs;
                }
                match filters_res {
                    Ok(list) => {
                        view.saved_searches.items = list;
                        view.saved_searches.conflict_error = None;
                        view.saved_searches.error = None;
                    }
                    Err(err) => {
                        view.saved_searches.record_error(locale, &err);
                    }
                }
                cx.notify();
            });
        }));
    }

    pub fn load_page(&mut self, page: i64, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        self.page = page.max(1);
        self.loading = true;
        self.load_error = None;
        let filters = self.definition.clone();
        let req_page = self.page;

        let Some(task) = spawn_facade_read(cx, move || async move {
            facade
                .search_saved_positions(filters, None, Some(req_page), Some(SAVED_PAGE_SIZE))
                .await
        }) else {
            self.loading = false;
            return;
        };
        let weak = cx.weak_entity();
        self.search_task = Some(cx.spawn(async move |_this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = weak.update(cx, |view, cx| {
                view.loading = false;
                match res {
                    Ok(page_data) => {
                        view.total = page_data.total.unwrap_or(page_data.items.len() as i64);
                        view.items = page_data.items;
                        if let Some(pid) = view.preview_id
                            && !view.items.iter().any(|h| h.position.id == pid)
                        {
                            view.preview_id = view.items.first().map(|h| h.position.id);
                        }
                        view.sync_preview_board(cx);
                    }
                    Err(err) => {
                        let locale = current_locale(cx);
                        view.load_error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub fn batch_remove(&mut self, ids: Vec<Uuid>, cx: &mut Context<Self>) {
        if ids.is_empty() || self.removing || self.restoring {
            return;
        }
        let Some(facade) = self.facade.clone() else {
            // Offline/test local removal support
            let remove_set: std::collections::HashSet<Uuid> = ids.iter().copied().collect();
            self.items.retain(|h| !remove_set.contains(&h.position.id));
            self.selected_ids.retain(|id| !remove_set.contains(id));
            self.last_removed_ids = ids;
            self.sync_preview_board(cx);
            cx.notify();
            return;
        };
        self.removing = true;
        self.remove_error = None;
        let ids_req = ids.clone();
        let Some(task) = spawn_facade_write(cx, move || async move {
            facade.batch_remove_bookmarks(ids_req).await
        }) else {
            self.removing = false;
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = weak.update(cx, |view, cx| {
                view.removing = false;
                let locale = current_locale(cx);
                match res {
                    Ok(_) => {
                        let removed_set: std::collections::HashSet<Uuid> =
                            ids.iter().copied().collect();
                        view.selected_ids.retain(|id| !removed_set.contains(id));
                        view.selected_cache
                            .retain(|id, _| !removed_set.contains(id));
                        view.last_removed_ids = ids;
                        view.load_page(view.page, cx);
                    }
                    Err(err) => {
                        view.remove_error = Some(
                            format!(
                                "{} {}",
                                I18n::t(locale, "position-browser", "removalFailed"),
                                format_domain_error(locale, &err)
                            )
                            .into(),
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn undo_last_remove(&mut self, cx: &mut Context<Self>) {
        if self.last_removed_ids.is_empty() || self.restoring || self.removing {
            return;
        }
        let ids = std::mem::take(&mut self.last_removed_ids);
        let Some(facade) = self.facade.clone() else {
            cx.notify();
            return;
        };
        self.restoring = true;
        self.remove_error = None;
        let Some(task) = spawn_facade_write(cx, move || async move {
            let mut failed: Option<DomainError> = None;
            for id in ids {
                if let Err(err) = facade.bookmark_position(id).await
                    && failed.is_none()
                {
                    failed = Some(err);
                }
            }
            failed
        }) else {
            self.restoring = false;
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let Ok(failure) = task.await else {
                return;
            };
            let _ = weak.update(cx, |view, cx| {
                view.restoring = false;
                if let Some(err) = failure {
                    let locale = current_locale(cx);
                    view.remove_error = Some(
                        format!(
                            "{} {}",
                            I18n::t(locale, "position-browser", "restoreFailed"),
                            format_domain_error(locale, &err)
                        )
                        .into(),
                    );
                }
                view.load_page(view.page, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn delete_active_saved_search(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.saved_searches.active().map(clone_saved_filter) else {
            return;
        };
        let Some(facade) = self.facade.clone() else {
            let locale = current_locale(cx);
            self.saved_searches.apply_deleted(active, locale);
            cx.notify();
            return;
        };
        self.saved_searches.busy = true;
        self.saved_searches.conflict_error = None;
        self.saved_searches.error = None;
        let id = active.id;
        let version = active.version;
        let Some(task) = spawn_facade_write(cx, move || async move {
            facade.delete_saved_search(id, version).await
        }) else {
            self.saved_searches.busy = false;
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = weak.update(cx, |view, cx| {
                view.saved_searches.busy = false;
                let locale = current_locale(cx);
                match res {
                    Ok(()) => {
                        view.saved_searches.apply_deleted(active, locale);
                    }
                    Err(err) => {
                        view.saved_searches.record_error(locale, &err);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn undo_delete_saved_search(&mut self, cx: &mut Context<Self>) {
        let Some(deleted) = self
            .saved_searches
            .deleted_undo
            .as_ref()
            .map(clone_saved_filter)
        else {
            return;
        };
        let Some(facade) = self.facade.clone() else {
            let locale = current_locale(cx);
            self.saved_searches.upsert_saved(deleted, locale);
            cx.notify();
            return;
        };
        self.saved_searches.busy = true;
        let input = CreatePositionFilter {
            name: deleted.name,
            definition: deleted.definition,
        };
        let Some(task) = spawn_facade_write(cx, move || async move {
            facade.save_position_search(input).await
        }) else {
            self.saved_searches.busy = false;
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = weak.update(cx, |view, cx| {
                view.saved_searches.busy = false;
                let locale = current_locale(cx);
                match res {
                    Ok(restored) => {
                        view.saved_searches.upsert_saved(restored, locale);
                    }
                    Err(err) => {
                        view.saved_searches.record_error(locale, &err);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn open_save_search_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(definition) = self.draft.to_definition() else {
            self.invalid_filter = true;
            cx.notify();
            return;
        };
        let locale = current_locale(cx);
        let active = self.saved_searches.active().map(clone_saved_filter);
        let facade = self.facade.clone();
        let busy = ModalBusyLock::new(false);
        let parent = cx.entity();

        let dialog_view = cx.new(|cx| {
            SaveSearchDialogView::new(facade, definition, active, busy.clone(), parent, window, cx)
        });

        let body_view = dialog_view.clone();
        open_library_dialog(
            window,
            cx,
            I18n::t(locale, "position-browser", "saveSearchTitle"),
            busy,
            move |_, _| body_view.clone().into_any_element(),
            None::<fn(&mut Window, &mut App) -> gpui_kit::AnyElement>,
        );
    }

    fn open_deck_transfer_dialog(
        &mut self,
        target_ids: Vec<Uuid>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if target_ids.is_empty() {
            return;
        }
        let locale = current_locale(cx);
        let mut names = HashMap::new();
        for hit in &self.items {
            names.entry(hit.position.id).or_insert_with(|| {
                format!(
                    "{} · {} · {} · {}",
                    hit.database_name,
                    hit.display_name,
                    I18n::format(
                        locale,
                        "position-browser",
                        "game",
                        &[("number", Param::from(i64::from(hit.position.game_no)))],
                    ),
                    I18n::format(
                        locale,
                        "position-browser",
                        "ply",
                        &[("number", Param::from(i64::from(hit.position.ply)))],
                    ),
                )
            });
        }
        for meta in self.selected_cache.values() {
            names.entry(meta.position_id).or_insert_with(|| {
                format!(
                    "{} · {} · {} · {}",
                    meta.database_name,
                    meta.display_name,
                    I18n::format(
                        locale,
                        "position-browser",
                        "game",
                        &[("number", Param::from(i64::from(meta.game_no)))],
                    ),
                    I18n::format(
                        locale,
                        "position-browser",
                        "ply",
                        &[("number", Param::from(i64::from(meta.ply)))],
                    ),
                )
            });
        }
        let busy = ModalBusyLock::new(false);
        let parent = cx.entity();
        let facade = self.facade.clone();
        let dialog_view = cx.new(|cx| {
            PositionDeckTransferDialog::new(
                facade,
                target_ids,
                names,
                busy.clone(),
                parent,
                window,
                cx,
            )
        });
        let body_view = dialog_view.clone();
        open_library_dialog(
            window,
            cx,
            I18n::t(locale, "position-browser", "addToDeck"),
            busy,
            move |_, _| body_view.clone().into_any_element(),
            None::<fn(&mut Window, &mut App) -> gpui_kit::AnyElement>,
        );
    }

    fn emit_open_position(
        &self,
        match_id: Uuid,
        game_no: i32,
        position_id: Uuid,
        cx: &mut Context<Self>,
    ) {
        cx.emit(SavedPositionsEvent::OpenInAnalysis {
            match_id,
            game_no,
            position_id,
        });
    }

    fn open_position_by_id(&self, id: Uuid, cx: &mut Context<Self>) {
        if let Some(hit) = self.items.iter().find(|h| h.position.id == id) {
            self.emit_open_position(
                hit.position.match_uuid,
                hit.position.game_no,
                hit.position.id,
                cx,
            );
        } else if let Some(meta) = self.selected_cache.get(&id) {
            self.emit_open_position(meta.match_id, meta.game_no, meta.position_id, cx);
        }
    }

    fn handle_table_mouse_move(
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

    fn handle_table_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.resizing.take().is_some() {
            self.persist_table_prefs(cx);
            cx.notify();
        }
    }

    fn render_filter_controls(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ent = cx.entity();
        let pb = |k: &'static str| I18n::t(locale, "position-browser", k);
        let applied_draft = SavedFilterDraft::from_definition(&self.definition);
        let chips = applied_draft.build_chips(locale, &self.databases);

        // Database cycle/selector chips
        let db_missing = self
            .draft
            .database_id
            .is_some_and(|id| !self.databases.iter().any(|d| d.id == id));
        let mut db_bar = div()
            .h_flex()
            .flex_wrap()
            .items_center()
            .gap(px(4.0))
            .child({
                let ent_all = ent.clone();
                ActionButton::new("saved-db-all")
                    .label(pb("allDatabases"))
                    .active(self.draft.database_id.is_none())
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_all.update(cx, |v, cx| {
                            v.draft.database_id = None;
                            cx.notify();
                        });
                    })
            });
        for db in &self.databases {
            let db_id = db.id;
            let ent_db = ent.clone();
            db_bar = db_bar.child(
                ActionButton::new(SharedString::from(format!("saved-db-{db_id}")))
                    .label(db.name.clone())
                    .active(self.draft.database_id == Some(db_id))
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_db.update(cx, |v, cx| {
                            v.draft.database_id = Some(db_id);
                            cx.notify();
                        });
                    }),
            );
        }
        if db_missing && let Some(id) = self.draft.database_id {
            db_bar = db_bar.child(
                div()
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .bg(p.warning_bg)
                    .text_label()
                    .text_color(p.warning_text)
                    .child(format!("{} ({id})", pb("missing"))),
            );
        }

        // Saved searches strip
        let mut saved_bar = div()
            .id("saved-searches-bar")
            .test_support()
            .h_flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .text_label()
                    .text_color(p.muted)
                    .child(pb("savedSearches")),
            );

        for item in &self.saved_searches.items {
            let item_id = item.id;
            let ent_pick = ent.clone();
            let mut lbl = item.name.clone();
            if item.database_missing {
                lbl.push_str(&format!(" · {}", pb("missing")));
            }
            saved_bar = saved_bar.child(
                ActionButton::new(SharedString::from(format!("saved-search-{item_id}")))
                    .label(lbl)
                    .active(self.saved_searches.active_id == Some(item_id))
                    .disabled(self.saved_searches.busy)
                    .palette(p)
                    .on_click(move |_, window, cx| {
                        ent_pick.update(cx, |v, cx| {
                            let next = if v.saved_searches.active_id == Some(item_id) {
                                None
                            } else {
                                Some(item_id)
                            };
                            v.choose_saved_search(next, window, cx);
                        });
                    }),
            );
        }

        saved_bar = saved_bar
            .child(
                ActionButton::new("saved-search-save-btn")
                    .icon(ActionIconName::Save)
                    .label(pb("saveCurrent"))
                    .disabled(self.saved_searches.busy)
                    .palette(p)
                    .on_click({
                        let ent_save = ent.clone();
                        move |_, window, cx| {
                            ent_save.update(cx, |v, cx| v.open_save_search_dialog(window, cx));
                        }
                    }),
            )
            .child(
                ActionButton::new("saved-search-delete-btn")
                    .icon(ActionIconName::Delete)
                    .label(pb("deleteSearch"))
                    .danger()
                    .disabled(self.saved_searches.active_id.is_none() || self.saved_searches.busy)
                    .palette(p)
                    .on_click({
                        let ent_del = ent.clone();
                        move |_, _, cx| {
                            ent_del.update(cx, |v, cx| v.delete_active_saved_search(cx));
                        }
                    }),
            )
            .child(
                ActionButton::new("saved-search-reload-btn")
                    .icon(ActionIconName::Refresh)
                    .tooltip(pb("reload"))
                    .compact()
                    .disabled(self.saved_searches.busy)
                    .palette(p)
                    .on_click({
                        let ent_rel = ent.clone();
                        move |_, _, cx| {
                            ent_rel.update(cx, |v, cx| v.load_metadata(cx));
                        }
                    }),
            );

        // Top primary search row
        let search_row = div()
            .h_flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .w(px(200.0))
                    .child(Input::new(&self.inputs.q).id("saved-filter-q")),
            )
            .child(
                div()
                    .w(px(160.0))
                    .child(Input::new(&self.inputs.player).id("saved-filter-player")),
            )
            .child(
                ActionButton::new("saved-filter-search-btn")
                    .icon(ActionIconName::Search)
                    .label(pb("search"))
                    .primary()
                    .disabled(self.loading)
                    .palette(p)
                    .on_click({
                        let ent_s = ent.clone();
                        move |_, _, cx| ent_s.update(cx, |v, cx| v.apply_draft(cx))
                    }),
            )
            .child(
                ActionButton::new("saved-filter-toggle-more")
                    .icon(ActionIconName::Filter)
                    .label(pb("filters"))
                    .active(self.filters_expanded)
                    .palette(p)
                    .on_click({
                        let ent_t = ent.clone();
                        move |_, _, cx| {
                            ent_t.update(cx, |v, cx| {
                                v.filters_expanded = !v.filters_expanded;
                                cx.notify();
                            });
                        }
                    }),
            )
            .child(
                ActionButton::new("saved-filter-clear-btn")
                    .icon(ActionIconName::Clear)
                    .tooltip(pb("clear"))
                    .compact()
                    .palette(p)
                    .on_click({
                        let ent_c = ent.clone();
                        move |_, window, cx| {
                            ent_c.update(cx, |v, cx| v.clear_filters(window, cx));
                        }
                    }),
            );

        // Applied filter chips strip
        let chip_strip = AppliedFilterChips::new("saved-applied-filter-chips", chips)
            .clear_all_label(pb("clear"))
            .palette(p)
            .on_remove({
                let ent_r = ent.clone();
                move |key, window, cx| {
                    ent_r.update(cx, |v, cx| v.remove_filter_chip(key, window, cx));
                }
            })
            .on_clear_all({
                let ent_c = ent.clone();
                move |window, cx| ent_c.update(cx, |v, cx| v.clear_filters(window, cx))
            });

        // 4 Filter Groups when expanded
        let groups_panel = self.filters_expanded.then(|| {
            let group_card = |id: &'static str, title: &'static str| {
                div()
                    .id(id)
                    .test_support()
                    .v_flex()
                    .gap(px(8.0))
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(
                        div()
                            .text_label()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text)
                            .child(title),
                    )
            };

            let field_box = |label: &'static str, child: gpui_kit::AnyElement| {
                div()
                    .v_flex()
                    .gap(px(3.0))
                    .child(div().text_meta().text_color(p.muted).child(label))
                    .child(child)
            };

            // Group 1: Match
            let g_match = group_card("saved-filter-group-match", pb("groupMatch")).child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(field_box(
                        pb("matchName"),
                        div()
                            .w(px(150.0))
                            .child(Input::new(&self.inputs.match_name).id("sf-match-name"))
                            .into_any_element(),
                    ))
                    .child(field_box(
                        pb("tournament"),
                        div()
                            .w(px(140.0))
                            .child(Input::new(&self.inputs.tournament).id("sf-tournament"))
                            .into_any_element(),
                    ))
                    .child(field_box(
                        pb("round"),
                        div()
                            .w(px(90.0))
                            .child(Input::new(&self.inputs.round).id("sf-round"))
                            .into_any_element(),
                    ))
                    .child(field_box(
                        pb("minPoints"),
                        div()
                            .w(px(80.0))
                            .child(Input::new(&self.inputs.match_points_min).id("sf-pts-min"))
                            .into_any_element(),
                    ))
                    .child(field_box(
                        pb("maxPoints"),
                        div()
                            .w(px(80.0))
                            .child(Input::new(&self.inputs.match_points_max).id("sf-pts-max"))
                            .into_any_element(),
                    )),
            );

            // Group 2: Position (board/match context)
            let mut plan_row = div().h_flex().flex_wrap().gap(px(4.0)).child({
                let ent_p = ent.clone();
                ActionButton::new("sf-plan-any")
                    .label(pb("any"))
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
                    ActionButton::new(SharedString::from(format!("sf-plan-{plan:?}")))
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

            let mut crawford_row = div().h_flex().gap(px(4.0));
            for (opt, key) in [(None, "any"), (Some(true), "yes"), (Some(false), "no")] {
                let ent_c = ent.clone();
                crawford_row = crawford_row.child(
                    ActionButton::new(SharedString::from(format!("sf-crawford-{key}")))
                        .label(pb(key))
                        .active(self.draft.crawford == opt)
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent_c.update(cx, |v, cx| {
                                v.draft.crawford = opt;
                                cx.notify();
                            });
                        }),
                );
            }

            let g_position = group_card("saved-filter-group-position", pb("groupPosition"))
                .child(field_box(
                    I18n::t(locale, "game-plan", "title"),
                    plan_row.into_any_element(),
                ))
                .child(
                    div()
                        .h_flex()
                        .flex_wrap()
                        .gap(px(8.0))
                        .child(field_box(
                            pb("die1"),
                            div()
                                .w(px(64.0))
                                .child(Input::new(&self.inputs.die1).id("sf-die1"))
                                .into_any_element(),
                        ))
                        .child(field_box(
                            pb("die2"),
                            div()
                                .w(px(64.0))
                                .child(Input::new(&self.inputs.die2).id("sf-die2"))
                                .into_any_element(),
                        ))
                        .child(field_box(
                            pb("whiteScore"),
                            div()
                                .w(px(84.0))
                                .child(Input::new(&self.inputs.white_score).id("sf-w-score"))
                                .into_any_element(),
                        ))
                        .child(field_box(
                            pb("blackScore"),
                            div()
                                .w(px(84.0))
                                .child(Input::new(&self.inputs.black_score).id("sf-b-score"))
                                .into_any_element(),
                        ))
                        .child(field_box(pb("crawford"), crawford_row.into_any_element()))
                        .child(field_box(
                            pb("cube"),
                            div()
                                .w(px(80.0))
                                .child(Input::new(&self.inputs.cube_value).id("sf-cube"))
                                .into_any_element(),
                        ))
                        .child(field_box(
                            pb("checkerId"),
                            div()
                                .w(px(150.0))
                                .child(Input::new(&self.inputs.position_id).id("sf-pos-id"))
                                .into_any_element(),
                        )),
                );

            // Group 3: Analysis (severity/loss + local analysis readiness)
            let mut analysis_row = div().h_flex().gap(px(4.0));
            for (opt, key) in [
                (None, "any"),
                (Some(PositionAnalysisState::Ready), "ready"),
                (Some(PositionAnalysisState::Missing), "missing"),
            ] {
                let ent_a = ent.clone();
                analysis_row = analysis_row.child(
                    ActionButton::new(SharedString::from(format!("sf-analysis-{key}")))
                        .label(pb(key))
                        .active(self.draft.analysis_state == opt)
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent_a.update(cx, |v, cx| {
                                v.draft.analysis_state = opt;
                                cx.notify();
                            });
                        }),
                );
            }

            let g_analysis = group_card("saved-filter-group-analysis", pb("groupAnalysis")).child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .items_end()
                    .gap(px(8.0))
                    .child(field_box(pb("analysis"), analysis_row.into_any_element()))
                    .child(field_box(
                        pb("minLoss"),
                        self.inputs.min_loss.clone().into_any_element(),
                    ))
                    .child(field_box(
                        pb("maxLoss"),
                        self.inputs.max_loss.clone().into_any_element(),
                    )),
            );

            // Group 4: Tags & Comments
            let g_notes = group_card("saved-filter-group-notes", pb("groupNotes")).child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(field_box(
                        pb("tag"),
                        div()
                            .w(px(150.0))
                            .child(Input::new(&self.inputs.tag).id("sf-tag"))
                            .into_any_element(),
                    ))
                    .child(field_box(
                        pb("comment"),
                        div()
                            .w(px(220.0))
                            .child(Input::new(&self.inputs.comment).id("sf-comment"))
                            .into_any_element(),
                    )),
            );

            div()
                .v_flex()
                .gap(px(8.0))
                .child(g_match)
                .child(g_position)
                .child(
                    div()
                        .h_flex()
                        .flex_wrap()
                        .gap(px(8.0))
                        .child(div().flex_1().min_w(px(280.0)).child(g_analysis))
                        .child(div().flex_1().min_w(px(280.0)).child(g_notes)),
                )
        });

        let mut container = div()
            .v_flex()
            .gap(px(8.0))
            .p(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(search_row)
            .child(db_bar)
            .child(saved_bar)
            .child(chip_strip);

        if self.invalid_filter {
            container = container.child(
                StatusBanner::new(
                    "saved-invalid-filter-banner",
                    BannerKind::Error,
                    pb("invalidFilter"),
                )
                .palette(p),
            );
        }

        if let Some(active) = self.saved_searches.active()
            && active.database_missing
        {
            container = container.child(
                StatusBanner::new(
                    "saved-search-missing-db",
                    BannerKind::Warning,
                    pb("databaseMissing"),
                )
                .palette(p),
            );
        }

        if self.saved_searches.is_dirty(&self.draft) {
            container = container.child(
                div()
                    .text_meta()
                    .text_color(p.warning_text)
                    .child(pb("dirty")),
            );
        }

        // Explicit 409 Revision Conflict Banner with Reload trigger
        if let Some(conflict) = self.saved_searches.conflict_error.clone() {
            let ent_rel = ent.clone();
            container = container.child(
                StatusBanner::new("saved-search-conflict-banner", BannerKind::Error, conflict)
                    .palette(p)
                    .on_retry(pb("reload"), move |_, _, cx| {
                        ent_rel.update(cx, |v, cx| v.load_metadata(cx));
                    }),
            );
        } else if let Some(err) = self.saved_searches.error.clone() {
            container = container.child(
                StatusBanner::new("saved-search-error-banner", BannerKind::Error, err).palette(p),
            );
        }

        if let Some(notice) = self.saved_searches.notice.clone() {
            let mut banner =
                StatusBanner::new("saved-search-notice-banner", BannerKind::Info, notice)
                    .palette(p);
            if self.saved_searches.deleted_undo.is_some() {
                let ent_u = ent.clone();
                banner = banner.on_retry(pb("undo"), move |_, _, cx| {
                    ent_u.update(cx, |v, cx| v.undo_delete_saved_search(cx));
                });
            }
            container = container.child(banner);
        }

        container.when_some(groups_panel, |this, panel| this.child(panel))
    }

    fn render_table_section(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ent = cx.entity();
        let pb = |k: &'static str| I18n::t(locale, "position-browser", k);
        let lib = |k: &'static str| I18n::t(locale, "library", k);

        let page_ids: Vec<Uuid> = self.items.iter().map(|h| h.position.id).collect();
        let all_on_page_selected =
            !page_ids.is_empty() && page_ids.iter().all(|id| self.selected_ids.contains(id));
        let sel_count = self.selected_ids.len();
        let sort_dir = effective_sort_direction(self.definition.sort, self.definition.direction);
        let active_cols = self.table_layout.active_columns();

        // Toolbar above table
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
                    .child(div().text_label().tabular_nums().text_color(p.muted).child(
                        I18n::format(
                            locale,
                            "position-browser",
                            "resultCount",
                            &[("count", Param::from(self.total))],
                        ),
                    ))
                    .when(sel_count > 0, |this| {
                        let ent_add = ent.clone();
                        let ent_rm = ent.clone();
                        let ent_clr = ent.clone();
                        let sel_ids_add = self.selected_ids.clone();
                        let sel_ids_rm = self.selected_ids.clone();
                        this.child(
                            div()
                                .text_label()
                                .tabular_nums()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.accent)
                                .child(I18n::format(
                                    locale,
                                    "position-browser",
                                    "selected",
                                    &[("count", Param::from(sel_count as i64))],
                                )),
                        )
                        .child(
                            ActionButton::new("saved-batch-add-deck")
                                .icon(ActionIconName::Deck)
                                .label(pb("addToDeck"))
                                .primary()
                                .disabled(self.removing || self.restoring)
                                .palette(p)
                                .on_click(move |_, window, cx| {
                                    ent_add.update(cx, |v, cx| {
                                        v.open_deck_transfer_dialog(
                                            sel_ids_add.clone(),
                                            window,
                                            cx,
                                        );
                                    });
                                }),
                        )
                        .child(
                            ActionButton::new("saved-batch-remove")
                                .icon(ActionIconName::Delete)
                                .label(if self.removing {
                                    pb("removing")
                                } else {
                                    pb("removeSaved")
                                })
                                .danger()
                                .disabled(self.removing || self.restoring)
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    ent_rm
                                        .update(cx, |v, cx| v.batch_remove(sel_ids_rm.clone(), cx));
                                }),
                        )
                        .child(
                            ActionButton::new("saved-clear-selection")
                                .icon(ActionIconName::Clear)
                                .label(pb("clearSelection"))
                                .variant(ActionButtonVariant::Ghost)
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    ent_clr.update(cx, |v, cx| v.clear_selection(cx));
                                }),
                        )
                    }),
            )
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.0))
                    .child({
                        let ent_sp = ent.clone();
                        let next_show = !self.table_layout.show_played;
                        ActionButton::new("saved-show-played-toggle")
                            .icon(ActionIconName::View)
                            .label(pb("showPlayed"))
                            .active(self.table_layout.show_played)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_sp.update(cx, |v, cx| v.set_show_played(next_show, cx));
                            })
                    })
                    .when(
                        !self.table_layout.is_sort_visible(self.definition.sort),
                        |this| {
                            let dir_label = lib(match sort_dir {
                                PositionBrowserDirection::Asc => "ascending",
                                PositionBrowserDirection::Desc => "descending",
                            });
                            this.child(div().text_meta().text_color(p.warning_text).child(
                                I18n::format(
                                    locale,
                                    "library",
                                    "sortedBy",
                                    &[
                                        (
                                            "column",
                                            Param::from(sort_field_label(
                                                locale,
                                                self.definition.sort,
                                            )),
                                        ),
                                        ("direction", Param::from(dir_label)),
                                    ],
                                ),
                            ))
                        },
                    )
                    .child({
                        let ent_cp = ent.clone();
                        ActionButton::new("saved-column-picker-btn")
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
                        ActionButton::new("saved-reset-widths-btn")
                            .icon(ActionIconName::Reset)
                            .tooltip(lib("resetColumnWidths"))
                            .compact()
                            .disabled(!self.table_layout.has_custom_widths())
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_rw.update(cx, |v, cx| v.reset_column_widths(cx));
                            })
                    }),
            );

        // Column picker drawer (all 12 data columns)
        let picker_drawer = self.column_picker_open.then(|| {
            let mut row = div()
                .id("saved-column-picker-panel")
                .test_support()
                .h_flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.0))
                .p(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface_alt);

            for &dc in SavedDataColumn::ALL {
                let checked = self.table_layout.visible_columns.contains(&dc);
                let ent_t = ent.clone();
                row = row.child(
                    ActionButton::new(SharedString::from(format!("saved-col-opt-{}", dc.id())))
                        .label(data_column_label(locale, dc))
                        .active(checked)
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            ent_t.update(cx, |v, cx| v.toggle_column_visibility(dc, !checked, cx));
                        }),
                );
            }
            row.child({
                let ent_all = ent.clone();
                ActionButton::new("saved-col-show-all")
                    .label(lib("showAllColumns"))
                    .variant(ActionButtonVariant::Ghost)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_all.update(cx, |v, cx| {
                            v.table_layout.show_all_columns();
                            v.persist_table_prefs(cx);
                            cx.notify();
                        });
                    })
            })
            .child({
                let ent_def = ent.clone();
                ActionButton::new("saved-col-reset-defaults")
                    .label(lib("resetColumns"))
                    .variant(ActionButtonVariant::Ghost)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_def.update(cx, |v, cx| {
                            v.table_layout.reset_visible_columns();
                            v.persist_table_prefs(cx);
                            cx.notify();
                        });
                    })
            })
        });

        // Header row
        let mut header_row = div()
            .h_flex()
            .items_center()
            .border_b_1()
            .border_color(p.border)
            .bg(p.surface_alt)
            .py(px(6.0));

        for &col in &active_cols {
            let w = px(self.table_layout.width_for(col) as f32);
            let mut cell = div()
                .h_flex()
                .items_center()
                .justify_between()
                .w(w)
                .flex_shrink_0()
                .px(px(6.0))
                .text_table_heading()
                .text_color(p.muted);

            if col == SavedPositionColumn::Select {
                let ent_sel = ent.clone();
                let ids_for_page = page_ids.clone();
                cell = cell.justify_center().child(
                    div()
                        .id("saved-select-page-checkbox")
                        .test_support()
                        .w(px(16.0))
                        .h(px(16.0))
                        .rounded(px(3.0))
                        .border_1()
                        .border_color(if all_on_page_selected {
                            p.accent
                        } else {
                            p.control_border
                        })
                        .bg(if all_on_page_selected {
                            p.accent
                        } else {
                            p.surface
                        })
                        .h_flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            ent_sel.update(cx, |v, cx| {
                                v.toggle_selection(&ids_for_page, !all_on_page_selected, cx);
                            });
                        })
                        .when(all_on_page_selected, |this| {
                            this.child(action_icon(ActionIconName::Confirm, 12.0, p.accent_text))
                        }),
                );
            } else if col == SavedPositionColumn::Actions {
                cell = cell.child("");
            } else {
                let mut sort_buttons = div()
                    .h_flex()
                    .items_center()
                    .gap(px(4.0))
                    .flex_1()
                    .min_w(px(0.0));
                for (i, &sort_key) in col.sort_keys().iter().enumerate() {
                    if i > 0 {
                        sort_buttons = sort_buttons.child(div().text_color(p.muted).child("/"));
                    }
                    let is_active_sort = self.definition.sort == sort_key;
                    let ent_sort = ent.clone();
                    sort_buttons = sort_buttons.child(
                        div()
                            .id(SharedString::from(format!("saved-sort-{sort_key:?}")))
                            .test_support()
                            .h_flex()
                            .items_center()
                            .gap(px(3.0))
                            .cursor_pointer()
                            .text_color(if is_active_sort { p.accent } else { p.muted })
                            .on_click(move |_, _, cx| {
                                ent_sort.update(cx, |v, cx| v.apply_sort(sort_key, cx));
                            })
                            .child(sort_field_label(locale, sort_key))
                            .when(is_active_sort, |this| {
                                let icon = match sort_dir {
                                    PositionBrowserDirection::Asc => ActionIconName::SortUp,
                                    PositionBrowserDirection::Desc => ActionIconName::SortDown,
                                };
                                this.child(action_icon(icon, 12.0, p.accent))
                            }),
                    );
                }

                // Mouse-drag resize handle on column right border
                let ent_drag = ent.clone();
                let start_w = self.table_layout.width_for(col);
                let resize_handle = div()
                    .id(SharedString::from(format!("saved-resize-{}", col.id())))
                    .w(px(6.0))
                    .h(px(18.0))
                    .flex_shrink_0()
                    .cursor(gpui_kit::CursorStyle::ResizeLeftRight)
                    .hover(move |s| s.bg(p.accent))
                    .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                        ent_drag.update(cx, |v, _| {
                            v.resizing = Some(ColumnResizeDrag {
                                column: col,
                                start_x: f32::from(event.position.x),
                                start_width: start_w,
                            });
                        });
                    });

                cell = cell.child(sort_buttons).child(resize_handle);
            }

            header_row = header_row.child(cell);
        }

        // Data rows
        let active_preview_id = self.active_preview().map(|h| h.position.id);
        let mut rows_col = div().v_flex();
        for hit in &self.items {
            let hit_id = hit.position.id;
            let is_selected = self.selected_ids.contains(&hit_id);
            let is_preview = active_preview_id == Some(hit_id);
            let ent_prev = ent.clone();

            let mut row = div()
                .id(SharedString::from(format!("saved-row-{hit_id}")))
                .test_support()
                .h_flex()
                .items_center()
                .min_h(px(34.0))
                .py(px(4.0))
                .border_b_1()
                .border_color(p.border)
                .bg(if is_preview { p.selected } else { p.surface })
                .cursor_pointer()
                .hover(move |s| {
                    s.bg(if is_preview {
                        p.selected
                    } else {
                        p.surface_alt
                    })
                })
                .on_click(move |_, _, cx| {
                    ent_prev.update(cx, |v, cx| {
                        v.preview_id = Some(hit_id);
                        v.sync_preview_board(cx);
                        cx.notify();
                    });
                });

            for &col in &active_cols {
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
                    SavedPositionColumn::Select => {
                        let ent_chk = ent.clone();
                        cell = cell.h_flex().justify_center().child(
                            div()
                                .id(SharedString::from(format!("saved-chk-{hit_id}")))
                                .test_support()
                                .w(px(16.0))
                                .h(px(16.0))
                                .rounded(px(3.0))
                                .border_1()
                                .border_color(if is_selected {
                                    p.accent
                                } else {
                                    p.control_border
                                })
                                .bg(if is_selected { p.accent } else { p.surface })
                                .h_flex()
                                .items_center()
                                .justify_center()
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    ent_chk.update(cx, |v, cx| {
                                        v.toggle_selection(&[hit_id], !is_selected, cx);
                                    });
                                })
                                .when(is_selected, |this| {
                                    this.child(action_icon(
                                        ActionIconName::Confirm,
                                        12.0,
                                        p.accent_text,
                                    ))
                                }),
                        );
                    }
                    SavedPositionColumn::Origin => {
                        let label = format!("#{} · {}", hit.match_no, hit.display_name);
                        cell = cell.child(label);
                    }
                    SavedPositionColumn::Database => {
                        cell = cell.child(hit.database_name.clone());
                    }
                    SavedPositionColumn::Game => {
                        cell = cell.child(format_int(locale, i64::from(hit.position.game_no)));
                    }
                    SavedPositionColumn::Ply => {
                        cell = cell.child(format_int(locale, i64::from(hit.position.ply)));
                    }
                    SavedPositionColumn::Player => {
                        let name = if hit.position.player == 0 {
                            &hit.match_info.white
                        } else {
                            &hit.match_info.black
                        };
                        cell = cell.child(name.clone());
                    }
                    SavedPositionColumn::Opponent => {
                        let name = if hit.position.player == 0 {
                            &hit.match_info.black
                        } else {
                            &hit.match_info.white
                        };
                        cell = cell.child(name.clone());
                    }
                    SavedPositionColumn::Dice => {
                        let dice_str = if hit.position.dice.len() >= 2 {
                            format!("{}-{}", hit.position.dice[0], hit.position.dice[1])
                        } else {
                            "—".into()
                        };
                        cell = cell.child(dice_str);
                    }
                    SavedPositionColumn::Score => {
                        let score = match (hit.white_score, hit.black_score) {
                            (Some(w), Some(b)) => format!(
                                "{w}–{b}{}",
                                if hit.crawford == Some(true) { " C" } else { "" }
                            ),
                            _ => "—".into(),
                        };
                        cell = cell.child(score);
                    }
                    SavedPositionColumn::Cube => {
                        let cube = hit
                            .cube_value
                            .map(|v| format_int(locale, i64::from(v)))
                            .unwrap_or_else(|| "—".into());
                        cell = cell.child(cube);
                    }
                    SavedPositionColumn::PlayedMove => {
                        let mv = if hit.position.notation.trim().is_empty() {
                            "—".into()
                        } else {
                            localize_decision(locale, &hit.position.notation)
                        };
                        cell = cell.child(mv);
                    }
                    SavedPositionColumn::Loss => {
                        if let Some(ne) = &hit.native_error {
                            let sev = SeverityStyle::resolve(ne.severity, &p);
                            cell = cell.h_flex().items_center().gap(px(4.0)).child(
                                div()
                                    .px(px(5.0))
                                    .py(px(1.0))
                                    .rounded(px(4.0))
                                    .bg(sev.bg)
                                    .text_color(sev.fg)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format_decimal(locale, ne.equity_loss, 3, 3)),
                            );
                        } else {
                            cell = cell.text_color(p.muted).child(pb("nativeUnknown"));
                        }
                    }
                    SavedPositionColumn::Decks => {
                        let txt = if hit.decks.is_empty() {
                            "—".into()
                        } else {
                            hit.decks
                                .iter()
                                .map(|d| d.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        };
                        cell = cell.child(txt);
                    }
                    SavedPositionColumn::Notes => {
                        let mut parts = Vec::new();
                        if !hit.tags.is_empty() {
                            parts.push(hit.tags.join(", "));
                        }
                        if let Some(note) = hit
                            .notes
                            .iter()
                            .find(|n| !n.comment_excerpt.trim().is_empty())
                        {
                            parts.push(note.comment_excerpt.clone());
                        }
                        let txt = if parts.is_empty() {
                            "—".into()
                        } else {
                            parts.join(" · ")
                        };
                        cell = cell.child(txt);
                    }
                    SavedPositionColumn::Actions => {
                        let ent_open = ent.clone();
                        let match_id = hit.position.match_uuid;
                        let game_no = hit.position.game_no;
                        cell = cell.h_flex().gap(px(4.0)).child(
                            ActionButton::new(SharedString::from(format!("saved-open-{hit_id}")))
                                .icon(ActionIconName::Analysis)
                                .tooltip(pb("openAnalysis"))
                                .dense_nav()
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    ent_open.update(cx, |v, cx| {
                                        v.emit_open_position(match_id, game_no, hit_id, cx);
                                    });
                                }),
                        );
                    }
                }
                row = row.child(cell);
            }
            rows_col = rows_col.child(row);
        }

        let last_page = ((self.total.max(0) as usize).div_ceil(SAVED_PAGE_SIZE as usize)).max(1);
        let ent_page = ent.clone();

        let mut wrapper = div()
            .v_flex()
            .gap(px(8.0))
            .flex_1()
            .min_w(px(0.0))
            .on_mouse_move(cx.listener(Self::handle_table_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::handle_table_mouse_up))
            .child(toolbar)
            .when_some(picker_drawer, |this, drawer| this.child(drawer));

        // Undo remove banner
        if !self.last_removed_ids.is_empty() {
            let count = self.last_removed_ids.len() as i64;
            let ent_undo = ent.clone();
            let ent_dis = ent.clone();
            wrapper = wrapper.child(
                StatusBanner::new(
                    "saved-undo-remove-banner",
                    BannerKind::Info,
                    I18n::format(
                        locale,
                        "position-browser",
                        "removedNotice",
                        &[("count", Param::from(count))],
                    ),
                )
                .palette(p)
                .on_retry(
                    if self.restoring {
                        pb("restoring")
                    } else {
                        pb("undo")
                    },
                    move |_, _, cx| ent_undo.update(cx, |v, cx| v.undo_last_remove(cx)),
                )
                .on_dismiss(move |_, _, cx| {
                    ent_dis.update(cx, |v, cx| {
                        v.last_removed_ids.clear();
                        cx.notify();
                    });
                }),
            );
        }

        if let Some(err) = self.remove_error.clone() {
            wrapper = wrapper.child(
                StatusBanner::new("saved-remove-error-banner", BannerKind::Error, err).palette(p),
            );
        }

        if let Some((result, names)) = &self.transfer_outcome {
            let mut outcome_box = div()
                .id("saved-transfer-outcomes")
                .test_support()
                .v_flex()
                .gap(px(4.0))
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
                        .child(div().text_panel().text_color(p.text).child(pb("outcome")))
                        .child({
                            let ent_cls = ent.clone();
                            ActionButton::new("saved-outcome-dismiss")
                                .icon(ActionIconName::Close)
                                .dense_nav()
                                .ghost()
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    ent_cls.update(cx, |v, cx| {
                                        v.transfer_outcome = None;
                                        cx.notify();
                                    });
                                })
                        }),
                );

            for (idx, item) in result.items.iter().enumerate() {
                let status_key = match item.status {
                    PositionAddStatus::Added => "added",
                    PositionAddStatus::Updated => "updated",
                    PositionAddStatus::Duplicate => "duplicate",
                    PositionAddStatus::AnalysisRequired => "analysis_required",
                    PositionAddStatus::NotFound => "not_found",
                };
                let label = names
                    .get(&item.position_id)
                    .cloned()
                    .unwrap_or_else(|| item.position_id.to_string());
                let pid = item.position_id;
                let ent_op = ent.clone();
                outcome_box = outcome_box.child(
                    div()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .text_table()
                        .child(div().flex_1().truncate().text_color(p.text).child(label))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.muted)
                                .child(pb(status_key)),
                        )
                        .when(item.status == PositionAddStatus::AnalysisRequired, |this| {
                            this.child(
                                ActionButton::new(SharedString::from(format!(
                                    "saved-outcome-open-{idx}"
                                )))
                                .icon(ActionIconName::Analysis)
                                .label(pb("openAnalysis"))
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    ent_op.update(cx, |v, cx| v.open_position_by_id(pid, cx));
                                }),
                            )
                        }),
                );
            }
            wrapper = wrapper.child(outcome_box);
        }

        if let Some(err) = self.load_error.clone() {
            let ent_retry = ent.clone();
            let cur_page = self.page;
            wrapper = wrapper.child(
                StatusBanner::new("saved-load-error", BannerKind::Error, err)
                    .palette(p)
                    .on_retry(pb("retrySimilar"), move |_, _, cx| {
                        ent_retry.update(cx, |v, cx| v.load_page(cur_page, cx));
                    }),
            );
        }

        if self.items.is_empty() && !self.loading {
            let has_filters = !SavedFilterDraft::from_definition(&self.definition)
                .build_chips(locale, &self.databases)
                .is_empty();
            wrapper = wrapper.child(
                EmptyState::new(
                    "saved-positions-empty",
                    pb("title"),
                    if has_filters {
                        pb("noFilterResults")
                    } else {
                        pb("empty")
                    },
                )
                .icon(ActionIconName::Bookmark)
                .palette(p),
            );
        } else {
            wrapper = wrapper
                .child(
                    div()
                        .id("saved-positions-table-scroll")
                        .test_support()
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
                    div()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_meta()
                                .text_color(p.muted)
                                .child(pb("selectionLimit")),
                        )
                        .child(
                            NumberedSearchPagination::new(
                                "saved-positions-pagination",
                                self.page.max(1) as usize,
                                last_page,
                            )
                            .disabled(self.loading)
                            .palette(p)
                            .on_select(move |target, _, cx| {
                                ent_page.update(cx, |v, cx| v.load_page(target as i64, cx));
                            }),
                        ),
                );
        }

        wrapper
    }

    fn render_preview_pane(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let pb = |k: &'static str| I18n::t(locale, "position-browser", k);
        let ent = cx.entity();

        let Some(hit) = self.active_preview() else {
            return div()
                .id("saved-preview-empty")
                .w(px(420.0))
                .flex_shrink_0()
                .p(px(14.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .child(
                    div()
                        .text_body()
                        .text_color(p.muted)
                        .child(pb("previewHint")),
                )
                .into_any_element();
        };

        let hit_match_id = hit.position.match_uuid;
        let hit_game_no = hit.position.game_no;
        let hit_id = hit.position.id;
        let hit_pos = hit.position.clone();
        let hit_match_name = hit.display_name.clone();
        let ent_open = ent.clone();
        let ent_sim = ent.clone();
        let ent_deck = ent.clone();
        let ent_rm = ent.clone();
        let ent_det = ent.clone();

        let mut pane = div()
            .id("saved-positions-preview-pane")
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
                    .h_flex()
                    .items_start()
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
                                    .child(hit.display_name.clone()),
                            )
                            .child(div().text_meta().text_color(p.muted).child(format!(
                                "{} · {} · {}",
                                hit.database_name,
                                I18n::format(
                                    locale,
                                    "position-browser",
                                    "game",
                                    &[("number", Param::from(i64::from(hit.position.game_no)))],
                                ),
                                I18n::format(
                                    locale,
                                    "position-browser",
                                    "ply",
                                    &[("number", Param::from(i64::from(hit.position.ply)))],
                                ),
                            ))),
                    )
                    .when(!hit.source_available, |this| {
                        this.child(
                            div()
                                .id("saved-retained-badge")
                                .test_support()
                                .px(px(7.0))
                                .py(px(2.0))
                                .rounded(px(4.0))
                                .bg(p.warning_bg)
                                .text_meta()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.warning_text)
                                .child("Retained"),
                        )
                    }),
            )
            // Board preview
            .child(
                div()
                    .w_full()
                    .h(px(270.0))
                    .child(self.preview_board.clone()),
            )
            // Primary actions
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .gap(px(6.0))
                    .child(
                        ActionButton::new("saved-preview-open-analysis")
                            .icon(ActionIconName::Analysis)
                            .label(pb("openAnalysis"))
                            .primary()
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_open.update(cx, |v, cx| {
                                    v.emit_open_position(hit_match_id, hit_game_no, hit_id, cx);
                                });
                            }),
                    )
                    .child(
                        ActionButton::new("saved-preview-find-similar")
                            .icon(ActionIconName::Similar)
                            .label(pb("findSimilar"))
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                let pos = hit_pos.clone();
                                let name = hit_match_name.clone();
                                ent_sim.update(cx, |_, cx| {
                                    cx.emit(SavedPositionsEvent::FindSimilar {
                                        position: pos,
                                        match_name: name,
                                    });
                                });
                            }),
                    )
                    .child(
                        ActionButton::new("saved-preview-add-deck")
                            .icon(ActionIconName::Deck)
                            .label(pb("addToDeck"))
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent_deck.update(cx, |v, cx| {
                                    v.open_deck_transfer_dialog(vec![hit_id], window, cx);
                                });
                            }),
                    )
                    .child(
                        ActionButton::new("saved-preview-remove")
                            .icon(ActionIconName::Unbookmark)
                            .tooltip(pb("removeSaved"))
                            .danger()
                            .compact()
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_rm.update(cx, |v, cx| v.batch_remove(vec![hit_id], cx));
                            }),
                    ),
            );

        // Played move (when enabled) + Native equity loss
        let mut stats = div().v_flex().gap(px(4.0));
        if self.table_layout.show_played && !hit.position.notation.trim().is_empty() {
            stats = stats.child(div().text_body().text_color(p.text).child(format!(
                "{}: {}",
                pb("playedMove"),
                localize_decision(locale, &hit.position.notation)
            )));
        }
        if let Some(ne) = &hit.native_error {
            let sev = SeverityStyle::resolve(ne.severity, &p);
            let prov = match ne.plies {
                Some(pl) => format!(
                    "{} · {}",
                    ne.engine,
                    I18n::format(
                        locale,
                        "position-browser",
                        "depth",
                        &[("plies", Param::from(i64::from(pl)))],
                    )
                ),
                None => ne.engine.clone(),
            };
            stats = stats.child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_label()
                            .text_color(p.muted)
                            .child(pb("nativeError")),
                    )
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
                                "{} · {}",
                                format_decimal(locale, ne.equity_loss, 3, 3),
                                sev.label(locale)
                            )),
                    )
                    .child(div().text_meta().text_color(p.muted).child(prov)),
            );
        } else {
            stats = stats.child(div().text_meta().text_color(p.muted).child(pb("needed")));
        }
        pane = pane.child(stats);

        // Decks membership
        let decks_line = if hit.decks.is_empty() {
            pb("noDecks").to_string()
        } else {
            hit.decks
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        pane = pane.child(
            div()
                .text_meta()
                .text_color(p.muted)
                .child(format!("{}: {decks_line}", pb("decks"))),
        );

        // Tags & notes
        if !hit.tags.is_empty() || !hit.notes.is_empty() {
            let mut notes_box = div()
                .v_flex()
                .gap(px(4.0))
                .pt(px(6.0))
                .border_t_1()
                .border_color(p.border);
            if !hit.tags.is_empty() {
                let mut tag_row = div().h_flex().flex_wrap().gap(px(4.0));
                for tag in &hit.tags {
                    tag_row = tag_row.child(
                        div()
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .bg(p.surface_alt)
                            .text_meta()
                            .text_color(p.text)
                            .child(tag.clone()),
                    );
                }
                notes_box = notes_box.child(tag_row);
            }
            for note in hit
                .notes
                .iter()
                .filter(|n| !n.comment_excerpt.trim().is_empty())
            {
                notes_box = notes_box.child(
                    div()
                        .text_meta()
                        .text_color(p.text)
                        .child(format!("[{}] {}", note.scope, note.comment_excerpt)),
                );
            }
            pane = pane.child(notes_box);
        }

        // Collapsible source & position metadata
        pane = pane
            .child(
                ActionButton::new("saved-preview-details-toggle")
                    .icon(if self.preview_details_expanded {
                        ActionIconName::Collapse
                    } else {
                        ActionIconName::Expand
                    })
                    .label(pb("previewDetails"))
                    .variant(ActionButtonVariant::Ghost)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_det.update(cx, |v, cx| {
                            v.preview_details_expanded = !v.preview_details_expanded;
                            cx.notify();
                        });
                    }),
            )
            .when(self.preview_details_expanded, |this| {
                this.child(
                    div()
                        .v_flex()
                        .gap(px(3.0))
                        .p(px(8.0))
                        .rounded(px(6.0))
                        .bg(p.surface_alt)
                        .text_meta()
                        .tabular_nums()
                        .text_color(p.muted)
                        .child(format!(
                            "{}: #{} · {}: {}",
                            pb("matchId"),
                            hit.match_no,
                            pb("checkerId"),
                            hit.position.position_id
                        ))
                        .when_some(hit.tournament.clone(), |el, t| {
                            el.child(format!("{}: {t}", pb("tournament")))
                        })
                        .when_some(hit.round.clone(), |el, r| {
                            el.child(format!("{}: {r}", pb("round")))
                        }),
                )
            });

        pane.into_any_element()
    }
}

impl Render for SavedPositionsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = NeonTheme::current_palette(cx);

        div()
            .id("saved-positions-workspace")
            .test_support()
            .v_flex()
            .gap(px(12.0))
            .w_full()
            .flex_1()
            .child(self.render_filter_controls(p, locale, cx))
            .child(
                div()
                    .h_flex()
                    .items_start()
                    .gap(px(14.0))
                    .w_full()
                    .flex_1()
                    .child(self.render_table_section(p, locale, cx))
                    .child(self.render_preview_pane(p, locale, cx)),
            )
    }
}

// ── Save / Update Named Search Modal ─────────────────────────────────────────

struct SaveSearchDialogView {
    facade: Option<BackendFacade>,
    definition: PositionFilterDefinition,
    active: Option<SavedPositionFilter>,
    name_input: Entity<InputState>,
    busy: ModalBusyLock,
    saving: bool,
    error: Option<SharedString>,
    parent: Entity<SavedPositionsView>,
}

impl SaveSearchDialogView {
    fn new(
        facade: Option<BackendFacade>,
        definition: PositionFilterDefinition,
        active: Option<SavedPositionFilter>,
        busy: ModalBusyLock,
        parent: Entity<SavedPositionsView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial_name = active.as_ref().map(|a| a.name.clone()).unwrap_or_default();
        let locale = current_locale(cx);
        let name_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder(I18n::t(
                locale,
                "position-browser",
                "searchName",
            ));
            if !initial_name.is_empty() {
                s = s.default_value(initial_name);
            }
            s
        });
        Self {
            facade,
            definition,
            active,
            name_input,
            busy,
            saving: false,
            error: None,
            parent,
        }
    }

    fn submit(&mut self, update_existing: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let name = self.name_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let Some(facade) = self.facade.clone() else {
            window.close_dialog(cx);
            return;
        };
        self.saving = true;
        self.busy.set_busy(true);
        self.error = None;
        let def = self.definition.clone();
        let active = self.active.as_ref().map(clone_saved_filter);

        let Some(task) = spawn_facade_write(cx, move || async move {
            if update_existing && let Some(existing) = active {
                facade
                    .update_saved_search(
                        existing.id,
                        UpdatePositionFilter {
                            name,
                            definition: def,
                            expected_version: existing.version,
                        },
                    )
                    .await
            } else {
                facade
                    .save_position_search(CreatePositionFilter {
                        name,
                        definition: def,
                    })
                    .await
            }
        }) else {
            self.saving = false;
            self.busy.set_busy(false);
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |view, window, cx| {
                view.saving = false;
                view.busy.set_busy(false);
                let locale = current_locale(cx);
                match res {
                    Ok(saved) => {
                        view.parent.update(cx, |parent, cx| {
                            parent.saved_searches.upsert_saved(saved, locale);
                            cx.notify();
                        });
                        window.close_dialog(cx);
                    }
                    Err(err) => {
                        if err.kind == ErrorKind::Conflict {
                            view.parent.update(cx, |parent, cx| {
                                parent.saved_searches.record_error(locale, &err);
                                cx.notify();
                            });
                        }
                        view.error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for SaveSearchDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = NeonTheme::current_palette(cx);
        let pb = |k: &'static str| I18n::t(locale, "position-browser", k);
        let name_empty = self.name_input.read(cx).value().trim().is_empty();
        let ent = cx.entity();

        div()
            .v_flex()
            .gap(px(12.0))
            .child(
                div()
                    .v_flex()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_label()
                            .text_color(p.text)
                            .child(pb("searchName")),
                    )
                    .child(Input::new(&self.name_input).id("save-search-name-input")),
            )
            .when_some(self.error.clone(), |this, err| {
                this.child(
                    StatusBanner::new("save-search-error", BannerKind::Error, err).palette(p),
                )
            })
            .child(
                div()
                    .h_flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child({
                        let ent_new = ent.clone();
                        let mut btn = ActionButton::new("save-search-new")
                            .icon(ActionIconName::Save)
                            .label(pb("saveNew"))
                            .disabled(name_empty || self.saving)
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent_new.update(cx, |v, cx| v.submit(false, window, cx));
                            });
                        if self.active.is_none() {
                            btn = btn.primary();
                        }
                        btn
                    })
                    .when(self.active.is_some(), |this| {
                        let ent_upd = ent.clone();
                        this.child(
                            ActionButton::new("save-search-update")
                                .icon(ActionIconName::Save)
                                .label(pb("updateSearch"))
                                .primary()
                                .disabled(name_empty || self.saving)
                                .palette(p)
                                .on_click(move |_, window, cx| {
                                    ent_upd.update(cx, |v, cx| v.submit(true, window, cx));
                                }),
                        )
                    }),
            )
    }
}

// ── Readiness Pre-check + Deck Transfer Dialog ───────────────────────────────

struct PositionDeckTransferDialog {
    facade: Option<BackendFacade>,
    position_ids: Vec<Uuid>,
    names: HashMap<Uuid, String>,
    busy: ModalBusyLock,
    parent: Entity<SavedPositionsView>,

    readiness: Option<PositionAnalysisReadiness>,
    readiness_error: Option<SharedString>,
    checking: bool,

    decks: Vec<Deck>,
    selected_deck_id: Option<Uuid>,
    new_deck_input: Entity<InputState>,
    transferring: bool,
    creating_deck: bool,
    error: Option<SharedString>,
}

impl PositionDeckTransferDialog {
    fn new(
        facade: Option<BackendFacade>,
        position_ids: Vec<Uuid>,
        names: HashMap<Uuid, String>,
        busy: ModalBusyLock,
        parent: Entity<SavedPositionsView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let locale = current_locale(cx);
        let new_deck_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(I18n::t(locale, "position-browser", "deckName"))
        });
        let mut view = Self {
            facade,
            position_ids,
            names,
            busy,
            parent,
            readiness: None,
            readiness_error: None,
            checking: false,
            decks: Vec::new(),
            selected_deck_id: None,
            new_deck_input,
            transferring: false,
            creating_deck: false,
            error: None,
        };
        view.load_initial(cx);
        view
    }

    fn load_initial(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        self.checking = true;
        self.readiness_error = None;
        let ids = self.position_ids.clone();
        let Some(task) = spawn_facade_read(cx, move || async move {
            let ready = facade.position_analysis_readiness(ids).await;
            let decks = facade.list_decks().await;
            (ready, decks)
        }) else {
            self.checking = false;
            return;
        };
        let weak = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            let Ok((ready_res, decks_res)) = task.await else {
                return;
            };
            let _ = weak.update(cx, |view, cx| {
                view.checking = false;
                let locale = current_locale(cx);
                match ready_res {
                    Ok(summary) => view.readiness = Some(summary),
                    Err(err) => {
                        view.readiness_error = Some(format_domain_error(locale, &err).into());
                    }
                }
                if let Ok(decks) = decks_res {
                    if view.selected_deck_id.is_none() {
                        view.selected_deck_id = decks.first().map(|d| d.id);
                    }
                    view.decks = decks;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn create_deck(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.creating_deck || self.transferring {
            return;
        }
        let name = self.new_deck_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let Some(facade) = self.facade.clone() else {
            return;
        };
        self.creating_deck = true;
        self.busy.set_busy(true);
        self.error = None;
        let Some(task) =
            spawn_facade_write(cx, move || async move { facade.create_deck(name).await })
        else {
            self.creating_deck = false;
            self.busy.set_busy(false);
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |view, window, cx| {
                view.creating_deck = false;
                view.busy.set_busy(false);
                match res {
                    Ok(deck) => {
                        view.selected_deck_id = Some(deck.id);
                        view.decks.push(deck);
                        view.new_deck_input
                            .update(cx, |s, cx| s.set_value(String::new(), window, cx));
                    }
                    Err(err) => {
                        let locale = current_locale(cx);
                        view.error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn transfer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(deck_id) = self.selected_deck_id else {
            return;
        };
        if self.transferring || self.creating_deck {
            return;
        }
        let Some(facade) = self.facade.clone() else {
            window.close_dialog(cx);
            return;
        };
        self.transferring = true;
        self.busy.set_busy(true);
        self.error = None;
        let ids = self.position_ids.clone();
        let Some(task) = spawn_facade_write(cx, move || async move {
            facade.add_positions_to_deck(deck_id, ids).await
        }) else {
            self.transferring = false;
            self.busy.set_busy(false);
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |view, window, cx| {
                view.transferring = false;
                view.busy.set_busy(false);
                match res {
                    Ok(outcome) => {
                        let names = view.names.clone();
                        view.parent.update(cx, |parent, cx| {
                            parent.transfer_outcome = Some((outcome, names));
                            parent.load_page(parent.page, cx);
                            cx.notify();
                        });
                        window.close_dialog(cx);
                    }
                    Err(err) => {
                        let locale = current_locale(cx);
                        view.error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for PositionDeckTransferDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = NeonTheme::current_palette(cx);
        let pb = |k: &'static str| I18n::t(locale, "position-browser", k);
        let ent = cx.entity();

        let total = self.position_ids.len() as i64;
        let ready = self
            .readiness
            .as_ref()
            .map_or(0, |r| (total - r.analysis_required - r.unavailable).max(0));
        let first_needed = self
            .readiness
            .as_ref()
            .and_then(|r| r.first_analysis_required);
        let disabled = self.transferring || self.creating_deck;

        let mut body = div().v_flex().gap(px(12.0));

        if let Some(err) = self.readiness_error.clone() {
            let ent_r = ent.clone();
            body = body.child(
                StatusBanner::new("transfer-readiness-err", BannerKind::Error, err)
                    .palette(p)
                    .on_retry(pb("retrySimilar"), move |_, _, cx| {
                        ent_r.update(cx, |v, cx| v.load_initial(cx));
                    }),
            );
        } else if self.checking {
            body = body.child(
                div()
                    .text_body()
                    .text_color(p.muted)
                    .child(pb("checkingAnalysis")),
            );
        } else if let Some(summary) = &self.readiness {
            if ready == 0 {
                body = body.child(
                    StatusBanner::new("transfer-none-ready", BannerKind::Warning, pb("noneReady"))
                        .palette(p),
                );
            }
            if summary.analysis_required > 0 {
                body = body.child(div().text_body().text_color(p.warning_text).child(
                    I18n::format(
                        locale,
                        "position-browser",
                        "needsLocalAnalysis",
                        &[
                            ("count", Param::from(summary.analysis_required)),
                            ("total", Param::from(total)),
                        ],
                    ),
                ));
            }
            if summary.unavailable > 0 {
                body = body.child(div().text_meta().text_color(p.muted).child(I18n::format(
                    locale,
                    "position-browser",
                    "unavailablePositions",
                    &[("count", Param::from(summary.unavailable))],
                )));
            }
        }

        if ready > 0 {
            let mut deck_list = div().h_flex().flex_wrap().gap(px(6.0));
            for deck in &self.decks {
                let id = deck.id;
                let ent_d = ent.clone();
                let count_txt = I18n::format(
                    locale,
                    "position-browser",
                    "deckCount",
                    &[("count", Param::from(deck.card_count))],
                );
                deck_list = deck_list.child(
                    ActionButton::new(SharedString::from(format!("transfer-deck-{id}")))
                        .label(format!("{} ({count_txt})", deck.name))
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

            body = body
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
                        .child(deck_list),
                )
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_1()
                                .child(Input::new(&self.new_deck_input).id("transfer-new-deck-in")),
                        )
                        .child({
                            let ent_cr = ent.clone();
                            ActionButton::new("transfer-create-deck-btn")
                                .icon(ActionIconName::Add)
                                .label(pb("createDeck"))
                                .disabled(
                                    disabled
                                        || self.new_deck_input.read(cx).value().trim().is_empty(),
                                )
                                .palette(p)
                                .on_click(move |_, window, cx| {
                                    ent_cr.update(cx, |v, cx| v.create_deck(window, cx));
                                })
                        }),
                );
        }

        if let Some(err) = self.error.clone() {
            body = body.child(
                StatusBanner::new("transfer-submit-error", BannerKind::Error, err).palette(p),
            );
        }

        // Action footer
        body = body.child(
            div()
                .h_flex()
                .justify_end()
                .gap(px(8.0))
                .when(ready > 0, |this| {
                    let ent_go = ent.clone();
                    let lbl = if self.transferring {
                        pb("transfer").to_string()
                    } else if ready == 1 {
                        pb("addOne").to_string()
                    } else {
                        I18n::format(
                            locale,
                            "position-browser",
                            "addCount",
                            &[("count", Param::from(ready))],
                        )
                    };
                    this.child(
                        ActionButton::new("transfer-confirm-btn")
                            .icon(ActionIconName::Add)
                            .label(lbl)
                            .primary()
                            .disabled(disabled || self.selected_deck_id.is_none())
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent_go.update(cx, |v, cx| v.transfer(window, cx));
                            }),
                    )
                })
                .when(ready == 0 && first_needed.is_some(), |this| {
                    let pid = first_needed.unwrap();
                    let parent = self.parent.clone();
                    this.child(
                        ActionButton::new("transfer-open-analysis-btn")
                            .icon(ActionIconName::Analysis)
                            .label(pb("openAnalysis"))
                            .primary()
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                parent.update(cx, |p_view, cx| p_view.open_position_by_id(pid, cx));
                            }),
                    )
                }),
        );

        body
    }
}

fn data_column_label(locale: Locale, col: SavedDataColumn) -> String {
    let pb = |k: &'static str| I18n::t(locale, "position-browser", k);
    let lib = |k: &'static str| I18n::t(locale, "library", k);
    match col {
        SavedDataColumn::Origin => lib("match").to_string(),
        SavedDataColumn::Database => pb("database").to_string(),
        SavedDataColumn::Game => pb("gameNumber").to_string(),
        SavedDataColumn::Ply => pb("moveNumber").to_string(),
        SavedDataColumn::Player => pb("player").to_string(),
        SavedDataColumn::Opponent => pb("opponent").to_string(),
        SavedDataColumn::Dice => pb("dice").to_string(),
        SavedDataColumn::Score => pb("score").to_string(),
        SavedDataColumn::Cube => pb("cubeShort").to_string(),
        SavedDataColumn::Loss => pb("nativeError").to_string(),
        SavedDataColumn::Decks => pb("decks").to_string(),
        SavedDataColumn::Notes => format!("{} / {}", lib("tags"), lib("comment")),
    }
}

fn sort_field_label(locale: Locale, sort: PositionBrowserSort) -> &'static str {
    let pb = |k: &'static str| I18n::t(locale, "position-browser", k);
    let lib = |k: &'static str| I18n::t(locale, "library", k);
    match sort {
        PositionBrowserSort::MatchId => "ID",
        PositionBrowserSort::Match => lib("match"),
        PositionBrowserSort::Database => pb("database"),
        PositionBrowserSort::Game => pb("gameNumber"),
        PositionBrowserSort::Ply => pb("moveNumber"),
        PositionBrowserSort::Player => pb("player"),
        PositionBrowserSort::Opponent => pb("opponent"),
        PositionBrowserSort::Dice => pb("dice"),
        PositionBrowserSort::Score => pb("score"),
        PositionBrowserSort::Cube => pb("cubeShort"),
        PositionBrowserSort::PlayedMove => pb("playedMove"),
        PositionBrowserSort::Loss => pb("nativeError"),
        PositionBrowserSort::Decks => pb("decks"),
        PositionBrowserSort::Tags => lib("tags"),
        PositionBrowserSort::Comment => lib("comment"),
    }
}
