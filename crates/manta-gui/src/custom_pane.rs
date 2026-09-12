use gpui::{
    AnyView, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, KeyDownEvent,
    ParentElement, Render, Styled, Window, div, rgb,
};

use crate::workspace::EditorEvent;

pub struct CustomPane {
    pub inner_view: AnyView,
    pub focus_handle: FocusHandle,
}

impl EventEmitter<EditorEvent> for CustomPane {}

impl CustomPane {
    pub fn new(inner_view: AnyView, cx: &mut Context<Self>) -> Self {
        Self {
            inner_view,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            ":" => {
                cx.emit(EditorEvent::ExecuteCommand("command-bar:open".to_string()));
            }
            _ => {}
        }
    }
}

impl Focusable for CustomPane {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CustomPane {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> impl gpui::prelude::IntoElement {
        div()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .child(self.inner_view.clone())
    }
}
