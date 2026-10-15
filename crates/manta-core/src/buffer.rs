use std::{
    fs::File,
    io::{BufWriter, Error},
    path::PathBuf,
};

use ropey::Rope;
use tree_sitter::InputEdit;

use crate::{
    history::{BufferEdit, History},
    language::{self, LanguageService},
};

pub struct Buffer {
    pub text: Rope,
    pub syntax: Box<dyn LanguageService>,
    history: History,
    pub path: Option<PathBuf>,
}

impl Buffer {
    pub fn new() -> Buffer {
        Self {
            text: Rope::new(),
            syntax: Box::new(language::org::OrgLanguageService::new()),
            history: History::new(),
            path: None,
        }
    }

    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Buffer, Error> {
        let mut buffer = Buffer::new();

        buffer.text = Rope::from_str(&std::fs::read_to_string(&path)?);
        buffer.path = Some(path.as_ref().to_path_buf());
        buffer.syntax.reload(&buffer.text);

        Ok(buffer)
    }

    /// One Atomic change
    /// Record the edit in History and apply
    pub fn edit(&mut self, edit: BufferEdit) {
        self.history.push(edit.clone());
        self.apply_edits(&[edit])
    }

    pub fn insert(&mut self, char_idx: usize, chunk: &str) {
        self.edit(BufferEdit {
            char_idx,
            inserted_text: chunk.to_string(),
            deleted_text: String::new(),
        });
    }

    pub fn delete(&mut self, start_idx: usize, end_idx: usize) {
        let deleted_text = self.text.slice(start_idx..end_idx).to_string();
        self.edit(BufferEdit {
            char_idx: start_idx,
            inserted_text: String::new(),
            deleted_text,
        });
    }

    pub fn undo(&mut self) -> Option<usize> {
        let batch = self.history.undo()?;
        let offset = batch.last().map(|e| e.char_idx);
        self.apply_edits(&batch);
        offset
    }

    pub fn redo(&mut self) -> Option<usize> {
        let batch = self.history.redo()?;
        let cursor_offset = batch.last().map(|e| e.char_idx);
        self.apply_edits(&batch);
        cursor_offset
    }

    pub fn end_transaction(&mut self) {
        self.history.commit();
    }

    fn apply_edits(&mut self, edits: &[BufferEdit]) {
        let mut ts_edits = Vec::with_capacity(edits.len());
        for edit in edits {
            ts_edits.push(self.apply_edit_internal(edit));
        }
        self.syntax.on_edits(&ts_edits, &self.text);
    }

    pub fn save(&mut self) -> Result<(), std::io::Error> {
        if let Some(path) = &self.path {
            let file = File::create(path)?;
            let mut writer = BufWriter::new(file);

            self.text.write_to(&mut writer)?;

            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No file path stred in this buffer",
            ))
        }
    }

    fn apply_edit_internal(&mut self, edit: &BufferEdit) -> InputEdit {
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

        InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position,
            old_end_position,
            new_end_position,
        }
    }
}

fn rope_offset_to_point(rope: &Rope, char_offset: usize) -> tree_sitter::Point {
    let line = rope.char_to_line(char_offset);
    let line_start_char = rope.line_to_char(line);
    let col = char_offset - line_start_char;

    let byte_col = rope.char_to_byte(char_offset) - rope.char_to_byte(col);

    tree_sitter::Point::new(line, byte_col)
}
