//! Complete NeonDojo Settings & Profiles dialog (`SettingsDialog.tsx`).
//!
//! Hosts all 7 categories (`General`, `Appearance`, `Analysis`, `Playing`,
//! `Board`, `Engines`, `Profiles`) in a 164px vertical `TextNavBar` sidebar
//! with a sticky footer, 3-destination dirty tracking, `SettingsDraftGuardDialog`,
//! live `BackendFacade` async persistence via `TokioBridge`, and offline/test fallback.

pub mod analysis;
pub mod appearance;
pub mod board;
pub mod draft_guard;
pub mod engines;
pub mod general;
pub mod playing;
pub mod profiles;

use crate::app_context::DesktopAppContext;
use crate::i18n::{current_locale, format_domain_error, set_locale};
use crate::local_state::LocalUiState;
use crate::settings::analysis::{
    ThresholdInputEntities, open_opening_book_dialog, render_analysis_section,
};
use crate::settings::appearance::render_appearance_section;
use crate::settings::board::{
    BoardSectionCallbacks, appearance_to_board_config, render_board_section,
};
use crate::settings::draft_guard::{
    GuardedDestination, SettingsCategory, SettingsDraft, config_locale_to_locale,
    config_mode_to_theme_mode, default_host_settings, default_profile_configuration,
    locale_to_config_locale, open_settings_draft_guard_dialog, palette_id_to_ui_theme,
    theme_mode_to_config_mode, ui_theme_to_palette_id,
};
use crate::settings::engines::{
    EngineSectionCallbacks, HostConcurrencyField, render_engines_section,
};
use crate::settings::general::render_general_section;
use crate::settings::playing::{PlaySectionCallbacks, render_playing_section};
use crate::settings::profiles::{ProfileSectionCallbacks, render_profiles_section};
use crate::theme::{NeonPaletteId, NeonTheme, NeonTypographyExt, ResolvedMode, ThemeMode};
use crate::tokio_bridge::TokioBridge;
use crate::ui::action_button::ActionButton;
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::decimal_input::{DecimalInput, DecimalInputConfig};
use crate::ui::icons::ActionIconName;
use crate::ui::modal::{ModalBusyLock, ModalConfig, ModalSize, open_modal_dialog};
use crate::ui::text_nav::{TextNavBar, TextNavDensity, TextNavItem, TextNavOrientation};
use gpui_kit::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Subscription, TestSupportExt, Window,
    base::StyledExt,
    component::WindowExt,
    component::input::{InputEvent, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::configuration::{
    ConfigurationBookProvider, ConfigurationDifference, ConfigurationDirection,
    ConfigurationEngine, ConfigurationProfile, RestoreConfiguration, UpdateConfiguration,
    differences,
};
use neondojo::domain_error::ErrorKind;
use neondojo::facade::BackendFacade;
use neondojo::player_directory::VirtualPlayerProfile;
use neondojo::profiles::{
    ActivateConfigurationProfile, CreateConfigurationProfile, DeleteConfigurationProfile,
    ProfileSourceAction, RenameConfigurationProfile, SaveConfigurationProfile,
};
use neondojo::settings::{EngineInfo, OpenSageInstallation};
use neondojo_board::{
    BgBlitzThemeEntry, BoardPosition, BoardView, ThemeAtlasCache, discover_bgblitz_themes,
};
use neondojo_i18n::{I18n, Locale};
use neondojo_rules::options::engine_depths;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Payload emitted to `AppShell` whenever Settings successfully applies changes.
#[derive(Debug, Clone)]
pub struct AppliedSettingsEvent {
    pub locale: Locale,
    pub palette_id: NeonPaletteId,
    pub mode: ThemeMode,
    pub board_theme: String,
    pub active_profile_name: SharedString,
    pub profile_modified: bool,
}

pub type OnSettingsApplied = Rc<dyn Fn(AppliedSettingsEvent, &mut Window, &mut App) + 'static>;

/// Complete stateful GPUI Settings & Profiles modal view.
pub struct SettingsDialogView {
    facade: Option<BackendFacade>,
    persist_path: Option<PathBuf>,
    category: SettingsCategory,
    draft: SettingsDraft,
    engines: Vec<EngineInfo>,
    sage_installation: Option<OpenSageInstallation>,
    virtual_profiles: Vec<VirtualPlayerProfile>,
    profiles: Vec<ConfigurationProfile>,
    active_profile_id: Option<Uuid>,
    active_profile_name: SharedString,
    active_profile_revision: i64,
    profile_differences: Vec<ConfigurationDifference>,
    bgblitz_themes: Vec<BgBlitzThemeEntry>,

    // Persistent GPUI inputs & board preview
    player_name_input: Entity<InputState>,
    gnubg_path_input: Entity<InputState>,
    sage_path_input: Entity<InputState>,
    profile_name_input: Entity<InputState>,
    threshold_inputs: ThresholdInputEntities,
    tutor_threshold_input: Entity<DecimalInput>,
    preview_board: Entity<BoardView>,

    // Async status & conflict flags
    busy: ModalBusyLock,
    loading: bool,
    saving: bool,
    checking_gnubg: bool,
    verifying_sage: bool,
    save_error: Option<SharedString>,
    conflict: bool,
    gnubg_check_error: Option<SharedString>,
    sage_error: Option<SharedString>,
    profile_error: Option<SharedString>,
    profile_conflict: bool,

    on_applied: Option<OnSettingsApplied>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsDialogView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        initial_category: SettingsCategory,
        facade: Option<BackendFacade>,
        persist_path: Option<PathBuf>,
        busy: ModalBusyLock,
        on_applied: Option<OnSettingsApplied>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let locale = current_locale(cx);
        let sep = locale.decimal_separator();
        let active_theme = cx.try_global::<NeonTheme>().copied();
        let local_state = cx.try_global::<LocalUiState>().cloned().unwrap_or_default();
        let app_ctx = cx.try_global::<DesktopAppContext>().cloned();

        let palette_id = active_theme
            .map(|t| t.palette_id)
            .unwrap_or(local_state.appearance.theme);
        let mode = active_theme
            .map(|t| t.mode)
            .unwrap_or(local_state.appearance.mode);
        let gnubg_bin = app_ctx
            .as_ref()
            .map(|c| c.bootstrap.gnubg_binary.clone())
            .unwrap_or_else(|| "gnubg".into());

        let base_config =
            default_profile_configuration(locale, palette_id, mode, &local_state.board_theme);
        let base_host = default_host_settings(&gnubg_bin);
        let draft = SettingsDraft::new(base_config.clone(), 1, base_host.clone(), None);

        let player_name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(draft.draft_config.general.player_name.clone())
        });
        let gnubg_path_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(draft.draft_host.gnubg_path.clone())
        });
        let sage_path_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("/path/to/neongammon/venv/bin/python")
        });
        let profile_name_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(I18n::t(locale, "profile", "name")));

        let this_weak = cx.weak_entity();
        let make_threshold_input = |id: &str,
                                    initial: String,
                                    field: &'static str,
                                    window: &mut Window,
                                    cx: &mut Context<Self>| {
            let weak = this_weak.clone();
            cx.new(|cx| {
                DecimalInput::new(
                    DecimalInputConfig::new(id, 0.005)
                        .min(0.001)
                        .max(1.0)
                        .separator(sep)
                        .width(104.0),
                    initial,
                    Some(Rc::new(move |value, _, cx| {
                        let val = value.to_string();
                        let _ = weak.update(cx, |this, cx| {
                            match field {
                                "questionable" => this.draft.thresholds.questionable = val,
                                "error" => this.draft.thresholds.error = val,
                                "blunder" => this.draft.thresholds.blunder = val,
                                "large_blunder" => this.draft.thresholds.large_blunder = val,
                                _ => {}
                            }
                            this.draft.sync_parsed_thresholds();
                            this.save_error = None;
                            cx.notify();
                        });
                    })),
                    window,
                    cx,
                )
            })
        };

        let threshold_inputs = ThresholdInputEntities {
            questionable: make_threshold_input(
                "settings-threshold-questionable",
                draft.thresholds.questionable.clone(),
                "questionable",
                window,
                cx,
            ),
            error: make_threshold_input(
                "settings-threshold-error",
                draft.thresholds.error.clone(),
                "error",
                window,
                cx,
            ),
            blunder: make_threshold_input(
                "settings-threshold-blunder",
                draft.thresholds.blunder.clone(),
                "blunder",
                window,
                cx,
            ),
            large_blunder: make_threshold_input(
                "settings-threshold-large-blunder",
                draft.thresholds.large_blunder.clone(),
                "large_blunder",
                window,
                cx,
            ),
        };

        let weak_tutor = this_weak.clone();
        let tutor_threshold_input = cx.new(|cx| {
            DecimalInput::new(
                DecimalInputConfig::new("settings-tutor-threshold", 0.005)
                    .min(0.0)
                    .max(1.0)
                    .separator(sep)
                    .width(104.0),
                draft.tutor_threshold_str.clone(),
                Some(Rc::new(move |value, _, cx| {
                    let val = value.to_string();
                    let _ = weak_tutor.update(cx, |this, cx| {
                        this.draft.tutor_threshold_str = val;
                        this.draft.sync_parsed_thresholds();
                        this.save_error = None;
                        cx.notify();
                    });
                })),
                window,
                cx,
            )
        });

        let atlas = app_ctx
            .as_ref()
            .map(|c| Arc::clone(&c.board_atlas))
            .unwrap_or_else(|| Arc::new(Mutex::new(ThemeAtlasCache::default())));
        let bgblitz_themes = app_ctx
            .as_ref()
            .and_then(|c| c.bootstrap.custom_themes_dir.as_deref())
            .and_then(|dir| discover_bgblitz_themes(dir).ok())
            .unwrap_or_default();

        let preview_board = cx.new(|cx| {
            let mut board = BoardView::new("settings-board-preview", atlas);
            board.set_theme_config(
                appearance_to_board_config(&draft.draft_config.appearance),
                cx,
            );
            board.set_position(
                BoardPosition {
                    dice: vec![3, 1],
                    ..BoardPosition::default()
                },
                cx,
            );
            board
        });

        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe_in(
            &player_name_input,
            window,
            |this, state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.draft.draft_config.general.player_name =
                        state.read(cx).value().to_string();
                    this.save_error = None;
                    cx.notify();
                }
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &gnubg_path_input,
            window,
            |this, state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let val = state.read(cx).value().to_string();
                    if val != this.draft.draft_host.gnubg_path {
                        this.draft.draft_host.gnubg_path = val;
                        this.draft.gnubg_edited = true;
                        this.draft.gnubg_checked_version = None;
                        this.gnubg_check_error = None;
                        this.save_error = None;
                        cx.notify();
                    }
                }
            },
        ));

        let mut view = Self {
            facade,
            persist_path,
            category: initial_category,
            draft,
            engines: Vec::new(),
            sage_installation: None,
            virtual_profiles: Vec::new(),
            profiles: Vec::new(),
            active_profile_id: None,
            active_profile_name: SharedString::from("Standard"),
            active_profile_revision: 1,
            profile_differences: Vec::new(),
            bgblitz_themes,
            player_name_input,
            gnubg_path_input,
            sage_path_input,
            profile_name_input,
            threshold_inputs,
            tutor_threshold_input,
            preview_board,
            busy,
            loading: false,
            saving: false,
            checking_gnubg: false,
            verifying_sage: false,
            save_error: None,
            conflict: false,
            gnubg_check_error: None,
            sage_error: None,
            profile_error: None,
            profile_conflict: false,
            on_applied,
            _subscriptions: subscriptions,
        };

        if view.facade.is_some() {
            view.load_from_backend(window, cx);
        }
        view
    }

    pub fn category(&self) -> SettingsCategory {
        self.category
    }

    pub fn draft(&self) -> &SettingsDraft {
        &self.draft
    }

    pub fn is_dirty(&self) -> bool {
        self.draft.is_dirty()
    }

    fn sync_inputs_from_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let player_name = self.draft.draft_config.general.player_name.clone();
        self.player_name_input
            .update(cx, |s, cx| s.set_value(player_name, window, cx));

        let gnubg_path = self.draft.draft_host.gnubg_path.clone();
        self.gnubg_path_input
            .update(cx, |s, cx| s.set_value(gnubg_path, window, cx));

        let q = self.draft.thresholds.questionable.clone();
        let e = self.draft.thresholds.error.clone();
        let b = self.draft.thresholds.blunder.clone();
        let lb = self.draft.thresholds.large_blunder.clone();
        let tutor = self.draft.tutor_threshold_str.clone();

        self.threshold_inputs
            .questionable
            .update(cx, |i, cx| i.set_canonical_value(&q, window, cx));
        self.threshold_inputs
            .error
            .update(cx, |i, cx| i.set_canonical_value(&e, window, cx));
        self.threshold_inputs
            .blunder
            .update(cx, |i, cx| i.set_canonical_value(&b, window, cx));
        self.threshold_inputs
            .large_blunder
            .update(cx, |i, cx| i.set_canonical_value(&lb, window, cx));
        self.tutor_threshold_input
            .update(cx, |i, cx| i.set_canonical_value(&tutor, window, cx));

        let board_cfg = appearance_to_board_config(&self.draft.draft_config.appearance);
        self.preview_board
            .update(cx, |board, cx| board.set_theme_config(board_cfg, cx));
    }

    /// Loads configuration, host settings, engine catalog, NeonGammon installation,
    /// named profiles, and virtual player profiles in parallel via `TokioBridge`.
    pub fn load_from_backend(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        self.loading = true;
        self.save_error = None;
        self.conflict = false;
        self.profile_conflict = false;
        cx.notify();

        let task = TokioBridge::spawn_read(cx, async move {
            let (cfg, settings, engines, sage, profiles, virtuals, my_profile) = tokio::join!(
                facade.get_configuration(),
                facade.get_settings(),
                facade.list_engines(),
                facade.get_open_sage(),
                facade.list_profiles(),
                facade.list_virtual_profiles(),
                facade.get_my_player_profile(),
            );
            (cfg, settings, engines, sage, profiles, virtuals, my_profile)
        });

        cx.spawn_in(window, async move |this, cx| {
            let Ok((cfg_res, host_res, eng_res, sage_res, prof_res, vp_res, my_res)) = task.await
            else {
                return;
            };
            let _ = this.update_in(cx, |view, window, cx| {
                view.loading = false;
                if let Ok(engines) = eng_res {
                    view.engines = engines;
                }
                if let Ok(sage) = sage_res {
                    view.sage_path_input.update(cx, |inp, cx| {
                        inp.set_value(sage.python_path.clone(), window, cx);
                    });
                    view.sage_installation = Some(sage);
                }
                if let Ok(profs) = prof_res {
                    view.profiles = profs;
                }
                if let Ok(vps) = vp_res {
                    view.virtual_profiles = vps.items;
                }
                let my_id = my_res.ok().and_then(|m| m.profile.map(|p| p.id));

                match (cfg_res, host_res) {
                    (Ok(cfg_state), Ok(host_settings)) => {
                        view.active_profile_id = Some(cfg_state.active_profile.id);
                        view.active_profile_name =
                            SharedString::from(cfg_state.active_profile.name.clone());
                        view.active_profile_revision = cfg_state.active_profile.revision;
                        view.profile_differences = cfg_state.differences.clone();
                        view.draft = SettingsDraft::new(
                            cfg_state.current_configuration,
                            cfg_state.revision,
                            host_settings,
                            my_id,
                        );
                        view.sync_inputs_from_draft(window, cx);
                    }
                    (Err(err), _) | (_, Err(err)) => {
                        let locale = current_locale(cx);
                        view.save_error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Requests switching category or closing Settings; opens stacked
    /// `SettingsDraftGuardDialog` whenever unapplied edits exist across any of
    /// the 3 save destinations.
    pub fn request_destination(
        &mut self,
        destination: GuardedDestination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.saving || self.verifying_sage {
            return;
        }
        if !self.draft.is_dirty() {
            self.continue_to(destination, window, cx);
            return;
        }
        let locale = current_locale(cx);
        let hint = self
            .draft
            .dirty_destinations()
            .hint(locale)
            .map(SharedString::from);
        let can_apply = self.draft.can_apply(self.saving);
        let ent_discard = cx.entity();
        let ent_apply = cx.entity();

        open_settings_draft_guard_dialog(
            window,
            cx,
            self.busy.clone(),
            can_apply,
            hint,
            Rc::new(move |window, cx| {
                ent_discard.update(cx, |view, cx| {
                    view.draft.discard();
                    view.sync_inputs_from_draft(window, cx);
                    view.continue_to(destination, window, cx);
                });
            }),
            Rc::new(move |window, cx| {
                ent_apply.update(cx, |view, cx| {
                    view.apply_draft(Some(destination), window, cx);
                });
            }),
        );
    }

    fn continue_to(
        &mut self,
        destination: GuardedDestination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match destination {
            GuardedDestination::Close | GuardedDestination::Benchmark => {
                window.close_dialog(cx);
            }
            GuardedDestination::Profiles => {
                self.category = SettingsCategory::Profiles;
                self.recompute_profile_differences();
                cx.notify();
            }
            GuardedDestination::OpeningBook => {
                let book = &self.draft.draft_config.opening_book;
                let provider = match book.provider {
                    ConfigurationBookProvider::Off => "off",
                    ConfigurationBookProvider::Automatic => "automatic",
                    ConfigurationBookProvider::Backgammondb => "backgammondb",
                    ConfigurationBookProvider::Xg => "xg",
                };
                open_opening_book_dialog(
                    window,
                    cx,
                    self.facade.clone(),
                    provider.to_string(),
                    book.path.clone(),
                    book.xg_path.clone(),
                );
            }
        }
    }

    fn recompute_profile_differences(&mut self) {
        if let Some(active) = self
            .profiles
            .iter()
            .find(|p| Some(p.id) == self.active_profile_id)
        {
            self.profile_differences = differences(&active.configuration, &self.draft.draft_config);
        }
    }

    /// Applies unapplied edits across whichever of the **3 save destinations**
    /// are dirty, then updates live theme, locale, local state, and shell menus.
    pub fn apply_draft(
        &mut self,
        continue_after: Option<GuardedDestination>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.draft.sync_parsed_thresholds();
        if !self.draft.can_apply(self.saving) {
            return;
        }
        let destinations = self.draft.dirty_destinations();

        if let Some(facade) = self.facade.clone() {
            self.saving = true;
            self.busy.set_busy(true);
            self.save_error = None;
            self.conflict = false;
            cx.notify();

            let update_cfg = destinations.profile_config.then(|| UpdateConfiguration {
                expected_revision: self.draft.config_revision,
                configuration: self.draft.draft_config.clone(),
            });
            let host_payload = destinations
                .host_engines
                .then(|| self.draft.host_settings_payload());
            let my_profile_update = destinations
                .my_player_profile
                .then_some(self.draft.draft_my_player_profile);

            let task = TokioBridge::spawn_write(cx, async move {
                let saved_cfg = if let Some(input) = update_cfg {
                    Some(facade.save_configuration(input).await?)
                } else {
                    None
                };
                let saved_host = if let Some(settings) = host_payload {
                    Some(facade.save_settings(settings).await?)
                } else {
                    None
                };
                let saved_my = if let Some(id) = my_profile_update {
                    Some(facade.set_my_player_profile(id).await?)
                } else {
                    None
                };
                Ok::<_, neondojo::domain_error::DomainError>((saved_cfg, saved_host, saved_my))
            });

            cx.spawn_in(window, async move |this, cx| {
                let Ok(res) = task.await else {
                    return;
                };
                let _ = this.update_in(cx, |view, window, cx| {
                    view.saving = false;
                    view.busy.set_busy(false);
                    match res {
                        Ok((saved_cfg, saved_host, saved_my)) => {
                            if let Some(cfg_state) = saved_cfg {
                                view.active_profile_id = Some(cfg_state.active_profile.id);
                                view.active_profile_name =
                                    SharedString::from(cfg_state.active_profile.name);
                                view.active_profile_revision = cfg_state.active_profile.revision;
                                view.profile_differences = cfg_state.differences;
                                view.draft.rebase_profile(
                                    cfg_state.current_configuration,
                                    cfg_state.revision,
                                );
                            }
                            if let Some(host) = saved_host {
                                view.draft.saved_host = host.clone();
                                view.draft.draft_host = host;
                                view.draft.gnubg_edited = false;
                            }
                            if let Some(my) = saved_my {
                                let id = my.profile.map(|p| p.id);
                                view.draft.saved_my_player_profile = id;
                                view.draft.draft_my_player_profile = id;
                            }
                            view.commit_live_ui_effects(window, cx);
                            if let Some(dest) = continue_after {
                                view.continue_to(dest, window, cx);
                            }
                        }
                        Err(err) => {
                            let locale = current_locale(cx);
                            view.conflict = err.kind == ErrorKind::Conflict;
                            view.save_error = Some(if view.conflict {
                                I18n::t(locale, "settings", "conflict").into()
                            } else {
                                format_domain_error(locale, &err).into()
                            });
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        } else {
            // Offline / headless test commit: rebase locally and apply live UI changes.
            self.draft.saved_config = self.draft.draft_config.clone();
            self.draft.saved_host = self.draft.draft_host.clone();
            self.draft.saved_my_player_profile = self.draft.draft_my_player_profile;
            self.draft.gnubg_edited = false;
            self.commit_live_ui_effects(window, cx);
            if let Some(dest) = continue_after {
                self.continue_to(dest, window, cx);
            }
            cx.notify();
        }
    }

    fn commit_live_ui_effects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let locale = config_locale_to_locale(&self.draft.draft_config.general.locale);
        let palette_id = ui_theme_to_palette_id(&self.draft.draft_config.appearance.ui_theme);
        let mode = config_mode_to_theme_mode(&self.draft.draft_config.appearance.mode);
        let board_theme = self.draft.draft_config.appearance.board_theme.clone();
        let system_dark = ResolvedMode::from_window_appearance(window.appearance()).is_dark();

        set_locale(locale, cx);
        NeonTheme::set(palette_id, mode, system_dark, cx);

        // Update DecimalInput decimal separators when language switched
        let sep = locale.decimal_separator();
        self.threshold_inputs
            .questionable
            .update(cx, |i, cx| i.set_separator(sep, window, cx));
        self.threshold_inputs
            .error
            .update(cx, |i, cx| i.set_separator(sep, window, cx));
        self.threshold_inputs
            .blunder
            .update(cx, |i, cx| i.set_separator(sep, window, cx));
        self.threshold_inputs
            .large_blunder
            .update(cx, |i, cx| i.set_separator(sep, window, cx));
        self.tutor_threshold_input
            .update(cx, |i, cx| i.set_separator(sep, window, cx));

        // Persist LocalUiState atomically if configured
        if let Some(mut state) = cx.try_global::<LocalUiState>().cloned() {
            state.locale = locale.code().to_string();
            state.appearance.theme = palette_id;
            state.appearance.mode = mode;
            state.board_theme = board_theme.clone();
            state.play_sounds = self.draft.draft_config.general.play_sounds;
            if let Some(path) = &self.persist_path {
                let _ = state.save_atomic(path);
            }
            cx.set_global(state);
        }

        if let Some(cb) = self.on_applied.clone() {
            cb(
                AppliedSettingsEvent {
                    locale,
                    palette_id,
                    mode,
                    board_theme,
                    active_profile_name: self.active_profile_name.clone(),
                    profile_modified: !self.profile_differences.is_empty(),
                },
                window,
                cx,
            );
        }
    }

    fn check_gnubg(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let path = self.draft.draft_host.gnubg_path.clone();
        let threads = self.draft.draft_host.gnubg_threads;
        self.checking_gnubg = true;
        self.gnubg_check_error = None;
        cx.notify();

        let task =
            TokioBridge::spawn_read(cx, async move { facade.check_gnubg(path, threads).await });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |view, _, cx| {
                view.checking_gnubg = false;
                match res {
                    Ok(ok) => {
                        view.draft.gnubg_checked_version = Some(ok.version);
                        view.draft.gnubg_edited = false;
                    }
                    Err(err) => {
                        let locale = current_locale(cx);
                        view.gnubg_check_error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn verify_sage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let path = self.sage_path_input.read(cx).value().to_string();
        let version = self
            .sage_installation
            .as_ref()
            .map(|s| s.version)
            .unwrap_or(1);
        self.verifying_sage = true;
        self.busy.set_busy(true);
        self.sage_error = None;
        cx.notify();

        let task =
            TokioBridge::spawn_write(
                cx,
                async move { facade.verify_open_sage(version, path).await },
            );
        cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |view, _, cx| {
                view.verifying_sage = false;
                view.busy.set_busy(false);
                match res {
                    Ok(inst) => view.sage_installation = Some(inst),
                    Err(err) => {
                        let locale = current_locale(cx);
                        view.sage_error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn create_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.profile_name_input.read(cx).value().trim().to_string();
        if name.is_empty() || name.chars().count() > 80 {
            let locale = current_locale(cx);
            self.profile_error = Some(I18n::t(locale, "profile", "invalidName").into());
            cx.notify();
            return;
        }
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let input = CreateConfigurationProfile {
            name,
            expected_state_revision: self.draft.config_revision,
            configuration: self.draft.draft_config.clone(),
        };
        self.run_profile_mutation(
            window,
            cx,
            async move { facade.create_profile(input).await },
        );
    }

    fn activate_profile(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let Some(target) = self.profiles.iter().find(|p| p.id == id) else {
            return;
        };
        let input = ActivateConfigurationProfile {
            expected_state_revision: self.draft.config_revision,
            expected_target_revision: target.revision,
            expected_source_revision: self.active_profile_revision,
            source_action: ProfileSourceAction::Keep,
            current_configuration: self.draft.draft_config.clone(),
        };
        self.run_profile_mutation(window, cx, async move {
            facade.activate_profile(id, input).await
        });
    }

    fn overwrite_profile(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let Some(target) = self.profiles.iter().find(|p| p.id == id) else {
            return;
        };
        let input = SaveConfigurationProfile {
            expected_state_revision: self.draft.config_revision,
            expected_profile_revision: target.revision,
            configuration: self.draft.draft_config.clone(),
        };
        self.run_profile_mutation(window, cx, async move {
            facade.save_profile_snapshot(id, input).await
        });
    }

    fn rename_profile(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.profile_name_input.read(cx).value().trim().to_string();
        if name.is_empty() || name.chars().count() > 80 {
            let locale = current_locale(cx);
            self.profile_error = Some(I18n::t(locale, "profile", "invalidName").into());
            cx.notify();
            return;
        }
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let Some(target) = self.profiles.iter().find(|p| p.id == id) else {
            return;
        };
        let input = RenameConfigurationProfile {
            name,
            expected_state_revision: self.draft.config_revision,
            expected_profile_revision: target.revision,
        };
        self.run_profile_mutation(
            window,
            cx,
            async move { facade.update_profile(id, input).await },
        );
    }

    fn delete_profile(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let Some(target) = self.profiles.iter().find(|p| p.id == id) else {
            return;
        };
        let input = DeleteConfigurationProfile {
            expected_state_revision: self.draft.config_revision,
            expected_profile_revision: target.revision,
        };
        self.run_profile_mutation(
            window,
            cx,
            async move { facade.delete_profile(id, input).await },
        );
    }

    fn restore_active_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone() else {
            return;
        };
        let input = RestoreConfiguration {
            expected_state_revision: self.draft.config_revision,
            expected_profile_revision: self.active_profile_revision,
        };
        self.run_profile_mutation(window, cx, async move {
            facade.restore_configuration(input).await
        });
    }

    fn run_profile_mutation<F>(&mut self, window: &mut Window, cx: &mut Context<Self>, fut: F)
    where
        F: std::future::Future<
                Output = Result<
                    neondojo::configuration::ConfigurationState,
                    neondojo::domain_error::DomainError,
                >,
            > + Send
            + 'static,
    {
        self.saving = true;
        self.busy.set_busy(true);
        self.profile_error = None;
        self.profile_conflict = false;
        cx.notify();

        let facade = self.facade.clone();
        let task = TokioBridge::spawn_write(cx, async move {
            let state = fut.await?;
            let list = if let Some(f) = facade {
                f.list_profiles().await.ok()
            } else {
                None
            };
            Ok::<_, neondojo::domain_error::DomainError>((state, list))
        });

        cx.spawn_in(window, async move |this, cx| {
            let Ok(res) = task.await else {
                return;
            };
            let _ = this.update_in(cx, |view, window, cx| {
                view.saving = false;
                view.busy.set_busy(false);
                match res {
                    Ok((cfg_state, maybe_list)) => {
                        view.active_profile_id = Some(cfg_state.active_profile.id);
                        view.active_profile_name =
                            SharedString::from(cfg_state.active_profile.name);
                        view.active_profile_revision = cfg_state.active_profile.revision;
                        view.profile_differences = cfg_state.differences;
                        if let Some(list) = maybe_list {
                            view.profiles = list;
                        }
                        view.draft
                            .rebase_profile(cfg_state.current_configuration, cfg_state.revision);
                        view.sync_inputs_from_draft(window, cx);
                        view.commit_live_ui_effects(window, cx);
                    }
                    Err(err) => {
                        let locale = current_locale(cx);
                        view.profile_conflict = err.kind == ErrorKind::Conflict;
                        view.profile_error = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for SettingsDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = NeonTheme::current_palette(cx);
        let system_dark = ResolvedMode::from_window_appearance(window.appearance()).is_dark();
        let apply_label = I18n::t(locale, "profile", "apply");
        let disabled = self.saving || self.verifying_sage;
        let entity = cx.entity();

        let nav_items: Vec<TextNavItem> = SettingsCategory::ALL
            .iter()
            .map(|&cat| {
                TextNavItem::new(cat.id(), cat.label(locale))
                    .element_id(format!("settings-cat-{}", cat.id()))
                    .disabled(disabled)
            })
            .collect();

        let sidebar = div()
            .w(px(164.0))
            .flex_shrink_0()
            .pr(px(12.0))
            .border_r_1()
            .border_color(p.border)
            .child(
                TextNavBar::new("settings-category-nav", nav_items)
                    .active(Some(self.category.id()))
                    .orientation(TextNavOrientation::Vertical)
                    .density(TextNavDensity::Inline)
                    .palette(p)
                    .on_select({
                        let ent = entity.clone();
                        move |id, window, cx| {
                            if let Some(next) = SettingsCategory::from_id(id) {
                                ent.update(cx, |view, cx| {
                                    if next == SettingsCategory::Profiles {
                                        view.request_destination(
                                            GuardedDestination::Profiles,
                                            window,
                                            cx,
                                        );
                                    } else {
                                        view.category = next;
                                        cx.notify();
                                    }
                                });
                            }
                        }
                    }),
            );

        let content = match self.category {
            SettingsCategory::General => render_general_section(
                &self.draft,
                apply_label,
                &self.player_name_input,
                &self.virtual_profiles,
                disabled,
                p,
                Rc::new({
                    let ent = entity.clone();
                    move |lang, _, cx| {
                        ent.update(cx, |v, cx| {
                            v.draft.draft_config.general.locale = locale_to_config_locale(lang);
                            v.save_error = None;
                            cx.notify();
                        });
                    }
                }),
                Rc::new({
                    let ent = entity.clone();
                    move |id, _, cx| {
                        ent.update(cx, |v, cx| {
                            v.draft.draft_my_player_profile = id;
                            v.save_error = None;
                            cx.notify();
                        });
                    }
                }),
                cx,
            )
            .into_any_element(),

            SettingsCategory::Appearance => render_appearance_section(
                ui_theme_to_palette_id(&self.draft.draft_config.appearance.ui_theme),
                config_mode_to_theme_mode(&self.draft.draft_config.appearance.mode),
                system_dark,
                apply_label,
                disabled,
                p,
                Rc::new({
                    let ent = entity.clone();
                    move |pid, _, cx| {
                        ent.update(cx, |v, cx| {
                            v.draft.draft_config.appearance.ui_theme = palette_id_to_ui_theme(pid);
                            v.save_error = None;
                            cx.notify();
                        });
                    }
                }),
                Rc::new({
                    let ent = entity.clone();
                    move |mode, _, cx| {
                        ent.update(cx, |v, cx| {
                            v.draft.draft_config.appearance.mode = theme_mode_to_config_mode(mode);
                            v.save_error = None;
                            cx.notify();
                        });
                    }
                }),
                cx,
            )
            .into_any_element(),

            SettingsCategory::Analysis => render_analysis_section(
                &self.draft,
                &self.engines,
                &self.threshold_inputs,
                disabled,
                p,
                Rc::new({
                    let ent = entity.clone();
                    move |engine, _, cx| {
                        ent.update(cx, |v, cx| {
                            let id = match engine {
                                ConfigurationEngine::Gnubg => "gnubg",
                                ConfigurationEngine::OpenSage => "bgsage",
                            };
                            let allowed = engine_depths(id, "analysis");
                            if !allowed.contains(&v.draft.draft_config.analysis.plies)
                                && let Some(&first) = allowed.first()
                            {
                                v.draft.draft_config.analysis.plies = first;
                            }
                            v.draft.draft_config.analysis.engine = engine;
                            cx.notify();
                        });
                    }
                }),
                Rc::new({
                    let ent = entity.clone();
                    move |ply, _, cx| {
                        ent.update(cx, |v, cx| {
                            v.draft.draft_config.analysis.plies = ply;
                            cx.notify();
                        });
                    }
                }),
                Rc::new({
                    let ent = entity.clone();
                    move |scope, _, cx| {
                        ent.update(cx, |v, cx| {
                            v.draft.draft_config.analysis.scope = scope;
                            cx.notify();
                        });
                    }
                }),
                Rc::new({
                    let ent = entity.clone();
                    move |window, cx| {
                        ent.update(cx, |v, cx| {
                            v.request_destination(GuardedDestination::OpeningBook, window, cx);
                        });
                    }
                }),
                cx,
            )
            .into_any_element(),

            SettingsCategory::Play => render_playing_section(
                &self.draft,
                &self.engines,
                &self.tutor_threshold_input,
                disabled,
                p,
                PlaySectionCallbacks {
                    on_engine: Rc::new({
                        let ent = entity.clone();
                        move |engine, _, cx| {
                            ent.update(cx, |v, cx| {
                                let id = match engine {
                                    ConfigurationEngine::Gnubg => "gnubg",
                                    ConfigurationEngine::OpenSage => "bgsage",
                                };
                                let allowed = engine_depths(id, "play");
                                if !allowed.contains(&v.draft.draft_config.play.plies)
                                    && let Some(&first) = allowed.first()
                                {
                                    v.draft.draft_config.play.plies = first;
                                }
                                v.draft.draft_config.play.engine = engine;
                                v.draft.draft_config.play.top_two = false;
                                cx.notify();
                            });
                        }
                    }),
                    on_ply: Rc::new({
                        let ent = entity.clone();
                        move |ply, _, cx| {
                            ent.update(cx, |v, cx| {
                                v.draft.draft_config.play.plies = ply;
                                cx.notify();
                            });
                        }
                    }),
                    on_top_two: Rc::new({
                        let ent = entity.clone();
                        move |top_two, _, cx| {
                            ent.update(cx, |v, cx| {
                                v.draft.draft_config.play.top_two = top_two;
                                cx.notify();
                            });
                        }
                    }),
                    on_mode: Rc::new({
                        let ent = entity.clone();
                        move |mode, _, cx| {
                            ent.update(cx, |v, cx| {
                                v.draft.draft_config.play.default_mode = mode;
                                cx.notify();
                            });
                        }
                    }),
                    on_auto_analysis: Rc::new({
                        let ent = entity.clone();
                        move |auto, _, cx| {
                            ent.update(cx, |v, cx| {
                                v.draft.draft_config.play.training_auto_analysis = auto;
                                cx.notify();
                            });
                        }
                    }),
                    on_human_player: Rc::new({
                        let ent = entity.clone();
                        move |human, _, cx| {
                            ent.update(cx, |v, cx| {
                                v.draft.draft_config.play.human_player = human;
                                cx.notify();
                            });
                        }
                    }),
                    on_match_length: Rc::new({
                        let ent = entity.clone();
                        move |len, _, cx| {
                            ent.update(cx, |v, cx| {
                                v.draft.draft_config.play.match_length = len;
                                cx.notify();
                            });
                        }
                    }),
                    on_sounds: Rc::new({
                        let ent = entity.clone();
                        move |sounds, _, cx| {
                            ent.update(cx, |v, cx| {
                                v.draft.draft_config.general.play_sounds = sounds;
                                cx.notify();
                            });
                        }
                    }),
                    on_direction: Rc::new({
                        let ent = entity.clone();
                        move |dir: ConfigurationDirection, _, cx| {
                            ent.update(cx, |v, cx| {
                                v.draft.draft_config.appearance.bearoff_direction = dir;
                                cx.notify();
                            });
                        }
                    }),
                },
                cx,
            )
            .into_any_element(),

            SettingsCategory::Board => render_board_section(
                &self.draft.draft_config.appearance,
                &self.preview_board,
                &self.bgblitz_themes,
                disabled,
                p,
                BoardSectionCallbacks {
                    on_board_theme: Rc::new({
                        let ent = entity.clone();
                        move |theme, _, cx| {
                            ent.update(cx, |v, cx| {
                                v.draft.draft_config.appearance.board_theme = theme;
                                let cfg =
                                    appearance_to_board_config(&v.draft.draft_config.appearance);
                                v.preview_board
                                    .update(cx, |b, cx| b.set_theme_config(cfg, cx));
                                cx.notify();
                            });
                        }
                    }),
                    on_checker_theme: Rc::new({
                        let ent = entity.clone();
                        move |theme, _, cx| {
                            ent.update(cx, |v, cx| {
                                v.draft.draft_config.appearance.checker_theme = theme;
                                let cfg =
                                    appearance_to_board_config(&v.draft.draft_config.appearance);
                                v.preview_board
                                    .update(cx, |b, cx| b.set_theme_config(cfg, cx));
                                cx.notify();
                            });
                        }
                    }),
                    on_cube_theme: Rc::new({
                        let ent = entity.clone();
                        move |theme, _, cx| {
                            ent.update(cx, |v, cx| {
                                v.draft.draft_config.appearance.cube_theme = theme;
                                let cfg =
                                    appearance_to_board_config(&v.draft.draft_config.appearance);
                                v.preview_board
                                    .update(cx, |b, cx| b.set_theme_config(cfg, cx));
                                cx.notify();
                            });
                        }
                    }),
                    on_dice_theme: Rc::new({
                        let ent = entity.clone();
                        move |theme, _, cx| {
                            ent.update(cx, |v, cx| {
                                v.draft.draft_config.appearance.dice_theme = theme;
                                let cfg =
                                    appearance_to_board_config(&v.draft.draft_config.appearance);
                                v.preview_board
                                    .update(cx, |b, cx| b.set_theme_config(cfg, cx));
                                cx.notify();
                            });
                        }
                    }),
                },
                cx,
            )
            .into_any_element(),

            SettingsCategory::Engines => render_engines_section(
                &self.draft,
                &self.gnubg_path_input,
                &self.sage_path_input,
                self.sage_installation.as_ref(),
                self.checking_gnubg,
                self.verifying_sage,
                self.gnubg_check_error.as_deref(),
                self.sage_error.as_deref(),
                disabled,
                p,
                EngineSectionCallbacks {
                    on_check_gnubg: Rc::new({
                        let ent = entity.clone();
                        move |window, cx| ent.update(cx, |v, cx| v.check_gnubg(window, cx))
                    }),
                    on_verify_sage: Rc::new({
                        let ent = entity.clone();
                        move |window, cx| ent.update(cx, |v, cx| v.verify_sage(window, cx))
                    }),
                    on_concurrency: Rc::new({
                        let ent = entity.clone();
                        move |field, value, _, cx| {
                            ent.update(cx, |v, cx| {
                                match field {
                                    HostConcurrencyField::GnubgAnalysis => {
                                        v.draft.draft_host.gnubg_threads = value;
                                    }
                                    HostConcurrencyField::GnubgPlay => {
                                        v.draft.draft_host.gnubg_play_threads = value;
                                    }
                                    HostConcurrencyField::SageAnalysis => {
                                        v.draft.draft_host.open_sage_threads = value;
                                    }
                                    HostConcurrencyField::SagePlay => {
                                        v.draft.draft_host.open_sage_play_threads = value;
                                    }
                                    HostConcurrencyField::ParallelJobs => {
                                        v.draft.draft_host.parallel_analysis_jobs = value;
                                    }
                                }
                                cx.notify();
                            });
                        }
                    }),
                },
                cx,
            )
            .into_any_element(),

            SettingsCategory::Profiles => render_profiles_section(
                &self.profiles,
                self.active_profile_id,
                &self.profile_differences,
                &self.profile_name_input,
                self.profile_conflict,
                self.profile_error.as_deref(),
                disabled,
                p,
                ProfileSectionCallbacks {
                    on_create: Rc::new({
                        let ent = entity.clone();
                        move |window, cx| ent.update(cx, |v, cx| v.create_profile(window, cx))
                    }),
                    on_activate: Rc::new({
                        let ent = entity.clone();
                        move |id, window, cx| {
                            ent.update(cx, |v, cx| v.activate_profile(id, window, cx));
                        }
                    }),
                    on_overwrite: Rc::new({
                        let ent = entity.clone();
                        move |id, window, cx| {
                            ent.update(cx, |v, cx| v.overwrite_profile(id, window, cx));
                        }
                    }),
                    on_rename: Rc::new({
                        let ent = entity.clone();
                        move |id, window, cx| {
                            ent.update(cx, |v, cx| v.rename_profile(id, window, cx));
                        }
                    }),
                    on_delete: Rc::new({
                        let ent = entity.clone();
                        move |id, window, cx| {
                            ent.update(cx, |v, cx| v.delete_profile(id, window, cx));
                        }
                    }),
                    on_restore_all: Rc::new({
                        let ent = entity.clone();
                        move |window, cx| {
                            ent.update(cx, |v, cx| v.restore_active_profile(window, cx));
                        }
                    }),
                    on_reload_server: Rc::new({
                        let ent = entity.clone();
                        move |window, cx| {
                            ent.update(cx, |v, cx| v.load_from_backend(window, cx));
                        }
                    }),
                },
                cx,
            )
            .into_any_element(),
        };

        let destinations = self.draft.dirty_destinations();
        let dirty_hint = destinations.hint(locale);
        let can_apply = self.draft.can_apply(self.saving);

        div()
            .id("settings-modal-body")
            .test_support()
            .v_flex()
            .gap(px(14.0))
            .w_full()
            .child(
                div()
                    .h_flex()
                    .items_start()
                    .gap(px(18.0))
                    .min_h(px(430.0))
                    .max_h(px(560.0))
                    .child(sidebar)
                    .child(
                        div()
                            .id("settings-content-scroll")
                            .v_flex()
                            .flex_1()
                            .min_w(px(0.0))
                            .max_h(px(560.0))
                            .overflow_y_scroll()
                            .pr(px(4.0))
                            .when(self.loading, |this| {
                                this.child(
                                    div()
                                        .text_body()
                                        .text_color(p.muted)
                                        .child(I18n::t(locale, "settings", "loading")),
                                )
                            })
                            .when(!self.loading, |this| this.child(content)),
                    ),
            )
            .when_some(self.save_error.clone(), |this, err| {
                let ent_reload = entity.clone();
                let mut banner =
                    StatusBanner::new("settings-save-error", BannerKind::Error, err).palette(p);
                if self.conflict {
                    banner = banner.on_retry(
                        I18n::t(locale, "settings", "loadLatest"),
                        move |_, window, cx| {
                            ent_reload.update(cx, |v, cx| v.load_from_backend(window, cx));
                        },
                    );
                }
                this.child(banner)
            })
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .pt(px(12.0))
                    .border_t_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .v_flex()
                            .gap(px(2.0))
                            .flex_1()
                            .min_w(px(0.0))
                            .when(destinations.any(), |this| {
                                this.child(
                                    div()
                                        .text_label()
                                        .text_color(p.accent)
                                        .child(I18n::t(locale, "settings", "unsaved")),
                                )
                            })
                            .when_some(dirty_hint, |this, hint| {
                                this.child(div().text_meta().text_color(p.muted).child(hint))
                            }),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                ActionButton::new("settings-modal-done")
                                    .icon(ActionIconName::Close)
                                    .label(I18n::t(locale, "settings", "done"))
                                    .disabled(disabled)
                                    .palette(p)
                                    .on_click({
                                        let ent = entity.clone();
                                        move |_, window, cx| {
                                            ent.update(cx, |v, cx| {
                                                v.request_destination(
                                                    GuardedDestination::Close,
                                                    window,
                                                    cx,
                                                );
                                            });
                                        }
                                    }),
                            )
                            .when(
                                self.category != SettingsCategory::Profiles
                                    && (destinations.any() || self.saving),
                                |this| {
                                    let ent = entity.clone();
                                    this.child(
                                        ActionButton::new("settings-modal-apply")
                                            .icon(ActionIconName::Save)
                                            .label(if self.saving {
                                                I18n::t(locale, "settings", "saving")
                                            } else {
                                                apply_label
                                            })
                                            .primary()
                                            .disabled(!can_apply)
                                            .palette(p)
                                            .on_click(move |_, window, cx| {
                                                ent.update(cx, |v, cx| {
                                                    v.apply_draft(None, window, cx);
                                                });
                                            }),
                                    )
                                },
                            ),
                    ),
            )
    }
}

/// Opens the full Settings & Profiles dialog inside `WindowExt::open_dialog`,
/// returning the persistent [`Entity<SettingsDialogView>`] handle.
#[allow(clippy::too_many_arguments)]
pub fn open_settings_dialog(
    window: &mut Window,
    cx: &mut App,
    initial_category: SettingsCategory,
    facade: Option<BackendFacade>,
    persist_path: Option<PathBuf>,
    busy: ModalBusyLock,
    on_applied: Option<OnSettingsApplied>,
) -> Entity<SettingsDialogView> {
    let locale = current_locale(cx);
    let view = cx.new(|cx| {
        SettingsDialogView::new(
            initial_category,
            facade,
            persist_path,
            busy.clone(),
            on_applied,
            window,
            cx,
        )
    });

    let config = ModalConfig::new(I18n::t(locale, "settings", "title"))
        .subtitle(I18n::t(locale, "settings", "description"))
        .size(ModalSize::ExtraWide)
        .busy_lock(busy);

    let view_for_body = view.clone();
    open_modal_dialog(
        window,
        cx,
        config,
        move |_, _| view_for_body.clone().into_any_element(),
        None::<fn(&mut Window, &mut App) -> gpui_kit::AnyElement>,
        None,
    );

    view
}
