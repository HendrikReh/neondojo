//! Status banners, empty/error states, and engine status dot indicators.

use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::icons::{ActionIconName, action_icon};
use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, SharedString, Styled, TestSupportExt, Window, base::StyledExt, div,
    prelude::FluentBuilder, px,
};
use std::rc::Rc;

type BannerActionHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Semantic tone for [`StatusBanner`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum BannerKind {
    Error,
    Success,
    Warning,
    #[default]
    Info,
}

/// Inline status banner supporting optional Retry and Dismiss actions.
#[derive(IntoElement)]
pub struct StatusBanner {
    id: ElementId,
    kind: BannerKind,
    title: Option<SharedString>,
    message: SharedString,
    retry_label: Option<SharedString>,
    palette: Option<NeonPalette>,
    on_retry: Option<BannerActionHandler>,
    on_dismiss: Option<BannerActionHandler>,
}

impl StatusBanner {
    pub fn new(
        id: impl Into<ElementId>,
        kind: BannerKind,
        message: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            title: None,
            message: message.into(),
            retry_label: None,
            palette: None,
            on_retry: None,
            on_dismiss: None,
        }
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn palette(mut self, palette: NeonPalette) -> Self {
        self.palette = Some(palette);
        self
    }

    pub fn on_retry(
        mut self,
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.retry_label = Some(label.into());
        self.on_retry = Some(Rc::new(handler));
        self
    }

    pub fn on_dismiss(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for StatusBanner {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = self
            .palette
            .unwrap_or_else(|| NeonTheme::current_palette(cx));

        let (bg, fg, border, icon) = match self.kind {
            BannerKind::Error => (
                p.danger_bg,
                p.danger_text,
                p.danger_border,
                ActionIconName::Warning,
            ),
            BannerKind::Success => (
                p.success_bg,
                p.success_text,
                p.border,
                ActionIconName::Confirm,
            ),
            BannerKind::Warning => (
                p.warning_bg,
                p.warning_text,
                p.border,
                ActionIconName::Warning,
            ),
            BannerKind::Info => (p.selected, p.text, p.border, ActionIconName::Help),
        };

        div()
            .id(self.id)
            .test_support()
            .h_flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_color(fg)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_1()
                    .min_w(px(0.0))
                    .child(action_icon(icon, 16.0, fg))
                    .child(
                        div()
                            .v_flex()
                            .gap(px(2.0))
                            .flex_1()
                            .min_w(px(0.0))
                            .when_some(self.title, |this, title| {
                                this.child(
                                    div()
                                        .text_label()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(title),
                                )
                            })
                            .child(div().text_body().child(self.message)),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(6.0))
                    .flex_shrink_0()
                    .when_some(self.on_retry, |this, handler| {
                        let label = self.retry_label.unwrap_or_else(|| "Retry".into());
                        this.child(
                            ActionButton::new("banner-retry")
                                .label(label)
                                .icon(ActionIconName::Refresh)
                                .palette(p)
                                .on_click(move |event, window, cx| handler(event, window, cx)),
                        )
                    })
                    .when_some(self.on_dismiss, |this, handler| {
                        this.child(
                            ActionButton::new("banner-dismiss")
                                .icon(ActionIconName::Close)
                                .tooltip("Dismiss")
                                .variant(ActionButtonVariant::Ghost)
                                .dense_nav()
                                .palette(p)
                                .on_click(move |event, window, cx| handler(event, window, cx)),
                        )
                    }),
            )
    }
}

/// Centered empty or placeholder panel with optional icon and action slot.
#[derive(IntoElement)]
pub struct EmptyState {
    id: ElementId,
    icon: Option<ActionIconName>,
    title: SharedString,
    description: SharedString,
    action: Option<AnyElement>,
    palette: Option<NeonPalette>,
}

impl EmptyState {
    pub fn new(
        id: impl Into<ElementId>,
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            icon: None,
            title: title.into(),
            description: description.into(),
            action: None,
            palette: None,
        }
    }

    pub fn icon(mut self, icon: ActionIconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    pub fn palette(mut self, palette: NeonPalette) -> Self {
        self.palette = Some(palette);
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = self
            .palette
            .unwrap_or_else(|| NeonTheme::current_palette(cx));

        div()
            .id(self.id)
            .test_support()
            .v_flex()
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .p(px(32.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .when_some(self.icon, |this, icon| {
                this.child(
                    div()
                        .h_flex()
                        .items_center()
                        .justify_center()
                        .w(px(44.0))
                        .h(px(44.0))
                        .rounded(px(10.0))
                        .bg(p.surface_alt)
                        .child(action_icon(icon, 20.0, p.muted)),
                )
            })
            .child(div().text_subject().text_color(p.text).child(self.title))
            .child(
                div()
                    .text_body()
                    .text_color(p.muted)
                    .max_w(px(480.0))
                    .child(self.description),
            )
            .when_some(self.action, |this, action| {
                this.child(div().pt(px(6.0)).child(action))
            })
    }
}

/// Health status of a local analysis/play engine shown in the shell footer.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum EngineHealth {
    #[default]
    Ready,
    Busy,
    Warning,
    Unavailable,
}

/// Compact 6px status dot + engine status label for the footer bar.
#[derive(IntoElement)]
pub struct EngineStatusDot {
    id: ElementId,
    health: EngineHealth,
    label: SharedString,
    palette: Option<NeonPalette>,
}

impl EngineStatusDot {
    pub fn new(
        id: impl Into<ElementId>,
        health: EngineHealth,
        label: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            health,
            label: label.into(),
            palette: None,
        }
    }

    pub fn palette(mut self, palette: NeonPalette) -> Self {
        self.palette = Some(palette);
        self
    }
}

impl RenderOnce for EngineStatusDot {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = self
            .palette
            .unwrap_or_else(|| NeonTheme::current_palette(cx));
        let dot_color = match self.health {
            EngineHealth::Ready => p.success_text,
            EngineHealth::Busy => p.accent,
            EngineHealth::Warning => p.warning_text,
            EngineHealth::Unavailable => p.danger_text,
        };

        div()
            .id(self.id)
            .test_support()
            .h_flex()
            .items_center()
            .gap(px(6.0))
            .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(dot_color))
            .child(div().text_meta().text_color(p.muted).child(self.label))
    }
}
