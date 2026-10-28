use std::{
    any::{Any, TypeId},
    cell::Cell,
    collections::HashMap,
    rc::Rc,
};

use gpui::Context;

use crate::{Arg, EditorAPI, command::CommandModeState};

pub trait EventPayload: Clone + 'static {}

#[derive(Debug, Clone)]
pub enum EditorEvent {
    Started(EditorStarted),
    CursorMoved(CursorMoved),
    BufferEdited(BufferEdited),
    CommandModeChanged(CommandModeChanged),
    CommandExecuted(CommandExecuted),
}

#[derive(Clone, Debug)]
pub struct EditorStarted;

impl EventPayload for EditorStarted {}

#[derive(Debug, Clone)]
pub struct CursorMoved {
    line: usize,
    byte: usize,
    char: usize,
}

impl EventPayload for CursorMoved {}

#[derive(Debug, Clone)]
pub struct BufferEdited;

impl EventPayload for BufferEdited {}

#[derive(Debug, Clone)]
pub struct CommandModeChanged(pub CommandModeState);

impl EventPayload for CommandModeChanged {}

#[derive(Debug, Clone)]
pub struct CommandExecuted {
    pub id: String,
    pub args: Vec<Arg>,
    pub result: Result<(), String>,
}

impl EventPayload for CommandExecuted {}

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

pub struct EventHub<T> {
    slots: HashMap<TypeId, Vec<EventSlot<T>>>,
    // plugin slots here
}

pub struct EventSlot<T> {
    alive: Rc<Cell<bool>>,
    handler: Box<dyn Fn(&dyn Any, &mut dyn EditorAPI<T>, &mut Context<T>)>,
}

impl<T> Default for EventHub<T> {
    fn default() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }
}

impl<T: 'static> EventHub<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(
        &mut self,
        id: TypeId,
        handler: Box<dyn Fn(&dyn Any, &mut dyn EditorAPI<T>, &mut Context<T>)>,
    ) -> EventSubscription {
        let alive = Rc::new(Cell::new(true));

        self.slots.entry(id).or_default().push(EventSlot {
            alive: alive.clone(),
            handler: handler,
        });
        EventSubscription { alive }
    }

    pub fn dispatch(
        &mut self,
        event: &EditorEvent,
        api: &mut dyn EditorAPI<T>,
        cx: &mut Context<T>,
    ) {
        self.prune();

        match event {
            EditorEvent::Started(editor_started) => {
                self.emit(
                    &TypeId::of::<EditorStarted>(),
                    editor_started as &dyn Any,
                    api,
                    cx,
                );
            }
            EditorEvent::CursorMoved(cursor_moved) => self.typed_dispatch(cursor_moved, api, cx),
            EditorEvent::BufferEdited(buffer_edited) => self.typed_dispatch(buffer_edited, api, cx),
            EditorEvent::CommandModeChanged(command_mode_changed) => {
                self.typed_dispatch(command_mode_changed, api, cx)
            }
            EditorEvent::CommandExecuted(command_executed) => {
                self.typed_dispatch(command_executed, api, cx)
            }
        }
    }

    fn emit(
        &mut self,
        id: &TypeId,
        payload: &dyn Any,
        api: &mut dyn EditorAPI<T>,
        cx: &mut Context<T>,
    ) {
        if let Some(slots) = self.slots.get_mut(id) {
            for slot in slots.iter().filter(|s| s.alive.get()) {
                (slot.handler)(payload, api, cx);
            }
        }
    }

    fn prune(&mut self) {
        self.slots.retain(|_, v| v.iter().any(|s| s.alive.get()));
    }
}
