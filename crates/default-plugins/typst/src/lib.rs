use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    process::{Command, ExitStatus},
    sync::{Arc, Mutex},
};

use gpui::{
    AnyView, AppContext, IntoElement, ParentElement, Render, Styled, div, img, px, rgb, svg,
};
use manta_api::{InlineReplacement, MantaPlugin};

pub struct TypstPlugin;

pub const MOCK_TYPST_ID: &str = "typst";

impl<T: 'static> MantaPlugin<T> for TypstPlugin {
    fn id(&self) -> &'static str {
        MOCK_TYPST_ID
    }

    fn on_load(&self, api: &mut dyn manta_api::EditorAPI<T>, cx: &mut gpui::prelude::Context<T>) {
        api.register_command(
            "typst-mock:toggle",
            "Toggle the red square",
            Box::new(|api, cx| {
                let cursor = api.get_curosr_byte_offset(cx);

                if let Some(id) = api.get_replacement_at_byte(MOCK_TYPST_ID.to_string(), cursor, cx)
                {
                    api.remove_inline_replacement(id, cx);
                    return;
                }

                let text = api.get_buffer_text(cx);
                let mut start = None;

                let mut dollar_count = 0;
                for (i, c) in text[..cursor].char_indices() {
                    if c == '$' {
                        if i == 0 || text.as_bytes().get(i - 1) != Some(&b'\\') {
                            dollar_count += 1;
                            start = Some(i);
                        }
                    }
                }

                if dollar_count % 2 == 0 {
                    return;
                }

                let end = match text[cursor..].find('$') {
                    Some(x) => cursor + x,
                    None => return,
                };

                let formular = text[start.unwrap()..=end].to_string();

                let view: AnyView = cx.new(|cx| TypstFormularView::new(formular)).into();

                let replacement = InlineReplacement {
                    plugin_id: MOCK_TYPST_ID.to_string(),
                    start_byte: start.unwrap(),
                    end_byte: end,
                    replacement: view,
                };

                dbg!(&replacement);

                let id = api.add_inline_replacement(replacement, cx);
            }),
        );
    }
}

struct TypstFormularView {
    formular: String,
    svg_path: Option<String>,
}

impl TypstFormularView {
    fn new(formular: String) -> Self {
        let temp_dir = std::env::temp_dir();
        let typst_file = temp_dir.join("manta-typst-formular.typ");
        let svg_file = temp_dir.join("manta-typst-formular.svg");

        let content = format!(
            "#set page(width: auto, height: auto, margin: 0pt, fill: none)
#set text(fill: rgb(\"ffffff\"), size: 12pt) \n{}",
            formular
        );

        let mut svg_path = None;

        if fs::write(&typst_file, content).is_ok() {
            let status = Command::new("typst")
                .args([
                    "compile",
                    typst_file.to_str().unwrap(),
                    svg_file.to_str().unwrap(),
                ])
                .status();

            if let Ok(status) = status {
                if status.success() {
                    svg_path = Some(svg_file.to_string_lossy().to_string());
                }
            }
        }

        Self { formular, svg_path }
    }
}

impl Render for TypstFormularView {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::prelude::Context<Self>,
    ) -> impl IntoElement {
        if let Some(path) = self.svg_path.clone() {
            let p: PathBuf = PathBuf::from(path);
            dbg!(&p);
            div()
                .flex()
                .flex_row()
                .items_center()
                .child(img(p))
                .into_any_element()
        } else {
            div()
                .bg(rgb(0xffaaaa))
                .text_color(rgb(0x990000))
                .child(format!("Error rendering: {}", self.formular))
                .into_any_element()
        }
    }
}
