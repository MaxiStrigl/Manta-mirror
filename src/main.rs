use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

use gpui::*;
use modalkit::actions::{Action, EditAction, EditorAction, HistoryAction, InsertTextAction};
use modalkit::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use modalkit::editing::context::EditContext;
use modalkit::env::vim::VimMode;
use modalkit::env::vim::keybindings::VimMachine;
use modalkit::keybindings::BindingMachine;
use modalkit::prelude::*;
use modalkit::prelude::{RepeatType, Specifier};
use modalkit::{
    editing::{application::EmptyInfo, key::KeyManager},
    env::vim::{VimState, keybindings::default_vim_keys},
    key::TerminalKey,
};
use ropey::Rope;
use tree_sitter::{InputEdit, Language, Parser, Query, StreamingIterator};

unsafe extern "C" {
    fn tree_sitter_org() -> *const std::ffi::c_void;
}

#[derive(Clone, Debug)]
pub enum EditKind {
    Insert,
    Delete,
}

#[derive(Clone, Debug)]
pub struct BufferEdit {
    offset: usize,
    text: String,
    kind: EditKind,
}

#[derive(Clone, Debug)]
struct History {
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
}

pub struct Buffer {
    text: Rope,
    history: History,
    query: Arc<Query>,

    parser: Parser,
    tree: Option<tree_sitter::Tree>,
}

impl Buffer {
    fn new(_cx: &mut Context<Self>) -> Buffer {
        let text = Rope::new();
        let history = History::new();

        let org_language = unsafe { Language::from_raw(tree_sitter_org() as *const _) };
        let mut parser = Parser::new();
        parser
            .set_language(&org_language)
            .expect("Failed to load org langauge");

        let tree = parse_rope(&mut parser, None, &text);

        let query_src = r#"
            (headline 
             stars: (stars) @heading.stars
             ) @heading"#;

        let query =
            Arc::new(Query::new(&org_language, query_src).expect("Failed to setup org query"));

        Self {
            text,
            history,
            query,
            parser,
            tree,
        }
    }

    fn insert_internal(&mut self, offset: usize, chunk: &str) {
        let start_byte = self.text.char_to_byte(offset);
        let start_position = Self::rope_offset_to_point(&self.text, offset);

        self.text.insert(offset, chunk);

        let new_end_byte = self.text.char_to_byte(offset + chunk.chars().count());
        let new_end_position =
            Self::rope_offset_to_point(&self.text, offset + chunk.chars().count());

        if let Some(tree) = &mut self.tree {
            tree.edit(&InputEdit {
                start_byte,
                old_end_byte: start_byte,
                new_end_byte,
                start_position,
                old_end_position: start_position,
                new_end_position,
            });
        }

        self.tree = parse_rope(&mut self.parser, self.tree.as_ref(), &self.text);
    }

    pub fn delete_internal(&mut self, range: Range<usize>) {
        let start_byte = self.text.char_to_byte(range.start);
        let start_position = Self::rope_offset_to_point(&self.text, range.start);
        let old_end_byte = self.text.char_to_byte(range.end);
        let old_end_position = Self::rope_offset_to_point(&self.text, range.end);

        self.text.remove(range);

        let new_end_byte = start_byte;
        let new_end_position = start_position;

        if let Some(tree) = &mut self.tree {
            tree.edit(&InputEdit {
                start_byte,
                old_end_byte,
                new_end_byte,
                start_position,
                old_end_position,
                new_end_position,
            });
        }

        self.tree = parse_rope(&mut self.parser, self.tree.as_ref(), &self.text)
    }

    pub fn insert_text(&mut self, offset: usize, chunk: &str, cx: &mut Context<Self>) {
        self.history.current_edit.push(BufferEdit {
            offset: offset,
            text: chunk.to_string(),
            kind: EditKind::Insert,
        });

        if !self.history.redo_stack.is_empty() {
            self.history.redo_stack.clear();
        }

        self.insert_internal(offset, chunk);
        cx.notify();
    }

