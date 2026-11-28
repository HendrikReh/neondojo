//! Complete native GPUI Study workspace:
//! - 5 roving sub-tabs via [`TextNavBar`]: `Overview`, `Flashcards`, `Reports`,
//!   `Recurring Mistakes`, and `Practice History`
//! - Scoped [`AnkiCardFaceView`] lifecycle strictly limited to `card.kind == "anki"`
//! - Native `.apkg` file pickers (`cx.prompt_for_new_path` / multi-file `cx.prompt_for_paths`)
//! - Live push subscriptions (`subscribe_assessment_updates`, `subscribe_anki_sync_updates`)
//! - Full FSRS review, Practice Rounds, Transfer Practice, 3 Special Practice modals,
//!   Structured JSON AI Match Assessments with clickable `game.decision` ref chips,
//!   Recurring Mistake Practice, and Practice History with `canvas()` trend chart.

pub mod common;
pub mod flashcards;
pub mod fsrs_review;
pub mod history;
pub mod mistakes;
pub mod overview;
pub mod reports;

use self::common::{
    ALL_CARDS_DECK, DEFAULT_CARD_PREVIEW_WIDTH, MAX_CARD_PREVIEW_WIDTH, MIN_CARD_PREVIEW_WIDTH,
    StudyTab, clamp_new_cards_per_day, clamp_preview_width, game_plan_label, mistake_kind_label,
    spawn_facade_read, spawn_facade_write, update_local_ui_state,
};
use self::flashcards::{
    AnkiSyncOverviewCallbacks, AnkiSyncOverviewModalState, BatchCardAction, DeckManageModalState,
    DuplicateCardsModalState, FlashcardsCallbacks, FlashcardsState, open_anki_sync_overview_modal,
    open_deck_management_dialog, open_duplicate_cards_dialog, render_flashcards_browser,
};
use self::fsrs_review::{
    FsrsReviewCallbacks, FsrsReviewState, PipPracticeModalState, PracticeRoundCallbacks,
    PracticeRoundState, ScoreVariantsModalState, TimedPreviewModalState, TransferPracticeCallbacks,
    open_pip_practice_modal, open_score_variants_modal, open_timed_preview_modal,
    render_fsrs_review, render_practice_round, render_transfer_practice_section,
};
use self::history::{
    HistoryCallbacks, PracticeHistoryState, SelectedHistoryEntry, render_practice_history,
};
use self::mistakes::{
    MistakePracticeModalState, MistakesCallbacks, RecurringMistakesState,
    open_mistake_practice_modal, render_recurring_mistakes,
};
use self::overview::{
    OverviewCallbacks, StudyOverviewState, default_study_player, render_study_overview,
};
use self::reports::{
    ReportDetailTab, ReportsCallbacks, StudyReportsState, build_learning_review_positions,
    render_study_reports,
};
use crate::app_context::{DesktopAppContext, themed_board};
use crate::events::WorkspaceEvent;
use crate::i18n::{current_locale, format_domain_error};
use crate::keybindings::{
    CTX_STUDY_EXERCISE, CTX_TEXT_NAV, CheckOrContinue, CubeDouble, CubeDrop, CubeNoDouble,
    CubeTake, NavFirst, NavLast, NavNext, NavPrev, RateAgain, RateEasy, RateGood, RateHard,
    ShowAnswer, SplitterGrow, SplitterGrowFast, SplitterMax, SplitterMin, SplitterReset,
    SplitterShrink, SplitterShrinkFast, UndoCheckerStep,
};
use crate::local_state::LocalUiState;
use crate::theme::{NeonTheme, NeonTypographyExt};
use crate::tokio_bridge::ReadTask;
use crate::ui::action_button::ActionButton;
use crate::ui::modal::{ModalConfig, ModalSize, open_modal_dialog};
use crate::ui::text_nav::{RovingStep, TextNavBar, TextNavDensity, TextNavItem, next_roving_index};
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, PathPromptOptions, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, TestSupportExt, Window,
    base::StyledExt, component::WindowExt, component::input::InputState, div, px,
};
use neondojo::configuration::ConfigurationLocale;
use neondojo::domain_error::ErrorKind;
use neondojo::facade::BackendFacade;
use neondojo::game_plan::GamePlan;
use neondojo::learning::{
    AddDeckCards, CardAttemptRequest, CardQuestion, DeleteCard, DeleteCards, DuplicateQuery,
    EditCardTags, LearningPreferences, MoveCards, PipAnswer, Rating, RemoveCards, ResolveDuplicate,
    ReviewCard, RoundAdvance, RoundAnswer, SaveCardTopic, ScoreAnswer, SyncAction,
    SyncActionRequest, TagCard, TransferRequest,
};
use neondojo::practice::evaluation::{CubeOfferAction, CubeResponseAction, PracticeAnswer};
use neondojo::recurring_mistakes::{MistakeAttemptRequest, MistakeKind, MistakePracticeRequest};
use neondojo::study::ReportQuery;
use neondojo_board::themes::ThemeAtlasCache;
use neondojo_board::{BoardEvent, BoardView};
use neondojo_i18n::{I18n, Locale, Param, format_int};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

fn locale_to_config_locale(locale: Locale) -> ConfigurationLocale {
    match locale {
        Locale::De => ConfigurationLocale::De,
        Locale::En => ConfigurationLocale::En,
    }
}

/// Top-level native GPUI Study workspace.
pub struct StudyWorkspace {
    visible: bool,
    active_tab: StudyTab,
    focus_handle: FocusHandle,
    configured_player: String,
    selected_player: String,
    independent_only: bool,
    resumable_round_id: Option<Uuid>,

    overview: StudyOverviewState,
    flashcards: FlashcardsState,
    fsrs: FsrsReviewState,
    practice_round: PracticeRoundState,
    reports: StudyReportsState,
    mistakes: RecurringMistakesState,
    history: PracticeHistoryState,

    // Active modal handle for live AnkiSyncOverview push updates
    anki_sync_modal: Option<Entity<AnkiSyncOverviewModalState>>,
    // Active modal handle for interactive mistake practice board clicks
    active_mistake_modal: Option<Entity<MistakePracticeModalState>>,

    // Abortable read guards & live push tasks
    _board_subs: Vec<Subscription>,
    _push_tasks: Vec<Task<()>>,
    _overview_read: Option<ReadTask<()>>,
    _cards_read: Option<ReadTask<()>>,
    _reports_read: Option<ReadTask<()>>,
    _mistakes_read: Option<ReadTask<()>>,
    _history_read: Option<ReadTask<()>>,
    initialized_backend: bool,
}

impl EventEmitter<WorkspaceEvent> for StudyWorkspace {}

impl Focusable for StudyWorkspace {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl StudyWorkspace {
    /// Creates a new [`StudyWorkspace`] initialized from persisted [`LocalUiState`].
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let local = cx.try_global::<LocalUiState>().cloned().unwrap_or_default();
        let atlas = cx
            .try_global::<DesktopAppContext>()
            .map(|c| Arc::clone(&c.board_atlas))
            .unwrap_or_else(|| Arc::new(Mutex::new(ThemeAtlasCache::default())));

        let initial_board_cfg = cx
            .try_global::<DesktopAppContext>()
            .map(|c| c.board_config.clone())
            .unwrap_or_else(|| neondojo_board::BoardThemeConfig {
                board_theme: local.board_theme.clone(),
                ..neondojo_board::BoardThemeConfig::default()
            });

        let preview_board = cx.new(|cx| {
            let mut bv = BoardView::new("study-card-preview-board", Arc::clone(&atlas));
            bv.set_theme_config(initial_board_cfg.clone(), cx);
            bv
        });
        let fsrs_board = cx.new(|cx| {
            let mut bv = BoardView::new("study-fsrs-board", Arc::clone(&atlas));
            bv.set_theme_config(initial_board_cfg.clone(), cx);
            bv
        });
        let round_board = cx.new(|cx| {
            let mut bv = BoardView::new("study-round-board", Arc::clone(&atlas));
            bv.set_theme_config(initial_board_cfg.clone(), cx);
            bv
        });
        let report_board = cx.new(|cx| {
            let mut bv = BoardView::new("study-report-board", Arc::clone(&atlas));
            bv.set_theme_config(initial_board_cfg, cx);
            bv
        });

        let fsrs_sub = cx.subscribe(&fsrs_board, |this, _, event: &BoardEvent, cx| {
            if let BoardEvent::PointClicked(pt) = event {
                this.handle_fsrs_board_click(*pt, cx);
            }
        });
        let round_sub = cx.subscribe(&round_board, |this, _, event: &BoardEvent, cx| {
            if let BoardEvent::PointClicked(pt) = event {
                this.handle_round_board_click(*pt, cx);
            }
        });

        let resumable_round_id = local
            .study
            .practice_round_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok());

        let overview = StudyOverviewState::new(window, cx);
        let flashcards = FlashcardsState::new(
            local.study.deck_id.clone(),
            local.card_preview_width,
            preview_board,
            window,
            cx,
        );
        let fsrs = FsrsReviewState::new(fsrs_board, cx);
        let practice_round = PracticeRoundState::new(round_board, window, cx);
        let reports = StudyReportsState::new(report_board);
        let mistakes = RecurringMistakesState::new(window, cx);
        let history = PracticeHistoryState::new();

        let mut ws = Self {
            visible: true,
            active_tab: StudyTab::Overview,
            focus_handle: cx.focus_handle(),
            configured_player: String::new(),
            selected_player: String::new(),
            independent_only: local.study.independent_only,
            resumable_round_id,
            overview,
            flashcards,
            fsrs,
            practice_round,
            reports,
            mistakes,
            history,
            anki_sync_modal: None,
            active_mistake_modal: None,
            _board_subs: vec![fsrs_sub, round_sub],
            _push_tasks: Vec::new(),
            _overview_read: None,
            _cards_read: None,
            _reports_read: None,
            _mistakes_read: None,
            _history_read: None,
            initialized_backend: false,
        };

