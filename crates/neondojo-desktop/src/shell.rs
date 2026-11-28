//! NeonDojo top-level GPUI desktop application shell (`AppShell`).
//!
//! Mirrors `web/src/app/App.tsx`:
//! - Single-row header: brand mark -> Home, 7 workspace tabs in canonical order,
//!   Help dropdown (`NeonDojo manual`, `NeonGammon manual`), `ProfileSelector`,
//!   and compact Settings gear button (`Cmd+,`).
//! - Finder-safe async backend startup (`CoreRuntime::start`) with native
//!   PostgreSQL connection retry & configuration banner/dialog (never panics).
//! - Live `neondojo-i18n` localization across header, menus, home, footer, and
//!   re-translatable `AppNotice` / `AppError` banners.
//! - Complete Settings & Profiles dialog (`src/settings/`) with 3-destination
//!   dirty tracking and `SettingsDraftGuardDialog`.
//! - Lazy `Option<Entity<...Workspace>>` workspace instances preserving state
//!   across tab switches, full [`crate::events::WorkspaceEvent`] cross-workspace
//!   routing, plus an `AppResetEvent` hook for backup restores.

use crate::app_context::{BackendConnectionState, DesktopAppContext};
use crate::events::WorkspaceEvent;
use crate::i18n::{current_locale, format_error};
use crate::keybindings::{
    CTX_APP, GoHome, ImportMatchFiles, ImportMatchFolder, ManageProfiles, MenuState, NewPlayMatch,
    OpenAbout, OpenDatabaseBackup, OpenNeonGammonManual, OpenSettings, OpenUserGuide,
    SelectWorkspaceAnalysis, SelectWorkspaceExtras, SelectWorkspaceLibrary, SelectWorkspacePlay,
    SelectWorkspacePlayers, SelectWorkspacePositions, SelectWorkspaceStudy, SetModeDark,
    SetModeLight, SetModeSystem, SetPaletteClub, SetPaletteGraphite, SetPalettePaper,
    ShowProfileChanges, install_app_menus,
};
use crate::local_state::LocalUiState;
use crate::settings::draft_guard::SettingsCategory;
use crate::settings::{AppliedSettingsEvent, SettingsDialogView, open_settings_dialog};
use crate::theme::{
    NeonPalette, NeonPaletteId, NeonTheme, NeonTypographyExt, ResolvedMode, ThemeMode,
};
use crate::tokio_bridge::TokioBridge;
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::banners::{BannerKind, EngineHealth, EngineStatusDot, StatusBanner};
use crate::ui::drop_zone::{DroppedMatchBatch, MatchDropZone};
use crate::ui::icons::{ActionIconName, brand_logo};
use crate::ui::modal::{ModalBusyLock, ModalConfig, ModalSize, open_modal_dialog};
use crate::ui::text_nav::{TextNavBar, TextNavDensity, TextNavItem};
use crate::workspaces::analysis::AnalysisWorkspace;
use crate::workspaces::extras::ExtrasWorkspace;
use crate::workspaces::library::LibraryWorkspace;
use crate::workspaces::play::PlayWorkspace;
use crate::workspaces::players::PlayersWorkspace;
use crate::workspaces::positions::PositionsWorkspace;
use crate::workspaces::study::StudyWorkspace;
use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable,
    FontWeight, InteractiveElement, IntoElement, ParentElement, PathPromptOptions, Render,
    SharedString, StatefulInteractiveElement, Styled, TestSupportExt, Window,
    base::StyledExt,
    component::WindowExt,
    component::input::{Input, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::api::{VersionInfo, version_info};
use neondojo::runtime::CoreRuntime;
use neondojo_i18n::{AppError, I18n, Locale, Param};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// External documentation links surfaced in the Help menu.
pub const USER_GUIDE_URL: &str = "https://neondojo.org/docs/user-guide";
pub const NEONGAMMON_MANUAL_URL: &str = "https://neondojo.org/docs/neongammon-manual";

type BootSnapshot = (
    Arc<CoreRuntime>,
    neondojo::facade::BackendFacade,
    Option<neondojo::configuration::ConfigurationState>,
    Option<Vec<neondojo::settings::EngineInfo>>,
);

async fn boot_core_and_snapshot(
    core_config: neondojo::runtime::CoreConfig,
) -> anyhow::Result<BootSnapshot> {
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        handle.block_on(async move {
            let runtime = CoreRuntime::start(core_config).await?;
            let facade = runtime.facade();
            let config_state = facade.get_configuration().await.ok();
            let engines = facade.list_engines().await.ok();
            Ok((Arc::new(runtime), facade, config_state, engines))
        })
    })
    .await?
}

/// Canonical top-level workspaces matching `App.tsx` header tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkspaceId {
    Play,
    Analysis,
    Study,
    Positions,
    Library,
    Players,
    Extras,
}

impl WorkspaceId {
    /// Ordered list matching the exact header navigation order in `App.tsx`.
    pub const ALL: &'static [Self] = &[
        Self::Play,
        Self::Analysis,
        Self::Study,
        Self::Positions,
        Self::Library,
        Self::Players,
        Self::Extras,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Play => "play",
            Self::Analysis => "analysis",
            Self::Study => "study",
            Self::Positions => "positions",
            Self::Library => "library",
            Self::Players => "players",
            Self::Extras => "extras",
        }
    }

    pub const fn tab_element_id(self) -> &'static str {
        match self {
            Self::Play => "workspace-tab-play",
            Self::Analysis => "workspace-tab-analysis",
            Self::Study => "workspace-tab-study",
            Self::Positions => "workspace-tab-positions",
            Self::Library => "workspace-tab-library",
            Self::Players => "workspace-tab-players",
            Self::Extras => "workspace-tab-extras",
        }
    }

    /// Translation key inside the `"workspace"` message group.
    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::Play => "play",
            Self::Analysis => "analysis",
            Self::Study => "learning",
            Self::Positions => "positions",
            Self::Library => "database",
            Self::Players => "players",
            Self::Extras => "extras",
        }
    }

    /// Localized workspace name from `neondojo-i18n`.
    pub fn localized_label(self, locale: Locale) -> &'static str {
        I18n::t(locale, "workspace", self.i18n_key())
    }

    /// English default label.
    pub fn label(self) -> &'static str {
        self.localized_label(Locale::En)
    }

    pub const fn shortcut_label(self) -> &'static str {
        match self {
            Self::Play => "Cmd+1",
            Self::Analysis => "Cmd+2",
            Self::Study => "Cmd+3",
            Self::Positions => "Cmd+4",
            Self::Library => "Cmd+5",
            Self::Players => "Cmd+6",
            Self::Extras => "Cmd+7",
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::Play => 0,
            Self::Analysis => 1,
            Self::Study => 2,
            Self::Positions => 3,
            Self::Library => 4,
            Self::Players => 5,
            Self::Extras => 6,
        }
    }

    pub const fn icon(self) -> ActionIconName {
        match self {
            Self::Play => ActionIconName::Play,
            Self::Analysis => ActionIconName::Analysis,
            Self::Study => ActionIconName::Cards,
            Self::Positions => ActionIconName::Bookmark,
            Self::Library => ActionIconName::Database,
            Self::Players => ActionIconName::View,
            Self::Extras => ActionIconName::Dice,
        }
    }

    pub const fn key_context(self) -> Option<&'static str> {
        match self {
            Self::Play => Some(crate::keybindings::CTX_PLAY),
            Self::Analysis => Some(crate::keybindings::CTX_ANALYSIS),
            Self::Study => Some(crate::keybindings::CTX_STUDY_EXERCISE),
            Self::Positions | Self::Library | Self::Players | Self::Extras => None,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|ws| ws.id() == id)
    }
}

