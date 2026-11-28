//! Settings → Board section (`BoardSettingsSection.tsx`).
//!
//! Two-column desktop layout matching `BoardSettingsSection.tsx`:
//! - Left: independently scrollable 2-column `BoardGalleryTile` grid covering
//!   all 9 built-in miniature board previews and any discovered BGBlitz ZIPs.
//! - Right (always visible, never scrolls out of view): selected theme header,
//!   3 compact dropdown pickers for Checkers, Doubling cube, and Dice in a
//!   single row, and a large live GPU `BoardView` preview directly underneath
//!   showing both dice colors, bar/bear-off checkers, and the doubling cube.

use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::icons::{ActionIconName, action_icon};
use gpui_kit::{
    App, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt, Window, base::StyledExt, div,
    prelude::FluentBuilder, px,
};
use neondojo::configuration::ConfigurationAppearance;
use neondojo_board::{
    BgBlitzThemeEntry, BoardGalleryTile, BoardPosition, BoardThemeConfig, BoardView,
    BuiltInThemeId, MATCH_BOARD_DICE,
};
use neondojo_i18n::{I18n, Param};
use std::rc::Rc;

pub type BoardThemeHandler = Rc<dyn Fn(String, &mut Window, &mut App) + 'static>;
pub type OptionalThemeHandler = Rc<dyn Fn(Option<String>, &mut Window, &mut App) + 'static>;
pub type BoardPickerHandler =
    Rc<dyn Fn(Option<BoardArtworkPicker>, &mut Window, &mut App) + 'static>;

/// Which independent artwork dropdown menu is currently open in Settings → Board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardArtworkPicker {
    Checkers,
    Cube,
    Dice,
}

