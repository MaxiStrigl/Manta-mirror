use std::path::PathBuf;

use gpui::{AnyView, Context, FocusHandle};

pub trait MantaPlugin<T> {
    fn id(&self) -> &'static str;
    fn on_load(&self, api: &mut dyn EditorAPI<T>);
    fn on_unload(&self, api: &mut dyn EditorAPI<T>) {}
}

pub trait EditorAPI<T> {
    fn register_command(
        &mut self,
        name: &str,
        action: Box<dyn Fn(&mut dyn EditorAPI<T>, &mut Context<T>)>,
    );

    fn open_panel(&mut self, view: AnyView, focus: Option<FocusHandle>, cx: &mut Context<T>);
    fn close_panel(&mut self, cx: &mut Context<T>);
    fn open_file(&mut self, path: &str, cx: &mut Context<T>);

    // --- Gather information ---

    fn workspace_dir(&self) -> PathBuf;
}
