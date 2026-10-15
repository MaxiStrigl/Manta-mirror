#[derive(Clone, Debug)]
pub struct BufferEdit {
    pub char_idx: usize,
    pub inserted_text: String,
    pub deleted_text: String,
}

impl BufferEdit {
    fn inverse(&self) -> Self {
        BufferEdit {
            char_idx: self.char_idx,
            inserted_text: self.deleted_text.clone(),
            deleted_text: self.inserted_text.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct History {
    undo_stack: Vec<Vec<BufferEdit>>,
    redo_stack: Vec<Vec<BufferEdit>>,
    current_edit: Vec<BufferEdit>,
}

impl History {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            current_edit: Vec::new(),
        }
    }

    pub fn push(&mut self, edit: BufferEdit) {
        self.current_edit.push(edit);
        self.redo_stack.clear();
    }

    pub fn commit(&mut self) {
        if !self.current_edit.is_empty() {
            let completed = std::mem::take(&mut self.current_edit);
            self.undo_stack.push(completed);
        }
    }

    pub fn undo(&mut self) -> Option<Vec<BufferEdit>> {
        self.commit();
        let batch = self.undo_stack.pop()?;
        self.redo_stack.push(batch.clone());

        Some(batch.iter().rev().map(|e| e.inverse()).collect())
    }

    pub fn redo(&mut self) -> Option<Vec<BufferEdit>> {
        let batch = self.redo_stack.pop()?;
        self.undo_stack.push(batch.clone());
        Some(batch)
    }
}
