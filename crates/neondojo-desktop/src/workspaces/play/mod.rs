//! Native GPUI Live Play workspace (`PlayWorkspace`).
//!
//! Full desktop parity with `web/src/workspaces/play/`:
//! - Non-modal empty-state `PlaySetupDialog` preserving the **Play empty-state
//!   focus exception** (never steals focus from shell header tabs).
//! - Single-flight `PlaySessionEntity` (abortable `InitialLoad` via `ReadTask`,
//!   non-abortable `Mutation` via `TokioBridge::spawn_write`).
//! - Synchronous `PlayTurnEntity` (`neondojo_rules::backgammon::checker_paths`)
//!   for hover destination dots, single-click step execution, dice swap,
//!   half-used doubles die masks, `Z` undo with silent reverse animation, and
//!   forced passes.
//! - Isolated `PlayAnimationEntity` & `PlayAudioEngine` observed ONLY by
//!   `PlayBoardStage` (0% idle GPU; header & history never re-render during
//!   checker/dice animations).
//! - Exact `playInterruption` priority stack (`blocked` → `resign` → `hint` →
//!   inline `coaching` → `cube` → `feedback` → `game_over` → `match_over`) and
//!   draggable, clamped `PlayBoardDialog`.
//! - Live mode & Tutor threshold editing (`preserveSteps = true`), inline Coach
//!   toolbar + details modal, Training/Tutor/Hint modals reusing `crate::review::*`
//!   and secondary preview `BoardView`, version-pinned `PlayHistory` review
//!   (`Escape` returns to live board preserving uncommitted steps), Resign dialog
//!   with 422 bot-decline handling, Pause draft warning, Money-game auto-review,
//!   and Study report handoff.

pub mod animation;
pub mod board_dialog;
pub mod board_stage;
pub mod dialogs;
pub mod history;
pub mod interruption;
pub mod session;
pub mod setup_dialog;
pub mod turn;

