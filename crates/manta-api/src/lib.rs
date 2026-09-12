use std::path::PathBuf;

use gpui::{AnyView, Context, FocusHandle};

#[derive(Debug, Clone)]
pub struct CommandInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct InlineReplacement {
    pub plugin_id: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub replacement: AnyView,
}

pub trait MantaPlugin<T> {
    fn id(&self) -> &'static str;
    fn on_load(&self, api: &mut dyn EditorAPI<T>, cx: &mut Context<T>);
    fn on_unload(&self, api: &mut dyn EditorAPI<T>) {}
}

pub trait EditorAPI<T> {
    fn register_command(
        &mut self,
        name: &str,
        desciption: &str,
        action: Box<dyn Fn(&mut dyn EditorAPI<T>, &mut Context<T>)>,
    );

    fn focus_main_panel(&mut self, cx: &mut Context<T>);
    fn open_panel(&mut self, view: AnyView, focus: Option<FocusHandle>, cx: &mut Context<T>);
    fn close_panel(&mut self, cx: &mut Context<T>);
    fn open_file(&mut self, path: &str, cx: &mut Context<T>);
    fn open_custom_panel(&mut self, view: AnyView, focus: Option<FocusHandle>, cx: &mut Context<T>);

    fn set_bottom_bar(
        &mut self,
        view: Option<AnyView>,
        focus_handle: Option<FocusHandle>,
        cx: &mut Context<T>,
    );
    fn execute_command(&mut self, name: &str, cx: &mut Context<T>) -> Result<(), String>;
    fn save_active_file(&mut self, cx: &mut Context<T>);
    fn add_inline_replacement(
        &mut self,
        replacement: InlineReplacement,
        cx: &mut Context<T>,
    ) -> usize;
    fn remove_inline_replacement(&mut self, id: usize, cx: &mut Context<T>);

    fn get_curosr_byte_offset(&mut self, cx: &mut Context<T>) -> usize;
    fn get_buffer_text(&mut self, cx: &mut Context<T>) -> String;
    fn get_replacement_at_byte(
        &self,
        plugin_id: String,
        byte_offset: usize,
        cx: &mut Context<T>,
    ) -> Option<usize>;

    // --- Gather information ---

    fn workspace_dir(&self) -> PathBuf;
    fn get_available_commands(&self) -> Vec<CommandInfo>;
}
