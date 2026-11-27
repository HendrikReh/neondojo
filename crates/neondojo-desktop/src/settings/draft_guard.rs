//! Settings draft state, 3-destination dirty tracking, threshold validation,
//! and stacked `SettingsDraftGuardDialog` (`useSettingsDraft.ts` &
//! `SettingsDraftGuardDialog.tsx`).

use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonPaletteId, NeonTheme, NeonTypographyExt, ThemeMode};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::icons::ActionIconName;
use crate::ui::modal::{ModalBusyLock, ModalConfig, ModalSize, open_modal_dialog};
use gpui_kit::{
    App, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, TestSupportExt,
    Window, base::StyledExt, component::WindowExt, div, px,
};
use neondojo::configuration::{
    ConfigurationAnalysis, ConfigurationAnnotations, ConfigurationAppearance,
    ConfigurationBoardView, ConfigurationBook, ConfigurationBookProvider, ConfigurationCardFilters,
    ConfigurationDirection, ConfigurationEngine, ConfigurationGeneral, ConfigurationGeneration,
    ConfigurationLearning, ConfigurationLibrary, ConfigurationLibraryFilters, ConfigurationLocale,
    ConfigurationMode, ConfigurationPlay, ConfigurationPositions, ConfigurationReview,
    ConfigurationScope, ConfigurationSearchFilter, ConfigurationSortDirection,
    ConfigurationUiTheme, ErrorThresholds, ProfileConfiguration,
};
use neondojo::play::PlayMode;
use neondojo::position_browser::PositionFilterDefinition;
use neondojo::settings::AppSettings;
use neondojo_i18n::{I18n, Locale, canonical_decimal};
use std::rc::Rc;
use uuid::Uuid;

/// The 7 vertical sidebar categories in Settings (`SettingsDialog.tsx`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsCategory {
    #[default]
    General,
    Appearance,
    Analysis,
    Play,
    Board,
    Engines,
    Profiles,
}

impl SettingsCategory {
    pub const ALL: &'static [Self] = &[
        Self::General,
        Self::Appearance,
        Self::Analysis,
        Self::Play,
        Self::Board,
        Self::Engines,
        Self::Profiles,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Appearance => "appearance",
            Self::Analysis => "analysis",
            Self::Play => "play",
            Self::Board => "board",
            Self::Engines => "engines",
            Self::Profiles => "profiles",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.id() == id)
    }

    pub fn label(self, locale: Locale) -> &'static str {
        match self {
            Self::Profiles => I18n::t(locale, "profile", "profiles"),
            Self::Appearance => I18n::t(locale, "ui-appearance", "appearance"),
            _ => I18n::t(locale, "settings", self.id()),
        }
    }
}

/// Destination guarded by [`open_settings_draft_guard_dialog`] when unapplied edits exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardedDestination {
    Close,
    Profiles,
    OpeningBook,
    Benchmark,
}

/// Tracks dirty flags independently across NeonDojo's **3 distinct save destinations**:
/// 1. Active profile configuration (`save_configuration`)
/// 2. Host-local engine paths & thread limits (`save_settings`)
/// 3. Library "My player profile" (`set_my_player_profile`)
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SaveDestinations {
    pub profile_config: bool,
    pub host_engines: bool,
    pub my_player_profile: bool,
}

impl SaveDestinations {
    pub fn any(self) -> bool {
        self.profile_config || self.host_engines || self.my_player_profile
    }

    /// Concise localized summary describing which persistence targets Apply will write.
    pub fn hint(self, locale: Locale) -> Option<String> {
        let mut parts = Vec::new();
        if self.profile_config {
            parts.push(I18n::t(locale, "settings", "applyConfiguration"));
        }
        if self.host_engines {
            parts.push(I18n::t(locale, "settings", "applyInstallation"));
        }
        if self.my_player_profile {
            parts.push(I18n::t(locale, "player-directory", "myProfileHelp"));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }
}

/// Editable string buffers for the 4 strictly-increasing error thresholds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThresholdStrings {
    pub questionable: String,
    pub error: String,
    pub blunder: String,
    pub large_blunder: String,
}