/// Active view inside [`AppShell`]: either the Home landing screen or a workspace.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppView {
    #[default]
    Home,
    Workspace(WorkspaceId),
}

/// Event emitted when the entire desktop shell resets workspace entities
/// (e.g. after restoring a database backup).
#[derive(Debug, Clone)]
pub struct AppResetEvent {
    pub generation: u64,
    pub reason: SharedString,
}

/// Text payload for an [`AppNotice`] supporting live re-translation on language switch.
#[derive(Debug, Clone)]
pub enum AppNoticeText {
    Literal(SharedString),
    Localized {
        group: &'static str,
        key: &'static str,
    },
    Error(AppError),
}

impl AppNoticeText {
    pub fn resolve(&self, cx: &App) -> SharedString {
        let locale = current_locale(cx);
        match self {
            Self::Literal(text) => text.clone(),
            Self::Localized { group, key } => SharedString::from(I18n::t(locale, group, key)),
            Self::Error(err) => SharedString::from(format_error(cx, err)),
        }
    }
}

impl From<SharedString> for AppNoticeText {
    fn from(value: SharedString) -> Self {
        Self::Literal(value)
    }
}

impl From<String> for AppNoticeText {
    fn from(value: String) -> Self {
        Self::Literal(SharedString::from(value))
    }
}

impl From<&'static str> for AppNoticeText {
    fn from(value: &'static str) -> Self {
        Self::Literal(SharedString::from(value))
    }
}

impl From<AppError> for AppNoticeText {
    fn from(value: AppError) -> Self {
        Self::Error(value)
    }
}

/// Global shell notice rendered in the `AppNotices` strip above workspace content.
#[derive(Debug, Clone)]
pub struct AppNotice {
    pub id: SharedString,
    pub kind: BannerKind,
    pub title: Option<AppNoticeText>,
    pub message: AppNoticeText,
    pub dismissible: bool,
}

/// Profile summary displayed by `ProfileSelector` in the app header.
#[derive(Debug, Clone)]
pub struct ProfileHeaderSummary {
    pub name: SharedString,
    pub modified: bool,
    pub warning_count: usize,
}

impl Default for ProfileHeaderSummary {
    fn default() -> Self {
        Self {
            name: SharedString::from("Standard"),
            modified: false,
            warning_count: 0,
        }
    }
}

/// Engine health summary displayed in [`AppShell`]'s footer bar.
#[derive(Debug, Clone)]
pub struct FooterEngineStatus {
    pub gnubg_health: EngineHealth,
    pub neongammon_health: EngineHealth,
}

impl Default for FooterEngineStatus {
    fn default() -> Self {
        Self {
            gnubg_health: EngineHealth::Ready,
            neongammon_health: EngineHealth::Ready,
        }
    }
}

impl FooterEngineStatus {
    fn engine_label(locale: Locale, engine_name: &str, health: EngineHealth) -> String {
        let key = match health {
            EngineHealth::Ready => "app.engineConnected",
            EngineHealth::Busy => "app.healthChecking",
            EngineHealth::Warning => "app.healthCheckFailed",
            EngineHealth::Unavailable => "app.healthUnavailable",
        };
        I18n::format(locale, "app", key, &[("engine", Param::from(engine_name))])
    }
}

/// Lifecycle metadata tracked per lazily mounted workspace entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceMountMeta {
    pub workspace: WorkspaceId,
    pub entity_id: EntityId,
    pub mount_generation: u64,
    pub visit_count: usize,
    pub active: bool,
}

#[derive(Default)]
struct WorkspaceEntities {
    play: Option<Entity<PlayWorkspace>>,
    analysis: Option<Entity<AnalysisWorkspace>>,
    study: Option<Entity<StudyWorkspace>>,
    positions: Option<Entity<PositionsWorkspace>>,
    library: Option<Entity<LibraryWorkspace>>,
    players: Option<Entity<PlayersWorkspace>>,
    extras: Option<Entity<ExtrasWorkspace>>,
    meta: [Option<WorkspaceMountMeta>; 7],
}

impl WorkspaceEntities {
    fn meta(&self, id: WorkspaceId) -> Option<WorkspaceMountMeta> {
        self.meta[id.index()]
    }

    fn is_mounted(&self, id: WorkspaceId) -> bool {
        self.meta[id.index()].is_some()
    }

    fn mounted_count(&self) -> usize {
        self.meta.iter().filter(|m| m.is_some()).count()
    }

    fn clear_all(&mut self) {
        self.play = None;
        self.analysis = None;
        self.study = None;
        self.positions = None;
        self.library = None;
        self.players = None;
        self.extras = None;
        self.meta = [None; 7];
    }

    fn set_visible(&mut self, id: WorkspaceId, visible: bool, cx: &mut App) {
        if let Some(meta) = &mut self.meta[id.index()] {
            if visible && !meta.active {
                meta.visit_count += 1;
            }
            meta.active = visible;
        }
        match id {
            WorkspaceId::Play => {
                if let Some(ws) = &self.play {
                    ws.update(cx, |w, cx| w.set_visible(visible, cx));
                }
            }
            WorkspaceId::Analysis => {
                if let Some(ws) = &self.analysis {
                    ws.update(cx, |w, cx| w.set_visible(visible, cx));
                }
            }
            WorkspaceId::Study => {
                if let Some(ws) = &self.study {
                    ws.update(cx, |w, cx| w.set_visible(visible, cx));
                }
            }
            WorkspaceId::Positions => {
                if let Some(ws) = &self.positions {
                    ws.update(cx, |w, cx| w.set_visible(visible, cx));
                }
            }
            WorkspaceId::Library => {
                if let Some(ws) = &self.library {
                    ws.update(cx, |w, cx| w.set_visible(visible, cx));
                }
            }
            WorkspaceId::Players => {
                if let Some(ws) = &self.players {
                    ws.update(cx, |w, cx| w.set_visible(visible, cx));
                }
            }
            WorkspaceId::Extras => {
                if let Some(ws) = &self.extras {
                    ws.update(cx, |w, cx| w.set_visible(visible, cx));
                }
            }
        }
    }

