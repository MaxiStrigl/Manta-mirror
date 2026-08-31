use std::collections::HashMap;
use std::path::PathBuf;

use gpui::{AnyView, Window};
use gpui::{Context, Entity, EventEmitter, FocusHandle, IntoElement, div, prelude::*, rgb};
use manta_api::{EditorAPI, MantaPlugin};
use manta_core::buffer::Buffer;

use crate::VimEditor;

pub struct Workspace {
    pub editor: Entity<VimEditor>,
    pub workspace_dir: PathBuf,

    pub active_panel: Option<AnyView>,
    pub next_focus: Option<FocusHandle>,

    pub commands:
        HashMap<String, Box<dyn Fn(&mut dyn EditorAPI<Workspace>, &mut Context<Workspace>)>>,

    pub plugins: Vec<Box<dyn MantaPlugin<Workspace>>>,
}

pub enum EditorEvent {
    ExecuteCommand(String),
}
impl EventEmitter<EditorEvent> for VimEditor {}

impl Workspace {
    pub fn new(
        cx: &mut Context<Self>,
        workspace_dir: PathBuf,
        initial_file: Option<PathBuf>,
    ) -> Self {
        let buffer = cx.new(|_| {
            if let Some(path) = initial_file {
                Buffer::from_file(path).unwrap_or_else(|_| Buffer::new())
            } else {
                Buffer::new()
            }
        });

        let editor = cx.new(|cx| VimEditor::new(cx, buffer.clone()));

        cx.subscribe(&editor, |this, _, event, cx| match event {
            EditorEvent::ExecuteCommand(name) => {
                let _ = this.execute_command(name, cx);
            }
        })
        .detach();

        Self {
            editor,
            workspace_dir,
            active_panel: None,
            next_focus: None,
            commands: HashMap::new(),
            plugins: Vec::new(),
        }
    }

    pub fn load_plugin(
        &mut self,
        mut plugin: Box<dyn MantaPlugin<Workspace>>,
        cx: &mut Context<Self>,
    ) {
        plugin.on_load(self as &mut dyn EditorAPI<Workspace>);
        self.plugins.push(plugin);
    }

    pub fn execute_command(&mut self, name: &str, cx: &mut Context<Self>) -> Result<(), String> {
        if let Some(action) = self.commands.remove(name) {
            action(self as &mut dyn EditorAPI<Workspace>, cx);
            self.commands.insert(name.to_string(), action);
            Ok(())
        } else {
            Err(format!("Command: \"{}\" not found", name))
        }
    }
}

impl EditorAPI<Workspace> for Workspace {
    fn register_command(
        &mut self,
        name: &str,
        action: Box<dyn Fn(&mut dyn EditorAPI<Workspace>, &mut Context<Workspace>)>,
    ) {
        self.commands.insert(name.to_string(), action);
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
        self.next_focus = Some(self.editor.read(cx).focus_handle.clone());
        cx.notify();
    }

    fn open_file(&mut self, path: &str, cx: &mut Context<Workspace>) {
        let buffer = cx.new(|_| Buffer::from_file(path).unwrap_or_else(|_| Buffer::new()));

        let new_editor = cx.new(|cx| VimEditor::new(cx, buffer));
        self.editor = new_editor;
        self.close_panel(cx);
    }

    fn workspace_dir(&self) -> PathBuf {
        self.workspace_dir.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(handle) = self.next_focus.take() {
            handle.focus(window);
        }

        let mut layout = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0xe1e1e1))
            .child(self.editor.clone());

        if let Some(panel) = &self.active_panel {
            layout = layout.child(
                div()
                    .h_64()
                    .border_t_1()
                    .border_color(rgb(0x0000ff))
                    .child(panel.clone()),
            );
        }

        layout
    }
}
