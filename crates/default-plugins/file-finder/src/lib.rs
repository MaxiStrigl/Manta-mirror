use std::sync::{Arc, Mutex};

use gpui::{AppContext, Context, WeakEntity};
use manta_api::{EditorAPI, MantaPlugin, Theme};

use crate::ui::FileFinder;

mod ui;

pub struct PluginState {
    active_finder: Option<WeakEntity<FileFinder>>,
    theme: Option<Arc<Theme>>,
}

pub struct FileFinderPlugin {
    state: Arc<Mutex<PluginState>>,
}

impl Default for FileFinderPlugin {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(PluginState {
                active_finder: None,
                theme: None,
            })),
        }
    }
}

impl<T: EditorAPI<T> + 'static> MantaPlugin<T> for FileFinderPlugin {
    fn id(&self) -> &'static str {
        "default-file-finder"
    }

    fn on_load(&self, api: &mut dyn manta_api::EditorAPI<T>, _cs: &mut Context<T>) {
        let theme = api.get_theme();
        let theme_mutex = Arc::new(theme);

        {
            let mut state = self.state.lock().unwrap();
            state.theme = Some(Arc::clone(&theme_mutex));
        }

        let state_clone = self.state.clone();
        let theme_for_command = Arc::clone(&theme_mutex);

        api.register_command(
            "file-finder:open",
            "Open File Finder",
            Box::new(move |api, cx| {
                let root_dir = api.workspace_dir();
                let finder_theme = Arc::clone(&theme_for_command);
                let file_finder = cx.new(|cx| FileFinder::new(cx, root_dir, finder_theme));

                {
                    let mut state = state_clone.lock().unwrap();
                    state.active_finder = Some(file_finder.downgrade());
                }

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

    fn on_theme_change(&self, new_theme: Theme, cx: &mut Context<T>) {
        let state = self.state.lock().unwrap();

        if let Some(finder) = &state.active_finder {
            let _ = finder.update(cx, |finder, cx| {
                finder.theme = Arc::new(new_theme);
                cx.notify();
            });
        }
    }
}