    fn render_workspace(&self, id: WorkspaceId) -> Option<AnyElement> {
        let inner = match id {
            WorkspaceId::Play => self.play.clone().map(IntoElement::into_any_element),
            WorkspaceId::Analysis => self.analysis.clone().map(IntoElement::into_any_element),
            WorkspaceId::Study => self.study.clone().map(IntoElement::into_any_element),
            WorkspaceId::Positions => self.positions.clone().map(IntoElement::into_any_element),
            WorkspaceId::Library => self.library.clone().map(IntoElement::into_any_element),
            WorkspaceId::Players => self.players.clone().map(IntoElement::into_any_element),
            WorkspaceId::Extras => self.extras.clone().map(IntoElement::into_any_element),
        }?;

        Some(
            div()
                .id(SharedString::from(format!("workspace-panel-{}", id.id())))
                .test_support()
                .v_flex()
                .flex_1()
                .size_full()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .when_some(id.key_context(), |this, ctx| this.key_context(ctx))
                .child(inner)
                .into_any_element(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderPopover {
    Help,
    Profile,
}

/// Top-level NeonDojo desktop application shell.
pub struct AppShell {
    focus_handle: FocusHandle,
    version: VersionInfo,
    view: AppView,
    workspaces: WorkspaceEntities,
    reset_generation: u64,
    notices: Vec<AppNotice>,
    benchmark_busy: bool,
    profile: ProfileHeaderSummary,
    engine_status: FooterEngineStatus,
    open_popover: Option<HeaderPopover>,
    settings_modal_busy: ModalBusyLock,
    active_settings_view: Option<Entity<SettingsDialogView>>,
    has_open_match: bool,
    last_dropped_files: Vec<PathBuf>,
    state_persist_path: Option<PathBuf>,
}

impl EventEmitter<AppResetEvent> for AppShell {}

impl AppShell {
    /// Creates a new `AppShell`, focusing its root handle so global keyboard
    /// actions (`Cmd+0..7`, `Cmd+,`) work immediately.
    pub fn new(
        state_persist_path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        cx.observe_window_appearance(window, |this, window, cx| {
            NeonTheme::sync_system_appearance(window.appearance(), cx);
            this.sync_menus(cx);
            cx.notify();
        })
        .detach();

        let shell = Self {
            focus_handle,
            version: version_info(),
            view: AppView::Home,
            workspaces: WorkspaceEntities::default(),
            reset_generation: 0,
            notices: Vec::new(),
            benchmark_busy: false,
            profile: ProfileHeaderSummary::default(),
            engine_status: FooterEngineStatus::default(),
            open_popover: None,
            settings_modal_busy: ModalBusyLock::new(false),
            active_settings_view: None,
            has_open_match: false,
            last_dropped_files: Vec::new(),
            state_persist_path,
        };
        shell.sync_menus(cx);
        shell
    }

    /// Asynchronously starts [`CoreRuntime`] via [`TokioBridge`] using the
    /// Finder-safe bootstrap config in [`DesktopAppContext`].
    ///
    /// Never panics if PostgreSQL is unreachable; records the failure in
    /// `DesktopAppContext` so `AppShell` renders a native retry & URL-config banner.
    pub fn boot_backend_async(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut app_ctx) = cx.try_global::<DesktopAppContext>().cloned() else {
            return;
        };
        let core_config = match app_ctx.bootstrap.to_core_config() {
            Ok(cfg) => cfg,
            Err(err) => {
                app_ctx.set_failed(err.to_string());
                cx.set_global(app_ctx);
                self.engine_status.gnubg_health = EngineHealth::Unavailable;
                self.engine_status.neongammon_health = EngineHealth::Unavailable;
                cx.notify();
                return;
            }
        };

        app_ctx.set_connecting();
        let themes_dir_for_scan = app_ctx.bootstrap.custom_themes_dir.clone();
        let atlas_for_scan = Arc::clone(&app_ctx.board_atlas);
        cx.set_global(app_ctx);
        self.engine_status.gnubg_health = EngineHealth::Busy;
        self.engine_status.neongammon_health = EngineHealth::Busy;
        cx.notify();

        if let Some(themes_dir) = themes_dir_for_scan
            && cx.try_global::<TokioBridge>().is_some()
        {
            let theme_task = TokioBridge::spawn_read(cx, async move {
                tokio::task::spawn_blocking(move || {
                    neondojo_board::discover_bgblitz_themes(&themes_dir).ok()
                })
                .await
                .ok()
                .flatten()
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Some(entries)) = theme_task.await else {
                    return;
                };
                if let Ok(mut guard) = atlas_for_scan.lock() {
                    guard.set_bgblitz_entries(entries);
                }
                let _ = this.update_in(cx, |shell, _, cx| {
                    if let Some(play) = shell.workspaces.play.clone() {
                        play.update(cx, |_, cx| cx.notify());
                    }
                    if let Some(analysis) = shell.workspaces.analysis.clone() {
                        analysis.update(cx, |_, cx| cx.notify());
                    }
                    if let Some(study) = shell.workspaces.study.clone() {
                        study.update(cx, |_, cx| cx.notify());
                    }
                    cx.notify();
                });
            })
            .detach();
        }

        let task = TokioBridge::spawn_read(cx, boot_core_and_snapshot(core_config));

        cx.spawn_in(window, async move |this, cx| {
            let Ok(result) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Some(mut app_ctx) = cx.try_global::<DesktopAppContext>().cloned() {
                    match result {
                        Ok((runtime, facade, cfg_state, engines)) => {
                            app_ctx.set_connected(runtime, facade);
                            cx.set_global(app_ctx);
                            if let Some(cfg) = cfg_state {
                                shell.profile = ProfileHeaderSummary {
                                    name: SharedString::from(cfg.active_profile.name),
                                    modified: cfg.dirty,
                                    warning_count: cfg.initialization_warnings.len(),
                                };
                                let palette_id =
                                    crate::settings::draft_guard::ui_theme_to_palette_id(
                                        &cfg.current_configuration.appearance.ui_theme,
                                    );
                                let mode = crate::settings::draft_guard::config_mode_to_theme_mode(
                                    &cfg.current_configuration.appearance.mode,
                                );
                                let locale = crate::settings::draft_guard::config_locale_to_locale(
                                    &cfg.current_configuration.general.locale,
                                );
                                let board_cfg = crate::settings::board::appearance_to_board_config(
                                    &cfg.current_configuration.appearance,
                                );
                                if let Some(mut ctx) = cx.try_global::<DesktopAppContext>().cloned()
                                {
                                    ctx.board_config = board_cfg.clone();
                                    cx.set_global(ctx);
                                }
                                if let Some(mut state) = cx.try_global::<LocalUiState>().cloned() {
                                    state.board_theme = board_cfg.board_theme.clone();
                                    cx.set_global(state);
                                }
                                if let Some(play) = shell.workspaces.play.clone() {
                                    play.update(cx, |ws, cx| {
                                        ws.set_theme_config(board_cfg.clone(), cx);
                                    });
                                }
                                if let Some(analysis) = shell.workspaces.analysis.clone() {
                                    analysis.update(cx, |ws, cx| {
                                        ws.set_theme_config(board_cfg.clone(), cx);
                                    });
                                }
                                if let Some(study) = shell.workspaces.study.clone() {
                                    study.update(cx, |ws, cx| ws.set_theme_config(board_cfg, cx));
                                }
                                crate::i18n::set_locale(locale, cx);
                                shell.apply_appearance(palette_id, mode, window, cx);
                            }
                            if let Some(list) = engines {
                                let gnubg_ok = list.iter().any(|e| e.id == "gnubg" && e.available);
                                let sage_ok = list.iter().any(|e| e.id == "bgsage" && e.available);
                                shell.engine_status.gnubg_health = if gnubg_ok {
                                    EngineHealth::Ready
                                } else {
                                    EngineHealth::Unavailable
                                };
                                shell.engine_status.neongammon_health = if sage_ok {
                                    EngineHealth::Ready
                                } else {
                                    EngineHealth::Unavailable
                                };
                            }
                            if let Some(lib) = shell.workspaces.library.clone() {
                                lib.update(cx, |ws, cx| ws.refresh_on_library_mutated(cx));
                            }
                            // A user can enter Positions while the runtime is still connecting.
                            if shell.view == AppView::Workspace(WorkspaceId::Positions) {
                                shell
                                    .workspaces
                                    .set_visible(WorkspaceId::Positions, true, cx);
                            }
                            shell.sync_menus(cx);
                        }
                        Err(err) => {
                            app_ctx.set_failed(err.to_string());
                            cx.set_global(app_ctx);
                            shell.engine_status.gnubg_health = EngineHealth::Unavailable;
                            shell.engine_status.neongammon_health = EngineHealth::Unavailable;
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Opens the native PostgreSQL connection settings dialog when DB bootstrap fails.
    pub fn open_database_connection_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let initial_url = cx
            .try_global::<DesktopAppContext>()
            .map(|c| c.bootstrap.database_url.clone())
            .unwrap_or_default();
        let url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("postgres://user:pass@127.0.0.1:54329/neondojo")
                .default_value(initial_url)
        });
        let shell_entity = cx.entity();
        let config = ModalConfig::new("PostgreSQL Connection").size(ModalSize::Standard);
        let input_for_body = url_input.clone();
        let input_for_footer = url_input;

        open_modal_dialog(
            window,
            cx,
            config,
            move |_: &mut Window, cx: &mut App| {
                let p = NeonTheme::current_palette(cx);
                div()
                    .id("db-bootstrap-dialog-body")
                    .test_support()
                    .v_flex()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_body()
                            .text_color(p.muted)
                            .child("NeonDojo could not connect to PostgreSQL. Update the connection URI saved in Application Support/NeonDojo/bootstrap.json and retry."),
                    )
                    .child(Input::new(&input_for_body).id("db-bootstrap-url-input"))
                    .into_any_element()
            },
            Some(move |_: &mut Window, cx: &mut App| {
                let p = NeonTheme::current_palette(cx);
                let shell_ref = shell_entity.clone();
                let inp_ref = input_for_footer.clone();
                div()
                    .h_flex()
                    .gap(px(8.0))
                    .child(
                        ActionButton::new("db-bootstrap-cancel")
                            .label(I18n::t(current_locale(cx), "profile", "cancel"))
                            .palette(p)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        ActionButton::new("db-bootstrap-retry")
                            .icon(ActionIconName::Refresh)
                            .label(I18n::t(current_locale(cx), "workspace", "retry"))
                            .primary()
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                let new_url = inp_ref.read(cx).value().trim().to_string();
                                if !new_url.is_empty()
                                    && let Some(mut ctx) =
                                        cx.try_global::<DesktopAppContext>().cloned()
                                {
                                    let _ = ctx.update_database_url(new_url);
                                    cx.set_global(ctx);
                                }
                                window.close_dialog(cx);
                                shell_ref.update(cx, |s, cx| s.boot_backend_async(window, cx));
                            }),
                    )
                    .into_any_element()
            }),
            None,
        );
    }

    pub fn active_view(&self) -> AppView {
        self.view
    }

    pub fn mounted_workspace_count(&self) -> usize {
        self.workspaces.mounted_count()
    }

    pub fn workspace_mount_meta(&self, id: WorkspaceId) -> Option<WorkspaceMountMeta> {
        self.workspaces.meta(id)
    }

    pub fn play_workspace(&self) -> Option<Entity<PlayWorkspace>> {
        self.workspaces.play.clone()
    }

    pub fn analysis_workspace(&self) -> Option<Entity<AnalysisWorkspace>> {
        self.workspaces.analysis.clone()
    }

    pub fn study_workspace(&self) -> Option<Entity<StudyWorkspace>> {
        self.workspaces.study.clone()
    }

    pub fn positions_workspace(&self) -> Option<Entity<PositionsWorkspace>> {
        self.workspaces.positions.clone()
    }

    pub fn library_workspace(&self) -> Option<Entity<LibraryWorkspace>> {
        self.workspaces.library.clone()
    }

    pub fn players_workspace(&self) -> Option<Entity<PlayersWorkspace>> {
        self.workspaces.players.clone()
    }

    pub fn extras_workspace(&self) -> Option<Entity<ExtrasWorkspace>> {
        self.workspaces.extras.clone()
    }

    pub fn is_benchmark_busy(&self) -> bool {
        self.benchmark_busy
    }

    pub fn has_open_match(&self) -> bool {
        self.has_open_match
    }

    pub fn reset_generation(&self) -> u64 {
        self.reset_generation
    }

    pub fn notices(&self) -> &[AppNotice] {
        &self.notices
    }

    pub fn last_dropped_files(&self) -> &[PathBuf] {
        &self.last_dropped_files
    }

    pub fn settings_modal_busy_lock(&self) -> ModalBusyLock {
        self.settings_modal_busy.clone()
    }

    pub fn active_settings_view(&self) -> Option<Entity<SettingsDialogView>> {
        self.active_settings_view.clone()
    }

    pub fn set_benchmark_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.benchmark_busy = busy;
        cx.notify();
    }

    pub fn set_profile_summary(&mut self, profile: ProfileHeaderSummary, cx: &mut Context<Self>) {
        self.profile = profile;
        self.sync_menus(cx);
        cx.notify();
    }

    pub fn set_engine_status(&mut self, status: FooterEngineStatus, cx: &mut Context<Self>) {
        self.engine_status = status;
        cx.notify();
    }

    pub fn push_notice(&mut self, notice: AppNotice, cx: &mut Context<Self>) {
        self.notices.retain(|existing| existing.id != notice.id);
        self.notices.push(notice);
        cx.notify();
    }

    pub fn dismiss_notice(&mut self, id: &str, cx: &mut Context<Self>) {
        self.notices.retain(|n| n.id.as_ref() != id);
        cx.notify();
    }

    fn subscribe_workspace<W: EventEmitter<WorkspaceEvent> + 'static>(
        entity: &Entity<W>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(
            entity,
            window,
            |shell, _emitter, event: &WorkspaceEvent, window, cx| {
                shell.handle_workspace_event(event.clone(), window, cx);
            },
        )
        .detach();
    }

