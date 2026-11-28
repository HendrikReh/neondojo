//! Settings → Appearance section (`AppearanceSettingsSection.tsx` & `UiAppearancePreview.tsx`).
//!
//! Renders Interface Theme (`Graphite`, `Club`, `Paper`) and Display Mode
//! (`Light`, `Dark`, `System`) pickers alongside a scoped `&NeonPalette`
//! live preview card that does NOT mutate the active shell theme until Apply.

use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonPaletteId, NeonTypographyExt, ResolvedMode, ThemeMode};
use crate::ui::action_button::ActionButton;
use gpui_kit::{
    App, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString, Styled,
    TestSupportExt, Window, base::StyledExt, div, px,
};
use neondojo_i18n::{I18n, Param, format_equity};
use std::rc::Rc;

pub type SelectPaletteHandler = Rc<dyn Fn(NeonPaletteId, &mut Window, &mut App) + 'static>;
pub type SelectModeHandler = Rc<dyn Fn(ThemeMode, &mut Window, &mut App) + 'static>;

/// Scoped `&NeonPalette` live interface preview (`UiAppearancePreview.tsx`).
pub fn render_ui_appearance_preview(preview: &NeonPalette, cx: &App) -> impl IntoElement {
    let locale = current_locale(cx);
    let theme_label = I18n::t(locale, "ui-appearance", preview.id.as_str());
    let mode_label = I18n::t(
        locale,
        "ui-appearance",
        if preview.mode.is_dark() {
            "dark"
        } else {
            "light"
        },
    );

    let rows = [
        (
            "13/8 6/5",
            I18n::t(locale, "ui-appearance", "best"),
            preview.success_text,
            preview.success_bg,
            format_equity(locale, 0.214),
        ),
        (
            "24/18",
            I18n::t(locale, "ui-appearance", "warning"),
            preview.warning_text,
            preview.warning_bg,
            format_equity(locale, 0.172),
        ),
        (
            "13/7",
            I18n::t(locale, "ui-appearance", "error"),
            preview.danger_text,
            preview.danger_bg,
            format_equity(locale, -0.021),
        ),
    ];

    let mut table = div()
        .v_flex()
        .gap(px(4.0))
        .p(px(10.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(preview.border)
        .bg(preview.surface)
        .child(
            div()
                .text_label()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(preview.text)
                .child(I18n::t(locale, "ui-appearance", "candidates")),
        )
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .pb(px(4.0))
                .border_b_1()
                .border_color(preview.border)
                .text_table_heading()
                .text_color(preview.muted)
                .child(I18n::t(locale, "ui-appearance", "move"))
                .child(I18n::t(locale, "ui-appearance", "equity")),
        );

    for (mv, badge, fg, bg, eq) in rows {
        table = table.child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .py(px(3.0))
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().text_table().text_color(preview.text).child(mv))
                        .child(
                            div()
                                .text_micro()
                                .font_weight(FontWeight::SEMIBOLD)
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(px(4.0))
                                .bg(bg)
                                .text_color(fg)
                                .child(badge),
                        ),
                )
                .child(
                    div()
                        .text_table()
                        .tabular_nums()
                        .text_color(preview.text)
                        .child(eq),
                ),
        );
    }

    div()
        .id("settings-ui-appearance-preview")
        .test_support()
        .v_flex()
        .gap(px(10.0))
        .p(px(12.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(preview.control_border)
        .bg(preview.bg)
        .child(div().text_meta().text_color(preview.muted).child(I18n::t(
            locale,
            "ui-appearance",
            "preview",
        )))
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(14.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(preview.border)
                .bg(preview.surface)
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(preview.text)
                        .child("NeonDojo"),
                )
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .pb(px(2.0))
                        .border_b_2()
                        .border_color(preview.accent)
                        .text_color(preview.text)
                        .child(I18n::t(locale, "ui-appearance", "analysis")),
                )
                .child(div().text_label().text_color(preview.muted).child(I18n::t(
                    locale,
                    "ui-appearance",
                    "database",
                ))),
        )
        .child(
            div()
                .h_flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_label()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(preview.text)
                        .child(I18n::t(locale, "ui-appearance", "position")),
                )
                .child(
                    div()
                        .text_meta()
                        .text_color(preview.muted)
                        .child(format!("{theme_label} · {mode_label}")),
                ),
        )
        .child(table)
}

/// Renders Settings → Appearance with palette/mode pickers and scoped live preview.
#[allow(clippy::too_many_arguments)]
pub fn render_appearance_section(
    selected_palette: NeonPaletteId,
    selected_mode: ThemeMode,
    system_dark: bool,
    apply_label: &str,
    disabled: bool,
    active_palette: NeonPalette,
    on_palette: SelectPaletteHandler,
    on_mode: SelectModeHandler,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let resolved_preview_mode: ResolvedMode = selected_mode.resolve(system_dark);
    let preview_palette = NeonPalette::resolve(selected_palette, resolved_preview_mode);

    let mut theme_row = div().h_flex().gap(px(8.0));
    for &pid in NeonPaletteId::ALL {
        let on_palette = on_palette.clone();
        theme_row = theme_row.child(
            ActionButton::new(SharedString::from(format!(
                "settings-palette-{}",
                pid.as_str()
            )))
            .label(I18n::t(locale, "ui-appearance", pid.as_str()))
            .active(selected_palette == pid)
            .disabled(disabled)
            .palette(active_palette)
            .on_click(move |_, window, cx| on_palette(pid, window, cx)),
        );
    }

    let mut mode_row = div().h_flex().gap(px(8.0));
    for &mode in ThemeMode::ALL {
        let on_mode = on_mode.clone();
        mode_row = mode_row.child(
            ActionButton::new(SharedString::from(format!(
                "settings-mode-{}",
                mode.as_str()
            )))
            .label(I18n::t(locale, "ui-appearance", mode.as_str()))
            .active(selected_mode == mode)
            .disabled(disabled)
            .palette(active_palette)
            .on_click(move |_, window, cx| on_mode(mode, window, cx)),
        );
    }

    div()
        .id("settings-section-appearance")
        .v_flex()
        .gap(px(16.0))
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .child(
                    div()
                        .text_panel()
                        .text_color(active_palette.text)
                        .child(I18n::t(locale, "ui-appearance", "interfaceTheme")),
                )
                .child(theme_row),
        )
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .child(
                    div()
                        .text_panel()
                        .text_color(active_palette.text)
                        .child(I18n::t(locale, "ui-appearance", "displayMode")),
                )
                .child(mode_row)
                .child(
                    div()
                        .text_meta()
                        .text_color(active_palette.muted)
                        .child(I18n::t(locale, "ui-appearance", "systemHelp")),
                ),
        )
        .child(render_ui_appearance_preview(&preview_palette, cx))
        .child(
            div()
                .text_meta()
                .text_color(active_palette.muted)
                .child(I18n::format(
                    locale,
                    "ui-appearance",
                    "help",
                    &[("action", Param::from(apply_label))],
                )),
        )
}
