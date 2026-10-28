use std::cell::Cell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::default;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{AnyView, KeyBinding, Window, actions};
use gpui::{Context, Entity, EventEmitter, FocusHandle, IntoElement, div, prelude::*, rgb};
use manta_api::{
    Arg, CommandCompletion, CommandKey, CommandMachine, CommandSpec, CompletionKind, EditorAPI,
    EditorEvent, EventHub, EventSubscription, MantaPlugin,
};
use manta_config::theme::Theme;
use manta_config::{MantaConfig, Settings};
use manta_core::buffer::Buffer;

use crate::{CustomPane, VimEditor};

pub enum CenterPane {
    Editor(Entity<VimEditor>),
    CustomUI(AnyView, FocusHandle),
}

pub struct CommandEntry {
    spec: CommandSpec,
    action: Box<dyn Fn(&mut dyn EditorAPI<Workspace>, &[Arg], &mut Context<Workspace>)>,
}

pub struct Workspace {
    pub center_pane: CenterPane,
    pub workspace_dir: PathBuf,

    pub active_panel: Option<AnyView>,
    pub next_focus: Option<FocusHandle>,
    pub bottom_bar: Option<AnyView>,

    pub focus_handle: FocusHandle,
    pub command_machine: CommandMachine,

    pub commands: HashMap<&'static str, CommandEntry>, // key = spec.id (core:save)
    pub event_hub: EventHub<Workspace>,

    // here plugin event slots later
    pub plugins: Vec<Box<dyn MantaPlugin<Workspace>>>,
    pub settings: Arc<Settings>,
    pub theme: Arc<Theme>,
}

actions!(workspace, [OpenCommandMode]);

impl Workspace {
    pub fn new(
        cx: &mut Context<Self>,
        workspace_dir: PathBuf,
        config: MantaConfig,
        initial_file: Option<PathBuf>,
    ) -> Self {
        let buffer = cx.new(|_| {
            if let Some(path) = initial_file {
                Buffer::from_file(path).unwrap_or_else(|_| Buffer::new())
            } else {
                Buffer::new()
            }
        });

        cx.bind_keys([KeyBinding::new(
            "ctrl-p",
            OpenCommandMode,
            Some("Workspace"),
        )]);

        let settings = Arc::new(config.settings);
        let theme = Arc::new(config.theme);

        let editor = cx.new(|cx| VimEditor::new(cx, buffer.clone(), &theme, &settings));

        // cx.subscribe(&editor, |this, _, event, cx| match event {
        //     EditorEvent::ExecuteCommand(name) => {
        //         let _ = this.execute_command(name, cx);
        //     }
        // })
        // .detach();

        let center_pane = CenterPane::Editor(editor);

        let mut workspace = Self {
            center_pane,
            workspace_dir,
            active_panel: None,
            next_focus: None,
            bottom_bar: None,
            commands: HashMap::new(),
            plugins: Vec::new(),
            settings,
            theme,
            event_hub: EventHub::new(),
            focus_handle: cx.focus_handle(),
            command_machine: CommandMachine::new(),
        };
        workspace.register_core_commands();
        workspace
    }

    pub fn load_plugin(&mut self, plugin: Box<dyn MantaPlugin<Workspace>>, cx: &mut Context<Self>) {
        plugin.on_load(self as &mut dyn EditorAPI<Workspace>, cx);
        self.plugins.push(plugin);
    }

    pub fn execute_command(
        &mut self,
        id: &str,
        args: &[Arg],
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(entry) = self.commands.remove(id) else {
            return Err(format!("Command: '{id}' not found"));
        };
        let command_id = entry.spec.id;

        (entry.action)(self as &mut dyn EditorAPI<Workspace>, args, cx);
        self.commands.insert(command_id, entry);
        Ok(())
    }

    fn register_core_commands(&mut self) {
        self.register_command(
            CommandSpec::new("core:save", "Save file"),
            Box::new(|api, _args, cx| {
                api.save_active_file(cx);
            }),
        );

        self.register_command(
            CommandSpec::new("core:quit", "Quit Manta"),
            Box::new(|_api, _args, cx| {
                cx.quit();
            }),
        );
    }

    fn open_command_mode(&mut self, cx: &mut Context<Self>) {
        if self.command_machine.state.open {
            return;
        }

        println!("opening");
        self.command_machine.open();
        let candidates = self.completions_for(String::new());
        self.command_machine.set_candidates(candidates);
        self.next_focus = Some(self.focus_handle.clone());
        let state = self.command_machine.state.clone();
        self.emit(&EditorEvent::CommandModeChanged(state), cx);
        cx.notify();
    }

