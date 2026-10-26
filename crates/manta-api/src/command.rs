use std::path::PathBuf;

// --- Command Mode state

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandModeState {
    pub open: bool,
    pub input: String,
    pub cursor: usize,
    pub candidates: Vec<CommandCompletion>,
    pub selection: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandCompletion {
    pub kind: CompletionKind,
    pub insert: String, // what TAB inserts
    pub label: String,  // Ui displey
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionKind {
    Command,
    File,
}

// --- inputs

#[derive(Debug, Clone)]
pub enum CommandKey {
    Input(char),
    Backspace,
    Delete,
    Enter,
    Escape,
    TabCompletion,
    PrevCandidate,
    NextCandidate,
    HistoryPrev,
    HistoryNext,
    MoveLeft,
    MoveRight,
    Start,
    End,
}

#[derive(Debug, Clone)]
pub enum CommandEffect {
    Updated,
    Submited(String),
    Canceled,
}

// --- machine

pub struct CommandMachine {
    pub state: CommandModeState,
    history: Vec<String>,
    history_index: usize,
}

impl CommandMachine {
    pub fn new() -> Self {
        Self {
            state: CommandModeState {
                open: false,
                input: String::new(),
                cursor: 0,
                candidates: Vec::new(),
                selection: 0,
            },
            history: Vec::new(),
            history_index: 0,
        }
    }

    pub fn open(&mut self) {
        self.state = CommandModeState {
            open: true,
            input: String::new(),
            cursor: 0,
            candidates: Vec::new(),
            selection: 0,
        };
        self.history_index = self.history.len();
    }

    pub fn close(&mut self) {
        self.state.open = false;
        self.state.input.clear();
        self.state.candidates.clear();
        self.state.selection = 0;
    }

    pub fn set_candidates(&mut self, candidates: Vec<CommandCompletion>) {
        self.state.candidates = candidates;
    }

    pub fn apply(&mut self, key: CommandKey) -> CommandEffect {
        if !self.state.open {
            return CommandEffect::Canceled;
        }
        dbg!(&key);

        match key {
            CommandKey::Input(ch) => {
                self.state.input.insert(self.state.cursor, ch);
                self.state.cursor += ch.len_utf8();
                CommandEffect::Updated
            }
            CommandKey::Backspace => {
                if let Some(c) = self.state.input[..self.state.cursor].chars().last() {
                    let prev = self.state.cursor - c.len_utf8();
                    self.state.input.replace_range(prev..self.state.cursor, "");
                    self.state.cursor = prev;
                }
                CommandEffect::Updated
            }
            CommandKey::Delete => {
                if let Some(c) = self.state.input[self.state.cursor..].chars().next() {
                    let next = self.state.cursor + c.len_utf8();
                    self.state.input.replace_range(self.state.cursor..next, "");
                }
                CommandEffect::Updated
            }
            CommandKey::Enter => {
                let cmd = self.state.input.to_owned();
                self.close();
                if cmd.trim().is_empty() {
                    return CommandEffect::Canceled;
                }
                self.push_history(cmd.trim().to_owned());
                CommandEffect::Submited(cmd)
            }
            CommandKey::Escape => {
                self.close();
                CommandEffect::Canceled
            }
            CommandKey::TabCompletion => {
                if let Some(c) = self.state.candidates.get(self.state.selection).cloned() {
                    self.state.input = c.insert;
                    self.state.cursor = self.state.input.len();
                    self.state.candidates.clear();
                }
                CommandEffect::Updated
            }
            CommandKey::PrevCandidate => {
                if !self.state.candidates.is_empty() {
                    let n = self.state.candidates.len();
                    self.state.selection = (self.state.selection + n - 1) % n;
                }
                CommandEffect::Updated
            }
            CommandKey::NextCandidate => {
                if !self.state.candidates.is_empty() {
                    let n = self.state.candidates.len();
                    self.state.selection = (self.state.selection + 1) % n;
                }
                CommandEffect::Updated
            }
            CommandKey::HistoryPrev => {
                if !self.history.is_empty() && self.history_index > 0 {
                    self.history_index -= 1;
                    self.state.input = self.history[self.history_index].clone();
                    self.state.cursor = self.state.input.len();
                }
                CommandEffect::Updated
            }
            CommandKey::HistoryNext => {
                if self.history_index < self.history.len() {
                    self.history_index += 1;
                    self.state.input = self
                        .history
                        .get(self.history_index)
                        .cloned()
                        .unwrap_or_default();
                    self.state.cursor = self.state.input.len();
                }
                CommandEffect::Updated
            }
            CommandKey::MoveLeft => {
                if let Some(c) = self.state.input[..self.state.cursor].chars().last() {
                    self.state.cursor -= c.len_utf8();
                }
                CommandEffect::Updated
            }
            CommandKey::MoveRight => {
                if let Some(c) = self.state.input[self.state.cursor..].chars().next() {
                    self.state.cursor += c.len_utf8();
                }
                CommandEffect::Updated
            }
            CommandKey::Start => {
                self.state.cursor = 0;
                CommandEffect::Updated
            }
            CommandKey::End => {
                self.state.cursor = self.state.input.len();
                CommandEffect::Updated
            }
        }
    }

    pub fn push_history(&mut self, entry: String) {
        if entry.is_empty() || self.history.last() == Some(&entry) {
            return;
        }
        self.history.push(entry);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineTarget {
    Absolute(usize),
    Relative(isize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamKind {
    Text,   // Free text
    File,   // Path completed against workspace dir
    Line,   // Line number
    Number, // Number
    Bool,   // optional/flag args
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamStyle {
    Positional, // Filled in Order/Counted
    Option,     // -name value or -name=value
    Flag,       // -name -> Arg::Bool(true)
}

#[derive(Debug, Clone)]
pub struct ParamSpec {
    pub name: &'static str,
    pub kind: ParamKind,
    pub style: ParamStyle,
    pub required: bool,
}

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub id: &'static str, // core:save
    pub description: String,
    pub params: Vec<ParamSpec>,
}

impl CommandSpec {
    pub fn new(id: &'static str, description: impl Into<String>) -> Self {
        CommandSpec {
            id,
            description: description.into(),
            params: Vec::new(),
        }
    }

    /// Register Positional and Required param
    pub fn param(mut self, name: &'static str, kind: ParamKind) -> Self {
        self.params.push(ParamSpec {
            name,
            kind,
            style: ParamStyle::Positional,
            required: true,
        });
        self
    }

    /// Register Positional and Optional param
    pub fn opt(mut self, name: &'static str, kind: ParamKind) -> Self {
        self.params.push(ParamSpec {
            name,
            kind,
            style: ParamStyle::Positional,
            required: false,
        });
        self
    }

    /// Register Flag/Bool, optional param
    pub fn flag(mut self, name: &'static str) -> Self {
        self.params.push(ParamSpec {
            name,
            kind: ParamKind::Bool,
            style: ParamStyle::Flag,
            required: false,
        });
        self
    }

    /// Register Option (name=value/name value), optional param
    pub fn option(mut self, name: &'static str, kind: ParamKind) -> Self {
        self.params.push(ParamSpec {
            name,
            kind,
            style: ParamStyle::Option,
            required: false,
        });
        self
    }
}

impl CommandSpec {
    pub fn bind_args(&self, tokens: &[&str]) -> Result<Vec<Arg>, String> {
        let mut pos: usize = 0;
        let mut filled: Vec<Option<Arg>> = vec![None; self.params.len()];

        let mut i: usize = 0;

        while i < tokens.len() {
            let token = tokens[i];

            if token.starts_with("-") {
                let rest = token.trim_start_matches("-");
                let (name, val) = match rest.split_once("=") {
                    Some((n, v)) => (n, Some(v)),
                    None => (rest, None),
                };

                let is_known_option = self.params.iter().any(|p| p.name == name);

                if !is_known_option {
                    while pos < self.params.len()
                        && self.params[pos].style != ParamStyle::Positional
                    {
                        pos += 1;
                    }

                    if pos >= self.params.len() {
                        return Err(format!("too many arguments for {}", self.id));
                    }

                    filled[pos] = Some(self.map_arg(pos, token)?);
                    pos += 1;
                } else {
                    // treat it as a optional one
                    let Some(p) = self.params.iter().position(|p| p.name == name) else {
                        return Err(format!("unknown option -{name}"));
                    };

                    if self.params[p].style == ParamStyle::Positional {
                        return Err(format!("'{name}' is a positional argument"));
                    }

                    if filled[p].is_some() {
                        return Err(format!("Duplicate option '-{name}'"));
                    }

                    let value = match val {
                        Some(v) => v.to_owned(),
                        None if self.params[p].kind != ParamKind::Bool => {
                            i += 1;

                            tokens
                                .get(i)
                                .copied()
                                .ok_or_else(|| format!("option '-{name}' needs a value"))?
                                .to_owned()
                        }
                        None => match tokens.get(i + 1).copied() {
                            Some(t) if matches!(t, "true" | "false" | "1" | "0") => {
                                i += 1;
                                t.to_owned().to_owned()
                            }
                            _ => String::new(),
                        },
                    };

                    filled[p] = Some(self.map_arg(p, &value)?);
                }
            } else {
                while pos < self.params.len() && self.params[pos].style != ParamStyle::Positional {
                    pos += 1;
                }

                if pos >= self.params.len() {
                    return Err(format!("too many arguments for {}", self.id));
                }

                filled[pos] = Some(self.map_arg(pos, token)?);
                pos += 1;
            }

            i += 1;
        }

        for (p, spec) in self.params.iter().enumerate() {
            if spec.required && filled[p].is_none() {
                return Err(format!(
                    "missing required param \"{}\" for \"{}\"",
                    spec.name, self.id
                ));
            }
        }

        Ok(filled.into_iter().flatten().collect())
    }

    fn map_arg(&self, p: usize, raw: &str) -> Result<Arg, String> {
        let name = self.params[p].name;

        match self.params[p].kind {
            ParamKind::Text => Ok(Arg::Text(raw.to_owned())),
            ParamKind::File => Ok(Arg::File(PathBuf::from(raw))),
            ParamKind::Line => {
                if let Some(stripped) = raw.strip_prefix("+") {
                    let val = stripped
                        .parse::<isize>()
                        .map_err(|_| format!("'{raw}' is not a valid line number"))?;

                    Ok(Arg::Line(LineTarget::Relative(val)))
                } else if let Some(stripped) = raw.strip_prefix("-") {
                    let val = stripped
                        .parse::<isize>()
                        .map_err(|_| format!("'{raw}' is not a valid line number"))?;

                    Ok(Arg::Line(LineTarget::Relative(-val)))
                } else {
                    let val = raw
                        .parse::<usize>()
                        .map_err(|_| format!("'{raw}' is not a valid line number"))?;
                    Ok(Arg::Line(LineTarget::Absolute(val)))
                }
            }

            ParamKind::Number => raw
                .parse::<i64>()
                .map(Arg::Number)
                .map_err(|_| format!("'{raw}' is not a number")),
            ParamKind::Bool => match raw {
                "" | "true" | "1" => Ok(Arg::Bool(true)),
                "false" | "0" => Ok(Arg::Bool(false)),
                _ => Err(format!("'{raw}' is not a flag for '{name}'")),
            },
        }
    }
}

// Runtime Args passed to the command
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg {
    Text(String),
    File(PathBuf),
    Line(LineTarget),
    Number(i64),
    Bool(bool),
}

#[allow(dead_code, unused_imports)]
mod tests {
    use std::path::PathBuf;

    use super::{Arg, CommandSpec, LineTarget, ParamKind};

    fn goto_spec() -> CommandSpec {
        CommandSpec::new("core:goto", "Jump to line and optionally to column")
            .param("line", ParamKind::Line)
            .opt("column", ParamKind::Number)
    }

    fn save_spec() -> CommandSpec {
        CommandSpec::new("core:write", "write buffer to file")
            .opt("path", ParamKind::File)
            .flag("force")
    }

    fn search_spec() -> CommandSpec {
        CommandSpec::new("search:find", "find a pattern")
            .param("pattern", ParamKind::Text)
            .flag("case-insensitive")
            .option("limit", ParamKind::Number)
    }

    fn check(spec: &CommandSpec, tokens: &[&str], expected: Result<Vec<Arg>, &str>) {
        match (spec.bind_args(tokens), expected) {
            (Ok(got), Ok(ex)) => assert_eq!(got, ex, "tokens: {tokens:?}"),
            (Ok(got), Err(_)) => {
                panic!("Expected error for tokens {tokens:?} but got {got:?}")
            }
            (Err(got), Ok(_)) => panic!("Expected Ok for {tokens:?} but got error {got:?}"),
            (Err(got), Err(ex)) => assert_eq!(got, ex, "tokens: {tokens:?}"),
        }
    }

    // Positionals + ordering
    #[test]
    fn test_goto_binds_positional_order() {
        let g = goto_spec();
        // leave optional out
        check(&g, &["42"], Ok(vec![Arg::Line(LineTarget::Absolute(42))]));
        // provide optinal
        check(
            &g,
            &["42", "16"],
            Ok(vec![Arg::Line(LineTarget::Absolute(42)), Arg::Number(16)]),
        );
    }

    #[test]
    fn test_goto_rejects_invalid() {
        let g = goto_spec();
        check(
            &g,
            &[],
            Err("missing required param \"line\" for \"core:goto\""),
        );
        check(
            &g,
            &["42", "16", "41"],
            Err("too many arguments for core:goto"),
        );
        check(&g, &["abc"], Err("'abc' is not a valid line number"));
        check(&g, &["42", "abc"], Err("'abc' is not a number"));
    }

    #[test]
    fn negative_goto_numbers() {
        let g = goto_spec();
        check(&g, &["-5"], Ok(vec![Arg::Line(LineTarget::Relative(-5))]));
        check(&g, &["+5"], Ok(vec![Arg::Line(LineTarget::Relative(5))]));
    }

    // Flags + Positionals
    #[test]
    fn save_flags_can_be_everywhere() {
        let s = save_spec();
        check(
            &s,
            &["notes.org"],
            Ok(vec![Arg::File(PathBuf::from("notes.org"))]),
        );

        check(
            &s,
            &["notes.org", "-force"],
            Ok(vec![Arg::File(PathBuf::from("notes.org")), Arg::Bool(true)]),
        );

        check(
            &s,
            &["-force", "notes.org"],
            Ok(vec![Arg::File(PathBuf::from("notes.org")), Arg::Bool(true)]),
        );
    }

    #[test]
    fn save_flag_edge_cases() {
        let s = save_spec();

        check(
            &s,
            &["-force", "false", "notes.org"],
            Ok(vec![
                Arg::File(PathBuf::from("notes.org")),
                Arg::Bool(false),
            ]),
        );

        check(
            &s,
            &["-force=1", "notes.org"],
            Ok(vec![Arg::File(PathBuf::from("notes.org")), Arg::Bool(true)]),
        );

        check(
            &s,
            &["-force=yes", "notes.org"],
            Err("'yes' is not a flag for 'force'"),
        );
        check(&s, &["-force"], Ok(vec![Arg::Bool(true)]));
        check(&s, &["-force", "-force"], Err("Duplicate option '-force'"));
        check(
            &s,
            &["-path=note.org"],
            Err("'path' is a positional argument"),
        );
    }
}