    pub fn delete_text(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        if range.end > self.text.len_chars() || range.start >= range.end {
            return;
        }

        let deleted_text = self.text.slice(range.clone()).to_string();
        self.history.current_edit.push(BufferEdit {
            offset: range.start,
            text: deleted_text,
            kind: EditKind::Delete,
        });
        self.history.redo_stack.clear();

        self.delete_internal(range);
        cx.notify();
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) -> Option<usize> {
        let edits = self.history.undo_stack.pop()?;
        let mut final_offset = None;

        for e in edits.iter().rev() {
            match e.kind {
                EditKind::Insert => {
                    self.delete_internal(e.offset..e.offset + e.text.chars().count());
                    final_offset = Some(e.offset)
                }
                EditKind::Delete => {
                    self.insert_internal(e.offset, &e.text);
                    final_offset = Some(e.offset)
                }
            }
        }

        self.history.redo_stack.push(edits);
        cx.notify();
        final_offset
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) -> Option<usize> {
        let edits = self.history.redo_stack.pop()?;
        let mut final_offset = None;

        for e in edits.iter() {
            match e.kind {
                EditKind::Delete => {
                    self.delete_internal(e.offset..e.offset + e.text.chars().count());
                    final_offset = Some(e.offset + e.text.chars().count());
                }
                EditKind::Insert => {
                    self.insert_internal(e.offset, &e.text);
                    final_offset = Some(e.offset + e.text.chars().count());
                }
            }
        }
        self.history.undo_stack.push(edits);
        cx.notify();
        final_offset
    }

    fn end_edit_group(&mut self) {
        if !self.history.current_edit.is_empty() {
            self.history
                .undo_stack
                .push(self.history.current_edit.clone());
            self.history.current_edit.clear();
        }
    }

    fn rope_offset_to_point(rope: &Rope, char_offset: usize) -> tree_sitter::Point {
        let line = rope.char_to_line(char_offset);
        let line_start_char = rope.line_to_char(line);
        let col = char_offset - line_start_char;

        let byte_col = rope.char_to_byte(char_offset) - rope.char_to_byte(col);

        tree_sitter::Point::new(line, byte_col)
    }
}

struct VimEditor {
    buffer: Entity<Buffer>,
    _buffer_subscription: Subscription,

    cursor_offset: usize,
    folded_ranges: Vec<Range<usize>>,
    line_map: Vec<usize>,

    cursor_visible: bool,
    is_insert_mode: bool,
    _blink_task: Task<()>,
    target_col: usize,
    last_activity: Instant,

    vim_machine: VimMachine<TerminalKey, EmptyInfo>,

    scroll_handle: UniformListScrollHandle,
    focus_handle: FocusHandle,
}

impl VimEditor {
    pub fn new(cx: &mut Context<Self>, buffer: Entity<Buffer>) -> Self {
        let _buffer_subscription = cx.observe(&buffer, |_, _, cx| {
            cx.notify();
        });

        let vim_binding = default_vim_keys::<EmptyInfo>();

        let vim_machine = default_vim_keys();
        let folded_ranges = Vec::new();
        let line_map = Vec::new();

        let _blink_task = cx.spawn(async move |this, cx| {
            let blink_interval = std::time::Duration::from_millis(500);
            loop {
                let check = this.update(cx, |editor, _cx| editor.last_activity);

                let last_activity = match check {
                    Ok(n) => n,
                    Err(_) => {
                        println!("Last activity failed.");
                        break;
                    }
                };

                let elapsed = last_activity.elapsed();

                if elapsed < blink_interval {
                    let result = this.update(cx, |editor, cx| {
                        if !editor.cursor_visible {
                            editor.cursor_visible = true;
                            cx.notify();
                        }
                    });

                    if result.is_err() {
                        break;
                    }

                    cx.background_executor()
                        .timer(blink_interval - elapsed)
                        .await;
                } else {
                    let result = this.update(cx, |editor, cx| {
                        editor.cursor_visible = !editor.cursor_visible;
                        cx.notify();
                    });

                    if result.is_err() {
                        break;
                    }

                    cx.background_executor().timer(blink_interval).await;
                }
            }
        });

        let scroll_handle = UniformListScrollHandle::new();

        Self {
            buffer,
            _buffer_subscription,
            cursor_offset: 0,
            target_col: 0,
            scroll_handle,
            focus_handle: cx.focus_handle(),
            vim_machine,
            last_activity: Instant::now(),
            cursor_visible: true,
            is_insert_mode: false,
            _blink_task,
            line_map,
            folded_ranges,
        }
    }

