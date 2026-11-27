//! Positions workspace (implemented by `ws-positions`).
use crate::events::WorkspaceEvent;
use gpui_kit::{Context, EventEmitter, IntoElement, Render, Window, div};

pub struct PositionsWorkspace {
    visible: bool,
}
impl PositionsWorkspace {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { visible: true }
    }
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        cx.notify();
    }
}
impl EventEmitter<WorkspaceEvent> for PositionsWorkspace {}
impl Render for PositionsWorkspace {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
