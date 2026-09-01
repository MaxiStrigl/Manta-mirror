use std::path::PathBuf;

use gpui::AppContext;
use manta_api::{EditorAPI, MantaPlugin};

use crate::ui::FileFinder;

mod ui;

pub struct FileFinderPlugin;

impl<T: EditorAPI<T> + 'static> MantaPlugin<T> for FileFinderPlugin {
    fn id(&self) -> &'static str {
        "default-file-finder"
    }

    fn on_load(&self, api: &mut dyn manta_api::EditorAPI<T>) {
        api.register_command(
            "file-finder:open",
            "Open File Finder",
            Box::new(|api, cx| {
                let root_dir = api.workspace_dir();
                let file_finder = cx.new(|cx| FileFinder::new(cx, root_dir));

                cx.subscribe(
                    &file_finder,
                    |workspace: &mut T, _, event, cx| match event {
                        ui::FinderEvent::Close => workspace.close_panel(cx),
                        ui::FinderEvent::Open(path) => {
                            workspace.open_file(path, cx);
                            workspace.close_panel(cx);
                        }
                    },
                )
                .detach();
                let handle = file_finder.read(cx).focus_handle.clone();

                api.open_panel(file_finder.into(), Some(handle), cx);
            }),
        );
    }
}
