use std::{collections::HashMap, ops::Range};

use ropey::Rope;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator, Tree};
use tree_sitter_org::generated::{self, AnyNode};

use crate::language::{
    Heading, HeadingID, LanguageService,
    OrgToken::{self},
    StyleSpan,
};

pub struct OrgLanguageService {
    pub tree: Option<Tree>,

    parser: Parser,
    query: Query,
    capture_styles: Vec<OrgToken>,
    source: String,
    headings: Vec<Heading>,
    by_byte: HashMap<usize, usize>,
}

impl OrgLanguageService {
    pub fn new() -> Self {
        let language = tree_sitter_org::language();
        let mut parser = Parser::new();
        parser
            .set_language(&language)
            .expect("Failed to setup treesitter language");

        let query = Query::new(&language, tree_sitter_org::HIGHLIGHTS_QUERY)
            .expect("Failed to create org query");

        let capture_styles: Vec<OrgToken> = query
            .capture_names()
            .iter()
            .map(|name| match *name {
                "OrgStars1" | "OrgStars2" | "OrgStars3" => OrgToken::HeadlineStars,
                "OrgHeadlineLevel1" | "OrgHeadlineLevel2" | "OrgHeadlineLevel3" => {
                    OrgToken::Headline
                }
                _ => OrgToken::Text,
            })
            .collect::<Vec<_>>();

        Self {
            parser,
            tree: None,
            query,
            capture_styles,
            source: String::new(),
            headings: Vec::new(),
            by_byte: HashMap::new(),
        }
    }

    pub fn reparse(&mut self, text: &Rope) {
        let source = text.to_string();
        let old = self.tree.take();
        self.tree = self.parser.parse(&source, old.as_ref());
        self.source = source;

        let mut headings = Vec::new();
        if let Some(tree) = &self.tree {
            self.collect_headlines(tree.root_node(), &mut headings);
        }

        self.by_byte = headings
            .iter()
            .enumerate()
            .map(|(i, h)| (h.span.start, i))
            .collect();
        self.headings = headings
    }

    fn collect_headlines<'t>(&self, node: tree_sitter::Node<'t>, out: &mut Vec<Heading>) {
        if let Some(heading) = self.headline_from_node(node) {
            out.push(heading);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_headlines(child, out);
        }
    }

    fn headline_from_node<'t>(&self, node: tree_sitter::Node<'t>) -> Option<Heading> {
        let any_node = generated::AnyNode::from_node(node, self.source.as_bytes());
        let generated::AnyNode::Headline(headline) = &any_node else {
            return None;
        };

        let start = headline.span.start_byte;
        let end = headline.span.end_byte;

        let stars_start = headline.stars.span.start_byte;
        let stars_end = headline.stars.span.end_byte;
        let stars_text = &self.source[stars_start..stars_end];
        let level = stars_text.chars().count();

        let span_node = match node.parent() {
            Some(parent)
                if matches!(
                    generated::AnyNode::from_node(parent, self.source.as_bytes()),
                    AnyNode::Section(_)
                ) =>
            {
                parent
            }
            _ => node,
        };

        let fold_start = span_node.start_position().row;
        let mut fold_end = span_node.end_position().row;

        if span_node.end_byte() == self.source.len() {
            fold_end += 1;
        }

        let title = self.source[start..end]
            .trim_start_matches('*')
            .trim()
            .to_string();

        Some(Heading {
            id: HeadingID(start), // Probably not perfect (insert line before)
            row: node.start_position().row,
            level: level,
            title: title,
            span: start..end,
            fold_span: span_node.start_byte()..span_node.end_byte(),
            fold_lines: fold_start..fold_end,
        })
    }
}

