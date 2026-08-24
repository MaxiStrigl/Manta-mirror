use std::path::PathBuf;

use gpui::Window;
use gpui::{Context, Entity, EventEmitter, FocusHandle, IntoElement, div, prelude::*, rgb};

use crate::editor::*;
use crate::new_file_finder;

pub struct Workspace {
    pub editor: Entity<VimEditor>,
    pub file_finder: Option<Entity<new_file_finder::FileFinder>>,

    pub workspace_dir: PathBuf,

    next_focus: Option<FocusHandle>,
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

        cx.subscribe(&editor, |this, _editor, event, cx| match event {
            EditorEvent::OpenFinder => this.open_finder(cx),
        })
        .detach();

        Self {
            editor,
            file_finder: None,
            workspace_dir,
            next_focus: None,
        }
    }

    pub fn open_finder(&mut self, cx: &mut Context<Self>) {
        println!("Opening finder");
        // let finder = cx.new(|cx| FileFinder::new(cx, PathBuf::from("/home/maxi/")));
        let finder =
            cx.new(|cx| new_file_finder::FileFinder::new(cx, PathBuf::from("/home/maxi/")));

        cx.subscribe(&finder, |this, _finder, event, cx| match &event {
            new_file_finder::FinderEvent::Close => this.close_finder(cx),
            &new_file_finder::FinderEvent::Open(path_str) => {
                this.open_file(path_str, cx);
            }
        })
        .detach();

        self.file_finder = Some(finder.clone());

        self.next_focus = Some(finder.read(cx).searcher.focus_handle.clone());
        cx.notify();
    }

    pub fn open_file(&mut self, path: impl AsRef<std::path::Path>, cx: &mut Context<Self>) {
        let new_buffer = cx.new(|cx| Buffer::from_file(path).unwrap_or_else(|_| Buffer::new()));

        let new_editor = cx.new(|cx| VimEditor::new(cx, new_buffer));

        cx.subscribe(&new_editor, |this, _, event, cx| match event {
            EditorEvent::OpenFinder => this.open_finder(cx),
        })
        .detach();

        self.editor = new_editor;
        self.close_finder(cx);
    }

    fn close_finder(&mut self, cx: &mut Context<Self>) {
        self.file_finder = None;

        self.next_focus = Some(self.editor.read(cx).focus_handle.clone());
        cx.notify();
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

        if let Some(finder) = &self.file_finder {
            layout = layout.child(
                div()
                    .h_64()
                    .border_t_1()
                    .border_color(rgb(0x444444))
                    .child(finder.clone()),
            );
        }

        layout
    }
}
