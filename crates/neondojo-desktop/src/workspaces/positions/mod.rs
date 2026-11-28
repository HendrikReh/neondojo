//! Native GPUI **Positions** workspace (`Saved Positions`, `All Analyzed Decisions`,
//! and `Similar Positions & Practice`).
//!
//! Architecture:
//! - [`PositionsWorkspace`] hosts the three sibling sub-views behind a zero-pill
//!   [`crate::ui::text_nav::TextNavBar`] (`TextNavDensity::Inline`) and preserves
//!   sub-view state across switches.
//! - [`columns`]: 13 Saved Positions column definitions, width clamping,
//!   `LocalUiState::positions` persistence, and 200-item selection cap.
//! - [`saved_positions`]: 4 filter groups, `AppliedFilterChips`, Named Saved
//!   Searches (409 conflict banner + Undo delete), 13-column resizable/sortable
//!   table, batch remove + Undo, right-hand `BoardView` preview, and readiness
//!   pre-check + Deck Transfer dialog.
//! - [`decision_search`]: First-visit auto-search for `My player profile`, filter
//!   bar, resizable dense results table, `BoardView` + `crate::review::*` preview,
//!   checker & cube `OpenMatchInAnalysis` emission, Add-to-Deck dialog, and
//!   up-to-20-errors Practice modal.
//! - [`similar_positions`]: Pip-distance search (up to 50 hits, 25/page, 4 toggles),
//!   click-to-move practice board (`neondojo_rules`), Your Move vs Best Play arrow
//!   overlay, Retry Missed Moves pass, practice history, and persisted failed-save
//!   retry queue (`LocalUiState::study.pending_similar_practice`).
//! - [`practice_chart`]: Custom GPUI `canvas()` `SimilarPracticeChart` with hover
//!   and keyboard (`Left`/`Right`/`Home`/`End`) point selection.

pub mod columns;
pub mod decision_search;
pub mod practice_board;
pub mod practice_chart;
pub mod saved_positions;
pub mod similar_positions;

use crate::events::WorkspaceEvent;
use crate::i18n::current_locale;
use crate::local_state::LocalUiState;
use crate::theme::{NeonTheme, NeonTypographyExt};
use crate::tokio_bridge::{ReadTask, TokioBridge};
use crate::ui::text_nav::{TextNavBar, TextNavDensity, TextNavItem};
use decision_search::DecisionSearchView;
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, Subscription, Task, TestSupportExt, Window,
    base::StyledExt, div, px,
};
use neondojo::game_plan::GamePlan;
use neondojo::model::Position;
use neondojo::recurring_mistakes::MistakeKind;
use neondojo_board::BoardThemeConfig;
use neondojo_i18n::{I18n, Locale};
use saved_positions::{SavedPositionsEvent, SavedPositionsView};
use similar_positions::{SimilarPositionsEvent, SimilarPositionsView};
use std::future::Future;
use std::path::PathBuf;
use tokio::task::JoinError;

/// All six NeonGammon game-plan categories in canonical UI order.
pub(crate) const ALL_GAME_PLANS: &[GamePlan] = &[
    GamePlan::Opening,
    GamePlan::PureRace,
    GamePlan::Racing,
    GamePlan::Attacking,
    GamePlan::Priming,
    GamePlan::Anchoring,
];

/// All six mistake categories in canonical UI order.
pub(crate) const ALL_MISTAKE_KINDS: &[MistakeKind] = &[
    MistakeKind::Checker,
    MistakeKind::MissedDouble,
    MistakeKind::PrematureDouble,
    MistakeKind::WrongTake,
    MistakeKind::WrongPass,
    MistakeKind::UnclassifiedCube,
];

/// Localized label for a [`GamePlan`].
pub(crate) fn game_plan_label(locale: Locale, plan: GamePlan) -> &'static str {
    let key = match plan {
        GamePlan::Opening => "opening",
        GamePlan::PureRace => "pure_race",
        GamePlan::Racing => "racing",
        GamePlan::Attacking => "attacking",
        GamePlan::Priming => "priming",
        GamePlan::Anchoring => "anchoring",
    };
    I18n::t(locale, "game-plan", key)
}

/// Localized label for a [`MistakeKind`].
pub(crate) fn mistake_kind_label(locale: Locale, kind: MistakeKind) -> &'static str {
    let key = match kind {
        MistakeKind::Checker => "checker",
        MistakeKind::MissedDouble => "missed_double",
        MistakeKind::PrematureDouble => "premature_double",
        MistakeKind::WrongTake => "wrong_take",
        MistakeKind::WrongPass => "wrong_pass",
        MistakeKind::UnclassifiedCube => "unclassified_cube",
    };
    I18n::t(locale, "recurring-mistakes", key)
}

/// Resolves a [`BoardThemeConfig`] from [`LocalUiState::board_theme`].
pub(crate) fn board_theme_from_local_state(state: &LocalUiState) -> BoardThemeConfig {
    BoardThemeConfig {
        board_theme: state.board_theme.clone(),
        ..BoardThemeConfig::default()
    }
}

/// Spawns an abortable `BackendFacade` read query onto [`TokioBridge`] inside
/// `spawn_blocking(move || handle.block_on(...))` so cross-crate SQLx HRTB
/// futures never trip Rust's `Send` bound checker.
pub(crate) fn spawn_facade_read<F, Fut, R>(cx: &App, f: F) -> Option<ReadTask<R>>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = R>,
    R: Send + 'static,
{
    cx.try_global::<TokioBridge>()?;
    let handle = TokioBridge::handle(cx);
    Some(TokioBridge::spawn_read(cx, async move {
        tokio::task::spawn_blocking(move || handle.block_on(f()))
            .await
            .expect("positions facade read task panicked")
    }))
}