        ws.try_init_backend(window, cx);
        ws
    }

    /// Shows or hides the Study workspace, keeping any native `WKWebView` hidden when inactive.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        self.sync_anki_webview_visibility(cx);
        cx.notify();
    }

    /// Updates all Study workspace boards with a new board/checker/cube/dice theme.
    pub fn set_theme_config(
        &mut self,
        config: neondojo_board::BoardThemeConfig,
        cx: &mut Context<Self>,
    ) {
        self.flashcards
            .preview_board_view
            .update(cx, |bv, cx| bv.set_theme_config(config.clone(), cx));
        self.fsrs
            .board_view
            .update(cx, |bv, cx| bv.set_theme_config(config.clone(), cx));
        self.practice_round
            .board_view
            .update(cx, |bv, cx| bv.set_theme_config(config.clone(), cx));
        self.reports
            .preview_board_view
            .update(cx, |bv, cx| bv.set_theme_config(config, cx));
        cx.notify();
    }

    /// Switches directly to the Reports sub-tab and opens the saved report for `match_id`.
    pub fn open_report_for_match(&mut self, match_id: Uuid, cx: &mut Context<Self>) {
        self.active_tab = StudyTab::Reports;
        self.reports.pending_open_match_id = Some(match_id);
        self.sync_anki_webview_visibility(cx);

        if let Some(found) = self.find_report_id_for_match(match_id) {
            self.reports.pending_open_match_id = None;
            self.open_report_by_id(found, cx);
        } else {
            self.load_reports(false, cx);
        }
        cx.notify();
    }

    pub fn active_tab(&self) -> StudyTab {
        self.active_tab
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    fn facade(cx: &App) -> Option<BackendFacade> {
        cx.try_global::<DesktopAppContext>()
            .and_then(|c| c.facade())
    }

    fn find_report_id_for_match(&self, match_id: Uuid) -> Option<Uuid> {
        self.reports
            .reports
            .iter()
            .find(|r| {
                r.library_match_id == Some(match_id) || r.id == match_id || r.session_id == match_id
            })
            .map(|r| r.id)
    }

    fn sync_anki_webview_visibility(&self, cx: &mut App) {
        let on_flashcards = self.visible && self.active_tab == StudyTab::Flashcards;
        let show_fsrs_anki = on_flashcards && self.fsrs.active && self.fsrs.is_anki_card();
        let show_round_anki = on_flashcards
            && self
                .practice_round
                .round
                .as_ref()
                .is_some_and(|r| r.card.as_ref().is_some_and(|c| c.kind == "anki"));
        let show_preview_anki = on_flashcards
            && !self.fsrs.active
            && self.practice_round.round.is_none()
            && self
                .flashcards
                .preview_item()
                .is_some_and(|i| i.card.kind == "anki");

        self.fsrs
            .anki_face_view
            .update(cx, |v, cx| v.set_visible(show_fsrs_anki, cx));
        self.practice_round
            .anki_face_view
            .update(cx, |v, cx| v.set_visible(show_round_anki, cx));
        self.flashcards
            .preview_anki_view
            .update(cx, |v, cx| v.set_visible(show_preview_anki, cx));
    }

    fn try_init_backend(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.initialized_backend {
            return;
        }
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        self.initialized_backend = true;
        self.start_push_subscriptions(facade, cx);
        self.load_overview(Some(window), cx);
        self.load_decks_and_cards(cx);
        self.load_databases(cx);
    }

    fn start_push_subscriptions(&mut self, facade: BackendFacade, cx: &mut Context<Self>) {
        // 1. Live AI Match Assessment updates
        let mut assess_rx = facade.subscribe_assessment_updates();
        let assess_task = cx.spawn(async move |this, cx| {
            loop {
                match assess_rx.recv().await {
                    Ok(report_id) => {
                        let _ = this.update(cx, |ws, cx| {
                            if ws.reports.open_report_id == Some(report_id) {
                                ws.load_assessment_for_open_report(cx);
                            }
                        });
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let _ = this.update(cx, |ws, cx| {
                            if ws.reports.open_report_id.is_some() {
                                ws.load_assessment_for_open_report(cx);
                            }
                        });
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        // 2. Live Anki Auto-Sync updates
        let mut anki_rx = facade.subscribe_anki_sync_updates();
        let anki_task = cx.spawn(async move |this, cx| {
            while matches!(
                anki_rx.recv().await,
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
            ) {
                let _ = this.update(cx, |ws, cx| {
                    ws.refresh_anki_sync_modal(cx);
                    if let Some(deck_id) = ws.flashcards.selected_deck_uuid() {
                        ws.load_deck_auto_sync(deck_id, cx);
                    }
                });
            }
        });

        self._push_tasks = vec![assess_task, anki_task];
    }

    pub fn select_tab(&mut self, tab: StudyTab, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_tab == tab {
            return;
        }
        self.active_tab = tab;
        self.sync_anki_webview_visibility(cx);
        self.try_init_backend(window, cx);
        match tab {
            StudyTab::Overview => self.load_overview(Some(window), cx),
            StudyTab::Flashcards => {
                if self.flashcards.decks.is_empty() {
                    self.load_decks_and_cards(cx);
                }
            }
            StudyTab::Reports => {
                if self.reports.reports.is_empty() {
                    self.load_reports(false, cx);
                }
            }
            StudyTab::Mistakes => self.load_mistakes(cx),
            StudyTab::History => self.load_history(cx),
        }
        cx.notify();
    }

    fn step_tab(&mut self, step: RovingStep, window: &mut Window, cx: &mut Context<Self>) {
        let disabled = [false; 5];
        if let Some(next_idx) = next_roving_index(&disabled, self.active_tab.index(), step) {
            self.select_tab(StudyTab::ALL[next_idx], window, cx);
        }
    }

    // ── Data Loaders ────────────────────────────────────────────────────────

    fn load_overview(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        self.overview.summary_loading = true;
        self.overview.mistakes_loading = true;
        self.overview.summary_error = None;
        self.overview.mistakes_error = None;
        cx.notify();

        let player = self.selected_player.clone();
        let indep = self.independent_only;
        let configured = self.configured_player.clone();
        let locale = current_locale(cx);

        let read = spawn_facade_read(cx, move || async move {
            let summary = facade.study_overview().await;
            let prefs = facade.get_learning_preferences().await;
            let cfg_player = if configured.is_empty() {
                facade
                    .get_settings()
                    .await
                    .ok()
                    .map(|s| s.player_name)
                    .unwrap_or_default()
            } else {
                configured
            };
            let query = neondojo::recurring_mistakes::MistakesQuery {
                player: (!player.is_empty()).then_some(player),
                database_id: None,
                game_plan: None,
                min_loss: Some(0.08),
                independent_only: Some(indep),
                resolve: (!cfg_player.is_empty()).then_some(cfg_player.clone()),
            };
            let mistakes = facade.list_recurring_mistakes(query).await;
            (summary, prefs, cfg_player, mistakes)
        });

        let has_window = window.is_some();
        let _ = has_window;
        cx.spawn(async move |this, cx| {
            let Ok((summary, prefs, cfg_player, mistakes)) = read.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.overview.summary_loading = false;
                ws.overview.mistakes_loading = false;
                if !cfg_player.is_empty() {
                    ws.configured_player = cfg_player.clone();
                }
                match summary {
                    Ok(s) => {
                        ws.overview.new_cards_per_day =
                            clamp_new_cards_per_day(s.new_cards_per_day);
                        ws.overview.summary = Some(s);
                    }
                    Err(err) => {
                        ws.overview.summary_error = Some(format_domain_error(locale, &err));
                    }
                }
                if let Ok(p) = prefs {
                    ws.overview.new_cards_per_day = clamp_new_cards_per_day(p.new_cards_per_day);
                }
                match mistakes {
                    Ok(m) => {
                        if ws.selected_player.is_empty() {
                            let auto = m.resolved_player.clone().unwrap_or_else(|| {
                                default_study_player(&ws.configured_player, &m.players)
                            });
                            if !auto.is_empty() {
                                ws.selected_player = auto;
                                ws.overview.mistakes = Some(m);
                                ws.load_overview(None, cx);
                                return;
                            }
                        }
                        ws.overview.mistakes = Some(m);
                    }
                    Err(err) => {
                        ws.overview.mistakes_error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save_new_cards_per_day(&mut self, value: i32, window: &mut Window, cx: &mut Context<Self>) {
        let clamped = clamp_new_cards_per_day(value);
        self.overview.sync_new_cards_input(clamped, window, cx);
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        self.overview.saving_preferences = true;
        self.overview.preferences_notice = None;
        let locale = current_locale(cx);
        cx.notify();

        let task = spawn_facade_write(cx, move || async move {
            let saved = facade
                .update_learning_preferences(LearningPreferences {
                    new_cards_per_day: clamped,
                })
                .await?;
            let summary = facade.study_overview().await.ok();
            Ok::<_, neondojo::domain_error::DomainError>((saved, summary))
        });

        cx.spawn(async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.overview.saving_preferences = false;
                match res {
                    Ok((prefs, summary)) => {
                        ws.overview.new_cards_per_day =
                            clamp_new_cards_per_day(prefs.new_cards_per_day);
                        if let Some(s) = summary {
                            ws.overview.summary = Some(s);
                        }
                        ws.overview.preferences_notice =
                            Some(I18n::t(locale, "play-mode", "thresholdSaved").to_string());
                    }
                    Err(err) => {
                        ws.overview.summary_error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn set_selected_player(&mut self, player: String, cx: &mut Context<Self>) {
        if self.selected_player == player {
            return;
        }
        self.selected_player = player;
        self.load_overview(None, cx);
        self.load_mistakes(cx);
        cx.notify();
    }

    fn toggle_independent_only(&mut self, cx: &mut Context<Self>) {
        self.independent_only = !self.independent_only;
        let next = self.independent_only;
        update_local_ui_state(cx, |s| s.study.independent_only = next);
        self.load_overview(None, cx);
        self.load_mistakes(cx);
        cx.notify();
    }

    fn load_databases(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let read = spawn_facade_read(cx, move || async move { facade.list_databases().await });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(dbs)) = read.await {
                let _ = this.update(cx, |ws, cx| {
                    ws.mistakes.databases = dbs;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn load_decks_and_cards(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        self.flashcards.decks_loading = true;
        self.flashcards.cards_loading = true;
        let locale = current_locale(cx);
        let deck_sel = self.flashcards.selected_deck.clone();
        let query = self
            .flashcards
            .filters
            .to_query(&deck_sel, self.flashcards.page);
        let deck_uuid = self.flashcards.selected_deck_uuid();

        let read = spawn_facade_read(cx, move || async move {
            let decks = facade.list_decks().await;
            let cards = facade.search_cards(deck_uuid, query).await;
            let topics = facade.list_card_topics().await.unwrap_or_default();
            let tags = facade.suggest_card_tags().await.unwrap_or_default();
            let transfer_hist = facade.transfer_practice_history().await.unwrap_or_default();
            let auto_sync = match deck_uuid {
                Some(id) => facade.get_anki_auto_sync(id).await.ok(),
                None => None,
            };
            (decks, cards, topics, tags, transfer_hist, auto_sync)
        });

        cx.spawn(async move |this, cx| {
            let Ok((decks, cards, topics, tags, transfer_hist, auto_sync)) = read.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.flashcards.decks_loading = false;
                ws.flashcards.cards_loading = false;
                match decks {
                    Ok(list) => {
                        ws.flashcards.decks = list;
                        ws.flashcards.decks_error = None;
                    }
                    Err(err) => {
                        ws.flashcards.decks_error = Some(format_domain_error(locale, &err));
                    }
                }
                match cards {
                    Ok(page) => {
                        ws.flashcards.selection.refresh_from_page(&page.items);
                        ws.flashcards.card_page = Some(page);
                        ws.flashcards.cards_error = None;
                    }
                    Err(err) => {
                        ws.flashcards.cards_error = Some(format_domain_error(locale, &err));
                    }
                }
                ws.flashcards.saved_topics = topics;
                ws.flashcards.suggested_tags = tags;
                ws.flashcards.deck_auto_sync = auto_sync;
                ws.practice_round.transfer_history = transfer_hist;
                ws.flashcards.sync_preview_visuals(
                    ws.visible && ws.active_tab == StudyTab::Flashcards && !ws.fsrs.active,
                    cx,
                );
                cx.notify();
            });
        })
        .detach();
    }

    fn load_cards_only(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        self.flashcards.cards_loading = true;
        self.flashcards.cards_error = None;
        let locale = current_locale(cx);
        let deck_sel = self.flashcards.selected_deck.clone();
        let query = self
            .flashcards
            .filters
            .to_query(&deck_sel, self.flashcards.page);
        let deck_uuid = self.flashcards.selected_deck_uuid();

        let read = spawn_facade_read(cx, move || async move {
            facade.search_cards(deck_uuid, query).await
        });
        cx.spawn(async move |this, cx| {
            let Ok(res) = read.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.flashcards.cards_loading = false;
                match res {
                    Ok(page) => {
                        ws.flashcards.selection.refresh_from_page(&page.items);
                        ws.flashcards.card_page = Some(page);
                    }
                    Err(err) => {
                        ws.flashcards.cards_error = Some(format_domain_error(locale, &err));
                    }
                }
                ws.flashcards.sync_preview_visuals(
                    ws.visible && ws.active_tab == StudyTab::Flashcards && !ws.fsrs.active,
                    cx,
                );
                cx.notify();
            });
        })
        .detach();
    }

    fn load_deck_auto_sync(&mut self, deck_id: Uuid, cx: &mut Context<Self>) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let read = spawn_facade_read(cx, move || async move {
            facade.get_anki_auto_sync(deck_id).await.ok()
        });
        cx.spawn(async move |this, cx| {
            if let Ok(settings) = read.await {
                let _ = this.update(cx, |ws, cx| {
                    if ws.flashcards.selected_deck_uuid() == Some(deck_id) {
                        ws.flashcards.deck_auto_sync = settings;
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    // ── FSRS Review Flow ────────────────────────────────────────────────────

    fn start_fsrs_study(&mut self, deck_id: Uuid, cx: &mut Context<Self>) {
        self.active_tab = StudyTab::Flashcards;
        self.practice_round.round = None;
        self.flashcards.selected_deck = deck_id.to_string();
        update_local_ui_state(cx, |s| s.study.deck_id = deck_id.to_string());

        let deck_name = self
            .flashcards
            .decks
            .iter()
            .find(|d| d.id == deck_id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "Deck".into());

        self.fsrs.active = true;
        self.fsrs.deck_id = Some(deck_id);
        self.fsrs.deck_name = deck_name;
        self.fsrs.last_review = None;
        self.fsrs.conflict_notice = None;
        self.load_next_fsrs_card(cx);
        self.sync_anki_webview_visibility(cx);
        cx.notify();
    }

    fn load_next_fsrs_card(&mut self, cx: &mut Context<Self>) {
        let Some(deck_id) = self.fsrs.deck_id else {
            return;
        };
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        self.fsrs.loading = true;
        self.fsrs.error = None;
        let locale = current_locale(cx);
        let facade_for_turns = facade.clone();

        let read = spawn_facade_read(
            cx,
            move || async move { facade.next_fsrs_card(deck_id).await },
        );
        cx.spawn(async move |this, cx| {
            let Ok(res) = read.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                if !ws.fsrs.active || ws.fsrs.deck_id != Some(deck_id) {
                    return;
                }
                match res {
                    Ok(next) => {
                        ws.fsrs.apply_next_card(next, Some(&facade_for_turns), cx);
                        ws.sync_anki_webview_visibility(cx);
                    }
                    Err(err) => {
                        ws.fsrs.loading = false;
                        ws.fsrs.error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn handle_fsrs_board_click(&mut self, point: i32, cx: &mut Context<Self>) {
        if !self.fsrs.active || self.fsrs.effective_revealed().is_some() {
            return;
        }
        if let Some(ctrl) = self.fsrs.checker.as_mut()
            && ctrl.handle_point_click(point)
        {
            self.fsrs.sync_visuals(cx);
            cx.notify();
        }
    }

    fn reveal_fsrs_card(&mut self, cx: &mut Context<Self>) {
        if !self.fsrs.active || self.fsrs.busy || self.fsrs.effective_revealed().is_some() {
            return;
        }
        let Some(card) = self.fsrs.current_card().cloned() else {
            return;
        };
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        self.fsrs.busy = true;
        self.fsrs.error = None;
        let locale = current_locale(cx);

        let task = spawn_facade_write(cx, move || async move {
            facade.reveal_fsrs_card(card.id, card.version).await
        });
        cx.spawn(async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                match res {
                    Ok(rev) => {
                        ws.fsrs.apply_revealed(rev, cx);
                        ws.sync_anki_webview_visibility(cx);
                    }
                    Err(err) if err.kind == ErrorKind::Conflict => {
                        ws.fsrs
                            .handle_conflict_reload(format_domain_error(locale, &err), cx);
                        ws.load_next_fsrs_card(cx);
                    }
                    Err(err) => {
                        ws.fsrs.busy = false;
                        ws.fsrs.error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn check_fsrs_answer(&mut self, cx: &mut Context<Self>) {
        if !self.fsrs.active || self.fsrs.attempt_busy || self.fsrs.effective_revealed().is_some() {
            return;
        }
        let Some(card) = self.fsrs.current_card().cloned() else {
            return;
        };
        let Some(answer) = self.fsrs.build_pending_answer() else {
            return;
        };
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let request_id = self
            .fsrs
            .idempotency
            .attempt_request_id(card.id, card.version);
        self.fsrs.attempt_busy = true;
        self.fsrs.error = None;
        let locale = current_locale(cx);

        let task = spawn_facade_write(cx, move || async move {
            facade
                .submit_fsrs_attempt(
                    card.id,
                    CardAttemptRequest {
                        expected_version: card.version,
                        request_id,
                        answer,
                    },
                )
                .await
        });

        cx.spawn(async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                match res {
                    Ok(attempted) => {
                        ws.fsrs.apply_attempt(attempted, cx);
                        ws.sync_anki_webview_visibility(cx);
                    }
                    Err(err) if err.kind == ErrorKind::Conflict => {
                        ws.fsrs
                            .handle_conflict_reload(format_domain_error(locale, &err), cx);
                        ws.load_next_fsrs_card(cx);
                    }
                    Err(err) => {
                        ws.fsrs.attempt_busy = false;
                        ws.fsrs.error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn submit_fsrs_rating(&mut self, rating: Rating, cx: &mut Context<Self>) {
        if !self.fsrs.can_submit_rating() {
            return;
        }
        let Some((card_id, session_id)) = self
            .fsrs
            .effective_revealed()
            .map(|r| (r.card_id, r.session_id))
        else {
            return;
        };
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let request_id = self.fsrs.idempotency.grade_request_id(session_id, rating);
        self.fsrs.busy = true;
        self.fsrs.error = None;
        let locale = current_locale(cx);

        let task = spawn_facade_write(cx, move || async move {
            facade
                .submit_fsrs_grade(
                    card_id,
                    ReviewCard {
                        session_id,
                        request_id,
                        rating,
                    },
                )
                .await
        });

        cx.spawn(async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.fsrs.busy = false;
                match res {
                    Ok(result) => {
                        ws.fsrs.last_review = Some(result);
                        ws.fsrs.conflict_notice = None;
                        ws.fsrs.idempotency.reset();
                        ws.load_next_fsrs_card(cx);
                        ws.load_overview(None, cx);
                    }
                    Err(err) if err.kind == ErrorKind::Conflict => {
                        ws.fsrs
                            .handle_conflict_reload(format_domain_error(locale, &err), cx);
                        ws.load_next_fsrs_card(cx);
                    }
                    Err(err) => {
                        ws.fsrs.error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ── Non-FSRS Practice Round & Transfer Practice ─────────────────────────

    fn start_practice_round(&mut self, limit: i64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        self.fsrs.stop(cx);
        self.practice_round.round_size = limit;
        self.practice_round.loading = true;
        let deck_sel = self.flashcards.selected_deck.clone();
        let mut query = self.flashcards.filters.to_query(&deck_sel, 1);
        query.page = None;
        query.limit = Some(if limit == 20 { 20 } else { 10 });
        let locale = current_locale(cx);
        let facade_turns = facade.clone();

        let task = spawn_facade_write(cx, move || async move {
            facade.start_practice_round(query).await
        });

        cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |ws, window, cx| {
                match res {
                    Ok(round) => {
                        let rid = round.id;
                        ws.resumable_round_id = Some(rid);
                        update_local_ui_state(cx, |s| {
                            s.study.practice_round_id = Some(rid.to_string())
                        });
                        ws.practice_round
                            .apply_round(round, Some(&facade_turns), window, cx);
                        ws.sync_anki_webview_visibility(cx);
                    }
                    Err(err) => {
                        ws.practice_round.loading = false;
                        ws.flashcards.transfer_error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn resume_practice_round(
        &mut self,
        round_id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        self.fsrs.stop(cx);
        self.practice_round.loading = true;
        let locale = current_locale(cx);
        let facade_turns = facade.clone();
        let read = spawn_facade_read(cx, move || async move {
            facade.get_practice_round(round_id).await
        });

        cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = read.await else {
                return;
            };
            let _ = this.update_in(cx, |ws, window, cx| {
                match res {
                    Ok(round) => {
                        ws.practice_round
                            .apply_round(round, Some(&facade_turns), window, cx);
                        ws.sync_anki_webview_visibility(cx);
                    }
                    Err(err) => {
                        ws.practice_round.loading = false;
                        ws.resumable_round_id = None;
                        update_local_ui_state(cx, |s| s.study.practice_round_id = None);
                        ws.flashcards.transfer_error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn handle_round_board_click(&mut self, point: i32, cx: &mut Context<Self>) {
        if self
            .practice_round
            .round
            .as_ref()
            .is_none_or(|r| r.result.is_some())
        {
            return;
        }
        if let Some(ctrl) = self.practice_round.checker.as_mut()
            && ctrl.handle_point_click(point)
        {
            self.practice_round.sync_visuals(cx);
            cx.notify();
        }
    }

    fn submit_practice_round_item(
        &mut self,
        check_answer: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(round) = self.practice_round.round.as_ref() else {
            return;
        };
        let Some(card) = round.card.as_ref() else {
            return;
        };
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let answer = if check_answer {
            let Some(a) = self.practice_round.build_pending_answer() else {
                return;
            };
            Some(a)
        } else {
            None
        };
        let payload = RoundAnswer {
            version: round.version,
            card_version: card.version,
            request_id: self.practice_round.request_id,
            answer,
            confidence: self.practice_round.confidence,
        };
        let rid = round.id;
        self.practice_round.busy = true;
        let locale = current_locale(cx);
        let facade_turns = facade.clone();

        let task = spawn_facade_write(cx, move || async move {
            facade.submit_practice_round_item(rid, payload).await
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |ws, window, cx| {
                match res {
                    Ok(updated) => {
                        ws.practice_round
                            .apply_round(updated, Some(&facade_turns), window, cx);
                        ws.sync_anki_webview_visibility(cx);
                    }
                    Err(err) => {
                        ws.practice_round.busy = false;
                        ws.practice_round.error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn advance_or_mark_round(
        &mut self,
        advance: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(round) = self.practice_round.round.as_ref() else {
            return;
        };
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let lesson_raw = self.practice_round.lesson_input.read(cx).value();
        let lesson = (!lesson_raw.trim().is_empty()).then(|| lesson_raw.trim().to_string());
        let body = RoundAdvance {
            version: round.version,
            marked: if advance { round.marked } else { !round.marked },
            diagnosis: self
                .practice_round
                .diagnosis
                .map(|d| d.as_str().to_string()),
            lesson,
        };
        let rid = round.id;
        self.practice_round.busy = true;
        let locale = current_locale(cx);
        let facade_turns = facade.clone();

        let task = spawn_facade_write(cx, move || async move {
            if advance {
                facade.advance_practice_round(rid, body).await
            } else {
                facade.mark_practice_round(rid, body).await
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |ws, window, cx| {
                match res {
                    Ok(updated) => {
                        ws.practice_round
                            .apply_round(updated, Some(&facade_turns), window, cx);
                        ws.sync_anki_webview_visibility(cx);
                    }
                    Err(err) => {
                        ws.practice_round.busy = false;
                        ws.practice_round.error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn retry_practice_round(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(round) = self.practice_round.round.as_ref() else {
            return;
        };
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let (rid, ver) = (round.id, round.version);
        self.practice_round.busy = true;
        let locale = current_locale(cx);
        let facade_turns = facade.clone();

        let task = spawn_facade_write(cx, move || async move {
            facade.retry_practice_round(rid, ver).await
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |ws, window, cx| {
                match res {
                    Ok(updated) => {
                        ws.practice_round
                            .apply_round(updated, Some(&facade_turns), window, cx);
                        ws.sync_anki_webview_visibility(cx);
                    }
                    Err(err) => {
                        ws.practice_round.busy = false;
                        ws.practice_round.error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn start_transfer_practice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_player.trim().is_empty() {
            return;
        }
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let req = TransferRequest {
            player: self.selected_player.clone(),
            database_id: self.mistakes.database_id,
            game_plan: self.practice_round.transfer_game_plan,
            kind: self.practice_round.transfer_kind,
            follow_up: self.practice_round.transfer_follow_up,
        };
        self.fsrs.stop(cx);
        self.practice_round.loading = true;
        let locale = current_locale(cx);
        let facade_turns = facade.clone();

        let task = spawn_facade_write(cx, move || async move {
            facade.start_transfer_practice(req).await
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |ws, window, cx| {
                match res {
                    Ok(round) => {
                        ws.practice_round
                            .apply_round(round, Some(&facade_turns), window, cx);
                        ws.sync_anki_webview_visibility(cx);
                    }
                    Err(err) => {
                        ws.practice_round.loading = false;
                        ws.flashcards.transfer_error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ── Native macOS .apkg File Pickers & AnkiConnect Actions ───────────────

    fn prompt_and_export_apkg(&mut self, deck_id: Uuid, cx: &mut Context<Self>) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let deck_name = self
            .flashcards
            .decks
            .iter()
            .find(|d| d.id == deck_id)
            .map(|d| d.name.replace("::", "-"))
            .unwrap_or_else(|| "neondojo-deck".into());
        let suggested = format!("{deck_name}.apkg");
        let dir = dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let rx = cx.prompt_for_new_path(&dir, Some(&suggested));
        let locale = current_locale(cx);
        let cfg_locale = Some(locale_to_config_locale(locale));

        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.flashcards.transfer_busy = true;
                ws.flashcards.transfer_error = None;
                ws.flashcards.transfer_status =
                    Some(I18n::t(locale, "learning", "exporting").to_string());
                cx.notify();
            });

            let write_task = cx.update(|cx| {
                spawn_facade_write(cx, move || async move {
                    facade
                        .export_deck_apkg_to_path(deck_id, cfg_locale, &path)
                        .await
                })
            });
            let Ok(res) = write_task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.flashcards.transfer_busy = false;
                match res {
                    Ok(saved) => {
                        ws.flashcards.transfer_status = Some(format!(
                            "{}: {}",
                            I18n::t(locale, "learning", "export"),
                            saved.display()
                        ));
                    }
                    Err(err) => {
                        ws.flashcards.transfer_status = None;
                        ws.flashcards.transfer_error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn prompt_and_import_apkg(&mut self, deck_id: Option<Uuid>, cx: &mut Context<Self>) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let locale = current_locale(cx);
        let prompt_label = if deck_id.is_some() {
            I18n::t(locale, "card-practice", "ankiImport")
        } else {
            I18n::t(locale, "card-practice", "externalAnkiImport")
        };
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(SharedString::from(prompt_label)),
        });

        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            if paths.is_empty() {
                return;
            }
            let total = paths.len();
            let mut succeeded = 0usize;
            let mut last_error: Option<String> = None;

            for (idx, path) in paths.into_iter().enumerate() {
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("package.apkg")
                    .to_string();
                let _ = this.update(cx, |ws, cx| {
                    ws.flashcards.transfer_busy = true;
                    ws.flashcards.transfer_error = None;
                    ws.flashcards.transfer_status = Some(I18n::format(
                        locale,
                        "card-practice",
                        "ankiImporting",
                        &[
                            ("current", Param::from((idx + 1) as i64)),
                            ("total", Param::from(total as i64)),
                            ("file", Param::from(file_name.as_str())),
                        ],
                    ));
                    cx.notify();
                });

                let f = facade.clone();
                let p = path.clone();
                let step = cx.update(|cx| {
                    spawn_facade_write(cx, move || async move {
                        if let Some(did) = deck_id {
                            f.import_deck_apkg_from_path(did, &p)
                                .await
                                .map(|r| (r.added as usize, 1usize))
                        } else {
                            f.import_anki_package_from_path(&p).await.map(|decks| {
                                let cards: usize = decks.iter().map(|d| d.cards).sum();
                                (cards, decks.len())
                            })
                        }
                    })
                });
                match step.await {
                    Ok(Ok(_)) => succeeded += 1,
                    Ok(Err(err)) => {
                        last_error = Some(I18n::format(
                            locale,
                            "card-practice",
                            "ankiImportFailed",
                            &[
                                ("file", Param::from(file_name.as_str())),
                                (
                                    "error",
                                    Param::from(format_domain_error(locale, &err).as_str()),
                                ),
                            ],
                        ));
                    }
                    Err(err) => last_error = Some(err.to_string()),
                }
            }

            let _ = this.update(cx, |ws, cx| {
                ws.flashcards.transfer_busy = false;
                ws.flashcards.transfer_status = Some(I18n::format(
                    locale,
                    "card-practice",
                    "ankiPackagesImported",
                    &[
                        ("count", Param::from(succeeded as i64)),
                        ("total", Param::from(total as i64)),
                    ],
                ));
                ws.flashcards.transfer_error = last_error;
                ws.load_decks_and_cards(cx);
                ws.load_overview(None, cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn send_deck_to_anki(&mut self, deck_id: Uuid, cx: &mut Context<Self>) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let locale = current_locale(cx);
        let cfg_locale = Some(locale_to_config_locale(locale));
        let deck_name = self
            .flashcards
            .decks
            .iter()
            .find(|d| d.id == deck_id)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        self.flashcards.transfer_busy = true;
        self.flashcards.transfer_error = None;
        self.flashcards.transfer_status =
            Some(I18n::t(locale, "card-practice", "sendingAnki").to_string());
        cx.notify();

        let task = spawn_facade_write(cx, move || async move {
            facade.send_deck_to_anki(deck_id, cfg_locale).await
        });
        cx.spawn(async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.flashcards.transfer_busy = false;
                match res {
                    Ok(()) => {
                        ws.flashcards.transfer_status = Some(I18n::format(
                            locale,
                            "card-practice",
                            "ankiSent",
                            &[("deck", Param::from(deck_name.as_str()))],
                        ));
                        ws.load_deck_auto_sync(deck_id, cx);
                    }
                    Err(err) => {
                        ws.flashcards.transfer_status = None;
                        ws.flashcards.transfer_error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn sync_deck_with_anki(&mut self, deck_id: Uuid, cx: &mut Context<Self>) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let locale = current_locale(cx);
        let deck_name = self
            .flashcards
            .decks
            .iter()
            .find(|d| d.id == deck_id)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        self.flashcards.transfer_busy = true;
        self.flashcards.transfer_error = None;
        self.flashcards.transfer_status =
            Some(I18n::t(locale, "card-practice", "syncingAnkiReviews").to_string());
        cx.notify();

        let task = spawn_facade_write(cx, move || async move {
            facade.sync_deck_with_anki(deck_id).await
        });
        cx.spawn(async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.flashcards.transfer_busy = false;
                match res {
                    Ok(sync) => {
                        ws.flashcards.transfer_status = Some(I18n::format(
                            locale,
                            "card-practice",
                            "ankiReviewsSynced",
                            &[
                                ("deck", Param::from(deck_name.as_str())),
                                ("profile", Param::from(sync.profile.as_str())),
                                ("received", Param::from(sync.received_reviews as i64)),
                                ("sent", Param::from(sync.sent_reviews as i64)),
                                ("matched", Param::from(sync.matched_cards as i64)),
                                ("unmatched", Param::from(sync.unmatched_cards as i64)),
                            ],
                        ));
                        ws.load_decks_and_cards(cx);
                        ws.load_overview(None, cx);
                    }
                    Err(err) => {
                        ws.flashcards.transfer_status = None;
                        ws.flashcards.transfer_error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_deck_auto_sync(&mut self, deck_id: Uuid, enabled: bool, cx: &mut Context<Self>) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let locale = current_locale(cx);
        let task = spawn_facade_write(cx, move || async move {
            facade.update_anki_auto_sync(deck_id, enabled).await
        });
        cx.spawn(async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                match res {
                    Ok(settings) => ws.flashcards.deck_auto_sync = Some(settings),
                    Err(err) => {
                        ws.flashcards.transfer_error = Some(format_domain_error(locale, &err))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ── Modals: AnkiSyncOverview, DeckManagement, Duplicates, Batch Actions, Special Practice

    fn open_anki_sync_overview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let modal_state = cx.new(|_| AnkiSyncOverviewModalState {
            decks: Vec::new(),
            loading: true,
            busy: false,
            error: None,
            status_notice: None,
            only_open_errors: false,
            expanded_history_deck: None,
            history_rows: Vec::new(),
            history_loading: false,
            history_open_only: false,
            retry_progress: None,
        });
        self.anki_sync_modal = Some(modal_state.clone());
        self.refresh_anki_sync_modal(cx);

        let ws_entity = cx.entity();
        let cb = AnkiSyncOverviewCallbacks {
            on_sync_now: Rc::new({
                let ws = ws_entity.clone();
                let ms = modal_state.clone();
                move |deck_id, _, cx| {
                    let Some(facade) = Self::facade(cx) else {
                        return;
                    };
                    ms.update(cx, |s, cx| {
                        s.busy = true;
                        s.error = None;
                        cx.notify();
                    });
                    let locale = current_locale(cx);
                    let task = spawn_facade_write(cx, move || async move {
                        facade
                            .anki_auto_sync_action(
                                deck_id,
                                SyncActionRequest {
                                    action: SyncAction::SyncNow,
                                    retry_of: None,
                                    include_result: false,
                                },
                            )
                            .await
                    });
                    let ms2 = ms.clone();
                    let ws2 = ws.clone();
                    cx.spawn(async move |cx| {
                        let res = task.await;
                        cx.update(|cx| {
                            ms2.update(cx, |s, cx| {
                                s.busy = false;
                                if let Ok(Err(err)) = res {
                                    s.error = Some(format_domain_error(locale, &err));
                                }
                                cx.notify();
                            });
                            ws2.update(cx, |w, cx| w.refresh_anki_sync_modal(cx));
                        });
                    })
                    .detach();
                }
            }),
            on_pause_resume: Rc::new({
                let ws = ws_entity.clone();
                let ms = modal_state.clone();
                move |deck_id, pause, _, cx| {
                    let Some(facade) = Self::facade(cx) else {
                        return;
                    };
                    let action = if pause {
                        SyncAction::Pause
                    } else {
                        SyncAction::Resume
                    };
                    let task = spawn_facade_write(cx, move || async move {
                        facade
                            .anki_auto_sync_action(
                                deck_id,
                                SyncActionRequest {
                                    action,
                                    retry_of: None,
                                    include_result: false,
                                },
                            )
                            .await
                    });
                    let ws2 = ws.clone();
                    let ms2 = ms.clone();
                    let locale = current_locale(cx);
                    cx.spawn(async move |cx| {
                        let res = task.await;
                        cx.update(|cx| {
                            if let Ok(Err(err)) = res {
                                ms2.update(cx, |s, cx| {
                                    s.error = Some(format_domain_error(locale, &err));
                                    cx.notify();
                                });
                            }
                            ws2.update(cx, |w, cx| w.refresh_anki_sync_modal(cx));
                        });
                    })
                    .detach();
                }
            }),
            on_toggle_history: Rc::new({
                let ms = modal_state.clone();
                move |deck_id, _, cx| {
                    let closing = ms.read(cx).expanded_history_deck == Some(deck_id);
                    if closing {
                        ms.update(cx, |s, cx| {
                            s.expanded_history_deck = None;
                            s.history_rows.clear();
                            cx.notify();
                        });
                        return;
                    }
                    ms.update(cx, |s, cx| {
                        s.expanded_history_deck = Some(deck_id);
                        s.history_loading = true;
                        s.history_rows.clear();
                        cx.notify();
                    });
                    let Some(facade) = Self::facade(cx) else {
                        return;
                    };
                    let read = spawn_facade_read(cx, move || async move {
                        facade.anki_sync_history(deck_id).await
                    });
                    let ms2 = ms.clone();
                    cx.spawn(async move |cx| {
                        if let Ok(Ok(rows)) = read.await {
                            cx.update(|cx| {
                                ms2.update(cx, |s, cx| {
                                    s.history_loading = false;
                                    s.history_rows = rows;
                                    cx.notify();
                                });
                            });
                        }
                    })
                    .detach();
                }
            }),
            on_retry_entry: Rc::new({
                let ws = ws_entity.clone();
                let ms = modal_state.clone();
                move |deck_id, entry_id, _, cx| {
                    let Some(facade) = Self::facade(cx) else {
                        return;
                    };
                    let locale = current_locale(cx);
                    ms.update(cx, |s, cx| {
                        s.busy = true;
                        cx.notify();
                    });
                    let task = spawn_facade_write(cx, move || async move {
                        facade
                            .retry_anki_sync_error(deck_id, Some(entry_id), false)
                            .await
                    });
                    let ms2 = ms.clone();
                    let ws2 = ws.clone();
                    cx.spawn(async move |cx| {
                        let res = task.await;
                        cx.update(|cx| {
                            ms2.update(cx, |s, cx| {
                                s.busy = false;
                                match res {
                                    Ok(Ok(_)) => {
                                        s.status_notice = Some(
                                            I18n::t(
                                                locale,
                                                "card-practice",
                                                "ankiHistoryRetryCompleted",
                                            )
                                            .to_string(),
                                        );
                                    }
                                    Ok(Err(err)) => {
                                        s.error = Some(format_domain_error(locale, &err));
                                    }
                                    Err(_) => {}
                                }
                                cx.notify();
                            });
                            ws2.update(cx, |w, cx| w.refresh_anki_sync_modal(cx));
                        });
                    })
                    .detach();
                }
            }),
            on_retry_all_open: Rc::new({
                let ws = ws_entity.clone();
                let ms = modal_state.clone();
                move |_, cx| {
                    let Some(facade) = Self::facade(cx) else {
                        return;
                    };
                    let targets: Vec<Uuid> = ms
                        .read(cx)
                        .decks
                        .iter()
                        .filter(|d| d.open_errors > 0)
                        .map(|d| d.deck_id)
                        .collect();
                    if targets.is_empty() {
                        return;
                    }
                    let total = targets.len();
                    let locale = current_locale(cx);
                    ms.update(cx, |s, cx| {
                        s.busy = true;
                        s.error = None;
                        s.retry_progress = Some((0, total));
                        cx.notify();
                    });
                    let ms2 = ms.clone();
                    let ws2 = ws.clone();
                    cx.spawn(async move |cx| {
                        let mut succeeded = 0i64;
                        let mut failed = 0i64;
                        for (idx, did) in targets.into_iter().enumerate() {
                            let f = facade.clone();
                            let step = cx.update(|cx| {
                                spawn_facade_write(cx, move || async move {
                                    f.retry_anki_sync_error(did, None, true).await
                                })
                            });
                            match step.await {
                                Ok(Ok(Some(res))) if res.open_errors == 0 => succeeded += 1,
                                Ok(Ok(None)) => succeeded += 1,
                                _ => failed += 1,
                            }
                            cx.update(|cx| {
                                ms2.update(cx, |s, cx| {
                                    s.retry_progress = Some((idx + 1, total));
                                    cx.notify();
                                });
                            });
                        }
                        cx.update(|cx| {
                            ms2.update(cx, |s, cx| {
                                s.busy = false;
                                s.retry_progress = None;
                                s.status_notice = Some(I18n::format(
                                    locale,
                                    "card-practice",
                                    "ankiOverviewRetryAllResult",
                                    &[
                                        ("succeeded", Param::from(succeeded)),
                                        ("failed", Param::from(failed)),
                                    ],
                                ));
                                cx.notify();
                            });
                            ws2.update(cx, |w, cx| w.refresh_anki_sync_modal(cx));
                        });
                    })
                    .detach();
                }
            }),
            on_refresh: Rc::new({
                let ws = ws_entity.clone();
                move |_, cx| {
                    ws.update(cx, |w, cx| w.refresh_anki_sync_modal(cx));
                }
            }),
        };

        open_anki_sync_overview_modal(modal_state, cb, window, cx);
    }

    fn refresh_anki_sync_modal(&mut self, cx: &mut Context<Self>) {
        let Some(modal) = self.anki_sync_modal.clone() else {
            return;
        };
        let Some(facade) = Self::facade(cx) else {
            modal.update(cx, |s, cx| {
                s.loading = false;
                cx.notify();
            });
            return;
        };
        let locale = current_locale(cx);
        let read = spawn_facade_read(cx, move || async move { facade.anki_sync_overview().await });
        cx.spawn(async move |_, cx| {
            let Ok(res) = read.await else {
                return;
            };
            cx.update(|cx| {
                modal.update(cx, |s, cx| {
                    s.loading = false;
                    match res {
                        Ok(decks) => s.decks = decks,
                        Err(err) => s.error = Some(format_domain_error(locale, &err)),
                    }
                    cx.notify();
                });
            });
        })
        .detach();
    }

    fn open_deck_management(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected_id = self
            .flashcards
            .selected_deck_uuid()
            .or_else(|| self.flashcards.decks.first().map(|d| d.id));
        let initial_name = selected_id
            .and_then(|id| self.flashcards.decks.iter().find(|d| d.id == id))
            .map(|d| d.name.clone())
            .unwrap_or_default();
        let rename_input = cx.new(|cx| InputState::new(window, cx).default_value(initial_name));
        let state = cx.new(|_| DeckManageModalState {
            decks: self.flashcards.decks.clone(),
            selected_id,
            rename_input,
            confirm_delete: false,
            busy: false,
            notice: None,
            error: None,
        });

        let ws_entity = cx.entity();
        let on_rename = Rc::new({
            let ws = ws_entity.clone();
            move |st: Entity<DeckManageModalState>,
                  id: Uuid,
                  name: String,
                  ver: i64,
                  _window: &mut Window,
                  cx: &mut App| {
                let Some(facade) = Self::facade(cx) else {
                    return;
                };
                let locale = current_locale(cx);
                st.update(cx, |s, cx| {
                    s.busy = true;
                    s.error = None;
                    cx.notify();
                });
                let task = spawn_facade_write(cx, move || async move {
                    facade.update_deck(id, name, ver).await
                });
                let ws2 = ws.clone();
                cx.spawn(async move |cx| {
                    let res = task.await;
                    cx.update(|cx| {
                        st.update(cx, |s, cx| {
                            s.busy = false;
                            match res {
                                Ok(Ok(updated)) => {
                                    if let Some(slot) =
                                        s.decks.iter_mut().find(|d| d.id == updated.id)
                                    {
                                        *slot = updated;
                                    }
                                    s.notice = Some(
                                        I18n::t(locale, "play-mode", "thresholdSaved").to_string(),
                                    );
                                }
                                Ok(Err(err)) if err.kind == ErrorKind::Conflict => {
                                    s.error = Some(
                                        I18n::t(locale, "deck-management", "changed").to_string(),
                                    );
                                }
                                Ok(Err(err)) => {
                                    s.error = Some(format_domain_error(locale, &err));
                                }
                                Err(_) => {}
                            }
                            cx.notify();
                        });
                        ws2.update(cx, |w, cx| w.load_decks_and_cards(cx));
                    });
                })
                .detach();
            }
        });

        let on_delete = Rc::new({
            let ws = ws_entity.clone();
            move |st: Entity<DeckManageModalState>,
                  id: Uuid,
                  ver: i64,
                  _window: &mut Window,
                  cx: &mut App| {
                let Some(facade) = Self::facade(cx) else {
                    return;
                };
                let locale = current_locale(cx);
                st.update(cx, |s, cx| {
                    s.busy = true;
                    s.error = None;
                    cx.notify();
                });
                let task =
                    spawn_facade_write(
                        cx,
                        move || async move { facade.delete_deck(id, ver).await },
                    );
                let ws2 = ws.clone();
                cx.spawn(async move |cx| {
                    let res = task.await;
                    cx.update(|cx| {
                        st.update(cx, |s, cx| {
                            s.busy = false;
                            match res {
                                Ok(Ok(())) => {
                                    s.decks.retain(|d| d.id != id);
                                    s.selected_id = s.decks.first().map(|d| d.id);
                                    s.confirm_delete = false;
                                }
                                Ok(Err(err)) if err.kind == ErrorKind::Conflict => {
                                    s.error = Some(
                                        I18n::t(locale, "deck-management", "changed").to_string(),
                                    );
                                }
                                Ok(Err(err)) => {
                                    s.error = Some(format_domain_error(locale, &err));
                                }
                                Err(_) => {}
                            }
                            cx.notify();
                        });
                        ws2.update(cx, |w, cx| {
                            if w.flashcards.selected_deck_uuid() == Some(id) {
                                w.flashcards.selected_deck = ALL_CARDS_DECK.to_string();
                                update_local_ui_state(cx, |u| {
                                    u.study.deck_id = ALL_CARDS_DECK.to_string()
                                });
                            }
                            w.load_decks_and_cards(cx);
                        });
                    });
                })
                .detach();
            }
        });

        let on_refresh = Rc::new(
            move |st: Entity<DeckManageModalState>, _: &mut Window, cx: &mut App| {
                let Some(facade) = Self::facade(cx) else {
                    return;
                };
                let locale = current_locale(cx);
                let read = spawn_facade_read(cx, move || async move { facade.list_decks().await });
                cx.spawn(async move |cx| {
                    if let Ok(Ok(decks)) = read.await {
                        cx.update(|cx| {
                            st.update(cx, |s, cx| {
                                s.decks = decks;
                                s.error = None;
                                s.notice = Some(
                                    I18n::t(locale, "deck-management", "refreshed").to_string(),
                                );
                                cx.notify();
                            });
                        });
                    }
                })
                .detach();
            },
        );

        open_deck_management_dialog(state, on_rename, on_delete, on_refresh, window, cx);
    }

    fn open_duplicates_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = cx.new(|_| DuplicateCardsModalState {
            page: 1,
            data: None,
            loading: true,
            busy: false,
            error: None,
            keep_choice: Default::default(),
        });

        let load_page = Rc::new(
            move |st: Entity<DuplicateCardsModalState>,
                  page: usize,
                  _: &mut Window,
                  cx: &mut App| {
                let Some(facade) = Self::facade(cx) else {
                    st.update(cx, |s, cx| {
                        s.loading = false;
                        cx.notify();
                    });
                    return;
                };
                st.update(cx, |s, cx| {
                    s.page = page;
                    s.loading = true;
                    s.error = None;
                    cx.notify();
                });
                let locale = current_locale(cx);
                let read = spawn_facade_read(cx, move || async move {
                    facade
                        .list_duplicate_cards(DuplicateQuery {
                            after: None,
                            page: Some(page as i64),
                        })
                        .await
                });
                cx.spawn(async move |cx| {
                    let res = read.await;
                    cx.update(|cx| {
                        st.update(cx, |s, cx| {
                            s.loading = false;
                            match res {
                                Ok(Ok(data)) => s.data = Some(data),
                                Ok(Err(err)) => s.error = Some(format_domain_error(locale, &err)),
                                Err(_) => {}
                            }
                            cx.notify();
                        });
                    });
                })
                .detach();
            },
        );

        load_page(state.clone(), 1, window, cx);

        let ws_entity = cx.entity();
        let load_after_merge = load_page.clone();
        let on_merge = Rc::new(
            move |st: Entity<DuplicateCardsModalState>,
                  keep_id: Uuid,
                  keep_version: i64,
                  delete_id: Uuid,
                  delete_version: i64,
                  window: &mut Window,
                  cx: &mut App| {
                let Some(facade) = Self::facade(cx) else {
                    return;
                };
                let locale = current_locale(cx);
                st.update(cx, |s, cx| {
                    s.busy = true;
                    s.error = None;
                    cx.notify();
                });
                let task = spawn_facade_write(cx, move || async move {
                    facade
                        .merge_duplicate_cards(ResolveDuplicate {
                            keep_id,
                            keep_version,
                            delete_id,
                            delete_version,
                        })
                        .await
                });
                let ws2 = ws_entity.clone();
                let load2 = load_after_merge.clone();
                let page = st.read(cx).page;
                ws2.update(cx, |_, cx| {
                    cx.spawn_in(window, async move |this, cx| {
                        let res = task.await;
                        let _ = this.update_in(cx, |w, window, cx| {
                            st.update(cx, |s, cx| {
                                s.busy = false;
                                if let Ok(Err(err)) = &res {
                                    s.error = Some(format_domain_error(locale, err));
                                }
                                cx.notify();
                            });
                            if matches!(res, Ok(Ok(()))) {
                                load2(st.clone(), page, window, cx);
                                w.load_decks_and_cards(cx);
                            }
                        });
                    })
                    .detach();
                });
            },
        );

        open_duplicate_cards_dialog(state, on_merge, load_page, window, cx);
    }

    fn open_batch_card_dialog(
        &mut self,
        action: BatchCardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.flashcards.selection.len();
        if count == 0 {
            return;
        }
        let locale = current_locale(cx);
        let title = match action {
            BatchCardAction::AddToDeck => I18n::t(locale, "deck-library", "addCards"),
            BatchCardAction::MoveToDeck => I18n::t(locale, "deck-library", "actions"),
            BatchCardAction::RemoveFromDeck => I18n::t(locale, "deck-library", "removeCards"),
            BatchCardAction::DeletePermanently => I18n::t(locale, "deck-library", "deleteCards"),
        };
        let source_deck = self.flashcards.selected_deck_record().cloned();
        let decks: Vec<neondojo::learning::Deck> = self
            .flashcards
            .decks
            .iter()
            .filter(|d| {
                action != BatchCardAction::MoveToDeck
                    || Some(d.id) != source_deck.as_ref().map(|s| s.id)
            })
            .cloned()
            .collect();
        let target_id = cx.new(|_| decks.first().map(|d| d.id));
        let target_body = target_id.clone();
        let target_footer = target_id;
        let decks_body = decks.clone();
        let decks_footer = decks;
        let ws_entity = cx.entity();
        let source_name = source_deck
            .as_ref()
            .map(|d| d.name.clone())
            .unwrap_or_default();

        open_modal_dialog(
            window,
            cx,
            ModalConfig::new(title).size(ModalSize::Standard),
            move |_, cx| {
                let p = NeonTheme::current_palette(cx);
                let locale = current_locale(cx);
                let current_target = *target_body.read(cx);
                let help = match action {
                    BatchCardAction::AddToDeck => I18n::format(
                        locale,
                        "deck-library",
                        "addCardsHelp",
                        &[("count", Param::from(count as i64))],
                    ),
                    BatchCardAction::MoveToDeck => I18n::format(
                        locale,
                        "deck-library",
                        "moveHelp",
                        &[
                            ("count", Param::from(count as i64)),
                            ("name", Param::from(source_name.as_str())),
                        ],
                    ),
                    BatchCardAction::RemoveFromDeck => I18n::format(
                        locale,
                        "deck-library",
                        "removeCardsHelp",
                        &[
                            ("count", Param::from(count as i64)),
                            ("name", Param::from(source_name.as_str())),
                        ],
                    ),
                    BatchCardAction::DeletePermanently => I18n::format(
                        locale,
                        "deck-library",
                        "deleteCardsHelp",
                        &[("count", Param::from(count as i64))],
                    ),
                };

                let mut body = div()
                    .id("batch-card-action-modal")
                    .test_support()
                    .v_flex()
                    .gap(px(10.0))
                    .child(div().text_body().text_color(p.text).child(help));

                if matches!(
                    action,
                    BatchCardAction::AddToDeck | BatchCardAction::MoveToDeck
                ) {
                    let mut picker = div().h_flex().flex_wrap().gap(px(6.0));
                    for deck in &decks_body {
                        let did = deck.id;
                        let tb = target_body.clone();
                        picker = picker.child(
                            ActionButton::new(SharedString::from(format!("batch-target-{did}")))
                                .label(deck.name.clone())
                                .active(current_target == Some(did))
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    tb.update(cx, |slot, cx| {
                                        *slot = Some(did);
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    body = body.child(picker);
                }
                body.into_any_element()
            },
            Some(move |_: &mut Window, cx: &mut App| {
                let p = NeonTheme::current_palette(cx);
                let locale = current_locale(cx);
                let ws_run = ws_entity.clone();
                let tf = target_footer.clone();
                let df = decks_footer.clone();
                let src = source_deck.clone();
                div()
                    .h_flex()
                    .gap(px(8.0))
                    .child(
                        ActionButton::new("batch-cancel")
                            .label(I18n::t(locale, "deck-management", "cancel"))
                            .palette(p)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        ActionButton::new("batch-confirm")
                            .label(title)
                            .variant(if action == BatchCardAction::DeletePermanently {
                                crate::ui::action_button::ActionButtonVariant::DangerSolid
                            } else {
                                crate::ui::action_button::ActionButtonVariant::Primary
                            })
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                let target_deck = (*tf.read(cx))
                                    .and_then(|id| df.iter().find(|d| d.id == id).cloned());
                                window.close_dialog(cx);
                                ws_run.update(cx, |ws, cx| {
                                    ws.execute_batch_card_action(
                                        action,
                                        src.clone(),
                                        target_deck,
                                        cx,
                                    );
                                });
                            }),
                    )
                    .into_any_element()
            }),
            None,
        );
    }

    fn execute_batch_card_action(
        &mut self,
        action: BatchCardAction,
        source: Option<neondojo::learning::Deck>,
        target: Option<neondojo::learning::Deck>,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let ids = self.flashcards.selection.ids();
        let metas = self.flashcards.selection.metas();
        if ids.is_empty() {
            return;
        }
        let locale = current_locale(cx);

        let task = spawn_facade_write(cx, move || async move {
            match action {
                BatchCardAction::AddToDeck => {
                    let Some(tgt) = target else {
                        return Ok(String::new());
                    };
                    let res = facade
                        .add_deck_cards(
                            tgt.id,
                            AddDeckCards {
                                expected_version: tgt.version,
                                card_ids: ids,
                            },
                        )
                        .await?;
                    Ok(I18n::format(
                        locale,
                        "deck-library",
                        "addedCards",
                        &[
                            ("name", Param::from(tgt.name.as_str())),
                            ("count", Param::from(res.added as i64)),
                            ("existing", Param::from(res.already_in_target as i64)),
                        ],
                    ))
                }
                BatchCardAction::MoveToDeck => {
                    let (Some(src), Some(tgt)) = (source, target) else {
                        return Ok(String::new());
                    };
                    let res = facade
                        .move_cards(
                            src.id,
                            MoveCards {
                                source_version: src.version,
                                target_id: tgt.id,
                                target_version: tgt.version,
                                card_ids: ids,
                            },
                        )
                        .await?;
                    Ok(I18n::format(
                        locale,
                        "deck-library",
                        "moved",
                        &[
                            ("count", Param::from(res.moved as i64)),
                            ("name", Param::from(tgt.name.as_str())),
                        ],
                    ))
                }
                BatchCardAction::RemoveFromDeck => {
                    let Some(src) = source else {
                        return Ok(String::new());
                    };
                    let n = ids.len() as i64;
                    facade
                        .remove_deck_cards(
                            src.id,
                            RemoveCards {
                                expected_version: src.version,
                                card_ids: ids,
                            },
                        )
                        .await?;
                    Ok(I18n::format(
                        locale,
                        "deck-library",
                        "removedCards",
                        &[
                            ("name", Param::from(src.name.as_str())),
                            ("count", Param::from(n)),
                        ],
                    ))
                }
                BatchCardAction::DeletePermanently => {
                    let n = metas.len() as i64;
                    let cards = metas
                        .into_iter()
                        .map(|m| DeleteCard {
                            id: m.id,
                            version: m.version,
                            deck_ids: m.deck_ids,
                        })
                        .collect();
                    facade.delete_cards(DeleteCards { cards }).await?;
                    Ok(I18n::format(
                        locale,
                        "deck-library",
                        "deletedCards",
                        &[("count", Param::from(n))],
                    ))
                }
            }
        });

        cx.spawn(async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                match res {
                    Ok(msg) => {
                        ws.flashcards.selection.clear();
                        if !msg.is_empty() {
                            ws.flashcards.transfer_status = Some(msg);
                        }
                        ws.flashcards.transfer_error = None;
                        ws.load_decks_and_cards(cx);
                        ws.load_overview(None, cx);
                    }
                    Err(err) => {
                        ws.flashcards.transfer_error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_pips_modal(&mut self, card: CardQuestion, window: &mut Window, cx: &mut Context<Self>) {
        let board_view = cx.new(|cx| themed_board("study-pips-board", cx));
        let on_submit = Rc::new(
            move |card_id: Uuid,
                  counts: [i32; 2],
                  st: Entity<PipPracticeModalState>,
                  _: &mut Window,
                  cx: &mut App| {
                let Some(facade) = Self::facade(cx) else {
                    return;
                };
                let locale = current_locale(cx);
                st.update(cx, |s, cx| {
                    s.busy = true;
                    s.error = None;
                    cx.notify();
                });
                let task = spawn_facade_write(cx, move || async move {
                    facade.check_card_pips(PipAnswer { card_id, counts }).await
                });
                cx.spawn(async move |cx| {
                    let res = task.await;
                    cx.update(|cx| {
                        st.update(cx, |s, cx| {
                            s.busy = false;
                            match res {
                                Ok(Ok(pip_res)) => s.result = Some(pip_res),
                                Ok(Err(err)) => s.error = Some(format_domain_error(locale, &err)),
                                Err(_) => {}
                            }
                            cx.notify();
                        });
                    });
                })
                .detach();
            },
        );
        open_pip_practice_modal(card, board_view, on_submit, window, cx);
    }

    fn open_score_variants(&mut self, card_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let board_view = cx.new(|cx| themed_board("study-score-variants-board", cx));
        let state = cx.new(|_| ScoreVariantsModalState {
            card_id,
            practice: None,
            index: 0,
            evaluation: None,
            loading: true,
            busy: false,
            error: None,
            board_view,
        });

        if let Some(facade) = Self::facade(cx) {
            let st_load = state.clone();
            let locale = current_locale(cx);
            let task = spawn_facade_write(cx, move || async move {
                facade.start_score_variants(card_id).await
            });
            cx.spawn(async move |_, cx| {
                let res = task.await;
                cx.update(|cx| {
                    st_load.update(cx, |s, cx| {
                        s.loading = false;
                        match res {
                            Ok(Ok(pr)) => {
                                s.practice = Some(pr);
                                s.sync_board(cx);
                            }
                            Ok(Err(err)) => s.error = Some(format_domain_error(locale, &err)),
                            Err(_) => {}
                        }
                        cx.notify();
                    });
                });
            })
            .detach();
        } else {
            state.update(cx, |s, _| s.loading = false);
        }

        let on_answer = Rc::new(
            move |st: Entity<ScoreVariantsModalState>,
                  answer: PracticeAnswer,
                  _: &mut Window,
                  cx: &mut App| {
                let Some(facade) = Self::facade(cx) else {
                    return;
                };
                let (card_id, practice_id, index) = {
                    let s = st.read(cx);
                    let Some(pr) = &s.practice else {
                        return;
                    };
                    (s.card_id, pr.id, s.index)
                };
                let locale = current_locale(cx);
                st.update(cx, |s, cx| {
                    s.busy = true;
                    s.error = None;
                    cx.notify();
                });
                let task = spawn_facade_write(cx, move || async move {
                    facade
                        .score_practice_variants(
                            card_id,
                            ScoreAnswer {
                                index,
                                practice_id,
                                answer,
                            },
                        )
                        .await
                });
                cx.spawn(async move |cx| {
                    let res = task.await;
                    cx.update(|cx| {
                        st.update(cx, |s, cx| {
                            s.busy = false;
                            match res {
                                Ok(Ok(eval)) => s.evaluation = Some(eval),
                                Ok(Err(err)) => s.error = Some(format_domain_error(locale, &err)),
                                Err(_) => {}
                            }
                            cx.notify();
                        });
                    });
                })
                .detach();
            },
        );

        open_score_variants_modal(state, on_answer, window, cx);
    }

    fn open_timed_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cards = self
            .flashcards
            .card_page
            .as_ref()
            .map(|p| {
                p.items
                    .iter()
                    .map(|i| neondojo::learning::CardBrowseItem {
                        card: i.card.clone(),
                        browse: neondojo::learning::CardBrowseContext {
                            decks: Vec::new(),
                            tags: i.browse.tags.clone(),
                            card_tags: i.browse.card_tags.clone(),
                            tags_version: i.browse.tags_version,
                            comment_excerpt: i.browse.comment_excerpt.clone(),
                        },
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let board_view = cx.new(|cx| themed_board("study-timed-preview-board", cx));
        let anki_face_view = cx.new(|cx| crate::anki_webview::AnkiCardFaceView::new(None, cx));
        let state = cx.new(|_| TimedPreviewModalState {
            cards,
            index: 0,
            showing_answer: false,
            running: false,
            viewed_count: 1,
            question_seconds: 8,
            answer_seconds: 4,
            current_answer: None,
            board_view,
            anki_face_view,
        });
        state.update(cx, |s, cx| s.sync_visuals(cx));

        let step_next =
            Rc::new(
                move |st: Entity<TimedPreviewModalState>, _: &mut Window, cx: &mut App| {
                    let need_preview_id = st.update(cx, |s, cx| {
                        if s.cards.is_empty() {
                            return None;
                        }
                        if !s.showing_answer {
                            s.showing_answer = true;
                            s.sync_visuals(cx);
                            cx.notify();
                            s.cards.get(s.index).map(|c| c.card.id)
                        } else {
                            s.index = (s.index + 1) % s.cards.len();
                            s.showing_answer = false;
                            s.current_answer = None;
                            s.viewed_count += 1;
                            s.sync_visuals(cx);
                            cx.notify();
                            None
                        }
                    });
                    if let (Some(card_id), Some(facade)) = (need_preview_id, Self::facade(cx)) {
                        let read = spawn_facade_read(cx, move || async move {
                            facade.preview_card(card_id).await
                        });
                        let st2 = st.clone();
                        cx.spawn(async move |cx| {
                            if let Ok(Ok(ans)) = read.await {
                                cx.update(|cx| {
                                    st2.update(cx, |s, cx| {
                                        s.current_answer = Some(ans);
                                        s.sync_visuals(cx);
                                        cx.notify();
                                    });
                                });
                            }
                        })
                        .detach();
                    }
                },
            );

        let step_for_timer = step_next.clone();
        let on_toggle = Rc::new(
            move |st: Entity<TimedPreviewModalState>, window: &mut Window, cx: &mut App| {
                let now_running = st.update(cx, |s, cx| {
                    s.running = !s.running;
                    cx.notify();
                    s.running
                });
                if now_running {
                    let st_loop = st.clone();
                    let step_cb = step_for_timer.clone();
                    st.update(cx, |_, cx| {
                        cx.spawn_in(window, async move |this, cx| {
                            loop {
                                let (running, delay_secs) = this
                                    .read_with(cx, |s, _| {
                                        (
                                            s.running,
                                            if s.showing_answer {
                                                s.answer_seconds
                                            } else {
                                                s.question_seconds
                                            },
                                        )
                                    })
                                    .unwrap_or((false, 4));
                                if !running {
                                    break;
                                }
                                cx.background_executor()
                                    .timer(Duration::from_secs(delay_secs.max(1)))
                                    .await;
                                let still_running = cx
                                    .update(|window, cx| {
                                        if !st_loop.read(cx).running {
                                            return false;
                                        }
                                        step_cb(st_loop.clone(), window, cx);
                                        true
                                    })
                                    .unwrap_or(false);
                                if !still_running {
                                    break;
                                }
                            }
                        })
                        .detach();
                    });
                }
            },
        );

        open_timed_preview_modal(state, on_toggle, step_next, window, cx);
    }

    // ── Reports & AI Assessment Loaders ─────────────────────────────────────

    fn load_reports(&mut self, load_more: bool, cx: &mut Context<Self>) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let after = if load_more {
            let Some(cursor) = self.reports.next_cursor.clone() else {
                return;
            };
            self.reports.loading_more = true;
            Some(cursor)
        } else {
            self.reports.loading = true;
            None
        };
        self.reports.error = None;
        let locale = current_locale(cx);
        cx.notify();

        let read = spawn_facade_read(cx, move || async move {
            facade
                .list_study_reports(ReportQuery {
                    limit: Some(25),
                    after,
                    group_by: Some("session".into()),
                })
                .await
        });

        cx.spawn(async move |this, cx| {
            let Ok(res) = read.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.reports.loading = false;
                ws.reports.loading_more = false;
                match res {
                    Ok(page) => {
                        if load_more {
                            ws.reports.reports.extend(page.items);
                        } else {
                            ws.reports.reports = page.items;
                        }
                        ws.reports.next_cursor = page.next_cursor;
                        if let Some(pending_mid) = ws.reports.pending_open_match_id
                            && let Some(rid) = ws.find_report_id_for_match(pending_mid)
                        {
                            ws.reports.pending_open_match_id = None;
                            ws.open_report_by_id(rid, cx);
                        }
                    }
                    Err(err) => {
                        ws.reports.error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_report_by_id(&mut self, report_id: Uuid, cx: &mut Context<Self>) {
        self.reports.close_open_report();
        self.reports.open_report_id = Some(report_id);
        self.reports.open_loading = true;
        let Some(facade) = Self::facade(cx) else {
            self.reports.open_loading = false;
            cx.notify();
            return;
        };
        let locale = current_locale(cx);
        let cfg_locale = locale_to_config_locale(locale);

        let read = spawn_facade_read(cx, move || async move {
            let session = facade.get_study_report(report_id).await?;
            let assessment = if session.match_length > 0 {
                facade
                    .get_match_assessment(report_id, cfg_locale)
                    .await
                    .ok()
            } else {
                None
            };
            Ok::<_, neondojo::domain_error::DomainError>((session, assessment))
        });

        cx.spawn(async move |this, cx| {
            let Ok(res) = read.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                if ws.reports.open_report_id != Some(report_id) {
                    return;
                }
                ws.reports.open_loading = false;
                match res {
                    Ok((session, assessment)) => {
                        ws.reports.detail_tab = if session.match_length > 0 {
                            ReportDetailTab::Assessment
                        } else {
                            ReportDetailTab::Decisions
                        };
                        ws.reports.open_session = Some(session);
                        ws.reports.assessment = assessment;
                    }
                    Err(err) => {
                        ws.reports.open_error = Some(format_domain_error(locale, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_assessment_for_open_report(&mut self, cx: &mut Context<Self>) {
        let Some(report_id) = self.reports.open_report_id else {
            return;
        };
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        self.reports.assessment_loading = true;
        self.reports.assessment_error = None;
        let locale = current_locale(cx);
        let cfg_locale = locale_to_config_locale(locale);

        let read = spawn_facade_read(cx, move || async move {
            facade.get_match_assessment(report_id, cfg_locale).await
        });
        cx.spawn(async move |this, cx| {
            let Ok(res) = read.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                if ws.reports.open_report_id != Some(report_id) {
                    return;
                }
                ws.reports.assessment_loading = false;
                match res {
                    Ok(state) => ws.reports.assessment = Some(state),
                    Err(err) => {
                        ws.reports.assessment_error = Some(format_domain_error(locale, &err))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn start_assessment_for_open_report(&mut self, cx: &mut Context<Self>) {
        let (Some(report_id), Some(session)) =
            (self.reports.open_report_id, &self.reports.open_session)
        else {
            return;
        };
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let version = session.study_report_version.unwrap_or(session.version);
        let locale = current_locale(cx);
        let cfg_locale = locale_to_config_locale(locale);
        self.reports.assessment_loading = true;
        self.reports.assessment_error = None;
        cx.notify();

        let task = spawn_facade_write(cx, move || async move {
            facade
                .start_match_assessment(report_id, cfg_locale, version)
                .await
        });
        cx.spawn(async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.reports.assessment_loading = false;
                match res {
                    Ok(state) => ws.reports.assessment = Some(state),
                    Err(err) => {
                        ws.reports.assessment_error = Some(format_domain_error(locale, &err))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn ensure_report_history_positions(&mut self, cx: &mut Context<Self>) {
        if !self.reports.positions.is_empty() || self.reports.positions_loading {
            return;
        }
        let (Some(report_id), Some(session)) =
            (self.reports.open_report_id, &self.reports.open_session)
        else {
            return;
        };
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let version = session.study_report_version.unwrap_or(session.version);
        self.reports.positions_loading = true;
        let locale = current_locale(cx);

        let read = spawn_facade_read(cx, move || async move {
            facade.study_report_history(report_id, version).await
        });
        cx.spawn(async move |this, cx| {
            let Ok(res) = read.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                if ws.reports.open_report_id != Some(report_id) {
                    return;
                }
                ws.reports.positions_loading = false;
                match res {
                    Ok(history) => {
                        if let Some(review) = ws
                            .reports
                            .open_session
                            .as_ref()
                            .and_then(|s| s.review.as_ref())
                        {
                            ws.reports.positions =
                                build_learning_review_positions(review, &history);
                            ws.reports.sync_preview_board(cx);
                        }
                    }
                    Err(err) => {
                        ws.reports.positions_error = Some(format_domain_error(locale, &err))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ── Recurring Mistakes & Practice Modal ─────────────────────────────────

    fn load_mistakes(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        self.mistakes.loading = true;
        self.mistakes.error = None;
        let query = self.mistakes.build_query(
            &self.selected_player,
            self.independent_only,
            (!self.configured_player.is_empty()).then(|| self.configured_player.clone()),
            cx,
        );
        let locale = current_locale(cx);

        let read = spawn_facade_read(cx, move || async move {
            facade.list_recurring_mistakes(query).await
        });
        cx.spawn(async move |this, cx| {
            let Ok(res) = read.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.mistakes.loading = false;
                match res {
                    Ok(data) => ws.mistakes.data = Some(data),
                    Err(err) => ws.mistakes.error = Some(format_domain_error(locale, &err)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_unanalyzed_matches(&mut self, cx: &mut Context<Self>) {
        if self.selected_player.trim().is_empty() {
            return;
        }
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        let player = self.selected_player.clone();
        let db = self.mistakes.database_id;
        let indep = self.independent_only;
        self.mistakes.unanalyzed_loading = true;

        let read = spawn_facade_read(cx, move || async move {
            facade
                .list_unanalyzed_matches(&player, db, indep, Some(100))
                .await
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(items)) = read.await {
                let _ = this.update(cx, |ws, cx| {
                    ws.mistakes.unanalyzed_loading = false;
                    ws.mistakes.unanalyzed = items;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn launch_mistake_practice(
        &mut self,
        game_plan: GamePlan,
        mistake_kind: MistakeKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locale = current_locale(cx);
        let title = format!(
            "{} · {}",
            game_plan_label(locale, game_plan),
            mistake_kind_label(locale, mistake_kind)
        );
        let board_view = cx.new(|cx| themed_board("study-mistake-practice-board", cx));
        let modal_state = cx.new(|_| MistakePracticeModalState {
            title,
            items: Vec::new(),
            index: 0,
            loading: true,
            busy: false,
            error: None,
            checker: None,
            selected_cube_offer: None,
            selected_cube_response: None,
            attempt: None,
            revealed: None,
            board_view: board_view.clone(),
        });

        // Subscribe board clicks directly to modal_state
        let ms_board = modal_state.clone();
        let sub = cx.subscribe(&board_view, move |_, _, event: &BoardEvent, cx| {
            if let BoardEvent::PointClicked(pt) = event {
                ms_board.update(cx, |s, cx| {
                    if s.attempt.is_none()
                        && s.revealed.is_none()
                        && let Some(ctrl) = s.checker.as_mut()
                        && ctrl.handle_point_click(*pt)
                    {
                        s.sync_board(cx);
                        cx.notify();
                    }
                });
            }
        });
        self._board_subs.push(sub);
        self.active_mistake_modal = Some(modal_state.clone());

        if let Some(facade) = Self::facade(cx) {
            let req = MistakePracticeRequest {
                player: self.selected_player.clone(),
                database_id: self.mistakes.database_id,
                game_plan,
                min_loss: self.mistakes.parsed_min_loss(cx).or(Some(0.08)),
                mistake_kind,
                independent_only: self.independent_only,
            };
            let facade_turns = facade.clone();
            let ms_load = modal_state.clone();
            let task = spawn_facade_write(cx, move || async move {
                facade.practice_recurring_mistake(req).await
            });
            cx.spawn(async move |_, cx| {
                let res = task.await;
                cx.update(|cx| {
                    ms_load.update(cx, |s, cx| {
                        s.loading = false;
                        match res {
                            Ok(Ok(practice)) => {
                                s.items = practice.items;
                                s.index = 0;
                                s.load_current_question(Some(&facade_turns), cx);
                            }
                            Ok(Err(err)) => s.error = Some(format_domain_error(locale, &err)),
                            Err(_) => {}
                        }
                        cx.notify();
                    });
                });
            })
            .detach();
        } else {
            modal_state.update(cx, |s, _| s.loading = false);
        }

        let on_check = Rc::new(
            move |st: Entity<MistakePracticeModalState>, _: &mut Window, cx: &mut App| {
                let Some(facade) = Self::facade(cx) else {
                    return;
                };
                let (reference, answer) = {
                    let s = st.read(cx);
                    let Some(q) = s.current() else {
                        return;
                    };
                    let Some(ans) = s.build_pending_answer() else {
                        return;
                    };
                    (q.reference.clone(), ans)
                };
                let locale = current_locale(cx);
                st.update(cx, |s, cx| {
                    s.busy = true;
                    s.error = None;
                    cx.notify();
                });
                let task = spawn_facade_write(cx, move || async move {
                    facade
                        .attempt_recurring_mistake(MistakeAttemptRequest { reference, answer })
                        .await
                });
                cx.spawn(async move |cx| {
                    let res = task.await;
                    cx.update(|cx| {
                        st.update(cx, |s, cx| {
                            s.busy = false;
                            match res {
                                Ok(Ok(att)) => {
                                    s.attempt = Some(att);
                                    s.sync_board(cx);
                                }
                                Ok(Err(err)) => s.error = Some(format_domain_error(locale, &err)),
                                Err(_) => {}
                            }
                            cx.notify();
                        });
                    });
                })
                .detach();
            },
        );

        let on_reveal = Rc::new(
            move |st: Entity<MistakePracticeModalState>, _: &mut Window, cx: &mut App| {
                let Some(facade) = Self::facade(cx) else {
                    return;
                };
                let Some(reference) = st.read(cx).current().map(|q| q.reference.clone()) else {
                    return;
                };
                let locale = current_locale(cx);
                st.update(cx, |s, cx| {
                    s.busy = true;
                    s.error = None;
                    cx.notify();
                });
                let task = spawn_facade_write(cx, move || async move {
                    facade.reveal_recurring_mistake(&reference).await
                });
                cx.spawn(async move |cx| {
                    let res = task.await;
                    cx.update(|cx| {
                        st.update(cx, |s, cx| {
                            s.busy = false;
                            match res {
                                Ok(Ok(ans)) => {
                                    s.revealed = Some(ans);
                                    s.sync_board(cx);
                                }
                                Ok(Err(err)) => s.error = Some(format_domain_error(locale, &err)),
                                Err(_) => {}
                            }
                            cx.notify();
                        });
                    });
                })
                .detach();
            },
        );

        let on_next = Rc::new(
            move |st: Entity<MistakePracticeModalState>, _: &mut Window, cx: &mut App| {
                let facade = Self::facade(cx);
                st.update(cx, |s, cx| {
                    s.index += 1;
                    s.load_current_question(facade.as_ref(), cx);
                    cx.notify();
                });
            },
        );

        open_mistake_practice_modal(modal_state, on_check, on_reveal, on_next, window, cx);
    }

    // ── Practice History Loader ─────────────────────────────────────────────

    fn load_history(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        self.history.loading = true;
        self.history.error = None;
        let locale = current_locale(cx);

        let read = spawn_facade_read(cx, move || async move {
            let mistakes = facade.list_mistake_practice_history().await;
            let similar = facade.list_similar_practice_history(None).await;
            let transfer = facade.transfer_practice_history().await;
            (mistakes, similar, transfer)
        });
        cx.spawn(async move |this, cx| {
            let Ok((mistakes, similar, transfer)) = read.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.history.loading = false;
                match mistakes {
                    Ok(h) => ws.history.mistake_days = h.items,
                    Err(err) => ws.history.error = Some(format_domain_error(locale, &err)),
                }
                if let Ok(sim) = similar {
                    ws.history.similar_rounds = sim.items;
                    ws.history.similar_next_cursor = sim.next_cursor;
                }
                if let Ok(tr) = transfer {
                    ws.history.transfer_rounds = tr;
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_more_similar_history(&mut self, cx: &mut Context<Self>) {
        let Some(cursor) = self.history.similar_next_cursor else {
            return;
        };
        let Some(facade) = Self::facade(cx) else {
            return;
        };
        self.history.loading_more_similar = true;
        let read = spawn_facade_read(cx, move || async move {
            facade.list_similar_practice_history(Some(cursor)).await
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(page)) = read.await {
                let _ = this.update(cx, |ws, cx| {
                    ws.history.loading_more_similar = false;
                    ws.history.similar_rounds.extend(page.items);
                    ws.history.similar_next_cursor = page.next_cursor;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    // ── Keyboard Shortcuts (Study Exercise + Splitter + Report Escape/Arrows)

    fn adjust_preview_width(&mut self, delta: i32, cx: &mut Context<Self>) {
        let next = clamp_preview_width(
            (self.flashcards.preview_width as i32 + delta).max(MIN_CARD_PREVIEW_WIDTH as i32)
                as u32,
        );
        self.flashcards.preview_width = next;
        update_local_ui_state(cx, |s| s.card_preview_width = Some(next));
        cx.notify();
    }

    fn set_preview_width(&mut self, width: u32, cx: &mut Context<Self>) {
        let next = clamp_preview_width(width);
        self.flashcards.preview_width = next;
        update_local_ui_state(cx, |s| s.card_preview_width = Some(next));
        cx.notify();
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_tab == StudyTab::Reports && self.reports.open_report_id.is_some() {
            match event.keystroke.key.as_str() {
                "escape" => {
                    self.reports.close_open_report();
                    cx.stop_propagation();
                    cx.notify();
                }
                "up" if self.reports.step_decision(-1, cx) => {
                    self.ensure_report_history_positions(cx);
                    cx.stop_propagation();
                    cx.notify();
                }
                "down" if self.reports.step_decision(1, cx) => {
                    self.ensure_report_history_positions(cx);
                    cx.stop_propagation();
                    cx.notify();
                }
                _ => {}
            }
        }
    }
}

impl Render for StudyWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            return div().id("study-workspace-hidden").test_support();
        }
        self.try_init_backend(window, cx);

        let palette = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let ws_entity = cx.entity();

        let nav_items: Vec<TextNavItem> = StudyTab::ALL
            .iter()
            .map(|&tab| {
                let mut item =
                    TextNavItem::new(tab.id(), tab.label(locale)).element_id(tab.element_id());
                if tab == StudyTab::Flashcards
                    && let Some(sum) = &self.overview.summary
                    && sum.today_count > 0
                {
                    item = item.badge(format_int(locale, sum.today_count));
                }
                item
            })
            .collect();

        let ws_tab = ws_entity.clone();
        let tab_bar = div()
            .id("study-sub-tabs-container")
            .test_support()
            .key_context(CTX_TEXT_NAV)
            .on_action(cx.listener(|this, _: &NavPrev, window, cx| {
                this.step_tab(RovingStep::Prev, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NavNext, window, cx| {
                this.step_tab(RovingStep::Next, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NavFirst, window, cx| {
                this.step_tab(RovingStep::First, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NavLast, window, cx| {
                this.step_tab(RovingStep::Last, window, cx);
            }))
            .border_b_1()
            .border_color(palette.border)
            .child(
                TextNavBar::new("study-sub-tabs", nav_items)
                    .active(Some(self.active_tab.id()))
                    .density(TextNavDensity::Inline)
                    .palette(palette)
                    .on_select(move |id, window, cx| {
                        if let Some(tab) = StudyTab::from_id(id) {
                            ws_tab.update(cx, |w, cx| w.select_tab(tab, window, cx));
                        }
                    }),
            );

        let content: gpui_kit::AnyElement = match self.active_tab {
            StudyTab::Overview => {
                let cb = OverviewCallbacks {
                    on_study_deck: Rc::new({
                        let ws = ws_entity.clone();
                        move |deck_id, _, cx| ws.update(cx, |w, cx| w.start_fsrs_study(deck_id, cx))
                    }),
                    on_open_flashcards: Rc::new({
                        let ws = ws_entity.clone();
                        move |window, cx| {
                            ws.update(cx, |w, cx| w.select_tab(StudyTab::Flashcards, window, cx))
                        }
                    }),
                    on_open_mistakes: Rc::new({
                        let ws = ws_entity.clone();
                        move |window, cx| {
                            ws.update(cx, |w, cx| w.select_tab(StudyTab::Mistakes, window, cx))
                        }
                    }),
                    on_practice_mistake: Rc::new({
                        let ws = ws_entity.clone();
                        move |gp, mk, window, cx| {
                            ws.update(cx, |w, cx| w.launch_mistake_practice(gp, mk, window, cx))
                        }
                    }),
                    on_save_new_cards_per_day: Rc::new({
                        let ws = ws_entity.clone();
                        move |val, window, cx| {
                            ws.update(cx, |w, cx| w.save_new_cards_per_day(val, window, cx))
                        }
                    }),
                    on_select_player: Rc::new({
                        let ws = ws_entity.clone();
                        move |p, _, cx| ws.update(cx, |w, cx| w.set_selected_player(p, cx))
                    }),
                    on_toggle_independent: Rc::new({
                        let ws = ws_entity.clone();
                        move |_, cx| ws.update(cx, |w, cx| w.toggle_independent_only(cx))
                    }),
                    on_refresh: Rc::new({
                        let ws = ws_entity.clone();
                        move |window, cx| ws.update(cx, |w, cx| w.load_overview(Some(window), cx))
                    }),
                };
                render_study_overview(
                    &self.overview,
                    &self.selected_player,
                    self.independent_only,
                    palette,
                    cb,
                    cx,
                )
                .into_any_element()
            }
            StudyTab::Flashcards => {
                if self.fsrs.active {
                    let cb = FsrsReviewCallbacks {
                        on_back: Rc::new({
                            let ws = ws_entity.clone();
                            move |_, cx| {
                                ws.update(cx, |w, cx| {
                                    w.fsrs.stop(cx);
                                    w.sync_anki_webview_visibility(cx);
                                    cx.notify();
                                })
                            }
                        }),
                        on_reveal: Rc::new({
                            let ws = ws_entity.clone();
                            move |_, cx| ws.update(cx, |w, cx| w.reveal_fsrs_card(cx))
                        }),
                        on_check_answer: Rc::new({
                            let ws = ws_entity.clone();
                            move |_, cx| ws.update(cx, |w, cx| w.check_fsrs_answer(cx))
                        }),
                        on_undo_checker: Rc::new({
                            let ws = ws_entity.clone();
                            move |_, cx| {
                                ws.update(cx, |w, cx| {
                                    if let Some(c) = w.fsrs.checker.as_mut() {
                                        c.undo_step();
                                    }
                                    w.fsrs.sync_visuals(cx);
                                    cx.notify();
                                })
                            }
                        }),
                        on_select_cube_offer: Rc::new({
                            let ws = ws_entity.clone();
                            move |act, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.fsrs.selected_cube_offer = Some(act);
                                    cx.notify();
                                })
                            }
                        }),
                        on_select_cube_response: Rc::new({
                            let ws = ws_entity.clone();
                            move |act, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.fsrs.selected_cube_response = Some(act);
                                    cx.notify();
                                })
                            }
                        }),
                        on_rate: Rc::new({
                            let ws = ws_entity.clone();
                            move |rating, _, cx| {
                                ws.update(cx, |w, cx| w.submit_fsrs_rating(rating, cx))
                            }
                        }),
                        on_arrow_target: Rc::new({
                            let ws = ws_entity.clone();
                            move |target, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.fsrs.arrow_target = target;
                                    w.fsrs.sync_visuals(cx);
                                    cx.notify();
                                })
                            }
                        }),
                        on_preview_mode: Rc::new({
                            let ws = ws_entity.clone();
                            move |mode, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.fsrs.preview_mode = mode;
                                    w.fsrs.sync_visuals(cx);
                                    cx.notify();
                                })
                            }
                        }),
                        on_reload: Rc::new({
                            let ws = ws_entity.clone();
                            move |_, cx| ws.update(cx, |w, cx| w.load_next_fsrs_card(cx))
                        }),
                    };
                    render_fsrs_review(&self.fsrs, palette, cb, cx).into_any_element()
                } else if self.practice_round.round.is_some() {
                    let cb = PracticeRoundCallbacks {
                        on_close: Rc::new({
                            let ws = ws_entity.clone();
                            move |_, cx| {
                                ws.update(cx, |w, cx| {
                                    w.practice_round.round = None;
                                    w.practice_round
                                        .anki_face_view
                                        .update(cx, |v, cx| v.set_request(None, cx));
                                    w.sync_anki_webview_visibility(cx);
                                    cx.notify();
                                })
                            }
                        }),
                        on_set_confidence: Rc::new({
                            let ws = ws_entity.clone();
                            move |sure, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.practice_round.confidence = Some(sure);
                                    cx.notify();
                                })
                            }
                        }),
                        on_set_diagnosis: Rc::new({
                            let ws = ws_entity.clone();
                            move |diag, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.practice_round.diagnosis = diag;
                                    cx.notify();
                                })
                            }
                        }),
                        on_submit: Rc::new({
                            let ws = ws_entity.clone();
                            move |check, window, cx| {
                                ws.update(cx, |w, cx| {
                                    w.submit_practice_round_item(check, window, cx)
                                })
                            }
                        }),
                        on_toggle_mark: Rc::new({
                            let ws = ws_entity.clone();
                            move |window, cx| {
                                ws.update(cx, |w, cx| w.advance_or_mark_round(false, window, cx))
                            }
                        }),
                        on_advance: Rc::new({
                            let ws = ws_entity.clone();
                            move |window, cx| {
                                ws.update(cx, |w, cx| w.advance_or_mark_round(true, window, cx))
                            }
                        }),
                        on_retry_pass2: Rc::new({
                            let ws = ws_entity.clone();
                            move |window, cx| {
                                ws.update(cx, |w, cx| w.retry_practice_round(window, cx))
                            }
                        }),
                        on_select_cube_offer: Rc::new({
                            let ws = ws_entity.clone();
                            move |act, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.practice_round.selected_cube_offer = Some(act);
                                    cx.notify();
                                })
                            }
                        }),
                        on_select_cube_response: Rc::new({
                            let ws = ws_entity.clone();
                            move |act, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.practice_round.selected_cube_response = Some(act);
                                    cx.notify();
                                })
                            }
                        }),
                        on_undo_checker: Rc::new({
                            let ws = ws_entity.clone();
                            move |_, cx| {
                                ws.update(cx, |w, cx| {
                                    if let Some(c) = w.practice_round.checker.as_mut() {
                                        c.undo_step();
                                    }
                                    w.practice_round.sync_visuals(cx);
                                    cx.notify();
                                })
                            }
                        }),
                    };
                    render_practice_round(&self.practice_round, palette, cb, cx).into_any_element()
                } else {
                    let transfer_el = if self.flashcards.show_tools_drawer {
                        Some(
                            render_transfer_practice_section(
                                &self.practice_round,
                                &self.selected_player,
                                palette,
                                TransferPracticeCallbacks {
                                    on_set_plan: Rc::new({
                                        let ws = ws_entity.clone();
                                        move |plan, _, cx| {
                                            ws.update(cx, |w, cx| {
                                                w.practice_round.transfer_game_plan = plan;
                                                cx.notify();
                                            })
                                        }
                                    }),
                                    on_set_kind: Rc::new({
                                        let ws = ws_entity.clone();
                                        move |kind, _, cx| {
                                            ws.update(cx, |w, cx| {
                                                w.practice_round.transfer_kind = kind;
                                                cx.notify();
                                            })
                                        }
                                    }),
                                    on_toggle_follow_up: Rc::new({
                                        let ws = ws_entity.clone();
                                        move |_, cx| {
                                            ws.update(cx, |w, cx| {
                                                w.practice_round.transfer_follow_up =
                                                    !w.practice_round.transfer_follow_up;
                                                cx.notify();
                                            })
                                        }
                                    }),
                                    on_start: Rc::new({
                                        let ws = ws_entity.clone();
                                        move |window, cx| {
                                            ws.update(cx, |w, cx| {
                                                w.start_transfer_practice(window, cx)
                                            })
                                        }
                                    }),
                                },
                                cx,
                            )
                            .into_any_element(),
                        )
                    } else {
                        None
                    };

                    let cb = FlashcardsCallbacks {
                        on_select_deck: Rc::new({
                            let ws = ws_entity.clone();
                            move |deck_id, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.flashcards.selected_deck = deck_id.clone();
                                    w.flashcards.page = 1;
                                    w.flashcards.preview_card_id = None;
                                    w.flashcards.preview_answer = None;
                                    update_local_ui_state(cx, |s| s.study.deck_id = deck_id);
                                    w.load_decks_and_cards(cx);
                                    cx.notify();
                                })
                            }
                        }),
                        on_toggle_create_deck: Rc::new({
                            let ws = ws_entity.clone();
                            move |_, cx| {
                                ws.update(cx, |w, cx| {
                                    w.flashcards.show_create_deck = !w.flashcards.show_create_deck;
                                    cx.notify();
                                })
                            }
                        }),
                        on_create_deck: Rc::new({
                            let ws = ws_entity.clone();
                            move |name, window, cx| {
                                let Some(facade) = Self::facade(cx) else {
                                    return;
                                };
                                let locale = current_locale(cx);
                                let task = spawn_facade_write(cx, move || async move {
                                    facade.create_deck(name).await
                                });
                                let ws2 = ws.clone();
                                ws2.update(cx, |_, cx| {
                                    cx.spawn_in(window, async move |this, cx| {
                                        let res = task.await;
                                        let _ = this.update_in(cx, |w, window, cx| {
                                            match res {
                                                Ok(Ok(deck)) => {
                                                    w.flashcards.show_create_deck = false;
                                                    w.flashcards.new_deck_input.update(
                                                        cx,
                                                        |i, cx| {
                                                            i.set_value(String::new(), window, cx)
                                                        },
                                                    );
                                                    w.flashcards.selected_deck =
                                                        deck.id.to_string();
                                                    update_local_ui_state(cx, |s| {
                                                        s.study.deck_id = deck.id.to_string()
                                                    });
                                                    w.load_decks_and_cards(cx);
                                                }
                                                Ok(Err(err)) => {
                                                    w.flashcards.transfer_error =
                                                        Some(format_domain_error(locale, &err));
                                                }
                                                Err(_) => {}
                                            }
                                            cx.notify();
                                        });
                                    })
                                    .detach();
                                });
                            }
                        }),
                        on_study_deck: Rc::new({
                            let ws = ws_entity.clone();
                            move |id, _, cx| ws.update(cx, |w, cx| w.start_fsrs_study(id, cx))
                        }),
                        on_start_round: Rc::new({
                            let ws = ws_entity.clone();
                            move |limit, window, cx| {
                                ws.update(cx, |w, cx| w.start_practice_round(limit, window, cx))
                            }
                        }),
                        on_resume_round: Rc::new({
                            let ws = ws_entity.clone();
                            move |rid, window, cx| {
                                ws.update(cx, |w, cx| w.resume_practice_round(rid, window, cx))
                            }
                        }),
                        on_open_manage_decks: Rc::new({
                            let ws = ws_entity.clone();
                            move |window, cx| {
                                ws.update(cx, |w, cx| w.open_deck_management(window, cx))
                            }
                        }),
                        on_open_duplicates: Rc::new({
                            let ws = ws_entity.clone();
                            move |window, cx| {
                                ws.update(cx, |w, cx| w.open_duplicates_modal(window, cx))
                            }
                        }),
                        on_open_anki_sync_overview: Rc::new({
                            let ws = ws_entity.clone();
                            move |window, cx| {
                                ws.update(cx, |w, cx| w.open_anki_sync_overview(window, cx))
                            }
                        }),
                        on_export_apkg: Rc::new({
                            let ws = ws_entity.clone();
                            move |id, _, cx| ws.update(cx, |w, cx| w.prompt_and_export_apkg(id, cx))
                        }),
                        on_import_deck_apkg: Rc::new({
                            let ws = ws_entity.clone();
                            move |id, _, cx| {
                                ws.update(cx, |w, cx| w.prompt_and_import_apkg(Some(id), cx))
                            }
                        }),
                        on_import_external_apkg: Rc::new({
                            let ws = ws_entity.clone();
                            move |_, cx| ws.update(cx, |w, cx| w.prompt_and_import_apkg(None, cx))
                        }),
                        on_send_to_anki: Rc::new({
                            let ws = ws_entity.clone();
                            move |id, _, cx| ws.update(cx, |w, cx| w.send_deck_to_anki(id, cx))
                        }),
                        on_sync_with_anki: Rc::new({
                            let ws = ws_entity.clone();
                            move |id, _, cx| ws.update(cx, |w, cx| w.sync_deck_with_anki(id, cx))
                        }),
                        on_toggle_auto_sync: Rc::new({
                            let ws = ws_entity.clone();
                            move |id, en, _, cx| {
                                ws.update(cx, |w, cx| w.toggle_deck_auto_sync(id, en, cx))
                            }
                        }),
                        on_apply_filters: Rc::new({
                            let ws = ws_entity.clone();
                            move |_, cx| {
                                ws.update(cx, |w, cx| {
                                    w.flashcards.sync_filters_from_inputs(cx);
                                    w.flashcards.page = 1;
                                    w.load_cards_only(cx);
                                    cx.notify();
                                })
                            }
                        }),
                        on_toggle_advanced: Rc::new({
                            let ws = ws_entity.clone();
                            move |_, cx| {
                                ws.update(cx, |w, cx| {
                                    w.flashcards.show_advanced_filters =
                                        !w.flashcards.show_advanced_filters;
                                    cx.notify();
                                })
                            }
                        }),
                        on_set_kind: Rc::new({
                            let ws = ws_entity.clone();
                            move |kind, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.flashcards.filters.kind = kind;
                                    w.flashcards.page = 1;
                                    w.load_cards_only(cx);
                                    cx.notify();
                                })
                            }
                        }),
                        on_set_status: Rc::new({
                            let ws = ws_entity.clone();
                            move |st, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.flashcards.filters.status = st;
                                    w.flashcards.page = 1;
                                    w.load_cards_only(cx);
                                    cx.notify();
                                })
                            }
                        }),
                        on_set_game_plan: Rc::new({
                            let ws = ws_entity.clone();
                            move |gp, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.flashcards.filters.game_plan = gp;
                                    w.flashcards.page = 1;
                                    w.load_cards_only(cx);
                                    cx.notify();
                                })
                            }
                        }),
                        on_remove_chip: Rc::new({
                            let ws = ws_entity.clone();
                            move |chip_id, window, cx| {
                                ws.update(cx, |w, cx| {
                                    match chip_id.as_str() {
                                        "q" => w.flashcards.filters.q.clear(),
                                        "player" => w.flashcards.filters.player.clear(),
                                        "kind" => w.flashcards.filters.kind = None,
                                        "status" => w.flashcards.filters.status = None,
                                        "game_plan" => w.flashcards.filters.game_plan = None,
                                        "tag" => w.flashcards.filters.tag.clear(),
                                        "topics" => w.flashcards.filters.topics.clear(),
                                        "comment" => w.flashcards.filters.comment.clear(),
                                        "position_id" => w.flashcards.filters.position_id.clear(),
                                        _ => {}
                                    }
                                    w.flashcards.push_filters_to_inputs(window, cx);
                                    w.flashcards.page = 1;
                                    w.load_cards_only(cx);
                                    cx.notify();
                                })
                            }
                        }),
                        on_clear_filters: Rc::new({
                            let ws = ws_entity.clone();
                            move |window, cx| {
                                ws.update(cx, |w, cx| {
                                    w.flashcards.filters.clear();
                                    w.flashcards.push_filters_to_inputs(window, cx);
                                    w.flashcards.page = 1;
                                    w.load_cards_only(cx);
                                    cx.notify();
                                })
                            }
                        }),
                        on_save_topic: Rc::new({
                            let ws = ws_entity.clone();
                            move |name, window, cx| {
                                let Some(facade) = Self::facade(cx) else {
                                    return;
                                };
                                let query = ws.update(cx, |w, cx| {
                                    w.flashcards.sync_filters_from_inputs(cx);
                                    let sel = w.flashcards.selected_deck.clone();
                                    let mut q = w.flashcards.filters.to_query(&sel, 1);
                                    q.page = None;
                                    q.limit = None;
                                    q
                                });
                                let locale = current_locale(cx);
                                let task = spawn_facade_write(cx, move || async move {
                                    facade
                                        .save_card_topic(SaveCardTopic {
                                            name,
                                            filter: query,
                                        })
                                        .await
                                });
                                let ws2 = ws.clone();
                                ws2.update(cx, |_, cx| {
                                    cx.spawn_in(window, async move |this, cx| {
                                        let res = task.await;
                                        let _ = this.update_in(cx, |w, window, cx| {
                                            match res {
                                                Ok(Ok(topic)) => {
                                                    w.flashcards.saved_topics.push(topic);
                                                    w.flashcards.topic_name_input.update(
                                                        cx,
                                                        |i, cx| {
                                                            i.set_value(String::new(), window, cx)
                                                        },
                                                    );
                                                }
                                                Ok(Err(err)) => {
                                                    w.flashcards.transfer_error =
                                                        Some(format_domain_error(locale, &err));
                                                }
                                                Err(_) => {}
                                            }
                                            cx.notify();
                                        });
                                    })
                                    .detach();
                                });
                            }
                        }),
                        on_apply_topic: Rc::new({
                            let ws = ws_entity.clone();
                            move |query, window, cx| {
                                ws.update(cx, |w, cx| {
                                    w.flashcards.filters =
                                        flashcards::CardFilterState::from_topic_filter(&query);
                                    w.flashcards.push_filters_to_inputs(window, cx);
                                    w.flashcards.page = 1;
                                    w.load_cards_only(cx);
                                    cx.notify();
                                })
                            }
                        }),
                        on_delete_topic: Rc::new({
                            let ws = ws_entity.clone();
                            move |id, _, cx| {
                                let Some(facade) = Self::facade(cx) else {
                                    return;
                                };
                                let task = spawn_facade_write(cx, move || async move {
                                    facade.delete_card_topic(id).await
                                });
                                let ws2 = ws.clone();
                                cx.spawn(async move |cx| {
                                    if matches!(task.await, Ok(Ok(()))) {
                                        cx.update(|cx| {
                                            ws2.update(cx, |w, cx| {
                                                w.flashcards.saved_topics.retain(|t| t.id != id);
                                                cx.notify();
                                            });
                                        });
                                    }
                                })
                                .detach();
                            }
                        }),
                        on_select_page: Rc::new({
                            let ws = ws_entity.clone();
                            move |pg, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.flashcards.page = pg;
                                    w.load_cards_only(cx);
                                    cx.notify();
                                })
                            }
                        }),
                        on_toggle_card_select: Rc::new({
                            let ws = ws_entity.clone();
                            move |meta, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.flashcards.selection.toggle(meta);
                                    cx.notify();
                                })
                            }
                        }),
                        on_select_all_page: Rc::new({
                            let ws = ws_entity.clone();
                            move |_, cx| {
                                ws.update(cx, |w, cx| {
                                    if let Some(page) = &w.flashcards.card_page {
                                        let batch =
                                            page.items.iter().map(|i| common::SelectedCardMeta {
                                                id: i.card.id,
                                                version: i.card.version,
                                                tags_version: i.browse.tags_version,
                                                deck_ids: i
                                                    .browse
                                                    .decks
                                                    .iter()
                                                    .map(|d| d.id)
                                                    .collect(),
                                            });
                                        w.flashcards.selection.select_batch(batch);
                                        cx.notify();
                                    }
                                })
                            }
                        }),
                        on_clear_selection: Rc::new({
                            let ws = ws_entity.clone();
                            move |_, cx| {
                                ws.update(cx, |w, cx| {
                                    w.flashcards.selection.clear();
                                    cx.notify();
                                })
                            }
                        }),
                        on_preview_card: Rc::new({
                            let ws = ws_entity.clone();
                            move |id, _, cx| {
                                ws.update(cx, |w, cx| {
                                    w.flashcards.preview_card_id = Some(id);
                                    w.flashcards.preview_answer = None;
                                    w.flashcards.sync_preview_visuals(
                                        w.visible && w.active_tab == StudyTab::Flashcards,
                                        cx,
                                    );
                                    cx.notify();
                                })
                            }
                        }),
                        on_reveal_preview: Rc::new({
                            let ws = ws_entity.clone();
                            move |id, _, cx| {
                                let Some(facade) = Self::facade(cx) else {
                                    return;
                                };
                                ws.update(cx, |w, cx| {
                                    w.flashcards.preview_answer_loading = true;
                                    cx.notify();
                                });
                                let read = spawn_facade_read(cx, move || async move {
                                    facade.preview_card(id).await
                                });
                                let ws2 = ws.clone();
                                cx.spawn(async move |cx| {
                                    let res = read.await;
                                    cx.update(|cx| {
                                        ws2.update(cx, |w, cx| {
                                            w.flashcards.preview_answer_loading = false;
                                            if let Ok(Ok(ans)) = res
                                                && w.flashcards.preview_card_id == Some(id)
                                            {
                                                w.flashcards.preview_answer = Some(ans);
                                                w.flashcards.sync_preview_visuals(
                                                    w.visible
                                                        && w.active_tab == StudyTab::Flashcards,
                                                    cx,
                                                );
                                            }
                                            cx.notify();
                                        });
                                    });
                                })
                                .detach();
                            }
                        }),
                        on_open_pips_modal: Rc::new({
                            let ws = ws_entity.clone();
                            move |card, window, cx| {
                                ws.update(cx, |w, cx| w.open_pips_modal(card, window, cx))
                            }
                        }),
                        on_open_score_variants: Rc::new({
                            let ws = ws_entity.clone();
                            move |id, window, cx| {
                                ws.update(cx, |w, cx| w.open_score_variants(id, window, cx))
                            }
                        }),
                        on_open_timed_preview: Rc::new({
                            let ws = ws_entity.clone();
                            move |window, cx| {
                                ws.update(cx, |w, cx| w.open_timed_preview(window, cx))
                            }
                        }),
                        on_open_batch_action: Rc::new({
                            let ws = ws_entity.clone();
                            move |action, window, cx| {
                                ws.update(cx, |w, cx| w.open_batch_card_dialog(action, window, cx))
                            }
                        }),
                        on_apply_tags: Rc::new({
                            let ws = ws_entity.clone();
                            move |add_flag, tag, window, cx| {
                                let Some(facade) = Self::facade(cx) else {
                                    return;
                                };
                                let cards: Vec<TagCard> = ws
                                    .read(cx)
                                    .flashcards
                                    .selection
                                    .metas()
                                    .into_iter()
                                    .map(|m| TagCard {
                                        id: m.id,
                                        tags_version: m.tags_version,
                                    })
                                    .collect();
                                if cards.is_empty() {
                                    return;
                                }
                                let req = EditCardTags {
                                    cards,
                                    add: if add_flag {
                                        vec![tag.clone()]
                                    } else {
                                        Vec::new()
                                    },
                                    remove: if !add_flag { vec![tag] } else { Vec::new() },
                                };
                                let locale = current_locale(cx);
                                let task = spawn_facade_write(cx, move || async move {
                                    facade.update_card_tags(req).await
                                });
                                let ws2 = ws.clone();
                                ws2.update(cx, |_, cx| {
                                    cx.spawn_in(window, async move |this, cx| {
                                        let res = task.await;
                                        let _ = this.update_in(cx, |w, window, cx| {
                                            match res {
                                                Ok(Ok(_)) => {
                                                    w.flashcards.edit_tag_input.update(
                                                        cx,
                                                        |i, cx| {
                                                            i.set_value(String::new(), window, cx)
                                                        },
                                                    );
                                                    w.load_cards_only(cx);
                                                }
                                                Ok(Err(err)) => {
                                                    w.flashcards.transfer_error =
                                                        Some(format_domain_error(locale, &err));
                                                }
                                                Err(_) => {}
                                            }
                                            cx.notify();
                                        });
                                    })
                                    .detach();
                                });
                            }
                        }),
                        on_adjust_preview_width: Rc::new({
                            let ws = ws_entity.clone();
                            move |delta, _, cx| {
                                ws.update(cx, |w, cx| w.adjust_preview_width(delta, cx))
                            }
                        }),
                        on_toggle_tools: Rc::new({
                            let ws = ws_entity.clone();
                            move |_, cx| {
                                ws.update(cx, |w, cx| {
                                    w.flashcards.show_tools_drawer =
                                        !w.flashcards.show_tools_drawer;
                                    cx.notify();
                                })
                            }
                        }),
                    };
                    render_flashcards_browser(
                        &self.flashcards,
                        self.resumable_round_id,
                        transfer_el,
                        palette,
                        cb,
                        cx,
                    )
                    .into_any_element()
                }
            }
            StudyTab::Reports => {
                let cb = ReportsCallbacks {
                    on_refresh: Rc::new({
                        let ws = ws_entity.clone();
                        move |_, cx| ws.update(cx, |w, cx| w.load_reports(false, cx))
                    }),
                    on_load_more: Rc::new({
                        let ws = ws_entity.clone();
                        move |_, cx| ws.update(cx, |w, cx| w.load_reports(true, cx))
                    }),
                    on_open_report: Rc::new({
                        let ws = ws_entity.clone();
                        move |id, _, cx| ws.update(cx, |w, cx| w.open_report_by_id(id, cx))
                    }),
                    on_close_report: Rc::new({
                        let ws = ws_entity.clone();
                        move |_, cx| {
                            ws.update(cx, |w, cx| {
                                w.reports.close_open_report();
                                cx.notify();
                            })
                        }
                    }),
                    on_select_detail_tab: Rc::new({
                        let ws = ws_entity.clone();
                        move |tab, _, cx| {
                            ws.update(cx, |w, cx| {
                                w.reports.detail_tab = tab;
                                if tab == ReportDetailTab::Decisions {
                                    w.ensure_report_history_positions(cx);
                                }
                                cx.notify();
                            })
                        }
                    }),
                    on_start_assessment: Rc::new({
                        let ws = ws_entity.clone();
                        move |_, cx| ws.update(cx, |w, cx| w.start_assessment_for_open_report(cx))
                    }),
                    on_retry_assessment: Rc::new({
                        let ws = ws_entity.clone();
                        move |_, cx| ws.update(cx, |w, cx| w.load_assessment_for_open_report(cx))
                    }),
                    on_select_ref: Rc::new({
                        let ws = ws_entity.clone();
                        move |ref_str, _, cx| {
                            ws.update(cx, |w, cx| {
                                w.reports.select_evidence_ref(&ref_str, cx);
                                w.ensure_report_history_positions(cx);
                                cx.notify();
                            })
                        }
                    }),
                    on_clear_ref: Rc::new({
                        let ws = ws_entity.clone();
                        move |_, cx| {
                            ws.update(cx, |w, cx| {
                                w.reports.selected_ref_key = None;
                                cx.notify();
                            })
                        }
                    }),
                    on_preview_mode: Rc::new({
                        let ws = ws_entity.clone();
                        move |mode, _, cx| {
                            ws.update(cx, |w, cx| {
                                w.reports.preview_mode = mode;
                                w.reports.sync_preview_board(cx);
                                cx.notify();
                            })
                        }
                    }),
                    on_toggle_attempts: Rc::new({
                        let ws = ws_entity.clone();
                        move |key, _, cx| {
                            ws.update(cx, |w, cx| {
                                if w.reports.expanded_attempt_key.as_deref() == Some(key.as_str()) {
                                    w.reports.expanded_attempt_key = None;
                                } else {
                                    w.reports.expanded_attempt_key = Some(key);
                                }
                                cx.notify();
                            })
                        }
                    }),
                    on_open_in_analysis: Rc::new({
                        let ws = ws_entity.clone();
                        move |match_id, game_no, decision_no, _, cx| {
                            ws.update(cx, |_, cx| {
                                cx.emit(WorkspaceEvent::OpenMatchInAnalysis {
                                    match_id,
                                    game_no,
                                    position_id: None,
                                    decision_no,
                                });
                            })
                        }
                    }),
                };
                render_study_reports(&self.reports, palette, cb, cx).into_any_element()
            }
            StudyTab::Mistakes => {
                let cb = MistakesCallbacks {
                    on_select_player: Rc::new({
                        let ws = ws_entity.clone();
                        move |p, _, cx| ws.update(cx, |w, cx| w.set_selected_player(p, cx))
                    }),
                    on_select_database: Rc::new({
                        let ws = ws_entity.clone();
                        move |db, _, cx| {
                            ws.update(cx, |w, cx| {
                                w.mistakes.database_id = db;
                                w.load_mistakes(cx);
                                cx.notify();
                            })
                        }
                    }),
                    on_select_game_plan: Rc::new({
                        let ws = ws_entity.clone();
                        move |gp, _, cx| {
                            ws.update(cx, |w, cx| {
                                w.mistakes.game_plan = gp;
                                w.load_mistakes(cx);
                                cx.notify();
                            })
                        }
                    }),
                    on_toggle_independent: Rc::new({
                        let ws = ws_entity.clone();
                        move |_, cx| ws.update(cx, |w, cx| w.toggle_independent_only(cx))
                    }),
                    on_refresh: Rc::new({
                        let ws = ws_entity.clone();
                        move |_, cx| ws.update(cx, |w, cx| w.load_mistakes(cx))
                    }),
                    on_practice_group: Rc::new({
                        let ws = ws_entity.clone();
                        move |gp, mk, window, cx| {
                            ws.update(cx, |w, cx| w.launch_mistake_practice(gp, mk, window, cx))
                        }
                    }),
                    on_toggle_unanalyzed: Rc::new({
                        let ws = ws_entity.clone();
                        move |_, cx| {
                            ws.update(cx, |w, cx| {
                                w.mistakes.unanalyzed_open = !w.mistakes.unanalyzed_open;
                                if w.mistakes.unanalyzed_open {
                                    w.load_unanalyzed_matches(cx);
                                }
                                cx.notify();
                            })
                        }
                    }),
                    on_open_match: Rc::new({
                        let ws = ws_entity.clone();
                        move |match_id, _, cx| {
                            ws.update(cx, |_, cx| {
                                cx.emit(WorkspaceEvent::OpenMatchInAnalysis {
                                    match_id,
                                    game_no: None,
                                    position_id: None,
                                    decision_no: None,
                                });
                            })
                        }
                    }),
                    on_analyze_matches: Rc::new({
                        let ws = ws_entity.clone();
                        move |match_ids, _, cx| {
                            ws.update(cx, |_, cx| {
                                cx.emit(WorkspaceEvent::StartBulkAnalysis { match_ids });
                            })
                        }
                    }),
                };
                render_recurring_mistakes(
                    &self.mistakes,
                    &self.selected_player,
                    self.independent_only,
                    palette,
                    cb,
                    cx,
                )
                .into_any_element()
            }
            StudyTab::History => {
                let cb = HistoryCallbacks {
                    on_refresh: Rc::new({
                        let ws = ws_entity.clone();
                        move |_, cx| ws.update(cx, |w, cx| w.load_history(cx))
                    }),
                    on_load_more_similar: Rc::new({
                        let ws = ws_entity.clone();
                        move |_, cx| ws.update(cx, |w, cx| w.load_more_similar_history(cx))
                    }),
                    on_select: Rc::new({
                        let ws = ws_entity.clone();
                        move |sel: Option<SelectedHistoryEntry>, _, cx| {
                            ws.update(cx, |w, cx| {
                                w.history.selected = sel;
                                cx.notify();
                            })
                        }
                    }),
                };
                render_practice_history(&self.history, palette, cb, cx).into_any_element()
            }
        };

        div()
            .id("study-workspace-root")
            .test_support()
            .key_context(CTX_STUDY_EXERCISE)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            // Study exercise keyboard shortcuts (`A`, `Enter`, `Z`, `N`, `D`, `T`, `P`, `1`–`4`)
            .on_action(cx.listener(|this, _: &ShowAnswer, window, cx| {
                if this.fsrs.active {
                    this.reveal_fsrs_card(cx);
                } else if this.practice_round.round.is_some() {
                    this.submit_practice_round_item(false, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &CheckOrContinue, window, cx| {
                if this.fsrs.active {
                    if this.fsrs.effective_revealed().is_none() {
                        if this.fsrs.build_pending_answer().is_some() {
                            this.check_fsrs_answer(cx);
                        } else {
                            this.reveal_fsrs_card(cx);
                        }
                    }
                } else if let Some(round) = &this.practice_round.round {
                    if round.result.is_some() {
                        this.advance_or_mark_round(true, window, cx);
                    } else if this.practice_round.build_pending_answer().is_some() {
                        this.submit_practice_round_item(true, window, cx);
                    } else {
                        this.submit_practice_round_item(false, window, cx);
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &UndoCheckerStep, _, cx| {
                if this.fsrs.active {
                    if let Some(c) = this.fsrs.checker.as_mut() {
                        c.undo_step();
                    }
                    this.fsrs.sync_visuals(cx);
                    cx.notify();
                } else if this.practice_round.round.is_some() {
                    if let Some(c) = this.practice_round.checker.as_mut() {
                        c.undo_step();
                    }
                    this.practice_round.sync_visuals(cx);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &CubeNoDouble, _, cx| {
                if this.fsrs.active {
                    this.fsrs.selected_cube_offer = Some(CubeOfferAction::NoDouble);
                    cx.notify();
                } else if this.practice_round.round.is_some() {
                    this.practice_round.selected_cube_offer = Some(CubeOfferAction::NoDouble);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &CubeDouble, _, cx| {
                if this.fsrs.active {
                    this.fsrs.selected_cube_offer = Some(CubeOfferAction::Double);
                    cx.notify();
                } else if this.practice_round.round.is_some() {
                    this.practice_round.selected_cube_offer = Some(CubeOfferAction::Double);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &CubeTake, _, cx| {
                if this.fsrs.active {
                    this.fsrs.selected_cube_response = Some(CubeResponseAction::Take);
                    cx.notify();
                } else if this.practice_round.round.is_some() {
                    this.practice_round.selected_cube_response = Some(CubeResponseAction::Take);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &CubeDrop, _, cx| {
                if this.fsrs.active {
                    this.fsrs.selected_cube_response = Some(CubeResponseAction::Drop);
                    cx.notify();
                } else if this.practice_round.round.is_some() {
                    this.practice_round.selected_cube_response = Some(CubeResponseAction::Drop);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &RateAgain, _, cx| {
                this.submit_fsrs_rating(Rating::Again, cx);
            }))
            .on_action(cx.listener(|this, _: &RateHard, _, cx| {
                this.submit_fsrs_rating(Rating::Hard, cx);
            }))
            .on_action(cx.listener(|this, _: &RateGood, _, cx| {
                this.submit_fsrs_rating(Rating::Good, cx);
            }))
            .on_action(cx.listener(|this, _: &RateEasy, _, cx| {
                this.submit_fsrs_rating(Rating::Easy, cx);
            }))
            // Splitter keyboard actions for resizable card preview pane
            .on_action(cx.listener(|this, _: &SplitterShrink, _, cx| {
                this.adjust_preview_width(-24, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitterGrow, _, cx| {
                this.adjust_preview_width(24, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitterShrinkFast, _, cx| {
                this.adjust_preview_width(-80, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitterGrowFast, _, cx| {
                this.adjust_preview_width(80, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitterMin, _, cx| {
                this.set_preview_width(MIN_CARD_PREVIEW_WIDTH, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitterMax, _, cx| {
                this.set_preview_width(MAX_CARD_PREVIEW_WIDTH, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitterReset, _, cx| {
                this.set_preview_width(DEFAULT_CARD_PREVIEW_WIDTH, cx);
            }))
            .v_flex()
            .flex_1()
            .size_full()
            .p(px(20.0))
            .gap(px(14.0))
            .bg(palette.bg)
            .child(div().h_flex().items_center().justify_between().child(
                div().text_page().text_color(palette.text).child(I18n::t(
                    locale,
                    "workspace",
                    "learning",
                )),
            ))
            .child(tab_bar)
            .child(
                div()
                    .id("study-workspace-scroll-body")
                    .v_flex()
                    .flex_1()
                    .overflow_y_scroll()
                    .child(content),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure_app;
    use chrono::Utc;
    use gpui_kit::{WindowOptions, test::TestWindowExt};
    use neondojo::learning::{
        CardAnswer, CardQuestion, Deck, NextCard, Rating, RatingIntervals, RevealedCard,
        ReviewCounts, StudyDeck,
    };
    use neondojo::match_analysis::{AnalyzedGame, Decision};
    use neondojo::match_assessment::{
        Assessment, AssessmentJob, AssessmentState, AssessmentStatus, Availability, KeyDecision,
        Topic, TopicName, Training,
    };
    use neondojo::model::Position;
    use neondojo::play::{PlayEngine, PlayMode, PlayPhase, PlayReview, PlaySession};
    use neondojo::recurring_mistakes::{MistakeGroup, MistakeKind, MistakeRates};
    use neondojo::study::StudyReportSummary;
    use neondojo_rules::backgammon::initial_board;
    use std::sync::{Arc, Mutex};

    use self::common::{
        CARD_SELECTION_LIMIT, CardSelectionSet, SelectedCardMeta, clamp_new_cards_per_day,
        parse_and_clamp_new_cards_per_day,
    };
    use self::flashcards::group_decks;
    use self::fsrs_review::FsrsIdempotencyTracker;
    use self::overview::{
        OverviewCardsInput, OverviewMistakesInput, RecommendedStudyAction, default_study_player,
        rank_mistake_groups, recommend_study_action,
    };
    use self::reports::{
        StudyReportGroup, format_evidence_ref_chip, group_study_reports, parse_evidence_ref,
    };

    #[test]
    fn clamps_new_cards_per_day_to_zero_through_1000() {
        assert_eq!(clamp_new_cards_per_day(-50), 0);
        assert_eq!(clamp_new_cards_per_day(0), 0);
        assert_eq!(clamp_new_cards_per_day(25), 25);
        assert_eq!(clamp_new_cards_per_day(1000), 1000);
        assert_eq!(clamp_new_cards_per_day(5000), 1000);

        assert_eq!(parse_and_clamp_new_cards_per_day(" -10 "), Some(0));
        assert_eq!(parse_and_clamp_new_cards_per_day("42"), Some(42));
        assert_eq!(parse_and_clamp_new_cards_per_day("9999"), Some(1000));
        assert_eq!(parse_and_clamp_new_cards_per_day(""), None);
        assert_eq!(parse_and_clamp_new_cards_per_day("abc"), None);
    }

    #[test]
    fn overview_recommender_prioritizes_due_deck_then_significant_mistake_group() {
        let deck = StudyDeck {
            id: Uuid::new_v4(),
            name: "Endgames".into(),
        };
        assert_eq!(
            recommend_study_action(
                OverviewCardsInput::Ready { deck: Some(&deck) },
                OverviewMistakesInput::ChoosePlayer
            ),
            RecommendedStudyAction::Deck {
                deck_id: deck.id,
                deck_name: "Endgames".into(),
            }
        );

        let small = MistakeGroup {
            game_plan: GamePlan::Opening,
            mistake_kind: MistakeKind::Checker,
            count: 5,
            match_count: 3,
            total_loss: 1.5,
            rates: Some(MistakeRates {
                decision_count: 12,
                error_rate: 0.4,
                loss_per_100: 12.5,
                small_sample: true,
                trend: None,
            }),
        };
        let major = MistakeGroup {
            game_plan: GamePlan::Priming,
            mistake_kind: MistakeKind::MissedDouble,
            count: 14,
            match_count: 5,
            total_loss: 2.8,
            rates: Some(MistakeRates {
                decision_count: 45,
                error_rate: 0.31,
                loss_per_100: 6.2,
                small_sample: false,
                trend: None,
            }),
        };
        let groups = [small, major];
        let ranked = rank_mistake_groups(&groups);
        assert_eq!(ranked[0].game_plan, GamePlan::Priming);
        assert!(matches!(
            recommend_study_action(
                OverviewCardsInput::Ready { deck: None },
                OverviewMistakesInput::Ready {
                    group: ranked.first().copied()
                }
            ),
            RecommendedStudyAction::Mistake {
                game_plan: GamePlan::Priming,
                mistake_kind: MistakeKind::MissedDouble,
                ..
            }
        ));

        assert_eq!(
            default_study_player("alice", &["Bob".into(), "Alice".into()]),
            "Alice"
        );
        assert_eq!(
            default_study_player("alice", &["Alice".into(), "alice".into()]),
            ""
        );
    }

    #[test]
    fn enforces_200_card_cross_page_selection_cap() {
        let mut sel = CardSelectionSet::default();
        let make_meta = |n: u128| SelectedCardMeta {
            id: Uuid::from_u128(n),
            version: 1,
            tags_version: 1,
            deck_ids: Vec::new(),
        };

        let batch: Vec<_> = (1..=250).map(make_meta).collect();
        let added = sel.select_batch(batch);
        assert_eq!(added, CARD_SELECTION_LIMIT);
        assert_eq!(sel.len(), 200);
        assert!(sel.cap_hit());

        // Toggling a 201st card is rejected cleanly
        assert!(!sel.toggle(make_meta(999)));
        assert_eq!(sel.len(), 200);
        assert!(sel.cap_hit());

        // Deselecting one allows adding another
        assert!(!sel.toggle(make_meta(1)));
        assert_eq!(sel.len(), 199);
        assert!(!sel.cap_hit());
        assert!(sel.toggle(make_meta(999)));
        assert_eq!(sel.len(), 200);
    }

    #[test]
    fn fsrs_idempotency_reuses_request_id_on_retry_and_rotates_on_change() {
        let mut tracker = FsrsIdempotencyTracker::default();
        let session_a = Uuid::new_v4();
        let session_b = Uuid::new_v4();

        let first = tracker.grade_request_id(session_a, Rating::Good);
        let retry_same = tracker.grade_request_id(session_a, Rating::Good);
        assert_eq!(
            first, retry_same,
            "retry for same session+rating reuses UUID"
        );

        let diff_rating = tracker.grade_request_id(session_a, Rating::Hard);
        assert_ne!(first, diff_rating, "changing rating mints new UUID");

        let diff_session = tracker.grade_request_id(session_b, Rating::Hard);
        assert_ne!(diff_rating, diff_session, "changing session mints new UUID");

        let card_id = Uuid::new_v4();
        let att1 = tracker.attempt_request_id(card_id, 2);
        let att2 = tracker.attempt_request_id(card_id, 2);
        let att3 = tracker.attempt_request_id(card_id, 3);
        assert_eq!(att1, att2);
        assert_ne!(att1, att3);

        tracker.reset();
        let after_reset = tracker.grade_request_id(session_b, Rating::Hard);
        assert_ne!(diff_session, after_reset);
    }

    #[test]
    fn groups_decks_hierarchically_and_filters_preserving_active_deck() {
        let now = Utc::now();
        let mk = |name: &str| Deck {
            id: Uuid::new_v4(),
            name: name.into(),
            version: 1,
            card_count: 5,
            due_count: 2,
            created_at: now,
        };
        let decks = vec![
            mk("Cube::Blitz"),
            mk("General"),
            mk("Cube::Holding"),
            mk("Endgame::Bearoff::2-checker"),
        ];
        let grouped = group_decks(&decks, "", None);
        assert_eq!(grouped[0].path, "");
        assert_eq!(grouped[0].decks[0].1, "General");
        assert_eq!(grouped[1].path, "Cube");
        assert_eq!(grouped[1].decks.len(), 2);
        assert_eq!(grouped[2].path, "Endgame › Bearoff");

        let filtered = group_decks(&decks, "bearoff", Some(decks[1].id));
        assert_eq!(
            filtered.len(),
            2,
            "kept deck + matching deck remain visible"
        );
    }

    #[test]
    fn parses_assessment_evidence_refs_and_groups_money_sessions() {
        assert_eq!(parse_evidence_ref("2:14"), Some((2, 14)));
        assert_eq!(parse_evidence_ref("3.7"), Some((3, 7)));
        assert_eq!(parse_evidence_ref("0:5"), None);
        assert_eq!(parse_evidence_ref("bad"), None);
        assert_eq!(format_evidence_ref_chip("2:14"), "2.14");

        let now = Utc::now();
        let money_session = Uuid::new_v4();
        let mk_rep =
            |session_id: Uuid, match_length: i32, game_no: Option<i32>| StudyReportSummary {
                id: Uuid::new_v4(),
                session_id,
                session_scores: vec![3, 1],
                money_session_ended: false,
                library_match_id: None,
                game_no,
                name: "Alice · GNUbg".into(),
                engine_id: "gnubg".into(),
                bot_name: "GNUbg".into(),
                created_at: now,
                updated_at: now,
                human_player: 0,
                human_name: "Alice".into(),
                match_length,
                scores: vec![1, 0],
                play_mode: "coach".into(),
                first_attempt_rating: Some(5.2),
                assisted: false,
                analysis_plies: 3,
            };
        let list = vec![
            mk_rep(money_session, 0, Some(2)),
            mk_rep(Uuid::new_v4(), 7, None),
            mk_rep(money_session, 0, Some(1)),
        ];
        let groups = group_study_reports(&list);
        assert_eq!(groups.len(), 2);
        match &groups[0] {
            StudyReportGroup::MoneySession { reports, .. } => {
                assert_eq!(reports.len(), 2);
                assert_eq!(reports[0].game_no, Some(1));
                assert_eq!(reports[1].game_no, Some(2));
            }
            _ => panic!("expected grouped money session first"),
        }
    }

    #[gpui_kit::test]
    async fn headless_study_renders_all_5_tabs_anki_scoping_and_assessment_ref_chips(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(LocalUiState::default(), false, cx);
        });

        let emitted_events: Arc<Mutex<Vec<WorkspaceEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let emitted_clone = Arc::clone(&emitted_events);

        let (window, study) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    let entity = cx.new(|cx| StudyWorkspace::new(window, cx));
                    cx.subscribe(&entity, move |_, event: &WorkspaceEvent, _| {
                        emitted_clone.lock().unwrap().push(event.clone());
                    })
                    .detach();
                    entity
                })
            })
            .expect("open headless Study window");

        let match_id = Uuid::new_v4();
        let report_id = Uuid::new_v4();
        let anki_card_id = Uuid::new_v4();

        cx.update_window(window, |_, window, cx| {
            // 1. Overview tab renders cleanly by default
            window.render_frame(cx);
            assert_eq!(study.read(cx).active_tab(), StudyTab::Overview);
            assert!(window.find("study-overview-panel").visible());
            assert!(window.find("study-overview-next-action").visible());
            assert!(window.find("study-overview-due-card").visible());
            assert!(window.find("study-overview-mistakes-card").visible());

            // 2. Switch to Flashcards with >12 decks to verify deck filter input appears
            window.click("study-tab-flashcards", cx);
            study.update(cx, |ws, cx| {
                let now = Utc::now();
                ws.flashcards.decks = (1..=14)
                    .map(|i| Deck {
                        id: Uuid::new_v4(),
                        name: format!("Group::Deck {i}"),
                        version: 1,
                        card_count: 10,
                        due_count: 2,
                        created_at: now,
                    })
                    .collect();
                cx.notify();
            });
            window.render_frame(cx);
            assert_eq!(study.read(cx).active_tab(), StudyTab::Flashcards);
            assert!(window.find("study-flashcards-panel").visible());
            assert!(
                window.find("deck-filter-wrapper").visible(),
                "deck filter input must appear when decks > 12"
            );

            // 3. FSRS Review: check answer vs rating separation + scoped AnkiCardFaceView
            study.update(cx, |ws, cx| {
                ws.fsrs.active = true;
                ws.fsrs.deck_id = Some(ws.flashcards.decks[0].id);
                ws.fsrs.deck_name = "Anki Deck".into();
                ws.fsrs.apply_next_card(
                    NextCard {
                        card: Some(CardQuestion {
                            id: anki_card_id,
                            version: 1,
                            kind: "anki".into(),
                            position: None,
                            players: [String::new(), String::new()],
                            prompt: "Third-party Anki card".into(),
                            source: None,
                            due_at: Utc::now(),
                            review_count: 0,
                            tags_version: 1,
                        }),
                        counts: ReviewCounts {
                            total: 1,
                            due: 1,
                            new: 0,
                            new_available: 0,
                        },
                        active_attempt: None,
                    },
                    None,
                    cx,
                );
                assert!(
                    !ws.fsrs.can_submit_rating(),
                    "rating must stay blocked before reveal/check"
                );
                cx.notify();
            });
            window.render_frame(cx);
            assert!(window.find("study-fsrs-review").visible());
            assert!(
                window.find("anki-card-face-view").visible(),
                "AnkiCardFaceView must mount when card.kind == 'anki'"
            );

            // Reveal card -> rating bar appears and can_submit_rating() flips true
            study.update(cx, |ws, cx| {
                ws.fsrs.apply_revealed(
                    RevealedCard {
                        card_id: anki_card_id,
                        version: 1,
                        answer: CardAnswer {
                            solution: None,
                            notes: String::new(),
                            tags: Vec::new(),
                            played: String::new(),
                        },
                        intervals: RatingIntervals {
                            again: 600,
                            hard: 3600,
                            good: 86_400,
                            easy: 345_600,
                        },
                        session_id: Uuid::new_v4(),
                    },
                    cx,
                );
                assert!(ws.fsrs.can_submit_rating());
                cx.notify();
            });
            window.render_frame(cx);
            assert!(window.find("fsrs-rating-bar").visible());
            study.update(cx, |ws, cx| ws.fsrs.stop(cx));

            // 4. Reports + Structured JSON AI Assessment with clickable game.decision ref chips
            study.update(cx, |ws, cx| {
                ws.open_report_for_match(match_id, cx);
                ws.reports.open_report_id = Some(report_id);
                ws.reports.open_loading = false;
                ws.reports.detail_tab = ReportDetailTab::Assessment;
                ws.reports.open_session = Some(PlaySession {
                    paused: false,
                    engine_snapshots: None,
                    game_results: None,
                    library_match_id: Some(match_id),
                    learning_used: false,
                    game_learning_used: false,
                    money_session_ended: false,
                    match_forfeited: false,
                    study_report_id: Some(report_id),
                    study_report_version: Some(1),
                    review_game_no: None,
                    review: Some(PlayReview {
                        analysis_plies: 3,
                        games: vec![AnalyzedGame {
                            cube_enabled: true,
                            engine: "gnubg".into(),
                            game_no: 2,
                            decisions: vec![Decision {
                                decision_no: 14,
                                checker_no: Some(1),
                                player: 0,
                                kind: "checker".into(),
                                played: "13/10 24/23".into(),
                                best: "8/5 6/5".into(),
                                played_equity: Some(0.05),
                                best_equity: Some(0.18),
                                equity_loss: 0.13,
                                severity: "blunder".into(),
                                dice: Some([3, 1]),
                                evaluation_status: "evaluated".into(),
                                checker_candidates: Vec::new(),
                                cube_evaluation: None,
                            }],
                            statistics: None,
                        }],
                        assisted: false,
                        first_attempt_loss: 0.13,
                        final_choice_loss: 0.13,
                        first_attempt_count: 1,
                        retry_count: 0,
                        first_attempt_rating: Some(6.5),
                        first_attempt_rating_decisions: 10,
                        first_attempt_rating_method: "gnubg_emg_500_v1".into(),
                        first_attempt_rating_profile: "gnubg; 3-ply".into(),
                        independent_first_attempt_count: 1,
                        assisted_first_attempt_count: 0,
                    }),
                    play_mode: PlayMode::Coach,
                    tutor_threshold: 0.04,
                    error_thresholds: Default::default(),
                    training_auto_analysis: true,
                    analysis_plies: 3,
                    pending_decision: None,
                    decisions: Vec::new(),
                    id: report_id,
                    version: 1,
                    human_player: 0,
                    human_name: "Alice".into(),
                    match_length: 7,
                    bearoff_direction: "right".into(),
                    engine: PlayEngine {
                        id: "gnubg".into(),
                        version: "1.08".into(),
                        plies: 3,
                        top_two: false,
                        difficulty: None,
                    },
                    position: Position {
                        cube_enabled: Some(true),
                        id: Uuid::new_v4(),
                        match_uuid: match_id,
                        ordinal: 0,
                        game_no: 2,
                        ply: 1,
                        player: 0,
                        dice: vec![3, 1],
                        board: initial_board().to_vec(),
                        position_id: "4HPwATDgc/ABMA".into(),
                        gnubg_match_id: "MAAAAAAAAAAA".into(),
                        notation: String::new(),
                    },
                    phase: PlayPhase::MatchOver,
                    can_double: false,
                    legal_moves: Vec::new(),
                    legal_turns: None,
                    history: Vec::new(),
                    scores: [7, 4],
                    game_no: 2,
                    winner: Some(0),
                    opening: None,
                });
                ws.reports.assessment = Some(AssessmentState {
                    availability: Availability::Ready,
                    abbreviated_match: false,
                    job: Some(AssessmentJob {
                        status: AssessmentStatus::Completed,
                        model: "gpt-5".into(),
                        error: None,
                        result: Some(Assessment {
                            summary: "Solid race, missed key opening point.".into(),
                            next_match_focus: "Prioritize 5-point anchor and inner points.".into(),
                            topics: vec![Topic {
                                topic: TopicName::Opening,
                                observation: "Missed making the 5-point on 3-1.".into(),
                                evidence_refs: vec!["2:14".into()],
                            }],
                            key_decisions: vec![KeyDecision {
                                evidence_ref: "2:14".into(),
                                explanation: "8/5 6/5 dominates 13/10 24/23.".into(),
                                next_question: "Which point builds the strongest prime?".into(),
                            }],
                            training: vec![Training {
                                focus: "Opening 3-1".into(),
                                exercise: "Drill opening replies".into(),
                                success_criterion: "0 blunders".into(),
                                evidence_refs: vec!["2:14".into()],
                            }],
                            limitations: vec!["Single match sample".into()],
                        }),
                    }),
                });
                cx.notify();
            });
            window.render_frame(cx);
            assert_eq!(study.read(cx).active_tab(), StudyTab::Reports);
            assert!(window.find("study-match-assessment-panel").visible());
            assert!(window.find("assessment-summary").visible());
            assert!(window.find("assessment-topics").visible());
            assert!(window.find("assessment-key-decisions").visible());
            assert!(window.find("assessment-training").visible());
            assert!(window.find("assessment-limitations").visible());

            // Click `2.14` ref chip -> opens preview + click `Open in Analysis` emits event
            window.click("assessment-topic-ref-0-0", cx);
            window.render_frame(cx);
            assert!(window.find("study-report-decision-preview").visible());

            window.click("preview-open-in-analysis", cx);
            window.render_frame(cx);

            // Switch to Decisions & Attempts tab
            window.click("report-tab-decisions", cx);
            window.render_frame(cx);
            assert!(window.find("study-play-learning-review").visible());

            // 5. Switch to Recurring Mistakes tab
            window.click("study-tab-mistakes", cx);
            window.render_frame(cx);
            assert_eq!(study.read(cx).active_tab(), StudyTab::Mistakes);
            assert!(window.find("study-mistakes-panel").visible());

            // 6. Switch to Practice History tab
            window.click("study-tab-history", cx);
            window.render_frame(cx);
            assert_eq!(study.read(cx).active_tab(), StudyTab::History);
            assert!(window.find("study-history-panel").visible());

            // 7. Visibility toggle hides cleanly (renders 0-size hidden sentinel)
            study.update(cx, |ws, cx| ws.set_visible(false, cx));
            window.render_frame(cx);
            assert!(!study.read(cx).visible);
            assert!(!window.find("study-workspace-hidden").visible());
        })
        .expect("render all 5 Study sub-tabs in headless mode");

        let events = emitted_events.lock().unwrap();
        assert!(
            events.iter().any(|ev| matches!(
                ev,
                WorkspaceEvent::OpenMatchInAnalysis {
                    match_id: mid,
                    game_no: Some(2),
                    decision_no: Some(14),
                    ..
                } if *mid == match_id
            )),
            "clicking Open in Analysis on ref chip must emit OpenMatchInAnalysis(match_id, game=2, decision=14)"
        );
    }

    #[test]
    fn checker_exercise_controller_plays_opening_31_and_undoes() {
        let pos = Position {
            cube_enabled: Some(true),
            id: Uuid::new_v4(),
            match_uuid: Uuid::new_v4(),
            ordinal: 1,
            game_no: 1,
            ply: 1,
            player: 0,
            dice: vec![3, 1],
            board: neondojo::backgammon::initial_board().to_vec(),
            position_id: "4HPwATDgc/ABMA".into(),
            gnubg_match_id: "cAgAAAAAAAAA".into(),
            notation: String::new(),
        };
        let mut ctrl = common::CheckerExerciseController::from_position(&pos, None);
        assert!(!ctrl.is_forced_pass());
        assert!(!ctrl.is_complete());
        // Click 8 (two destinations: 5 or 7) -> selects source, then 5 commits 8/5
        assert!(ctrl.handle_point_click(8));
        assert_eq!(ctrl.selected_source, Some(8));
        assert!(ctrl.handle_point_click(5));
        assert!(!ctrl.is_complete());
        assert_eq!(ctrl.chosen_steps.len(), 1);
        // With only the 1-die remaining, clicking 6 auto-commits its sole destination 6/5
        assert!(ctrl.handle_point_click(6));
        assert!(ctrl.is_complete());
        assert_eq!(ctrl.chosen_steps.len(), 2);
        assert!(ctrl.undo_step());
        assert!(!ctrl.is_complete());
        assert_eq!(ctrl.chosen_steps.len(), 1);
    }

    #[gpui_kit::test]
    async fn fsrs_conflict_reload_clears_active_card_and_switches_anki_scoping(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(LocalUiState::default(), false, cx);

            let atlas = Arc::new(Mutex::new(ThemeAtlasCache::new(None, 0)));
            let board_view = cx.new(|_| BoardView::new("test-fsrs-board", atlas));
            let mut fsrs = FsrsReviewState::new(board_view, cx);

            let anki_id = Uuid::new_v4();
            fsrs.apply_next_card(
                NextCard {
                    card: Some(CardQuestion {
                        id: anki_id,
                        version: 1,
                        kind: "anki".into(),
                        position: None,
                        players: [String::new(), String::new()],
                        prompt: "Anki card".into(),
                        source: None,
                        due_at: Utc::now(),
                        review_count: 0,
                        tags_version: 1,
                    }),
                    counts: ReviewCounts {
                        total: 1,
                        due: 1,
                        new: 1,
                        new_available: 1,
                    },
                    active_attempt: None,
                },
                None,
                cx,
            );
            assert!(fsrs.anki_face_view.read(cx).request().is_some());

            // 409 Conflict reload clears active card, records conflict notice, and clears AnkiCardFaceView
            fsrs.handle_conflict_reload("409 revision conflict".into(), cx);
            assert!(fsrs.current_card().is_none());
            assert_eq!(
                fsrs.conflict_notice.as_deref(),
                Some("409 revision conflict")
            );
            assert!(fsrs.anki_face_view.read(cx).request().is_none());

            // Native checker card must keep AnkiCardFaceView strictly cleared
            let checker_id = Uuid::new_v4();
            fsrs.apply_next_card(
                NextCard {
                    card: Some(CardQuestion {
                        id: checker_id,
                        version: 1,
                        kind: "checker".into(),
                        position: Some(Position {
                            cube_enabled: Some(true),
                            id: Uuid::new_v4(),
                            match_uuid: Uuid::new_v4(),
                            ordinal: 1,
                            game_no: 1,
                            ply: 1,
                            player: 0,
                            dice: vec![3, 1],
                            board: neondojo::backgammon::initial_board().to_vec(),
                            position_id: "4HPwATDgc/ABMA".into(),
                            gnubg_match_id: "cAgAAAAAAAAA".into(),
                            notation: String::new(),
                        }),
                        players: ["Alice".into(), "Bob".into()],
                        prompt: "Checker card".into(),
                        source: None,
                        due_at: Utc::now(),
                        review_count: 2,
                        tags_version: 1,
                    }),
                    counts: ReviewCounts {
                        total: 1,
                        due: 1,
                        new: 0,
                        new_available: 0,
                    },
                    active_attempt: None,
                },
                None,
                cx,
            );
            assert!(fsrs.anki_face_view.read(cx).request().is_none());
            assert!(!fsrs.can_submit_rating());
        });
    }
}
