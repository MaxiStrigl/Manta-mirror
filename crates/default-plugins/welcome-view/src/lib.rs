use std::sync::{Arc, Mutex};

use gpui::{
    AppContext, Context, EventEmitter, FocusHandle, InteractiveElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, WeakEntity, div, rgb,
};
use manta_api::{EditorAPI, EditorEvent, EventSubscription, MantaPlugin, Theme};

pub struct WelcomView {
    focus_handle: FocusHandle,
    theme: Theme,
}

pub enum WelcomeEvent {
    ExecuteCommand(String),
}

impl EventEmitter<WelcomeEvent> for WelcomView {}

impl Render for WelcomView {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::prelude::Context<Self>,
    ) -> impl gpui::prelude::IntoElement {
        let theme_color_bg = self.theme.color_bg;
        let theme_color_surface = self.theme.color_surface;
        let theme_color_text = self.theme.color_text;

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(rgb(theme_color_bg))
            .track_focus(&self.focus_handle)
            .child(
                div()
                    .text_xl()
                    .text_color(rgb(theme_color_text))
                    .child("Welcome to Manta"),
            )
            .child(
                div()
                    .p_2()
                    .bg(rgb(theme_color_surface))
                    .mt_4()
                    .child("Open File")
                    .id("open-file-btn")
                    .on_click(cx.listener(|_this, _event, _window, cx| {
                        println!("Button pressed");
                        cx.emit(WelcomeEvent::ExecuteCommand("core:black".to_string()));
                    })),
            )
    }
}

pub struct PluginState {
    active_view: Option<WeakEntity<WelcomView>>,
    start_subscription: Option<EventSubscription>,
}

pub struct WelcomeViewPlugin {
    state: Arc<Mutex<PluginState>>,
}

impl Default for WelcomeViewPlugin {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(PluginState {
                active_view: None,
                start_subscription: None,
            })),
        }
    }
}

impl<T: EditorAPI<T> + 'static> MantaPlugin<T> for WelcomeViewPlugin {
    fn id(&self) -> &'static str {
        "welcome-gui"
    }

    fn on_load(&self, api: &mut dyn manta_api::EditorAPI<T>, cx: &mut Context<T>) {
        let state = Arc::clone(&self.state);

        let subscription = api.subscribe_event(
            Box::new(move |event, api, cx| {
                if event == &EditorEvent::EdiotrStarted {
                    let theme = api.get_theme();
                    let view = cx.new(|cx| WelcomView {
                        focus_handle: cx.focus_handle(),
                        theme,
                    });
                    let handle = view.read(cx).focus_handle.clone();

                    cx.subscribe(&view, |workspace: &mut T, _, event, cx| match event {
                        WelcomeEvent::ExecuteCommand(cmd) => {
                            // let _ = workspace.execute_command(cmd, cx);
                        }
                    })
                    .detach();

                    state.lock().unwrap().active_view = Some(view.downgrade());

                    api.open_custom_panel(view.into(), Some(handle), cx);
                }
            }),
            cx,
        );

        let state = Arc::clone(&self.state);
        state.lock().unwrap().start_subscription = Some(subscription);
    }

    fn on_theme_change(&self, new_theme: Theme, cx: &mut Context<T>) {
        let state = self.state.lock().unwrap();

        if let Some(weak_view) = state.active_view.as_ref() {
            let _ = weak_view.update(cx, |view, cx| {
                view.theme = new_theme;
                cx.notify();
            });
        }
    }
}
