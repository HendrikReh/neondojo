//! Unified NeonDojo action button mirroring `web/src/shared/ui/ActionButton.tsx`.
//!
//! Preserves exact hit-target sizes (`44×44` compact icon buttons, `26×26` dense
//! Analysis navigation buttons), outlined vs solid danger variants, inline
//! monospace keycap badges, automatic `(Shortcut)` tooltip suffixes, and
//! high-legibility disabled compact icon tinting.

use crate::theme::{
    COMPACT_ICON_BUTTON_SIZE, DENSE_BOARD_NAV_BUTTON_SIZE, FONT_MONO_FAMILY, FONT_SIZE_META,
    NeonPalette, NeonTheme, NeonTypographyExt,
};
use crate::ui::icons::{ActionIconName, DEFAULT_ACTION_ICON_SIZE, action_icon};
use gpui_kit::{
    App, ClickEvent, CursorStyle, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled, TestSupportExt,
    Window, base::StyledExt, component::tooltip::Tooltip, div, prelude::FluentBuilder, px,
};
use std::rc::Rc;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Visual emphasis variants matching `ActionButton.tsx` and `base.css`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ActionButtonVariant {
    #[default]
    Default,
    Primary,
    /// Outlined danger button (`danger_text` + `danger_border` on `surface`).
    Danger,
    /// High-emphasis filled destructive confirmation (`danger_button` bg + white text).
    DangerSolid,
    /// Borderless quiet toolbar trigger.
    Ghost,
}

/// Density & hit-target presets.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ActionButtonSize {
    /// Standard button with text label and optional icon / keycap badge.
    #[default]
    Regular,
    /// Compact `44×44` icon-only action button (`docs/design/action-icons-proposal.md`).
    Compact,
    /// Dense `26×26` board navigation control in desktop Analysis.
    DenseNav,
}

/// Formats a shortcut label into compact macOS-friendly keycap glyphs.
pub fn format_shortcut_keycap(shortcut: &str) -> String {
    let trimmed = shortcut.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let parts: Vec<&str> = trimmed.split('+').map(str::trim).collect();
    let mut out = String::new();
    for part in parts {
        match part.to_ascii_lowercase().as_str() {
            "enter" | "return" => out.push('↵'),
            "escape" | "esc" => out.push_str("Esc"),
            "shift" => out.push('⇧'),
            "cmd" | "meta" | "super" => out.push('⌘'),
            "alt" | "option" => out.push('⌥'),
            "ctrl" | "control" => out.push('⌃'),
            "arrowleft" | "left" => out.push('←'),
            "arrowright" | "right" => out.push('→'),
            "arrowup" | "up" => out.push('↑'),
            "arrowdown" | "down" => out.push('↓'),
            "space" => out.push_str("Space"),
            _ => out.push_str(part),
        }
    }
    out
}

/// Appends `(Shortcut)` to a tooltip/label when a shortcut is configured and
/// not already mentioned inside the tooltip text.
pub fn compose_action_tooltip(
    label: Option<&str>,
    tooltip: Option<&str>,
    shortcut: Option<&str>,
) -> Option<String> {
    let base = tooltip
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| label.map(str::trim).filter(|s| !s.is_empty()))?;
    let Some(shortcut) = shortcut.map(str::trim).filter(|s| !s.is_empty()) else {
        return Some(base.to_string());
    };
    let keycap = format_shortcut_keycap(shortcut);
    if base.contains(shortcut) || (!keycap.is_empty() && base.contains(&keycap)) {
        Some(base.to_string())
    } else {
        Some(format!("{base} ({keycap})"))
    }
}

/// Styled NeonDojo action button supporting regular, compact (`44×44`), and
/// dense (`26×26`) presentations.
#[derive(IntoElement)]
pub struct ActionButton {
    id: ElementId,
    label: Option<SharedString>,
    icon: Option<ActionIconName>,
    tooltip: Option<SharedString>,
    shortcut: Option<SharedString>,
    variant: ActionButtonVariant,
    size: ActionButtonSize,
    tab_stop: bool,
    active: bool,
    disabled: bool,
    palette: Option<NeonPalette>,
    on_click: Option<ClickHandler>,
}