/// Spawns a non-abortable `BackendFacade` write mutation onto [`TokioBridge`]
/// using the same SQLx HRTB `Send`-safe `spawn_blocking` bridge.
pub(crate) fn spawn_facade_write<F, Fut, R>(cx: &App, f: F) -> Option<Task<Result<R, JoinError>>>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = R>,
    R: Send + 'static,
{
    cx.try_global::<TokioBridge>()?;
    let handle = TokioBridge::handle(cx);
    Some(TokioBridge::spawn_write(cx, async move {
        tokio::task::spawn_blocking(move || handle.block_on(f()))
            .await
            .expect("positions facade write task panicked")
    }))
}

/// The three sibling sub-views inside the Positions workspace.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PositionsSubView {
    #[default]
    SavedPositions,
    DecisionSearch,
    SimilarPositions,
}

impl PositionsSubView {
    pub const fn id(self) -> &'static str {
        match self {
            Self::SavedPositions => "saved",
            Self::DecisionSearch => "decisions",
            Self::SimilarPositions => "similar",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "saved" => Some(Self::SavedPositions),
            "decisions" => Some(Self::DecisionSearch),
            "similar" => Some(Self::SimilarPositions),
            _ => None,
        }
    }
}

/// Native GPUI **Positions** workspace hosting Saved Positions, All Analyzed
/// Decisions, and Similar Positions & Practice as three sibling sub-views.
pub struct PositionsWorkspace {
    visible: bool,
    active_sub_view: PositionsSubView,
    saved_view: Entity<SavedPositionsView>,
    decisions_view: Entity<DecisionSearchView>,
    similar_view: Entity<SimilarPositionsView>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<WorkspaceEvent> for PositionsWorkspace {}

impl PositionsWorkspace {
    /// Creates a new [`PositionsWorkspace`] using NeonDojo's default local UI
    /// state persistence path (`~/Library/Application Support/NeonDojo/ui-state.json`).
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_persist_path(Some(LocalUiState::default_path()), window, cx)
    }

    /// Creates a [`PositionsWorkspace`] with a custom atomic persistence path
    /// (passing `Some(tempdir_path)` or `None` in headless tests avoids writing
    /// to the user's real application-support state file).
    pub fn new_with_persist_path(
        persist_path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let saved_path = persist_path.clone();
        let saved_view = cx.new(|cx| SavedPositionsView::new(saved_path, window, cx));
        let decisions_view = cx.new(|cx| DecisionSearchView::new(window, cx));
        let similar_view = cx.new(|cx| SimilarPositionsView::new(persist_path, window, cx));

        let mut subscriptions = Vec::new();

        subscriptions.push(cx.subscribe(
            &saved_view,
            |this, _, event: &SavedPositionsEvent, cx| match event {
                SavedPositionsEvent::OpenInAnalysis {
                    match_id,
                    game_no,
                    position_id,
                } => {
                    cx.emit(WorkspaceEvent::OpenMatchInAnalysis {
                        match_id: *match_id,
                        game_no: Some(*game_no),
                        position_id: Some(*position_id),
                        decision_no: None,
                    });
                }
                SavedPositionsEvent::FindSimilar {
                    position,
                    match_name,
                } => {
                    this.open_similar_for_position(position.clone(), match_name.clone(), cx);
                }
                SavedPositionsEvent::ShowNotice(notice) => {
                    cx.emit(WorkspaceEvent::ShowNotice(notice.clone()));
                }
            },
        ));

        subscriptions.push(cx.subscribe(
            &decisions_view,
            |_this, _, event: &WorkspaceEvent, cx| {
                cx.emit(event.clone());
            },
        ));

        subscriptions.push(cx.subscribe(
            &similar_view,
            |this, _, event: &SimilarPositionsEvent, cx| match event {
                SimilarPositionsEvent::BackToSaved => {
                    this.select_sub_view(PositionsSubView::SavedPositions, cx);
                }
                SimilarPositionsEvent::Workspace(ev) => {
                    cx.emit((**ev).clone());
                }
            },
        ));

        Self {
            visible: false,
            active_sub_view: PositionsSubView::SavedPositions,
            saved_view,
            decisions_view,
            similar_view,
            _subscriptions: subscriptions,
        }
    }

    /// Notifies the workspace and its currently active sub-view of visibility changes.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        self.sync_sub_view_visibility(cx);
        cx.notify();
    }

    pub fn active_sub_view(&self) -> PositionsSubView {
        self.active_sub_view
    }

    pub fn select_sub_view(&mut self, view: PositionsSubView, cx: &mut Context<Self>) {
        if self.active_sub_view == view {
            return;
        }
        self.active_sub_view = view;
        self.sync_sub_view_visibility(cx);
        cx.notify();
    }

    /// Switches directly to Similar Positions & Practice targeting `position`.
    pub fn open_similar_for_position(
        &mut self,
        position: Position,
        match_name: String,
        cx: &mut Context<Self>,
    ) {
        self.active_sub_view = PositionsSubView::SimilarPositions;
        self.similar_view.update(cx, |sim, cx| {
            sim.set_source(position, match_name, cx);
        });
        self.sync_sub_view_visibility(cx);
        cx.notify();
    }

    pub fn saved_view(&self) -> &Entity<SavedPositionsView> {
        &self.saved_view
    }

    pub fn decisions_view(&self) -> &Entity<DecisionSearchView> {
        &self.decisions_view
    }

    pub fn similar_view(&self) -> &Entity<SimilarPositionsView> {
        &self.similar_view
    }

    fn sync_sub_view_visibility(&mut self, cx: &mut Context<Self>) {
        let vis = self.visible;
        let active = self.active_sub_view;
        self.saved_view.update(cx, |v, cx| {
            v.set_visible(vis && active == PositionsSubView::SavedPositions, cx);
        });
        self.decisions_view.update(cx, |v, cx| {
            v.set_visible(vis && active == PositionsSubView::DecisionSearch, cx);
        });
        self.similar_view.update(cx, |v, cx| {
            v.set_visible(vis && active == PositionsSubView::SimilarPositions, cx);
        });
    }
}

