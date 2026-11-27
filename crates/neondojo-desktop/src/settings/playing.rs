//! Settings → Playing section (`PlaySettingsSection.tsx`).
//!
//! Covers Play engine, strength/ply (plus NeonGammon 2-ply top-2 re-evaluation),
//! default play mode (Competition / Training / Coach / Tutor), Tutor threshold
//! `DecimalInput`, automatic training analysis, checker color, match length,
//! game sounds, and bear-off direction.

use crate::i18n::current_locale;
use crate::settings::draft_guard::SettingsDraft;
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::decimal_input::DecimalInput;
use gpui_kit::{
    App, Entity, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, Window,
    base::StyledExt, div, prelude::FluentBuilder, px,
};
use neondojo::configuration::{ConfigurationDirection, ConfigurationEngine};
use neondojo::play::PlayMode;
use neondojo::settings::EngineInfo;
use neondojo_i18n::{I18n, format_ply};
use neondojo_rules::options::engine_depths;
use std::rc::Rc;

pub type PlayEngineHandler = Rc<dyn Fn(ConfigurationEngine, &mut Window, &mut App) + 'static>;
pub type PlayIntHandler = Rc<dyn Fn(i32, &mut Window, &mut App) + 'static>;
pub type PlayBoolHandler = Rc<dyn Fn(bool, &mut Window, &mut App) + 'static>;
pub type PlayModeHandler = Rc<dyn Fn(PlayMode, &mut Window, &mut App) + 'static>;
pub type PlayDirectionHandler = Rc<dyn Fn(ConfigurationDirection, &mut Window, &mut App) + 'static>;

/// Callbacks mutating the Play portion of [`SettingsDraft`].
#[derive(Clone)]
pub struct PlaySectionCallbacks {
    pub on_engine: PlayEngineHandler,
    pub on_ply: PlayIntHandler,
    pub on_top_two: PlayBoolHandler,
    pub on_mode: PlayModeHandler,
    pub on_auto_analysis: PlayBoolHandler,
    pub on_human_player: PlayIntHandler,
    pub on_match_length: PlayIntHandler,
    pub on_sounds: PlayBoolHandler,
    pub on_direction: PlayDirectionHandler,
}

