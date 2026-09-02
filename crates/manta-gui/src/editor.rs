use std::ops::Range;
use std::time::Instant;

use gpui::*;
use manta_core::buffer::Buffer;
use modalkit::actions::{
    Action, CommandAction, CommandBarAction, EditAction, EditorAction, HistoryAction,
    InsertTextAction,
};
use modalkit::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use modalkit::editing::context::EditContext;
use modalkit::env::vim::VimMode;
use modalkit::env::vim::keybindings::VimMachine;
use modalkit::keybindings::BindingMachine;
use modalkit::prelude::Specifier;
use modalkit::prelude::*;
use modalkit::{
    editing::application::EmptyInfo, env::vim::keybindings::default_vim_keys, key::TerminalKey,
};

use crate::workspace::EditorEvent;

enum BufferEvent {
    LinesInserted { row: usize, line_delta: usize },
    LinesDeleted { row: usize, line_delta: usize },
}
impl EventEmitter<BufferEvent> for Buffer {}

pub struct VimEditor {
    pub buffer: Entity<Buffer>,
    _buffer_subscription: Subscription,

    cursor_offset: usize,
    folded_start_lines: Vec<usize>,
    line_map: Vec<usize>,

    cursor_visible: bool,
    is_insert_mode: bool,
    _blink_task: Task<()>,
    target_col: usize,
    last_activity: Instant,

    vim_machine: VimMachine<TerminalKey, EmptyInfo>,

    scroll_handle: UniformListScrollHandle,
    pub focus_handle: FocusHandle,
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

impl VimEditor {
    pub fn new(cx: &mut Context<Self>, buffer: Entity<Buffer>) -> Self {
        let _buffer_subscription = cx.subscribe(&buffer, |editor, _, event, cx| {
            match event {
                BufferEvent::LinesInserted { row, line_delta } => {
                    for fold_start in editor.folded_start_lines.iter_mut() {
                        if *fold_start >= *row {
                            *fold_start += line_delta;
                        }
                    }

                    editor.rebuild_line_map(cx);
                }
                BufferEvent::LinesDeleted { row, line_delta } => {
                    for fold_start in editor.folded_start_lines.iter_mut() {
                        if *fold_start >= *row {
                            *fold_start -= line_delta;
                        }
                    }
                    editor.rebuild_line_map(cx);
                }
            }
            cx.notify();
        });

        let vim_machine = default_vim_keys();
        let folded_start_lines = Vec::new();
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

        let mut res = Self {
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
            folded_start_lines,
        };

        res.rebuild_line_map(cx);
        res
    }

