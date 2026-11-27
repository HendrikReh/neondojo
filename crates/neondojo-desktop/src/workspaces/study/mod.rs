//! Study workspace (implemented by `ws-study`).
use crate::events::WorkspaceEvent;
use gpui_kit::{Context, EventEmitter, IntoElement, Render, Window, div};

pub struct StudyWorkspace {
    visible: bool,
}
impl StudyWorkspace {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { visible: true }
    }
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        cx.notify();
    }
}
impl EventEmitter<WorkspaceEvent> for StudyWorkspace {}
impl Render for StudyWorkspace {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
