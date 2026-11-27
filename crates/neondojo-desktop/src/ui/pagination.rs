//! Compact `TextNav`-styled numbered pagination (`NumberedSearchPagination.tsx`).
//!
//! Displays First, Previous (`‹`), up to 5 nearby page numbers with tabular
//! numerals, Next (`›`), and Last controls.

use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use gpui_kit::{
    App, CursorStyle, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, TestSupportExt, Window,
    base::StyledExt, div, hsla, px,
};
use std::rc::Rc;

type PageSelectHandler = Rc<dyn Fn(usize, &mut Window, &mut App) + 'static>;

/// Maximum number of individual page links displayed at once.
pub const MAX_VISIBLE_PAGES: usize = 5;

/// Computes up to 5 nearby 1-based page numbers around `current_page` within `1..=last_page`.
pub fn visible_page_window(current_page: usize, last_page: usize) -> Vec<usize> {
    if last_page == 0 {
        return Vec::new();
    }
    let page = current_page.clamp(1, last_page);
    if last_page <= MAX_VISIBLE_PAGES {
        return (1..=last_page).collect();
    }
    let half = MAX_VISIBLE_PAGES / 2;
    let mut start = page.saturating_sub(half).max(1);
    let mut end = start + MAX_VISIBLE_PAGES - 1;
    if end > last_page {
        end = last_page;
        start = end + 1 - MAX_VISIBLE_PAGES;
    }
    (start..=end).collect()
}

/// Compact underlined pagination bar matching `NumberedSearchPagination.tsx`.
#[derive(IntoElement)]
pub struct NumberedSearchPagination {
    id: ElementId,
    page: usize,
    last_page: usize,
    disabled: bool,
    palette: Option<NeonPalette>,
    on_select: Option<PageSelectHandler>,
}

impl NumberedSearchPagination {
    pub fn new(id: impl Into<ElementId>, page: usize, last_page: usize) -> Self {
        Self {
            id: id.into(),
            page: page.max(1),
            last_page,
            disabled: false,
            palette: None,
            on_select: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn palette(mut self, palette: NeonPalette) -> Self {
        self.palette = Some(palette);
        self
    }

    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }

    fn render_button(
        element_id: SharedString,
        label: SharedString,
        target_page: usize,
        active: bool,
        disabled: bool,
        p: NeonPalette,
        on_select: Option<PageSelectHandler>,
    ) -> impl IntoElement {
        let transparent = hsla(0.0, 0.0, 0.0, 0.0);
        let text_color = if disabled {
            p.compact_disabled
        } else if active {
            p.text
        } else {
            p.muted
        };
        let border_color = if active { p.accent } else { transparent };

        let mut btn = div()
            .id(element_id)
            .test_support()
            .h_flex()
            .items_center()
            .justify_center()
            .min_w(px(26.0))
            .px(px(6.0))
            .py(px(4.0))
            .text_label()
            .tabular_nums()
            .font_weight(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            })
            .text_color(text_color)
            .border_b_2()
            .border_color(border_color)
            .child(label);

        if disabled || active {
            btn = btn.cursor(CursorStyle::Arrow);
        } else {
            let hover_text = p.text;
            let hover_border = p.border_strong;
            btn = btn
                .cursor_pointer()
                .hover(move |s| s.text_color(hover_text).border_color(hover_border));
            if let Some(cb) = on_select {
                btn = btn.on_click(move |_, window, cx| cb(target_page, window, cx));
            }
        }
        btn
    }
}

impl RenderOnce for NumberedSearchPagination {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = self
            .palette
            .unwrap_or_else(|| NeonTheme::current_palette(cx));
        let last_page = self.last_page.max(1);
        let current = self.page.clamp(1, last_page);
        let pages = visible_page_window(current, last_page);

        let at_start = self.disabled || current <= 1;
        let at_end = self.disabled || current >= last_page;

        let mut bar = div()
            .id(self.id)
            .test_support()
            .h_flex()
            .items_center()
            .gap(px(6.0))
            .child(Self::render_button(
                "pagination-first".into(),
                "«".into(),
                1,
                false,
                at_start,
                p,
                self.on_select.clone(),
            ))
            .child(Self::render_button(
                "pagination-prev".into(),
                "‹".into(),
                current.saturating_sub(1).max(1),
                false,
                at_start,
                p,
                self.on_select.clone(),
            ));

        for page_num in pages {
            bar = bar.child(Self::render_button(
                SharedString::from(format!("pagination-page-{page_num}")),
                SharedString::from(page_num.to_string()),
                page_num,
                page_num == current,
                self.disabled,
                p,
                self.on_select.clone(),
            ));
        }

        bar.child(Self::render_button(
            "pagination-next".into(),
            "›".into(),
            (current + 1).min(last_page),
            false,
            at_end,
            p,
            self.on_select.clone(),
        ))
        .child(Self::render_button(
            "pagination-last".into(),
            "»".into(),
            last_page,
            false,
            at_end,
            p,
            self.on_select,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_window_centers_and_clamps_correctly() {
        assert_eq!(visible_page_window(1, 0), Vec::<usize>::new());
        assert_eq!(visible_page_window(1, 1), vec![1]);
        assert_eq!(visible_page_window(2, 4), vec![1, 2, 3, 4]);
        assert_eq!(visible_page_window(1, 10), vec![1, 2, 3, 4, 5]);
        assert_eq!(visible_page_window(2, 10), vec![1, 2, 3, 4, 5]);
        assert_eq!(visible_page_window(5, 10), vec![3, 4, 5, 6, 7]);
        assert_eq!(visible_page_window(9, 10), vec![6, 7, 8, 9, 10]);
        assert_eq!(visible_page_window(10, 10), vec![6, 7, 8, 9, 10]);
        assert_eq!(visible_page_window(99, 10), vec![6, 7, 8, 9, 10]);
    }
}