impl LanguageService for OrgLanguageService {
    fn language_id(&self) -> &'static str {
        "org"
    }

    fn on_edits(&mut self, edits: &[tree_sitter::InputEdit], text: &Rope) {
        if let Some(tree) = &mut self.tree {
            for edit in edits {
                tree.edit(edit);
            }
        }
        self.reparse(text);
    }

    fn style_spans(&self, text: &Rope, byte_range: Range<usize>) -> Vec<StyleSpan> {
        let mut spans = Vec::new();

        if let Some(tree) = &self.tree {
            let mut cursor = QueryCursor::new();
            cursor.set_byte_range(byte_range.clone());

            let matches =
                cursor.matches(&self.query, tree.root_node(), |node: tree_sitter::Node| {
                    text.byte_slice(node.byte_range()).chunks()
                });

            matches.for_each(|m| {
                for captrue in m.captures {
                    let style = self.capture_styles[captrue.index as usize];
                    let start = captrue
                        .node
                        .byte_range()
                        .start
                        .saturating_sub(byte_range.start);

                    let end = captrue
                        .node
                        .byte_range()
                        .end
                        .saturating_sub(byte_range.start);

                    spans.push(StyleSpan {
                        byte_range: start..end,
                        style,
                    });
                }
            });
        }
        spans
    }

    fn outline(&self) -> Vec<Heading> {
        self.headings.clone()
    }

    fn fold_lines(&self, headline_id: HeadingID) -> Option<Range<usize>> {
        let index = *self.by_byte.get(&headline_id.0)?;
        Some(self.headings[index].fold_lines.clone())
    }

    fn headline_at_line(&self, line: usize) -> Option<Heading> {
        self.headings
            .iter()
            .filter(|h| h.row <= line)
            .max_by_key(|h| h.row)
            .cloned()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use ropey::Rope;
//
//     #[test]
//     fn test_outline_extration() {
//         let content = "* Top level heading\nSome body text here.\n** subheading level 2\n*** One more subheading level 3\n* Second top level";
//         let rope = Rope::from_str(content);
//
//         let mut service = OrgLanguageService::new();
//         service.reparse(&rope);
//
//         let heading = service.outline();
//
//         assert_eq!(heading.len(), 4, "Should find exactly 4 headings");
//
//         assert_eq!(heading[0].level, 1);
//         assert_eq!(heading[0].title, "Top level heading");
//
//         assert_eq!(heading[1].level, 2);
//         assert_eq!(heading[1].title, "subheading level 2");
//
//         assert_eq!(heading[2].level, 3);
//         assert_eq!(heading[2].title, "One more subheading level 3");
//
//         assert_eq!(heading[3].level, 1);
//         assert_eq!(heading[3].title, "Second top level");
//     }

//     #[test]
//     fn test_fold_span_out_of_bounds() {
//         let content = "* Some Headline";
//         let rope = Rope::from_str(content);
//
//         let mut service = OrgLanguageService::new();
//         service.reparse(&rope);
//
//         assert!(service.fold_span(99).is_none());
//     }
//
//     #[test]
//     fn test_fold_span() {
//         let content =
//             "* Section one\nBody text for section one.\nMore body.\n* Section Two\nBody of two";
//         let rope = Rope::from_str(content);
//         let mut service = OrgLanguageService::new();
//         service.reparse(&rope);
//
//         let headings = service.outline();
//         assert_eq!(headings.len(), 2);
//
//         let heading1_start = headings[0].span.start;
//         let heading2_start = headings[1].span.start;
//
//         let fold_range_1 = service.fold_span(heading1_start);
//         assert!(
//             fold_range_1.is_some(),
//             "Fold span for section 1 should exist"
//         );
//
//         let range_1 = fold_range_1.unwrap();
//         let folded_text_1 = rope.byte_slice(range_1).to_string();
//
//         assert!(
//             folded_text_1.contains("* Section one"),
//             "Should include the headline"
//         );
//         assert!(
//             folded_text_1.contains("Body text for section one."),
//             "Should include the section"
//         );
//         assert!(
//             !folded_text_1.contains("* Section Two"),
//             "Should not include the next section"
//         );
//
//         let fold_range_2 = service.fold_span(heading2_start);
//
//         assert!(
//             fold_range_2.is_some(),
//             "Fold span for section 2 should exist"
//         );
//
//         let range_2 = fold_range_2.unwrap();
//         let folded_text_2 = rope.byte_slice(range_2).to_string();
//
//         assert!(
//             folded_text_2.contains("* Section Two"),
//             "Should include the headline"
//         );
//         assert!(
//             folded_text_2.contains("Body of two"),
//             "Should include the section"
//         );
//     }
// }
