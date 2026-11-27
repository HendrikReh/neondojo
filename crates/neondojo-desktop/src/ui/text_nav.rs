//! Zero-pill underlined text navigation bar (`docs/ui-navigation.md`).
//!
//! Used for the top workspace header (`20px` gap), inline workspace sub-tabs
//! (`16px` gap), stacked vertical lists (`8px` gap), and compact pagination bars.
//! Active items show a crisp `2px` accent bar with transparent background —
//! never pill fills or segmented-control chrome.

use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use gpui_kit::{
    App, CursorStyle, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, TestSupportExt, Window,
    base::StyledExt, component::tooltip::Tooltip, div, hsla, prelude::FluentBuilder, px,
};
use std::rc::Rc;

type SelectHandler = Rc<dyn Fn(&str, &mut Window, &mut App) + 'static>;
type DirtyGuard = Rc<dyn Fn(&str) -> bool + 'static>;

/// Layout axis for [`TextNavBar`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TextNavOrientation {
    #[default]
    Horizontal,
    Vertical,
}

/// Spacing & typographic density tiers from `docs/ui-navigation.md`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TextNavDensity {
    /// Top app header (`20px` gap, `13px` control text).
    #[default]
    Header,
    /// Workspace inline sub-navigation (`16px` gap, `13px` control text).
    Inline,
    /// Compact navigation (`12px` horizontal / `8px` vertical, `12px` label text).
    Compact,
}

/// Roving keyboard step inside a [`TextNavBar`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RovingStep {
    Prev,
    Next,
    First,
    Last,
}

/// Computes the next enabled index under roving arrow/Home/End navigation.
pub fn next_roving_index(
    disabled_flags: &[bool],
    current: usize,
    step: RovingStep,
) -> Option<usize> {
    let len = disabled_flags.len();
    if len == 0 {
        return None;
    }
    match step {
        RovingStep::First => disabled_flags.iter().position(|&d| !d),
        RovingStep::Last => disabled_flags.iter().rposition(|&d| !d),
        RovingStep::Next => {
            let start = current.min(len - 1);
            for offset in 1..=len {
                let candidate = (start + offset) % len;
                if !disabled_flags[candidate] {
                    return Some(candidate);
                }
            }
            None
        }
        RovingStep::Prev => {
            let start = current.min(len - 1);
            for offset in 1..=len {
                let candidate = (start + len - offset) % len;
                if !disabled_flags[candidate] {
                    return Some(candidate);
                }
            }
            None
        }
    }
}

/// Single entry inside a [`TextNavBar`].
#[derive(Debug, Clone)]
pub struct TextNavItem {
    pub id: SharedString,
    pub label: SharedString,
    pub element_id: Option<SharedString>,
    pub badge: Option<SharedString>,
    pub tooltip: Option<SharedString>,
    pub disabled: bool,
}

