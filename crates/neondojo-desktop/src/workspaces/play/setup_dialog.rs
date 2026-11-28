//! Non-modal board-anchored New Match setup dialog (`PlaySetupDialog`).
//!
//! Mirrors `PlaySetupForm.tsx` and `usePlaySetupOptions.ts`:
//! - Preserves the **Play empty-state focus exception** (`modal: false`,
//!   `focus_on_open: false` on empty Play state so header tab focus is kept).
//! - Engine selector (`GNUbg` vs `NeonGammon`).
//! - NeonGammon named difficulty presets (`Beginner` / `Casual` / `Intermediate` /
//!   `Full Strength`) mutually exclusive with `2-ply + Top-2 (10 mEq)`.
//! - Checker color (`White` / `Black`), Money + 1..=25 pt match lengths.
//! - 4 play modes (`Competition`, `Training`, `Coach`, `Tutor`) + Training
//!   auto-analysis toggle + jump-to-Settings button (`SettingsCategory::Playing`).

use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::icons::ActionIconName;
use crate::workspaces::play::dialogs::{PlayArgCallback, PlayVoidCallback};
use gpui_kit::{
    AnyElement, App, ClickEvent, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, TestSupportExt,
    base::StyledExt,
    component::input::{Input, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::configuration::{ConfigurationEngine, ProfileConfiguration};
use neondojo::engines::SageDifficulty;
use neondojo::play::{CreatePlaySession, PlayMode, PlaySession};
use neondojo::settings::EngineInfo;
use neondojo_i18n::{I18n, Param, format_decimal, format_ply};
use neondojo_rules::options::engine_depths;

/// Supported point match lengths plus `0` for Money game (`PlaySetupForm.tsx:228`).
pub const SETUP_MATCH_LENGTHS: &[i32] = &[0, 1, 3, 5, 7, 9, 11, 15, 21, 25];

/// Calibrated 5-point PR benchmarks for the 4 NeonGammon opponent presets
/// (`PlaySetupForm.tsx:21-30`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SagePresetInfo {
    pub difficulty: SageDifficulty,
    pub i18n_key: &'static str,
    pub five_point_pr: f64,
    pub checker_plies: i32,
}

pub const SAGE_PRESETS: &[SagePresetInfo] = &[
    SagePresetInfo {
        difficulty: SageDifficulty::Beginner,
        i18n_key: "beginner",
        five_point_pr: 20.3,
        checker_plies: 1,
    },
    SagePresetInfo {
        difficulty: SageDifficulty::Casual,
        i18n_key: "casual",
        five_point_pr: 15.8,
        checker_plies: 1,
    },
    SagePresetInfo {
        difficulty: SageDifficulty::Intermediate,
        i18n_key: "intermediate",
        five_point_pr: 9.3,
        checker_plies: 1,
    },
    SagePresetInfo {
        difficulty: SageDifficulty::FullStrength,
        i18n_key: "full_strength",
        five_point_pr: 1.0,
        checker_plies: 3,
    },
];

/// Localized play mode metadata.
pub const PLAY_MODES: &[(PlayMode, &str, &str)] = &[
    (PlayMode::Competition, "competition", "competitionHelp"),
    (PlayMode::Training, "training", "trainingHelp"),
    (PlayMode::Coach, "coach", "coachHelp"),
    (PlayMode::Tutor, "tutor", "tutorHelp"),
];

/// Local form choices for starting a new match (mutually exclusive presets vs top-2).
#[derive(Debug, Clone, PartialEq)]
pub struct PlaySetupChoices {
    pub human_player: i32,
    pub match_length: i32,
    pub mode: PlayMode,
    pub tutor_threshold: f64,
    pub training_auto_analysis: bool,
    pub player_name: String,
    pub name_edited: bool,
    pub bot_engine: String,
    pub engine_edited: bool,
    pub bot_plies: i32,
    pub plies_edited: bool,
    pub bot_top_two: bool,
    pub bot_difficulty: Option<SageDifficulty>,
    pub analysis_engine: String,
    pub analysis_plies: i32,
    pub bearoff_direction: String,
}

impl Default for PlaySetupChoices {
    fn default() -> Self {
        Self {
            human_player: 0,
            match_length: 5,
            mode: PlayMode::Coach,
            tutor_threshold: 0.04,
            training_auto_analysis: true,
            player_name: String::new(),
            name_edited: false,
            bot_engine: "gnubg".to_string(),
            engine_edited: false,
            bot_plies: 1,
            plies_edited: false,
            bot_top_two: false,
            bot_difficulty: None,
            analysis_engine: "gnubg".to_string(),
            analysis_plies: 3,
            bearoff_direction: "right".to_string(),
        }
    }
}

impl PlaySetupChoices {
    /// Seeds defaults from the confirmed [`ProfileConfiguration`].
    pub fn from_configuration(cfg: &ProfileConfiguration) -> Self {
        let bot_engine = match cfg.play.engine {
            ConfigurationEngine::Gnubg => "gnubg",
            ConfigurationEngine::OpenSage => "bgsage",
        };
        let analysis_engine = match cfg.analysis.engine {
            ConfigurationEngine::Gnubg => "gnubg",
            ConfigurationEngine::OpenSage => "bgsage",
        };
        let depths = engine_depths(bot_engine, "play");
        let bot_plies = if depths.contains(&cfg.play.plies) {
            cfg.play.plies
        } else {
            depths.first().copied().unwrap_or(1)
        };
        let bot_top_two = bot_engine == "bgsage" && bot_plies == 2 && cfg.play.top_two;
        Self {
            human_player: cfg.play.human_player.clamp(0, 1),
            match_length: cfg.play.match_length.clamp(0, 25),
            mode: cfg.play.default_mode,
            tutor_threshold: cfg.analysis.error_thresholds.error,
            training_auto_analysis: cfg.play.training_auto_analysis,
            player_name: cfg.general.player_name.clone(),
            name_edited: false,
            bot_engine: bot_engine.to_string(),
            engine_edited: false,
            bot_plies,
            plies_edited: false,
            bot_top_two,
            bot_difficulty: None,
            analysis_engine: analysis_engine.to_string(),
            analysis_plies: cfg.analysis.plies,
            bearoff_direction: match cfg.appearance.bearoff_direction {
                neondojo::configuration::ConfigurationDirection::Left => "left".to_string(),
                neondojo::configuration::ConfigurationDirection::Right => "right".to_string(),
            },
        }
    }

    /// Switches bot engine (`"gnubg"` or `"bgsage"`), resetting preset/top-two cleanly.
    pub fn select_engine(&mut self, engine: &str) {
        if engine != "gnubg" && engine != "bgsage" {
            return;
        }
        self.bot_engine = engine.to_string();
        self.engine_edited = true;
        let depths = engine_depths(engine, "play");
        let preferred = if engine == "bgsage" { 2 } else { 1 };
        self.bot_plies = if depths.contains(&preferred) {
            preferred
        } else {
            depths.first().copied().unwrap_or(1)
        };
        self.plies_edited = false;
        self.bot_top_two = false;
        self.bot_difficulty = None;
    }

    /// Selects a raw ply strength (clears any named NeonGammon preset & `top_two`).
    pub fn select_ply(&mut self, plies: i32) {
        let depths = engine_depths(&self.bot_engine, "play");
        if !depths.contains(&plies) {
            return;
        }
        self.bot_plies = plies;
        self.plies_edited = true;
        self.bot_top_two = false;
        self.bot_difficulty = None;
    }

    /// Selects NeonGammon `2-ply + Top-2 (10 mEq)` — mutually exclusive with named presets.
    pub fn select_top_two(&mut self) {
        if self.bot_engine != "bgsage" {
            return;
        }
        let depths = engine_depths("bgsage", "play");
        if !depths.contains(&2) {
            return;
        }
        self.bot_plies = 2;
        self.plies_edited = true;
        self.bot_top_two = true;
        self.bot_difficulty = None;
    }

    /// Selects a NeonGammon named difficulty preset — mutually exclusive with `2-ply + Top-2`.
    pub fn select_difficulty(&mut self, difficulty: SageDifficulty) {
        if self.bot_engine != "bgsage" {
            return;
        }
        self.bot_difficulty = Some(difficulty);
        self.bot_top_two = false;
        self.plies_edited = true;
    }

    /// Builds the backend [`CreatePlaySession`] payload obeying all preset/ply invariants.
    pub fn to_create_request(&self) -> CreatePlaySession {
        let human_name = self.name_edited.then(|| self.player_name.clone());
        let (play_engine, play_plies, play_top_two, play_difficulty) =
            if let Some(difficulty) = self.bot_difficulty.filter(|_| self.bot_engine == "bgsage") {
                (Some("bgsage".to_string()), None, None, Some(difficulty))
            } else if self.engine_edited || self.plies_edited || self.bot_top_two {
                (
                    Some(self.bot_engine.clone()),
                    Some(self.bot_plies),
                    Some(self.bot_top_two),
                    None,
                )
            } else {
                (None, None, None, None)
            };

        CreatePlaySession {
            play_mode: Some(self.mode),
            tutor_threshold: None,
            training_auto_analysis: Some(self.training_auto_analysis),
            human_player: self.human_player.clamp(0, 1),
            match_length: self.match_length.clamp(0, 25),
            human_name,
            play_engine,
            play_plies,
            play_top_two,
            play_difficulty,
        }
    }
}

/// Callbacks for [`render_play_setup_form`].
#[derive(Clone)]
pub struct PlaySetupCallbacks {
    pub on_engine: PlayArgCallback<&'static str>,
    pub on_ply: PlayArgCallback<i32>,
    pub on_top_two: PlayVoidCallback,
    pub on_difficulty: PlayArgCallback<SageDifficulty>,
    pub on_human_player: PlayArgCallback<i32>,
    pub on_match_length: PlayArgCallback<i32>,
    pub on_mode: PlayArgCallback<PlayMode>,
    pub on_auto_analysis: PlayArgCallback<bool>,
    pub on_open_settings: PlayVoidCallback,
    pub on_submit: PlayVoidCallback,
    pub on_resume: PlayVoidCallback,
}

/// Renders the complete New Match setup form inside `PlayBoardDialog`.
#[allow(clippy::too_many_arguments)]
pub fn render_play_setup_form(
    choices: &PlaySetupChoices,
    name_input: &Entity<InputState>,
    engines: &[EngineInfo],
    session: Option<&PlaySession>,
    error_banner: Option<AnyElement>,
    busy: bool,
    palette: NeonPalette,
    cb: PlaySetupCallbacks,
    wide: bool,
    cx: &App,
) -> (AnyElement, AnyElement) {
    let locale = current_locale(cx);
    let p = palette;
    let sage_info = engines.iter().find(|e| e.id == "bgsage");
    let engine_available = |id: &str| {
        engines
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.available)
            .unwrap_or(true)
    };
    let bot_depths = engine_depths(&choices.bot_engine, "play");

    let engine_row = div()
        .h_flex()
        .gap(px(8.0))
        .child({
            let avail = engine_available("gnubg");
            let on_engine = cb.on_engine.clone();
            ActionButton::new("play-setup-engine-gnubg")
                .tab_stop()
                .label(if avail {
                    "GNUbg".to_string()
                } else {
                    format!("GNUbg — {}", I18n::t(locale, "settings", "unavailable"))
                })
                .active(choices.bot_engine == "gnubg")
                .disabled(busy || (!avail && choices.bot_engine != "gnubg"))
                .palette(p)
                .on_click(move |_: &ClickEvent, window, cx| on_engine("gnubg", window, cx))
        })
        .child({
            let avail = engine_available("bgsage");
            let on_engine = cb.on_engine.clone();
            ActionButton::new("play-setup-engine-bgsage")
                .tab_stop()
                .label(if avail {
                    "NeonGammon".to_string()
                } else {
                    format!(
                        "NeonGammon — {}",
                        I18n::t(locale, "settings", "unavailable")
                    )
                })
                .active(choices.bot_engine == "bgsage")
                .disabled(busy || (!avail && choices.bot_engine != "bgsage"))
                .palette(p)
                .on_click(move |_: &ClickEvent, window, cx| on_engine("bgsage", window, cx))
        });

    let mut strength_block = div().v_flex().gap(px(6.0));
    if choices.bot_engine == "bgsage" {
        let preset_heading = if choices.match_length == 5 {
            I18n::t(locale, "play", "difficultyFivePoint")
        } else {
            I18n::t(locale, "play", "difficultyPresets")
        };
        let mut preset_row = div().v_flex().gap(px(4.0));
        for preset in SAGE_PRESETS {
            let preset_disabled = sage_info.is_some_and(|info| {
                !info.available
                    || info.difficulty_presets == Some(false)
                    || !info.plies.contains(&preset.checker_plies)
            });
            let mut label = I18n::t(locale, "play", preset.i18n_key).to_string();
            if preset.difficulty == SageDifficulty::FullStrength {
                label.push_str(" · ");
                label.push_str(I18n::t(locale, "play", "fullStrengthDepth"));
            }
            if choices.match_length == 5 {
                label.push_str(&format!(
                    " · ≈ {} PR",
                    format_decimal(locale, preset.five_point_pr, 1, 1)
                ));
            }
            let diff = preset.difficulty;
            let on_diff = cb.on_difficulty.clone();
            preset_row = preset_row.child(
                ActionButton::new(SharedString::from(format!(
                    "play-setup-preset-{}",
                    preset.i18n_key
                )))
                .tab_stop()
                .label(label)
                .active(choices.bot_difficulty == Some(diff))
                .disabled(busy || preset_disabled)
                .palette(p)
                .on_click(move |_: &ClickEvent, window, cx| on_diff(diff, window, cx)),
            );
        }
        strength_block = strength_block
            .child(div().text_meta().text_color(p.muted).child(preset_heading))
            .child(preset_row);
    }

    let mut depth_row = div().flex().flex_row().flex_wrap().gap(px(6.0));
    for ply in &bot_depths {
        let ply = *ply;
        let on_ply = cb.on_ply.clone();
        let active =
            choices.bot_difficulty.is_none() && !choices.bot_top_two && choices.bot_plies == ply;
        depth_row = depth_row.child(
            ActionButton::new(SharedString::from(format!("play-setup-ply-{ply}")))
                .tab_stop()
                .label(format_ply(locale, ply.max(0) as u32))
                .active(active)
                .disabled(busy)
                .palette(p)
                .on_click(move |_: &ClickEvent, window, cx| on_ply(ply, window, cx)),
        );
    }
    if choices.bot_engine == "bgsage" && bot_depths.contains(&2) {
        let on_top_two = cb.on_top_two.clone();
        depth_row = depth_row.child(
            ActionButton::new("play-setup-top-two")
                .tab_stop()
                .label(format!(
                    "{} + {}",
                    format_ply(locale, 2),
                    I18n::t(locale, "profile", "top_two")
                ))
                .active(choices.bot_difficulty.is_none() && choices.bot_top_two)
                .disabled(busy)
                .palette(p)
                .on_click(move |_: &ClickEvent, window, cx| on_top_two(window, cx)),
        );
    }
    strength_block = strength_block.child(depth_row);

    let color_row = div()
        .h_flex()
        .gap(px(8.0))
        .child({
            let on_human = cb.on_human_player.clone();
            ActionButton::new("play-setup-color-white")
                .tab_stop()
                .label(I18n::t(locale, "play", "white"))
                .active(choices.human_player == 0)
                .disabled(busy)
                .palette(p)
                .on_click(move |_: &ClickEvent, window, cx| on_human(0, window, cx))
        })
        .child({
            let on_human = cb.on_human_player.clone();
            ActionButton::new("play-setup-color-black")
                .tab_stop()
                .label(I18n::t(locale, "play", "black"))
                .active(choices.human_player == 1)
                .disabled(busy)
                .palette(p)
                .on_click(move |_: &ClickEvent, window, cx| on_human(1, window, cx))
        });

    let mut length_row = div().flex().flex_row().flex_wrap().gap(px(6.0));
    for &len in SETUP_MATCH_LENGTHS {
        let on_len = cb.on_match_length.clone();
        let label = if len == 0 {
            I18n::t(locale, "app", "common.moneyGame").to_string()
        } else {
            I18n::format(
                locale,
                "app",
                "board.points",
                &[("count", Param::from(len))],
            )
        };
        length_row = length_row.child(
            ActionButton::new(SharedString::from(format!("play-setup-length-{len}")))
                .tab_stop()
                .label(label)
                .active(choices.match_length == len)
                .disabled(busy)
                .palette(p)
                .on_click(move |_: &ClickEvent, window, cx| on_len(len, window, cx)),
        );
    }

    let mut mode_col = div().v_flex().gap(px(6.0));
    for &(mode, key, help_key) in PLAY_MODES {
        let on_mode = cb.on_mode.clone();
        let active = choices.mode == mode;
        mode_col = mode_col.child(
            div()
                .id(SharedString::from(format!("play-setup-mode-{key}")))
                .test_support()
                .h_flex()
                .items_start()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(7.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(if active { p.accent } else { p.border })
                .bg(if active { p.selected } else { p.surface_alt })
                .when(!busy, |this| {
                    this.cursor_pointer()
                        .tab_index(0)
                        .focus_visible(move |style| style.border_color(p.accent).border_2())
                        .on_click(move |_: &ClickEvent, window, cx| on_mode(mode, window, cx))
                })
                .child(
                    div()
                        .mt(px(3.0))
                        .w(px(10.0))
                        .h(px(10.0))
                        .rounded_full()
                        .border_1()
                        .border_color(if active { p.accent } else { p.control_border })
                        .bg(if active { p.accent } else { p.surface })
                        .flex_shrink_0(),
                )
                .child(
                    div()
                        .v_flex()
                        .gap(px(2.0))
                        .flex_1()
                        .min_w(px(0.0))
                        .child(
                            div()
                                .text_label()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.text)
                                .child(I18n::t(locale, "play-mode", key)),
                        )
                        .child(
                            div()
                                .w_full()
                                .text_meta()
                                .text_color(p.muted)
                                .child(I18n::t(locale, "play-mode", help_key)),
                        ),
                ),
        );
    }

    let analysis_engine_name = if choices.analysis_engine == "bgsage" {
        "NeonGammon"
    } else {
        "GNUbg"
    };
    let engine_footer_line = format!(
        "{} · {} · {} / {} · GNUbg",
        I18n::t(locale, "settings", "analysisEngine"),
        analysis_engine_name,
        format_ply(locale, choices.analysis_plies.max(1) as u32),
        I18n::t(locale, "play", "rulesEngine"),
    );

    let on_settings = cb.on_open_settings.clone();
    let on_submit = cb.on_submit.clone();
    let on_resume = cb.on_resume.clone();

    let column = || {
        div()
            .v_flex()
            .min_w(px(0.0))
            .gap(px(10.0))
            .when(wide, |el| el.flex_1())
    };
    let players_and_engines = column()
        .child(
            div()
                .v_flex()
                .gap(px(4.0))
                .child(div().text_label().text_color(p.muted).child(I18n::t(
                    locale,
                    "play",
                    "playerName",
                )))
                .child(Input::new(name_input).id("play-setup-player-name")),
        )
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .child(div().text_label().text_color(p.muted).child(I18n::t(
                    locale,
                    "play",
                    "botEngine",
                )))
                .child(engine_row)
                .child(
                    div()
                        .text_label()
                        .text_color(p.muted)
                        .child(I18n::t(locale, "play", "strength")),
                )
                .child(strength_block)
                .child(div().text_meta().text_color(p.muted).child(I18n::t(
                    locale,
                    "play",
                    "matchChoicesHelp",
                ))),
        )
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .pt(px(8.0))
                .border_t_1()
                .border_color(p.border)
                .child(
                    div()
                        .text_meta()
                        .text_color(p.muted)
                        .flex_1()
                        .child(engine_footer_line),
                )
                .child(
                    ActionButton::new("play-setup-open-settings")
                        .tab_stop()
                        .icon(ActionIconName::Settings)
                        .tooltip(I18n::t(locale, "play", "changeEngine"))
                        .compact()
                        .disabled(busy)
                        .palette(p)
                        .on_click(move |_: &ClickEvent, window, cx| on_settings(window, cx)),
                ),
        );
    let match_options =
        column()
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(div().text_label().text_color(p.muted).child(I18n::t(
                        locale,
                        "play",
                        "humanColor",
                    )))
                    .child(color_row),
            )
            .child(
                div()
                    .v_flex()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_label()
                            .text_color(p.muted)
                            .child(I18n::t(locale, "play", "length")),
                    )
                    .child(length_row),
            )
            .child(
                div()
                    .v_flex()
                    .gap(px(6.0))
                    .child(div().text_label().text_color(p.muted).child(I18n::t(
                        locale,
                        "play-mode",
                        "mode",
                    )))
                    .child(mode_col)
                    .when(choices.mode == PlayMode::Tutor, |this| {
                        this.child(div().text_meta().tabular_nums().text_color(p.muted).child(
                            format!(
                                "{} · {}",
                                I18n::t(locale, "play-mode", "threshold"),
                                format_decimal(locale, choices.tutor_threshold, 3, 6)
                            ),
                        ))
                    })
                    .when(choices.mode == PlayMode::Training, |this| {
                        let on_auto = cb.on_auto_analysis.clone();
                        let next = !choices.training_auto_analysis;
                        this.child(
                            ActionButton::new("play-setup-training-auto")
                                .tab_stop()
                                .label(I18n::t(locale, "play-mode", "automatic"))
                                .active(choices.training_auto_analysis)
                                .disabled(busy)
                                .palette(p)
                                .on_click(move |_: &ClickEvent, window, cx| {
                                    on_auto(next, window, cx)
                                }),
                        )
                    }),
            );
    let body = div()
        .id("play-setup-form")
        .test_support()
        .v_flex()
        .gap(px(12.0))
        .when_some(error_banner, |this, err| this.child(err))
        .when(busy, |this| {
            this.child(
                div()
                    .text_meta()
                    .text_color(p.accent)
                    .child(I18n::t(locale, "play", "thinking")),
            )
        })
        .child(
            div()
                .flex()
                .gap(px(16.0))
                .when(!wide, |el| el.flex_col())
                .when(wide, |el| el.flex_row().items_start())
                .child(players_and_engines)
                .child(match_options),
        )
        .into_any_element();
    let footer = div()
        .h_flex()
        .items_center()
        .gap(px(8.0))
        .flex_wrap()
        .justify_end()
        .pt(px(8.0))
        .border_t_1()
        .border_color(p.border)
        .child(
            ActionButton::new("play-setup-start")
                .icon(ActionIconName::Play)
                .tab_stop()
                .label(I18n::t(locale, "play", "start"))
                .primary()
                .shortcut("Cmd+Enter")
                .disabled(busy)
                .palette(p)
                .on_click(move |_: &ClickEvent, window, cx| on_submit(window, cx)),
        )
        .when_some(session, |this, s| {
            let key = if s.paused {
                "resumeMatch"
            } else {
                "backToMatch"
            };
            let icon = if s.paused {
                ActionIconName::Resume
            } else {
                ActionIconName::Back
            };
            this.child(
                ActionButton::new("play-setup-resume-or-back")
                    .icon(icon)
                    .tab_stop()
                    .label(I18n::t(locale, "play", key))
                    .disabled(busy)
                    .palette(p)
                    .on_click(move |_: &ClickEvent, window, cx| on_resume(window, cx)),
            )
        })
        .into_any_element();
    (body, footer)
}