/// Callbacks mutating independent board artwork slots and dropdown state.
#[derive(Clone)]
pub struct BoardSectionCallbacks {
    pub on_board_theme: BoardThemeHandler,
    pub on_checker_theme: OptionalThemeHandler,
    pub on_cube_theme: OptionalThemeHandler,
    pub on_dice_theme: OptionalThemeHandler,
    pub on_toggle_picker: BoardPickerHandler,
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

/// Fixed legal GNUbg preview position matching `BoardSettingsSection.tsx` with
/// checkers on points, on both bars, and borne off on both sides.
pub fn preview_board_position() -> BoardPosition {
    BoardPosition {
        board: [
            -1, -1, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -4, 4, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0, 1, 1,
        ],
        dice: vec![3, 2],
        gnubg_match_id: "EYHpACAAGAAE".to_string(),
        ..BoardPosition::default()
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

fn resolve_theme_label(id: &str, theme_options: &[(String, SharedString)]) -> SharedString {
    theme_options
        .iter()
        .find(|(opt_id, _)| opt_id == id)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| SharedString::from(id.to_string()))
}

type PickerSelectHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

struct PickerOption {
    element_id: SharedString,
    label: SharedString,
    active: bool,
    on_select: PickerSelectHandler,
}

struct PickerTriggerProps {
    slot: BoardArtworkPicker,
    element_id: &'static str,
    field_label: &'static str,
    current_value: SharedString,
    is_open: bool,
    disabled: bool,
    palette: NeonPalette,
    on_toggle: BoardPickerHandler,
}

fn render_picker_trigger(props: PickerTriggerProps) -> impl IntoElement {
    let PickerTriggerProps {
        slot,
        element_id,
        field_label,
        current_value,
        is_open,
        disabled,
        palette,
        on_toggle,
    } = props;
    let border_color = if is_open {
        palette.accent
    } else {
        palette.control_border
    };
    let chevron = if is_open {
        ActionIconName::Expand
    } else {
        ActionIconName::Collapse
    };

    div()
        .v_flex()
        .flex_1()
        .min_w(px(0.0))
        .gap(px(4.0))
        .child(
            div()
                .text_meta()
                .text_color(palette.muted)
                .overflow_hidden()
                .whitespace_nowrap()
                .child(field_label),
        )
        .child(
            div()
                .id(element_id)
                .test_support()
                .h_flex()
                .items_center()
                .justify_between()
                .gap(px(6.0))
                .h(px(30.0))
                .px(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(border_color)
                .bg(if is_open {
                    palette.selected
                } else {
                    palette.surface
                })
                .when(!disabled, |this| {
                    this.cursor_pointer()
                        .hover(|s| s.border_color(palette.accent))
                        .on_click(move |_, window, cx| {
                            on_toggle(if is_open { None } else { Some(slot) }, window, cx);
                        })
                })
                .when(disabled, |this| this.opacity(0.55))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_table()
                        .text_color(palette.text)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(current_value),
                )
                .child(action_icon(chevron, 13.0, palette.muted)),
        )
}

fn render_picker_menu(
    slot: BoardArtworkPicker,
    options: Vec<PickerOption>,
    palette: NeonPalette,
    on_close: BoardPickerHandler,
) -> impl IntoElement {
    let mut list = div()
        .id(match slot {
            BoardArtworkPicker::Checkers => "settings-picker-menu-checkers",
            BoardArtworkPicker::Cube => "settings-picker-menu-cube",
            BoardArtworkPicker::Dice => "settings-picker-menu-dice",
        })
        .test_support()
        .v_flex()
        .gap(px(2.0))
        .w(px(224.0))
        .max_h(px(290.0))
        .overflow_y_scroll()
        .p(px(4.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(palette.control_border)
        .bg(palette.surface)
        .shadow_lg();

    for opt in options {
        let on_select = opt.on_select;
        let close = on_close.clone();
        list = list.child(
            div()
                .id(opt.element_id)
                .test_support()
                .h_flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .px(px(8.0))
                .py(px(5.0))
                .rounded(px(5.0))
                .cursor_pointer()
                .bg(if opt.active {
                    palette.selected
                } else {
                    gpui_kit::Hsla::transparent_black()
                })
                .hover(|s| s.bg(palette.surface_alt))
                .on_click(move |_, window, cx| {
                    on_select(window, cx);
                    close(None, window, cx);
                })
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_table()
                        .when(opt.active, |d| d.font_weight(FontWeight::SEMIBOLD))
                        .text_color(if opt.active {
                            palette.accent
                        } else {
                            palette.text
                        })
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(opt.label),
                )
                .when(opt.active, |this| {
                    this.child(action_icon(ActionIconName::Confirm, 13.0, palette.accent))
                }),
        );
    }

    let menu_anchor = match slot {
        BoardArtworkPicker::Checkers => div().absolute().top(px(94.0)).left(px(12.0)).child(list),
        BoardArtworkPicker::Cube => div().absolute().top(px(94.0)).left(px(110.0)).child(list),
        BoardArtworkPicker::Dice => div().absolute().top(px(94.0)).right(px(12.0)).child(list),
    };

    div()
        .id("settings-board-picker-backdrop")
        .absolute()
        .inset_0()
        .on_click(move |_, window, cx| on_close(None, window, cx))
        .child(menu_anchor)
}

/// Renders Settings → Board with an independently scrollable gallery on the left
/// and a pinned live `BoardView` + compact Checkers/Cube/Dice dropdowns on the right.
#[allow(clippy::too_many_arguments)]
pub fn render_board_section(
    appearance: &ConfigurationAppearance,
    preview_board: &Entity<BoardView>,
    bgblitz_themes: &[BgBlitzThemeEntry],
    open_picker: Option<BoardArtworkPicker>,
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

    let selected_board_name = resolve_theme_label(&appearance.board_theme, &theme_options);
    let preview_title = I18n::format(
        locale,
        "settings",
        "preview",
        &[("theme", Param::from(selected_board_name.as_ref()))],
    );

    let checker_label = match appearance.checker_theme.as_deref() {
        None => SharedString::from(follow_board),
        Some(id) => resolve_theme_label(id, &theme_options),
    };
    let cube_label = match appearance.cube_theme.as_deref() {
        None => SharedString::from(follow_board),
        Some(id) => resolve_theme_label(id, &theme_options),
    };
    let dice_label = match appearance.dice_theme.as_deref() {
        None => SharedString::from(follow_checkers),
        Some(id) if id == MATCH_BOARD_DICE => SharedString::from(follow_board),
        Some(id) => resolve_theme_label(id, &theme_options),
    };

    let selected_badge = Some((
        SharedString::from(I18n::t(locale, "settings", "selected")),
        palette.accent_text,
    ));

    // Left column (1fr): independently scrollable 2-column gallery matching `settings.css`.
    let mut gallery_grid = div().flex().flex_row().flex_wrap().gap(px(10.0));
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
        gallery_grid = gallery_grid.child(
            div()
                .id(SharedString::from(format!("settings-board-tile-wrap-{id}")))
                .w(px(130.0))
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    if !disabled {
                        on_board(target.clone(), window, cx);
                    }
                })
                .child(tile.render_element_with_badge(
                    SharedString::from(format!("board-gallery-{id}")),
                    selected_badge.clone(),
                )),
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
        gallery_grid = gallery_grid.child(
            div()
                .id(SharedString::from(format!("settings-board-tile-wrap-{id}")))
                .w(px(130.0))
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    if !disabled {
                        on_board(target.clone(), window, cx);
                    }
                })
                .child(tile.render_element_with_badge(
                    SharedString::from(format!("board-gallery-{id}")),
                    selected_badge.clone(),
                )),
        );
    }

    let left_gallery = div()
        .v_flex()
        .w(px(282.0))
        .flex_shrink_0()
        .h_full()
        .min_h(px(0.0))
        .gap(px(6.0))
        .child(
            div()
                .text_label()
                .text_color(palette.muted)
                .child(I18n::t(locale, "settings", "gallery")),
        )
        .child(
            div()
                .id("settings-board-gallery-scroll")
                .test_support()
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .pr(px(4.0))
                .child(gallery_grid),
        );

    // Right column: pinned live preview card with 3 compact dropdown pickers above the board.
    let picker_row = div()
        .h_flex()
        .w_full()
        .gap(px(8.0))
        .child(render_picker_trigger(PickerTriggerProps {
            slot: BoardArtworkPicker::Checkers,
            element_id: "settings-picker-checkers",
            field_label: I18n::t(locale, "profile", "checker_theme"),
            current_value: checker_label,
            is_open: open_picker == Some(BoardArtworkPicker::Checkers),
            disabled,
            palette,
            on_toggle: cb.on_toggle_picker.clone(),
        }))
        .child(render_picker_trigger(PickerTriggerProps {
            slot: BoardArtworkPicker::Cube,
            element_id: "settings-picker-cube",
            field_label: I18n::t(locale, "profile", "cube_theme"),
            current_value: cube_label,
            is_open: open_picker == Some(BoardArtworkPicker::Cube),
            disabled,
            palette,
            on_toggle: cb.on_toggle_picker.clone(),
        }))
        .child(render_picker_trigger(PickerTriggerProps {
            slot: BoardArtworkPicker::Dice,
            element_id: "settings-picker-dice",
            field_label: I18n::t(locale, "profile", "dice_theme"),
            current_value: dice_label,
            is_open: open_picker == Some(BoardArtworkPicker::Dice),
            disabled,
            palette,
            on_toggle: cb.on_toggle_picker.clone(),
        }));

    let open_menu = open_picker.filter(|_| !disabled).map(|slot| {
        let mut options = Vec::new();
        match slot {
            BoardArtworkPicker::Checkers => {
                let on_pick = cb.on_checker_theme.clone();
                options.push(PickerOption {
                    element_id: "settings-checker-match-board".into(),
                    label: SharedString::from(follow_board),
                    active: appearance.checker_theme.is_none(),
                    on_select: Rc::new(move |window, cx| on_pick(None, window, cx)),
                });
                for (id, label) in &theme_options {
                    let on_pick = cb.on_checker_theme.clone();
                    let target = id.clone();
                    options.push(PickerOption {
                        element_id: SharedString::from(format!("settings-checker-{id}")),
                        label: label.clone(),
                        active: appearance.checker_theme.as_deref() == Some(id.as_str()),
                        on_select: Rc::new(move |window, cx| {
                            on_pick(Some(target.clone()), window, cx);
                        }),
                    });
                }
            }
            BoardArtworkPicker::Cube => {
                let on_pick = cb.on_cube_theme.clone();
                options.push(PickerOption {
                    element_id: "settings-cube-match-board".into(),
                    label: SharedString::from(follow_board),
                    active: appearance.cube_theme.is_none(),
                    on_select: Rc::new(move |window, cx| on_pick(None, window, cx)),
                });
                for (id, label) in &theme_options {
                    let on_pick = cb.on_cube_theme.clone();
                    let target = id.clone();
                    options.push(PickerOption {
                        element_id: SharedString::from(format!("settings-cube-{id}")),
                        label: label.clone(),
                        active: appearance.cube_theme.as_deref() == Some(id.as_str()),
                        on_select: Rc::new(move |window, cx| {
                            on_pick(Some(target.clone()), window, cx);
                        }),
                    });
                }
            }
            BoardArtworkPicker::Dice => {
                let on_match_board = cb.on_dice_theme.clone();
                options.push(PickerOption {
                    element_id: "settings-dice-match-board".into(),
                    label: SharedString::from(follow_board),
                    active: appearance.dice_theme.as_deref() == Some(MATCH_BOARD_DICE),
                    on_select: Rc::new(move |window, cx| {
                        on_match_board(Some(MATCH_BOARD_DICE.to_string()), window, cx);
                    }),
                });
                let on_match_checkers = cb.on_dice_theme.clone();
                options.push(PickerOption {
                    element_id: "settings-dice-match-checkers".into(),
                    label: SharedString::from(follow_checkers),
                    active: appearance.dice_theme.is_none(),
                    on_select: Rc::new(move |window, cx| on_match_checkers(None, window, cx)),
                });
                for (id, label) in &theme_options {
                    let on_pick = cb.on_dice_theme.clone();
                    let target = id.clone();
                    options.push(PickerOption {
                        element_id: SharedString::from(format!("settings-dice-{id}")),
                        label: label.clone(),
                        active: appearance.dice_theme.as_deref() == Some(id.as_str()),
                        on_select: Rc::new(move |window, cx| {
                            on_pick(Some(target.clone()), window, cx);
                        }),
                    });
                }
            }
        }
        render_picker_menu(slot, options, palette, cb.on_toggle_picker.clone())
    });

    let right_preview = div()
        .id("settings-board-preview-pane")
        .test_support()
        .relative()
        .v_flex()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .min_h(px(0.0))
        .gap(px(10.0))
        .p(px(14.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(palette.border)
        .bg(palette.surface_alt)
        .child(
            div()
                .text_panel()
                .text_color(palette.text)
                .child(preview_title),
        )
        .child(picker_row)
        .child(
            div()
                .flex_1()
                .min_h(px(300.0))
                .w_full()
                .child(preview_board.clone()),
        )
        .when_some(open_menu, |this, menu| this.child(menu));

    div()
        .id("settings-section-board")
        .test_support()
        .v_flex()
        .h_full()
        .min_h(px(0.0))
        .gap(px(10.0))
        .child(div().text_meta().text_color(palette.muted).child(I18n::t(
            locale,
            "settings",
            "boardHelp",
        )))
        .child(
            div()
                .h_flex()
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .gap(px(20.0))
                .items_stretch()
                .child(left_gallery)
                .child(right_preview),
        )
}
