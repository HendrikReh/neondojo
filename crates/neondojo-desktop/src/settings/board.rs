//! Settings → Board section (`BoardSettingsSection.tsx`).
//!
//! Embeds a live GPU `neondojo_board::BoardView` preview alongside independent
//! selectors for Board surface, Checker set, Doubling cube, and Dice, plus a
//! responsive `neondojo_board::BoardGalleryTile` grid covering all 9 built-in
//! themes and any discovered BGBlitz ZIP archives.

use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use gpui_kit::{
    App, Entity, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, base::StyledExt, div, px,
};
use neondojo::configuration::ConfigurationAppearance;
use neondojo_board::{
    BgBlitzThemeEntry, BoardGalleryTile, BoardThemeConfig, BoardView, BuiltInThemeId,
    MATCH_BOARD_DICE,
};
use neondojo_i18n::I18n;
use std::rc::Rc;

pub type BoardThemeHandler = Rc<dyn Fn(String, &mut Window, &mut App) + 'static>;
pub type OptionalThemeHandler = Rc<dyn Fn(Option<String>, &mut Window, &mut App) + 'static>;

/// Callbacks mutating independent board artwork slots.
#[derive(Clone)]
pub struct BoardSectionCallbacks {
    pub on_board_theme: BoardThemeHandler,
    pub on_checker_theme: OptionalThemeHandler,
    pub on_cube_theme: OptionalThemeHandler,
    pub on_dice_theme: OptionalThemeHandler,
}

/// Converts [`ConfigurationAppearance`] into [`neondojo_board::BoardThemeConfig`].
pub fn appearance_to_board_config(appearance: &ConfigurationAppearance) -> BoardThemeConfig {
    BoardThemeConfig {
        board_theme: appearance.board_theme.clone(),
        checker_theme: appearance.checker_theme.clone(),
        cube_theme: appearance.cube_theme.clone(),
        dice_theme: appearance.dice_theme.clone(),
    }
}

fn builtin_theme_label(id: BuiltInThemeId) -> &'static str {
    match id {
        BuiltInThemeId::Forest => "Forest",
        BuiltInThemeId::Walnut => "Walnut",
        BuiltInThemeId::Slate => "Slate",
        BuiltInThemeId::Moonstone => "Moonstone",
        BuiltInThemeId::Viridian => "Viridian",
        BuiltInThemeId::TerraRossa => "Terra Rossa",
        BuiltInThemeId::Ion => "Ion",
        BuiltInThemeId::Pelagic => "Pelagic",
        BuiltInThemeId::Carbon => "Carbon",
    }
}

