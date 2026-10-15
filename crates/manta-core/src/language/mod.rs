use std::ops::Range;

use ropey::Rope;
use tree_sitter::InputEdit;

pub mod org;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrgToken {
    HeadlineStars,
    Headline,
    Text,
}

pub struct StyleSpan {
    pub byte_range: Range<usize>,
    pub style: OrgToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeadingID(pub usize);

#[derive(Debug, Clone)]
pub struct Heading {
    pub id: HeadingID,
    pub row: usize,
    pub level: usize,
    pub title: String,
    pub span: Range<usize>,
    pub fold_span: Range<usize>,
    pub fold_lines: Range<usize>,
}

pub trait LanguageService {
    /// Returns the language id
    fn language_id(&self) -> &'static str;

    /// Edit the tree and reparse
    fn on_edits(&mut self, edits: &[InputEdit], text: &Rope);

    /// Full (re)-parse. Ude this on file load
    fn reload(&mut self, text: &Rope) {
        self.on_edits(&[], text);
    }

    /// Returns a list of style spans (Eg. how headings should look)
    fn style_spans(&self, text: &Rope, line_range: Range<usize>) -> Vec<StyleSpan>;

    /// Returns a document outline (List of headings)
    fn outline(&self) -> Vec<Heading>;

    fn fold_lines(&self, headline_id: HeadingID) -> Option<Range<usize>>;

    /// Returns the next parent heading if any
    fn headline_at_line(&self, line: usize) -> Option<Heading>;
}
