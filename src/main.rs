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
use tree_sitter::{InputEdit, Language, Parser};

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

struct VimEditor {
    text: Rope,
    cursor_offset: usize,
    vim_state: VimState,

    cursor_visible: bool,
    is_insert_mode: bool,
    _blink_task: Task<()>,
    target_col: usize,
    last_activity: Instant,

    vim_machine: VimMachine<TerminalKey, EmptyInfo>,

    scroll_handle: UniformListScrollHandle,
    focus_handle: FocusHandle,

    history: History,
    key_manager: KeyManager<TerminalKey, Action<EmptyInfo>, RepeatType>,

    parser: Parser,
    tree: Option<tree_sitter::Tree>,
}

impl VimEditor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let text = Rope::new();

        let text_clone = text.clone();

        let list_state = ListState::new(text_clone.len_lines(), ListAlignment::Top, px(200.0));

        let vim_binding = default_vim_keys::<EmptyInfo>();
        let key_manager = KeyManager::new(vim_binding);

        let vim_state = VimState::default();

        let vim_machine = default_vim_keys();

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

        let org_language = unsafe { Language::from_raw(tree_sitter_org() as *const _) };
        let mut parser = Parser::new();
        parser
            .set_language(&org_language)
            .expect("Failed to load org langauge");

        let tree = parse_rope(&mut parser, None, &text);

        let scroll_handle = UniformListScrollHandle::new();
        let history = History::new();

        Self {
            text,
            cursor_offset: 0,
            target_col: 0,
            scroll_handle,
            focus_handle: cx.focus_handle(),
            vim_state,
            history,
            vim_machine,
            parser,
            tree,
            last_activity: Instant::now(),
            cursor_visible: true,
            is_insert_mode: false,
            _blink_task,
            key_manager,
        }
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cursor_visible = true;

        let old_line_idx = self.text.char_to_line(self.cursor_offset);

        let terminal_key = Self::translate_key(event);
        self.vim_machine.input_key(terminal_key);

        while let Some((action, context)) = self.vim_machine.pop() {
            self.execute_action(action, context);
        }

        if self.vim_machine.mode() == VimMode::Normal && !self.history.current_edit.is_empty() {
            self.history
                .undo_stack
                .push(self.history.current_edit.clone());
            self.history.current_edit = Vec::new();
        }

        self.is_insert_mode = self.vim_machine.mode() == VimMode::Insert;

        let new_line_idx = self.text.char_to_line(self.cursor_offset);
        let strategy = if new_line_idx < old_line_idx {
            ScrollStrategy::Top
        } else {
            ScrollStrategy::Bottom
        };

        self.scroll_handle.scroll_to_item(new_line_idx, strategy);
        self.last_activity = Instant::now();

        cx.notify();
    }

    fn execute_action(&mut self, action: Action<EmptyInfo>, ctx: EditContext) {
        match action {
            // 1. Text Insertion (Typing in Insert mode)
            // Modalkit emits characters or strings directly as EditorActions.
            Action::Editor(EditorAction::InsertText(insert_action)) => match insert_action {
                InsertTextAction::Type(Specifier::Exact(c), _dir, _count) => {
                    match c {
                        // 1. Standard character typing
                        Char::Single(ch) => {
                            self.history.current_edit.push(BufferEdit {
                                offset: self.cursor_offset,
                                text: ch.to_string(),
                                kind: EditKind::Insert,
                            });

                            if !self.history.redo_stack.is_empty() {
                                self.history.redo_stack.clear();
                            }

                            self.insert_char(ch);

                            let current_line_idx = self.text.char_to_line(self.cursor_offset);
                            let current_line_start = self.text.line_to_char(current_line_idx);
                            self.target_col = self.cursor_offset - current_line_start;
                        }

                        // 2. Vim Digraphs (e.g., typing two chars to make a special symbol)
                        Char::Digraph(c1, c2) => {
                            // For now, just push both. Later you'd look them up in a Digraph table.
                            self.history.current_edit.push(BufferEdit {
                                offset: self.cursor_offset,
                                text: c1.to_string(),
                                kind: EditKind::Insert,
                            });

                            if !self.history.redo_stack.is_empty() {
                                self.history.redo_stack.clear();
                            }

                            self.insert_char(c1);

                            self.history.current_edit.push(BufferEdit {
                                offset: self.cursor_offset,
                                text: c2.to_string(),
                                kind: EditKind::Insert,
                            });

                            self.insert_char(c2);

                            let current_line_idx = self.text.char_to_line(self.cursor_offset);
                            let current_line_start = self.text.line_to_char(current_line_idx);
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
                    self.text.insert(self.cursor_offset, &text);
                    self.cursor_offset += text.chars().count();
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
                                let current_line_idx = self.text.char_to_line(self.cursor_offset);
                                let line_start = self.text.line_to_char(current_line_idx);
                                let line = self.text.line(current_line_idx);

                                let mut line_len = line.len_chars();

                                if line_len > 0 && line.char(line_len - 1) == '\n' {
                                    line_len = line_len.saturating_sub(1);
                                }

                                let line_end = current_line_idx + line_len;

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
                                self.move_cursor_vertical(multiplier as isize);
                            }
                            MoveType::Line(MoveDir1D::Previous) => {
                                self.move_cursor_vertical((multiplier as isize) * -1);
                            }
                            MoveType::LinePos(MovePosition::Beginning) => {
                                let line_idx = self.text.char_to_line(self.cursor_offset);
                                self.cursor_offset = self.text.line_to_char(line_idx);
                                self.target_col = 0;
                            }
                            MoveType::LinePos(MovePosition::End) => {
                                let line_idx = self.text.char_to_line(self.cursor_offset);
                                let line_start = self.text.line_to_char(line_idx);
                                let line = self.text.line(line_idx);

                                let mut char_count = line.len_chars();

                                if char_count > 0 && line.char(char_count - 1) == '\n' {
                                    char_count -= 1;
                                }

                                let end_col = char_count.saturating_sub(1);

                                self.cursor_offset = line_start + end_col;

                                self.target_col = usize::MAX;
                            }

                            MoveType::FinalNonBlank(_) => {
                                let line_idx = self.text.char_to_line(self.cursor_offset);
                                let line_start = self.text.line_to_char(line_idx);
                                let line = self.text.line(line_idx);

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

                                let current_line_idx = self.text.char_to_line(self.cursor_offset);

                                let last_line = self.text.len_lines().saturating_sub(1);
                                let target_line_idx =
                                    (current_line_idx + multiplier).min(last_line);

                                let line_start = self.text.line_to_char(target_line_idx);
                                let line = self.text.line(target_line_idx);

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
                                let deleted_text = self.text.slice(range.clone()).to_string();

                                self.history.current_edit.push(BufferEdit {
                                    offset: start,
                                    text: deleted_text,
                                    kind: EditKind::Delete,
                                });

                                self.text.remove(range);
                                self.cursor_offset = start;
                            }
                        }
                        EditTarget::Motion(MoveType::Column(MoveDir1D::Next, _), count) => {
                            let multiplier = match count {
                                Count::Exact(n) => n,
                                _ => 1,
                            };

                            let end = (self.cursor_offset + multiplier).min(self.text.len_chars());

                            self.text.remove(self.cursor_offset..end);
                        }
                        _ => println!("Unknown Delete target: {:?}", target),
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
                    if let Some(edits) = self.history.undo_stack.pop() {
                        for e in edits.iter().rev() {
                            match e.kind {
                                EditKind::Insert => {
                                    self.text
                                        .remove(e.offset..e.offset + e.text.chars().count());
                                    self.cursor_offset = e.offset;
                                }
                                EditKind::Delete => {
                                    self.text.insert(e.offset, &e.text);
                                    self.cursor_offset = e.offset;
                                }
                            }
                        }

                        self.history.redo_stack.push(edits);
                    }
                }
            }
            Action::Editor(EditorAction::History(HistoryAction::Redo(count))) => {
                let multiplier = match count {
                    Count::Exact(n) => n,
                    Count::Contextual => ctx.get_count().unwrap_or(1),
                    _ => 1,
                };

                for _ in 0..multiplier {
                    if let Some(edits) = self.history.redo_stack.pop() {
                        for e in edits.iter() {
                            match e.kind {
                                EditKind::Delete => {
                                    self.text
                                        .remove(e.offset..e.offset + e.text.chars().count());
                                    self.cursor_offset = e.offset + e.text.chars().count();
                                }
                                EditKind::Insert => {
                                    self.text.insert(e.offset, &e.text);
                                    self.cursor_offset = e.offset + e.text.chars().count();
                                }
                            }
                        }

                        self.history.undo_stack.push(edits);
                    }
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

    fn move_cursor_vertical(&mut self, lines_to_move: isize) {
        let current_line_idx = self.text.char_to_line(self.cursor_offset);

        let new_line_idx = (current_line_idx as isize + lines_to_move)
            .max(0)
            .min(self.text.len_lines().saturating_sub(1) as isize)
            as usize;

        let new_line_start = self.text.line_to_char(new_line_idx);

        let new_line_len = self
            .text
            .line(new_line_idx)
            .chars()
            .filter(|&c| c != '\n')
            .count();

        let new_col = self.target_col.min(new_line_len);

        self.cursor_offset = new_line_start + new_col;
    }

    fn insert_char(&mut self, ch: char) {
        let start_byte = self.text.char_to_byte(self.cursor_offset);
        let start_position = Self::rope_offset_to_point(&self.text, self.cursor_offset);

        self.text.insert_char(self.cursor_offset, ch);
        self.cursor_offset += 1;

        let old_end_byte = start_byte; // Same as we insert and don't delete
        let old_end_position = start_position;

        let new_end_byte = self.text.char_to_byte(self.cursor_offset);
        let new_end_position = Self::rope_offset_to_point(&self.text, self.cursor_offset);

        if let Some(tree) = &mut self.tree {
            tree.edit(&InputEdit {
                start_byte,
                old_end_byte,
                new_end_byte,
                start_position,
                old_end_position,
                new_end_position: new_end_position,
            });
        }

        self.tree = parse_rope(&mut self.parser, self.tree.as_ref(), &self.text);
    }

    fn rope_offset_to_point(rope: &Rope, char_offset: usize) -> tree_sitter::Point {
        let line = rope.char_to_line(char_offset);
        let line_start_char = rope.line_to_char(line);
        let col = char_offset - line_start_char;

        let byte_col = rope.char_to_byte(char_offset) - rope.char_to_byte(col);

        tree_sitter::Point::new(line, byte_col)
    }
}

impl Render for VimEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cursor_line_idx = self.text.char_to_line(self.cursor_offset);

        let text = self.text.clone();
        let cursor_offset = self.cursor_offset.clone();
        let cursor_visible = self.cursor_visible.clone();
        let is_insert_mode = self.is_insert_mode.clone();

        let editor_list = uniform_list(
            "editor_list",
            text.len_lines(),
            move |visible_range, _window, _app| {
                visible_range
                    .map(|line_idx| {
                        let line_slice = text.line(line_idx);

                        let mut line_str = line_slice.to_string();

                        if line_str.ends_with('\n') {
                            line_str.pop();
                        }

                        if line_idx == cursor_line_idx {
                            let line_start_char = text.line_to_char(line_idx);
                            let cursor_col = cursor_offset - line_start_char;

                            let chars: Vec<char> = line_str.chars().collect();
                            let safe_col = cursor_col.min(chars.len());

                            let befor_cursor: String = chars[..safe_col].iter().collect();

                            let cursor_char = if cursor_col < chars.len() {
                                chars[safe_col].to_string()
                            } else {
                                " ".to_string()
                            };

                            let after_cursor: String = if safe_col + 1 < chars.len() {
                                chars[safe_col + 1..].iter().collect()
                            } else {
                                String::new()
                            };

                            if is_insert_mode {
                                div()
                                    .flex()
                                    .flex_row()
                                    .text_color(rgb(0xfefefe))
                                    .font_family("Liga SFMonoNerdFont")
                                    .child(befor_cursor)
                                    .child(div().w(px(2.0)).bg(if cursor_visible {
                                        rgba(0xccccccff)
                                    } else {
                                        rgba(0x00000000)
                                    }))
                                    .child(cursor_char)
                                    .child(after_cursor)
                            } else {
                                div()
                                    .flex()
                                    .flex_row()
                                    .text_color(rgb(0xfefefe))
                                    .font_family("Liga SFMonoNerdFont")
                                    .child(befor_cursor)
                                    .child(
                                        div()
                                            .bg(if cursor_visible {
                                                rgba(0xccccccff)
                                            } else {
                                                rgba(0x00000000)
                                            })
                                            .text_color(rgb(0x1e1e1e))
                                            .child(cursor_char),
                                    )
                                    .child(after_cursor)
                            }
                        } else {
                            let display_str = if line_str.is_empty() {
                                " ".to_string()
                            } else {
                                line_str
                            };

                            div()
                                .flex()
                                .flex_row()
                                .text_color(rgb(0xfefefe))
                                .font_family("Liga SFMonoNerdFont")
                                .child(display_str)
                        }
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

        let window = cx
            .open_window(options, |window, cx| cx.new(|cx| VimEditor::new(cx)))
            .expect("Failed to open window");

        window
            .update(cx, |view, window, cx| {
                view.focus_handle.focus(window);
            })
            .unwrap();
    });
}