/// Renders Settings → Board with live `BoardView`, 4 independent artwork rows,
/// and the `BoardGalleryTile` grid.
#[allow(clippy::too_many_arguments)]
pub fn render_board_section(
    appearance: &ConfigurationAppearance,
    preview_board: &Entity<BoardView>,
    bgblitz_themes: &[BgBlitzThemeEntry],
    disabled: bool,
    palette: NeonPalette,
    cb: BoardSectionCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let follow_board = I18n::t(locale, "settings", "followBoard");
    let follow_checkers = I18n::t(locale, "settings", "followCheckers");

    let mut theme_options: Vec<(String, SharedString)> = BuiltInThemeId::ALL
        .into_iter()
        .map(|id| {
            (
                id.as_str().to_string(),
                SharedString::from(builtin_theme_label(id)),
            )
        })
        .collect();
    for entry in bgblitz_themes {
        theme_options.push((entry.id.clone(), SharedString::from(entry.name.clone())));
    }

    // Checker set selector
    let mut checker_row = div().flex().flex_row().flex_wrap().gap(px(6.0)).child({
        let on_pick = cb.on_checker_theme.clone();
        ActionButton::new("settings-checker-match-board")
            .label(follow_board)
            .active(appearance.checker_theme.is_none())
            .disabled(disabled)
            .palette(palette)
            .on_click(move |_, window, cx| on_pick(None, window, cx))
    });
    for (id, label) in theme_options.iter().take(9) {
        let on_pick = cb.on_checker_theme.clone();
        let target = id.clone();
        checker_row = checker_row.child(
            ActionButton::new(SharedString::from(format!("settings-checker-{id}")))
                .label(label.clone())
                .active(appearance.checker_theme.as_deref() == Some(id.as_str()))
                .disabled(disabled)
                .palette(palette)
                .on_click(move |_, window, cx| on_pick(Some(target.clone()), window, cx)),
        );
    }

    // Doubling cube selector
    let mut cube_row = div().flex().flex_row().flex_wrap().gap(px(6.0)).child({
        let on_pick = cb.on_cube_theme.clone();
        ActionButton::new("settings-cube-match-board")
            .label(follow_board)
            .active(appearance.cube_theme.is_none())
            .disabled(disabled)
            .palette(palette)
            .on_click(move |_, window, cx| on_pick(None, window, cx))
    });
    for (id, label) in theme_options.iter().take(9) {
        let on_pick = cb.on_cube_theme.clone();
        let target = id.clone();
        cube_row = cube_row.child(
            ActionButton::new(SharedString::from(format!("settings-cube-{id}")))
                .label(label.clone())
                .active(appearance.cube_theme.as_deref() == Some(id.as_str()))
                .disabled(disabled)
                .palette(palette)
                .on_click(move |_, window, cx| on_pick(Some(target.clone()), window, cx)),
        );
    }

    // Dice selector (`match-board`, `None` = follow checkers, or explicit theme)
    let dice_row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(px(6.0))
        .child({
            let on_pick = cb.on_dice_theme.clone();
            ActionButton::new("settings-dice-match-board")
                .label(follow_board)
                .active(appearance.dice_theme.as_deref() == Some(MATCH_BOARD_DICE))
                .disabled(disabled)
                .palette(palette)
                .on_click(move |_, window, cx| {
                    on_pick(Some(MATCH_BOARD_DICE.to_string()), window, cx);
                })
        })
        .child({
            let on_pick = cb.on_dice_theme.clone();
            ActionButton::new("settings-dice-match-checkers")
                .label(follow_checkers)
                .active(appearance.dice_theme.is_none())
                .disabled(disabled)
                .palette(palette)
                .on_click(move |_, window, cx| on_pick(None, window, cx))
        });

    // Responsive gallery using `neondojo_board::BoardGalleryTile`
    let mut gallery = div().flex().flex_row().flex_wrap().gap(px(10.0));
    for builtin in BuiltInThemeId::ALL {
        let id = builtin.as_str().to_string();
        let selected = appearance.board_theme == id;
        let on_board = cb.on_board_theme.clone();
        let target = id.clone();
        let tile = BoardGalleryTile {
            theme_id: id.clone(),
            label: SharedString::from(builtin_theme_label(builtin)),
            selected,
            bgblitz_entry: None,
            accent: palette.accent,
            border: palette.border,
            surface_selected: palette.selected,
        };
        gallery = gallery.child(
            div()
                .id(SharedString::from(format!("settings-board-tile-wrap-{id}")))
                .w(px(148.0))
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    if !disabled {
                        on_board(target.clone(), window, cx);
                    }
                })
                .child(tile.render_element(SharedString::from(format!("board-gallery-{id}")))),
        );
    }

    for entry in bgblitz_themes {
        let id = entry.id.clone();
        let selected = appearance.board_theme == id;
        let on_board = cb.on_board_theme.clone();
        let target = id.clone();
        let tile = BoardGalleryTile {
            theme_id: id.clone(),
            label: SharedString::from(entry.name.clone()),
            selected,
            bgblitz_entry: Some(entry.clone()),
            accent: palette.accent,
            border: palette.border,
            surface_selected: palette.selected,
        };
        gallery = gallery.child(
            div()
                .id(SharedString::from(format!("settings-board-tile-wrap-{id}")))
                .w(px(148.0))
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    if !disabled {
                        on_board(target.clone(), window, cx);
                    }
                })
                .child(tile.render_element(SharedString::from(format!("board-gallery-{id}")))),
        );
    }

    div()
        .id("settings-section-board")
        .v_flex()
        .gap(px(14.0))
        .child(div().text_meta().text_color(palette.muted).child(I18n::t(
            locale,
            "settings",
            "boardHelp",
        )))
        .child(
            div()
                .w_full()
                .h(px(220.0))
                .rounded(px(8.0))
                .overflow_hidden()
                .border_1()
                .border_color(palette.border)
                .child(preview_board.clone()),
        )
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .child(
                    div()
                        .text_panel()
                        .text_color(palette.text)
                        .child(I18n::t(locale, "settings", "gallery")),
                )
                .child(gallery),
        )
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .pt(px(8.0))
                .border_t_1()
                .border_color(palette.border)
                .child(div().text_label().text_color(palette.muted).child(I18n::t(
                    locale,
                    "profile",
                    "checker_theme",
                )))
                .child(checker_row),
        )
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .child(div().text_label().text_color(palette.muted).child(I18n::t(
                    locale,
                    "profile",
                    "cube_theme",
                )))
                .child(cube_row),
        )
        .child(
            div()
                .v_flex()
                .gap(px(6.0))
                .child(div().text_label().text_color(palette.muted).child(I18n::t(
                    locale,
                    "profile",
                    "dice_theme",
                )))
                .child(dice_row),
        )
}
