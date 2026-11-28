//! Settings → Profiles panel (`ProfileSettingsPanel.tsx`).
//!
//! Supports listing, creating, renaming, overwriting, activating, deleting, and
//! restoring named configuration profiles, displaying live field diffs against
//! the saved profile snapshot, and resolving 409 revision conflicts.

use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::icons::ActionIconName;
use gpui_kit::{
    App, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString, Styled,
    Window,
    base::StyledExt,
    component::input::{Input, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::configuration::{ConfigurationDifference, ConfigurationProfile};
use neondojo_i18n::{I18n, Locale, Param};
use serde_json::Value;
use std::rc::Rc;
use uuid::Uuid;

pub type ProfileActionHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;
pub type ProfileIdHandler = Rc<dyn Fn(Uuid, &mut Window, &mut App) + 'static>;

/// Callbacks driving profile CRUD & 409 conflict recovery.
#[derive(Clone)]
pub struct ProfileSectionCallbacks {
    pub on_create: ProfileActionHandler,
    pub on_activate: ProfileIdHandler,
    pub on_overwrite: ProfileIdHandler,
    pub on_rename: ProfileIdHandler,
    pub on_delete: ProfileIdHandler,
    pub on_restore_all: ProfileActionHandler,
    pub on_reload_server: ProfileActionHandler,
}

/// Translates a dotted `ConfigurationDifference.path` (e.g. `"appearance.ui_theme"`)
/// into a localized human-readable label using the `"profile"` message group.
pub fn format_diff_path(locale: Locale, path: &str) -> String {
    path.split('.')
        .map(|segment| I18n::try_t(locale, "profile", segment).unwrap_or(segment))
        .collect::<Vec<_>>()
        .join(" › ")
}

/// Formats a JSON configuration value concisely for the diff table.
pub fn format_diff_value(locale: Locale, value: &Value) -> String {
    match value {
        Value::Null => I18n::t(locale, "profile", "empty").to_string(),
        Value::Bool(true) => I18n::t(locale, "profile", "yes").to_string(),
        Value::Bool(false) => I18n::t(locale, "profile", "no").to_string(),
        Value::String(s) if s.is_empty() => I18n::t(locale, "profile", "empty").to_string(),
        Value::String(s) => I18n::try_t(locale, "profile", s)
            .unwrap_or(s.as_str())
            .to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(items) => format!("[{}]", items.len()),
        Value::Object(map) => format!("{{{}}}", map.len()),
    }
}

/// Renders Settings → Profiles.
#[allow(clippy::too_many_arguments)]
pub fn render_profiles_section(
    profiles: &[ConfigurationProfile],
    active_profile_id: Option<Uuid>,
    differences: &[ConfigurationDifference],
    profile_name_input: &Entity<InputState>,
    conflict: bool,
    error_message: Option<&str>,
    busy: bool,
    palette: NeonPalette,
    cb: ProfileSectionCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);

    let mut list = div().v_flex().gap(px(8.0));
    for prof in profiles {
        let is_active = active_profile_id == Some(prof.id);
        let id = prof.id;
        list = list.child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .gap(px(10.0))
                .p(px(10.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(if is_active {
                    palette.accent
                } else {
                    palette.border
                })
                .bg(if is_active {
                    palette.selected
                } else {
                    palette.surface
                })
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_panel()
                                .text_color(palette.text)
                                .child(prof.name.clone()),
                        )
                        .when(is_active, |this| {
                            this.child(
                                div()
                                    .text_micro()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(palette.accent)
                                    .text_color(palette.accent_text)
                                    .child(I18n::t(locale, "profile", "active")),
                            )
                        }),
                )
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(6.0))
                        .when(!is_active, |this| {
                            let on_activate = cb.on_activate.clone();
                            this.child(
                                ActionButton::new(SharedString::from(format!(
                                    "profile-activate-{id}"
                                )))
                                .label(I18n::t(locale, "profile", "activate"))
                                .disabled(busy)
                                .palette(palette)
                                .on_click(move |_, window, cx| on_activate(id, window, cx)),
                            )
                        })
                        .child({
                            let on_overwrite = cb.on_overwrite.clone();
                            ActionButton::new(SharedString::from(format!("profile-save-{id}")))
                                .icon(ActionIconName::Save)
                                .label(I18n::t(locale, "profile", "overwrite"))
                                .disabled(busy)
                                .palette(palette)
                                .on_click(move |_, window, cx| on_overwrite(id, window, cx))
                        })
                        .child({
                            let on_rename = cb.on_rename.clone();
                            ActionButton::new(SharedString::from(format!("profile-rename-{id}")))
                                .icon(ActionIconName::Edit)
                                .tooltip(I18n::t(locale, "profile", "rename"))
                                .dense_nav()
                                .disabled(busy)
                                .palette(palette)
                                .on_click(move |_, window, cx| on_rename(id, window, cx))
                        })
                        .child({
                            let on_delete = cb.on_delete.clone();
                            ActionButton::new(SharedString::from(format!("profile-delete-{id}")))
                                .icon(ActionIconName::Delete)
                                .tooltip(if is_active {
                                    I18n::t(locale, "profile", "deleteActive")
                                } else {
                                    I18n::t(locale, "profile", "delete")
                                })
                                .danger()
                                .dense_nav()
                                .disabled(busy || is_active)
                                .palette(palette)
                                .on_click(move |_, window, cx| on_delete(id, window, cx))
                        }),
                ),
        );
    }

    // Live field diff table against saved profile snapshot
    let diff_section = if differences.is_empty() {
        div()
            .text_meta()
            .text_color(palette.muted)
            .child(I18n::t(locale, "profile", "noDifferences"))
            .into_any_element()
    } else {
        let mut table = div()
            .v_flex()
            .gap(px(4.0))
            .p(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(palette.border)
            .bg(palette.surface)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .pb(px(6.0))
                    .child(
                        div()
                            .text_label()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.text)
                            .child(I18n::format(
                                locale,
                                "profile",
                                "changeCount",
                                &[("count", Param::from(differences.len() as i64))],
                            )),
                    )
                    .child(
                        ActionButton::new("profile-reset-all")
                            .icon(ActionIconName::Reset)
                            .label(I18n::t(locale, "profile", "resetAll"))
                            .disabled(busy)
                            .palette(palette)
                            .on_click({
                                let on_restore = cb.on_restore_all.clone();
                                move |_, window, cx| on_restore(window, cx)
                            }),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .gap(px(8.0))
                    .pb(px(4.0))
                    .border_b_1()
                    .border_color(palette.border)
                    .text_table_heading()
                    .text_color(palette.muted)
                    .child(div().flex_1().child(I18n::t(locale, "profile", "field")))
                    .child(
                        div()
                            .w(px(110.0))
                            .child(I18n::t(locale, "profile", "saved")),
                    )
                    .child(
                        div()
                            .w(px(110.0))
                            .child(I18n::t(locale, "profile", "current")),
                    ),
            );

        for diff in differences {
            table = table.child(
                div()
                    .h_flex()
                    .gap(px(8.0))
                    .py(px(3.0))
                    .text_table()
                    .child(
                        div()
                            .flex_1()
                            .text_color(palette.text)
                            .truncate()
                            .child(format_diff_path(locale, &diff.path)),
                    )
                    .child(
                        div()
                            .w(px(110.0))
                            .text_color(palette.muted)
                            .truncate()
                            .child(format_diff_value(locale, &diff.saved)),
                    )
                    .child(
                        div()
                            .w(px(110.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.accent)
                            .truncate()
                            .child(format_diff_value(locale, &diff.current)),
                    ),
            );
        }
        table.into_any_element()
    };

    div()
        .id("settings-section-profiles")
        .v_flex()
        .gap(px(14.0))
        .when(conflict, |this| {
            let on_reload = cb.on_reload_server.clone();
            let on_create = cb.on_create.clone();
            this.child(
                div()
                    .v_flex()
                    .gap(px(8.0))
                    .child(
                        StatusBanner::new(
                            "profile-conflict-banner",
                            BannerKind::Warning,
                            I18n::t(locale, "profile", "conflict"),
                        )
                        .palette(palette),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap(px(8.0))
                            .child(
                                ActionButton::new("profile-conflict-reload")
                                    .icon(ActionIconName::Refresh)
                                    .label(I18n::t(locale, "profile", "reloadServer"))
                                    .disabled(busy)
                                    .palette(palette)
                                    .on_click(move |_, window, cx| on_reload(window, cx)),
                            )
                            .child(
                                ActionButton::new("profile-conflict-save-new")
                                    .icon(ActionIconName::Add)
                                    .label(I18n::t(locale, "profile", "saveConflict"))
                                    .primary()
                                    .disabled(busy)
                                    .palette(palette)
                                    .on_click(move |_, window, cx| on_create(window, cx)),
                            ),
                    ),
            )
        })
        .when_some(error_message, |this, err| {
            this.child(
                StatusBanner::new("profile-error-banner", BannerKind::Error, err.to_string())
                    .palette(palette),
            )
        })
        .child(
            div()
                .h_flex()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .child(Input::new(profile_name_input).id("settings-profile-name-input")),
                )
                .child(
                    ActionButton::new("profile-create-button")
                        .icon(ActionIconName::Add)
                        .label(I18n::t(locale, "profile", "create"))
                        .primary()
                        .disabled(busy)
                        .palette(palette)
                        .on_click({
                            let on_create = cb.on_create.clone();
                            move |_, window, cx| on_create(window, cx)
                        }),
                ),
        )
        .child(list)
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .pt(px(8.0))
                .border_t_1()
                .border_color(palette.border)
                .child(div().text_panel().text_color(palette.text).child(I18n::t(
                    locale,
                    "profile",
                    "differences",
                )))
                .child(diff_section),
        )
}
