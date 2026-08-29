use std::io::Error;

use ropey::Rope;
use tree_sitter::InputEdit;

use crate::{
    history::{BufferEdit, History},
    syntax::OrgSyntax,
};

pub struct Buffer {
    pub text: Rope,
    pub syntax: OrgSyntax,
    history: History,
}

impl Buffer {
    pub fn new() -> Buffer {
        let text = Rope::new();
        let history = History::new();
        let syntax = OrgSyntax::new();

        Self {
            text,
            syntax,
            history,
        }
    }

    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Buffer, Error> {
        let file = std::fs::File::open(path)?;
        let text = Rope::from_reader(file)?;
        let history = History::new();
        let syntax = OrgSyntax::new();

        Ok(Self {
            text,
            syntax,
            history,
        })
    }

    pub fn insert(&mut self, char_idx: usize, chunk: &str) {
        let edit = BufferEdit {
            char_idx: char_idx,
            deleted_text: String::new(),
            inserted_text: chunk.to_string(),
        };

        self.apply_edit_internal(&edit);

        self.history.push(edit);
    }

    pub fn delete(&mut self, start_idx: usize, end_idx: usize) {
        let deleted_text = self.text.slice(start_idx..end_idx).to_string();

        let edit = BufferEdit {
            char_idx: start_idx,
            inserted_text: String::new(),
            deleted_text,
        };

        self.apply_edit_internal(&edit);
        self.history.push(edit);
    }

    pub fn undo(&mut self) -> Option<usize> {
        let mut final_offset = None;

        if let Some(mut edits) = self.history.undo() {
            edits.reverse();
            for edit in edits {
                let inverted_edit = BufferEdit {
                    char_idx: edit.char_idx,
                    deleted_text: edit.inserted_text,
                    inserted_text: edit.deleted_text,
                };

                self.apply_edit_internal(&inverted_edit);

                final_offset = Some(edit.char_idx);
            }
        }

        final_offset
    }

    pub fn end_transaction(&mut self) {
        self.history.commit();
    }

    pub fn redo(&mut self) -> Option<usize> {
        let mut final_offset = None;
        if let Some(edits) = self.history.redo() {
            for edit in edits {
                self.apply_edit_internal(&edit);

                final_offset = Some(edit.char_idx + edit.inserted_text.chars().count())
            }
        }

        final_offset
    }

    fn apply_edit_internal(&mut self, edit: &BufferEdit) {
        let start_byte = self.text.char_to_byte(edit.char_idx);
        let start_position = rope_offset_to_point(&self.text, edit.char_idx);

        let old_end_byte = start_byte + edit.deleted_text.len();
        let old_end_position = rope_offset_to_point(
            &self.text,
            edit.char_idx + edit.deleted_text.chars().count(),
        );

        if !edit.deleted_text.is_empty() {
            let end_char = edit.char_idx + edit.deleted_text.chars().count();
            self.text.remove(edit.char_idx..end_char);
        }

        if !edit.inserted_text.is_empty() {
            self.text.insert(edit.char_idx, &edit.inserted_text);
        }

        let new_end_byte = start_byte + edit.inserted_text.len();
        let new_end_position = rope_offset_to_point(
            &self.text,
            edit.char_idx + edit.inserted_text.chars().count(),
        );

        let ts_edit = InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position,
            old_end_position,
            new_end_position,
        };

        if let Some(tree) = &mut self.syntax.tree {
            tree.edit(&ts_edit);
        }

        self.syntax.parse_rope(&self.text);
    }
}

fn rope_offset_to_point(rope: &Rope, char_offset: usize) -> tree_sitter::Point {
    let line = rope.char_to_line(char_offset);
    let line_start_char = rope.line_to_char(line);
    let col = char_offset - line_start_char;

    let byte_col = rope.char_to_byte(char_offset) - rope.char_to_byte(col);

    tree_sitter::Point::new(line, byte_col)
}