    fn rebuild_line_map(&mut self, cx: &mut Context<Self>) {
        self.line_map.clear();
        let buffer = self.buffer.read(cx);
        let total_lines = buffer.text.len_lines();
        let mut current_line = 0;

        while current_line < total_lines {
            for fold in &self.folded_ranges {
                if fold.contains(&current_line) {
                    current_line = fold.end;
                    break;
                }

                self.line_map.push(current_line);
                current_line += 1;
            }
        }
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cursor_visible = true;

        let old_line_idx = self.buffer.read(cx).text.char_to_line(self.cursor_offset);

        let terminal_key = Self::translate_key(event);
        self.vim_machine.input_key(terminal_key);

        while let Some((action, context)) = self.vim_machine.pop() {
            self.execute_action(action, context, cx);
        }

        if self.vim_machine.mode() == VimMode::Normal {
            self.buffer.update(cx, |buf, _| buf.end_edit_group());
        }

        self.is_insert_mode = self.vim_machine.mode() == VimMode::Insert;

        let new_line_idx = self.buffer.read(cx).text.char_to_line(self.cursor_offset);
        let strategy = if new_line_idx < old_line_idx {
            ScrollStrategy::Top
        } else {
            ScrollStrategy::Bottom
        };

        self.scroll_handle.scroll_to_item(new_line_idx, strategy);
        self.last_activity = Instant::now();

        cx.notify();
    }

