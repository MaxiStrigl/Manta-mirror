use gpui::{
    AnyElement, EventEmitter, FocusHandle, KeyDownEvent, ScrollStrategy, UniformListScrollHandle,
    Window, div, prelude::*, rgb, uniform_list,
};
use nucleo::{Config, Matcher, Utf32Str, pattern::Pattern};
use std::sync::Arc;

#[derive(Clone)]
pub enum FuzzySearchEvent<T> {
    Select(T),
    Close,
    BackspaceOnEmptySearch,
    None,
}

impl<T: 'static + Clone> EventEmitter<FuzzySearchEvent<T>> for FuzzySearch<T> {}

#[derive(Clone)]
pub struct FuzzySearch<T: 'static + Clone> {
    search: String,
    prefix: String,
    all_items: Vec<T>,
    results: Vec<T>,
    selected_index: usize,
    pub focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    matcher: Matcher,

    render_item: Arc<dyn Fn(&T, bool) -> AnyElement>,
    get_text: Arc<dyn Fn(&T) -> String>,
}

impl<T: 'static + Clone> FuzzySearch<T> {
    pub fn new(
        focus_handle: FocusHandle,
        render_item: impl Fn(&T, bool) -> AnyElement + 'static,
        get_text: impl Fn(&T) -> String + 'static,
    ) -> Self {
        Self {
            search: String::new(),
            prefix: String::new(),
            all_items: Vec::new(),
            results: Vec::new(),
            selected_index: 0,
            focus_handle,
            scroll_handle: UniformListScrollHandle::new(),
            matcher: Matcher::new(Config::DEFAULT),

            render_item: Arc::new(render_item),
            get_text: Arc::new(get_text),
        }
    }

    pub fn set_all_items(&mut self, items: Vec<T>) {
        self.all_items = items;
        self.update_search();
    }

    pub fn set_prefix(&mut self, prefix: impl Into<String>) {
        self.prefix = prefix.into();
    }

    pub fn clear_search(&mut self) {
        self.search.clear();
        self.update_search();
    }

    pub fn update_search(&mut self) {
        if self.search.is_empty() {
            self.results = self.all_items.clone()
        } else {
            let pattern = Pattern::parse(
                &self.search,
                nucleo::pattern::CaseMatching::Smart,
                nucleo::pattern::Normalization::Smart,
            );

            let mut utf32_buf = Vec::new();

            let mut scored: Vec<(T, u32)> = self
                .all_items
                .iter()
                .filter_map(|item| {
                    let text: String = (self.get_text)(item);
                    let utf32_text = Utf32Str::new(&text, &mut utf32_buf);
                    let score = pattern.score(utf32_text, &mut self.matcher)?;

                    Some((item.clone(), score))
                })
                .collect();

            scored.sort_by(|a, b| b.1.cmp(&a.1));

            self.results = scored.into_iter().map(|(i, _)| i).collect();
        }

        self.selected_index = 0;
        self.scroll_handle
            .scroll_to_item(0, gpui::ScrollStrategy::Top);
    }

    pub fn handle_key_down(&mut self, event: &KeyDownEvent) -> FuzzySearchEvent<T> {
        let is_ctrl = event.keystroke.modifiers.control;

        let old_selected_idx = self.selected_index;
        let scroll_margin = 1;

        match event.keystroke.key.as_str() {
            "escape" => return FuzzySearchEvent::Close,
            "backspace" => {
                if self.search.pop().is_some() {
                    self.update_search();
                } else {
                    return FuzzySearchEvent::BackspaceOnEmptySearch;
                }
            }
            "enter" => {
                if let Some(selected) = self.results.get(self.selected_index) {
                    return FuzzySearchEvent::Select(selected.clone());
                }
            }
            "down" => {
                self.selected_index =
                    (self.selected_index + 1).min(self.results.len().saturating_sub(1))
            }
            "n" if is_ctrl => {
                self.selected_index =
                    (self.selected_index + 1).min(self.results.len().saturating_sub(1))
            }
            "up" => self.selected_index = self.selected_index.saturating_sub(1),
            "p" if is_ctrl => self.selected_index = self.selected_index.saturating_sub(1),

            _ => {
                if let Some(c) = &event.keystroke.key_char {
                    if !is_ctrl {
                        self.search.push_str(&c);
                        self.update_search();
                    }
                }
            }
        }

        let (strategy, scroll_idx) = if self.selected_index < old_selected_idx {
            (
                ScrollStrategy::Top,
                self.selected_index.saturating_sub(scroll_margin),
            )
        } else if self.selected_index > old_selected_idx {
            let max_idx = self.results.len().saturating_sub(1);
            (
                ScrollStrategy::Bottom,
                (self.selected_index + scroll_margin).min(max_idx),
            )
        } else {
            (ScrollStrategy::Top, self.selected_index)
        };

        self.scroll_handle.scroll_to_item(scroll_idx, strategy);
        FuzzySearchEvent::None
    }

    pub fn render<V: 'static>(
        &mut self,
        cx: &mut Context<V>,
        handle_key_down: impl Fn(&mut V, &KeyDownEvent, &mut Window, &mut Context<V>) + 'static,
    ) -> impl IntoElement {
        let results = self.results.clone();
        let selected_idx = self.selected_index;
        let render_item = self.render_item.clone();

        div()
            .id("fuzzy-search")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e1e))
            .border_1()
            .border_color(rgb(0x444444))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(handle_key_down))
            // search buffeer
            .child(
                div()
                    .p_2()
                    .border_b_1()
                    .border_color(rgb(0x333333))
                    .text_color(rgb(0xffffff))
                    .flex()
                    .flex_row()
                    .child(format!("{}{}", self.prefix, self.search)),
            )
            .child(
                uniform_list("results", results.len(), move |range, _, _| {
                    range
                        .map(|idx| render_item(&results[idx], idx == selected_idx))
                        .collect()
                })
                .track_scroll(self.scroll_handle.clone())
                .flex_1(),
            )
            .into_any_element()
    }
}
