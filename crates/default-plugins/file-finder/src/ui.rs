use std::path::PathBuf;

use gpui::{Context, EventEmitter, IntoElement, Window, div, rgb};
use gpui::{KeyDownEvent, prelude::*};

use manta_components::{FuzzySearch, FuzzySearchEvent};

#[derive(Clone)]
pub enum FinderEvent {
    Close,
    Open(String),
}

#[derive(Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

#[derive(Clone)]
pub struct FileFinder {
    pub searcher: FuzzySearch<FileEntry>,
    current_dir: PathBuf,
}

impl EventEmitter<FinderEvent> for FileFinder {}

impl FileFinder {
    pub fn new(cx: &mut Context<Self>, rootdir: PathBuf) -> FileFinder {
        let searcher = FuzzySearch::new(
            cx.focus_handle(),
            |entry: &FileEntry, is_selected: bool| {
                let display = if entry.is_dir {
                    format!("{}/", entry.name)
                } else {
                    entry.name.clone()
                };

                let color = if is_selected {
                    rgb(0xffffff)
                } else if entry.is_dir {
                    rgb(0x61afef)
                } else {
                    rgb(0xcccccc)
                };

                div()
                    .px_2()
                    .py_1()
                    .bg(if is_selected {
                        rgb(0x007acc)
                    } else {
                        rgb(0x1e1e1e)
                    })
                    .text_color(color)
                    .child(display)
                    .into_any_element()
            },
            |entry: &FileEntry| entry.name.clone(),
        );

        let mut finder = FileFinder {
            searcher,
            current_dir: rootdir.clone(),
        };
        finder.load_dir(cx, rootdir);

        finder
    }

    fn load_dir(&mut self, cx: &mut Context<Self>, path: PathBuf) {
        self.current_dir = path.clone();
        let display_path = format!("{}/", path.to_string_lossy().to_string());

        let fetch_task = cx.background_executor().spawn(async move {
            let mut entries = Vec::new();

            if path.parent().is_some() {
                entries.push(FileEntry {
                    name: "..".to_string(),
                    path: path.parent().unwrap().to_path_buf(),
                    is_dir: true,
                });
            }

            if let Ok(dir) = std::fs::read_dir(path) {
                dir.filter_map(|d| d.ok()).for_each(|e| {
                    let name = e.file_name().to_string_lossy().to_string();

                    if !name.starts_with(".") {
                        entries.push(FileEntry {
                            name,
                            path: e.path(),
                            is_dir: e.file_type().map_or(false, |t| t.is_dir()),
                        });
                    }
                });
            }
            entries.sort_by(|a, b| {
                if a.name == ".." {
                    std::cmp::Ordering::Less
                } else if b.name == ".." {
                    std::cmp::Ordering::Greater
                } else {
                    b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name))
                }
            });

            entries
        });

        cx.spawn(async move |this, cx| {
            let entries = fetch_task.await;

            let _ = cx.update(|app| {
                this.update(app, |this, cx| {
                    this.searcher.clear_search();
                    this.searcher.set_all_items(entries);
                    this.searcher.set_prefix(display_path);
                    cx.notify();
                })
            });
        })
        .detach();
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = self.searcher.handle_key_down(event);

        match action {
            FuzzySearchEvent::Select(entry) => {
                if entry.is_dir {
                    self.load_dir(cx, entry.path);
                } else {
                    cx.emit(FinderEvent::Open(entry.path.to_string_lossy().to_string()));
                }
            }
            FuzzySearchEvent::Close => cx.emit(FinderEvent::Close),
            FuzzySearchEvent::BackspaceOnEmptySearch => {
                if let Some(parent) = self.current_dir.parent() {
                    let path = parent.to_path_buf();
                    self.load_dir(cx, path);
                }
            }
            FuzzySearchEvent::None => {}
        }

        cx.notify();
    }
}

impl Render for FileFinder {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.searcher.render(cx, Self::handle_key_down)
    }
}