/// Renders Settings → Playing.
#[allow(clippy::too_many_arguments)]
pub fn render_playing_section(
    draft: &SettingsDraft,
    engines: &[EngineInfo],
    tutor_threshold_input: &Entity<DecimalInput>,
    disabled: bool,
    palette: NeonPalette,
    cb: PlaySectionCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let play = &draft.draft_config.play;
    let engine_id = match play.engine {
        ConfigurationEngine::Gnubg => "gnubg",
        ConfigurationEngine::OpenSage => "bgsage",
    };

    let engine_available = |id: &str| {
        engines
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.available)
            .unwrap_or(true)
    };

    let engine_row = div()
        .h_flex()
        .gap(px(8.0))
        .child({
            let on_engine = cb.on_engine.clone();
            let avail = engine_available("gnubg");
            ActionButton::new("settings-play-engine-gnubg")
                .label(if avail {
                    "GNUbg".to_string()
                } else {
                    format!("GNUbg — {}", I18n::t(locale, "settings", "unavailable"))
                })
                .active(play.engine == ConfigurationEngine::Gnubg)
                .disabled(disabled || !avail)
                .palette(palette)
                .on_click(move |_, window, cx| on_engine(ConfigurationEngine::Gnubg, window, cx))
        })
        .child({
            let on_engine = cb.on_engine.clone();
            let avail = engine_available("bgsage");
            ActionButton::new("settings-play-engine-bgsage")
                .label(if avail {
                    "NeonGammon".to_string()
                } else {
                    format!(
                        "NeonGammon — {}",
                        I18n::t(locale, "settings", "unavailable")
                    )
                })
                .active(play.engine == ConfigurationEngine::OpenSage)
                .disabled(disabled || !avail)
                .palette(palette)
                .on_click(move |_, window, cx| on_engine(ConfigurationEngine::OpenSage, window, cx))
        });

    let mut depth_row = div().h_flex().gap(px(6.0));
    for ply in engine_depths(engine_id, "play") {
        let on_ply = cb.on_ply.clone();
        depth_row = depth_row.child(
            ActionButton::new(SharedString::from(format!("settings-play-ply-{ply}")))
                .label(format_ply(locale, ply.max(0) as u32))
                .active(play.plies == ply)
                .disabled(disabled)
                .palette(palette)
                .on_click(move |_, window, cx| on_ply(ply, window, cx)),
        );
    }

    let show_top_two = play.engine == ConfigurationEngine::OpenSage && play.plies == 2;
    let modes = [
        (PlayMode::Competition, "competition", "competitionHelp"),
        (PlayMode::Training, "training", "trainingHelp"),
        (PlayMode::Coach, "coach", "coachHelp"),
        (PlayMode::Tutor, "tutor", "tutorHelp"),
    ];
    let active_mode_help = modes
        .iter()
        .find(|(m, _, _)| *m == play.default_mode)
        .map(|(_, _, help)| I18n::t(locale, "play-mode", help))
        .unwrap_or("");

    let mut mode_row = div().h_flex().gap(px(6.0));
    for (mode, key, _) in modes {
        let on_mode = cb.on_mode.clone();
        mode_row = mode_row.child(
            ActionButton::new(SharedString::from(format!("settings-play-mode-{key}")))
                .label(I18n::t(locale, "play-mode", key))
                .active(play.default_mode == mode)
                .disabled(disabled)
                .palette(palette)
                .on_click(move |_, window, cx| on_mode(mode, window, cx)),
        );
    }

    let match_lengths = [0, 3, 5, 7, 9, 11];
    let mut length_row = div().flex().flex_row().flex_wrap().gap(px(6.0));
    for len in match_lengths {
        let on_len = cb.on_match_length.clone();
        let label = if len == 0 {
            I18n::t(locale, "app", "common.moneyGame").to_string()
        } else {
            format!("{len} pt")
        };
        length_row = length_row.child(
            ActionButton::new(SharedString::from(format!("settings-play-length-{len}")))
                .label(label)
                .active(play.match_length == len)
                .disabled(disabled)
                .palette(palette)
                .on_click(move |_, window, cx| on_len(len, window, cx)),
        );
    }

    div()
        .id("settings-section-play")
        .v_flex()
        .gap(px(16.0))
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .child(div().text_panel().text_color(palette.text).child(I18n::t(
                    locale,
                    "settings",
                    "playEngine",
                )))
                .child(engine_row)
                .child(div().text_label().text_color(palette.muted).child(I18n::t(
                    locale,
                    "settings",
                    "playStrength",
                )))
                .child(depth_row)
                .when(show_top_two, |this| {
                    let on_top_two = cb.on_top_two.clone();
                    let next = !play.top_two;
                    this.child(
                        ActionButton::new("settings-play-top-two")
                            .label(I18n::t(locale, "profile", "top_two"))
                            .active(play.top_two)
                            .disabled(disabled)
                            .palette(palette)
                            .on_click(move |_, window, cx| on_top_two(next, window, cx)),
                    )
                }),
        )
        .child(
            div()
                .v_flex()
                .gap(px(8.0))
                .pt(px(8.0))
                .border_t_1()
                .border_color(palette.border)
                .child(div().text_panel().text_color(palette.text).child(I18n::t(
                    locale,
                    "settings",
                    "playFeedbackGroup",
                )))
                .child(mode_row)
                .child(
                    div()
                        .text_meta()
                        .text_color(palette.muted)
                        .child(active_mode_help),
                )
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(14.0))
                        .child(
                            div()
                                .v_flex()
                                .gap(px(4.0))
                                .child(div().text_label().text_color(palette.muted).child(I18n::t(
                                    locale,
                                    "play-mode",
                                    "threshold",
                                )))
                                .child(tutor_threshold_input.clone()),
                        )
                        .child({
                            let on_auto = cb.on_auto_analysis.clone();
                            let next = !play.training_auto_analysis;
                            ActionButton::new("settings-play-auto-analysis")
                                .label(I18n::t(locale, "play-mode", "automatic"))
                                .active(play.training_auto_analysis)
                                .disabled(disabled)
                                .palette(palette)
                                .on_click(move |_, window, cx| on_auto(next, window, cx))
                        }),
                ),
        )
        .child(
            div()
                .v_flex()
                .gap(px(8.0))
                .pt(px(8.0))
                .border_t_1()
                .border_color(palette.border)
                .child(div().text_panel().text_color(palette.text).child(I18n::t(
                    locale,
                    "settings",
                    "playPreferencesGroup",
                )))
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .child({
                            let on_human = cb.on_human_player.clone();
                            ActionButton::new("settings-play-color-white")
                                .label(I18n::t(locale, "app", "common.white"))
                                .active(play.human_player == 0)
                                .disabled(disabled)
                                .palette(palette)
                                .on_click(move |_, window, cx| on_human(0, window, cx))
                        })
                        .child({
                            let on_human = cb.on_human_player.clone();
                            ActionButton::new("settings-play-color-black")
                                .label(I18n::t(locale, "app", "common.black"))
                                .active(play.human_player == 1)
                                .disabled(disabled)
                                .palette(palette)
                                .on_click(move |_, window, cx| on_human(1, window, cx))
                        })
                        .child({
                            let on_sounds = cb.on_sounds.clone();
                            let next = !draft.draft_config.general.play_sounds;
                            ActionButton::new("settings-play-sounds")
                                .label(I18n::t(locale, "profile", "play_sounds"))
                                .active(draft.draft_config.general.play_sounds)
                                .disabled(disabled)
                                .palette(palette)
                                .on_click(move |_, window, cx| on_sounds(next, window, cx))
                        }),
                )
                .child(length_row)
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().text_label().text_color(palette.muted).child(I18n::t(
                            locale,
                            "profile",
                            "bearoff_direction",
                        )))
                        .child({
                            let on_dir = cb.on_direction.clone();
                            ActionButton::new("settings-bearoff-left")
                                .label("← Left")
                                .active(
                                    draft.draft_config.appearance.bearoff_direction
                                        == ConfigurationDirection::Left,
                                )
                                .disabled(disabled)
                                .palette(palette)
                                .on_click(move |_, window, cx| {
                                    on_dir(ConfigurationDirection::Left, window, cx);
                                })
                        })
                        .child(
                            ActionButton::new("settings-bearoff-right")
                                .label("Right →")
                                .active(
                                    draft.draft_config.appearance.bearoff_direction
                                        == ConfigurationDirection::Right,
                                )
                                .disabled(disabled)
                                .palette(palette)
                                .on_click(move |_, window, cx| {
                                    (cb.on_direction)(ConfigurationDirection::Right, window, cx);
                                }),
                        ),
                ),
        )
}