impl Render for PositionsWorkspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = NeonTheme::current_palette(cx);
        let ent = cx.entity();

        let nav_items = vec![
            TextNavItem::new(
                PositionsSubView::SavedPositions.id(),
                I18n::t(locale, "workspace", "saved"),
            )
            .element_id("positions-tab-saved"),
            TextNavItem::new(
                PositionsSubView::DecisionSearch.id(),
                I18n::t(locale, "workspace", "all"),
            )
            .element_id("positions-tab-decisions"),
            TextNavItem::new(
                PositionsSubView::SimilarPositions.id(),
                I18n::t(locale, "position-browser", "similar"),
            )
            .element_id("positions-tab-similar"),
        ];

        let nav = TextNavBar::new("positions-sub-nav", nav_items)
            .active(Some(self.active_sub_view.id()))
            .density(TextNavDensity::Inline)
            .palette(p)
            .on_select(move |id, _, cx| {
                if let Some(next) = PositionsSubView::from_id(id) {
                    ent.update(cx, |ws, cx| ws.select_sub_view(next, cx));
                }
            });

        let active_body = match self.active_sub_view {
            PositionsSubView::SavedPositions => self.saved_view.clone().into_any_element(),
            PositionsSubView::DecisionSearch => self.decisions_view.clone().into_any_element(),
            PositionsSubView::SimilarPositions => self.similar_view.clone().into_any_element(),
        };

        div()
            .id("positions-workspace-root")
            .test_support()
            .v_flex()
            .gap(px(12.0))
            .size_full()
            .p(px(16.0))
            .bg(p.bg)
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
                            .h_flex()
                            .items_center()
                            .gap(px(20.0))
                            .child(div().text_page().text_color(p.text).child(I18n::t(
                                locale,
                                "position-browser",
                                "title",
                            )))
                            .child(nav),
                    ),
            )
            .child(
                div()
                    .id("positions-workspace-scroll")
                    .test_support()
                    .v_flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(active_body),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::columns::{
        DEFAULT_SAVED_VISIBLE_COLUMNS, POSITION_SELECTION_LIMIT, SavedDataColumn,
        SavedPositionColumn, SavedTableLayout, effective_sort_direction, toggle_position_selection,
        toggled_sort_direction,
    };
    use super::decision_search::{DecisionFilterDraft, DecisionHitRow};
    use super::practice_board::CheckerPracticeDraft;
    use super::practice_chart::{
        ChartLayout, POINT_HIT_RADIUS_PX, ROLLING_WINDOWS, inner_plot_bounds, map_norm_point,
        step_selected_index,
    };
    use super::saved_positions::{SavedFilterDraft, SavedSearchesState, clone_saved_filter};
    use super::similar_positions::{
        PENDING_SIMILAR_PREFIX, PendingPracticeSummaryPayload, PendingSimilarPracticeQueue,
        clone_practice_summary,
    };
    use super::*;
    use crate::configure_app;
    use chrono::{Duration, TimeZone, Utc};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{Bounds, WindowBounds, WindowOptions, point, size};

    fn desktop_window_options() -> WindowOptions {
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.0), px(0.0)),
                size(px(1600.0), px(1000.0)),
            ))),
            ..WindowOptions::default()
        }
    }
    use neondojo::domain_error::{DomainError, ErrorKind};
    use neondojo::model::{MatchSummary, MoveSeverity};
    use neondojo::position_browser::{
        PositionAnalysisState, PositionBrowserDirection, PositionBrowserHit, PositionBrowserSort,
        PositionFilterDefinition, PositionNativeError, SavedPositionFilter,
    };
    use neondojo::recurring_mistakes::{LossDirection, MistakeReference};
    use neondojo::similar_positions::{SimilarHit, SimilarPracticeSummary};
    use neondojo_rules::backgammon::initial_board;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;
    use uuid::Uuid;

    fn sample_position(id: Uuid, match_uuid: Uuid, game_no: i32, ply: i32) -> Position {
        Position {
            cube_enabled: Some(true),
            id,
            match_uuid,
            ordinal: ply,
            game_no,
            ply,
            player: 0,
            dice: vec![3, 1],
            board: initial_board().to_vec(),
            position_id: "4HPwATDgc/ABMA".into(),
            gnubg_match_id: "cAkAAAAAAAAA".into(),
            notation: "8/5 6/5".into(),
        }
    }

    fn sample_saved_hit(
        pos_id: Uuid,
        match_id: Uuid,
        source_available: bool,
    ) -> PositionBrowserHit {
        PositionBrowserHit {
            position: sample_position(pos_id, match_id, 2, 7),
            match_info: MatchSummary {
                id: match_id,
                source_available,
                filename: "club.xg".into(),
                format: "xg".into(),
                white: "Alice".into(),
                black: "Bob".into(),
                match_length: 7,
                position_count: 100,
                created_at: Utc::now(),
            },
            source_available,
            match_no: 42,
            database_id: Uuid::new_v4(),
            database_name: "Main".into(),
            display_name: "Alice vs Bob".into(),
            tournament: Some("Spring Open".into()),
            round: Some("R3".into()),
            white_score: Some(3),
            black_score: Some(2),
            crawford: Some(false),
            cube_value: Some(2),
            cube_owner: Some(0),
            tags: vec!["opening".into()],
            notes: vec![],
            card_ready: true,
            decks: vec![],
            native_error: Some(PositionNativeError {
                job_id: Some(Uuid::new_v4()),
                analysis_id: "job:test".into(),
                imported_analysis_id: None,
                engine: "gnubg".into(),
                plies: Some(3),
                depth: Some("3-ply".into()),
                equity_loss: 0.085,
                severity: MoveSeverity::Blunder,
            }),
        }
    }

    fn sample_decision_hit(checker: bool) -> DecisionHitRow {
        let match_id = Uuid::new_v4();
        let pos_id = if checker { Uuid::new_v4() } else { Uuid::nil() };
        DecisionHitRow {
            reference: MistakeReference {
                job_id: Some(Uuid::new_v4()),
                imported_analysis_id: None,
                game_no: 3,
                decision_no: 14,
            },
            kind: if checker {
                "checker".into()
            } else {
                "cube_offer".into()
            },
            position: sample_position(pos_id, match_id, 3, 14),
            players: ["Alice".into(), "Bob".into()],
            engine: "gnubg".into(),
            plies: Some(3),
            depth: Some("3-ply".into()),
            method: None,
            source: Some("native".into()),
            game_plan: GamePlan::Opening,
            mistake_kind: Some(if checker {
                MistakeKind::Checker
            } else {
                MistakeKind::MissedDouble
            }),
            played: if checker {
                "13/10 24/23".into()
            } else {
                "no double".into()
            },
            best: if checker {
                "8/5 6/5".into()
            } else {
                "double".into()
            },
            equity_loss: 0.092,
            match_length: 7,
            away: Some([4, 5]),
            crawford: false,
            cube_value: 1,
            cube_owner: None,
            practicable: true,
        }
    }

    fn sample_practice_summary(
        id: Uuid,
        minutes_offset: i64,
        avg_loss: f64,
        is_retry: bool,
    ) -> SimilarPracticeSummary {
        let base = Utc.with_ymd_and_hms(2026, 5, 1, 10, 0, 0).unwrap();
        SimilarPracticeSummary {
            id,
            completed_at: base + Duration::minutes(minutes_offset),
            total_positions: 10,
            scored_moves: 10,
            missed_moves: if avg_loss > 0.0 { 3 } else { 0 },
            total_equity_loss: avg_loss * 10.0,
            average_equity_loss: avg_loss,
            is_retry,
        }
    }

    #[test]
    fn selection_cap_enforces_200_limit_across_pages() {
        let page1: Vec<Uuid> = (0..120).map(|_| Uuid::new_v4()).collect();
        let page2: Vec<Uuid> = (0..120).map(|_| Uuid::new_v4()).collect();

        let sel = toggle_position_selection(&[], &page1, true, POSITION_SELECTION_LIMIT);
        assert_eq!(sel.len(), 120);

        let sel = toggle_position_selection(&sel, &page2, true, POSITION_SELECTION_LIMIT);
        assert_eq!(sel.len(), POSITION_SELECTION_LIMIT);
        assert_eq!(sel[0], page1[0]);
        assert_eq!(sel[119], page1[119]);
        assert_eq!(sel[120], page2[0]);
        assert_eq!(sel[199], page2[79]);

        // Unchecking a subset removes only those items while keeping order.
        let sel = toggle_position_selection(&sel, &page1[0..20], false, POSITION_SELECTION_LIMIT);
        assert_eq!(sel.len(), 180);
        assert_eq!(sel[0], page1[20]);
    }

    #[test]
    fn saved_columns_13_prefs_persist_and_guard_last_column() {
        assert_eq!(SavedPositionColumn::CONTENT_COLUMNS.len(), 13);
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ui-state.json");

        let mut layout = SavedTableLayout::default();
        assert_eq!(layout.visible_columns, DEFAULT_SAVED_VISIBLE_COLUMNS);
        assert!(!layout.show_played);
        assert!(
            !layout
                .active_columns()
                .contains(&SavedPositionColumn::PlayedMove)
        );

        // Enable played move, add Decks column, resize Origin, clamp under min
        layout.show_played = true;
        assert!(layout.toggle_column(SavedDataColumn::Decks, true));
        assert!(
            layout
                .active_columns()
                .contains(&SavedPositionColumn::PlayedMove)
        );
        assert!(
            layout
                .active_columns()
                .contains(&SavedPositionColumn::Decks)
        );
        layout.set_width(SavedPositionColumn::Origin, 40); // below min 112
        assert_eq!(layout.width_for(SavedPositionColumn::Origin), 112);
        layout.set_width(SavedPositionColumn::Origin, 310);
        assert_eq!(layout.width_for(SavedPositionColumn::Origin), 310);

        // Save to LocalUiState::positions and reload from disk
        let mut local = LocalUiState::default();
        layout
            .save_to_local_state(&mut local, Some(&path))
            .expect("atomic save");
        let reloaded_local = LocalUiState::load_or_default(&path);
        let restored = SavedTableLayout::from_prefs(&reloaded_local.positions);
        assert_eq!(restored, layout);
        assert!(restored.show_played);
        assert_eq!(restored.width_for(SavedPositionColumn::Origin), 310);

        // Never allow hiding the final visible column
        let mut single = SavedTableLayout::default();
        for &col in DEFAULT_SAVED_VISIBLE_COLUMNS.iter().skip(1) {
            assert!(single.toggle_column(col, false));
        }
        assert_eq!(single.visible_columns.len(), 1);
        assert!(!single.toggle_column(single.visible_columns[0], false));
        assert_eq!(single.visible_columns.len(), 1);

        // Sort direction defaults (Loss -> Desc, others -> Asc) and toggles
        assert_eq!(
            effective_sort_direction(PositionBrowserSort::Loss, None),
            PositionBrowserDirection::Desc
        );
        assert_eq!(
            effective_sort_direction(PositionBrowserSort::Match, None),
            PositionBrowserDirection::Asc
        );
        assert_eq!(
            toggled_sort_direction(PositionBrowserSort::Match, None, PositionBrowserSort::Match),
            PositionBrowserDirection::Desc
        );
    }

    #[test]
    fn saved_filter_draft_validates_all_4_groups_and_builds_chips() {
        let mut draft = SavedFilterDraft {
            match_name: "Open Final".into(),
            game_plan: Some(GamePlan::Priming),
            die1: "6".into(),
            die2: "1".into(),
            cube_value: "4".into(),
            analysis_state: Some(PositionAnalysisState::Ready),
            min_loss: "0,020".into(),
            tag: "blitz".into(),
            ..SavedFilterDraft::default()
        };

        let def = draft.to_definition().expect("valid 4-group filter");
        assert_eq!(def.die1, Some(1)); // normalized ascending
        assert_eq!(def.die2, Some(6));
        assert_eq!(def.cube_value, Some(4));
        assert_eq!(def.min_loss, Some(0.02));
        assert_eq!(def.game_plan, Some(GamePlan::Priming));

        let chips = draft.build_chips(Locale::En, &[]);
        assert!(chips.iter().any(|c| c.id.as_ref() == "match_name"));
        assert!(chips.iter().any(|c| c.id.as_ref() == "game_plan"));
        assert!(chips.iter().any(|c| c.id.as_ref() == "dice"));
        assert!(chips.iter().any(|c| c.id.as_ref() == "analysis_state"));
        assert!(chips.iter().any(|c| c.id.as_ref() == "tag"));

        // Removing dice chip clears both die1 and die2
        draft.remove_key("dice");
        assert!(draft.die1.is_empty() && draft.die2.is_empty());

        // Non-power-of-two cube is rejected
        draft.cube_value = "3".into();
        assert!(draft.to_definition().is_none());
    }

    #[test]
    fn saved_search_409_conflict_and_undo_delete_state() {
        let mut state = SavedSearchesState::default();
        let saved = SavedPositionFilter {
            id: Uuid::new_v4(),
            name: "Blunders".into(),
            definition: PositionFilterDefinition::default(),
            version: 3,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            database_missing: false,
        };
        let saved_id = saved.id;
        state.upsert_saved(clone_saved_filter(&saved), Locale::En);
        assert_eq!(state.active_id, Some(saved_id));

        // Simulate HTTP 409 revision conflict
        let conflict = DomainError::new(
            ErrorKind::Conflict,
            "Position filter was changed; reload before saving",
        );
        state.record_error(Locale::En, &conflict);
        assert!(state.conflict_error.is_some());
        assert!(state.error.is_none());

        // Delete captures item for Undo
        state.apply_deleted(saved, Locale::En);
        assert!(state.items.is_empty());
        assert!(state.active_id.is_none());
        assert_eq!(state.deleted_undo.as_ref().map(|d| d.id), Some(saved_id));
    }

    #[test]
    fn decision_search_open_analysis_event_covers_both_checker_and_cube() {
        let checker = sample_decision_hit(true);
        let cube = sample_decision_hit(false);

        match checker.to_open_analysis_event() {
            Some(WorkspaceEvent::OpenMatchInAnalysis {
                match_id,
                game_no,
                position_id,
                decision_no,
            }) => {
                assert_eq!(match_id, checker.position.match_uuid);
                assert_eq!(game_no, Some(3));
                assert_eq!(position_id, Some(checker.position.id));
                assert_eq!(decision_no, None);
            }
            other => panic!("unexpected event for checker hit: {other:?}"),
        }

        match cube.to_open_analysis_event() {
            Some(WorkspaceEvent::OpenMatchInAnalysis {
                match_id,
                game_no,
                position_id,
                decision_no,
            }) => {
                assert_eq!(match_id, cube.position.match_uuid);
                assert_eq!(game_no, Some(3));
                assert_eq!(position_id, None);
                assert_eq!(decision_no, Some(14));
            }
            other => panic!("unexpected event for cube hit: {other:?}"),
        }

        // Draft filter inherits profile error loss when min_loss is None
        let draft = DecisionFilterDraft {
            player: "Alice".into(),
            ..DecisionFilterDraft::default()
        };
        let q = draft
            .to_query(0.025, LossDirection::Desc, 1)
            .expect("valid decision query");
        assert_eq!(q.min_loss, Some(0.025));
    }

    #[test]
    fn checker_practice_draft_computes_legal_turns_steps_swap_and_arrows() {
        let pos = sample_position(Uuid::new_v4(), Uuid::new_v4(), 1, 1);
        let mut draft = CheckerPracticeDraft::from_position(&pos);
        assert!(draft.turn_error.is_none());
        assert!(!draft.is_forced_pass());
        assert!(!draft.can_submit());
        assert!(draft.can_swap_dice(false));

        let initial_dice = draft.displayed_dice();
        assert!(draft.swap_dice(false));
        assert_eq!(draft.displayed_dice(), [initial_dice[1], initial_dice[0]]);

        // Complete a legal 2-step turn by clicking valid sources
        let src1 = draft.next_points(None)[0];
        assert!(draft.click_point(src1, false));
        assert!(draft.can_undo());
        let src2 = draft.next_points(None)[0];
        assert!(draft.click_point(src2, false));
        assert!(draft.can_submit());

        // Project both Your Move and Best Play arrows
        let played_preview = draft.project_draft_preview();
        assert!(played_preview.is_ok());
        assert_eq!(played_preview.steps().map(|s| s.len()), Some(2));

        let best_preview = draft.project_notation_preview("8/5 6/5");
        assert!(best_preview.is_ok());
        assert_eq!(best_preview.steps().map(|s| s.len()), Some(2));

        assert!(draft.undo_step());
        assert!(!draft.can_submit());
    }

    #[test]
    fn similar_practice_retry_queue_persists_in_local_ui_state() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("ui-state.json");

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let p1 = PendingPracticeSummaryPayload {
            id: id1,
            completed_at: Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap(),
            total_positions: 5,
            losses: vec![0.0, 0.04, 0.0, 0.12, 0.0],
            is_retry: false,
        };
        let p2 = PendingPracticeSummaryPayload {
            id: id2,
            completed_at: Utc.with_ymd_and_hms(2026, 5, 1, 12, 5, 0).unwrap(),
            total_positions: 2,
            losses: vec![0.0, 0.02],
            is_retry: true,
        };

        let mut local = LocalUiState::default();
        let mut queue = PendingSimilarPracticeQueue::default();
        queue
            .enqueue(p1.clone(), &mut local, Some(&path))
            .expect("enqueue p1");
        queue
            .enqueue(p2.clone(), &mut local, Some(&path))
            .expect("enqueue p2");
        // Re-enqueue same ID deduplicates
        queue
            .enqueue(p1.clone(), &mut local, Some(&path))
            .expect("dedup p1");
        assert_eq!(queue.entries.len(), 2);
        assert!(
            local
                .study
                .pending_similar_practice
                .iter()
                .all(|e| e.key.starts_with(PENDING_SIMILAR_PREFIX))
        );

        // Reload from disk and verify intact recovery
        let reloaded_local = LocalUiState::load_or_default(&path);
        let mut recovered = PendingSimilarPracticeQueue::load_from_local_state(&reloaded_local);
        assert_eq!(recovered.entries.len(), 2);
        assert_eq!(recovered.entries[0].id, id2);
        assert_eq!(recovered.entries[1].id, id1);

        // Remove on successful backend save
        let mut local2 = reloaded_local;
        recovered
            .remove(id1, &mut local2, Some(&path))
            .expect("remove id1");
        let final_local = LocalUiState::load_or_default(&path);
        let final_queue = PendingSimilarPracticeQueue::load_from_local_state(&final_local);
        assert_eq!(final_queue.entries.len(), 1);
        assert_eq!(final_queue.entries[0].id, id2);
    }

    #[test]
    fn similar_practice_chart_geometry_rolling_windows_and_hit_test() {
        // Input arrives newest first; layout reverses chronologically.
        let r3 = sample_practice_summary(Uuid::new_v4(), 60, 0.030, false);
        let r2_retry = sample_practice_summary(Uuid::new_v4(), 30, 0.010, true);
        let r1 = sample_practice_summary(Uuid::new_v4(), 0, 0.090, false);

        let r3_id = r3.id;
        let r2_id = r2_retry.id;
        let r1_id = r1.id;
        let layout = ChartLayout::compute(&[r3, r2_retry, r1], ROLLING_WINDOWS[0])
            .expect("non-empty chart layout");
        assert_eq!(layout.points.len(), 3);
        assert_eq!(layout.points[0].id, r1_id);
        assert_eq!(layout.points[1].id, r2_id);
        assert_eq!(layout.points[2].id, r3_id);
        assert!((layout.points[0].norm_x - 0.0).abs() < 1e-5);
        assert!((layout.points[1].norm_x - 0.5).abs() < 1e-5);
        assert!((layout.points[2].norm_x - 1.0).abs() < 1e-5);

        // Rolling average excludes retry rounds: mean([0.090, 0.030]) = 0.060
        assert!(layout.show_trend);
        assert_eq!(layout.initial_indices, vec![0, 2]);
        assert!(layout.points[1].rolling_average.is_none());
        let avg_last = layout.points[2]
            .rolling_average
            .expect("initial rolling mean");
        assert!((avg_last - 0.060).abs() < 1e-6);

        // Nice Y axis ceiling for 0.090 -> 0.090 (unit 0.01)
        assert!((layout.axis_max - 0.090).abs() < 1e-6);

        // Screen-space hit-test selects the exact point
        let plot = inner_plot_bounds(Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(500.0), px(200.0)),
        });
        let p1_screen = map_norm_point(plot, layout.points[1].norm_x, layout.points[1].norm_y);
        assert_eq!(
            layout.hit_test_point(plot, p1_screen, POINT_HIT_RADIUS_PX),
            Some(1)
        );
        assert_eq!(
            layout.hit_test_point(plot, point(px(0.0), px(0.0)), 4.0),
            None
        );

        // Keyboard index stepping
        assert_eq!(step_selected_index(3, 2, "left"), Some(1));
        assert_eq!(step_selected_index(3, 1, "right"), Some(2));
        assert_eq!(step_selected_index(3, 2, "home"), Some(0));
        assert_eq!(step_selected_index(3, 0, "end"), Some(2));
    }

    #[gpui_kit::test]
    async fn headless_positions_workspace_switches_all_three_subviews_and_emits_events(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let dir = tempdir().expect("tempdir");
        let persist_path = dir.path().join("ui-state.json");

        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(LocalUiState::default(), false, cx);
        });

        let emitted: Arc<Mutex<Vec<WorkspaceEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let emitted_clone = Arc::clone(&emitted);

        let (window, ws) = cx
            .update(|cx| {
                gpui_kit::open_window(desktop_window_options(), cx, |window, cx| {
                    let entity = cx.new(|cx| {
                        PositionsWorkspace::new_with_persist_path(Some(persist_path), window, cx)
                    });
                    cx.subscribe(&entity, move |_, ev: &WorkspaceEvent, _| {
                        emitted_clone.lock().unwrap().push(ev.clone());
                    })
                    .detach();
                    entity
                })
            })
            .expect("open test window");

        // Populate all 3 sub-views with realistic data and render each headlessly
        let pos_id = Uuid::new_v4();
        let match_id = Uuid::new_v4();
        let saved_hit = sample_saved_hit(pos_id, match_id, false);
        let checker_decision = sample_decision_hit(true);
        let cube_decision = sample_decision_hit(false);
        let similar_hit = SimilarHit {
            position: sample_position(Uuid::new_v4(), Uuid::new_v4(), 1, 5),
            match_name: "Club Final".into(),
            white: "Alice".into(),
            black: "Bob".into(),
            distance: 4,
            equity_loss: Some(0.032),
        };
        let hist_r1 = sample_practice_summary(Uuid::new_v4(), 0, 0.080, false);
        let hist_r2 = sample_practice_summary(Uuid::new_v4(), 15, 0.020, true);
        let hist_r3 = sample_practice_summary(Uuid::new_v4(), 45, 0.040, false);

        let saved_pos = saved_hit.position.clone();
        let hist_r1_id = hist_r1.id;
        let hist_r2_id = hist_r2.id;
        let hist_r3_id = hist_r3.id;

        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.set_visible(true, cx);
                ws.saved_view().update(cx, |sv, cx| {
                    sv.set_items_for_test(vec![saved_hit], cx);
                    sv.set_filters_expanded(true, cx);
                });
                ws.decisions_view().update(cx, |dv, cx| {
                    dv.set_items_for_test(
                        vec![checker_decision.clone(), cube_decision.clone()],
                        cx,
                    );
                });
                ws.similar_view().update(cx, |sim, cx| {
                    sim.set_source(saved_pos, "Alice vs Bob".into(), cx);
                    sim.set_hits_for_test(vec![similar_hit], cx);
                    sim.set_history_for_test(
                        vec![
                            clone_practice_summary(&hist_r3),
                            clone_practice_summary(&hist_r2),
                            clone_practice_summary(&hist_r1),
                        ],
                        cx,
                    );
                });
            });

            // 1. Saved Positions renders with 4 filter groups & retained badge
            window.render_frame(cx);
            assert!(window.find("saved-positions-workspace").visible());
            assert!(window.find("saved-filter-group-match").visible());
            assert!(window.find("saved-filter-group-position").visible());
            assert!(window.find("saved-filter-group-analysis").visible());
            assert!(window.find("saved-filter-group-notes").visible());
            assert!(window.find("saved-retained-badge").visible());

            // 2. Switch to All Analyzed Decisions via TextNavBar click
            window.click("positions-tab-decisions", cx);
            window.render_frame(cx);
            assert_eq!(
                ws.read(cx).active_sub_view(),
                PositionsSubView::DecisionSearch
            );
            assert!(window.find("decision-search-workspace").visible());

            // Trigger OpenMatchInAnalysis for both checker and cube hits
            let decisions_ent = ws.read(cx).decisions_view().clone();
            decisions_ent.update(cx, |dv, cx| {
                dv.open_hit_in_analysis(&checker_decision, cx);
                dv.open_hit_in_analysis(&cube_decision, cx);
            });

            // 3. Switch to Similar Positions & Practice via TextNavBar click
            window.click("positions-tab-similar", cx);
            window.render_frame(cx);
            assert_eq!(
                ws.read(cx).active_sub_view(),
                PositionsSubView::SimilarPositions
            );
            assert!(window.find("similar-positions-workspace").visible());
            assert!(window.find("similar-practice-chart").visible());
            assert!(window.find("similar-practice-chart-callout").visible());

            // Exercise chart keyboard navigation (Left / Right / Home / End) and window toggle
            let chart = ws.read(cx).similar_view().read(cx).chart().clone();
            chart.update(cx, |c, cx| {
                assert_eq!(c.selected_round_id(), Some(hist_r3_id));
                assert!(c.select_by_key("left", cx));
                assert_eq!(c.selected_round_id(), Some(hist_r2_id));
                assert!(c.select_by_key("home", cx));
                assert_eq!(c.selected_round_id(), Some(hist_r1_id));
                assert!(c.select_by_key("end", cx));
                assert_eq!(c.selected_round_id(), Some(hist_r3_id));
                c.set_rolling_window(10, cx);
                assert_eq!(c.rolling_window(), 10);
            });
            window.render_frame(cx);
        })
        .expect("update window");

        let events = emitted.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[0],
            WorkspaceEvent::OpenMatchInAnalysis {
                position_id: Some(_),
                decision_no: None,
                ..
            }
        ));
        assert!(matches!(
            events[1],
            WorkspaceEvent::OpenMatchInAnalysis {
                position_id: None,
                decision_no: Some(14),
                ..
            }
        ));
    }

    #[gpui_kit::test]
    async fn headless_saved_positions_column_prefs_409_conflict_undo_and_find_similar_routing(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let dir = tempdir().expect("tempdir");
        let persist_path = dir.path().join("ui-state.json");

        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(LocalUiState::default(), false, cx);
        });

        let persist_clone = persist_path.clone();
        let (window, ws) = cx
            .update(|cx| {
                gpui_kit::open_window(desktop_window_options(), cx, |window, cx| {
                    cx.new(|cx| {
                        PositionsWorkspace::new_with_persist_path(Some(persist_clone), window, cx)
                    })
                })
            })
            .expect("open test window");

        let pos_id = Uuid::new_v4();
        let match_id = Uuid::new_v4();
        let hit = sample_saved_hit(pos_id, match_id, true);

        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.set_visible(true, cx);
                ws.saved_view().update(cx, |sv, cx| {
                    sv.set_items_for_test(vec![hit], cx);
                });
            });
            window.render_frame(cx);

            // Toggle "Show played moves" via click & verify persistence on disk
            window.click("saved-show-played-toggle", cx);
            window.render_frame(cx);
            assert!(ws.read(cx).saved_view().read(cx).table_layout().show_played);

            // Open column picker and toggle "Decks" column
            window.click("saved-column-picker-btn", cx);
            window.render_frame(cx);
            assert!(window.find("saved-column-picker-panel").visible());
            window.click("saved-col-opt-decks", cx);
            window.render_frame(cx);
            assert!(
                ws.read(cx)
                    .saved_view()
                    .read(cx)
                    .table_layout()
                    .visible_columns
                    .contains(&SavedDataColumn::Decks)
            );

            // Surface a 409 conflict banner on Saved Searches
            let sv = ws.read(cx).saved_view().clone();
            sv.update(cx, |sv, cx| {
                let conflict = DomainError::new(
                    ErrorKind::Conflict,
                    "Position filter was changed; reload before saving",
                );
                sv.saved_searches_mut().record_error(Locale::En, &conflict);
                cx.notify();
            });
            window.render_frame(cx);
            assert!(window.find("saved-search-conflict-banner").visible());

            // Click "Find similar positions" in preview pane -> emits SavedPositionsEvent::FindSimilar
            window.click("saved-preview-find-similar", cx);
        })
        .expect("update window before emit flush");

        // GPUI flushes cx.emit() subscriptions at the outer update boundary.
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                ws.read(cx).active_sub_view(),
                PositionsSubView::SimilarPositions
            );
            assert!(window.find("similar-positions-workspace").visible());

            // Switch back to Saved, batch-remove offline, verify Undo banner renders
            window.click("positions-tab-saved", cx);
            window.render_frame(cx);
            let sv = ws.read(cx).saved_view().clone();
            sv.update(cx, |sv, cx| {
                sv.batch_remove(vec![pos_id], cx);
            });
            window.render_frame(cx);
            assert!(window.find("saved-undo-remove-banner").visible());
        })
        .expect("update window after emit flush");

        // Confirm column preferences persisted atomically to tempdir ui-state.json
        let reloaded = LocalUiState::load_or_default(&persist_path);
        assert!(reloaded.positions.show_played);
        assert!(
            reloaded
                .positions
                .visible_columns
                .iter()
                .any(|c| c == "decks")
        );
    }

    #[gpui_kit::test]
    async fn headless_similar_positions_filter_toggles_pending_retry_and_chart_window(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let dir = tempdir().expect("tempdir");
        let persist_path = dir.path().join("ui-state.json");

        // Seed a persisted pending failed-save round in LocalUiState before mounting
        let pending_id = Uuid::new_v4();
        let pending_payload = PendingPracticeSummaryPayload {
            id: pending_id,
            completed_at: Utc.with_ymd_and_hms(2026, 5, 2, 14, 0, 0).unwrap(),
            total_positions: 4,
            losses: vec![0.0, 0.05, 0.0, 0.03],
            is_retry: false,
        };
        let mut initial_state = LocalUiState::default();
        let mut q = PendingSimilarPracticeQueue::default();
        q.enqueue(pending_payload, &mut initial_state, Some(&persist_path))
            .expect("seed pending retry queue");

        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(initial_state, false, cx);
        });

        let (window, ws) = cx
            .update(|cx| {
                gpui_kit::open_window(desktop_window_options(), cx, |window, cx| {
                    cx.new(|cx| {
                        PositionsWorkspace::new_with_persist_path(Some(persist_path), window, cx)
                    })
                })
            })
            .expect("open test window");

        let source_pos = sample_position(Uuid::new_v4(), Uuid::new_v4(), 1, 3);
        let h1 = sample_practice_summary(Uuid::new_v4(), 0, 0.050, false);
        let h2 = sample_practice_summary(Uuid::new_v4(), 20, 0.025, false);

        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.set_visible(true, cx);
                ws.open_similar_for_position(source_pos, "Practice Match".into(), cx);
                ws.similar_view().update(cx, |sim, cx| {
                    sim.set_history_for_test(vec![h2, h1], cx);
                });
            });
            window.render_frame(cx);

            // Pending retry queue recovered from LocalUiState::study.pending_similar_practice
            assert_eq!(
                ws.read(cx)
                    .similar_view()
                    .read(cx)
                    .pending_queue()
                    .entries
                    .len(),
                1
            );
            assert!(window.find("similar-pending-retry-btn").visible());

            // Click all 4 pip-distance toggles
            window.click("similar-toggle-same-dice", cx);
            window.click("similar-toggle-same-score", cx);
            window.click("similar-toggle-same-cube", cx);
            window.click("similar-toggle-current-match", cx);
            window.render_frame(cx);
            let f = ws.read(cx).similar_view().read(cx).filters();
            assert!(f.same_dice && f.same_score && f.same_cube && f.include_current_match);

            // Click chart rolling-window 20 rounds button
            window.click("similar-chart-window-20", cx);
            window.render_frame(cx);
            assert_eq!(
                ws.read(cx)
                    .similar_view()
                    .read(cx)
                    .chart()
                    .read(cx)
                    .rolling_window(),
                20
            );
        })
        .expect("update window");
    }
}
