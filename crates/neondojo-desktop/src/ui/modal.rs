//! NeonDojo modal and library dialog wrappers built on `gpui_kit::component::WindowExt`.
//!
//! Enforces NeonDojo's core modal invariants:
//! - Automatic focus save & restore on close (provided by `gpui-kit` `WindowState`)
//! - Stacked dialogs (confirmation over parent modal)
//! - Dynamic `ModalBusyLock`: while busy, Escape, backdrop press, and Close button
//!   are all blocked until the operation finishes
//! - Standardized `LibraryDialog` header (20px semibold title + compact 44×44 Close button)

use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::icons::ActionIconName;
use gpui_kit::{
    AnyElement, App, ClickEvent, ParentElement, SharedString, Styled, Window,
    base::StyledExt,
    component::{WindowExt, dialog::DialogEntrance},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo_i18n::I18n;
use std::cell::Cell;
use std::rc::Rc;

/// Callback invoked when a modal dialog closes.
pub type ModalCloseCallback = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Shared handle locking modal dismissal while an async mutation (import,
/// backup restore, delete, analysis start) is in flight.
#[derive(Debug, Clone, Default)]
pub struct ModalBusyLock(Rc<Cell<bool>>);

impl ModalBusyLock {
    pub fn new(busy: bool) -> Self {
        Self(Rc::new(Cell::new(busy)))
    }

    pub fn is_busy(&self) -> bool {
        self.0.get()
    }

    pub fn set_busy(&self, busy: bool) {
        self.0.set(busy);
    }
}

/// Standard modal width presets matching NeonDojo's CSS modal containers.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum ModalSize {
    Compact,
    #[default]
    Standard,
    Wide,
    ExtraWide,
    Custom(f32),
}

impl ModalSize {
    pub const fn width_px(self) -> f32 {
        match self {
            Self::Compact => 420.0,
            Self::Standard => 540.0,
            Self::Wide => 740.0,
            Self::ExtraWide => 940.0,
            Self::Custom(w) => w,
        }
    }
}

/// Configuration for [`open_modal_dialog`] and [`open_library_dialog`].
#[derive(Clone)]
pub struct ModalConfig {
    pub title: SharedString,
    pub subtitle: Option<SharedString>,
    pub size: ModalSize,
    pub centered: bool,
    pub show_close_button: bool,
    pub overlay_closable: bool,
    pub keyboard_closable: bool,
    pub busy: ModalBusyLock,
    pub palette: Option<NeonPalette>,
}

impl ModalConfig {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            size: ModalSize::Standard,
            centered: true,
            show_close_button: true,
            overlay_closable: true,
            keyboard_closable: true,
            busy: ModalBusyLock::new(false),
            palette: None,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn size(mut self, size: ModalSize) -> Self {
        self.size = size;
        self
    }

    pub fn centered(mut self, centered: bool) -> Self {
        self.centered = centered;
        self
    }

    pub fn show_close_button(mut self, show: bool) -> Self {
        self.show_close_button = show;
        self
    }

    pub fn overlay_closable(mut self, closable: bool) -> Self {
        self.overlay_closable = closable;
        self
    }

    pub fn keyboard_closable(mut self, closable: bool) -> Self {
        self.keyboard_closable = closable;
        self
    }

    pub fn busy_lock(mut self, busy: ModalBusyLock) -> Self {
        self.busy = busy;
        self
    }

    pub fn palette(mut self, palette: NeonPalette) -> Self {
        self.palette = Some(palette);
        self
    }
}

/// Opens a NeonDojo modal dialog backed by `gpui-kit`'s stacked dialog manager.
///
/// The `body` and optional `footer` closures follow `WindowExt::open_dialog`'s
/// render-closure contract: create persistent `Entity` state before calling this
/// function and capture handles inside the closures.
pub fn open_modal_dialog<B, F>(
    window: &mut Window,
    cx: &mut App,
    config: ModalConfig,
    body: B,
    footer: Option<F>,
    on_close: Option<ModalCloseCallback>,
) where
    B: Fn(&mut Window, &mut App) -> AnyElement + 'static,
    F: Fn(&mut Window, &mut App) -> AnyElement + 'static,
{
    let body = Rc::new(body);
    let footer = footer.map(Rc::new);

    window.open_dialog(cx, move |mut dialog, window, cx| {
        let palette = config
            .palette
            .unwrap_or_else(|| NeonTheme::current_palette(cx));
        let is_busy = config.busy.is_busy();
        let busy_cancel = config.busy.clone();
        let busy_close_btn = config.busy.clone();

        let header = div()
            .h_flex()
            .items_start()
            .justify_between()
            .gap(px(12.0))
            .pb(px(12.0))
            .border_b_1()
            .border_color(palette.border)
            .child(
                div()
                    .v_flex()
                    .gap(px(4.0))
                    .flex_1()
                    .child(
                        div()
                            .text_dialog()
                            .text_color(palette.text)
                            .child(config.title.clone()),
                    )
                    .when_some(config.subtitle.clone(), |this, subtitle| {
                        this.child(div().text_meta().text_color(palette.muted).child(subtitle))
                    }),
            )
            .when(config.show_close_button, |this| {
                let close_tip = I18n::t(current_locale(cx), "settings", "done");
                this.child(
                    ActionButton::new("modal-close-button")
                        .icon(ActionIconName::Close)
                        .tooltip(close_tip)
                        .shortcut("Escape")
                        .variant(ActionButtonVariant::Ghost)
                        .compact()
                        .palette(palette)
                        .disabled(is_busy)
                        .on_click(move |_, window, cx| {
                            if !busy_close_btn.is_busy() {
                                window.close_dialog(cx);
                            }
                        }),
                )
            });

        let body_el = body(window, cx);
        let footer_el = footer.as_ref().map(|f| f(window, cx));

        dialog = dialog
            .w(px(config.size.width_px()))
            .p(px(20.0))
            .bg(palette.surface)
            .border_1()
            .border_color(palette.border)
            .rounded(px(10.0))
            .close_button(false)
            .entrance(DialogEntrance::None)
            .keyboard(config.keyboard_closable)
            .overlay_closable(config.overlay_closable)
            .on_cancel(move |_, _, _| !busy_cancel.is_busy())
            .child(
                div()
                    .v_flex()
                    .gap(px(16.0))
                    .child(header)
                    .child(div().v_flex().gap(px(12.0)).child(body_el)),
            );

        if config.centered {
            dialog = dialog.centered();
        }

        if let Some(footer_content) = footer_el {
            dialog = dialog.footer(
                div()
                    .h_flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .pt(px(12.0))
                    .border_t_1()
                    .border_color(palette.border)
                    .child(footer_content),
            );
        }

        if let Some(close_cb) = on_close.clone() {
            dialog = dialog.on_close(move |event, window, cx| close_cb(event, window, cx));
        }

        dialog
    });
}

/// Convenience wrapper matching `LibraryDialog.tsx` (standard width, centered,
/// NeonDojo header + footer strip).
pub fn open_library_dialog<B, F>(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    busy: ModalBusyLock,
    body: B,
    footer: Option<F>,
) where
    B: Fn(&mut Window, &mut App) -> AnyElement + 'static,
    F: Fn(&mut Window, &mut App) -> AnyElement + 'static,
{
    let config = ModalConfig::new(title).busy_lock(busy);
    open_modal_dialog(window, cx, config, body, footer, None);
}