impl ThresholdStrings {
    pub fn from_thresholds(t: ErrorThresholds) -> Self {
        Self {
            questionable: format_threshold(t.questionable),
            error: format_threshold(t.error),
            blunder: format_threshold(t.blunder),
            large_blunder: format_threshold(t.large_blunder),
        }
    }

    /// Parses and validates `0 < questionable < error < blunder < large_blunder <= 1.0`.
    pub fn parse_valid(&self) -> Option<ErrorThresholds> {
        let q = parse_unit_f64(&self.questionable)?;
        let e = parse_unit_f64(&self.error)?;
        let b = parse_unit_f64(&self.blunder)?;
        let lb = parse_unit_f64(&self.large_blunder)?;
        if 0.0 < q && q < e && e < b && b < lb && lb <= 1.0 {
            Some(ErrorThresholds {
                questionable: q,
                error: e,
                blunder: b,
                large_blunder: lb,
            })
        } else {
            None
        }
    }
}

fn format_threshold(value: f64) -> String {
    let raw = format!("{value:.4}");
    raw.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn parse_unit_f64(raw: &str) -> Option<f64> {
    let canon = canonical_decimal(raw);
    if canon.is_empty() {
        return None;
    }
    let v = canon.parse::<f64>().ok()?;
    v.is_finite().then_some(v)
}

/// Builds a valid baseline [`ProfileConfiguration`] for offline/demo/test startup
/// before a PostgreSQL backend finishes loading.
pub fn default_profile_configuration(
    locale: Locale,
    palette_id: NeonPaletteId,
    mode: ThemeMode,
    board_theme: &str,
) -> ProfileConfiguration {
    ProfileConfiguration {
        schema_version: 1,
        general: ConfigurationGeneral {
            player_name: "Player".into(),
            locale: match locale {
                Locale::De => ConfigurationLocale::De,
                Locale::En => ConfigurationLocale::En,
            },
            play_sounds: true,
        },
        analysis: ConfigurationAnalysis {
            engine: ConfigurationEngine::Gnubg,
            plies: 3,
            scope: ConfigurationScope::Match,
            search_filter: ConfigurationSearchFilter::EngineDefault,
            error_thresholds: ErrorThresholds::default(),
        },
        play: ConfigurationPlay {
            engine: ConfigurationEngine::Gnubg,
            plies: 2,
            top_two: false,
            default_mode: PlayMode::Coach,
            tutor_threshold: 0.04,
            training_auto_analysis: true,
            human_player: 0,
            match_length: 7,
        },
        appearance: ConfigurationAppearance {
            ui_theme: match palette_id {
                NeonPaletteId::Graphite => ConfigurationUiTheme::Graphite,
                NeonPaletteId::Club => ConfigurationUiTheme::Club,
                NeonPaletteId::Paper => ConfigurationUiTheme::Paper,
            },
            mode: match mode {
                ThemeMode::Light => ConfigurationMode::Light,
                ThemeMode::Dark => ConfigurationMode::Dark,
                ThemeMode::System => ConfigurationMode::System,
            },
            board_theme: if board_theme.is_empty() {
                "forest".into()
            } else {
                board_theme.to_string()
            },
            checker_theme: None,
            dice_theme: Some("match-board".into()),
            cube_theme: None,
            bearoff_direction: ConfigurationDirection::Right,
            board_view: ConfigurationBoardView::Arrows,
            analysis_board_width: None,
            analysis_overview_board_width: None,
            analysis_errors_board_width: None,
            analysis_statistics_board_width: None,
            analysis_notes_board_width: None,
        },
        opening_book: ConfigurationBook {
            provider: ConfigurationBookProvider::Automatic,
            path: neondojo::opening_book::default_path(),
            xg_path: neondojo::xg_book::default_path(),
        },
        library: ConfigurationLibrary {
            database_id: String::new(),
            filters: ConfigurationLibraryFilters {
                q: String::new(),
                player: String::new(),
                date_from: String::new(),
                date_to: String::new(),
                date_field: String::new(),
                tag: String::new(),
                comment: String::new(),
                position_id: String::new(),
                tournament: String::new(),
                round: String::new(),
                match_name: String::new(),
                match_type: String::new(),
                result: String::new(),
                match_points_min: String::new(),
                match_points_max: String::new(),
            },
            sort: "imported_at".into(),
            direction: ConfigurationSortDirection::Desc,
            visible_columns: Vec::new(),
            column_order: Vec::new(),
            column_widths: None,
        },
        positions: ConfigurationPositions {
            definition: PositionFilterDefinition::default(),
            show_played: false,
            column_widths: None,
            visible_columns: Vec::new(),
        },
        learning: ConfigurationLearning {
            deck_id: "__all__".into(),
            filters: ConfigurationCardFilters {
                q: String::new(),
                player: String::new(),
                tag: String::new(),
                topics: String::new(),
                game_plan: String::new(),
                comment: String::new(),
                position_id: String::new(),
                kind: String::new(),
                status: String::new(),
            },
            generation: ConfigurationGeneration {
                min_loss: 0.08,
                player: String::new(),
            },
        },
        annotations: ConfigurationAnnotations { filters: None },
        review: ConfigurationReview {
            player: String::new(),
            kind: String::new(),
            severity: String::new(),
            errors_only: false,
        },
    }
}

/// Builds a valid baseline host [`AppSettings`] for offline/demo/test startup.
pub fn default_host_settings(gnubg_path: &str) -> AppSettings {
    AppSettings {
        default_play_mode: PlayMode::Coach,
        tutor_threshold: 0.04,
        training_auto_analysis: true,
        version: 1,
        player_name: "Player".into(),
        analysis_engine: "gnubg".into(),
        analysis_plies: 3,
        play_engine: "gnubg".into(),
        play_plies: 2,
        play_top_two: false,
        gnubg_path: gnubg_path.to_string(),
        gnubg_threads: None,
        gnubg_play_threads: None,
        open_sage_threads: None,
        open_sage_play_threads: None,
        parallel_analysis_jobs: None,
        bearoff_direction: "right".into(),
    }
}

/// Complete mutable settings draft spanning all 3 persistence destinations.
#[derive(Debug, Clone)]
pub struct SettingsDraft {
    pub saved_config: ProfileConfiguration,
    pub draft_config: ProfileConfiguration,
    pub config_revision: i64,

    pub saved_host: AppSettings,
    pub draft_host: AppSettings,

    pub saved_my_player_profile: Option<Uuid>,
    pub draft_my_player_profile: Option<Uuid>,

    pub thresholds: ThresholdStrings,
    pub tutor_threshold_str: String,
    pub gnubg_edited: bool,
    pub gnubg_checked_version: Option<String>,
}

impl SettingsDraft {
    pub fn new(
        config: ProfileConfiguration,
        config_revision: i64,
        host: AppSettings,
        my_player_profile: Option<Uuid>,
    ) -> Self {
        let thresholds = ThresholdStrings::from_thresholds(config.analysis.error_thresholds);
        let tutor_threshold_str = format_threshold(config.play.tutor_threshold);
        Self {
            saved_config: config.clone(),
            draft_config: config,
            config_revision,
            saved_host: host.clone(),
            draft_host: host,
            saved_my_player_profile: my_player_profile,
            draft_my_player_profile: my_player_profile,
            thresholds,
            tutor_threshold_str,
            gnubg_edited: false,
            gnubg_checked_version: None,
        }
    }

    /// Reconciles valid threshold strings into `draft_config` before computing
    /// diffs or saving.
    pub fn sync_parsed_thresholds(&mut self) {
        if let Some(parsed) = self.thresholds.parse_valid() {
            self.draft_config.analysis.error_thresholds = parsed;
        }
        if let Some(tutor) = parse_unit_f64(&self.tutor_threshold_str)
            && (0.0..=1.0).contains(&tutor)
        {
            self.draft_config.play.tutor_threshold = tutor;
        }
    }

    /// Returns whether the 4 error thresholds are valid strictly-increasing numbers in `(0, 1]`.
    pub fn thresholds_valid(&self) -> bool {
        self.thresholds.parse_valid().is_some()
    }

    /// Returns whether the tutor threshold is a valid finite number in `[0, 1]`.
    pub fn tutor_threshold_valid(&self) -> bool {
        parse_unit_f64(&self.tutor_threshold_str).is_some_and(|v| (0.0..=1.0).contains(&v))
    }

    /// Computes dirty flags across all 3 save destinations independently.
    pub fn dirty_destinations(&self) -> SaveDestinations {
        let thresholds_dirty = self.thresholds
            != ThresholdStrings::from_thresholds(self.saved_config.analysis.error_thresholds);
        let tutor_dirty =
            self.tutor_threshold_str != format_threshold(self.saved_config.play.tutor_threshold);
        let config_dirty = thresholds_dirty
            || tutor_dirty
            || serde_json::to_value(&self.draft_config).ok()
                != serde_json::to_value(&self.saved_config).ok();

        let host_dirty = self.draft_host.gnubg_path != self.saved_host.gnubg_path
            || self.draft_host.gnubg_threads != self.saved_host.gnubg_threads
            || self.draft_host.gnubg_play_threads != self.saved_host.gnubg_play_threads
            || self.draft_host.open_sage_threads != self.saved_host.open_sage_threads
            || self.draft_host.open_sage_play_threads != self.saved_host.open_sage_play_threads
            || self.draft_host.parallel_analysis_jobs != self.saved_host.parallel_analysis_jobs;

        let my_profile_dirty = self.draft_my_player_profile != self.saved_my_player_profile;

        SaveDestinations {
            profile_config: config_dirty,
            host_engines: host_dirty,
            my_player_profile: my_profile_dirty,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_destinations().any()
    }

    pub fn can_apply(&self, saving: bool) -> bool {
        !saving && self.is_dirty() && self.thresholds_valid() && self.tutor_threshold_valid()
    }

    /// Discards all unapplied edits across all 3 destinations.
    pub fn discard(&mut self) {
        self.draft_config = self.saved_config.clone();
        self.draft_host = self.saved_host.clone();
        self.draft_my_player_profile = self.saved_my_player_profile;
        self.thresholds =
            ThresholdStrings::from_thresholds(self.saved_config.analysis.error_thresholds);
        self.tutor_threshold_str = format_threshold(self.saved_config.play.tutor_threshold);
        self.gnubg_edited = false;
    }

    /// Rebases the saved & draft profile configuration after a profile switch/restore.
    pub fn rebase_profile(&mut self, config: ProfileConfiguration, revision: i64) {
        self.saved_config = config.clone();
        self.draft_config = config;
        self.config_revision = revision;
        self.thresholds =
            ThresholdStrings::from_thresholds(self.saved_config.analysis.error_thresholds);
        self.tutor_threshold_str = format_threshold(self.saved_config.play.tutor_threshold);
    }

    /// Builds a synchronized [`AppSettings`] payload for `save_settings`.
    pub fn host_settings_payload(&self) -> AppSettings {
        let mut s = self.draft_host.clone();
        s.player_name = self.draft_config.general.player_name.clone();
        s.analysis_engine = match self.draft_config.analysis.engine {
            ConfigurationEngine::Gnubg => "gnubg".into(),
            ConfigurationEngine::OpenSage => "bgsage".into(),
        };
        s.analysis_plies = self.draft_config.analysis.plies;
        s.play_engine = match self.draft_config.play.engine {
            ConfigurationEngine::Gnubg => "gnubg".into(),
            ConfigurationEngine::OpenSage => "bgsage".into(),
        };
        s.play_plies = self.draft_config.play.plies;
        s.play_top_two = self.draft_config.play.top_two;
        s.default_play_mode = self.draft_config.play.default_mode;
        s.tutor_threshold = self.draft_config.play.tutor_threshold;
        s.training_auto_analysis = self.draft_config.play.training_auto_analysis;
        s.bearoff_direction = match self.draft_config.appearance.bearoff_direction {
            ConfigurationDirection::Left => "left".into(),
            ConfigurationDirection::Right => "right".into(),
        };
        s
    }
}

/// Converts [`ConfigurationUiTheme`] into [`NeonPaletteId`].
pub fn ui_theme_to_palette_id(theme: &ConfigurationUiTheme) -> NeonPaletteId {
    match theme {
        ConfigurationUiTheme::Graphite => NeonPaletteId::Graphite,
        ConfigurationUiTheme::Club => NeonPaletteId::Club,
        ConfigurationUiTheme::Paper => NeonPaletteId::Paper,
    }
}

/// Converts [`NeonPaletteId`] into [`ConfigurationUiTheme`].
pub fn palette_id_to_ui_theme(palette: NeonPaletteId) -> ConfigurationUiTheme {
    match palette {
        NeonPaletteId::Graphite => ConfigurationUiTheme::Graphite,
        NeonPaletteId::Club => ConfigurationUiTheme::Club,
        NeonPaletteId::Paper => ConfigurationUiTheme::Paper,
    }
}

/// Converts [`ConfigurationMode`] into [`ThemeMode`].
pub fn config_mode_to_theme_mode(mode: &ConfigurationMode) -> ThemeMode {
    match mode {
        ConfigurationMode::Light => ThemeMode::Light,
        ConfigurationMode::Dark => ThemeMode::Dark,
        ConfigurationMode::System => ThemeMode::System,
    }
}

/// Converts [`ThemeMode`] into [`ConfigurationMode`].
pub fn theme_mode_to_config_mode(mode: ThemeMode) -> ConfigurationMode {
    match mode {
        ThemeMode::Light => ConfigurationMode::Light,
        ThemeMode::Dark => ConfigurationMode::Dark,
        ThemeMode::System => ConfigurationMode::System,
    }
}

/// Converts [`ConfigurationLocale`] into [`Locale`].
pub fn config_locale_to_locale(locale: &ConfigurationLocale) -> Locale {
    match locale {
        ConfigurationLocale::De => Locale::De,
        ConfigurationLocale::En => Locale::En,
    }
}

/// Converts [`Locale`] into [`ConfigurationLocale`].
pub fn locale_to_config_locale(locale: Locale) -> ConfigurationLocale {
    match locale {
        Locale::De => ConfigurationLocale::De,
        Locale::En => ConfigurationLocale::En,
    }
}

/// Unit callback invoked by Settings dialogs and actions.
pub type DialogActionCallback = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

/// Opens the stacked `SettingsDraftGuardDialog` with **Keep editing**,
/// **Discard draft**, and **Apply and continue** buttons.
pub fn open_settings_draft_guard_dialog(
    window: &mut Window,
    cx: &mut App,
    busy: ModalBusyLock,
    can_apply: bool,
    apply_hint: Option<SharedString>,
    on_discard: DialogActionCallback,
    on_apply_and_continue: DialogActionCallback,
) {
    let locale = current_locale(cx);
    let config = ModalConfig::new(I18n::t(locale, "profile", "draftGuardTitle"))
        .size(ModalSize::Compact)
        .busy_lock(busy.clone());

    let busy_for_footer = busy;
    open_modal_dialog(
        window,
        cx,
        config,
        move |_window, cx| {
            let locale = current_locale(cx);
            let p: NeonPalette = NeonTheme::current_palette(cx);
            let mut body = div()
                .id("settings-draft-guard-body")
                .test_support()
                .v_flex()
                .gap(px(8.0))
                .child(div().text_body().text_color(p.text).child(I18n::t(
                    locale,
                    "profile",
                    "draftGuardMessage",
                )));
            if let Some(hint) = &apply_hint {
                body = body.child(div().text_meta().text_color(p.muted).child(hint.clone()));
            }
            body.into_any_element()
        },
        Some(move |_window: &mut Window, cx: &mut App| {
            let locale = current_locale(cx);
            let p = NeonTheme::current_palette(cx);
            let is_busy = busy_for_footer.is_busy();
            let discard_cb = on_discard.clone();
            let apply_cb = on_apply_and_continue.clone();

            div()
                .h_flex()
                .items_center()
                .justify_end()
                .gap(px(8.0))
                .child(
                    ActionButton::new("draft-guard-keep")
                        .label(I18n::t(locale, "settings", "keepEditing"))
                        .variant(ActionButtonVariant::Ghost)
                        .palette(p)
                        .disabled(is_busy)
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                        }),
                )
                .child(
                    ActionButton::new("draft-guard-discard")
                        .icon(ActionIconName::Reset)
                        .label(I18n::t(locale, "profile", "discardDraft"))
                        .danger()
                        .palette(p)
                        .disabled(is_busy)
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            discard_cb(window, cx);
                        }),
                )
                .child(
                    ActionButton::new("draft-guard-apply")
                        .icon(ActionIconName::Save)
                        .label(I18n::t(locale, "profile", "applyAndContinue"))
                        .primary()
                        .palette(p)
                        .disabled(is_busy || !can_apply)
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            apply_cb(window, cx);
                        }),
                )
                .into_any_element()
        }),
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_three_save_destinations_independently_and_validates_thresholds() {
        let cfg = default_profile_configuration(
            Locale::En,
            NeonPaletteId::Graphite,
            ThemeMode::System,
            "forest",
        );
        let host = default_host_settings("gnubg");
        let mut draft = SettingsDraft::new(cfg, 1, host, None);

        assert!(!draft.is_dirty());
        assert_eq!(draft.dirty_destinations(), SaveDestinations::default());

        // 1) Profile config dirty only
        draft.draft_config.general.player_name = "Alice".into();
        let d1 = draft.dirty_destinations();
        assert!(d1.profile_config && !d1.host_engines && !d1.my_player_profile);

        // 2) Host engines dirty too
        draft.draft_host.parallel_analysis_jobs = Some(4);
        let d2 = draft.dirty_destinations();
        assert!(d2.profile_config && d2.host_engines && !d2.my_player_profile);

        // 3) Library "My player profile" dirty too
        draft.draft_my_player_profile = Some(Uuid::new_v4());
        let d3 = draft.dirty_destinations();
        assert!(d3.profile_config && d3.host_engines && d3.my_player_profile);
        assert!(d3.hint(Locale::En).is_some());

        // Threshold validation rejects non-strictly-increasing values
        draft.thresholds.error = "0.02".into();
        draft.thresholds.questionable = "0.03".into();
        assert!(!draft.thresholds_valid());
        assert!(!draft.can_apply(false));

        // Comma-formatted strictly increasing values succeed
        draft.thresholds = ThresholdStrings {
            questionable: "0,02".into(),
            error: "0,05".into(),
            blunder: "0,09".into(),
            large_blunder: "0,18".into(),
        };
        assert!(draft.thresholds_valid());
        assert!(draft.can_apply(false));

        // Discard resets all 3 destinations cleanly
        draft.discard();
        assert!(!draft.is_dirty());
    }
}