use crate::app_context::DesktopAppContext;
use crate::events::WorkspaceEvent;
use crate::i18n::{current_locale, format_domain_error};
use crate::keybindings::{
    CTX_BOARD_REVIEW, CTX_PLAY, CTX_PLAY_SETUP, OfferDouble, PrimaryPlayStep, RequestHint,
    ReturnToLiveGame, ShowFinalBoard, ShowInitialBoard, StartPlayMatch, UndoCheckerStep,
};
use crate::local_state::LocalUiState;
use crate::settings::draft_guard::SettingsCategory;
use crate::shell::{AppNotice, AppNoticeText};
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::tokio_bridge::{ReadTask, TokioBridge};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::decimal_input::{DecimalInput, DecimalInputConfig};
use crate::ui::icons::ActionIconName;
use crate::workspaces::play::animation::{PlayAnimationEntity, PlayAnimationEvent};
use crate::workspaces::play::board_dialog::{
    PlayBoardDialogProps, PlayBoardDialogState, render_play_board_dialog,
};
use crate::workspaces::play::board_stage::{PlayBoardStage, PlayStageEvent, PlayStageSnapshot};
use crate::workspaces::play::dialogs::{
    PlayFeedbackCallbacks, PlayResultCallbacks, ResignLevel, has_play_move_preview,
    render_coach_toolbar, render_cube_dialog_body, render_feedback_body,
    render_forfeit_dialog_body, render_learning_review_body, render_pause_warning_body,
    render_resign_dialog_body, render_result_dialog_body,
};
use crate::workspaces::play::history::{
    PlayHistoryCallbacks, PlayHistoryReviewState, render_play_history_panel,
    render_play_history_review_toolbar,
};
use crate::workspaces::play::interruption::{
    PlayActionKind, PlayInterruption, PlayInterruptionInput, PlayTurnStatusContext, action_frames,
    board_array, compute_play_turn_status, is_money_game_finished, is_new_cube_offer,
    play_feedback_state, position_to_board_position, resolve_play_interruption,
};
use crate::workspaces::play::session::{
    AcceptedSession, PlaySessionEntity, PlaySessionEvent, run_facade,
};
use crate::workspaces::play::setup_dialog::{
    PLAY_MODES, PlaySetupCallbacks, PlaySetupChoices, render_play_setup_form,
};
use crate::workspaces::play::turn::PlayTurnEntity;
use gpui_kit::{
    AnyElement, App, AppContext, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyContext, ParentElement, Render, SharedString, Styled,
    Subscription, TestSupportExt, Window,
    base::StyledExt,
    component::input::{InputEvent, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::play::{
    FeedbackOperation, PAUSED_MATCHES_DATABASE_ID, PlayMode, PlayPhase, PlaySession,
};
use neondojo::settings::EngineInfo;
use neondojo_board::{
    BearoffDirection, BoardThemeConfig, BoardView, BoardViewMode, PlayFrame, PlayOrientation,
    PlaySoundKind, ThemeAtlasCache, project_move,
};
use neondojo_i18n::{I18n, Param, format_int, format_ply};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Complete native GPUI Live Play workspace.
pub struct PlayWorkspace {
    visible: bool,
    focus_handle: FocusHandle,

    // Core decoupled entities
    session_entity: Entity<PlaySessionEntity>,
    turn_entity: Entity<PlayTurnEntity>,
    animation_entity: Entity<PlayAnimationEntity>,
    board_stage: Entity<PlayBoardStage>,
    preview_board: Entity<BoardView>,

    // Setup dialog & persistent inputs
    setup_open: bool,
    setup_focus_on_open: bool,
    setup_choices: PlaySetupChoices,
    setup_name_input: Entity<InputState>,
    engines: Vec<EngineInfo>,
    _engines_read: Option<ReadTask<()>>,

    // Live tutor threshold input
    tutor_threshold_input: Entity<DecimalInput>,
    threshold_saving: bool,
    threshold_saved_indicator: bool,

    // Dialog / modal state
    dialog_state: PlayBoardDialogState,
    resign_open: bool,
    resign_level: ResignLevel,
    forfeit_open: bool,
    pause_warning_open: bool,
    hint_open: bool,
    hint_ready: bool,
    coach_details_open: bool,
    learning_review_open: bool,
    more_menu_open: bool,
    acknowledged_match_key: Option<String>,

    // Secondary candidate preview state (shared by Hint / Feedback / Coach Details)
    selected_candidate: Option<usize>,
    preview_mode: BoardViewMode,

    // Version-pinned history review
    review: PlayHistoryReviewState,
    _history_read: Option<ReadTask<()>>,

    // Automatic phase progression & money-game review de-duplication
    auto_money_reviews: HashSet<String>,
    auto_opening_tokens: HashSet<(Uuid, i64)>,
    initialized_restore: bool,

    // Cached local UI prefs for board theme
    theme_config: BoardThemeConfig,
    /// Diagnostic render counter used in tests to prove isolated animation isolation.
    render_count: u64,

    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<WorkspaceEvent> for PlayWorkspace {}

impl Focusable for PlayWorkspace {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl PlayWorkspace {
    /// Creates a new Live Play workspace.
    ///
    /// Preserves the **Play empty-state focus exception**: when no saved
    /// session exists, `PlaySetupDialog` opens non-modally with
    /// `focus_on_open = false` so entering empty Play NEVER steals focus from
    /// shell header tabs.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_internal(false, window, cx)
    }

    #[cfg(test)]
    fn new_for_test(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_internal(true, window, cx)
    }

    fn new_internal(silent_audio: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let local_state = cx.try_global::<LocalUiState>().cloned().unwrap_or_default();
        let locale = current_locale(cx);
        let atlas = cx
            .try_global::<DesktopAppContext>()
            .map(|ctx| Arc::clone(&ctx.board_atlas))
            .unwrap_or_else(|| Arc::new(Mutex::new(ThemeAtlasCache::default())));

        let stored_session_id = local_state.play_session_id;
        let has_backend = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
            .is_some();
        let initial_setup_open = stored_session_id.is_none() || !has_backend;

        let persist_path = if silent_audio {
            Some(std::env::temp_dir().join(format!("neondojo-play-test-{}.json", Uuid::new_v4())))
        } else {
            None
        };

        let session_entity = cx.new(|_| PlaySessionEntity::new(persist_path));
        let turn_entity = cx.new(|_| PlayTurnEntity::new());
        let animation_entity = cx.new(|_| {
            if silent_audio {
                PlayAnimationEntity::silent(local_state.play_sounds)
            } else {
                PlayAnimationEntity::new(local_state.play_sounds)
            }
        });
        let board_stage = cx.new(|cx| {
            PlayBoardStage::new(
                Arc::clone(&atlas),
                animation_entity.clone(),
                turn_entity.clone(),
                window,
                cx,
            )
        });
        let preview_board = cx.new(|cx| {
            let mut bv = BoardView::new("play-candidate-preview-board", atlas);
            bv.set_audio(None);
            bv.set_show_player_bars(false, cx);
            bv
        });

        let setup_choices = PlaySetupChoices::default();
        let setup_name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(I18n::t(locale, "play", "you"))
                .default_value(setup_choices.player_name.clone())
        });

        let this_weak = cx.weak_entity();
        let tutor_threshold_input = cx.new(|cx| {
            let weak = this_weak.clone();
            DecimalInput::new(
                DecimalInputConfig::new("play-live-tutor-threshold", 0.005)
                    .min(0.001)
                    .max(1.0)
                    .separator(locale.decimal_separator())
                    .width(92.0),
                format!("{:.3}", setup_choices.tutor_threshold),
                Some(Rc::new(move |value, window, cx| {
                    let parsed = value.trim().parse::<f64>().ok();
                    let _ = weak.update(cx, |ws, cx| {
                        ws.handle_tutor_threshold_change(parsed, window, cx);
                    });
                })),
                window,
                cx,
            )
        });

        let sub_name = cx.subscribe_in(
            &setup_name_input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.setup_choices.player_name = state.read(cx).value().to_string();
                    this.setup_choices.name_edited = true;
                    cx.notify();
                } else if matches!(event, InputEvent::PressEnter { .. }) {
                    this.start_setup_match(window, cx);
                }
            },
        );

        let sub_session = cx.subscribe_in(
            &session_entity,
            window,
            |this, _ent, event: &PlaySessionEvent, window, cx| {
                this.handle_session_event(event.clone(), window, cx);
            },
        );

        // Subscribe ONLY to PlayAnimationEvent (Started / Settled), NEVER to
        // per-frame ticks — keeping PlayWorkspace & PlayHistory out of the 60–120fps loop!
        let sub_anim = cx.subscribe_in(
            &animation_entity,
            window,
            |this, _ent, event: &PlayAnimationEvent, window, cx| {
                this.handle_animation_event(*event, window, cx);
            },
        );

        let sub_stage = cx.subscribe_in(
            &board_stage,
            window,
            |this, _stage, event: &PlayStageEvent, window, cx| {
                this.handle_stage_event(*event, window, cx);
            },
        );

        let theme_config = BoardThemeConfig {
            board_theme: local_state.board_theme,
            ..BoardThemeConfig::default()
        };

        let mut ws = Self {
            visible: true,
            focus_handle: cx.focus_handle(),
            session_entity,
            turn_entity,
            animation_entity,
            board_stage,
            preview_board,
            setup_open: initial_setup_open,
            // CRITICAL: automatic empty-state setup MUST NOT steal focus!
            setup_focus_on_open: false,
            setup_choices,
            setup_name_input,
            engines: Vec::new(),
            _engines_read: None,
            tutor_threshold_input,
            threshold_saving: false,
            threshold_saved_indicator: false,
            dialog_state: PlayBoardDialogState::new(cx.focus_handle()),
            resign_open: false,
            resign_level: ResignLevel::Single,
            forfeit_open: false,
            pause_warning_open: false,
            hint_open: false,
            hint_ready: false,
            coach_details_open: false,
            learning_review_open: false,
            more_menu_open: false,
            acknowledged_match_key: None,
            selected_candidate: None,
            preview_mode: BoardViewMode::Arrows,
            review: PlayHistoryReviewState::default(),
            _history_read: None,
            auto_money_reviews: HashSet::new(),
            auto_opening_tokens: HashSet::new(),
            initialized_restore: false,
            theme_config,
            render_count: 0,
            _subscriptions: vec![sub_name, sub_session, sub_anim, sub_stage],
        };

        if has_backend {
            ws.load_setup_metadata(window, cx);
            if let Some(id) = stored_session_id {
                ws.initialized_restore = true;
                ws.session_entity.update(cx, |s, cx| {
                    s.load_session(id, false, true, window, cx);
                });
            }
        }
        ws.push_stage_snapshot(cx);
        ws
    }

    /// Required public hook: shows or hides the Play workspace.
    ///
    /// Hiding immediately silences `PlayAudioEngine` and cancels running animations.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        self.animation_entity.update(cx, |anim, cx| {
            anim.set_visible(visible, cx);
        });
        if !visible {
            self.more_menu_open = false;
        }
        self.push_stage_snapshot(cx);
        cx.notify();
    }

    /// Required public hook: opens or resumes a specific play session by ID.
    ///
    /// Replaces any abortable `InitialLoad`, while respecting an in-flight
    /// non-abortable `Mutation`.
    pub fn open_session(
        &mut self,
        session_id: Uuid,
        resume: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.initialized_restore = true;
        self.setup_open = false;
        self.more_menu_open = false;
        self.animation_entity.update(cx, |anim, cx| anim.cancel(cx));
        self.session_entity.update(cx, |s, cx| {
            s.load_session(session_id, resume, false, window, cx);
        });
        self.push_stage_snapshot(cx);
        cx.notify();
    }

    /// Explicitly opens the New Match setup dialog (triggered by user action or
    /// `Cmd+N`), which DOES move focus into the dialog unlike empty-state entry.
    pub fn open_new_match_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_ui_busy(cx) {
            return;
        }
        self.setup_focus_on_open = true;
        self.setup_open = true;
        self.pause_warning_open = false;
        self.forfeit_open = false;
        self.more_menu_open = false;
        self.session_entity.update(cx, |s, cx| s.clear_error(cx));
        self.load_setup_metadata(window, cx);
        self.push_stage_snapshot(cx);
        cx.notify();
    }

    pub fn is_setup_open(&self) -> bool {
        self.visible && self.setup_open
    }

    pub fn session<'a>(&self, cx: &'a App) -> Option<&'a PlaySession> {
        self.session_entity.read(cx).session()
    }

    pub fn active_interruption(&self, cx: &App) -> Option<PlayInterruption> {
        let session = self.session(cx);
        let paused = session.is_some_and(|s| s.paused);
        let animating = self.animation_entity.read(cx).is_animating();
        let pending_roll = self.animation_entity.read(cx).pending_roll();
        let review_active = self.review.is_active(session);
        let coach_decision = self.coach_decision(session, cx);
        let match_key = session.map(|s| format!("{}:{}", s.id, s.game_no));
        let acknowledged = match_key.is_some() && self.acknowledged_match_key == match_key;

        resolve_play_interruption(PlayInterruptionInput {
            session,
            blocked: !self.visible
                || self.setup_open
                || paused
                || review_active
                || animating
                || pending_roll.is_some(),
            resign_open: self.resign_open,
            hint_open: self.hint_open,
            coaching: coach_decision.is_some(),
            acknowledged,
        })
    }

    pub fn session_entity(&self) -> &Entity<PlaySessionEntity> {
        &self.session_entity
    }

    pub fn turn_entity(&self) -> &Entity<PlayTurnEntity> {
        &self.turn_entity
    }

    pub fn animation_entity(&self) -> &Entity<PlayAnimationEntity> {
        &self.animation_entity
    }

    pub fn board_stage(&self) -> &Entity<PlayBoardStage> {
        &self.board_stage
    }

    pub fn render_count(&self) -> u64 {
        self.render_count
    }

    /// Updates the active board/checker/cube/dice artwork configuration live
    /// (for example after Settings applies a built-in or BGBlitz theme).
    pub fn set_theme_config(&mut self, config: BoardThemeConfig, cx: &mut Context<Self>) {
        self.theme_config = config;
        self.push_stage_snapshot(cx);
        self.sync_preview_board(cx);
        cx.notify();
    }

    fn is_ui_busy(&self, cx: &App) -> bool {
        self.session_entity.read(cx).is_busy() || self.animation_entity.read(cx).is_animating()
    }

    fn coach_decision<'a>(
        &self,
        session: Option<&'a PlaySession>,
        _cx: &App,
    ) -> Option<&'a neondojo::play::PlayDecision> {
        let session = session?;
        if !self.visible || self.setup_open || session.paused {
            return None;
        }
        let fb = play_feedback_state(Some(session));
        if fb.mode == PlayMode::Coach && fb.awaiting {
            fb.pending
        } else {
            None
        }
    }

    fn player_name(&self, session: Option<&PlaySession>, player: i32, cx: &App) -> String {
        let locale = current_locale(cx);
        match session {
            Some(s) if player == s.human_player => {
                if s.human_name.trim().is_empty() {
                    I18n::t(locale, "play", "you").to_string()
                } else {
                    s.human_name.clone()
                }
            }
            Some(_) => I18n::t(locale, "play", "bot").to_string(),
            None => {
                I18n::t(locale, "play", if player == 0 { "white" } else { "black" }).to_string()
            }
        }
    }

    fn load_setup_metadata(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        let task = TokioBridge::spawn_read(cx, async move {
            let res = run_facade(facade, |f| async move {
                let engines = f.list_engines().await.unwrap_or_default();
                let cfg = f.get_configuration().await.ok();
                Ok((engines, cfg))
            })
            .await;
            let _ = tx.send(res);
        });
        self._engines_read = Some(task);

        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok((engines, cfg_state))) = rx.await else {
                return;
            };
            let _ = this.update_in(cx, |ws, window, cx| {
                ws.engines = engines;
                if let Some(cfg) = cfg_state {
                    let next_choices =
                        PlaySetupChoices::from_configuration(&cfg.current_configuration);
                    if !ws.setup_choices.name_edited {
                        ws.setup_choices.player_name = next_choices.player_name.clone();
                        ws.setup_name_input.update(cx, |inp, cx| {
                            inp.set_value(next_choices.player_name.clone(), window, cx);
                        });
                    }
                    if !ws.setup_choices.engine_edited && !ws.setup_choices.plies_edited {
                        ws.setup_choices.bot_engine = next_choices.bot_engine;
                        ws.setup_choices.bot_plies = next_choices.bot_plies;
                        ws.setup_choices.bot_top_two = next_choices.bot_top_two;
                    }
                    ws.setup_choices.analysis_engine = next_choices.analysis_engine;
                    ws.setup_choices.analysis_plies = next_choices.analysis_plies;
                    ws.setup_choices.tutor_threshold = next_choices.tutor_threshold;
                    ws.setup_choices.bearoff_direction = next_choices.bearoff_direction;
                    ws.theme_config = BoardThemeConfig {
                        board_theme: cfg.current_configuration.appearance.board_theme.clone(),
                        checker_theme: cfg.current_configuration.appearance.checker_theme.clone(),
                        cube_theme: cfg.current_configuration.appearance.cube_theme.clone(),
                        dice_theme: cfg.current_configuration.appearance.dice_theme.clone(),
                    };
                }
                ws.push_stage_snapshot(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn handle_session_event(
        &mut self,
        event: PlaySessionEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PlaySessionEvent::LoadingStarted => {
                self.animation_entity.update(cx, |anim, cx| anim.cancel(cx));
                self.review.reset();
                self.push_stage_snapshot(cx);
                cx.notify();
            }
            PlaySessionEvent::StoredSessionMissing => {
                self.setup_open = true;
                self.setup_focus_on_open = false;
                self.push_stage_snapshot(cx);
                cx.notify();
            }
            PlaySessionEvent::SwitchedToCompetition { match_id } => {
                cx.emit(WorkspaceEvent::PlaySwitchedToCompetition { match_id });
            }
            PlaySessionEvent::LibraryMutated => {
                cx.emit(WorkspaceEvent::LibraryMutated);
            }
            PlaySessionEvent::Error(_) => {
                self.threshold_saving = false;
                self.animation_entity
                    .update(cx, |anim, cx| anim.set_pending_roll(None, Some(window), cx));
                self.push_stage_snapshot(cx);
                cx.notify();
            }
            PlaySessionEvent::Accepted(accepted) => {
                self.on_session_accepted(*accepted, window, cx);
            }
        }
    }

    fn on_session_accepted(
        &mut self,
        accepted: AcceptedSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session = &accepted.session;
        self.setup_open = false;
        self.resign_open = false;
        self.forfeit_open = false;
        self.pause_warning_open = false;
        self.more_menu_open = false;
        if !accepted.preserve_steps {
            self.coach_details_open = false;
            self.selected_candidate = None;
        }
        if !accepted.same_session || !accepted.same_game {
            self.learning_review_open = false;
            self.acknowledged_match_key = None;
        }
        if self.hint_open
            && session
                .pending_decision
                .as_ref()
                .is_some_and(|d| d.revealed)
        {
            self.hint_ready = true;
        }
        if self.threshold_saving {
            self.threshold_saving = false;
            self.threshold_saved_indicator = true;
        }

        self.review.reset();
        self.tutor_threshold_input.update(cx, |inp, cx| {
            inp.set_canonical_value(&format!("{:.3}", session.tutor_threshold), window, cx);
        });

        let session_clone = session.clone();
        self.turn_entity.update(cx, |t, cx| {
            t.sync_session(Some(&session_clone), accepted.preserve_steps, cx);
        });

        // Clear indeterminate pending-roll spinner and play server action animations / cube sound.
        self.animation_entity
            .update(cx, |anim, cx| anim.set_pending_roll(None, Some(window), cx));
        if !accepted.from_load
            && self.visible
            && let (Some(prev), Some(act)) = (accepted.previous.as_ref(), accepted.action)
        {
            let awaiting_staged = session
                .pending_decision
                .as_ref()
                .and_then(|d| d.action.as_ref())
                .is_some();
            if !awaiting_staged {
                let frames = action_frames(prev, session, act);
                if !frames.is_empty() {
                    self.animation_entity.update(cx, |anim, cx| {
                        anim.enqueue(frames, Some(window), cx);
                    });
                }
            }
            if is_new_cube_offer(prev, session) {
                self.animation_entity.read(cx).play_cue(PlaySoundKind::Cube);
            }
        }

        self.sync_preview_board(cx);
        self.push_stage_snapshot(cx);
        self.maybe_auto_advance(window, cx);
        cx.notify();
    }

    fn handle_animation_event(
        &mut self,
        event: PlayAnimationEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event == PlayAnimationEvent::Settled {
            self.push_stage_snapshot(cx);
            self.maybe_auto_advance(window, cx);
            cx.notify();
        }
    }

    fn handle_stage_event(
        &mut self,
        event: PlayStageEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PlayStageEvent::Roll => self.roll_turn(window, cx),
            PlayStageEvent::Double => self.double_cube(window, cx),
            PlayStageEvent::Confirm(idx) => {
                self.dispatch_action(PlayActionKind::Move, Some(idx), None, None, window, cx);
            }
            PlayStageEvent::Pass => {
                self.dispatch_action(PlayActionKind::Pass, None, None, None, window, cx);
            }
            PlayStageEvent::ContinueDecision => self.continue_pending_decision(window, cx),
            PlayStageEvent::RequestHint => {
                self.request_feedback(FeedbackOperation::Hint, window, cx)
            }
            PlayStageEvent::Undo => self.undo_checker_step(window, cx),
            PlayStageEvent::RetrySession => {
                self.session_entity.update(cx, |s, cx| {
                    s.retry(window, cx);
                });
            }
        }
    }

    /// Automatically progresses `OpeningRoll`, `OpeningResult`, and finished
    /// money-game auto-review checkpoints once idle.
    fn maybe_auto_advance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.visible
            || self.setup_open
            || self.forfeit_open
            || self.session_entity.read(cx).is_busy()
            || self.session_entity.read(cx).error().is_some()
            || self.animation_entity.read(cx).is_animating()
        {
            return;
        }
        let Some(session) = self.session(cx).cloned() else {
            return;
        };
        if session.paused || self.review.is_active(Some(&session)) {
            return;
        }

        // Money-game auto-review when a money game finishes.
        if is_money_game_finished(Some(&session)) && session.review.is_none() {
            let review_key = format!("{}:{}:{}", session.id, session.game_no, session.version);
            if self.auto_money_reviews.insert(review_key) {
                self.session_entity.update(cx, |s, cx| {
                    s.review_session(window, cx);
                });
            }
            return;
        }

        let phase_action = match session.phase {
            PlayPhase::OpeningRoll => Some(PlayActionKind::OpeningRoll),
            PlayPhase::OpeningResult => Some(PlayActionKind::StartOpeningMove),
            _ => None,
        };
        if let Some(act) = phase_action
            && self
                .auto_opening_tokens
                .insert((session.id, session.version))
        {
            self.dispatch_action(act, None, None, None, window, cx);
        }
    }

    fn dispatch_action(
        &mut self,
        action: PlayActionKind,
        move_index: Option<usize>,
        decision_id: Option<Uuid>,
        resign_level: Option<i32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session(cx).cloned() else {
            return;
        };
        if session.paused || self.is_ui_busy(cx) || self.review.is_active(Some(&session)) {
            return;
        }
        let show_pending_roll = action == PlayActionKind::OpeningRoll
            || (action == PlayActionKind::Roll
                && (decision_id.is_some()
                    || matches!(
                        session.play_mode,
                        PlayMode::Competition | PlayMode::Training
                    )
                    || !session.can_double));
        if show_pending_roll {
            self.animation_entity.update(cx, |anim, cx| {
                anim.set_pending_roll(Some(action), Some(window), cx);
            });
        }
        let started = self.session_entity.update(cx, |s, cx| {
            s.perform_action(action, move_index, decision_id, resign_level, window, cx)
        });
        if !started && show_pending_roll {
            self.animation_entity.update(cx, |anim, cx| {
                anim.set_pending_roll(None, Some(window), cx);
            });
        }
        self.push_stage_snapshot(cx);
        cx.notify();
    }

    fn roll_turn(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session(cx).cloned() else {
            return;
        };
        if !matches!(session.phase, PlayPhase::Roll)
            || self.pause_warning_open
            || self.active_interruption(cx).is_some()
        {
            return;
        }
        let routine_id = play_feedback_state(Some(&session))
            .routine_roll
            .map(|d| d.id);
        self.dispatch_action(PlayActionKind::Roll, None, routine_id, None, window, cx);
    }

    fn double_cube(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session(cx).cloned() else {
            return;
        };
        if !matches!(session.phase, PlayPhase::Roll)
            || !session.can_double
            || self.pause_warning_open
            || self.active_interruption(cx).is_some()
        {
            return;
        }
        self.dispatch_action(PlayActionKind::Double, None, None, None, window, cx);
    }

    fn continue_pending_decision(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session(cx).cloned() else {
            return;
        };
        let Some(decision) = session.pending_decision.as_ref() else {
            return;
        };
        let Some(act_str) = decision.action.as_deref() else {
            return;
        };
        let Some(kind) = PlayActionKind::from_action_str(act_str) else {
            return;
        };
        self.dispatch_action(
            kind,
            decision.move_index,
            Some(decision.id),
            None,
            window,
            cx,
        );
    }

    fn request_feedback(
        &mut self,
        operation: FeedbackOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session(cx).cloned() else {
            return;
        };
        if session.paused || self.is_ui_busy(cx) || self.review.is_active(Some(&session)) {
            return;
        }
        if matches!(operation, FeedbackOperation::Hint) {
            if session.play_mode == PlayMode::Competition {
                return;
            }
            self.hint_open = true;
            self.hint_ready = false;
            self.selected_candidate = None;
        }
        self.session_entity.update(cx, |s, cx| {
            s.update_feedback(operation, window, cx);
        });
        self.push_stage_snapshot(cx);
        cx.notify();
    }

    fn switch_mode(&mut self, mode: PlayMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_ui_busy(cx) {
            return;
        }
        self.session_entity.update(cx, |s, cx| {
            s.change_mode(mode, None, window, cx);
        });
        cx.notify();
    }

    fn handle_tutor_threshold_change(
        &mut self,
        parsed: Option<f64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(val) = parsed.filter(|v| v.is_finite() && *v > 0.0 && *v <= 1.0) else {
            return;
        };
        let Some(session) = self.session(cx).cloned() else {
            return;
        };
        if (session.tutor_threshold - val).abs() < 1e-6 || self.is_ui_busy(cx) {
            return;
        }
        self.threshold_saving = true;
        self.threshold_saved_indicator = false;
        self.session_entity.update(cx, |s, cx| {
            s.change_mode(session.play_mode, Some(val), window, cx);
        });
        cx.notify();
    }

    fn undo_checker_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session(cx).cloned() else {
            return;
        };
        let locked = self.session_entity.read(cx).is_locked();
        let animating = self.animation_entity.read(cx).is_animating();
        let review_active = self.review.is_active(Some(&session));
        let base_board = board_array(&session.position.board);
        let human_u8 = u8::try_from(session.human_player).unwrap_or(0).min(1);
        let reversed = self.turn_entity.update(cx, |t, cx| {
            t.undo(&base_board, human_u8, locked, animating, review_active, cx)
        });
        if let Some(frame) = reversed {
            // `reverse_frame` has `sound: None`, guaranteeing silent undo!
            self.animation_entity.update(cx, |anim, cx| {
                anim.enqueue(vec![PlayFrame::Checker(frame)], Some(window), cx);
            });
        }
    }

    fn request_pause(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session(cx).cloned() else {
            return;
        };
        let has_draft = !self.turn_entity.read(cx).steps().is_empty()
            && session
                .pending_decision
                .as_ref()
                .and_then(|d| d.action.as_deref())
                != Some("move");
        if has_draft {
            self.session_entity.update(cx, |s, cx| s.clear_error(cx));
            self.pause_warning_open = true;
            cx.notify();
        } else {
            self.pause_match(window, cx);
        }
    }

    fn pause_match(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.animation_entity.read(cx).is_animating() {
            return;
        }
        self.pause_warning_open = false;
        self.session_entity.update(cx, |s, cx| {
            s.perform_action(PlayActionKind::Pause, None, None, None, window, cx);
        });
        cx.notify();
    }

    fn resume_match(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session(cx).cloned() else {
            return;
        };
        if session.paused {
            self.session_entity.update(cx, |s, cx| {
                s.perform_action(PlayActionKind::Resume, None, None, None, window, cx);
            });
        } else {
            self.setup_open = false;
        }
        self.push_stage_snapshot(cx);
        cx.notify();
    }

    fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        let next = !self.animation_entity.read(cx).is_sounds_enabled();
        self.animation_entity.update(cx, |anim, cx| {
            anim.set_sounds_enabled(next, cx);
        });
        if let Some(mut state) = cx.try_global::<LocalUiState>().cloned() {
            state.play_sounds = next;
            cx.set_global(state);
        }
        cx.notify();
    }

    fn select_history_index(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_ui_busy(cx) {
            return;
        }
        let Some(session) = self.session(cx).cloned() else {
            return;
        };
        self.turn_entity.update(cx, |t, cx| t.set_hover(None, cx));
        if !self.review.select(&session, index) {
            return;
        }
        if !self.review.has_cached_for(&session) {
            self.fetch_history_snapshots(&session, window, cx);
        }
        self.push_stage_snapshot(cx);
        cx.notify();
    }

    fn fetch_history_snapshots(
        &mut self,
        session: &PlaySession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        let session_id = session.id;
        let version = session.version;
        let expected_len = session.history.len();
        self.review.mark_loading(session);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let read_task = TokioBridge::spawn_read(cx, async move {
            let res = run_facade(facade, move |f| async move {
                f.play_history(session_id, version).await
            })
            .await;
            let _ = tx.send(res);
        });
        self._history_read = Some(read_task);

        cx.spawn_in(window, async move |this, cx| {
            let Ok(result) = rx.await else {
                return;
            };
            let _ = this.update_in(cx, |ws, _window, cx| {
                let locale = current_locale(cx);
                match result {
                    Ok(snaps)
                        if snaps.session_id == session_id
                            && snaps.version == version
                            && snaps.items.len() == expected_len =>
                    {
                        ws.review.set_snapshots(snaps);
                    }
                    Ok(_) => {
                        ws.review.set_error(
                            session_id,
                            version,
                            I18n::t(locale, "app", "error.invalidResponse").to_string(),
                        );
                    }
                    Err(err) => {
                        ws.review
                            .set_error(session_id, version, format_domain_error(locale, &err));
                    }
                }
                ws.push_stage_snapshot(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn exit_history_review(&mut self, cx: &mut Context<Self>) {
        if self.review.reset() {
            // Uncommitted local checker steps in `turn_entity` stay completely untouched!
            self.push_stage_snapshot(cx);
            cx.notify();
        }
    }

    fn export_sgf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session(cx).cloned() else {
            return;
        };
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        let session_id = session.id;
        let write_task = TokioBridge::spawn_write(
            cx,
            run_facade(facade, move |f| async move {
                f.play_session_sgf(session_id).await
            }),
        );
        cx.spawn_in(window, async move |this, cx| {
            let Ok(result) = write_task.await else {
                return;
            };
            let _ = this.update_in(cx, |_ws, _window, cx| match result {
                Ok(sgf) => {
                    let filename = format!("neondojo-match-{session_id}.sgf");
                    let target = dirs::download_dir()
                        .unwrap_or_else(std::env::temp_dir)
                        .join(&filename);
                    let msg = match std::fs::write(&target, sgf) {
                        Ok(()) => format!("Saved SGF to {}", target.display()),
                        Err(err) => format!("Could not write SGF: {err}"),
                    };
                    cx.emit(WorkspaceEvent::ShowNotice(AppNotice {
                        id: SharedString::from(format!("play-sgf-{session_id}")),
                        kind: BannerKind::Success,
                        title: None,
                        message: AppNoticeText::Literal(msg.into()),
                        dismissible: true,
                    }));
                }
                Err(err) => {
                    let locale = current_locale(cx);
                    cx.emit(WorkspaceEvent::ShowNotice(AppNotice {
                        id: SharedString::from("play-sgf-error"),
                        kind: BannerKind::Error,
                        title: None,
                        message: AppNoticeText::Literal(format_domain_error(locale, &err).into()),
                        dismissible: true,
                    }));
                }
            });
        })
        .detach();
    }

    fn open_learning_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session(cx).cloned() else {
            return;
        };
        if session.review.is_some() {
            self.learning_review_open = true;
            cx.notify();
        } else {
            self.learning_review_open = true;
            self.session_entity.update(cx, |s, cx| {
                s.review_session(window, cx);
            });
            cx.notify();
        }
    }

    fn sync_preview_board(&self, cx: &mut Context<Self>) {
        let Some(session) = self.session(cx) else {
            return;
        };
        let Some(decision) = session.pending_decision.as_ref() else {
            return;
        };
        let alt = self
            .selected_candidate
            .and_then(|i| decision.alternatives.get(i))
            .filter(|a| a.action == "move")
            .or_else(|| decision.alternatives.iter().find(|a| a.action == "move"));
        let base_pos = position_to_board_position(&session.position);
        let preview = alt.map(|a| {
            project_move(
                &base_pos.board,
                base_pos.player,
                &base_pos.dice,
                &a.notation,
            )
        });
        let direction = if session.bearoff_direction == "left" {
            BearoffDirection::Left
        } else {
            BearoffDirection::Right
        };
        let orientation = PlayOrientation {
            human: u8::try_from(session.human_player).unwrap_or(0).min(1),
            direction,
        };
        let theme_cfg = self.theme_config.clone();
        let view_mode = self.preview_mode;
        self.preview_board.update(cx, |bv, cx| {
            bv.set_theme_config(theme_cfg, cx);
            bv.set_play_orientation(Some(orientation), cx);
            bv.set_position(base_pos, cx);
            bv.set_preview(preview, view_mode, cx);
            bv.set_show_pip_counts(true, cx);
            bv.set_interaction(None, cx);
        });
    }

    fn push_stage_snapshot(&self, cx: &mut Context<Self>) {
        let session = self.session(cx).cloned();
        let busy = self.session_entity.read(cx).is_busy();
        let animating = self.animation_entity.read(cx).is_animating();
        let pending_roll = self.animation_entity.read(cx).pending_roll();
        let ui_busy = busy || animating;
        let paused = session.as_ref().is_some_and(|s| s.paused);
        let review_active = self.review.is_active(session.as_ref());
        let fb = play_feedback_state(session.as_ref());
        let interruption = self.active_interruption(cx);

        let interactive =
            !paused && !self.hint_open && !self.resign_open && !fb.awaiting && !review_active;
        let can_roll = self.visible
            && !self.setup_open
            && interactive
            && session
                .as_ref()
                .is_some_and(|s| matches!(s.phase, PlayPhase::Roll));
        let can_move = session.as_ref().is_some_and(|s| {
            !busy
                && interactive
                && matches!(s.phase, PlayPhase::Move)
                && s.position.player == s.human_player
        });
        let forced_pass = self.turn_entity.read(cx).forced_pass(can_move);
        let can_pass = forced_pass
            && self.visible
            && !self.setup_open
            && !ui_busy
            && pending_roll.is_none()
            && !self.pause_warning_open
            && !self.forfeit_open;
        let roll_ready = can_roll && !ui_busy && interruption.is_none() && !self.pause_warning_open;

        let coach_pending = (fb.mode == PlayMode::Coach
            && matches!(
                fb.pending.and_then(|d| d.action.as_deref()),
                Some("move") | Some("pass")
            ))
        .then_some(fb.pending)
        .flatten();
        let can_continue_coach = coach_pending.is_some()
            && self.visible
            && !self.setup_open
            && !paused
            && !ui_busy
            && !self.hint_open
            && !self.resign_open
            && !self.forfeit_open
            && !review_active;

        let before_roll = session
            .as_ref()
            .is_some_and(|s| matches!(s.phase, PlayPhase::Roll));
        let show_board_actions = session.as_ref().is_some_and(|s| {
            !paused
                && !review_active
                && interruption.is_none()
                && !fb.awaiting
                && (before_roll
                    || (matches!(s.phase, PlayPhase::Move) && s.position.player == s.human_player))
        });
        let opening_phase = !review_active
            && session.as_ref().is_some_and(|s| {
                matches!(s.phase, PlayPhase::OpeningRoll | PlayPhase::OpeningResult)
            });

        let white_name = self.player_name(session.as_ref(), 0, cx);
        let black_name = self.player_name(session.as_ref(), 1, cx);
        let idle_direction = if self.setup_choices.bearoff_direction == "left" {
            BearoffDirection::Left
        } else {
            BearoffDirection::Right
        };

        let can_continue_coach_move =
            can_continue_coach && coach_pending.and_then(|d| d.action.as_deref()) == Some("move");
        let can_continue_coach_pass =
            can_continue_coach && coach_pending.and_then(|d| d.action.as_deref()) == Some("pass");

        let snapshot = PlayStageSnapshot {
            session,
            idle_human: u8::try_from(self.setup_choices.human_player)
                .unwrap_or(0)
                .min(1),
            idle_direction,
            theme_config: self.theme_config.clone(),
            paused,
            busy,
            review: self.review.clone(),
            interrupted: interruption.is_some(),
            show_board_actions,
            can_move,
            roll_ready,
            can_pass,
            can_continue_coach_move,
            can_continue_coach_pass,
            opening_phase,
            white_name,
            black_name,
        };

        self.board_stage.update(cx, |stage, cx| {
            stage.update_snapshot(snapshot, cx);
        });
    }

    fn start_setup_match(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.visible && self.setup_open && !self.is_ui_busy(cx) {
            let request = self.setup_choices.to_create_request();
            self.session_entity.update(cx, |session, cx| {
                session.create_session(request, window, cx);
            });
        }
    }

    fn handle_primary_step(
        &mut self,
        _: &PrimaryPlayStep,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.visible || self.setup_open || self.is_ui_busy(cx) {
            return;
        }
        let Some(session) = self.session(cx).cloned() else {
            return;
        };
        if session.paused || self.review.is_active(Some(&session)) {
            return;
        }
        let fb = play_feedback_state(Some(&session));
        if fb.awaiting {
            self.continue_pending_decision(window, cx);
            return;
        }
        if matches!(session.phase, PlayPhase::Roll) {
            self.roll_turn(window, cx);
        } else if matches!(session.phase, PlayPhase::Move)
            && session.position.player == session.human_player
        {
            let (forced, completed) = {
                let turn = self.turn_entity.read(cx);
                (turn.forced_pass(true), turn.completed_move_index(true))
            };
            if forced {
                self.dispatch_action(PlayActionKind::Pass, None, None, None, window, cx);
            } else if let Some(move_idx) = completed {
                self.dispatch_action(PlayActionKind::Move, Some(move_idx), None, None, window, cx);
            }
        }
    }

    fn handle_offer_double(
        &mut self,
        _: &OfferDouble,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.visible || self.setup_open {
            return;
        }
        self.double_cube(window, cx);
    }

    fn handle_request_hint(
        &mut self,
        _: &RequestHint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.visible || self.setup_open {
            return;
        }
        self.request_feedback(FeedbackOperation::Hint, window, cx);
    }

    fn handle_undo_step(
        &mut self,
        _: &UndoCheckerStep,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.visible || self.setup_open {
            return;
        }
        self.undo_checker_step(window, cx);
    }

    fn handle_return_to_live(
        &mut self,
        _: &ReturnToLiveGame,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.visible {
            return;
        }
        if self.review.is_active(self.session(cx)) {
            self.exit_history_review(cx);
        } else if self.more_menu_open {
            self.more_menu_open = false;
            cx.notify();
        } else if self.setup_open && !self.session_entity.read(cx).is_locked() {
            self.setup_open = false;
            self.push_stage_snapshot(cx);
            cx.notify();
        }
    }

    fn handle_show_initial(
        &mut self,
        _: &ShowInitialBoard,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.review.is_active(self.session(cx)) {
            self.review.set_view_mode(BoardViewMode::Before);
            self.push_stage_snapshot(cx);
            cx.notify();
        } else {
            self.preview_mode = BoardViewMode::Before;
            self.sync_preview_board(cx);
            cx.notify();
        }
    }

    fn handle_show_final(
        &mut self,
        _: &ShowFinalBoard,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.review.is_active(self.session(cx)) {
            self.review.set_view_mode(BoardViewMode::After);
            self.push_stage_snapshot(cx);
            cx.notify();
        } else {
            self.preview_mode = BoardViewMode::After;
            self.sync_preview_board(cx);
            cx.notify();
        }
    }

    fn render_error_banner(&self, p: NeonPalette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let err = self.session_entity.read(cx).error()?;
        let locale = current_locale(cx);
        let msg = if self.hint_open {
            format!("{} {}", I18n::t(locale, "play", "hintError"), err.message)
        } else {
            err.message.clone()
        };
        let retry_label = I18n::t(locale, "play", "retry");
        Some(
            StatusBanner::new("play-error-banner", BannerKind::Error, msg)
                .palette(p)
                .on_retry(
                    retry_label,
                    cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.session_entity.update(cx, |s, cx| {
                            s.retry(window, cx);
                        });
                    }),
                )
                .on_dismiss(cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.session_entity.update(cx, |s, cx| s.clear_error(cx));
                }))
                .into_any_element(),
        )
    }

    fn render_match_header(
        &self,
        session: &PlaySession,
        p: NeonPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = current_locale(cx);
        let ui_busy = self.is_ui_busy(cx);
        let paused = session.paused;
        let money_finished = is_money_game_finished(Some(session));
        let interruption = self.active_interruption(cx);
        let review_active = self.review.is_active(Some(session));
        let fb = play_feedback_state(Some(session));
        let sounds_on = self.animation_entity.read(cx).is_sounds_enabled();

        let white = self.player_name(Some(session), 0, cx);
        let black = self.player_name(Some(session), 1, cx);
        let score_line = format!(
            "{white} {} : {} {black}",
            format_int(locale, i64::from(session.scores[0])),
            format_int(locale, i64::from(session.scores[1]))
        );
        let format_line = if session.match_length > 0 {
            format!(
                "{} · {} {}",
                I18n::format(
                    locale,
                    "app",
                    "board.points",
                    &[("count", Param::from(session.match_length))]
                ),
                I18n::t(locale, "play", "game"),
                format_int(locale, i64::from(session.game_no))
            )
        } else {
            format!(
                "{} · {} {}",
                I18n::t(locale, "app", "common.moneyGame"),
                I18n::t(locale, "play", "game"),
                format_int(locale, i64::from(session.game_no))
            )
        };

        let strength_text = if let Some(diff) = session.engine.difficulty {
            let key = match diff {
                neondojo::engines::SageDifficulty::Beginner => "beginner",
                neondojo::engines::SageDifficulty::Casual => "casual",
                neondojo::engines::SageDifficulty::Intermediate => "intermediate",
                neondojo::engines::SageDifficulty::FullStrength => "full_strength",
            };
            I18n::t(locale, "play", key).to_string()
        } else if session.engine.top_two {
            format!(
                "{} + {}",
                format_ply(locale, session.engine.plies.max(1) as u32),
                I18n::t(locale, "profile", "top_two")
            )
        } else {
            format_ply(locale, session.engine.plies.max(1) as u32)
        };
        let bot_name = if session.engine.id == "bgsage" {
            "NeonGammon"
        } else {
            "GNUbg"
        };

        let visual = self.animation_entity.read(cx).current_visual().cloned();
        let pending_roll = self.animation_entity.read(cx).pending_roll();
        let forced_pass = self.turn_entity.read(cx).forced_pass(
            !paused
                && matches!(session.phase, PlayPhase::Move)
                && session.position.player == session.human_player,
        );
        let status_text = compute_play_turn_status(
            locale,
            &PlayTurnStatusContext {
                session: Some(session),
                review_active,
                paused,
                visual: visual.as_ref(),
                opening_phase: !review_active
                    && matches!(
                        session.phase,
                        PlayPhase::OpeningRoll | PlayPhase::OpeningResult
                    ),
                pending_roll,
                busy: self.session_entity.read(cx).is_busy(),
                hint_open: self.hint_open,
                awaiting_feedback: fb.awaiting,
                forced_pass,
            },
            |pl| self.player_name(Some(session), pl, cx),
        );

        let mode_disabled = ui_busy
            || paused
            || matches!(session.phase, PlayPhase::MatchOver)
            || money_finished
            || interruption.is_some();

        let mut mode_buttons = div().h_flex().items_center().gap(px(4.0));
        for &(mode, key, _) in PLAY_MODES {
            mode_buttons = mode_buttons.child(
                ActionButton::new(SharedString::from(format!("play-live-mode-{key}")))
                    .label(I18n::t(locale, "play-mode", key))
                    .active(session.play_mode == mode)
                    .disabled(mode_disabled)
                    .palette(p)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.switch_mode(mode, window, cx);
                    })),
            );
        }

        let can_resign = !paused
            && !matches!(session.phase, PlayPhase::GameOver | PlayPhase::MatchOver)
            && interruption.is_none()
            && !review_active
            && !fb.awaiting;
        let can_pause = !paused
            && !session.money_session_ended
            && (!matches!(session.phase, PlayPhase::MatchOver) || money_finished)
            && interruption.is_none();
        let can_forfeit =
            session.match_length > 0 && !paused && !matches!(session.phase, PlayPhase::MatchOver);

        let sound_label = if sounds_on {
            format!("{} ✓", I18n::t(locale, "play", "sounds"))
        } else {
            format!("{} ×", I18n::t(locale, "play", "sounds"))
        };

        div()
            .id("play-match-header")
            .test_support()
            .h_flex()
            .items_center()
            .justify_between()
            .flex_wrap()
            .gap(px(12.0))
            .px(px(14.0))
            .py(px(10.0))
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
                            .h_flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_subject()
                                    .tabular_nums()
                                    .text_color(p.text)
                                    .child(score_line),
                            )
                            .child(
                                div()
                                    .text_meta()
                                    .tabular_nums()
                                    .text_color(p.muted)
                                    .child(format_line),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(div().text_meta().text_color(p.muted).child(format!(
                                "{} · {bot_name} · {strength_text}",
                                I18n::t(locale, "play", "botEngine")
                            )))
                            .child("·")
                            .child(
                                div()
                                    .id("play-match-status")
                                    .test_support()
                                    .text_label()
                                    .text_color(p.accent)
                                    .child(status_text),
                            ),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(mode_buttons)
                    .when(session.play_mode == PlayMode::Tutor, |this| {
                        this.child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(div().text_meta().text_color(p.muted).child(I18n::t(
                                    locale,
                                    "play-mode",
                                    "threshold",
                                )))
                                .child(self.tutor_threshold_input.clone())
                                .when(
                                    self.threshold_saving || self.threshold_saved_indicator,
                                    |s| {
                                        let key = if self.threshold_saving {
                                            "thresholdSaving"
                                        } else {
                                            "thresholdSaved"
                                        };
                                        s.child(
                                            div().text_micro().text_color(p.muted).child(I18n::t(
                                                locale,
                                                "play-mode",
                                                key,
                                            )),
                                        )
                                    },
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        ActionButton::new("play-sound-toggle")
                            .label(sound_label)
                            .active(sounds_on)
                            .variant(ActionButtonVariant::Ghost)
                            .palette(p)
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.toggle_mute(cx);
                            })),
                    )
                    .when(can_resign, |this| {
                        this.child(
                            ActionButton::new("play-header-resign")
                                .icon(ActionIconName::Resign)
                                .label(I18n::t(locale, "play-resign", "confirmGame"))
                                .disabled(ui_busy)
                                .palette(p)
                                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                    if !this.is_ui_busy(cx) {
                                        this.session_entity.update(cx, |s, cx| s.clear_error(cx));
                                        this.resign_level = ResignLevel::Single;
                                        this.resign_open = true;
                                        this.push_stage_snapshot(cx);
                                        cx.notify();
                                    }
                                })),
                        )
                    })
                    .when(can_pause, |this| {
                        this.child(
                            ActionButton::new("play-header-pause")
                                .icon(ActionIconName::Pause)
                                .label(I18n::t(locale, "play", "pauseMatch"))
                                .disabled(ui_busy)
                                .palette(p)
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    this.request_pause(window, cx);
                                })),
                        )
                    })
                    .when(paused, |this| {
                        this.child(
                            ActionButton::new("play-header-resume")
                                .icon(ActionIconName::Resume)
                                .label(I18n::t(locale, "play", "resumeMatch"))
                                .primary()
                                .disabled(ui_busy)
                                .palette(p)
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    this.resume_match(window, cx);
                                })),
                        )
                    })
                    .child(
                        ActionButton::new("play-header-more")
                            .icon(ActionIconName::More)
                            .tooltip(I18n::t(locale, "play", "moreActions"))
                            .compact()
                            .active(self.more_menu_open)
                            .disabled(ui_busy)
                            .palette(p)
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                this.more_menu_open = !this.more_menu_open;
                                cx.notify();
                            })),
                    )
                    .when(self.more_menu_open, |this| {
                        this.child(
                            div()
                                .id("play-more-menu")
                                .test_support()
                                .h_flex()
                                .items_center()
                                .gap(px(6.0))
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(px(6.0))
                                .border_1()
                                .border_color(p.border_strong)
                                .bg(p.surface_alt)
                                .when(can_forfeit && interruption.is_none(), |m| {
                                    m.child(
                                        ActionButton::new("play-menu-forfeit")
                                            .icon(ActionIconName::Resign)
                                            .label(I18n::t(locale, "play-resign", "forfeitMatch"))
                                            .danger()
                                            .disabled(ui_busy || review_active)
                                            .palette(p)
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _window, cx| {
                                                    this.more_menu_open = false;
                                                    this.forfeit_open = true;
                                                    cx.notify();
                                                },
                                            )),
                                    )
                                })
                                .when(interruption != Some(PlayInterruption::MatchOver), |m| {
                                    m.child(
                                        ActionButton::new("play-menu-new-match")
                                            .icon(ActionIconName::NewGame)
                                            .label(I18n::t(locale, "play", "newMatch"))
                                            .disabled(ui_busy)
                                            .palette(p)
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, window, cx| {
                                                    this.open_new_match_setup(window, cx);
                                                },
                                            )),
                                    )
                                }),
                        )
                    }),
            )
            .into_any_element()
    }

    fn build_feedback_callbacks(&self, cx: &mut Context<Self>) -> PlayFeedbackCallbacks {
        let weak_retry = cx.weak_entity();
        let weak_reveal = cx.weak_entity();
        let weak_cont = cx.weak_entity();
        let weak_sel = cx.weak_entity();
        let weak_mode = cx.weak_entity();
        let weak_reset = cx.weak_entity();

        PlayFeedbackCallbacks {
            on_retry: Rc::new(move |window, cx| {
                let _ = weak_retry.update(cx, |ws, cx| {
                    ws.coach_details_open = false;
                    ws.request_feedback(FeedbackOperation::Retry, window, cx);
                });
            }),
            on_reveal: Rc::new(move |window, cx| {
                let _ = weak_reveal.update(cx, |ws, cx| {
                    ws.request_feedback(FeedbackOperation::Reveal, window, cx);
                });
            }),
            on_continue: Rc::new(move |window, cx| {
                let _ = weak_cont.update(cx, |ws, cx| {
                    ws.coach_details_open = false;
                    ws.continue_pending_decision(window, cx);
                });
            }),
            on_select_candidate: Rc::new(move |idx, _window, cx| {
                let _ = weak_sel.update(cx, |ws, cx| {
                    ws.selected_candidate = Some(idx);
                    ws.sync_preview_board(cx);
                    cx.notify();
                });
            }),
            on_preview_mode: Rc::new(move |mode, _window, cx| {
                let _ = weak_mode.update(cx, |ws, cx| {
                    ws.preview_mode = mode;
                    ws.sync_preview_board(cx);
                    cx.notify();
                });
            }),
            on_reset_preview: Rc::new(move |_window, cx| {
                let _ = weak_reset.update(cx, |ws, cx| {
                    ws.selected_candidate = None;
                    ws.preview_mode = BoardViewMode::Arrows;
                    ws.sync_preview_board(cx);
                    cx.notify();
                });
            }),
        }
    }

    fn render_active_dialog(
        &mut self,
        p: NeonPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let locale = current_locale(cx);
        let session = self.session(cx).cloned();
        let ui_busy = self.is_ui_busy(cx);
        let weak_drag_start = cx.weak_entity();
        let weak_drag_move = cx.weak_entity();
        let weak_drag_end = cx.weak_entity();
        let on_drag_start = Rc::new(move |pos, _win: &mut Window, cx: &mut App| {
            let _ = weak_drag_start.update(cx, |ws, cx| {
                ws.dialog_state.start_drag(pos);
                cx.notify();
            });
        });
        let on_drag_move = Rc::new(move |pos, dlg_size, win: &mut Window, cx: &mut App| {
            let vp = win.viewport_size();
            let vp_size = (vp.width.into(), vp.height.into());
            let _ = weak_drag_move.update(cx, |ws, cx| {
                if ws.dialog_state.update_drag(pos, dlg_size, vp_size) {
                    cx.notify();
                }
            });
        });
        let on_drag_end = Rc::new(move |_win: &mut Window, cx: &mut App| {
            let _ = weak_drag_end.update(cx, |ws, cx| {
                if ws.dialog_state.end_drag() {
                    cx.notify();
                }
            });
        });

        // 1. Setup dialog (non-modal; preserves empty-state focus exception!)
        if self.visible && self.setup_open {
            let setup_key = if self.setup_focus_on_open {
                "setup-focused"
            } else {
                "setup-idle"
            };
            self.dialog_state.sync_dialog_key(
                Some(setup_key),
                false,
                self.setup_focus_on_open,
                window,
                cx,
            );
            let error_banner = self.render_error_banner(p, cx);
            let weak = cx.weak_entity();
            let cb = PlaySetupCallbacks {
                on_engine: {
                    let w = weak.clone();
                    Rc::new(move |eng, _win, cx| {
                        let _ = w.update(cx, |ws, cx| {
                            ws.setup_choices.select_engine(eng);
                            cx.notify();
                        });
                    })
                },
                on_ply: {
                    let w = weak.clone();
                    Rc::new(move |ply, _win, cx| {
                        let _ = w.update(cx, |ws, cx| {
                            ws.setup_choices.select_ply(ply);
                            cx.notify();
                        });
                    })
                },
                on_top_two: {
                    let w = weak.clone();
                    Rc::new(move |_win, cx| {
                        let _ = w.update(cx, |ws, cx| {
                            ws.setup_choices.select_top_two();
                            cx.notify();
                        });
                    })
                },
                on_difficulty: {
                    let w = weak.clone();
                    Rc::new(move |diff, _win, cx| {
                        let _ = w.update(cx, |ws, cx| {
                            ws.setup_choices.select_difficulty(diff);
                            cx.notify();
                        });
                    })
                },
                on_human_player: {
                    let w = weak.clone();
                    Rc::new(move |human, _win, cx| {
                        let _ = w.update(cx, |ws, cx| {
                            ws.setup_choices.human_player = human;
                            ws.push_stage_snapshot(cx);
                            cx.notify();
                        });
                    })
                },
                on_match_length: {
                    let w = weak.clone();
                    Rc::new(move |len, _win, cx| {
                        let _ = w.update(cx, |ws, cx| {
                            ws.setup_choices.match_length = len;
                            cx.notify();
                        });
                    })
                },
                on_mode: {
                    let w = weak.clone();
                    Rc::new(move |mode, _win, cx| {
                        let _ = w.update(cx, |ws, cx| {
                            ws.setup_choices.mode = mode;
                            cx.notify();
                        });
                    })
                },
                on_auto_analysis: {
                    let w = weak.clone();
                    Rc::new(move |auto, _win, cx| {
                        let _ = w.update(cx, |ws, cx| {
                            ws.setup_choices.training_auto_analysis = auto;
                            cx.notify();
                        });
                    })
                },
                on_open_settings: {
                    let w = weak.clone();
                    Rc::new(move |_win, cx| {
                        let _ = w.update(cx, |_ws, cx| {
                            cx.emit(WorkspaceEvent::OpenSettings {
                                category: SettingsCategory::Play,
                            });
                        });
                    })
                },
                on_submit: {
                    let w = weak.clone();
                    Rc::new(move |win, cx| {
                        let _ = w.update(cx, |ws, cx| {
                            ws.start_setup_match(win, cx);
                        });
                    })
                },
                on_resume: {
                    let w = weak.clone();
                    Rc::new(move |win, cx| {
                        let _ = w.update(cx, |ws, cx| ws.resume_match(win, cx));
                    })
                },
                on_open_paused_matches: {
                    let w = weak.clone();
                    Rc::new(move |_win, cx| {
                        let _ = w.update(cx, |_, cx| {
                            cx.emit(WorkspaceEvent::OpenLibraryDatabase {
                                database_id: PAUSED_MATCHES_DATABASE_ID,
                            });
                        });
                    })
                },
            };

            let (body, footer) = render_play_setup_form(
                &self.setup_choices,
                &self.setup_name_input,
                &self.engines,
                session.as_ref(),
                error_banner,
                ui_busy,
                p,
                cb,
                window.viewport_size().width >= px(880.0),
                cx,
            );
            let on_close = {
                let w = weak;
                Rc::new(move |_win: &mut Window, cx: &mut App| {
                    let _ = w.update(cx, |ws, cx| {
                        if !ws.session_entity.read(cx).is_locked() {
                            ws.setup_open = false;
                            ws.session_entity.update(cx, |s, cx| s.clear_error(cx));
                            ws.push_stage_snapshot(cx);
                            cx.notify();
                        }
                    });
                })
            };
            return Some(render_play_board_dialog(
                PlayBoardDialogProps {
                    id: "play-setup-dialog".into(),
                    heading: SharedString::from(I18n::t(locale, "play", "newMatch")),
                    width_px: 960.0,
                    max_height_px: 760.0,
                    footer: Some(footer),
                    modal: false,
                    dismissible: true,
                    busy: ui_busy,
                    palette: p,
                },
                &self.dialog_state,
                body,
                Some(on_close),
                on_drag_start,
                on_drag_move,
                on_drag_end,
                window,
                cx,
            ));
        }

        let session = session?;
        let white_name = self.player_name(Some(&session), 0, cx);
        let black_name = self.player_name(Some(&session), 1, cx);
        let can_forfeit = session.match_length > 0
            && !session.paused
            && !matches!(session.phase, PlayPhase::MatchOver);

        // 2. Forfeit confirmation (stacks above other dialogs when opened)
        if self.forfeit_open && can_forfeit {
            self.dialog_state
                .sync_dialog_key(Some("forfeit"), true, true, window, cx);
            let err = self.render_error_banner(p, cx);
            let w1 = cx.weak_entity();
            let w2 = cx.weak_entity();
            let close_cb = Rc::new(move |_win: &mut Window, cx: &mut App| {
                let _ = w1.update(cx, |ws, cx| {
                    if !ws.session_entity.read(cx).is_locked() {
                        ws.forfeit_open = false;
                        ws.session_entity.update(cx, |s, cx| s.clear_error(cx));
                        cx.notify();
                    }
                });
            });
            let body = render_forfeit_dialog_body(
                ui_busy,
                err,
                p,
                close_cb.clone(),
                Rc::new(move |win, cx| {
                    let _ = w2.update(cx, |ws, cx| {
                        ws.dispatch_action(PlayActionKind::ForfeitMatch, None, None, None, win, cx);
                    });
                }),
                cx,
            );
            return Some(render_play_board_dialog(
                PlayBoardDialogProps {
                    id: "play-forfeit-dialog".into(),
                    heading: SharedString::from(I18n::t(locale, "play-resign", "forfeitMatch")),
                    width_px: 440.0,
                    max_height_px: 420.0,
                    footer: None,
                    modal: true,
                    dismissible: true,
                    busy: ui_busy,
                    palette: p,
                },
                &self.dialog_state,
                body,
                Some(close_cb),
                on_drag_start,
                on_drag_move,
                on_drag_end,
                window,
                cx,
            ));
        }

        // 3. Pause unsaved-draft warning dialog
        if self.pause_warning_open {
            self.dialog_state
                .sync_dialog_key(Some("pause-warning"), true, true, window, cx);
            let err = self.render_error_banner(p, cx);
            let w1 = cx.weak_entity();
            let w2 = cx.weak_entity();
            let close_cb = Rc::new(move |_win: &mut Window, cx: &mut App| {
                let _ = w1.update(cx, |ws, cx| {
                    if !ws.session_entity.read(cx).is_locked() {
                        ws.pause_warning_open = false;
                        ws.session_entity.update(cx, |s, cx| s.clear_error(cx));
                        cx.notify();
                    }
                });
            });
            let body = render_pause_warning_body(
                ui_busy,
                err,
                p,
                close_cb.clone(),
                Rc::new(move |win, cx| {
                    let _ = w2.update(cx, |ws, cx| ws.pause_match(win, cx));
                }),
                cx,
            );
            return Some(render_play_board_dialog(
                PlayBoardDialogProps {
                    id: "play-pause-dialog".into(),
                    heading: SharedString::from(I18n::t(locale, "play", "pauseMatch")),
                    width_px: 430.0,
                    max_height_px: 360.0,
                    footer: None,
                    modal: true,
                    dismissible: true,
                    busy: ui_busy,
                    palette: p,
                },
                &self.dialog_state,
                body,
                Some(close_cb),
                on_drag_start,
                on_drag_move,
                on_drag_end,
                window,
                cx,
            ));
        }

        // 4. Learning review modal
        if self.learning_review_open
            && let Some(review) = session.review.as_ref()
        {
            self.dialog_state
                .sync_dialog_key(Some("learning-review"), true, true, window, cx);
            let w1 = cx.weak_entity();
            let w2 = cx.weak_entity();
            let close_cb = Rc::new(move |_win: &mut Window, cx: &mut App| {
                let _ = w1.update(cx, |ws, cx| {
                    ws.learning_review_open = false;
                    cx.notify();
                });
            });
            let body = render_learning_review_body(
                &session,
                review,
                p,
                Rc::new(move |mid, _win, cx| {
                    let _ = w2.update(cx, |_ws, cx| {
                        cx.emit(WorkspaceEvent::OpenStudyReport { match_id: mid });
                    });
                }),
                cx,
            );
            return Some(render_play_board_dialog(
                PlayBoardDialogProps {
                    id: "play-learning-review-dialog".into(),
                    heading: SharedString::from(I18n::t(locale, "play-mode", "review")),
                    width_px: 620.0,
                    max_height_px: 640.0,
                    footer: None,
                    modal: true,
                    dismissible: true,
                    busy: ui_busy,
                    palette: p,
                },
                &self.dialog_state,
                body,
                Some(close_cb),
                on_drag_start,
                on_drag_move,
                on_drag_end,
                window,
                cx,
            ));
        }

        // 5. Coach Details modal (opened from inline PlayCoachToolbar)
        if self.coach_details_open && self.coach_decision(Some(&session), cx).is_some() {
            self.dialog_state
                .sync_dialog_key(Some("coach-details"), true, true, window, cx);
            let err = self.render_error_banner(p, cx);
            let fb_cb = self.build_feedback_callbacks(cx);
            let w1 = cx.weak_entity();
            let close_cb = Rc::new(move |_win: &mut Window, cx: &mut App| {
                let _ = w1.update(cx, |ws, cx| {
                    ws.coach_details_open = false;
                    cx.notify();
                });
            });
            let preview_opt = has_play_move_preview(&session).then_some(&self.preview_board);
            let body = div()
                .v_flex()
                .gap(px(10.0))
                .when_some(err, |this, e| this.child(e))
                .when_some(
                    render_feedback_body(
                        &session,
                        &white_name,
                        &black_name,
                        ui_busy,
                        self.selected_candidate,
                        self.preview_mode,
                        preview_opt,
                        p,
                        fb_cb,
                        cx,
                    ),
                    |this, panel| this.child(panel),
                )
                .into_any_element();
            return Some(render_play_board_dialog(
                PlayBoardDialogProps {
                    id: "play-coach-details-dialog".into(),
                    heading: SharedString::from(I18n::t(locale, "play-mode", "details")),
                    width_px: if preview_opt.is_some() { 700.0 } else { 540.0 },
                    max_height_px: 720.0,
                    footer: None,
                    modal: true,
                    dismissible: true,
                    busy: ui_busy,
                    palette: p,
                },
                &self.dialog_state,
                body,
                Some(close_cb),
                on_drag_start,
                on_drag_move,
                on_drag_end,
                window,
                cx,
            ));
        }

        // 6. Authoritative `playInterruption` stack
        let Some(interruption) = self.active_interruption(cx) else {
            self.dialog_state
                .sync_dialog_key(None, false, false, window, cx);
            return None;
        };

        let key = match interruption {
            PlayInterruption::Resign => "interruption-resign",
            PlayInterruption::Hint => "interruption-hint",
            PlayInterruption::Cube => "interruption-cube",
            PlayInterruption::Feedback => "interruption-feedback",
            PlayInterruption::GameOver => "interruption-game-over",
            PlayInterruption::MatchOver => "interruption-match-over",
        };
        self.dialog_state
            .sync_dialog_key(Some(key), true, true, window, cx);

        let err_banner = self.render_error_banner(p, cx);
        let resignation_declined = self
            .session_entity
            .read(cx)
            .error()
            .is_some_and(|e| e.resignation_declined);
        let fb = play_feedback_state(Some(&session));

        match interruption {
            PlayInterruption::Resign => {
                let w_lvl = cx.weak_entity();
                let w_ok = cx.weak_entity();
                let w_forfeit = cx.weak_entity();
                let w_close = cx.weak_entity();
                let close_cb = Rc::new(move |_win: &mut Window, cx: &mut App| {
                    let _ = w_close.update(cx, |ws, cx| {
                        if !ws.session_entity.read(cx).is_locked() {
                            ws.resign_open = false;
                            ws.session_entity.update(cx, |s, cx| s.clear_error(cx));
                            ws.push_stage_snapshot(cx);
                            cx.notify();
                        }
                    });
                });
                let body = render_resign_dialog_body(
                    &session,
                    &white_name,
                    &black_name,
                    self.resign_level,
                    ui_busy,
                    can_forfeit,
                    resignation_declined,
                    err_banner,
                    p,
                    Rc::new(move |lvl, _win, cx| {
                        let _ = w_lvl.update(cx, |ws, cx| {
                            ws.resign_level = lvl;
                            cx.notify();
                        });
                    }),
                    Rc::new(move |lvl, win, cx| {
                        let _ = w_ok.update(cx, |ws, cx| {
                            ws.dispatch_action(
                                PlayActionKind::Resign,
                                None,
                                None,
                                Some(lvl.as_i32()),
                                win,
                                cx,
                            );
                        });
                    }),
                    Rc::new(move |_win, cx| {
                        let _ = w_forfeit.update(cx, |ws, cx| {
                            ws.resign_open = false;
                            ws.forfeit_open = true;
                            cx.notify();
                        });
                    }),
                    close_cb.clone(),
                    cx,
                );
                Some(render_play_board_dialog(
                    PlayBoardDialogProps {
                        id: "play-resign-dialog".into(),
                        heading: SharedString::from(I18n::t(locale, "play-resign", "heading")),
                        width_px: 520.0,
                        max_height_px: 580.0,
                        footer: None,
                        modal: true,
                        dismissible: true,
                        busy: ui_busy,
                        palette: p,
                    },
                    &self.dialog_state,
                    body,
                    Some(close_cb),
                    on_drag_start,
                    on_drag_move,
                    on_drag_end,
                    window,
                    cx,
                ))
            }
            PlayInterruption::Hint => {
                let w_close = cx.weak_entity();
                let close_cb = Rc::new(move |_win: &mut Window, cx: &mut App| {
                    let _ = w_close.update(cx, |ws, cx| {
                        if !ws.session_entity.read(cx).is_locked() {
                            ws.hint_open = false;
                            ws.hint_ready = false;
                            ws.session_entity.update(cx, |s, cx| s.clear_error(cx));
                            ws.push_stage_snapshot(cx);
                            cx.notify();
                        }
                    });
                });
                let loading = self.session_entity.read(cx).is_busy();
                let preview_opt = (self.hint_ready && has_play_move_preview(&session))
                    .then_some(&self.preview_board);
                let fb_cb = self.build_feedback_callbacks(cx);
                let body = div()
                    .id("play-hint-dialog-body")
                    .test_support()
                    .v_flex()
                    .gap(px(10.0))
                    .when_some(err_banner, |this, e| this.child(e))
                    .when(loading, |this| {
                        this.child(div().text_body().text_color(p.muted).child(I18n::t(
                            locale,
                            "play",
                            "hintLoading",
                        )))
                    })
                    .when(!loading && self.hint_ready, |this| {
                        this.when_some(session.pending_decision.as_ref(), |inner, dec| {
                            inner.child(dialogs::render_decision_details(
                                &session,
                                dec,
                                &white_name,
                                &black_name,
                                self.selected_candidate,
                                self.preview_mode,
                                preview_opt,
                                p,
                                &fb_cb,
                                cx,
                            ))
                        })
                    })
                    .into_any_element();
                Some(render_play_board_dialog(
                    PlayBoardDialogProps {
                        id: "play-hint-dialog".into(),
                        heading: SharedString::from(I18n::t(locale, "play-mode", "hint")),
                        width_px: if preview_opt.is_some() { 700.0 } else { 540.0 },
                        max_height_px: 720.0,
                        footer: None,
                        modal: true,
                        dismissible: true,
                        busy: ui_busy,
                        palette: p,
                    },
                    &self.dialog_state,
                    body,
                    Some(close_cb),
                    on_drag_start,
                    on_drag_move,
                    on_drag_end,
                    window,
                    cx,
                ))
            }
            PlayInterruption::Cube => {
                let fb_cb = self.build_feedback_callbacks(cx);
                let fb_panel = render_feedback_body(
                    &session,
                    &white_name,
                    &black_name,
                    ui_busy,
                    self.selected_candidate,
                    self.preview_mode,
                    None,
                    p,
                    fb_cb,
                    cx,
                );
                let w = cx.weak_entity();
                let body = render_cube_dialog_body(
                    &session,
                    ui_busy,
                    fb.awaiting,
                    fb.competition,
                    can_forfeit,
                    err_banner,
                    fb_panel,
                    p,
                    {
                        let w = w.clone();
                        Rc::new(move |win, cx| {
                            let _ = w.update(cx, |ws, cx| {
                                ws.dispatch_action(PlayActionKind::Take, None, None, None, win, cx);
                            });
                        })
                    },
                    {
                        let w = w.clone();
                        Rc::new(move |win, cx| {
                            let _ = w.update(cx, |ws, cx| {
                                ws.dispatch_action(PlayActionKind::Drop, None, None, None, win, cx);
                            });
                        })
                    },
                    {
                        let w = w.clone();
                        Rc::new(move |win, cx| {
                            let _ = w.update(cx, |ws, cx| {
                                ws.request_feedback(FeedbackOperation::Hint, win, cx);
                            });
                        })
                    },
                    {
                        let w = w.clone();
                        Rc::new(move |win, cx| {
                            let _ = w.update(cx, |ws, cx| ws.request_pause(win, cx));
                        })
                    },
                    {
                        let w = w;
                        Rc::new(move |_win, cx| {
                            let _ = w.update(cx, |ws, cx| {
                                ws.forfeit_open = true;
                                cx.notify();
                            });
                        })
                    },
                    cx,
                );
                Some(render_play_board_dialog(
                    PlayBoardDialogProps {
                        id: "play-cube-dialog".into(),
                        heading: SharedString::from(I18n::t(locale, "play", "cubeDecision")),
                        width_px: 520.0,
                        max_height_px: 660.0,
                        footer: None,
                        modal: true,
                        dismissible: false,
                        busy: ui_busy,
                        palette: p,
                    },
                    &self.dialog_state,
                    body,
                    None,
                    on_drag_start,
                    on_drag_move,
                    on_drag_end,
                    window,
                    cx,
                ))
            }
            PlayInterruption::Feedback => {
                let fb_cb = self.build_feedback_callbacks(cx);
                let preview_opt = has_play_move_preview(&session).then_some(&self.preview_board);
                let fb_panel = render_feedback_body(
                    &session,
                    &white_name,
                    &black_name,
                    ui_busy,
                    self.selected_candidate,
                    self.preview_mode,
                    preview_opt,
                    p,
                    fb_cb,
                    cx,
                );
                let mode_key = match session.play_mode {
                    PlayMode::Competition => "competition",
                    PlayMode::Training => "training",
                    PlayMode::Coach => "coach",
                    PlayMode::Tutor => "tutor",
                };
                let body = div()
                    .v_flex()
                    .gap(px(10.0))
                    .when_some(err_banner, |this, e| this.child(e))
                    .when_some(fb_panel, |this, panel| this.child(panel))
                    .into_any_element();
                Some(render_play_board_dialog(
                    PlayBoardDialogProps {
                        id: "play-feedback-dialog".into(),
                        heading: SharedString::from(I18n::t(locale, "play-mode", mode_key)),
                        width_px: if preview_opt.is_some() { 700.0 } else { 540.0 },
                        max_height_px: 740.0,
                        footer: None,
                        modal: true,
                        dismissible: false,
                        busy: ui_busy,
                        palette: p,
                    },
                    &self.dialog_state,
                    body,
                    None,
                    on_drag_start,
                    on_drag_move,
                    on_drag_end,
                    window,
                    cx,
                ))
            }
            PlayInterruption::GameOver | PlayInterruption::MatchOver => {
                let fb_cb = self.build_feedback_callbacks(cx);
                let fb_panel = render_feedback_body(
                    &session,
                    &white_name,
                    &black_name,
                    ui_busy,
                    self.selected_candidate,
                    self.preview_mode,
                    None,
                    p,
                    fb_cb,
                    cx,
                );
                let failed = self.session_entity.read(cx).error().is_some();
                let match_key = format!("{}:{}", session.id, session.game_no);
                let w = cx.weak_entity();
                let res_cb = PlayResultCallbacks {
                    on_next_game: {
                        let w = w.clone();
                        Rc::new(move |win, cx| {
                            let _ = w.update(cx, |ws, cx| {
                                ws.dispatch_action(
                                    PlayActionKind::NextGame,
                                    None,
                                    None,
                                    None,
                                    win,
                                    cx,
                                );
                            });
                        })
                    },
                    on_end_session: {
                        let w = w.clone();
                        Rc::new(move |win, cx| {
                            let _ = w.update(cx, |ws, cx| {
                                ws.dispatch_action(
                                    PlayActionKind::EndSession,
                                    None,
                                    None,
                                    None,
                                    win,
                                    cx,
                                );
                            });
                        })
                    },
                    on_pause: {
                        let w = w.clone();
                        Rc::new(move |win, cx| {
                            let _ = w.update(cx, |ws, cx| ws.request_pause(win, cx));
                        })
                    },
                    on_forfeit: {
                        let w = w.clone();
                        Rc::new(move |_win, cx| {
                            let _ = w.update(cx, |ws, cx| {
                                ws.forfeit_open = true;
                                cx.notify();
                            });
                        })
                    },
                    on_new_match: {
                        let w = w.clone();
                        let mk = match_key.clone();
                        Rc::new(move |win, cx| {
                            let _ = w.update(cx, |ws, cx| {
                                ws.acknowledged_match_key = Some(mk.clone());
                                ws.open_new_match_setup(win, cx);
                            });
                        })
                    },
                    on_back_to_board: {
                        let w = w.clone();
                        let mk = match_key;
                        Rc::new(move |_win, cx| {
                            let _ = w.update(cx, |ws, cx| {
                                ws.acknowledged_match_key = Some(mk.clone());
                                ws.push_stage_snapshot(cx);
                                cx.notify();
                            });
                        })
                    },
                    on_learning_review: {
                        let w = w.clone();
                        Rc::new(move |win, cx| {
                            let _ = w.update(cx, |ws, cx| ws.open_learning_review(win, cx));
                        })
                    },
                    on_open_study_report: {
                        let w = w;
                        Rc::new(move |mid, _win, cx| {
                            let _ = w.update(cx, |_ws, cx| {
                                cx.emit(WorkspaceEvent::OpenStudyReport { match_id: mid });
                            });
                        })
                    },
                };

                let heading =
                    if interruption == PlayInterruption::MatchOver && session.money_session_ended {
                        I18n::t(locale, "play", "sessionEnded")
                    } else if interruption == PlayInterruption::MatchOver {
                        I18n::t(locale, "play", "match_over")
                    } else {
                        I18n::t(locale, "play", "game_over")
                    };

                let body = render_result_dialog_body(
                    interruption,
                    &session,
                    &white_name,
                    &black_name,
                    ui_busy,
                    failed,
                    can_forfeit,
                    err_banner,
                    fb_panel,
                    p,
                    res_cb,
                    cx,
                );
                Some(render_play_board_dialog(
                    PlayBoardDialogProps {
                        id: "play-result-dialog".into(),
                        heading: SharedString::from(heading),
                        width_px: 520.0,
                        max_height_px: 640.0,
                        footer: None,
                        modal: true,
                        dismissible: false,
                        busy: ui_busy,
                        palette: p,
                    },
                    &self.dialog_state,
                    body,
                    None,
                    on_drag_start,
                    on_drag_move,
                    on_drag_end,
                    window,
                    cx,
                ))
            }
        }
    }
}

