use std::path::PathBuf;

use file_finder::FileFinderPlugin;
use gpui::*;
use manta_api::EditorAPI;
use manta_gui::Workspace;
use typst::TypstPlugin;
use vim_command_bar::VimCommandBarPlugin;

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

                    let bar = Box::new(VimCommandBarPlugin);
                    workspace.load_plugin(bar, cx);

                    let typst = Box::new(TypstPlugin);
                    workspace.load_plugin(typst, cx);
                });

                workspace
            })
            .expect("Failed to open window");

        window
            .update(cx, |workspace, window, cx| {
                workspace.focus_main_panel(cx);
            })
            .unwrap();
    });
}