    fn ensure_workspace(
        &mut self,
        target: WorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspaces.is_mounted(target) {
            return;
        }
        let generation = self.reset_generation;
        let entity_id = match target {
            WorkspaceId::Play => {
                let entity = cx.new(|cx| PlayWorkspace::new(window, cx));
                Self::subscribe_workspace(&entity, window, cx);
                let id = entity.entity_id();
                self.workspaces.play = Some(entity);
                id
            }
            WorkspaceId::Analysis => {
                let entity = cx.new(|cx| AnalysisWorkspace::new(window, cx));
                Self::subscribe_workspace(&entity, window, cx);
                let id = entity.entity_id();
                self.workspaces.analysis = Some(entity);
                id
            }
            WorkspaceId::Study => {
                let entity = cx.new(|cx| StudyWorkspace::new(window, cx));
                Self::subscribe_workspace(&entity, window, cx);
                let id = entity.entity_id();
                self.workspaces.study = Some(entity);
                id
            }
            WorkspaceId::Positions => {
                let entity = cx.new(|cx| PositionsWorkspace::new(window, cx));
                Self::subscribe_workspace(&entity, window, cx);
                let id = entity.entity_id();
                self.workspaces.positions = Some(entity);
                id
            }
            WorkspaceId::Library => {
                let entity = cx.new(|cx| LibraryWorkspace::new(window, cx));
                Self::subscribe_workspace(&entity, window, cx);
                let id = entity.entity_id();
                self.workspaces.library = Some(entity);
                id
            }
            WorkspaceId::Players => {
                let entity = cx.new(|cx| PlayersWorkspace::new(window, cx));
                Self::subscribe_workspace(&entity, window, cx);
                let id = entity.entity_id();
                self.workspaces.players = Some(entity);
                id
            }
            WorkspaceId::Extras => {
                let entity = cx.new(|cx| ExtrasWorkspace::new(window, cx));
                Self::subscribe_workspace(&entity, window, cx);
                let id = entity.entity_id();
                self.workspaces.extras = Some(entity);
                id
            }
        };
        self.workspaces.meta[target.index()] = Some(WorkspaceMountMeta {
            workspace: target,
            entity_id,
            mount_generation: generation,
            visit_count: 1,
            active: true,
        });
    }