impl Render for PlayWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_count = self.render_count.saturating_add(1);
        let locale = current_locale(cx);
        let p = NeonTheme::current_palette(cx);
        let session = self.session(cx).cloned();
        let ui_busy = self.is_ui_busy(cx);
        let interruption = self.active_interruption(cx);

        let top_error_banner = if !self.setup_open
            && interruption.is_none()
            && !self.pause_warning_open
            && !self.forfeit_open
            && self.coach_decision(session.as_ref(), cx).is_none()
        {
            self.render_error_banner(p, cx)
        } else {
            None
        };

        let review_toolbar = session.as_ref().and_then(|s| {
            let w_ret = cx.weak_entity();
            let w_mode = cx.weak_entity();
            let w_retry = cx.weak_entity();
            render_play_history_review_toolbar(
                &self.review,
                s,
                p,
                Rc::new(move |_win, cx| {
                    let _ = w_ret.update(cx, |ws, cx| ws.exit_history_review(cx));
                }),
                Rc::new(move |mode, _win, cx| {
                    let _ = w_mode.update(cx, |ws, cx| {
                        ws.review.set_view_mode(mode);
                        ws.push_stage_snapshot(cx);
                        cx.notify();
                    });
                }),
                Rc::new(move |win, cx| {
                    let _ = w_retry.update(cx, |ws, cx| {
                        ws.session_entity.update(cx, |se, cx| {
                            se.retry(win, cx);
                        });
                    });
                }),
                cx,
            )
        });

        let coach_toolbar = session.as_ref().and_then(|s| {
            let dec = self.coach_decision(Some(s), cx)?;
            let err = if self.coach_details_open {
                None
            } else {
                self.render_error_banner(p, cx)
            };
            let w_cont = cx.weak_entity();
            let w_det = cx.weak_entity();
            Some(render_coach_toolbar(
                s,
                dec,
                ui_busy,
                err,
                p,
                Rc::new(move |win, cx| {
                    let _ = w_cont.update(cx, |ws, cx| ws.continue_pending_decision(win, cx));
                }),
                Rc::new(move |_win, cx| {
                    let _ = w_det.update(cx, |ws, cx| {
                        ws.coach_details_open = true;
                        ws.sync_preview_board(cx);
                        cx.notify();
                    });
                }),
                cx,
            ))
        });

        let narrow = window.viewport_size().width < px(900.0);
        let history_panel = session.as_ref().map(|s| {
            let fb = play_feedback_state(Some(s));
            let money_finished = is_money_game_finished(Some(s));
            let show_decisions =
                !fb.competition || matches!(s.phase, PlayPhase::MatchOver) || money_finished;
            let show_review_actions = matches!(s.phase, PlayPhase::MatchOver) || money_finished;
            let white = self.player_name(Some(s), 0, cx);
            let black = self.player_name(Some(s), 1, cx);
            let w = cx.weak_entity();
            render_play_history_panel(
                s,
                &white,
                &black,
                self.review.selected_index(Some(s)),
                ui_busy || fb.awaiting,
                show_decisions,
                show_review_actions,
                s.paused,
                narrow,
                p,
                PlayHistoryCallbacks {
                    on_select: {
                        let w = w.clone();
                        Rc::new(move |idx, win, cx| {
                            let _ = w.update(cx, |ws, cx| ws.select_history_index(idx, win, cx));
                        })
                    },
                    on_export_sgf: {
                        let w = w.clone();
                        Rc::new(move |win, cx| {
                            let _ = w.update(cx, |ws, cx| ws.export_sgf(win, cx));
                        })
                    },
                    on_open_learning_review: {
                        let w = w.clone();
                        Rc::new(move |win, cx| {
                            let _ = w.update(cx, |ws, cx| ws.open_learning_review(win, cx));
                        })
                    },
                    on_open_study_report: {
                        let w = w;
                        Rc::new(move |mid, _win, cx| {
                            let _ = w.update(cx, |_ws, cx| {
                                cx.emit(WorkspaceEvent::OpenStudyReport { match_id: mid });
                            });
                        })
                    },
                },
                cx,
            )
        });

        let header = match session.as_ref() {
            Some(s) => Some(self.render_match_header(s, p, cx)),
            None if !self.setup_open => Some(
                div()
                    .id("play-empty-header")
                    .test_support()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .px(px(14.0))
                    .py(px(10.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(
                        div()
                            .text_page()
                            .text_color(p.text)
                            .child(I18n::t(locale, "play", "title")),
                    )
                    .child(
                        ActionButton::new("play-empty-new-match")
                            .icon(ActionIconName::NewGame)
                            .label(I18n::t(locale, "play", "newMatch"))
                            .primary()
                            .disabled(ui_busy)
                            .palette(p)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.open_new_match_setup(window, cx);
                            })),
                    )
                    .into_any_element(),
            ),
            None => None,
        };

        let mut keys = KeyContext::default();
        if self.setup_open {
            keys.add(CTX_PLAY_SETUP);
        } else {
            keys.add(CTX_PLAY);
            keys.add(CTX_BOARD_REVIEW);
        }
        let dialog = self.render_active_dialog(p, window, cx);
        let (setup_dialog, board_dialog) = if self.setup_open {
            (dialog, None)
        } else {
            (None, dialog)
        };

        div()
            .id("play-workspace")
            .test_support()
            .relative()
            .track_focus(&self.focus_handle)
            .key_context(keys)
            .on_action(
                cx.listener(|ws, _: &StartPlayMatch, window, cx| ws.start_setup_match(window, cx)),
            )
            .on_action(cx.listener(Self::handle_primary_step))
            .on_action(cx.listener(Self::handle_offer_double))
            .on_action(cx.listener(Self::handle_request_hint))
            .on_action(cx.listener(Self::handle_undo_step))
            .on_action(cx.listener(Self::handle_return_to_live))
            .on_action(cx.listener(Self::handle_show_initial))
            .on_action(cx.listener(Self::handle_show_final))
            .v_flex()
            .flex_1()
            .size_full()
            .min_h(px(0.0))
            .p(px(12.0))
            .gap(px(10.0))
            .bg(p.bg)
            .when_some(header, |this, hdr| this.child(hdr))
            .when_some(top_error_banner, |this, err| this.child(err))
            .child(
                div()
                    .flex()
                    .when(narrow, |el| el.flex_col_reverse())
                    .when(!narrow, |el| el.flex_row())
                    .items_stretch()
                    .flex_1()
                    .min_h(px(0.0))
                    .gap(px(12.0))
                    .when_some(history_panel, |this, hist| this.child(hist))
                    .child(
                        div()
                            .id("play-table-stage-container")
                            .test_support()
                            .relative()
                            .v_flex()
                            .flex_1()
                            .min_w(px(0.0))
                            .min_h(px(0.0))
                            .gap(px(8.0))
                            .when_some(review_toolbar, |this, tb| this.child(tb))
                            .when_some(coach_toolbar, |this, tb| this.child(tb))
                            .child(self.board_stage.clone())
                            .when_some(board_dialog, |this, dlg| this.child(dlg)),
                    ),
            )
            .when_some(setup_dialog, |this, dlg| this.child(dlg))
    }
}