impl TextNavItem {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            element_id: None,
            badge: None,
            tooltip: None,
            disabled: false,
        }
    }

    pub fn element_id(mut self, element_id: impl Into<SharedString>) -> Self {
        self.element_id = Some(element_id.into());
        self
    }

    pub fn badge(mut self, badge: impl Into<SharedString>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// Transparent, underlined text navigation bar supporting horizontal tabs,
/// stacked vertical navigation lists, badges, and dirty-draft switch guards.
#[derive(IntoElement)]
pub struct TextNavBar {
    id: ElementId,
    items: Vec<TextNavItem>,
    active_id: Option<SharedString>,
    orientation: TextNavOrientation,
    density: TextNavDensity,
    palette: Option<NeonPalette>,
    dirty_guard: Option<DirtyGuard>,
    on_select: Option<SelectHandler>,
}

impl TextNavBar {
    pub fn new(id: impl Into<ElementId>, items: Vec<TextNavItem>) -> Self {
        Self {
            id: id.into(),
            items,
            active_id: None,
            orientation: TextNavOrientation::Horizontal,
            density: TextNavDensity::Header,
            palette: None,
            dirty_guard: None,
            on_select: None,
        }
    }

    pub fn active(mut self, active_id: Option<impl Into<SharedString>>) -> Self {
        self.active_id = active_id.map(Into::into);
        self
    }

    pub fn orientation(mut self, orientation: TextNavOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    pub fn density(mut self, density: TextNavDensity) -> Self {
        self.density = density;
        self
    }

    pub fn palette(mut self, palette: NeonPalette) -> Self {
        self.palette = Some(palette);
        self
    }

    /// Optional guard returning `true` if switching to the target item ID is
    /// permitted (e.g. `false` when a dirty draft refuses navigation).
    pub fn dirty_guard(mut self, guard: impl Fn(&str) -> bool + 'static) -> Self {
        self.dirty_guard = Some(Rc::new(guard));
        self
    }

    pub fn on_select(mut self, handler: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for TextNavBar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = self
            .palette
            .unwrap_or_else(|| NeonTheme::current_palette(cx));
        let gap_px = match (self.orientation, self.density) {
            (TextNavOrientation::Vertical, _) => 8.0,
            (TextNavOrientation::Horizontal, TextNavDensity::Header) => 20.0,
            (TextNavOrientation::Horizontal, TextNavDensity::Inline) => 16.0,
            (TextNavOrientation::Horizontal, TextNavDensity::Compact) => 12.0,
        };

        let transparent = hsla(0.0, 0.0, 0.0, 0.0);

        let mut container = div().id(self.id).test_support().gap(px(gap_px));
        container = match self.orientation {
            TextNavOrientation::Horizontal => container.h_flex().items_center(),
            TextNavOrientation::Vertical => container.v_flex().items_stretch(),
        };

        for item in self.items {
            let is_active = self
                .active_id
                .as_ref()
                .is_some_and(|active| active == &item.id);
            let item_element_id = item
                .element_id
                .clone()
                .unwrap_or_else(|| SharedString::from(format!("text-nav-{}", item.id)));

            let indicator_color = if is_active { p.accent } else { transparent };
            let text_color = if item.disabled {
                p.compact_disabled
            } else if is_active {
                p.text
            } else {
                p.muted
            };

            let mut tab = div()
                .id(item_element_id)
                .test_support()
                .h_flex()
                .items_center()
                .gap(px(6.0))
                .bg(transparent)
                .text_color(text_color)
                .flex_shrink_0();

            tab = match self.density {
                TextNavDensity::Compact => tab.text_label(),
                TextNavDensity::Header | TextNavDensity::Inline => tab.text_control(),
            };

            if is_active {
                tab = tab.font_weight(FontWeight::SEMIBOLD);
            } else {
                tab = tab.font_weight(FontWeight::MEDIUM);
            }

            tab = match self.orientation {
                TextNavOrientation::Horizontal => {
                    tab.py(px(6.0)).border_b_2().border_color(indicator_color)
                }
                TextNavOrientation::Vertical => tab
                    .px(px(8.0))
                    .py(px(4.0))
                    .border_l_2()
                    .border_color(indicator_color),
            };

            if item.disabled {
                tab = tab.cursor(CursorStyle::Arrow);
            } else {
                let hover_text = p.text;
                let hover_border = if is_active { p.accent } else { p.border_strong };
                tab = tab
                    .cursor_pointer()
                    .hover(move |style| style.text_color(hover_text).border_color(hover_border));

                if let Some(on_select) = self.on_select.clone() {
                    let target_id = item.id.clone();
                    let guard = self.dirty_guard.clone();
                    tab = tab.on_click(move |_, window, cx| {
                        if let Some(guard_fn) = &guard
                            && !guard_fn(target_id.as_ref())
                        {
                            return;
                        }
                        on_select(target_id.as_ref(), window, cx);
                    });
                }
            }

            tab = tab.child(item.label);

            if let Some(badge) = item.badge {
                tab = tab.child(
                    div()
                        .text_meta()
                        .tabular_nums()
                        .px(px(5.0))
                        .py(px(1.0))
                        .rounded(px(4.0))
                        .bg(p.surface_alt)
                        .text_color(if is_active { p.accent } else { p.muted })
                        .child(badge),
                );
            }

            tab = tab.when_some(item.tooltip, |this, tip| {
                this.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            });

            container = container.child(tab);
        }

        container
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roving_index_wraps_and_skips_disabled_entries() {
        let flags = [false, true, false, true, false];
        assert_eq!(next_roving_index(&flags, 0, RovingStep::Next), Some(2));
        assert_eq!(next_roving_index(&flags, 2, RovingStep::Next), Some(4));
        assert_eq!(next_roving_index(&flags, 4, RovingStep::Next), Some(0));
        assert_eq!(next_roving_index(&flags, 0, RovingStep::Prev), Some(4));
        assert_eq!(next_roving_index(&flags, 4, RovingStep::Prev), Some(2));
        assert_eq!(next_roving_index(&flags, 2, RovingStep::First), Some(0));
        assert_eq!(next_roving_index(&flags, 2, RovingStep::Last), Some(4));
    }

    #[test]
    fn roving_index_handles_empty_or_all_disabled() {
        assert_eq!(next_roving_index(&[], 0, RovingStep::Next), None);
        assert_eq!(next_roving_index(&[true, true], 0, RovingStep::Next), None);
        assert_eq!(next_roving_index(&[true, true], 0, RovingStep::First), None);
    }
}
