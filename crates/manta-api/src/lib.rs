mod command;
mod event;

use std::path::PathBuf;

use gpui::{AnyView, Context, FocusHandle};

pub use crate::event::{EditorEvent, EventSubscription};
pub use command::{
    Arg, CommandCompletion, CommandEffect, CommandKey, CommandMachine, CommandModeState,
    CommandSpec, CompletionKind, LineTarget, ParamKind, ParamSpec, ParamStyle,
};
pub use manta_config::theme::Theme;

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
    fn on_unload(&self, _api: &mut dyn EditorAPI<T>) {}

    fn on_theme_change(&self, new_theme: Theme, cx: &mut Context<T>);
}

pub trait EditorAPI<T> {
    // Commands
    fn register_command(
        &mut self,
        spec: CommandSpec,
        action: Box<dyn Fn(&mut dyn EditorAPI<T>, &[Arg], &mut Context<T>)>,
    );
    fn execute_command(
        &mut self,
        id: &str,
        args: &[Arg],
        cx: &mut Context<T>,
    ) -> Result<(), String>;

    fn open_command_mode(&mut self, cx: &mut Context<T>);

    // Events

    fn subscribe_event(
        &mut self,
        handler: Box<dyn Fn(&EditorEvent, &mut dyn EditorAPI<T>, &mut Context<T>)>,
        cx: &mut Context<T>,
    ) -> EventSubscription;

    // Views
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
    fn save_active_file(&mut self, cx: &mut Context<T>);
    fn add_inline_replacement(
        &mut self,
        replacement: InlineReplacement,
        cx: &mut Context<T>,
    ) -> usize;
    fn remove_inline_replacement(&mut self, id: usize, cx: &mut Context<T>);
    fn set_theme(&mut self, theme: Theme, cx: &mut Context<T>);

    fn get_cursor_byte_offset(&mut self, cx: &mut Context<T>) -> usize;
    fn get_cursor_char_position(&mut self, cx: &mut Context<T>) -> Option<usize>;
    fn get_cursor_line_position(&mut self, cx: &mut Context<T>) -> Option<usize>;
    fn get_buffer_text(&mut self, cx: &mut Context<T>) -> String;
    fn get_replacement_at_byte(
        &self,
        plugin_id: String,
        byte_offset: usize,
        cx: &mut Context<T>,
    ) -> Option<usize>;

    // --- Gather information ---

    fn workspace_dir(&self) -> PathBuf;
    fn get_available_commands(&self) -> Vec<CommandSpec>;
    fn get_buffer_path(&self, cx: &mut Context<T>) -> Option<PathBuf>;
    fn get_insert_mode_status(&self, cx: &mut Context<T>) -> bool;
    fn get_theme(&self) -> Theme;
}