    fn apply_command_key(&mut self, key: CommandKey, cx: &mut Context<Self>) {
        match self.command_machine.apply(key) {
            manta_api::CommandEffect::Updated => {
                let candidates = {
                    let input = self.command_machine.state.input.clone();
                    self.completions_for(input)
                };

                self.command_machine.set_candidates(candidates);
                let state = self.command_machine.state.clone();
                dbg!(&state);
                self.emit(&EditorEvent::CommandModeChanged(state), cx);
                cx.notify();
            }
            manta_api::CommandEffect::Submited(line) => self.submit_command(&line, cx),
            manta_api::CommandEffect::Canceled => self.exit_command_mode(cx),
        }
    }

    fn submit_command(&mut self, line: &str, cx: &mut Context<Self>) {
        let (id, args, result) = self.run_command(line, cx);
        self.exit_command_mode(cx);
        self.emit(&EditorEvent::CommandExecuted { id, args, result }, cx);
        cx.notify();
    }

    fn exit_command_mode(&mut self, cx: &mut Context<Self>) {
        let state = self.command_machine.state.clone();
        self.emit(&EditorEvent::CommandModeChanged(state), cx);
        self.focus_main_panel(cx);
        cx.notify();
    }

    fn command_key_from_event(&self, event: gpui::KeyDownEvent) -> Option<CommandKey> {
        let key = event.keystroke.key.as_str();
        match key {
            "backspace" => Some(CommandKey::Backspace),
            "delete" => Some(CommandKey::Delete),
            "enter" => Some(CommandKey::Enter),
            "escape" => Some(CommandKey::Escape),
            "tab" => Some(CommandKey::TabCompletion),
            "up" => Some(CommandKey::HistoryPrev),
            "next" => Some(CommandKey::HistoryNext),
            "left" => Some(CommandKey::MoveLeft),
            "right" => Some(CommandKey::MoveRight),
            "home" => Some(CommandKey::Start),
            "end" => Some(CommandKey::End),
            _ => Some(CommandKey::Input(key.chars().next()?)),
        }
    }

    fn run_command(
        &mut self,
        line: &str,
        cx: &mut Context<Self>,
    ) -> (String, Vec<Arg>, Result<(), String>) {
        let tokens: Vec<&str> = line.trim().split_whitespace().collect();
        let Some(id) = tokens.first().copied() else {
            return (String::new(), Vec::new(), Err("empty command".into()));
        };

        let id_owned = id.to_owned();
        let Some(entry) = self.commands.get(id) else {
            return (
                String::new(),
                Vec::new(),
                Err(format!("Command: '{id}' not found.")),
            );
        };

        match entry.spec.bind_args(&tokens[1..]) {
            Ok(args) => (id_owned, args.clone(), self.execute_command(&id, &args, cx)),
            Err(e) => (String::new(), Vec::new(), Err(e)),
        }
    }

    fn completions_for(&mut self, input: String) -> Vec<CommandCompletion> {
        if !input
            .split_whitespace()
            .next()
            .is_some_and(|h| h.len() < input.trim_end().len())
        {
            return self
                .commands
                .keys()
                .filter(|id| id.starts_with(&input))
                .map(|id| CommandCompletion {
                    kind: CompletionKind::Command,
                    insert: id.to_string(),
                    label: id.to_string(),
                })
                .collect();
        }

        let last = input.split_whitespace().next_back().unwrap_or("");
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.workspace_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(last) {
                    out.push(CommandCompletion {
                        kind: CompletionKind::File,
                        insert: name.clone(),
                        label: name,
                    });
                }
            }
        }

        out
    }

    pub fn emit(&mut self, event: &EditorEvent, cx: &mut Context<Self>) {
        let mut hub = std::mem::take(&mut self.event_hub);
        hub.dispatch(event, self as &mut dyn EditorAPI<Workspace>, cx);
        self.event_hub = hub;
        cx.notify();
    }
}

impl EditorAPI<Workspace> for Workspace {
    fn register_command(
        &mut self,
        spec: CommandSpec,
        action: Box<dyn Fn(&mut dyn EditorAPI<Workspace>, &[Arg], &mut Context<Workspace>)>,
    ) {
        self.commands.insert(spec.id, CommandEntry { spec, action });
    }

    fn execute_command(
        &mut self,
        id: &str,
        args: &[Arg],
        cx: &mut Context<Workspace>,
    ) -> Result<(), String> {
        Workspace::execute_command(self, id, args, cx)
    }