    fn execute_action(
        &mut self,
        action: Action<EmptyInfo>,
        ctx: EditContext,
        cx: &mut Context<Self>,
    ) {
        let text = self.buffer.read(cx).text.clone();
        match action {
            // 1. Text Insertion (Typing in Insert mode)
            // Modalkit emits characters or strings directly as EditorActions.
            Action::Editor(EditorAction::InsertText(insert_action)) => match insert_action {
                InsertTextAction::Type(Specifier::Exact(c), _dir, _count) => {
                    match c {
                        // 1. Standard character typing
                        Char::Single(ch) => {
                            self.buffer.update(cx, |buf, cx| {
                                buf.insert_text(self.cursor_offset, &ch.to_string(), cx);
                            });

                            let text = self.buffer.read(cx).text.clone();

                            self.cursor_offset += 1;
                            let current_line_idx = text.char_to_line(self.cursor_offset);
                            let current_line_start = text.line_to_char(current_line_idx);
                            self.target_col = self.cursor_offset - current_line_start;
                        }

                        // 2. Vim Digraphs (e.g., typing two chars to make a special symbol)
                        Char::Digraph(c1, c2) => {
                            // For now, just push both. Later you'd look them up in a Digraph table.

                            let concat: String = [c1, c2].iter().collect();

                            self.buffer.update(cx, |buf, cx| {
                                buf.insert_text(self.cursor_offset, &concat, cx);
                            });

                            let text = self.buffer.read(cx).text.clone();

                            self.cursor_offset += 2;
                            let current_line_idx = text.char_to_line(self.cursor_offset);
                            let current_line_start = text.line_to_char(current_line_idx);
                            self.target_col = self.cursor_offset - current_line_start;
                        }

                        // 3. Control Sequences
                        Char::CtrlSeq(seq) => {
                            println!("CtrlSeq insertion not implemented: {}", seq);
                        }

                        // 4. Copying from the line above/below (Ctrl-Y / Ctrl-E in Vim)
                        Char::CopyLine(dir) => {
                            println!(
                                "CopyLine insertion not implemented for direction: {:?}",
                                dir
                            );
                        }
                    }
                }
                InsertTextAction::Type(Specifier::Contextual, _, _) => {
                    println!("Contextual Type action not implemented");
                }

                InsertTextAction::Transcribe(text, _dir, _count) => {
                    println!("Transcribe Type action not implemented");
                }

                InsertTextAction::Paste(style, count) => {
                    if let Some(clipboard_item) = cx.read_from_clipboard() {
                        let text_to_insert = clipboard_item.text().unwrap_or_default();

                        if text_to_insert.is_empty() {
                            return;
                        }

                        let multiplier = match count {
                            Count::Exact(n) => n,
                            Count::Contextual => ctx.get_count().unwrap_or(1),
                            _ => 1,
                        };

                        let full_text = text_to_insert.repeat(multiplier);

                        let offset = match style {
                            PasteStyle::Side(dir) => match dir {
                                MoveDir1D::Previous => self.cursor_offset,
                                MoveDir1D::Next => (self.cursor_offset + 1)
                                    .min(self.buffer.read(cx).text.len_chars()),
                            },
                            _ => {
                                println!("Paste style not yet implmeented: {:?}", style);
                                return;
                            }
                        };

                        self.buffer
                            .update(cx, |buf, cx| buf.insert_text(offset, &full_text, cx));

                        self.cursor_offset = offset + full_text.chars().count() + 1;
                    }
                }

                _ => {
                    println!("Unhandled InsertTextAction: {:?}", insert_action);
                }
            },

            // 2. Text Editing (Delete, Yank, Change, etc.)
            // These are nested under `Edit` and wrapped in a `Specifier` to denote
            // whether the action was explicit (Exact) or implied (Contextual).
            Action::Editor(EditorAction::Edit(specifier, target)) => {
                match (specifier.clone(), target.clone()) {
                    (Specifier::Contextual, EditTarget::Motion(move_type, count))
                    | (
                        Specifier::Exact(EditAction::Motion),
                        EditTarget::Motion(move_type, count),
                    ) => {
                        let multiplier = match count {
                            Count::Exact(n) => n,
                            Count::Contextual => ctx.get_count().unwrap_or(1),
                            _ => 1,
                        };

                        match move_type {
                            MoveType::Column(dir, _) => {
                                let current_line_idx = text.char_to_line(self.cursor_offset);
                                let line_start = text.line_to_char(current_line_idx);
                                let line = text.line(current_line_idx);

                                let mut line_len = line.len_chars();

                                if line_len > 0 && line.char(line_len - 1) == '\n' {
                                    line_len = line_len.saturating_sub(1);
                                }

                                let line_end = line_start + line_len;

                                match dir {
                                    MoveDir1D::Previous => {
                                        self.cursor_offset = self
                                            .cursor_offset
                                            .saturating_sub(multiplier)
                                            .max(line_start);
                                    }
                                    MoveDir1D::Next => {
                                        self.cursor_offset =
                                            (self.cursor_offset + multiplier).min(line_end);
                                    }
                                }

                                self.target_col = self.cursor_offset - line_start;
                            }
                            MoveType::Line(MoveDir1D::Next) => {
                                self.move_cursor_vertical(multiplier as isize, cx);
                            }
                            MoveType::Line(MoveDir1D::Previous) => {
                                self.move_cursor_vertical((multiplier as isize) * -1, cx);
                            }
                            MoveType::LinePos(MovePosition::Beginning) => {
                                let line_idx = text.char_to_line(self.cursor_offset);
                                self.cursor_offset = text.line_to_char(line_idx);
                                self.target_col = 0;
                            }
                            MoveType::LinePos(MovePosition::End) => {
                                let line_idx = text.char_to_line(self.cursor_offset);
                                let line_start = text.line_to_char(line_idx);
                                let line = text.line(line_idx);

                                let mut char_count = line.len_chars();

                                if char_count > 0 && line.char(char_count - 1) == '\n' {
                                    char_count -= 1;
                                }

                                let end_col = char_count.saturating_sub(1);

                                self.cursor_offset = line_start + end_col;

                                self.target_col = usize::MAX;
                            }

                            MoveType::FinalNonBlank(_) => {
                                let line_idx = text.char_to_line(self.cursor_offset);
                                let line_start = text.line_to_char(line_idx);
                                let line = text.line(line_idx);

                                let mut non_blank_offset = 0;

                                for (i, c) in line.chars().enumerate() {
                                    if c != ' ' && c != '\t' && c != '\n' {
                                        non_blank_offset = i;
                                    }
                                }

                                self.cursor_offset = line_start + non_blank_offset;
                                self.target_col = non_blank_offset;
                            }
                            MoveType::FirstWord(dir) => {
                                let multiplier = match count {
                                    Count::Exact(n) => n.saturating_sub(1),
                                    _ => 0,
                                };
                                dbg!(count);
                                dbg!(multiplier);

                                let current_line_idx = text.char_to_line(self.cursor_offset);

                                let last_line = text.len_lines().saturating_sub(1);
                                let target_line_idx =
                                    (current_line_idx + multiplier).min(last_line);

                                let line_start = text.line_to_char(target_line_idx);
                                let line = text.line(target_line_idx);

                                let mut non_blank_offset = 0;

                                for (i, c) in line.chars().enumerate() {
                                    if c != ' ' && c != '\t' && c != '\n' {
                                        non_blank_offset = i;
                                        break;
                                    }
                                }

                                self.cursor_offset = line_start + non_blank_offset;
                                self.target_col = non_blank_offset;
                            }

                            _ => println!("Unknown MoveType: {:?}", move_type),
                        }
                    }
                    // If it is an explicit Delete command (like pressing 'x' or 'd')
                    (Specifier::Exact(EditAction::Delete), target) => match target {
                        EditTarget::Motion(MoveType::Column(MoveDir1D::Previous, _), count) => {
                            let multiplier = match count {
                                Count::Exact(n) => n,
                                _ => 1,
                            };

                            let start = self.cursor_offset.saturating_sub(multiplier);

                            if start < self.cursor_offset {
                                let range = start..self.cursor_offset;

                                self.buffer.update(cx, |buf, cx| buf.delete_text(range, cx));

                                self.cursor_offset = start;
                            }
                        }
                        EditTarget::Motion(MoveType::Column(MoveDir1D::Next, _), count) => {
                            let multiplier = match count {
                                Count::Exact(n) => n,
                                _ => 1,
                            };

                            let max_len = self.buffer.read(cx).text.chars().count();
                            let end = (self.cursor_offset + multiplier).min(max_len);

                            let range = self.cursor_offset..end;
                            self.buffer.update(cx, |buf, cx| buf.delete_text(range, cx));
                        }
                        _ => println!("Unknown Delete target: {:?}", target),
                    },
                    (Specifier::Exact(EditAction::Yank), target) => match target {
                        EditTarget::Motion(dir, count) => {
                            let multiplier = match count {
                                Count::Exact(n) => n,
                                Count::Contextual => ctx.get_count().unwrap_or(1),
                                _ => 1,
                            };

                            let offset = match dir {
                                MoveType::Column(MoveDir1D::Previous, _) => {
                                    self.cursor_offset.saturating_sub(multiplier)
                                }
                                MoveType::Column(MoveDir1D::Next, _) => (self.cursor_offset
                                    + multiplier)
                                    .min(self.buffer.read(cx).text.chars().count()),
                                MoveType::LinePos(MovePosition::End) => {
                                    let text = self.buffer.read(cx).text.clone();

                                    let line_idx = text.char_to_line(self.cursor_offset);
                                    let line_start = text.line_to_char(line_idx);
                                    let line = text.line(line_idx);
                                    let mut char_count = line.len_chars();

                                    if char_count > 0 && line.char(char_count - 1) == '\n' {
                                        char_count -= 1;
                                    }

                                    line_start + char_count
                                }
                                _ => {
                                    println!("Unhandled yank motion: {:?}", dir);
                                    return;
                                }
                            };

                            let start = self.cursor_offset.min(offset);
                            let end = self.cursor_offset.max(offset);

                            if start < end {
                                let yanked_text =
                                    self.buffer.read(cx).text.slice(start..end).to_string();

                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(yanked_text));
                            }
                        }
                        _ => println!("Unhandled Tartget for yanking: {:?}", target),
                    },
                    _ => {
                        println!("Unhandled  EditAction {:?} on {:?}", specifier, target);
                    }
                }
            }
            Action::Editor(EditorAction::History(HistoryAction::Undo(count))) => {
                let multiplier = match count {
                    Count::Exact(n) => n,
                    Count::Contextual => ctx.get_count().unwrap_or(1),
                    _ => 1,
                };

                for _ in 0..multiplier {
                    self.buffer.update(cx, |buf, cx| {
                        if let Some(offset) = buf.undo(cx) {
                            self.cursor_offset = offset;
                        }
                    });
                }
            }
            Action::Editor(EditorAction::History(HistoryAction::Redo(count))) => {
                let multiplier = match count {
                    Count::Exact(n) => n,
                    Count::Contextual => ctx.get_count().unwrap_or(1),
                    _ => 1,
                };

                for _ in 0..multiplier {
                    self.buffer.update(cx, |buf, cx| {
                        if let Some(offset) = buf.redo(cx) {
                            self.cursor_offset = offset;
                        }
                    });
                }
            }

            // 3. Fallback for Unimplemented Actions (Movements, Window commands, etc.)
            _ => {
                println!("Action triggered but not implemented: {:?}", action);
            }
        }
    }

    fn translate_key(event: &KeyDownEvent) -> TerminalKey {
        // 1. Map GPUI string keys to crossterm KeyCodes
        let code = match event.keystroke.key.as_str() {
            "escape" => KeyCode::Esc,
            "enter" => KeyCode::Enter,
            "backspace" => KeyCode::Backspace,
            "space" => KeyCode::Char(' '),
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "tab" => KeyCode::Tab,
            c if c.len() == 1 => KeyCode::Char(c.chars().next().unwrap()),
            _ => KeyCode::Null,
        };

        // 2. Map GPUI modifiers to crossterm KeyModifiers
        let mut modifiers = KeyModifiers::empty();
        if event.keystroke.modifiers.shift {
            modifiers.insert(KeyModifiers::SHIFT);
        }
        if event.keystroke.modifiers.control {
            modifiers.insert(KeyModifiers::CONTROL);
        }
        if event.keystroke.modifiers.alt {
            modifiers.insert(KeyModifiers::ALT);
        }

        // 3. Construct the crossterm event and convert it to modalkit's TerminalKey
        let key_event = KeyEvent::new(code, modifiers);
        TerminalKey::from(key_event)
    }

    fn move_cursor_vertical(&mut self, lines_to_move: isize, cx: &mut Context<Self>) {
        let text = self.buffer.read(cx).text.clone();
        let current_line_idx = text.char_to_line(self.cursor_offset);

        let new_line_idx = (current_line_idx as isize + lines_to_move)
            .max(0)
            .min(text.len_lines().saturating_sub(1) as isize) as usize;

        let new_line_start = text.line_to_char(new_line_idx);

        let new_line_len = text
            .line(new_line_idx)
            .chars()
            .filter(|&c| c != '\n')
            .count();

        let new_col = self.target_col.min(new_line_len);

        self.cursor_offset = new_line_start + new_col;
    }
}

