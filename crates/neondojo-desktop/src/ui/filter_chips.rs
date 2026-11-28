//! Active filter chip strip (`AppliedFilterChips.tsx`).
//!
//! Renders removable chips with `28px` minimum height, `300px` maximum width
//! with text truncation, and a `12px` close icon.

use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::icons::{ActionIconName, action_icon};
use gpui_kit::{
    App, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, TestSupportExt, Window, base::StyledExt, div,
    prelude::FluentBuilder, px,
};
use std::rc::Rc;

type RemoveChipHandler = Rc<dyn Fn(&str, &mut Window, &mut App) + 'static>;
type ClearAllHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

/// Single applied filter pill shown above a filtered table or workspace.
#[derive(Debug, Clone)]
pub struct AppliedFilterChip {
    pub id: SharedString,
    pub label: SharedString,
    pub value: Option<SharedString>,
    pub removable: bool,
}

impl AppliedFilterChip {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            value: None,
            removable: true,
        }
    }

    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn removable(mut self, removable: bool) -> Self {
        self.removable = removable;
        self
    }
}

/// Wrapping strip of removable filter chips with optional "Clear all" trigger.
#[derive(IntoElement)]
pub struct AppliedFilterChips {
    id: ElementId,
    chips: Vec<AppliedFilterChip>,
    clear_all_label: Option<SharedString>,
    palette: Option<NeonPalette>,
    on_remove: Option<RemoveChipHandler>,
    on_clear_all: Option<ClearAllHandler>,
}

impl AppliedFilterChips {
    pub fn new(id: impl Into<ElementId>, chips: Vec<AppliedFilterChip>) -> Self {
        Self {
            id: id.into(),
            chips,
            clear_all_label: Some("Clear all".into()),
            palette: None,
            on_remove: None,
            on_clear_all: None,
        }
    }

    pub fn clear_all_label(mut self, label: impl Into<SharedString>) -> Self {
        self.clear_all_label = Some(label.into());
        self
    }

    pub fn palette(mut self, palette: NeonPalette) -> Self {
        self.palette = Some(palette);
        self
    }

    pub fn on_remove(mut self, handler: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_remove = Some(Rc::new(handler));
        self
    }

    pub fn on_clear_all(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_clear_all = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for AppliedFilterChips {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.chips.is_empty() {
            return div().id(self.id).test_support();
        }

        let p = self
            .palette
            .unwrap_or_else(|| NeonTheme::current_palette(cx));
        let removable_count = self.chips.iter().filter(|c| c.removable).count();

        let mut strip = div()
            .id(self.id)
            .test_support()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(6.0));

        for chip in self.chips {
            let text_content = match chip.value {
                Some(val) if !val.is_empty() => {
                    SharedString::from(format!("{}: {}", chip.label, val))
                }
                _ => chip.label,
            };
            let chip_id = chip.id.clone();

            let mut pill = div()
                .h_flex()
                .items_center()
                .gap(px(6.0))
                .min_h(px(28.0))
                .max_w(px(300.0))
                .px(px(8.0))
                .py(px(3.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(p.border)
                .bg(p.surface_alt)
                .text_label()
                .text_color(p.text)
                .child(div().flex_1().min_w(px(0.0)).truncate().child(text_content));

            if chip.removable {
                let remove_handler = self.on_remove.clone();
                let hover_bg = p.selected;
                pill = pill.child(
                    div()
                        .id(SharedString::from(format!("filter-chip-remove-{chip_id}")))
                        .test_support()
                        .h_flex()
                        .items_center()
                        .justify_center()
                        .w(px(18.0))
                        .h(px(18.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(move |s| s.bg(hover_bg))
                        .child(action_icon(ActionIconName::Close, 12.0, p.muted))
                        .when_some(remove_handler, |this, cb| {
                            this.on_click(move |_, window, cx| cb(chip_id.as_ref(), window, cx))
                        }),
                );
            }

            strip = strip.child(pill);
        }

        if removable_count > 1
            && let (Some(label), Some(on_clear)) = (self.clear_all_label, self.on_clear_all)
        {
            let hover_color = p.text;
            strip = strip.child(
                div()
                    .id("filter-chips-clear-all")
                    .test_support()
                    .px(px(6.0))
                    .py(px(4.0))
                    .text_label()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.muted)
                    .cursor_pointer()
                    .hover(move |s| s.text_color(hover_color))
                    .on_click(move |_, window, cx| on_clear(window, cx))
                    .child(label),
            );
        }

        strip
    }
}
