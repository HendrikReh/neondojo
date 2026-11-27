//! Shared board preview controls (`Initial` / `Arrows` / `Final`) and
//! `BoardReview` keyboard action helper (`useBoardReviewShortcuts.ts`).

use crate::i18n::current_locale;
use crate::keybindings::CTX_BOARD_REVIEW;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::icons::ActionIconName;
use crate::ui::text_nav::{TextNavBar, TextNavDensity, TextNavItem};
use gpui_kit::{
    App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, Styled, TestSupportExt, Window, base::StyledExt, div, prelude::FluentBuilder, px,
};
use neondojo_board::BoardViewMode;
use neondojo_i18n::{I18n, Locale, Param};
use std::rc::Rc;

type ModeSelectHandler = Rc<dyn Fn(BoardViewMode, &mut Window, &mut App) + 'static>;
type ResetPreviewHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Canonical identifier string for a [`BoardViewMode`] inside `TextNavBar`.
pub const fn view_mode_id(mode: BoardViewMode) -> &'static str {
    match mode {
        BoardViewMode::Before => "before",
        BoardViewMode::Arrows => "arrows",
        BoardViewMode::After => "after",
    }
}

/// Parses a [`BoardViewMode`] from its canonical ID.
pub fn parse_view_mode(id: &str) -> Option<BoardViewMode> {
    match id {
        "before" => Some(BoardViewMode::Before),
        "arrows" => Some(BoardViewMode::Arrows),
        "after" => Some(BoardViewMode::After),
        _ => None,
    }
}

/// Localized label for a [`BoardViewMode`] (`Initial` / `Arrows` / `Final`).
pub fn view_mode_label(locale: Locale, mode: BoardViewMode) -> &'static str {
    I18n::t(locale, "board-review", view_mode_id(mode))
}

/// Maps `BoardReview` keyboard arrows (`Left` → `Before`, `Right` → `After`)
/// matching `useBoardReviewShortcuts.ts`.
pub const fn step_preview_mode(go_final: bool) -> BoardViewMode {
    if go_final {
        BoardViewMode::After
    } else {
        BoardViewMode::Before
    }
}

/// Compact board preview strip (`Initial` / `Arrows` / `Final` + optional Reset
/// preview button + active move status caption).
#[derive(IntoElement)]
pub struct PreviewBoardControls {
    id: ElementId,
    mode: BoardViewMode,
    preview_player: Option<SharedString>,
    preview_move: Option<SharedString>,
    can_reset: bool,
    palette: Option<NeonPalette>,
    on_mode: Option<ModeSelectHandler>,
    on_reset: Option<ResetPreviewHandler>,
}

impl PreviewBoardControls {
    pub fn new(id: impl Into<ElementId>, mode: BoardViewMode) -> Self {
        Self {
            id: id.into(),
            mode,
            preview_player: None,
            preview_move: None,
            can_reset: false,
            palette: None,
            on_mode: None,
            on_reset: None,
        }
    }

    pub fn preview_summary(
        mut self,
        player: impl Into<SharedString>,
        move_label: impl Into<SharedString>,
    ) -> Self {
        self.preview_player = Some(player.into());
        self.preview_move = Some(move_label.into());
        self
    }

    pub fn can_reset(mut self, can_reset: bool) -> Self {
        self.can_reset = can_reset;
        self
    }

    pub fn palette(mut self, palette: NeonPalette) -> Self {
        self.palette = Some(palette);
        self
    }

    pub fn on_mode(
        mut self,
        handler: impl Fn(BoardViewMode, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_mode = Some(Rc::new(handler));
        self
    }

    pub fn on_reset(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_reset = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for PreviewBoardControls {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = self
            .palette
            .unwrap_or_else(|| NeonTheme::current_palette(cx));

        let items = vec![
            TextNavItem::new("before", view_mode_label(locale, BoardViewMode::Before))
                .element_id("preview-mode-before")
                .tooltip(format!(
                    "{} (←)",
                    view_mode_label(locale, BoardViewMode::Before)
                )),
            TextNavItem::new("arrows", view_mode_label(locale, BoardViewMode::Arrows))
                .element_id("preview-mode-arrows"),
            TextNavItem::new("after", view_mode_label(locale, BoardViewMode::After))
                .element_id("preview-mode-after")
                .tooltip(format!(
                    "{} (→)",
                    view_mode_label(locale, BoardViewMode::After)
                )),
        ];

        let mut nav = TextNavBar::new("preview-mode-nav", items)
            .active(Some(view_mode_id(self.mode)))
            .density(TextNavDensity::Compact)
            .palette(p);

        if let Some(on_mode) = self.on_mode {
            nav = nav.on_select(move |id, window, cx| {
                if let Some(next) = parse_view_mode(id) {
                    on_mode(next, window, cx);
                }
            });
        }

        let summary = match (&self.preview_player, &self.preview_move) {
            (Some(player), Some(mv)) => Some(I18n::format(
                locale,
                "board-review",
                "showing",
                &[
                    ("view", Param::from(view_mode_label(locale, self.mode))),
                    ("player", Param::from(player.as_ref())),
                    ("move", Param::from(mv.as_ref())),
                ],
            )),
            _ => None,
        };

        div()
            .id(self.id)
            .test_support()
            .key_context(CTX_BOARD_REVIEW)
            .h_flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .w_full()
            .child(div().h_flex().items_center().gap(px(10.0)).child(nav).when(
                self.can_reset,
                |this| {
                    let mut btn = ActionButton::new("preview-mode-reset")
                        .icon(ActionIconName::Reset)
                        .tooltip(I18n::t(locale, "board-review", "reset"))
                        .variant(ActionButtonVariant::Ghost)
                        .dense_nav()
                        .palette(p);
                    if let Some(cb) = self.on_reset {
                        btn = btn.on_click(move |event, window, cx| cb(event, window, cx));
                    }
                    this.child(btn)
                },
            ))
            .when_some(summary, |this, text| {
                this.child(div().text_meta().text_color(p.muted).truncate().child(text))
            })
    }
}