    fn subscribe_event<E: EventPayload>(
        &mut self,
        handler: Box<
            dyn Fn(&manta_api::EditorEvent, &mut dyn EditorAPI<Workspace>, &mut Context<Workspace>),
        >,
        _cx: &mut Context<Workspace>,
    ) -> manta_api::EventSubscription {
        self.event_hub.subscribe(handler);
        self.event_slots.retain(|slot| slot.alive.get());
        let alive = Rc::new(Cell::new(true));

        self.event_slots.push(EventSlot {
            alive: alive.clone(),
            handler,
        });
        EventSubscription::new(alive)
    }

    fn open_panel(
        &mut self,
        view: AnyView,
        focus_handle: Option<FocusHandle>,
        cx: &mut Context<Workspace>,
    ) {
        self.active_panel = Some(view);
        self.next_focus = focus_handle;
        cx.notify();
    }

    fn close_panel(&mut self, cx: &mut Context<Workspace>) {
        self.active_panel = None;
        self.next_focus = match &self.center_pane {
            CenterPane::Editor(editor) => Some(editor.read(cx).focus_handle.clone()),
            CenterPane::CustomUI(_, focus_handle) => Some(focus_handle.clone()),
        };
        cx.notify();
    }

    fn open_file(&mut self, path: &str, cx: &mut Context<Workspace>) {
        let buffer = cx.new(|_| Buffer::from_file(path).unwrap_or_else(|_| Buffer::new()));
        let new_editor = cx.new(|cx| VimEditor::new(cx, buffer, &self.theme, &self.settings));
        // cx.subscribe(&new_editor, |this, _, event, cx| match event {
        //     EditorEvent::ExecuteCommand(name) => {
        //         let _ = this.execute_command(name, &[], cx);
        //     }
        // })
        // .detach();
        //
        self.center_pane = CenterPane::Editor(new_editor);
        self.close_panel(cx);
        let _ = self.execute_command("command-bar:update", &[], cx);
    }

    fn workspace_dir(&self) -> PathBuf {
        self.workspace_dir.clone()
    }

    fn get_available_commands(&self) -> Vec<CommandSpec> {
        self.commands
            .values()
            .map(|entry| entry.spec.clone())
            .collect()
    }

    fn get_buffer_path(&self, cx: &mut Context<Workspace>) -> Option<PathBuf> {
        if let CenterPane::Editor(editor) = &self.center_pane {
            let editor = editor.read(cx);
            let buffer = editor.buffer.clone();

            buffer.read(cx).path.clone()
        } else {
            None
        }
    }

    fn get_insert_mode_status(&self, cx: &mut Context<Workspace>) -> bool {
        if let CenterPane::Editor(editor) = &self.center_pane {
            let editor = editor.read(cx);
            editor.is_insert_mode
        } else {
            false
        }
    }

    fn set_bottom_bar(
        &mut self,
        view: Option<AnyView>,
        focus_handle: Option<FocusHandle>,
        cx: &mut Context<Workspace>,
    ) {
        self.bottom_bar = view;
        self.next_focus = focus_handle;
        cx.notify();
    }

    fn focus_main_panel(&mut self, cx: &mut Context<Workspace>) {
        let handle = match &self.center_pane {
            CenterPane::Editor(editor) => editor.read(cx).focus_handle.clone(),
            CenterPane::CustomUI(_, focus_handle) => focus_handle.clone(),
        };
        self.next_focus = Some(handle);
        cx.notify();
    }

    fn save_active_file(&mut self, cx: &mut Context<Workspace>) {
        if let CenterPane::Editor(editor) = &self.center_pane {
            editor.update(cx, |editor, cx| {
                editor.buffer.update(cx, |buffer, _cx| match buffer.save() {
                    Ok(_) => println!("File Saved"),
                    Err(e) => println!("Failed to save file: {}", e),
                })
            })
        }
    }

    fn add_inline_replacement(
        &mut self,
        replacement: manta_api::InlineReplacement,
        cx: &mut Context<Workspace>,
    ) -> usize {
        if let CenterPane::Editor(editor) = &self.center_pane {
            editor.update(cx, |editor, cx| {
                let id = editor.next_replacement_id;
                editor.next_replacement_id += 1;
                editor.replacements.insert(id, replacement);
                cx.notify();
                id
            })
        } else {
            usize::MAX
        }
    }

    fn remove_inline_replacement(&mut self, id: usize, cx: &mut Context<Workspace>) {
        if let CenterPane::Editor(editor) = &self.center_pane {
            editor.update(cx, |editor, cx| {
                editor.replacements.remove(&id);
                cx.notify();
            })
        }
    }

    fn get_cursor_byte_offset(&mut self, cx: &mut Context<Workspace>) -> usize {
        if let CenterPane::Editor(editor) = &self.center_pane {
            let editor = editor.read(cx);
            let buffer = editor.buffer.clone();

            buffer.read(cx).text.char_to_byte(editor.cursor_offset)
        } else {
            usize::MAX
        }
    }

