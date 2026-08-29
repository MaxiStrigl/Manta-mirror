use std::path::PathBuf;

use gpui::Window;
use gpui::{Context, Entity, EventEmitter, FocusHandle, IntoElement, div, prelude::*, rgb};
use manta_core::buffer::Buffer;

use crate::VimEditor;

pub struct Workspace {
    pub editor: Entity<VimEditor>,

    pub workspace_dir: PathBuf,
}

pub enum EditorEvent {
    OpenFinder,
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

        Self {
            editor,
            workspace_dir,
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut layout = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0xe1e1e1))
            .child(self.editor.clone());

        layout
    }
}