    fn rebuild_line_map(&mut self, cx: &mut Context<Self>) {
        self.line_map.clear();
        let buffer = self.buffer.read(cx);
        let total_lines = buffer.text.len_lines();

        let mut folded_ranges = Vec::new();

        if let Some(tree) = &buffer.syntax.tree {
            for &start_line in &self.folded_start_lines {
                let byte_offset = buffer.text.line_to_byte(start_line);
                let mut node = tree
                    .root_node()
                    .descendant_for_byte_range(byte_offset, byte_offset);

                while let Some(n) = node {
                    if n.kind() == "section" {
                        let mut cursor = n.walk();
                        let mut fold_start = n.start_position().row;

                        // Keep headline visible
                        for child in n.children(&mut cursor) {
                            if child.kind() == "headline" {
                                fold_start = child.end_position().row;
                                break;
                            }
                        }

                        let fold_end = n.end_position().row;

                        if fold_start < fold_end {
                            folded_ranges.push(fold_start..fold_end);
                        }
                        break;
                    }
                    node = n.parent();
                }
            }
        }

        folded_ranges.sort_by_key(|k| k.start);

        // Remove Folds that are inside oter folds like ([]) -> () or ([)] -> (]
        let mut merged_folds: Vec<Range<usize>> = Vec::new();
        for fold in &folded_ranges {
            if let Some(last) = merged_folds.last_mut() {
                if fold.start <= last.end {
                    last.end = last.end.max(fold.end);
                    continue;
                }
            }
            merged_folds.push(fold.clone());
        }

        folded_ranges = merged_folds;

        let mut current_line = 0;
        let mut fold_idx = 0;

        while current_line < total_lines {
            if fold_idx < folded_ranges.len() && current_line == folded_ranges[fold_idx].start {
                current_line = folded_ranges[fold_idx].end;
                fold_idx += 1;
            } else {
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

        if self.vim_machine.mode() == VimMode::Normal {
            if event.keystroke.key.as_str() == ":" {
                println!("Open command bar");
                cx.emit(EditorEvent::ExecuteCommand("command-bar:open".to_string()));
                return;
            }

            if terminal_key == TerminalKey::from(KeyCode::Tab) {
                self.toggle_fold(cx);
                self.last_activity = Instant::now();
                return;
            }
        }

        self.vim_machine.input_key(terminal_key);

        while let Some((action, context)) = self.vim_machine.pop() {
            self.execute_action(action, context, cx);
        }

        self.is_insert_mode = self.vim_machine.mode() == VimMode::Insert;
        self.clamp_cursor(cx);

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
                InsertTextAction::Type(Specifier::Exact(c), _dir, _count) => match c {
                    Char::Single(ch) => {
                        self.buffer.update(cx, |buf, cx| {
                            let old_lines = buf.text.len_lines();
                            buf.insert(self.cursor_offset, &ch.to_string());
                            let new_lines = buf.text.len_lines();

                            if new_lines > old_lines {
                                let row = buf.text.char_to_line(self.cursor_offset);
                                cx.emit(BufferEvent::LinesInserted {
                                    row,
                                    line_delta: new_lines - old_lines,
                                });
                            }
                        });

                        let text = self.buffer.read(cx).text.clone();

                        self.cursor_offset += 1;
                        let current_line_idx = text.char_to_line(self.cursor_offset);
                        let current_line_start = text.line_to_char(current_line_idx);
                        self.target_col = self.cursor_offset - current_line_start;
                    }

                    Char::Digraph(c1, c2) => {
                        let concat: String = [c1, c2].iter().collect();

                        self.buffer.update(cx, |buf, cx| {
                            let old_lines = buf.text.len_lines();
                            buf.insert(self.cursor_offset, &concat);
                            let new_lines = buf.text.len_lines();

                            if new_lines > old_lines {
                                let row = buf.text.char_to_line(self.cursor_offset);
                                cx.emit(BufferEvent::LinesInserted {
                                    row,
                                    line_delta: new_lines - old_lines,
                                });
                            }
                        });

                        let text = self.buffer.read(cx).text.clone();

                        self.cursor_offset += 2;
                        let current_line_idx = text.char_to_line(self.cursor_offset);
                        let current_line_start = text.line_to_char(current_line_idx);
                        self.target_col = self.cursor_offset - current_line_start;
                    }
                    Char::CtrlSeq(seq) => {
                        println!("CtrlSeq insertion not implemented: {}", seq);
                    }
                    Char::CopyLine(dir) => {
                        println!(
                            "CopyLine insertion not implemented for direction: {:?}",
                            dir
                        );
                    }
                },
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

                        self.buffer.update(cx, |buf, cx| {
                            let old_lines = buf.text.len_lines();
                            buf.insert(offset, &full_text);
                            let new_lines = buf.text.len_lines();

                            if new_lines > old_lines {
                                let row = buf.text.char_to_line(self.cursor_offset);
                                cx.emit(BufferEvent::LinesInserted {
                                    row,
                                    line_delta: new_lines - old_lines,
                                });
                            }
                        });

                        self.cursor_offset = offset + full_text.chars().count() + 1;
                    }
                }

                _ => {
                    println!("Unhandled InsertTextAction: {:?}", insert_action);
                }
            },
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
                    (Specifier::Exact(EditAction::Delete), target) => match target {
                        EditTarget::Motion(MoveType::Column(MoveDir1D::Previous, _), count) => {
                            let multiplier = match count {
                                Count::Exact(n) => n,
                                _ => 1,
                            };

                            let start = self.cursor_offset.saturating_sub(multiplier);

                            if start < self.cursor_offset {
                                let range = start..self.cursor_offset;

                                self.buffer.update(cx, |buf, cx| {
                                    let old_lines = buf.text.len_lines();
                                    buf.delete(range.start, range.end);
                                    let new_lines = buf.text.len_lines();

                                    if old_lines > new_lines {
                                        let row = buf.text.char_to_line(range.start);
                                        cx.emit(BufferEvent::LinesDeleted {
                                            row,
                                            line_delta: old_lines - new_lines,
                                        });
                                    }
                                });

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
                            self.buffer.update(cx, |buf, cx| {
                                let old_lines = buf.text.len_lines();
                                buf.delete(range.start, range.end);
                                let new_lines = buf.text.len_lines();

                                if old_lines > new_lines {
                                    let row = buf.text.char_to_line(range.start);
                                    cx.emit(BufferEvent::LinesDeleted {
                                        row,
                                        line_delta: old_lines - new_lines,
                                    });
                                }
                            });

                            let new_len = self.buffer.read(cx).text.chars().count();
                            self.cursor_offset = self.cursor_offset.min(new_len);
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
            Action::Editor(EditorAction::History(action)) => match action {
                HistoryAction::Checkpoint => {
                    self.buffer.update(cx, |buf, _| buf.end_transaction());
                }
                HistoryAction::Redo(count) => {
                    let multiplier = match count {
                        Count::Exact(n) => n,
                        Count::Contextual => ctx.get_count().unwrap_or(1),
                        _ => 1,
                    };

                    for _ in 0..multiplier {
                        self.buffer.update(cx, |buf, cx| {
                            if let Some(offset) = buf.redo() {
                                self.cursor_offset = offset;
                            }
                        });
                    }
                }

                HistoryAction::Undo(count) => {
                    let multiplier = match count {
                        Count::Exact(n) => n,
                        Count::Contextual => ctx.get_count().unwrap_or(1),
                        _ => 1,
                    };

                    for _ in 0..multiplier {
                        self.buffer.update(cx, |buf, cx| {
                            if let Some(offset) = buf.undo() {
                                self.cursor_offset = offset;
                            }
                        });
                    }
                }
            },
            _ => {
                println!("Action triggered but not implemented: {:?}", action);
            }
        }
    }

    fn clamp_cursor(&mut self, cx: &mut Context<Self>) {
        let text = self.buffer.read(cx).text.clone();

        if text.len_chars() == 0 {
            self.cursor_offset = 0;
            return;
        }

        let line_idx = text.char_to_line(self.cursor_offset);
        let line = text.line(line_idx);
        let line_start = text.line_to_char(line_idx);

        let mut char_count = line.len_chars();

        if char_count > 0 && line.char(char_count - 1) == '\n' {
            char_count -= 1;
        }

        let max_col = if self.is_insert_mode {
            char_count
        } else {
            char_count.saturating_sub(1)
        };

        let current_column = self.cursor_offset - line_start;

        if current_column > max_col {
            self.cursor_offset = line_start + max_col;
        }
    }

    fn translate_key(event: &KeyDownEvent) -> TerminalKey {
        // 1. Map GPUI string keys to crossterm KeyCodes
        let code = match event.keystroke.key.as_str() {
            "escape" => KeyCode::Esc,
            "enter" => KeyCode::Enter,
            "backspace" => KeyCode::Backspace,
            "delete" => KeyCode::Delete,
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
        let current_physical_line_idx = text.char_to_line(self.cursor_offset);

        let current_virtual_line_idx = self
            .line_map
            .iter()
            .position(|&l| l == current_physical_line_idx)
            .unwrap_or_else(|| {
                let pos = self
                    .line_map
                    .partition_point(|&l| l < current_physical_line_idx);
                pos.min(self.line_map.len().saturating_sub(1))
            });

        let new_virtual_line_idx = (current_virtual_line_idx as isize + lines_to_move)
            .max(0)
            .min(self.line_map.len().saturating_sub(1) as isize)
            as usize;

        let new_actual_line_idx = self.line_map[new_virtual_line_idx];

        let new_line_start = text.line_to_char(new_actual_line_idx);
        let new_line_len = text
            .line(new_actual_line_idx)
            .chars()
            .filter(|&c| c != '\n')
            .count();

        let new_col = self.target_col.min(new_line_len);

        self.cursor_offset = new_line_start + new_col;
    }

    fn toggle_fold(&mut self, cx: &mut Context<Self>) {
        let buffer = self.buffer.read(cx);
        let byte_offset = buffer.text.char_to_byte(self.cursor_offset);

        let tree = match &buffer.syntax.tree {
            Some(t) => t,
            None => return,
        };

        let mut node = tree
            .root_node()
            .descendant_for_byte_range(byte_offset, byte_offset);
        let mut section_node = None;

        // Find our section to collapse
        while let Some(n) = node {
            if n.kind() == "section" {
                println!("Found section");
                section_node = Some(n);
                break;
            }
            node = n.parent();
        }

        if let Some(sec) = section_node {
            let start_line = sec.start_position().row;

            if let Some(pos) = self
                .folded_start_lines
                .iter()
                .position(|l| l == &start_line)
            {
                self.folded_start_lines.remove(pos);
            } else {
                self.folded_start_lines.push(start_line);
            }

            self.rebuild_line_map(cx);
            cx.notify();
        }
    }
}

impl Focusable for VimEditor {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for VimEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let buffer_entity = self.buffer.clone();

        let cursor_line_idx = self.buffer.read(cx).text.char_to_line(self.cursor_offset);
        let cursor_offset = self.cursor_offset.clone();
        let cursor_visible = self.cursor_visible.clone();
        let is_insert_mode = self.is_insert_mode.clone();
        let line_map = self.line_map.clone();

        let editor_list = uniform_list(
            "editor_list",
            line_map.len(),
            move |visible_range, _window, app| {
                let buffer = buffer_entity.read(app);
                let text = buffer.text.clone();

                visible_range
                    .map(|virtual_index| {
                        let line_idx = line_map[virtual_index];

                        let line = text.line(line_idx);
                        let line_start_char = text.line_to_char(line_idx);
                        let line_start_byte = text.line_to_byte(line_idx);

                        let raw_syntax = buffer
                            .syntax
                            .get_highlights(&line.to_string(), line_start_byte);

                        let line_spans: Vec<StyleSpan> = raw_syntax
                            .iter()
                            .map(|span| {
                                let mut style = TextStyle::default();
                                match span.style {
                                    manta_core::syntax::OrgToken::HeadlineStars
                                    | manta_core::syntax::OrgToken::Headline => {
                                        style.color = rgb(0x6f9cde).into()
                                    }
                                    manta_core::syntax::OrgToken::Text => {}
                                }

                                StyleSpan {
                                    start_byte: line_start_byte + span.start_byte,
                                    end_byte: line_start_byte + span.end_byte,
                                    style,
                                }
                            })
                            .collect();

                        let mut elements = Vec::new();
                        let mut current_chunk = String::new();
                        let mut current_style = TextStyle::default();

                        let mut current_byte = line_start_byte;
                        let mut current_char_idx = line_start_char;

                        let flush_chunk =
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
    }
}
