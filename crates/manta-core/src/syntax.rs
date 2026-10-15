use ropey::Rope;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator, Tree};
use tree_sitter_org;

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

pub struct OrgSyntax {
    pub tree: Option<Tree>,
    parser: Parser,
    query: Query,
    capture_styles: Vec<OrgToken>,
}

impl OrgSyntax {
    pub fn new() -> Self {
        let mut parser = Parser::new();
        let language = tree_sitter_org::language();

        parser.set_language(&language).unwrap();

        let query = Query::new(&language, tree_sitter_org::HIGHLIGHTS_QUERY).unwrap();

        let capture_styles: Vec<OrgToken> = query
            .capture_names()
            .iter()
            .map(|name| {
                dbg!(name);
                match *name {
                    "OrgStars1" | "OrgStars2" | "OrgStars3" => OrgToken::HeadlineStars,
                    "OrgHeadlineLevel1" | "OrgHeadlineLevel2" | "OrgHeadlineLevel3" => {
                        OrgToken::Headline
                    }
                    _ => OrgToken::Text,
                }
            })
            .collect::<Vec<_>>();

        dbg!(&capture_styles);

        Self {
            tree: None,
            parser,
            query,
            capture_styles,
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

                &chunk[offset_in_chunk..].as_bytes()
            },
            self.tree.as_ref(),
            None,
        );

        self.tree = new_tree;
    }

    pub fn get_highlights(
        &self,
        rope: &Rope,
        line_text: &str,
        line_start_byte: usize,
    ) -> Vec<StyleSpan> {
        let mut spans = Vec::new();
        let line_end_byte = line_start_byte + line_text.len();

        if let Some(tree) = &self.tree {
            let mut cursor = QueryCursor::new();
            cursor.set_byte_range(line_start_byte..line_end_byte);

            let matches =
                cursor.matches(&self.query, tree.root_node(), |node: tree_sitter::Node| {
                    let range = node.byte_range();
                    rope.byte_slice(range).chunks()
                });

            matches.for_each(|m| {
                for captrue in m.captures {
                    let style = self.capture_styles[captrue.index as usize];
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