    fn get_cursor_char_position(&mut self, cx: &mut Context<Workspace>) -> Option<usize> {
        if let CenterPane::Editor(editor) = &self.center_pane {
            let editor = editor.read(cx);
            let buffer = editor.buffer.clone();
            let text = &buffer.read(cx).text;

            let line_number = text.char_to_line(editor.cursor_offset);
            let chars_to_line = text.line_to_char(line_number);

            return Some(editor.cursor_offset - chars_to_line);
        }

        None
    }

    fn get_cursor_line_position(&mut self, cx: &mut Context<Workspace>) -> Option<usize> {
        if let CenterPane::Editor(editor) = &self.center_pane {
            let editor = editor.read(cx);
            let buffer = editor.buffer.clone();
            let text = &buffer.read(cx).text;

            let line_number = text.char_to_line(editor.cursor_offset);

            return Some(line_number);
        }

        None
    }

    fn get_buffer_text(&mut self, cx: &mut Context<Workspace>) -> String {
        if let CenterPane::Editor(editor) = &self.center_pane {
            let editor = editor.read(cx);
            let buffer = editor.buffer.clone();

            buffer.read(cx).text.to_string()
        } else {
            String::new()
        }
    }

    fn get_replacement_at_byte(
        &self,
        plugin_id: String,
        byte_offset: usize,
        cx: &mut Context<Workspace>,
    ) -> Option<usize> {
        if let CenterPane::Editor(editor) = &self.center_pane {
            let editor = editor.read(cx);

            for (id, replacement) in &editor.replacements {
                if replacement.plugin_id == plugin_id
                    && replacement.start_byte <= byte_offset
                    && replacement.end_byte >= byte_offset
                {
                    return Some(*id);
                }
            }
            None
        } else {
            None
        }
    }

    fn open_custom_panel(
        &mut self,
        view: AnyView,
        focus: Option<FocusHandle>,
        cx: &mut Context<Workspace>,
    ) {
        let pane = cx.new(|cx| CustomPane::new(view, cx));
        let handle = if let Some(handle) = focus {
            handle
        } else {
            pane.read(cx).focus_handle.clone()
        };

        // cx.subscribe(&pane, |this, _, event, cx| match event {
        //     EditorEvent::ExecuteCommand(name) => {
        //         let _ = this.execute_command(name, &[], cx);
        //     }
        // })
        // .detach();

        self.center_pane = CenterPane::CustomUI(pane.clone().into(), handle);

        self.close_panel(cx);
    }

    fn get_theme(&self) -> Theme {
        (*self.theme).clone()
    }

    fn set_theme(&mut self, new_theme: Theme, cx: &mut Context<Self>) {
        self.theme = Arc::new(new_theme.clone());

        if let CenterPane::Editor(e) = &self.center_pane {
            e.update(cx, |editor, cx| {
                editor.update_theme(new_theme.clone(), cx);
                println!("Update theme");
            });
        }

        let plugins = std::mem::take(&mut self.plugins);
        for plugin in &plugins {
            plugin.on_theme_change(new_theme.clone(), cx);
        }

        self.plugins = plugins;
        cx.notify();
    }

    fn open_command_mode(&mut self, cx: &mut Context<Workspace>) {
        Workspace::open_command_mode(self, cx);
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let center_content = match &self.center_pane {
            CenterPane::Editor(editor) => div().size_full().child(editor.clone()),
            CenterPane::CustomUI(view, _) => div().size_full().child(view.clone()),
        };

        if let Some(handle) = self.next_focus.take() {
            handle.focus(window);
        }

        let mut layout = div()
            .track_focus(&self.focus_handle)
            .key_context("Workspace")
            .on_key_down(cx.listener(|workspace, event: &gpui::KeyDownEvent, _, cx| {
                if workspace.command_machine.state.open {
                    if let Some(key) = workspace.command_key_from_event(event.clone()) {
                        workspace.apply_command_key(key, cx);
                    }
                }
            }))
            .on_action(cx.listener(|workspace, _: &OpenCommandMode, _, cx| {
                Workspace::open_command_mode(workspace, cx);
            }))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0xe1e1e1))
            .child(center_content);

        if let Some(panel) = &self.active_panel {
            layout = layout.child(
                div()
                    // .border_t_1()
                    .border_color(rgb(self.theme.color_border))
                    .child(panel.clone()),
            );
        }

        if let Some(bar) = &self.bottom_bar {
            layout = layout.child(bar.clone())
        }

        layout
    }
}