impl ActionButton {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            label: None,
            icon: None,
            tooltip: None,
            shortcut: None,
            variant: ActionButtonVariant::Default,
            size: ActionButtonSize::Regular,
            tab_stop: false,
            active: false,
            disabled: false,
            palette: None,
            on_click: None,
        }
    }

    pub fn tab_stop(mut self) -> Self {
        self.tab_stop = true;
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn icon(mut self, icon: ActionIconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn variant(mut self, variant: ActionButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn primary(mut self) -> Self {
        self.variant = ActionButtonVariant::Primary;
        self
    }

    pub fn danger(mut self) -> Self {
        self.variant = ActionButtonVariant::Danger;
        self
    }

    pub fn danger_solid(mut self) -> Self {
        self.variant = ActionButtonVariant::DangerSolid;
        self
    }

    pub fn ghost(mut self) -> Self {
        self.variant = ActionButtonVariant::Ghost;
        self
    }

    pub fn size(mut self, size: ActionButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn compact(mut self) -> Self {
        self.size = ActionButtonSize::Compact;
        self
    }

    pub fn dense_nav(mut self) -> Self {
        self.size = ActionButtonSize::DenseNav;
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn palette(mut self, palette: NeonPalette) -> Self {
        self.palette = Some(palette);
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ActionButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = self
            .palette
            .unwrap_or_else(|| NeonTheme::current_palette(cx));
        let is_icon_only = matches!(
            self.size,
            ActionButtonSize::Compact | ActionButtonSize::DenseNav
        );

        let (bg, fg, border_color, hover_bg, hover_border) = match self.variant {
            ActionButtonVariant::Default => {
                if self.active {
                    (p.selected, p.accent, p.accent, p.selected, p.accent)
                } else {
                    (
                        p.surface,
                        p.text,
                        p.control_border,
                        p.selected,
                        p.border_strong,
                    )
                }
            }
            ActionButtonVariant::Primary => (
                p.accent,
                p.accent_text,
                p.accent,
                p.accent_hover,
                p.accent_hover,
            ),
            ActionButtonVariant::Danger => (
                p.surface,
                p.danger_text,
                p.danger_border,
                p.danger_bg,
                p.danger_text,
            ),
            ActionButtonVariant::DangerSolid => (
                p.danger_button,
                p.danger_button_text,
                p.danger_button,
                p.danger_button,
                p.danger_button,
            ),
            ActionButtonVariant::Ghost => {
                if self.active {
                    (p.selected, p.accent, p.border, p.selected, p.border_strong)
                } else {
                    (
                        gpui_kit::hsla(0.0, 0.0, 0.0, 0.0),
                        p.muted,
                        gpui_kit::hsla(0.0, 0.0, 0.0, 0.0),
                        p.surface_alt,
                        p.border,
                    )
                }
            }
        };

        let effective_fg = if self.disabled {
            if is_icon_only {
                // Keep disabled compact icons legible rather than washed-out opacity.
                p.compact_disabled
            } else {
                p.muted
            }
        } else {
            fg
        };

        let effective_bg = if self.disabled && !matches!(self.variant, ActionButtonVariant::Ghost) {
            p.surface_alt
        } else {
            bg
        };

        let effective_border = if self.disabled {
            p.border
        } else {
            border_color
        };

        let icon_px = match self.size {
            ActionButtonSize::Regular => 16.0,
            ActionButtonSize::Compact => DEFAULT_ACTION_ICON_SIZE,
            ActionButtonSize::DenseNav => 16.0,
        };

        let tooltip_text = compose_action_tooltip(
            self.label.as_deref(),
            self.tooltip.as_deref(),
            self.shortcut.as_deref(),
        );

        let keycap_text = if !is_icon_only {
            self.shortcut
                .as_deref()
                .map(format_shortcut_keycap)
                .filter(|k| !k.is_empty())
        } else {
            None
        };

        let mut el = div()
            .id(self.id)
            .test_support()
            .h_flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(effective_border)
            .bg(effective_bg)
            .text_color(effective_fg)
            .text_control()
            .font_weight(FontWeight::MEDIUM)
            .flex_shrink_0();

        el = match self.size {
            ActionButtonSize::Regular => el.min_h(px(32.0)).px(px(10.0)).py(px(5.0)),
            ActionButtonSize::Compact => el
                .w(px(COMPACT_ICON_BUTTON_SIZE))
                .h(px(COMPACT_ICON_BUTTON_SIZE)),
            ActionButtonSize::DenseNav => el
                .w(px(DENSE_BOARD_NAV_BUTTON_SIZE))
                .h(px(DENSE_BOARD_NAV_BUTTON_SIZE)),
        };

        if self.disabled {
            el = el.cursor(CursorStyle::Arrow);
            if !is_icon_only {
                el = el.opacity(0.62);
            }
        } else {
            el = el
                .when(self.tab_stop, |el| {
                    el.tab_index(0)
                        .focus_visible(move |style| style.border_2().border_color(p.accent))
                })
                .cursor_pointer()
                .hover(move |style| style.bg(hover_bg).border_color(hover_border));
            if let Some(handler) = self.on_click {
                el = el.on_click(move |event, window, cx| handler(event, window, cx));
            }
        }

        if let Some(icon_name) = self.icon {
            el = el.child(action_icon(icon_name, icon_px, effective_fg));
        }

        if !is_icon_only {
            if let Some(label) = self.label {
                el = el.child(label);
            }
            if let Some(keycap) = keycap_text {
                let (kbd_bg, kbd_border, kbd_fg) =
                    if matches!(self.variant, ActionButtonVariant::Primary) && !self.disabled {
                        (
                            gpui_kit::hsla(0.0, 0.0, 1.0, 0.16),
                            gpui_kit::hsla(0.0, 0.0, 1.0, 0.30),
                            p.accent_text,
                        )
                    } else {
                        (p.surface_alt, p.border, p.muted)
                    };
                el = el.child(
                    div()
                        .font_family(FONT_MONO_FAMILY)
                        .text_size(px(FONT_SIZE_META))
                        .line_height(px(14.0))
                        .px(px(5.0))
                        .py(px(1.0))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(kbd_border)
                        .bg(kbd_bg)
                        .text_color(kbd_fg)
                        .child(keycap),
                );
            }
        }

        el.when_some(tooltip_text, |this, text| {
            this.tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_shortcut_keycaps_consistently() {
        assert_eq!(format_shortcut_keycap("Enter"), "↵");
        assert_eq!(format_shortcut_keycap("Escape"), "Esc");
        assert_eq!(format_shortcut_keycap("Shift+E"), "⇧E");
        assert_eq!(format_shortcut_keycap("Cmd+,"), "⌘,");
        assert_eq!(format_shortcut_keycap("ArrowLeft"), "←");
    }

    #[test]
    fn appends_shortcut_suffix_without_duplication() {
        assert_eq!(
            compose_action_tooltip(Some("Settings"), None, Some("Cmd+,")).as_deref(),
            Some("Settings (⌘,)")
        );
        assert_eq!(
            compose_action_tooltip(Some("Next Error"), Some("Next Error (E)"), Some("E"))
                .as_deref(),
            Some("Next Error (E)")
        );
        assert_eq!(
            compose_action_tooltip(Some("Close"), None, None).as_deref(),
            Some("Close")
        );
    }
}
