use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{AnyView, Window};
use gpui::{Context, Entity, EventEmitter, FocusHandle, IntoElement, div, prelude::*, rgb};
use manta_api::{Arg, CommandInfo, CommandSpec, EditorAPI, MantaPlugin};
use manta_config::theme::Theme;
use manta_config::{MantaConfig, Settings};
use manta_core::buffer::Buffer;

use crate::{CustomPane, VimEditor};

pub enum CenterPane {
    Editor(Entity<VimEditor>),
    CustomUI(AnyView, FocusHandle),
}

struct CommandEntry {
    spec: CommandSpec,
    action: Box<dyn Fn(&mut dyn EditorAPI<Workspace>, &[Arg], &mut Context<Workspace>)>,
}

pub struct Workspace {
    pub center_pane: CenterPane,
    pub workspace_dir: PathBuf,

    pub active_panel: Option<AnyView>,
    pub next_focus: Option<FocusHandle>,
    pub bottom_bar: Option<AnyView>,

    pub commands: HashMap<&'static str, CommandEntry>, // key = spec.id (core:save)

    pub plugins: Vec<Box<dyn MantaPlugin<Workspace>>>,
    pub settings: Arc<Settings>,
    pub theme: Arc<Theme>,
}

pub enum EditorEvent {
    ExecuteCommand(String),
}
impl EventEmitter<EditorEvent> for VimEditor {}

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

        let settings = Arc::new(config.settings);
        let theme = Arc::new(config.theme);

        let editor = cx.new(|cx| VimEditor::new(cx, buffer.clone(), &theme, &settings));

        cx.subscribe(&editor, |this, _, event, cx| match event {
            EditorEvent::ExecuteCommand(name) => {
                let _ = this.execute_command(name, cx);
            }
        })
        .detach();

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
        };
        workspace.register_core_commands();

        workspace
    }

    pub fn load_plugin(&mut self, plugin: Box<dyn MantaPlugin<Workspace>>, cx: &mut Context<Self>) {
        plugin.on_load(self as &mut dyn EditorAPI<Workspace>, cx);
        self.plugins.push(plugin);
    }

    pub fn execute_command(&mut self, name: &str, cx: &mut Context<Self>) -> Result<(), String> {
        if let Some((description, action)) = self.commands.remove(name) {
            action(self as &mut dyn EditorAPI<Workspace>, cx);
            self.commands
                .insert(name.to_string(), (description, action));
            Ok(())
        } else {
            Err(format!("Command: \"{}\" not found", name))
        }
    }

    fn register_core_commands(&mut self) {
        self.register_command(
            "core:save",
            "Save File",
            Box::new(|api, cx| {
                api.save_active_file(cx);
            }),
        );

        self.register_command("core:quit", "Quit Editor", Box::new(|_api, cx| cx.quit()));

        self.register_command(
            "core:black",
            "Makes theme black",
            Box::new(move |api, cx| api.set_theme(Theme::default(), cx)),
        );
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
        let entry = self
            .commands
            .get(id)
            .ok_or_else(|| format!("Unknown command"))?;
        Workspace::execute_command(self, name, cx)
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

        cx.subscribe(&new_editor, |this, _, event, cx| match event {
            EditorEvent::ExecuteCommand(name) => {
                let _ = this.execute_command(name, cx);
            }
        })
        .detach();

        self.center_pane = CenterPane::Editor(new_editor);
        self.close_panel(cx);
        let _ = self.execute_command("command-bar:update", cx);
    }

    fn workspace_dir(&self) -> PathBuf {
        self.workspace_dir.clone()
    }

    fn get_available_commands(&self) -> Vec<manta_api::CommandInfo> {
        self.commands
            .iter()
            .map(|(name, (description, _))| CommandInfo {
                name: name.clone(),
                description: description.clone(),
            })
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

        cx.subscribe(&pane, |this, _, event, cx| match event {
            EditorEvent::ExecuteCommand(name) => {
                let _ = this.execute_command(name, cx);
            }
        })
        .detach();

        self.center_pane = CenterPane::CustomUI(pane.clone().into(), handle);

        self.close_panel(cx);
    }

    fn get_theme(&self) -> Theme {
        let x = (*self.theme).clone();
        dbg!(&self.theme);
        x
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
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let center_content = match &self.center_pane {
            CenterPane::Editor(editor) => div().size_full().child(editor.clone()),
            CenterPane::CustomUI(view, _) => div().size_full().child(view.clone()),
        };

        if let Some(handle) = self.next_focus.take() {
            handle.focus(window);
        }

        let mut layout = div()
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
