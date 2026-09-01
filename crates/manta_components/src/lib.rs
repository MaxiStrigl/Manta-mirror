use gpui::{
    AnyElement, EventEmitter, KeyDownEvent, UniformListScrollHandle, Window, div, prelude::*,
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
    items: Vec<T>,
    results: Vec<T>,
    selected_index: usize,
    scroll_handle: UniformListScrollHandle,

    matcher: Matcher,

    render_item: Arc<dyn Fn(&T, bool) -> AnyElement>,
    get_text: Arc<dyn Fn(&T) -> String>,
}

impl<T: 'static + Clone> FuzzySearch<T> {
    pub fn new(
        render_item: impl Fn(&T, bool) -> AnyElement + 'static,
        get_text: impl Fn(&T) -> String + 'static,
    ) -> Self {
        Self {
            items: Vec::new(),
            results: Vec::new(),
            selected_index: 0,
            scroll_handle: UniformListScrollHandle::new(),
            matcher: Matcher::new(Config::DEFAULT),

            render_item: Arc::new(render_item),
            get_text: Arc::new(get_text),
        }
    }

    pub fn set_all_items(&mut self, items: Vec<T>) {
        self.items = items.clone();
        self.results = items;
        self.selected_index = 0;
    }

    pub fn update_query(&mut self, query: &str) {
        if query.is_empty() {
            self.results = self.items.clone()
        } else {
            let pattern = Pattern::parse(
                query,
                nucleo::pattern::CaseMatching::Smart,
                nucleo::pattern::Normalization::Smart,
            );

            let mut utf32_buf = Vec::new();

            let mut scored: Vec<(T, u32)> = self
                .items
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

    pub fn handle_key_down(&mut self, event: &KeyDownEvent) -> Option<FuzzySearchEvent<T>> {
        let is_ctrl = event.keystroke.modifiers.control;

        match event.keystroke.key.as_str() {
            "enter" => {
                if let Some(selected) = self.results.get(self.selected_index) {
                    return Some(FuzzySearchEvent::Select(selected.clone()));
                }
            }
            "down" | "n" if is_ctrl => {
                self.selected_index =
                    (self.selected_index + 1).min(self.results.len().saturating_sub(1))
            }
            "up" | "p" if is_ctrl => self.selected_index = self.selected_index.saturating_sub(1),

            _ => (),
        };
        None
    }

    pub fn render_list(&mut self) -> impl IntoElement {
        let mut list = div().flex().flex_col().overflow_y_hidden();
        for (idx, element) in self.results.iter().enumerate() {
            let is_selected = idx == self.selected_index;
            list = list.child((self.render_item)(element, is_selected));
        }

        list
    }
}
