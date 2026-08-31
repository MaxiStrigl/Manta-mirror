use std::path::PathBuf;

use file_finder::FileFinderPlugin;
use gpui::*;
use manta_gui::Workspace;

fn main() {
    let args: Vec<_> = std::env::args().collect();

    let mut initial_file: Option<PathBuf> = None;
    let mut workspace_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    if args.len() > 1 {
        let path = PathBuf::from(&args[1]);

        if path.is_dir() {
            workspace_dir = path;
        } else if path.is_file() {
            initial_file = Some(path.clone());

            if let Some(parent) = path.parent() {
                workspace_dir = parent.to_path_buf();
            }
        }
    }

    let app = Application::new();

    app.run(|cx: &mut App| {
        let options = WindowOptions {
            ..Default::default()
        };

        let window = cx
            .open_window(options, |_window, cx| {
                let workspace = cx.new(|cx| Workspace::new(cx, workspace_dir, initial_file));

                workspace.update(cx, |workspace, cx| {
                    let finder = Box::new(FileFinderPlugin);

                    workspace.load_plugin(finder, cx);
                });

                workspace
            })
            .expect("Failed to open window");

        window
            .update(cx, |workspace, window, cx| {
                cx.focus_view(&workspace.editor, window);
            })
            .unwrap();
    });
}
