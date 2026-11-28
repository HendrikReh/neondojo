//! Draggable, board-anchored `PlayBoardDialog` mirroring
//! `web/src/shared/board/PlayBoardDialog.tsx`.
//!
//! Enforces exact Live Play dialog invariants:
//! - Clamped to window bounds with 16px margin while dragging the grip header.
//! - Focus trap & `CTX_MODAL` key context when `modal == true`.
//! - Non-dismissible via Escape, backdrop click, or Close button when
//!   `dismissible == false` or `busy == true`.
//! - Supports `modal == false && focus_on_open == false` for the Play empty-state
//!   setup dialog so entering empty Play NEVER steals focus from header tabs!

use crate::i18n::current_locale;
use crate::keybindings::{CTX_MODAL, CloseModal};
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::icons::{ActionIconName, action_icon};
use gpui_kit::{
    AnyElement, App, BoxShadow, ClickEvent, CursorStyle, ElementId, FocusHandle,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, Point, SharedString, StatefulInteractiveElement, Styled, TestSupportExt,
    Window, base::StyledExt, div, hsla, point, px,
};
use neondojo_i18n::I18n;
use std::rc::Rc;

/// Minimum inset from window edges when clamping a dragged board dialog (`16px`).
pub const DIALOG_VIEWPORT_MARGIN: f32 = 16.0;

/// Clamps a dialog's center point `(cx, cy)` so its full `(width, height)` box
/// stays within `[margin, viewport - margin]` on both axes.
pub fn clamp_dialog_center(
    center: (f32, f32),
    dialog_size: (f32, f32),
    viewport_size: (f32, f32),
    margin: f32,
) -> (f32, f32) {
    let half_w = (dialog_size.0 * 0.5).max(0.0);
    let half_h = (dialog_size.1 * 0.5).max(0.0);
    let min_x = half_w + margin;
    let max_x = (viewport_size.0 - half_w - margin).max(min_x);
    let min_y = half_h + margin;
    let max_y = (viewport_size.1 - half_h - margin).max(min_y);
    (center.0.clamp(min_x, max_x), center.1.clamp(min_y, max_y))
}

/// Clamps a manual drag offset `(dx, dy)` relative to the centered anchor box so
/// the dialog cannot be dragged outside the available viewport bounds.
pub fn clamp_drag_offset(
    offset: (f32, f32),
    dialog_size: (f32, f32),
    viewport_size: (f32, f32),
    margin: f32,
) -> (f32, f32) {
    let max_dx = ((viewport_size.0 - dialog_size.0) * 0.5 - margin).max(0.0);
    let max_dy = ((viewport_size.1 - dialog_size.1) * 0.5 - margin).max(0.0);
    (
        offset.0.clamp(-max_dx, max_dx),
        offset.1.clamp(-max_dy, max_dy),
    )
}

