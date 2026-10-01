use std::path::PathBuf;

use gpui::{
    AppContext, Context, EventEmitter, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, Render, Styled, Window, div, px, rgb,
};
use manta_api::{CommandInfo, EditorAPI, MantaPlugin};
use manta_components::{FuzzySearch, FuzzySearchEvent};
use manta_config;

pub enum BarState {
    Status,
    Command,
}

pub struct VimCommandBar {
    pub state: BarState,
    pub searcher: FuzzySearch<CommandInfo>,
    pub focus_handle: FocusHandle,
    pub input_text: String,
    pub available_commands: Vec<CommandInfo>,
    pub workspace_dir: Option<PathBuf>,
    pub is_insert_mode: bool,
}

impl VimCommandBar {
    pub fn new(cx: &mut Context<Self>) -> Self {

        let searcher = FuzzySearch::new(
            |command: &CommandInfo, is_slected: bool| {
                let theme_color_bg = manta_config::CONFIG.theme.color_bg;
                let theme_color_surface = manta_config::CONFIG.theme.color_surface;
                let theme_color_text = manta_config::CONFIG.theme.color_text;

                let display = command.name.clone();

                let bg = if is_slected {
                    rgb(theme_color_surface)
                } else {
                    rgb(theme_color_bg)
                };

                div()
                    .px_2()
                    .py_1()
                    .line_height(px(26.0))
                    .bg(bg)
                    .text_color(rgb(theme_color_text))
                    .child(display)
                    .into_any_element()
            },
            |command: &CommandInfo| command.name.clone(),
        );

        VimCommandBar {
            state: BarState::Status,
            searcher,
            focus_handle: cx.focus_handle(),
            input_text: String::new(),
            available_commands: Vec::new(),
            workspace_dir: None,
            is_insert_mode: false,
        }
    }

    fn set_commands(&mut self, available_commands: Vec<CommandInfo>) {
        self.available_commands = available_commands.clone();
        self.searcher.set_all_items(available_commands);
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.state, BarState::Status) {
            return;
        }

        if let Some(action) = self.searcher.handle_key_down(event) {
            match action {
                FuzzySearchEvent::Select(entry) => cx.emit(entry.name.clone()),
                FuzzySearchEvent::Close => cx.emit("core:abort".to_string()),
                _ => {}
            }
            cx.notify();
            return;
        }

        let is_ctrl = event.keystroke.modifiers.control;

        match event.keystroke.key.as_str() {
            "escape" => cx.emit("core:abort".to_string()),
            "backspace" => {
                self.input_text.pop();
                self.searcher.update_query(&self.input_text);
            }
            _ => {
                if let Some(c) = &event.keystroke.key_char {
                    if !is_ctrl {
                        self.input_text.push_str(c);
                        self.searcher.update_query(&self.input_text);
                    }
                }
            }
        }

        cx.notify();
    }
}

impl EventEmitter<String> for VimCommandBar {}

impl Render for VimCommandBar {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_color_bg = manta_config::CONFIG.theme.color_bg;
        let theme_color_surface = manta_config::CONFIG.theme.color_surface;
        let theme_color_text = manta_config::CONFIG.theme.color_text;
        let theme_color_border = manta_config::CONFIG.theme.color_border;

        let mut layout = div()
            .w_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down));

        match self.state {
            BarState::Status => {
                layout = layout.child(
                    div()
                        .h_8()
                        .bg(rgb(theme_color_surface))
                        .text_color(rgb(theme_color_text))
                        .border_t_1()
                        .border_color(rgb(theme_color_border))
                        .flex()
                        .px_2()
                        .child(format!(
                            "{} | {}",
                            if self.is_insert_mode.clone() {
                                "Insert"
                            } else {
                                "Normal"
                            },
                            &self
                                .workspace_dir
                                .clone()
                                .map_or("~".to_string(), |p| p.to_string_lossy().to_string())
                        )),
                );
            }
            BarState::Command => {
                layout = layout
                    .child(
                        div()
                            .w_full()
                            .max_h_64()
                            .bg(rgb(theme_color_bg))
                            .border_t_1()
                            .border_color(rgb(theme_color_border))
                            .child(self.searcher.render_list()),
                    )
                    .child(
                        div()
                            .h_8()
                            .w_full()
                            .bg(rgb(theme_color_surface))
                            .border_t_1()
                            .border_color(rgb(theme_color_border))
                            .text_color(rgb(theme_color_text))
                            .child(format!(":{}", self.input_text)),
                    )
            }
        }

        layout
    }
}

pub struct VimCommandBarPlugin;

impl<T: EditorAPI<T> + 'static> MantaPlugin<T> for VimCommandBarPlugin {
    fn id(&self) -> &'static str {
        "vim-command-bar"
    }

    fn on_load(&self, api: &mut dyn manta_api::EditorAPI<T>, cx: &mut Context<T>) {
        let path = api.get_buffer_path(cx);
        let bar = cx.new(|cx| {
            let mut bar = VimCommandBar::new(cx);
            bar.workspace_dir = path;
            bar
        });
        api.set_bottom_bar(Some(bar.clone().into()), None, cx);

        cx.subscribe(&bar, |workspace: &mut T, view, command_id, cx| {
            workspace.set_bottom_bar(Some(view.clone().into()), None, cx);

            view.update(cx, |bar, cx| {
                bar.state = BarState::Status;
                bar.input_text.clear();
                cx.notify();
            });

            workspace.focus_main_panel(cx);

            if command_id != "core:abort" {
                let _ = workspace.execute_command(command_id, cx);
            }
        })
        .detach();

        let bar_clone = bar.clone();

        api.register_command(
            "command-bar:open",
            "Open Command Bar",
            Box::new(move |api, cx| {
                let handle = bar.read(cx).focus_handle.clone();
                bar.update(cx, |bar, cx| {
                    bar.set_commands(api.get_available_commands());
                    bar.state = BarState::Command;
                    cx.notify();
                });

                api.set_bottom_bar(Some(bar.clone().into()), Some(handle), cx);
            }),
        );

        api.register_command(
            "command-bar:update",
            "Update Command Bar",
            Box::new(move |api, cx| {
                let path = api.get_buffer_path(cx);
                let insert_mode_status = api.get_insert_mode_status(cx);
                bar_clone.update(cx, |bar, cx| {
                    bar.workspace_dir = path;
                    bar.is_insert_mode = insert_mode_status;
                    cx.notify();
                });
            }),
        );
    }
}
