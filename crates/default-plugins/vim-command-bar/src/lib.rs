use gpui::{
    AppContext, Context, EventEmitter, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, Render, Styled, Window, div, rgb,
};
use manta_api::{CommandInfo, EditorAPI, MantaPlugin};
use manta_components::{FuzzySearch, FuzzySearchEvent};

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
}

impl VimCommandBar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let searcher = FuzzySearch::new(
            |command: &CommandInfo, is_slected: bool| {
                let display = command.name.clone();

                let bg = if is_slected {
                    rgb(0xff0000)
                } else {
                    rgb(0x00ff00)
                };

                div()
                    .px_2()
                    .py_1()
                    .bg(bg)
                    .text_color(rgb(0xffffff))
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
                        .bg(rgb(0x111111))
                        .text_color(rgb(0xffffff))
                        .flex()
                        .px_2()
                        .child(format!("Normal | 140 | Some_file.txt")),
                );
            }
            BarState::Command => {
                layout = layout
                    .child(
                        div()
                            .bottom_full()
                            .w_full()
                            .max_h_64()
                            .bg(rgb(0x1a1a1a))
                            .border_t_1()
                            .border_color(rgb(0x333333))
                            .child(self.searcher.render_list()),
                    )
                    .child(
                        div()
                            .h_8()
                            .w_full()
                            .bg(rgb(0x111111))
                            .text_color(rgb(0xffffff))
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
        let bar = cx.new(|cx| VimCommandBar::new(cx));
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
    }
}