    /// Routes typed [`WorkspaceEvent`]s emitted by any mounted workspace.
    pub fn handle_workspace_event(
        &mut self,
        event: WorkspaceEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            WorkspaceEvent::OpenMatchInAnalysis {
                match_id,
                game_no,
                position_id,
                decision_no,
            } => {
                self.has_open_match = true;
                self.select_workspace(WorkspaceId::Analysis, window, cx);
                if let Some(analysis) = self.workspaces.analysis.clone() {
                    analysis.update(cx, |ws, cx| {
                        ws.open_match(match_id, game_no, position_id, decision_no, window, cx);
                    });
                }
            }
            WorkspaceEvent::OpenPlaySession { session_id, resume } => {
                self.select_workspace(WorkspaceId::Play, window, cx);
                if let Some(play) = self.workspaces.play.clone() {
                    play.update(cx, |ws, cx| {
                        ws.open_session(session_id, resume, window, cx);
                    });
                }
            }
            WorkspaceEvent::OpenLibraryDatabase { database_id } => {
                self.select_workspace(WorkspaceId::Library, window, cx);
                if let Some(library) = &self.workspaces.library {
                    library.update(cx, |ws, cx| ws.select_database(Some(database_id), cx));
                }
            }
            WorkspaceEvent::SearchPositionInLibrary { position_id } => {
                self.select_workspace(WorkspaceId::Library, window, cx);
                if let Some(library) = self.workspaces.library.clone() {
                    library.update(cx, |ws, cx| {
                        ws.apply_position_search(position_id, cx);
                    });
                }
            }
            WorkspaceEvent::StartBulkAnalysis { match_ids } => {
                self.select_workspace(WorkspaceId::Library, window, cx);
                if let Some(library) = self.workspaces.library.clone() {
                    library.update(cx, |ws, cx| {
                        ws.start_bulk_analysis(match_ids, cx);
                    });
                }
            }
            WorkspaceEvent::OpenExtrasTool { tool } => {
                self.select_workspace(WorkspaceId::Extras, window, cx);
                if let Some(extras) = self.workspaces.extras.clone() {
                    extras.update(cx, |ws, cx| {
                        ws.select_tool(tool.as_ref(), cx);
                    });
                }
            }
            WorkspaceEvent::OpenSettings { category } => {
                self.open_settings_modal_at(category, window, cx);
            }
            WorkspaceEvent::OpenStudyReport { match_id } => {
                self.select_workspace(WorkspaceId::Study, window, cx);
                if let Some(study) = self.workspaces.study.clone() {
                    study.update(cx, |ws, cx| {
                        ws.open_report_for_match(match_id, cx);
                    });
                }
            }
            WorkspaceEvent::LibraryMutated
            | WorkspaceEvent::AnalysisJobChanged { .. }
            | WorkspaceEvent::AnnotationSaved { .. } => {
                if let Some(library) = self.workspaces.library.clone() {
                    library.update(cx, |ws, cx| ws.refresh_on_library_mutated(cx));
                }
            }
            WorkspaceEvent::PlaySwitchedToCompetition { match_id } => {
                if let Some(analysis) = self.workspaces.analysis.clone() {
                    analysis.update(cx, |ws, cx| ws.invalidate_match(match_id, cx));
                }
            }
            WorkspaceEvent::TriggerAppReset { reason } => {
                self.trigger_app_reset(reason, cx);
            }
            WorkspaceEvent::ShowNotice(notice) => {
                self.push_notice(notice, cx);
            }
            WorkspaceEvent::SetBenchmarkBusy(busy) => {
                self.set_benchmark_busy(busy, cx);
            }
        }
    }

    /// Prompts via native macOS file/folder picker and imports selected paths
    /// directly through [`LibraryWorkspace`].
    pub fn prompt_import_paths(
        &mut self,
        directories: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_workspace(WorkspaceId::Library, window, cx);
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: !directories,
            directories,
            multiple: true,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            if paths.is_empty() {
                return;
            }
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.select_workspace(WorkspaceId::Library, window, cx);
                if let Some(library) = shell.workspaces.library.clone() {
                    library.update(cx, |ws, cx| ws.import_paths(paths, cx));
                }
            });
        })
        .detach();
    }

    /// Navigates to the Home landing screen while preserving all mounted workspaces.
    pub fn go_home(&mut self, cx: &mut Context<Self>) {
        if let AppView::Workspace(prev) = self.view {
            self.workspaces.set_visible(prev, false, cx);
        }
        self.open_popover = None;
        self.view = AppView::Home;
        self.sync_menus(cx);
        cx.notify();
    }

    /// Navigates to a workspace, lazily creating its entity on first visit
    /// and retaining its state across subsequent tab switches.
    pub fn select_workspace(
        &mut self,
        target: WorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let AppView::Workspace(prev) = self.view {
            if prev == target {
                self.open_popover = None;
                cx.notify();
                return;
            }
            self.workspaces.set_visible(prev, false, cx);
        }

        self.ensure_workspace(target, window, cx);
        self.workspaces.set_visible(target, true, cx);

        self.open_popover = None;
        self.view = AppView::Workspace(target);
        self.sync_menus(cx);
        cx.notify();
    }

    /// Resets all mounted workspace entities (e.g. after database backup restore)
    /// and emits [`AppResetEvent`].
    pub fn trigger_app_reset(&mut self, reason: impl Into<SharedString>, cx: &mut Context<Self>) {
        let reason = reason.into();
        self.reset_generation += 1;
        self.workspaces.clear_all();
        self.has_open_match = false;
        self.notices.clear();
        self.open_popover = None;
        self.view = AppView::Home;
        if let Some(mut ctx) = cx.try_global::<DesktopAppContext>().cloned() {
            ctx.reset_generation = self.reset_generation;
            cx.set_global(ctx);
        }
        self.sync_menus(cx);
        cx.emit(AppResetEvent {
            generation: self.reset_generation,
            reason,
        });
        cx.notify();
    }

    /// Applies a new interface palette and/or display mode, projects it onto
    /// `gpui-kit`, persists `LocalUiState` if configured, and refreshes macOS menus.
    pub fn apply_appearance(
        &mut self,
        palette_id: NeonPaletteId,
        mode: ThemeMode,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let system_dark = ResolvedMode::from_window_appearance(window.appearance()).is_dark();
        NeonTheme::set(palette_id, mode, system_dark, cx);
        if let Some(state) = cx.try_global::<LocalUiState>().cloned() {
            let mut updated = state;
            updated.appearance.theme = palette_id;
            updated.appearance.mode = mode;
            if let Some(path) = &self.state_persist_path {
                let _ = updated.save_atomic(path);
            }
            cx.set_global(updated);
        }
        self.sync_menus(cx);
        cx.notify();
    }

    /// Opens the full 7-category Settings & Profiles modal dialog.
    pub fn open_settings_modal_at(
        &mut self,
        category: SettingsCategory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_popover = None;
        let facade = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade());
        let shell_entity = cx.entity();

        let view = open_settings_dialog(
            window,
            cx,
            category,
            facade,
            self.state_persist_path.clone(),
            self.settings_modal_busy.clone(),
            Some(Rc::new(
                move |applied: AppliedSettingsEvent, _window, cx| {
                    shell_entity.update(cx, |shell, cx| {
                        shell.profile.name = applied.active_profile_name;
                        shell.profile.modified = applied.profile_modified;
                        if let Some(mut ctx) = cx.try_global::<DesktopAppContext>().cloned() {
                            ctx.board_config = applied.board_config.clone();
                            cx.set_global(ctx);
                        }
                        if let Some(play) = shell.workspaces.play.clone() {
                            play.update(cx, |ws, cx| {
                                ws.set_theme_config(applied.board_config.clone(), cx);
                            });
                        }
                        if let Some(analysis) = shell.workspaces.analysis.clone() {
                            analysis.update(cx, |ws, cx| {
                                ws.set_theme_config(applied.board_config.clone(), cx);
                            });
                        }
                        if let Some(study) = shell.workspaces.study.clone() {
                            study.update(cx, |ws, cx| {
                                ws.set_theme_config(applied.board_config.clone(), cx);
                            });
                        }
                        shell.sync_menus(cx);
                        cx.notify();
                    });
                },
            )),
        );
        self.active_settings_view = Some(view);
        cx.notify();
    }

    /// Opens the Settings modal at the default `General` category.
    pub fn open_settings_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_modal_at(SettingsCategory::General, window, cx);
    }

    /// Handles a dropped batch of match files on the Home screen by routing
    /// accepted files straight into [`LibraryWorkspace::import_paths`].
    pub fn handle_dropped_matches(
        &mut self,
        batch: DroppedMatchBatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if batch.accepted.is_empty() {
            self.push_notice(
                AppNotice {
                    id: "drop-empty".into(),
                    kind: BannerKind::Warning,
                    title: None,
                    message: AppError::app("error.generic")
                        .with_diagnostic(format!(
                            "Skipped {} unsupported, {} hidden.",
                            batch.skipped_unsupported, batch.ignored_hidden
                        ))
                        .into(),
                    dismissible: true,
                },
                cx,
            );
            return;
        }
        let count = batch.accepted.len();
        let accepted = batch.accepted;
        self.last_dropped_files = accepted.clone();
        self.select_workspace(WorkspaceId::Library, window, cx);
        if let Some(library) = self.workspaces.library.clone() {
            library.update(cx, |ws, cx| ws.import_paths(accepted, cx));
        }
        self.push_notice(
            AppNotice {
                id: "drop-queued".into(),
                kind: BannerKind::Success,
                title: None,
                message: SharedString::from(format!(
                    "{} ({count})",
                    I18n::t(current_locale(cx), "app", "app.importPlain")
                ))
                .into(),
                dismissible: true,
            },
            cx,
        );
    }

    fn menu_state(&self, cx: &App) -> MenuState {
        let theme = cx.try_global::<NeonTheme>().copied();
        MenuState {
            locale: current_locale(cx),
            active_profile_name: self.profile.name.clone(),
            profile_modified: self.profile.modified,
            palette: theme.map(|t| t.palette_id).unwrap_or_default(),
            mode: theme.map(|t| t.mode).unwrap_or_default(),
            active_workspace_index: match self.view {
                AppView::Home => None,
                AppView::Workspace(id) => Some(id.index()),
            },
            has_open_match: self.has_open_match,
        }
    }

    fn sync_menus(&self, cx: &mut App) {
        let state = self.menu_state(cx);
        install_app_menus(cx, &state);
    }

    fn open_about_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let release = self.version.release;
        let config = ModalConfig::new("About NeonDojo").size(ModalSize::Compact);
        open_modal_dialog(
            window,
            cx,
            config,
            move |_, cx| {
                let p = NeonTheme::current_palette(cx);
                div()
                    .v_flex()
                    .gap(px(10.0))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(brand_logo(28.0))
                            .child(
                                div()
                                    .text_subject()
                                    .text_color(p.text)
                                    .child(format!("NeonDojo {release}")),
                            ),
                    )
                    .child(div().text_body().text_color(p.muted).child(I18n::t(
                        current_locale(cx),
                        "workspace",
                        "welcomeDescription",
                    )))
                    .into_any_element()
            },
            None::<fn(&mut Window, &mut App) -> gpui_kit::AnyElement>,
            None,
        );
    }

    fn render_header(&mut self, p: NeonPalette, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = current_locale(cx);
        let active_ws_id = match self.view {
            AppView::Home => None,
            AppView::Workspace(id) => Some(SharedString::from(id.id())),
        };

        let nav_items: Vec<TextNavItem> = WorkspaceId::ALL
            .iter()
            .map(|&ws| {
                let label = ws.localized_label(locale);
                TextNavItem::new(ws.id(), label)
                    .element_id(ws.tab_element_id())
                    .tooltip(format!("{label} ({})", ws.shortcut_label()))
            })
            .collect();

        let shell_for_nav = cx.entity();
        let workspace_nav = TextNavBar::new("header-workspace-nav", nav_items)
            .active(active_ws_id)
            .density(TextNavDensity::Header)
            .palette(p)
            .on_select(move |id, window, cx| {
                if let Some(ws) = WorkspaceId::from_id(id) {
                    shell_for_nav.update(cx, |shell, cx| shell.select_workspace(ws, window, cx));
                }
            });

        let help_open = self.open_popover == Some(HeaderPopover::Help);
        let profile_open = self.open_popover == Some(HeaderPopover::Profile);
        let profile_label = if self.profile.modified {
            format!(
                "{} · {}",
                self.profile.name,
                I18n::t(locale, "profile", "modified")
            )
        } else {
            self.profile.name.to_string()
        };

        let shell_for_settings = cx.entity();
        let shell_for_help = cx.entity();
        let shell_for_profile = cx.entity();

        div()
            .id("app-header")
            .test_support()
            .relative()
            .h_flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .px(px(16.0))
            .py(px(6.0))
            .bg(p.surface)
            .border_b_1()
            .border_color(p.border)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(20.0))
                    .child(
                        div()
                            .id("header-brand-home")
                            .test_support()
                            .h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .py(px(4.0))
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| this.go_home(cx)))
                            .child(brand_logo(22.0))
                            .child(
                                div()
                                    .text_panel()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text)
                                    .child("NeonDojo"),
                            ),
                    )
                    .child(workspace_nav),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        ActionButton::new("header-help-button")
                            .icon(ActionIconName::Help)
                            .label(I18n::t(locale, "app", "app.help"))
                            .variant(ActionButtonVariant::Ghost)
                            .active(help_open)
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                shell_for_help.update(cx, |shell, cx| {
                                    shell.open_popover =
                                        if shell.open_popover == Some(HeaderPopover::Help) {
                                            None
                                        } else {
                                            Some(HeaderPopover::Help)
                                        };
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        div()
                            .id("header-profile-selector")
                            .test_support()
                            .h_flex()
                            .items_center()
                            .gap(px(6.0))
                            .px(px(10.0))
                            .py(px(5.0))
                            .min_h(px(32.0))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(if profile_open {
                                p.accent
                            } else {
                                p.control_border
                            })
                            .bg(if profile_open { p.selected } else { p.surface })
                            .text_label()
                            .text_color(p.text)
                            .cursor_pointer()
                            .hover(move |s| s.bg(p.selected))
                            .on_click(move |_, _, cx| {
                                shell_for_profile.update(cx, |shell, cx| {
                                    shell.open_popover =
                                        if shell.open_popover == Some(HeaderPopover::Profile) {
                                            None
                                        } else {
                                            Some(HeaderPopover::Profile)
                                        };
                                    cx.notify();
                                });
                            })
                            .child(profile_label)
                            .when(self.profile.warning_count > 0, |this| {
                                this.child(
                                    div()
                                        .text_meta()
                                        .tabular_nums()
                                        .px(px(5.0))
                                        .py(px(1.0))
                                        .rounded(px(4.0))
                                        .bg(p.warning_bg)
                                        .text_color(p.warning_text)
                                        .child(self.profile.warning_count.to_string()),
                                )
                            }),
                    )
                    .child(
                        ActionButton::new("header-settings-button")
                            .icon(ActionIconName::Settings)
                            .tooltip(I18n::t(locale, "settings", "title"))
                            .shortcut("Cmd+,")
                            .variant(ActionButtonVariant::Ghost)
                            .compact()
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                shell_for_settings.update(cx, |shell, cx| {
                                    shell.open_settings_modal(window, cx);
                                });
                            }),
                    ),
            )
            .when_some(self.open_popover, |this, popover| {
                this.child(self.render_header_popover(popover, p, cx))
            })
    }

    fn render_header_popover(
        &self,
        popover: HeaderPopover,
        p: NeonPalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let locale = current_locale(cx);
        let shell = cx.entity();
        match popover {
            HeaderPopover::Help => div()
                .id("header-help-popover")
                .test_support()
                .absolute()
                .top(px(52.0))
                .right(px(140.0))
                .w(px(240.0))
                .v_flex()
                .gap(px(4.0))
                .p(px(6.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .child(
                    ActionButton::new("help-user-guide")
                        .icon(ActionIconName::Link)
                        .label(format!(
                            "{} ↗",
                            I18n::t(locale, "app", "app.neondojoManual")
                        ))
                        .variant(ActionButtonVariant::Ghost)
                        .palette(p)
                        .on_click({
                            let shell = shell.clone();
                            move |_, _, cx| {
                                cx.open_url(USER_GUIDE_URL);
                                shell.update(cx, |s, cx| {
                                    s.open_popover = None;
                                    cx.notify();
                                });
                            }
                        }),
                )
                .child(
                    ActionButton::new("help-neongammon-manual")
                        .icon(ActionIconName::Link)
                        .label(format!(
                            "{} ↗",
                            I18n::t(locale, "app", "app.neongammonManual")
                        ))
                        .variant(ActionButtonVariant::Ghost)
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            cx.open_url(NEONGAMMON_MANUAL_URL);
                            shell.update(cx, |s, cx| {
                                s.open_popover = None;
                                cx.notify();
                            });
                        }),
                ),
            HeaderPopover::Profile => div()
                .id("header-profile-popover")
                .test_support()
                .absolute()
                .top(px(52.0))
                .right(px(64.0))
                .w(px(240.0))
                .v_flex()
                .gap(px(6.0))
                .p(px(10.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .child(div().text_meta().text_color(p.muted).child(I18n::t(
                    locale,
                    "profile",
                    "activeProfile",
                )))
                .child(
                    div()
                        .text_panel()
                        .text_color(p.text)
                        .child(self.profile.name.clone()),
                )
                .child(
                    ActionButton::new("profile-open-settings")
                        .icon(ActionIconName::Settings)
                        .label(I18n::t(locale, "profile", "manage"))
                        .palette(p)
                        .on_click(move |_, window, cx| {
                            shell.update(cx, |s, cx| {
                                s.open_settings_modal_at(SettingsCategory::Profiles, window, cx);
                            });
                        }),
                ),
        }
    }

    fn render_home(&self, p: NeonPalette, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = current_locale(cx);
        let shell_drop = cx.entity();
        let shell_play = cx.entity();
        let shell_analysis = cx.entity();
        let shell_study = cx.entity();
        let shell_library = cx.entity();
        let shell_settings = cx.entity();

        div()
            .id("home-screen")
            .test_support()
            .v_flex()
            .flex_1()
            .size_full()
            .items_center()
            .p(px(28.0))
            .child(
                div()
                    .v_flex()
                    .w_full()
                    .max_w(px(1160.0))
                    .gap(px(20.0))
                    .child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .child(div().text_page().text_color(p.text).child(I18n::t(
                                locale,
                                "workspace",
                                "welcomeTitle",
                            )))
                            .child(div().text_body().text_color(p.muted).child(I18n::t(
                                locale,
                                "workspace",
                                "welcomeDescription",
                            ))),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_start()
                            .gap(px(20.0))
                            .w_full()
                            .child(
                                div().v_flex().flex_1().min_w(px(0.0)).child(
                                    MatchDropZone::new("home-match-drop-zone")
                                        .palette(p)
                                        .on_drop_batch(move |batch, window, cx| {
                                            shell_drop.update(cx, |shell, cx| {
                                                shell.handle_dropped_matches(batch, window, cx);
                                            });
                                        }),
                                ),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .w(px(320.0))
                                    .flex_shrink_0()
                                    .gap(px(12.0))
                                    .p(px(18.0))
                                    .rounded(px(10.0))
                                    .border_1()
                                    .border_color(p.border)
                                    .bg(p.surface)
                                    .child(div().text_panel().text_color(p.text).child(I18n::t(
                                        locale,
                                        "workspace",
                                        "quickStart",
                                    )))
                                    .child(
                                        ActionButton::new("home-quick-play")
                                            .icon(ActionIconName::Play)
                                            .label(I18n::t(locale, "workspace", "playComputer"))
                                            .shortcut("Cmd+1")
                                            .primary()
                                            .palette(p)
                                            .on_click(move |_, window, cx| {
                                                shell_play.update(cx, |s, cx| {
                                                    s.select_workspace(
                                                        WorkspaceId::Play,
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            }),
                                    )
                                    .child(
                                        ActionButton::new("home-quick-analysis")
                                            .icon(ActionIconName::Analysis)
                                            .label(I18n::t(locale, "workspace", "analysis"))
                                            .shortcut("Cmd+2")
                                            .palette(p)
                                            .on_click(move |_, window, cx| {
                                                shell_analysis.update(cx, |s, cx| {
                                                    s.select_workspace(
                                                        WorkspaceId::Analysis,
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            }),
                                    )
                                    .child(
                                        ActionButton::new("home-quick-study")
                                            .icon(ActionIconName::Cards)
                                            .label(I18n::t(locale, "workspace", "cards"))
                                            .shortcut("Cmd+3")
                                            .palette(p)
                                            .on_click(move |_, window, cx| {
                                                shell_study.update(cx, |s, cx| {
                                                    s.select_workspace(
                                                        WorkspaceId::Study,
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            }),
                                    )
                                    .child(
                                        ActionButton::new("home-quick-library")
                                            .icon(ActionIconName::Database)
                                            .label(I18n::t(locale, "workspace", "database"))
                                            .shortcut("Cmd+5")
                                            .palette(p)
                                            .on_click(move |_, window, cx| {
                                                shell_library.update(cx, |s, cx| {
                                                    s.select_workspace(
                                                        WorkspaceId::Library,
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            }),
                                    )
                                    .child(
                                        ActionButton::new("home-quick-settings")
                                            .icon(ActionIconName::Settings)
                                            .label(I18n::t(locale, "workspace", "settings"))
                                            .shortcut("Cmd+,")
                                            .palette(p)
                                            .on_click(move |_, window, cx| {
                                                shell_settings.update(cx, |s, cx| {
                                                    s.open_settings_modal(window, cx);
                                                });
                                            }),
                                    ),
                            ),
                    ),
            )
    }

    fn render_footer(&self, p: NeonPalette, cx: &App) -> impl IntoElement {
        let locale = current_locale(cx);
        let gnubg_label =
            FooterEngineStatus::engine_label(locale, "GNUbg", self.engine_status.gnubg_health);
        let sage_label = FooterEngineStatus::engine_label(
            locale,
            "NeonGammon",
            self.engine_status.neongammon_health,
        );
        let palette_label = I18n::t(locale, "ui-appearance", p.id.as_str());
        let mode_label = I18n::t(
            locale,
            "ui-appearance",
            if p.mode.is_dark() { "dark" } else { "light" },
        );

        div()
            .id("app-footer")
            .test_support()
            .h_flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .px(px(16.0))
            .py(px(6.0))
            .bg(p.bg)
            .border_t_1()
            .border_color(p.border)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(14.0))
                    .child(
                        EngineStatusDot::new(
                            "footer-gnubg-status",
                            self.engine_status.gnubg_health,
                            gnubg_label,
                        )
                        .palette(p),
                    )
                    .child(
                        EngineStatusDot::new(
                            "footer-neongammon-status",
                            self.engine_status.neongammon_health,
                            sage_label,
                        )
                        .palette(p),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(12.0))
                    .text_meta()
                    .tabular_nums()
                    .text_color(p.muted)
                    .child(format!("{palette_label} · {mode_label}"))
                    .child(format!("NeonDojo {}", self.version.release)),
            )
    }
}

impl Focusable for AppShell {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = NeonTheme::current_palette(cx);
        let shell_for_dismiss = cx.entity();
        let shell_for_db_retry = cx.entity();

        let db_failure =
            cx.try_global::<DesktopAppContext>()
                .and_then(|ctx| match &ctx.connection {
                    BackendConnectionState::Failed {
                        database_url_redacted,
                        message,
                    } => Some((database_url_redacted.clone(), message.clone())),
                    _ => None,
                });

        let notices_strip = if self.notices.is_empty() {
            None
        } else {
            let mut list = div()
                .id("app-notices")
                .test_support()
                .v_flex()
                .gap(px(8.0))
                .px(px(16.0))
                .pt(px(10.0));
            for notice in &self.notices {
                let mut banner = StatusBanner::new(
                    SharedString::from(format!("app-notice-{}", notice.id)),
                    notice.kind,
                    notice.message.resolve(cx),
                )
                .palette(p);
                if let Some(title) = &notice.title {
                    banner = banner.title(title.resolve(cx));
                }
                if notice.dismissible {
                    let notice_id = notice.id.clone();
                    let shell_ref = shell_for_dismiss.clone();
                    banner = banner.on_dismiss(move |_, _, cx| {
                        shell_ref.update(cx, |s, cx| s.dismiss_notice(notice_id.as_ref(), cx));
                    });
                }
                list = list.child(banner);
            }
            Some(list)
        };

        let content = match self.view {
            AppView::Home => self.render_home(p, cx).into_any_element(),
            AppView::Workspace(id) => self
                .workspaces
                .render_workspace(id)
                .unwrap_or_else(|| div().into_any_element()),
        };

        div()
            .id("neondojo-app-shell")
            .test_support()
            .track_focus(&self.focus_handle)
            .key_context(CTX_APP)
            .v_flex()
            .size_full()
            .bg(p.bg)
            .text_color(p.text)
            .on_action(cx.listener(|this, _: &GoHome, _, cx| this.go_home(cx)))
            .on_action(cx.listener(|this, _: &SelectWorkspacePlay, window, cx| {
                this.select_workspace(WorkspaceId::Play, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &SelectWorkspaceAnalysis, window, cx| {
                    this.select_workspace(WorkspaceId::Analysis, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &SelectWorkspaceStudy, window, cx| {
                this.select_workspace(WorkspaceId::Study, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &SelectWorkspacePositions, window, cx| {
                    this.select_workspace(WorkspaceId::Positions, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &SelectWorkspaceLibrary, window, cx| {
                this.select_workspace(WorkspaceId::Library, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SelectWorkspacePlayers, window, cx| {
                this.select_workspace(WorkspaceId::Players, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SelectWorkspaceExtras, window, cx| {
                this.select_workspace(WorkspaceId::Extras, window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                this.open_settings_modal(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenAbout, window, cx| {
                this.open_about_modal(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenDatabaseBackup, window, cx| {
                this.handle_workspace_event(
                    WorkspaceEvent::OpenExtrasTool {
                        tool: "backup".into(),
                    },
                    window,
                    cx,
                );
            }))
            .on_action(cx.listener(|this, _: &ManageProfiles, window, cx| {
                this.open_settings_modal_at(SettingsCategory::Profiles, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowProfileChanges, window, cx| {
                this.open_settings_modal_at(SettingsCategory::Profiles, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ImportMatchFiles, window, cx| {
                this.prompt_import_paths(false, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ImportMatchFolder, window, cx| {
                this.prompt_import_paths(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NewPlayMatch, window, cx| {
                this.select_workspace(WorkspaceId::Play, window, cx);
                if let Some(play) = this.workspaces.play.clone() {
                    play.update(cx, |ws, cx| ws.open_new_match_setup(window, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &SetPaletteGraphite, window, cx| {
                let mode = cx
                    .try_global::<NeonTheme>()
                    .map(|t| t.mode)
                    .unwrap_or_default();
                this.apply_appearance(NeonPaletteId::Graphite, mode, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SetPaletteClub, window, cx| {
                let mode = cx
                    .try_global::<NeonTheme>()
                    .map(|t| t.mode)
                    .unwrap_or_default();
                this.apply_appearance(NeonPaletteId::Club, mode, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SetPalettePaper, window, cx| {
                let mode = cx
                    .try_global::<NeonTheme>()
                    .map(|t| t.mode)
                    .unwrap_or_default();
                this.apply_appearance(NeonPaletteId::Paper, mode, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SetModeLight, window, cx| {
                let palette = cx
                    .try_global::<NeonTheme>()
                    .map(|t| t.palette_id)
                    .unwrap_or_default();
                this.apply_appearance(palette, ThemeMode::Light, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SetModeDark, window, cx| {
                let palette = cx
                    .try_global::<NeonTheme>()
                    .map(|t| t.palette_id)
                    .unwrap_or_default();
                this.apply_appearance(palette, ThemeMode::Dark, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SetModeSystem, window, cx| {
                let palette = cx
                    .try_global::<NeonTheme>()
                    .map(|t| t.palette_id)
                    .unwrap_or_default();
                this.apply_appearance(palette, ThemeMode::System, window, cx);
            }))
            .on_action(|_: &OpenUserGuide, _, cx| {
                cx.open_url(USER_GUIDE_URL);
            })
            .on_action(|_: &OpenNeonGammonManual, _, cx| {
                cx.open_url(NEONGAMMON_MANUAL_URL);
            })
            .child(self.render_header(p, cx))
            .when_some(db_failure, |this, (url, msg)| {
                let retry_shell = shell_for_db_retry.clone();
                this.child(
                    div().px(px(16.0)).pt(px(10.0)).child(
                        StatusBanner::new(
                            "backend-connection-error-banner",
                            BannerKind::Error,
                            format!("PostgreSQL ({url}): {msg}"),
                        )
                        .title(I18n::t(locale, "workspace", "configurationError"))
                        .palette(p)
                        .on_retry(
                            I18n::t(locale, "workspace", "retry"),
                            move |_, window, cx| {
                                retry_shell.update(cx, |s, cx| {
                                    s.open_database_connection_dialog(window, cx);
                                });
                            },
                        ),
                    ),
                )
            })
            .when(self.benchmark_busy, |this| {
                this.child(
                    div().px(px(16.0)).pt(px(10.0)).child(
                        StatusBanner::new(
                            "global-benchmark-busy-banner",
                            BannerKind::Warning,
                            I18n::t(locale, "workspace", "globalBusy"),
                        )
                        .palette(p),
                    ),
                )
            })
            .when_some(notices_strip, |this, strip| this.child(strip))
            .child(div().v_flex().flex_1().min_h(px(0.0)).child(content))
            .child(self.render_footer(p, cx))
    }
}