/// Persistent drag & focus state stored per active board dialog in `PlayWorkspace`.
#[derive(Clone)]
pub struct PlayBoardDialogState {
    pub focus_handle: FocusHandle,
    pub offset: (f32, f32),
    dragging: Option<DragOrigin>,
    focused_key: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct DragOrigin {
    start_pointer: Point<Pixels>,
    start_offset: (f32, f32),
}

impl PlayBoardDialogState {
    pub fn new(focus_handle: FocusHandle) -> Self {
        Self {
            focus_handle,
            offset: (0.0, 0.0),
            dragging: None,
            focused_key: None,
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }

    /// Resets drag offset when switching to a different dialog identity.
    pub fn sync_dialog_key(
        &mut self,
        key: Option<&str>,
        modal: bool,
        focus_on_open: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let same = self.focused_key.as_deref() == key;
        if same {
            return;
        }
        self.focused_key = key.map(str::to_string);
        self.offset = (0.0, 0.0);
        self.dragging = None;
        // CRITICAL: Only focus when modal OR explicitly requested by user action.
        // Non-modal empty-state PlaySetupDialog passes `modal = false, focus_on_open = false`
        // so entering empty Play NEVER steals focus from the app header tabs!
        if key.is_some() && (modal || focus_on_open) {
            self.focus_handle.focus(window, cx);
        }
    }

    pub fn start_drag(&mut self, pointer: Point<Pixels>) {
        self.dragging = Some(DragOrigin {
            start_pointer: pointer,
            start_offset: self.offset,
        });
    }

    pub fn update_drag(
        &mut self,
        pointer: Point<Pixels>,
        dialog_size: (f32, f32),
        viewport_size: (f32, f32),
    ) -> bool {
        let Some(origin) = self.dragging else {
            return false;
        };
        let dx: f32 = (pointer.x - origin.start_pointer.x).into();
        let dy: f32 = (pointer.y - origin.start_pointer.y).into();
        let raw = (origin.start_offset.0 + dx, origin.start_offset.1 + dy);
        self.offset = clamp_drag_offset(raw, dialog_size, viewport_size, DIALOG_VIEWPORT_MARGIN);
        true
    }

    pub fn end_drag(&mut self) -> bool {
        self.dragging.take().is_some()
    }
}

/// Configuration for rendering a draggable board-anchored dialog.
pub struct PlayBoardDialogProps {
    pub id: ElementId,
    pub heading: SharedString,
    pub width_px: f32,
    pub max_height_px: f32,
    pub modal: bool,
    pub dismissible: bool,
    pub busy: bool,
    pub palette: NeonPalette,
}

type VoidCallback = Rc<dyn Fn(&mut Window, &mut App) + 'static>;
type DragStartCallback = Rc<dyn Fn(Point<Pixels>, &mut Window, &mut App) + 'static>;
type DragMoveCallback = Rc<dyn Fn(Point<Pixels>, (f32, f32), &mut Window, &mut App) + 'static>;

/// Renders a draggable, clamped board-anchored dialog overlay.
#[allow(clippy::too_many_arguments)]
pub fn render_play_board_dialog(
    props: PlayBoardDialogProps,
    state: &PlayBoardDialogState,
    body: AnyElement,
    on_close: Option<VoidCallback>,
    on_drag_start: DragStartCallback,
    on_drag_move: DragMoveCallback,
    on_drag_end: VoidCallback,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let locale = current_locale(cx);
    let p = props.palette;
    let can_dismiss = props.dismissible && !props.busy && on_close.is_some();

    let viewport = window.viewport_size();
    let vp_w: f32 = viewport.width.into();
    let vp_h: f32 = viewport.height.into();
    let dialog_w = props
        .width_px
        .min((vp_w - DIALOG_VIEWPORT_MARGIN * 2.0).max(280.0));
    let dialog_h_est = props
        .max_height_px
        .min((vp_h - DIALOG_VIEWPORT_MARGIN * 2.0).max(220.0));
    let (dx, dy) = clamp_drag_offset(
        state.offset,
        (dialog_w, dialog_h_est),
        (vp_w, vp_h),
        DIALOG_VIEWPORT_MARGIN,
    );

    let drag_tip = I18n::t(locale, "play", "dragDialog");
    let close_tip = I18n::t(locale, "play", "closeDialog");

    let on_drag_start_hdr = on_drag_start.clone();
    let mut header = div()
        .id(SharedString::from(format!("{}-drag-handle", props.id)))
        .test_support()
        .h_flex()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .pb(px(10.0))
        .border_b_1()
        .border_color(p.border)
        .child(
            div()
                .h_flex()
                .items_center()
                .gap(px(8.0))
                .flex_1()
                .min_w(px(0.0))
                .cursor(if state.is_dragging() {
                    CursorStyle::ClosedHand
                } else {
                    CursorStyle::OpenHand
                })
                .on_mouse_down(
                    MouseButton::Left,
                    move |event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        on_drag_start_hdr(event.position, window, cx);
                    },
                )
                .child(action_icon(ActionIconName::Grip, 16.0, p.muted))
                .child(
                    div()
                        .text_dialog()
                        .text_color(p.text)
                        .truncate()
                        .child(props.heading),
                )
                .child(
                    div()
                        .text_micro()
                        .text_color(p.muted)
                        .truncate()
                        .child(if state.is_dragging() { drag_tip } else { "" }),
                ),
        );

    if props.dismissible
        && let Some(close_cb) = on_close.clone()
    {
        header = header.child(
            ActionButton::new(SharedString::from(format!("{}-close", props.id)))
                .icon(ActionIconName::Close)
                .tooltip(close_tip)
                .shortcut("Escape")
                .variant(ActionButtonVariant::Ghost)
                .compact()
                .disabled(props.busy)
                .palette(p)
                .on_click(move |_: &ClickEvent, window, cx| {
                    close_cb(window, cx);
                }),
        );
    }

    let mut card = div()
        .id(props.id.clone())
        .test_support()
        .v_flex()
        .gap(px(12.0))
        .w(px(dialog_w))
        .max_h(px(dialog_h_est))
        .overflow_y_scroll()
        .p(px(18.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(p.border_strong)
        .bg(p.surface)
        .shadow(vec![BoxShadow {
            color: p.shadow,
            offset: point(px(0.0), px(10.0)),
            blur_radius: px(28.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .ml(px(dx * 2.0))
        .mt(px(dy * 2.0))
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            // Stop backdrop dismissal when clicking inside the card.
            cx.stop_propagation();
        })
        .child(header)
        .child(body);

    if props.modal {
        let close_modal = on_close.clone();
        card = card
            .track_focus(&state.focus_handle)
            .key_context(CTX_MODAL)
            .on_action(move |_: &CloseModal, window, cx| {
                // Always swallow Escape inside a modal dialog; only dismiss when allowed.
                cx.stop_propagation();
                if can_dismiss && let Some(cb) = close_modal.as_ref() {
                    cb(window, cx);
                }
            });
    } else {
        let close_non_modal = on_close.clone();
        card =
            card.track_focus(&state.focus_handle)
                .on_action(move |_: &CloseModal, window, cx| {
                    if can_dismiss && let Some(cb) = close_non_modal.as_ref() {
                        cx.stop_propagation();
                        cb(window, cx);
                    }
                });
    }

    let backdrop_bg = if props.modal {
        p.overlay
    } else {
        hsla(0.0, 0.0, 0.0, 0.0)
    };

    let mut overlay = div()
        .id(SharedString::from(format!("{}-overlay", props.id)))
        .test_support()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(backdrop_bg)
        .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
            on_drag_move(event.position, (dialog_w, dialog_h_est), window, cx);
        })
        .on_mouse_up(MouseButton::Left, move |_: &MouseUpEvent, window, cx| {
            on_drag_end(window, cx);
        })
        .child(card);

    if props.modal {
        let backdrop_close = on_close;
        overlay = overlay.on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            if can_dismiss && let Some(cb) = backdrop_close.as_ref() {
                cb(window, cx);
            }
        });
    }

    overlay.into_any_element()
}
