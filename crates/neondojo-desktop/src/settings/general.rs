//! Settings → General section (`GeneralSettingsSection.tsx` & `MyPlayerProfileSetting.tsx`).
//!
//! Covers app language (`Deutsch` / `English`, applied on Apply), default player
//! name for future games, and the Library "My player profile" virtual profile
//! picker (Save Destination 3).

use crate::i18n::current_locale;
use crate::settings::draft_guard::{SettingsDraft, config_locale_to_locale};
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use gpui_kit::{
    App, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString, Styled,
    Window,
    base::StyledExt,
    component::input::{Input, InputState},
    div, px,
};
use neondojo::player_directory::VirtualPlayerProfile;
use neondojo_i18n::{I18n, Locale, Param};
use std::rc::Rc;
use uuid::Uuid;

pub type SelectLocaleHandler = Rc<dyn Fn(Locale, &mut Window, &mut App) + 'static>;
pub type SelectMyProfileHandler = Rc<dyn Fn(Option<Uuid>, &mut Window, &mut App) + 'static>;

/// Renders Settings → General: Language, Player name, and Library "My player profile".
#[allow(clippy::too_many_arguments)]
pub fn render_general_section(
    draft: &SettingsDraft,
    apply_label: &str,
    player_name_input: &Entity<InputState>,
    virtual_profiles: &[VirtualPlayerProfile],
    disabled: bool,
    palette: NeonPalette,
    on_locale: SelectLocaleHandler,
    on_my_profile: SelectMyProfileHandler,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let selected_locale = config_locale_to_locale(&draft.draft_config.general.locale);

    let mut lang_row = div().h_flex().gap(px(8.0));
    for lang in Locale::LANGUAGES {
        let on_locale = on_locale.clone();
        lang_row = lang_row.child(
            ActionButton::new(SharedString::from(format!(
                "settings-locale-{}",
                lang.code()
            )))
            .label(lang.display_name())
            .active(selected_locale == lang)
            .disabled(disabled)
            .palette(palette)
            .on_click(move |_, window, cx| on_locale(lang, window, cx)),
        );
    }

    let mut my_profile_row = div().flex().flex_row().flex_wrap().gap(px(6.0)).child({
        let on_pick = on_my_profile.clone();
        ActionButton::new("settings-my-profile-none")
            .label(I18n::t(locale, "profile", "empty"))
            .active(draft.draft_my_player_profile.is_none())
            .disabled(disabled)
            .palette(palette)
            .on_click(move |_, window, cx| on_pick(None, window, cx))
    });

    for vp in virtual_profiles {
        let vp_id = vp.id;
        let on_pick = on_my_profile.clone();
        my_profile_row = my_profile_row.child(
            ActionButton::new(SharedString::from(format!("settings-my-profile-{vp_id}")))
                .label(vp.name.clone())
                .active(draft.draft_my_player_profile == Some(vp_id))
                .disabled(disabled)
                .palette(palette)
                .on_click(move |_, window, cx| on_pick(Some(vp_id), window, cx)),
        );
    }

    div()
        .id("settings-section-general")
        .v_flex()
        .gap(px(18.0))
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .child(
                    div()
                        .text_panel()
                        .text_color(palette.text)
                        .child(I18n::t(locale, "settings", "language")),
                )
                .child(lang_row)
                .child(
                    div()
                        .text_meta()
                        .text_color(palette.muted)
                        .child(I18n::format(
                            locale,
                            "settings",
                            "languageHelp",
                            &[("action", Param::from(apply_label))],
                        )),
                ),
        )
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .child(div().text_panel().text_color(palette.text).child(I18n::t(
                    locale,
                    "settings",
                    "playerName",
                )))
                .child(
                    div()
                        .w(px(300.0))
                        .child(Input::new(player_name_input).id("settings-player-name-input")),
                )
                .child(div().text_meta().text_color(palette.muted).child(I18n::t(
                    locale,
                    "settings",
                    "playerHelp",
                ))),
        )
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .pt(px(10.0))
                .border_t_1()
                .border_color(palette.border)
                .child(
                    div()
                        .text_panel()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(palette.text)
                        .child(I18n::t(locale, "player-directory", "myProfile")),
                )
                .child(my_profile_row)
                .child(div().text_meta().text_color(palette.muted).child(I18n::t(
                    locale,
                    "player-directory",
                    "myProfileHelp",
                ))),
        )
}