#[derive(std::default::Default, PartialEq)]
struct TextStyle {
    color: Option<gpui::Rgba>,
}

struct StyleSpan {
    start_byte: usize,
    end_byte: usize,
    style: TextStyle,
}

impl Render for VimEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let buffer = self.buffer.read(cx);
        let text = buffer.text.clone();
        let cursor_line_idx = text.char_to_line(self.cursor_offset);

        let cursor_offset = self.cursor_offset.clone();
        let cursor_visible = self.cursor_visible.clone();
        let is_insert_mode = self.is_insert_mode.clone();

        let query = buffer.query.clone();
        let tree = buffer.tree.clone();

        let editor_list = uniform_list(
            "editor_list",
            text.len_lines(),
            move |visible_range, _window, _app| {
                let visible_start_line = visible_range.start;
                let visible_end_line = visible_range.end;

                let start_byte = text.line_to_byte(visible_start_line);
                let end_byte = if visible_end_line < text.len_lines() {
                    text.line_to_byte(visible_end_line)
                } else {
                    text.len_bytes()
                };

                let mut stye_spans: Vec<StyleSpan> = Vec::new();

                let star_count = query.capture_index_for_name("heading.stars").unwrap();

                if let Some(tree) = &tree {
                    let mut cursor = tree_sitter::QueryCursor::new();
                    cursor.set_byte_range(start_byte..end_byte);

                    let matches = cursor.matches(&query, tree.root_node(), "".as_bytes());

                    matches.for_each(|m| {
                        let mut heading_level = 0;

                        for cap in m.captures {
                            let capture_name = query.capture_names()[cap.index as usize];
                            if capture_name == "heading.stars" {
                                heading_level = cap.node.end_byte() - cap.node.start_byte();
                            }
                        }

                        for cap in m.captures {
                            let capture_name = query.capture_names()[cap.index as usize];

                            let mut style = TextStyle::default();
                            match capture_name {
                                "heading" => {
                                    style.color = match heading_level {
                                        1 => Some(rgba(0x00cef7ff)),
                                        2 => Some(rgba(0x0052f7ff)),
                                        3 => Some(rgba(0x00f7a4ff)),
                                        _ => Some(rgba(0x0000eeff)),
                                    }
                                }
                                _ => continue,
                            }

                            stye_spans.push(StyleSpan {
                                start_byte: cap.node.start_byte(),
                                end_byte: cap.node.end_byte(),
                                style,
                            });
                        }
                    });
                }

                visible_range
                    .map(|line_idx| {
                        let line = text.line(line_idx);
                        let line_start_char = text.line_to_char(line_idx);
                        let line_start_byte = text.line_to_byte(line_idx);
                        let line_end_byte = line_start_byte + line.len_bytes();

                        let line_spans: Vec<&StyleSpan> = stye_spans
                            .iter()
                            .filter(|s| {
                                s.start_byte < line_end_byte && line_start_byte < s.end_byte
                            })
                            .collect();

                        let mut elements = Vec::new();
                        let mut current_chunk = String::new();
                        let mut current_style = TextStyle::default();

                        let mut current_byte = line_start_byte;
                        let mut current_char_idx = line_start_char;

                        let mut flush_chunk =
                            |chunk: &mut String,
                             style: TextStyle,
                             elements: &mut Vec<AnyElement>| {
                                if !chunk.is_empty() {
                                    let mut element = div().child(chunk.clone());
                                    if let Some(c) = style.color {
                                        element = element.text_color(c)
                                    }

                                    elements.push(element.into_any_element());
                                    chunk.clear();
                                }
                            };

                        for ch in line.chars() {
                            if ch == '\n' {
                                break;
                            };

                            let is_cursor =
                                line_idx == cursor_line_idx && current_char_idx == cursor_offset;

                            let mut char_style = TextStyle::default();
                            for span in &line_spans {
                                if current_byte >= span.start_byte && current_byte < span.end_byte {
                                    if let Some(c) = span.style.color {
                                        char_style.color = Some(c)
                                    }
                                }
                            }

                            if char_style != current_style || is_cursor {
                                flush_chunk(&mut current_chunk, current_style, &mut elements);
                                current_style = char_style;
                            }

                            if is_cursor {
                                let cursor_el = if is_insert_mode {
                                    div()
                                        .flex()
                                        .flex_row()
                                        .child(div().w(px(2.0)).bg(if cursor_visible {
                                            rgba(0xccccccff)
                                        } else {
                                            rgba(0x00000000)
                                        }))
                                        .child(ch.to_string())
                                } else {
                                    div()
                                        .bg(if cursor_visible {
                                            rgba(0xccccccff)
                                        } else {
                                            rgba(0x00000000)
                                        })
                                        .text_color(rgb(0xe1e1e1))
                                        .child(ch.to_string())
                                };
                                elements.push(cursor_el.into_any_element());
                            } else {
                                current_chunk.push(ch);
                            }

                            current_byte += ch.len_utf8();
                            current_char_idx += 1;
                        }
                        flush_chunk(&mut current_chunk, current_style, &mut elements);

                        if line_idx == cursor_line_idx && current_char_idx == cursor_offset {
                            let eol_cursor = if is_insert_mode {
                                div()
                                    .flex()
                                    .flex_row()
                                    .child(div().w(px(2.0)).bg(if cursor_visible {
                                        rgba(0xccccccff)
                                    } else {
                                        rgba(0x00000000)
                                    }))
                                    .child(" ".to_string())
                            } else {
                                div()
                                    .bg(if cursor_visible {
                                        rgba(0xccccccff)
                                    } else {
                                        rgba(0x00000000)
                                    })
                                    .text_color(rgb(0xe1e1e1))
                                    .child(" ".to_string())
                            };

                            elements.push(eol_cursor.into_any_element());
                        }

                        div()
                            .flex()
                            .flex_row()
                            .text_color(rgb(0xfefefe))
                            .font_family("Liga SFMonoNerdFont")
                            .children(if elements.is_empty() {
                                vec![div().child(" ").into_any_element()]
                            } else {
                                elements
                            })
                    })
                    .collect()
            },
        )
        .size_full()
        .track_scroll(self.scroll_handle.clone());

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .text_sm()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .child(
                div()
                    .flex_1()
                    .p_4()
                    .text_color(rgb(0x4d4d4d))
                    .child(editor_list),
            )
            .child(
                div()
                    .w_full()
                    .h_6()
                    .bg(rgb(0x007acc))
                    .text_color(rgb(0xffffff))
                    .text_sm()
                    .font_weight(FontWeight::BOLD)
                    .px_2()
                    .flex()
                    .items_center()
                    .child(format!(
                        "Cursor offset: {} | Line: {}",
                        self.cursor_offset,
                        cursor_line_idx + 1
                    )),
            )
    }
}

fn parse_rope(
    parser: &mut tree_sitter::Parser,
    old_tree: Option<&tree_sitter::Tree>,
    text: &Rope,
) -> Option<tree_sitter::Tree> {
    parser.parse_with_options(
        &mut move |byte_offset, position| {
            if byte_offset > text.bytes().len() {
                return &[][..];
            }

            let (chunk, byte_idx, _, _) = text.chunk_at_byte(byte_offset);

            let offset_in_chunk = byte_offset - byte_idx;

            chunk[offset_in_chunk..].as_bytes()
        },
        old_tree,
        None,
    )
}

fn main() {
    let app = Application::new();

    app.run(|cx: &mut App| {
        let options = WindowOptions {
            ..Default::default()
        };

        let buffer = cx.new(|cx| Buffer::new(cx));

        let window = cx
            .open_window(options, |_window, cx| {
                cx.new(|cx| VimEditor::new(cx, buffer.clone()))
            })
            .expect("Failed to open window");

        window
            .update(cx, |view, window, _cx| {
                view.focus_handle.focus(window);
            })
            .unwrap();
    });
}
