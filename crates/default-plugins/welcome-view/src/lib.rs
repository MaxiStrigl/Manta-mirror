use gpui::{
    AppContext, Application, Context, EventEmitter, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window, WindowOptions, div, red,
    rgb, white,
};
use manta_api::{EditorAPI, MantaPlugin};

pub struct WelcomView {
    focus_handle: FocusHandle,
}

pub enum WelcomeEvent {
    ExecuteCommand(String),
}

impl EventEmitter<WelcomeEvent> for WelcomView {}

impl Render for WelcomView {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::prelude::Context<Self>,
    ) -> impl gpui::prelude::IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(rgb(0x1e1e1e))
            .track_focus(&self.focus_handle)
            .child(
                div()
                    .text_xl()
                    .text_color(rgb(0xafafaf))
                    .child("Welcome to Manta"),
            )
            .child(
                div()
                    .p_2()
                    .bg(rgb(0x333333))
                    .mt_4()
                    .child("Open File")
                    .id("open-file-btn")
                    .on_click(cx.listener(|this, event, window, cx| {
                        cx.emit(WelcomeEvent::ExecuteCommand("core:open_file".to_string()));
                    })),
            )
    }
}

pub struct WelcomeViewPlugin;

impl<T: EditorAPI<T> + 'static> MantaPlugin<T> for WelcomeViewPlugin {
    fn id(&self) -> &'static str {
        "welcome-gui"
    }

    fn on_load(&self, api: &mut dyn manta_api::EditorAPI<T>, cx: &mut Context<T>) {
        api.register_command(
            "welcome:open",
            "Open Start Screen",
            Box::new(|api, cx| {
                let view = cx.new(|cx| WelcomView {
                    focus_handle: cx.focus_handle(),
                });
                let handle = view.read(cx).focus_handle.clone();

                cx.subscribe(&view, |workspace: &mut T, _, event, cx| match event {
                    WelcomeEvent::ExecuteCommand(cmd) => {
                        let _ = workspace.execute_command(cmd, cx);
                    }
                })
                .detach();

                api.open_custom_panel(view.into(), Some(handle), cx);
            }),
        );
    }
}
