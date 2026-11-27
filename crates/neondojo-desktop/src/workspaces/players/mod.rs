//! Player Directory workspace (implemented by `ws-extras-players`).
use crate::events::WorkspaceEvent;
use gpui_kit::{Context, EventEmitter, IntoElement, Render, Window, div};

pub struct PlayersWorkspace {
    visible: bool,
}
impl PlayersWorkspace {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { visible: true }
    }
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        cx.notify();
    }
}
impl EventEmitter<WorkspaceEvent> for PlayersWorkspace {}
impl Render for PlayersWorkspace {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
