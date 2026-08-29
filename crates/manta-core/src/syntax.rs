use ropey::Rope;
use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator, Tree};

unsafe extern "C" {
    fn tree_sitter_org() -> *const std::ffi::c_void;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrgToken {
    HeadlineStars,
    Headline,
    Text,
}

pub struct StyleSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub style: OrgToken,
}

pub fn get_org_language() -> Language {
    unsafe { Language::from_raw(tree_sitter_org() as *const _) }
}

pub struct OrgSyntax {
    pub tree: Option<Tree>,
    parser: Parser,
    query: Query,
}

impl OrgSyntax {
    pub fn new() -> Self {
        let mut parser = Parser::new();
        let language = get_org_language();

        parser.set_language(&language).unwrap();

        let query_src = r#"
            (headline stars: (stars) @heading.stars
             ) @heading"#;

        let query = Query::new(&language, query_src).unwrap();

        Self {
            tree: None,
            parser,
            query,
        }
    }

    pub fn parse_rope(&mut self, text: &Rope) {
        let new_tree = self.parser.parse_with_options(
            &mut move |byte_offset, _| {
                if byte_offset > text.bytes().len() {
                    return &[][..];
                }

                let (chunk, byte_idx, _, _) = text.chunk_at_byte(byte_offset);

                let offset_in_chunk = byte_offset - byte_idx;

                chunk[offset_in_chunk..].as_bytes()
            },
            self.tree.as_ref(),
            None,
        );

        self.tree = new_tree;
    }

    pub fn get_highlights(&self, text: &str, line_start_byte: usize) -> Vec<StyleSpan> {
        let mut spans = Vec::new();

        if let Some(tree) = &self.tree {
            let mut cursor = QueryCursor::new();

            cursor.set_byte_range(line_start_byte..line_start_byte + text.len());

            let matches = cursor.matches(&self.query, tree.root_node(), "".as_bytes());

            matches.for_each(|m| {
                for captrue in m.captures {
                    let name = self.query.capture_names()[captrue.index as usize].to_string();
                    let style = match name.as_str() {
                        "headline.stars" => OrgToken::HeadlineStars,
                        "heading" => OrgToken::Headline,
                        _ => OrgToken::Text,
                    };

                    let start = captrue
                        .node
                        .byte_range()
                        .start
                        .saturating_sub(line_start_byte);

                    let end = captrue
                        .node
                        .byte_range()
                        .end
                        .saturating_sub(line_start_byte);

                    spans.push(StyleSpan {
                        start_byte: start,
                        end_byte: end,
                        style,
                    });
                }
            });
        }

        spans
    }
}
