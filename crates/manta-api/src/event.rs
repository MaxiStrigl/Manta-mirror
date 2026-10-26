use std::{cell::Cell, rc::Rc};

use crate::{Arg, command::CommandModeState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEvent {
    EdiotrStarted,
    CursorMoved {
        line: usize,
        byte: usize,
        char: usize,
    },
    BufferEdited,
    CommandModeChanged(CommandModeState),
    CommandExecuted {
        id: String,
        args: Vec<Arg>,
        result: Result<(), String>,
    },
}

pub struct EventSubscription {
    pub(crate) alive: Rc<Cell<bool>>,
}

impl EventSubscription {
    pub fn new(alive: Rc<Cell<bool>>) -> Self {
        Self { alive }
    }
    pub fn cancel(self) {
        self.alive.set(false);
    }
}

impl Drop for EventSubscription {
    fn drop(&mut self) {
        self.alive.set(false);
    }
}