#[cfg(test)]
mod layout_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keybindings::{CloseModal, register_keybindings};
    use crate::theme::{NeonPaletteId, ThemeMode};
    use crate::workspaces::play::board_dialog::{clamp_dialog_center, clamp_drag_offset};
    use crate::workspaces::play::dialogs::project_resignation;
    use crate::workspaces::play::interruption::{
        double_unavailable_reason_key, is_resignation_declined, native_steps,
    };
    use crate::workspaces::play::turn::{PlayTurnEntity, synchronous_checker_turns};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AppContext, TestAppContext, WindowOptions};
    use neondojo::configuration::ErrorThresholds;
    use neondojo::domain_error::{DomainError, ErrorKind};
    use neondojo::engines::SageDifficulty;
    use neondojo::model::Position;
    use neondojo::play::{
        OpeningRoll, PlayDecision, PlayEngine, PlayHistory, PlayMove, PlayPhase, PlayStep,
    };
    use neondojo_board::{DieUsage, checker_frame};
    use neondojo_rules::backgammon::initial_board;
    use std::time::{Duration, Instant};

    pub(super) fn make_session(phase: PlayPhase, mode: PlayMode) -> PlaySession {
        let id = Uuid::new_v4();
        PlaySession {
            paused: false,
            engine_snapshots: None,
            game_results: None,
            library_match_id: Some(Uuid::new_v4()),
            learning_used: false,
            game_learning_used: false,
            money_session_ended: false,
            match_forfeited: false,
            study_report_id: None,
            study_report_version: None,
            review_game_no: None,
            review: None,
            play_mode: mode,
            tutor_threshold: 0.04,
            error_thresholds: ErrorThresholds::default(),
            training_auto_analysis: true,
            analysis_plies: 3,
            pending_decision: None,
            decisions: Vec::new(),
            id,
            version: 1,
            human_player: 0,
            human_name: "Alice".to_string(),
            match_length: 5,
            bearoff_direction: "right".to_string(),
            engine: PlayEngine {
                id: "gnubg".to_string(),
                version: "GNUbg 1.08".to_string(),
                plies: 1,
                top_two: false,
                difficulty: None,
            },
            position: Position {
                cube_enabled: Some(true),
                id: Uuid::new_v4(),
                match_uuid: id,
                ordinal: 0,
                game_no: 1,
                ply: 1,
                player: 0,
                dice: vec![3, 1],
                board: initial_board().to_vec(),
                position_id: "4HPwATDgc/ABMA".to_string(),
                gnubg_match_id: "MAAAAAAAAAAA".to_string(),
                notation: String::new(),
            },
            phase,
            can_double: true,
            legal_moves: vec![
                PlayMove {
                    notation: "8/5 6/5".to_string(),
                    steps: vec![PlayStep { from: 8, to: 5 }, PlayStep { from: 6, to: 5 }],
                },
                PlayMove {
                    notation: "13/10 24/23".to_string(),
                    steps: vec![PlayStep { from: 13, to: 10 }, PlayStep { from: 24, to: 23 }],
                },
            ],
            legal_turns: None,
            history: vec![PlayHistory {
                player: 1,
                dice: vec![6, 5],
                notation: "24/13".to_string(),
            }],
            scores: [0, 0],
            game_no: 1,
            winner: None,
            opening: None,
        }
    }

    fn make_pending_decision(action: &str, warning: bool, severity: &str) -> PlayDecision {
        PlayDecision {
            first_rating_decisions: None,
            rating_decisions: None,
            game_no: 1,
            history_index: 1,
            id: Uuid::new_v4(),
            action: Some(action.to_string()),
            move_index: Some(0),
            original_action: Some(action.to_string()),
            original_move_index: Some(0),
            original_notation: Some("8/5 6/5".to_string()),
            notation: Some("8/5 6/5".to_string()),
            warning,
            revealed: true,
            assisted: false,
            retry_count: 0,
            first_loss: Some(0.05),
            loss: Some(0.05),
            first_severity: Some(severity.to_string()),
            severity: Some(severity.to_string()),
            alternatives: Vec::new(),
            attempts: Vec::new(),
            committed: false,
        }
    }

    #[test]
    fn play_interruption_enforces_exact_eight_level_priority_stack() {
        let mut session = make_session(PlayPhase::DoubleResponse, PlayMode::Tutor);
        session.pending_decision = Some(make_pending_decision("take", true, "bad"));

        // 1. blocked overrides everything
        assert_eq!(
            resolve_play_interruption(PlayInterruptionInput {
                session: Some(&session),
                blocked: true,
                resign_open: true,
                hint_open: true,
                coaching: true,
                acknowledged: false,
            }),
            None
        );

        // 2. resign beats hint, coaching, cube, feedback
        assert_eq!(
            resolve_play_interruption(PlayInterruptionInput {
                session: Some(&session),
                blocked: false,
                resign_open: true,
                hint_open: true,
                coaching: true,
                acknowledged: false,
            }),
            Some(PlayInterruption::Resign)
        );

        // 3. hint beats coaching/cube when NOT competition
        assert_eq!(
            resolve_play_interruption(PlayInterruptionInput {
                session: Some(&session),
                blocked: false,
                resign_open: false,
                hint_open: true,
                coaching: true,
                acknowledged: false,
            }),
            Some(PlayInterruption::Hint)
        );

        // Competition suppresses hint and falls through to cube
        let mut comp_session = session.clone();
        comp_session.play_mode = PlayMode::Competition;
        assert_eq!(
            resolve_play_interruption(PlayInterruptionInput {
                session: Some(&comp_session),
                blocked: false,
                resign_open: false,
                hint_open: true,
                coaching: false,
                acknowledged: false,
            }),
            Some(PlayInterruption::Cube)
        );

        // 4. inline coaching suppresses cube/feedback dialogs
        assert_eq!(
            resolve_play_interruption(PlayInterruptionInput {
                session: Some(&session),
                blocked: false,
                resign_open: false,
                hint_open: false,
                coaching: true,
                acknowledged: false,
            }),
            None
        );

        // 5. cube beats awaiting feedback
        assert_eq!(
            resolve_play_interruption(PlayInterruptionInput {
                session: Some(&session),
                blocked: false,
                resign_open: false,
                hint_open: false,
                coaching: false,
                acknowledged: false,
            }),
            Some(PlayInterruption::Cube)
        );

        // 6. feedback beats game_over
        session.phase = PlayPhase::GameOver;
        assert_eq!(
            resolve_play_interruption(PlayInterruptionInput {
                session: Some(&session),
                blocked: false,
                resign_open: false,
                hint_open: false,
                coaching: false,
                acknowledged: false,
            }),
            Some(PlayInterruption::Feedback)
        );

        // Routine roll without warning in Training/Coach is NOT awaiting feedback -> GameOver
        session.play_mode = PlayMode::Training;
        session.pending_decision = Some(make_pending_decision("roll", false, "none"));
        assert_eq!(
            resolve_play_interruption(PlayInterruptionInput {
                session: Some(&session),
                blocked: false,
                resign_open: false,
                hint_open: false,
                coaching: false,
                acknowledged: false,
            }),
            Some(PlayInterruption::GameOver)
        );

        // 7 & 8. Money game unfinished returns GameOver until money_session_ended, then MatchOver until acknowledged
        session.pending_decision = None;
        session.match_length = 0;
        session.phase = PlayPhase::MatchOver;
        session.money_session_ended = false;
        assert_eq!(
            resolve_play_interruption(PlayInterruptionInput {
                session: Some(&session),
                blocked: false,
                resign_open: false,
                hint_open: false,
                coaching: false,
                acknowledged: false,
            }),
            Some(PlayInterruption::GameOver)
        );
        session.money_session_ended = true;
        assert_eq!(
            resolve_play_interruption(PlayInterruptionInput {
                session: Some(&session),
                blocked: false,
                resign_open: false,
                hint_open: false,
                coaching: false,
                acknowledged: false,
            }),
            Some(PlayInterruption::MatchOver)
        );
        assert_eq!(
            resolve_play_interruption(PlayInterruptionInput {
                session: Some(&session),
                blocked: false,
                resign_open: false,
                hint_open: false,
                coaching: false,
                acknowledged: true,
            }),
            None
        );
    }

    #[test]
    fn play_session_single_flight_lock_rejects_concurrent_mutations_and_loads() {
        let mut ent = PlaySessionEntity::new(None);
        assert!(ent.can_start_mutation());
        assert!(ent.can_start_load());
        assert!(ent.acquire_test_mutation_lock());

        // While a non-abortable mutation is in flight, both concurrent mutations AND loads are rejected.
        assert!(ent.is_locked());
        assert!(ent.is_mutation_in_flight());
        assert!(!ent.can_start_mutation());
        assert!(!ent.can_start_load());
        assert!(!ent.acquire_test_mutation_lock());

        ent.release_test_mutation_lock();
        assert!(!ent.is_locked());
        assert!(ent.can_start_mutation());
        assert!(ent.can_start_load());
    }

    #[test]
    fn play_turn_synchronous_legality_swap_undo_doubles_and_forced_pass() {
        let board = initial_board();
        let mut turn = PlayTurnEntity::new();
        turn.load_local_position(&board, 0, [3, 1]);

        assert_eq!(turn.dice(), vec![3, 1]);
        assert!(!turn.forced_pass(true));
        assert!(turn.can_swap(true, false));

        // Hovering point 8 previews destination 5 (preferred first die = 3).
        turn.set_hover_local(Some(8));
        assert_eq!(turn.destinations(true), vec![5]);

        // Swapping dice changes preferred first die to 1 (point 8 -> 7).
        assert!(turn.swap_dice_local(true, false, false));
        assert_eq!(turn.dice(), vec![1, 3]);
        turn.set_hover_local(Some(8));
        assert_eq!(turn.destinations(true), vec![7]);

        // Swap back and play 8/5 then 6/5 -> complete turn.
        assert!(turn.swap_dice_local(true, false, false));
        let frame1 = turn
            .click_point_local(8, &board, 0, true, false)
            .expect("8/5 step");
        assert_eq!(frame1.motions[0].from, 8);
        assert_eq!(frame1.motions[0].to, 5);
        assert_eq!(frame1.sound, Some(PlaySoundKind::Move));
        assert!(!turn.can_swap(true, false));
        assert_eq!(turn.used_dice(), [DieUsage::Used, DieUsage::Unused]);

        let frame2 = turn
            .click_point_local(6, &board, 0, true, false)
            .expect("6/5 step");
        assert_eq!(frame2.motions[0].from, 6);
        assert_eq!(frame2.motions[0].to, 5);
        assert_eq!(turn.used_dice(), [DieUsage::Used, DieUsage::Used]);
        assert!(turn.completed_move_index(true).is_some());

        // Z undo returns a SILENT reverse frame (`sound: None`).
        let undone = turn
            .undo_local(&board, 0, false, false, false)
            .expect("undo step");
        assert_eq!(undone.sound, None);
        assert_eq!(undone.motions[0].from, 5);
        assert_eq!(undone.motions[0].to, 6);
        assert_eq!(turn.steps().len(), 1);
        assert!(turn.completed_move_index(true).is_none());

        // Doubles half-used die mask (`Unused` -> `Half` -> `Used` / `Unused` -> `Half` -> `Used`).
        let mut dbl = PlayTurnEntity::new();
        dbl.load_local_position(&board, 0, [6, 6]);
        assert!(!dbl.can_swap(true, false));
        assert_eq!(dbl.used_dice(), [DieUsage::Unused, DieUsage::Unused]);
        assert!(dbl.click_point_local(13, &board, 0, true, false).is_some());
        assert_eq!(dbl.used_dice(), [DieUsage::Half, DieUsage::Unused]);
        assert!(dbl.click_point_local(13, &board, 0, true, false).is_some());
        assert_eq!(dbl.used_dice(), [DieUsage::Used, DieUsage::Unused]);
        assert!(dbl.click_point_local(24, &board, 0, true, false).is_some());
        assert_eq!(dbl.used_dice(), [DieUsage::Used, DieUsage::Half]);
        assert!(dbl.click_point_local(24, &board, 0, true, false).is_some());
        assert_eq!(dbl.used_dice(), [DieUsage::Used, DieUsage::Used]);
        assert!(dbl.completed_move_index(true).is_some());

        // Forced pass: White on bar (point 25) against a closed Black home board (points 19..=24).
        let mut closed = [0i32; 26];
        closed[25] = 1;
        closed[6] = 14;
        closed[19..=24].fill(-2);
        closed[1] = -3;
        let blocked_turns = synchronous_checker_turns(&closed, 0, [4, 2]);
        assert!(blocked_turns.is_empty());
        let mut pass_turn = PlayTurnEntity::new();
        pass_turn.load_local_position(&closed, 0, [4, 2]);
        assert!(pass_turn.forced_pass(true));
        assert!(pass_turn.sources(true).is_empty());
    }

    #[test]
    fn setup_choices_enforce_preset_and_top_two_mutual_exclusion() {
        let mut choices = PlaySetupChoices::default();
        choices.select_engine("bgsage");
        choices.select_top_two();
        assert!(choices.bot_top_two);
        assert_eq!(choices.bot_plies, 2);
        assert_eq!(choices.bot_difficulty, None);
        let req1 = choices.to_create_request();
        assert_eq!(req1.play_engine.as_deref(), Some("bgsage"));
        assert_eq!(req1.play_plies, Some(2));
        assert_eq!(req1.play_top_two, Some(true));
        assert_eq!(req1.play_difficulty, None);

        // Selecting a named difficulty clears `top_two` and `play_plies`.
        choices.select_difficulty(SageDifficulty::Intermediate);
        assert!(!choices.bot_top_two);
        assert_eq!(choices.bot_difficulty, Some(SageDifficulty::Intermediate));
        let req2 = choices.to_create_request();
        assert_eq!(req2.play_engine.as_deref(), Some("bgsage"));
        assert_eq!(req2.play_plies, None);
        assert_eq!(req2.play_top_two, None);
        assert_eq!(req2.play_difficulty, Some(SageDifficulty::Intermediate));

        // Selecting 2-ply + Top-2 clears the named difficulty preset.
        choices.select_top_two();
        assert!(choices.bot_top_two);
        assert_eq!(choices.bot_difficulty, None);
    }

    #[test]
    fn resignation_projection_decline_detection_and_dialog_clamping() {
        let proj = project_resignation(ResignLevel::Gammon, 2, [2, 1], 0, 5);
        assert_eq!(proj.points, 4);
        assert_eq!(proj.projected_scores, [2, 5]);
        assert!(proj.ends_match);

        let money_proj = project_resignation(ResignLevel::Backgammon, 2, [0, 4], 0, 0);
        assert_eq!(money_proj.points, 6);
        assert_eq!(money_proj.projected_scores, [0, 10]);
        assert!(!money_proj.ends_match);

        let declined = DomainError::new(
            ErrorKind::UnprocessableEntity,
            "The bot declined this resignation.",
        );
        assert!(is_resignation_declined(&declined));
        let gnubg_declined = DomainError::new(
            ErrorKind::UnprocessableEntity,
            "GNUbg declined this resignation. Play on.",
        );
        assert!(is_resignation_declined(&gnubg_declined));
        let other = DomainError::new(ErrorKind::Conflict, "Stale version");
        assert!(!is_resignation_declined(&other));

        let clamped = clamp_dialog_center((2000.0, -100.0), (500.0, 400.0), (1280.0, 800.0), 16.0);
        assert_eq!(clamped, (1280.0 - 250.0 - 16.0, 200.0 + 16.0));
        let offset = clamp_drag_offset((999.0, -999.0), (500.0, 400.0), (1280.0, 800.0), 16.0);
        assert_eq!(offset, (374.0, -184.0));
    }

    #[test]
    fn action_frames_native_steps_and_new_cube_offer_detection() {
        assert_eq!(native_steps("bar/22 6/5", 0), vec![(25, 22), (6, 5)]);
        assert_eq!(native_steps("3/off 1/off", 1), vec![(22, 26), (24, 26)]);

        let before = make_session(PlayPhase::OpeningRoll, PlayMode::Coach);
        let mut after = before.clone();
        after.version = 2;
        after.phase = PlayPhase::OpeningResult;
        after.opening = Some(OpeningRoll {
            white_die: 5,
            black_die: 2,
            winner: 0,
            rolls: vec![[3, 3], [5, 2]],
        });
        let frames = action_frames(&before, &after, PlayActionKind::OpeningRoll);
        assert_eq!(frames.len(), 2);
        assert!(matches!(&frames[0], PlayFrame::Dice(d) if d.opening && d.tied));
        assert!(matches!(&frames[1], PlayFrame::Dice(d) if d.opening && !d.tied && d.player == 0));

        let mut cube_after = before.clone();
        cube_after.version = 2;
        cube_after.phase = PlayPhase::DoubleResponse;
        cube_after.history.push(PlayHistory {
            player: 1,
            dice: Vec::new(),
            notation: "double".to_string(),
        });
        assert!(is_new_cube_offer(&before, &cube_after));
        assert!(!is_new_cube_offer(&cube_after, &cube_after));

        // Double unavailable reason covers match-covered cube.
        let mut covered = make_session(PlayPhase::Roll, PlayMode::Coach);
        covered.match_length = 5;
        covered.scores = [4, 0];
        assert_eq!(
            double_unavailable_reason_key(Some(&covered)),
            "doubleMatchCovered"
        );
    }

    #[gpui_kit::test]
    async fn empty_state_preserves_header_tab_focus_exception(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            NeonTheme::init(NeonPaletteId::Graphite, ThemeMode::Light, false, cx);
            register_keybindings(cx);
            cx.set_global(LocalUiState::default());
        });

        struct Harness {
            header_focus: FocusHandle,
            play: Entity<PlayWorkspace>,
        }
        impl Render for Harness {
            fn render(
                &mut self,
                _window: &mut Window,
                _cx: &mut Context<Self>,
            ) -> impl IntoElement {
                div()
                    .size_full()
                    .v_flex()
                    .child(
                        div()
                            .id("header-tab-play")
                            .test_support()
                            .track_focus(&self.header_focus)
                            .child("Playing"),
                    )
                    .child(self.play.clone())
            }
        }

        let (window, harness) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    let header_focus = cx.focus_handle();
                    header_focus.focus(window, cx);
                    let play = cx.new(|cx| PlayWorkspace::new_for_test(window, cx));
                    cx.new(|_| Harness { header_focus, play })
                })
            })
            .expect("open empty-state harness window");

        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let h = harness.read(cx);
            assert!(
                h.play.read(cx).is_setup_open(),
                "empty Play should display PlaySetupDialog"
            );
            assert!(
                h.header_focus.is_focused(window),
                "Play empty-state setup dialog MUST NOT steal focus from header tab!"
            );
            assert!(window.find("play-setup-dialog").visible());
            assert!(
                !h.play
                    .read(cx)
                    .board_stage()
                    .read(cx)
                    .board_view()
                    .read(cx)
                    .show_player_bars(),
                "idle Play table behind New Match dialog must hide player score bars"
            );

            // Explicit user-triggered setup (e.g. clicking New Match) DOES focus the dialog.
            let play = h.play.clone();
            play.update(cx, |ws, cx| {
                ws.open_new_match_setup(window, cx);
            });
            window.render_frame(cx);
            assert!(
                !harness.read(cx).header_focus.is_focused(window),
                "explicit New Match setup should focus the dialog"
            );
        })
        .expect("verify empty-state focus exception");
    }

    #[gpui_kit::test]
    async fn modal_dialog_enforces_dismissible_vs_non_dismissible_escape_and_renders_live_session(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            NeonTheme::init(NeonPaletteId::Club, ThemeMode::Dark, true, cx);
            register_keybindings(cx);
            cx.set_global(LocalUiState::default());
        });

        let (window, play) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    cx.new(|cx| PlayWorkspace::new_for_test(window, cx))
                })
            })
            .expect("open play workspace window");

        // Load a live DoubleResponse session -> non-dismissible Cube modal!
        let cube_session = make_session(PlayPhase::DoubleResponse, PlayMode::Tutor);
        cx.update(|cx| {
            let session_ent = play.read(cx).session_entity().clone();
            session_ent.update(cx, |se, cx| {
                se.set_session_for_test(cube_session, false, cx);
            });
        });
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                play.read(cx).active_interruption(cx),
                Some(PlayInterruption::Cube)
            );
            assert!(window.find("play-cube-dialog").visible());
            assert!(
                window.try_find("play-cube-dialog-close").is_none(),
                "non-dismissible Cube dialog must not expose a Close button"
            );
            // Dispatch CloseModal (Escape) -> non-dismissible dialog MUST remain open!
            window.dispatch_action(Box::new(CloseModal), cx);
        })
        .expect("dispatch escape on cube dialog");
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                play.read(cx).active_interruption(cx),
                Some(PlayInterruption::Cube)
            );
            assert!(window.find("play-cube-dialog").visible());
        })
        .expect("verify non-dismissible cube dialog stays open");

        // Switch to Move phase and open dismissible Resign dialog -> Escape closes it cleanly.
        let move_session = make_session(PlayPhase::Move, PlayMode::Coach);
        cx.update(|cx| {
            let session_ent = play.read(cx).session_entity().clone();
            session_ent.update(cx, |se, cx| {
                se.set_session_for_test(move_session, false, cx);
            });
        });
        cx.update_window(window, |_, window, cx| {
            play.update(cx, |ws, cx| {
                ws.resign_open = true;
                ws.push_stage_snapshot(cx);
                cx.notify();
            });
            window.render_frame(cx);
            assert_eq!(
                play.read(cx).active_interruption(cx),
                Some(PlayInterruption::Resign)
            );
            assert!(window.find("play-resign-dialog").visible());
            window.dispatch_action(Box::new(CloseModal), cx);
        })
        .expect("dispatch escape on resign dialog");
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(play.read(cx).active_interruption(cx), None);
            assert!(window.try_find("play-resign-dialog").is_none());
            assert!(window.find("play-board-toolbar").visible());
            assert!(window.find("play-history-panel").visible());
        })
        .expect("verify dismissible resign dialog closed");
    }

    #[gpui_kit::test]
    async fn history_review_and_mode_edit_preserve_uncommitted_steps_and_animation_isolates_renders(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            NeonTheme::init(NeonPaletteId::Paper, ThemeMode::Light, false, cx);
            register_keybindings(cx);
            cx.set_global(LocalUiState::default());
        });

        let (window, play) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    cx.new(|cx| PlayWorkspace::new_for_test(window, cx))
                })
            })
            .expect("open play review/animation window");

        let move_session = make_session(PlayPhase::Move, PlayMode::Coach);
        let base_board = board_array(&move_session.position.board);

        // First flush session acceptance into PlayWorkspace -> TurnEntity.
        cx.update(|cx| {
            let session_ent = play.read(cx).session_entity().clone();
            session_ent.update(cx, |se, cx| {
                se.set_session_for_test(move_session.clone(), false, cx);
            });
        });

        // Stage 1 uncommitted step (`8/5`) synchronously.
        cx.update_window(window, |_, window, cx| {
            let turn_ent = play.read(cx).turn_entity().clone();
            turn_ent.update(cx, |t, cx| {
                let frame = t.click_point(8, &base_board, 0, true, false, cx);
                assert!(frame.is_some());
            });
            window.render_frame(cx);
        })
        .expect("stage local checker step");

        // Live mode edit (`preserve_steps = true`) MUST preserve staged steps!
        let mut tutor_session = move_session;
        tutor_session.version += 1;
        tutor_session.play_mode = PlayMode::Tutor;
        cx.update(|cx| {
            let session_ent = play.read(cx).session_entity().clone();
            session_ent.update(cx, |se, cx| {
                se.set_session_for_test(tutor_session, true, cx);
            });
        });
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                play.read(cx).turn_entity().read(cx).steps().len(),
                1,
                "live PlayMode update with preserve_steps=true must retain uncommitted steps"
            );

            // Enter history review and return to live board -> uncommitted step survives!
            play.update(cx, |ws, cx| {
                ws.select_history_index(0, window, cx);
            });
            window.render_frame(cx);
            assert!(window.find("play-history-review-toolbar").visible());
            play.update(cx, |ws, cx| {
                ws.exit_history_review(cx);
            });
            window.render_frame(cx);
            assert!(window.try_find("play-history-review-toolbar").is_none());
            assert_eq!(
                play.read(cx).turn_entity().read(cx).steps().len(),
                1,
                "returning from history review must preserve uncommitted local checker steps"
            );
        })
        .expect("verify history review and preserve_steps");

        // Verify animation isolation: PlayAnimationEntity 60fps ticks MUST notify ONLY
        // PlayBoardStage, NEVER PlayWorkspace, until Settled (0% idle GPU after end)!
        use std::sync::atomic::{AtomicUsize, Ordering};
        let ws_notifies = Arc::new(AtomicUsize::new(0));
        let stage_notifies = Arc::new(AtomicUsize::new(0));
        let (anim, stage_ent) = cx.update(|cx| {
            let ws_flag = Arc::clone(&ws_notifies);
            let stage_flag = Arc::clone(&stage_notifies);
            let stage_ent = play.read(cx).board_stage().clone();
            let anim = play.read(cx).animation_entity().clone();
            cx.observe(&play, move |_, _| {
                ws_flag.fetch_add(1, Ordering::SeqCst);
            })
            .detach();
            cx.observe(&stage_ent, move |_, _| {
                stage_flag.fetch_add(1, Ordering::SeqCst);
            })
            .detach();

            let f = checker_frame(&base_board, 6, 5, 0, vec![3, 1]);
            anim.update(cx, |a, cx| {
                a.enqueue(vec![PlayFrame::Checker(f)], None, cx);
            });
            (anim, stage_ent)
        });
        let _ = stage_ent;
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
        })
        .expect("render initial animation frame");

        ws_notifies.store(0, Ordering::SeqCst);
        stage_notifies.store(0, Ordering::SeqCst);

        let t0 = Instant::now();
        for step in 1..=5 {
            cx.update(|cx| {
                anim.update(cx, |a, cx| {
                    assert!(a.sample_at(t0 + Duration::from_millis(step * 16), cx));
                });
            });
        }
        assert_eq!(
            ws_notifies.load(Ordering::SeqCst),
            0,
            "intermediate 60fps PlayAnimationEntity ticks MUST NOT notify/re-render PlayWorkspace"
        );
        assert_eq!(
            stage_notifies.load(Ordering::SeqCst),
            5,
            "PlayBoardStage must observe every PlayAnimationEntity frame tick"
        );

        // Advancing past completion settles to 0% idle GPU (`!is_animating()`) and wakes PlayWorkspace once.
        cx.update(|cx| {
            anim.update(cx, |a, cx| {
                assert!(!a.sample_at(t0 + Duration::from_millis(1_000), cx));
                assert!(!a.is_animating());
            });
        });
        assert!(
            ws_notifies.load(Ordering::SeqCst) >= 1,
            "PlayWorkspace should wake once when PlayAnimationEntity emits Settled"
        );

        // Silencing audio on mute toggle & set_visible(false) increments audio generation & cancels animation.
        cx.update(|cx| {
            let gen0 = play.read(cx).animation_entity().read(cx).audio_generation();
            play.update(cx, |ws, cx| ws.toggle_mute(cx));
            let gen1 = {
                let anim_ref = play.read(cx).animation_entity().read(cx);
                assert!(!anim_ref.is_sounds_enabled());
                assert!(anim_ref.audio_generation() > gen0);
                anim_ref.audio_generation()
            };

            play.update(cx, |ws, cx| ws.set_visible(false, cx));
            let anim_ref = play.read(cx).animation_entity().read(cx);
            assert!(!anim_ref.is_animating());
            assert!(anim_ref.audio_generation() > gen1);
        });
    }

    #[gpui_kit::test]
    async fn setup_settings_button_emits_open_settings_and_coach_toolbar_renders_inline(
        cx: &mut TestAppContext,
    ) {
        use std::sync::atomic::{AtomicBool, Ordering};

        cx.update(|cx| {
            gpui_kit::init(cx);
            NeonTheme::init(NeonPaletteId::Graphite, ThemeMode::Light, false, cx);
            register_keybindings(cx);
            cx.set_global(LocalUiState::default());
        });

        let opened_play_settings = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&opened_play_settings);

        let (window, play) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    let ws = cx.new(|cx| PlayWorkspace::new_for_test(window, cx));
                    cx.subscribe(&ws, move |_, event: &WorkspaceEvent, _| {
                        if matches!(
                            event,
                            WorkspaceEvent::OpenSettings {
                                category: SettingsCategory::Play
                            }
                        ) {
                            flag.store(true, Ordering::SeqCst);
                        }
                    })
                    .detach();
                    ws
                })
            })
            .expect("open settings/coach test window");

        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("play-setup-open-settings").visible());
            window.click("play-setup-open-settings", cx);
        })
        .expect("click play settings button");

        assert!(
            opened_play_settings.load(Ordering::SeqCst),
            "clicking settings gear in PlaySetupDialog must emit WorkspaceEvent::OpenSettings {{ category: SettingsCategory::Play }}"
        );

        // Load a Coach session with a pending move decision -> inline PlayCoachToolbar renders without modal interruption.
        let mut coach_session = make_session(PlayPhase::Move, PlayMode::Coach);
        coach_session.pending_decision = Some(make_pending_decision("move", false, "bad"));
        cx.update(|cx| {
            let session_ent = play.read(cx).session_entity().clone();
            session_ent.update(cx, |se, cx| {
                se.set_session_for_test(coach_session, false, cx);
            });
        });
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(play.read(cx).active_interruption(cx), None);
            assert!(window.find("play-coach-toolbar").visible());
            assert!(window.try_find("play-feedback-dialog").is_none());
        })
        .expect("verify inline coach toolbar renders without modal interruption");
    }
}
