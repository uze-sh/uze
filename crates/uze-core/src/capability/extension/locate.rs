//! What sits at a place in an extension's file, for completion and hover.
//!
//! An editor asks while the author is typing, so the document is usually
//! broken: an open brace, a string not yet closed. Rather than parse it,
//! this reads the text up to the cursor the way KDL's lexer would (strings,
//! raw strings, comments, braces and statement ends), which answers on a
//! document that does not parse as well as on one that does.

/// Where the cursor is, in the terms completion offers names in.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Located {
    /// The nodes enclosing the cursor's statement, outermost first.
    pub path: Vec<String>,
    /// The name of the statement the cursor is in, once it has one.
    pub node: Option<String>,
    pub at: At,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum At {
    /// Where a statement's name is written: completion offers nodes.
    NodeName { prefix: String },
    /// After a statement's name, on a bare word: its properties.
    PropName { prefix: String },
    /// Inside a string: a property's value (`prop`) or an argument.
    Value {
        prop: Option<String>,
        prefix: String,
    },
    /// Inside `{…}` in a string: a reference to data or a lookup.
    Reference {
        prop: Option<String>,
        prefix: String,
    },
    /// In a comment, or nowhere completion means anything.
    #[default]
    Nothing,
}

/// What is at byte `offset` of `text`.
pub fn locate(text: &str, offset: usize) -> Located {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let mut lexer = Lexer::default();
    let mut chars = before.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        match lexer.mode {
            Mode::LineComment => {
                if ch == '\n' {
                    lexer.mode = Mode::Normal;
                    lexer.end_statement();
                }
            }
            Mode::BlockComment(depth) => {
                if ch == '*' && chars.peek().map(|(_, next)| *next) == Some('/') {
                    chars.next();
                    lexer.mode = if depth == 1 {
                        Mode::Normal
                    } else {
                        Mode::BlockComment(depth - 1)
                    };
                } else if ch == '/' && chars.peek().map(|(_, next)| *next) == Some('*') {
                    chars.next();
                    lexer.mode = Mode::BlockComment(depth + 1);
                }
            }
            Mode::String { raw_hashes } => {
                if raw_hashes.is_none() && ch == '\\' {
                    chars.next();
                    continue;
                }
                if ch == '"' {
                    let hashes = raw_hashes.unwrap_or(0);
                    let closes =
                        (0..hashes).all(|_| chars.next_if(|(_, next)| *next == '#').is_some());
                    if closes {
                        lexer.mode = Mode::Normal;
                        lexer.word = None;
                    }
                }
            }
            Mode::Normal => match ch {
                '/' if chars.peek().map(|(_, next)| *next) == Some('/') => {
                    chars.next();
                    lexer.mode = Mode::LineComment;
                }
                '/' if chars.peek().map(|(_, next)| *next) == Some('*') => {
                    chars.next();
                    lexer.mode = Mode::BlockComment(1);
                }
                '/' if chars.peek().map(|(_, next)| *next) == Some('-') => {
                    chars.next();
                }
                '#' if matches!(chars.peek().map(|(_, next)| *next), Some('"' | '#')) => {
                    let mut hashes = 1;
                    while chars.next_if(|(_, next)| *next == '#').is_some() {
                        hashes += 1;
                    }
                    if chars.next_if(|(_, next)| *next == '"').is_some() {
                        lexer.open_string(Some(hashes), at + hashes + 1);
                    }
                }
                '"' => lexer.open_string(None, at + 1),
                '{' => {
                    lexer.finish_word();
                    let name = lexer.node.take().unwrap_or_default();
                    lexer.path.push(name);
                    lexer.end_statement();
                }
                '}' => {
                    lexer.finish_word();
                    lexer.path.pop();
                    lexer.end_statement();
                }
                ';' | '\n' => {
                    lexer.finish_word();
                    lexer.end_statement();
                }
                '=' => {
                    lexer.finish_word();
                    lexer.pending_prop = lexer.last_word.take();
                }
                ch if ch.is_whitespace() => lexer.finish_word(),
                ch => lexer.word.get_or_insert_with(String::new).push(ch),
            },
        }
    }
    lexer.answer(before)
}

#[derive(Clone, Copy, Debug, Default)]
enum Mode {
    #[default]
    Normal,
    LineComment,
    BlockComment(u32),
    /// `raw_hashes` for a raw string, the `#`s it closes with.
    String {
        raw_hashes: Option<usize>,
    },
}

#[derive(Default)]
struct Lexer {
    mode: Mode,
    path: Vec<String>,
    /// The statement's name, once its first word ended.
    node: Option<String>,
    /// The bare word being read.
    word: Option<String>,
    last_word: Option<String>,
    /// The property a `=` named, waiting for its value.
    pending_prop: Option<String>,
    /// The property the open string is the value of.
    string_prop: Option<String>,
    string_text_start: usize,
}

impl Lexer {
    /// A string opens; `text_start` is the byte its text begins at.
    fn open_string(&mut self, raw_hashes: Option<usize>, text_start: usize) {
        self.finish_word();
        self.string_prop = self.pending_prop.take();
        self.mode = Mode::String { raw_hashes };
        self.string_text_start = text_start;
    }

    fn finish_word(&mut self) {
        if let Some(word) = self.word.take() {
            if self.node.is_none() {
                self.node = Some(word);
            } else {
                self.last_word = Some(word);
            }
        }
    }

    fn end_statement(&mut self) {
        self.node = None;
        self.word = None;
        self.last_word = None;
        self.pending_prop = None;
    }

    fn answer(mut self, before: &str) -> Located {
        let at = match self.mode {
            Mode::LineComment | Mode::BlockComment(_) => At::Nothing,
            Mode::String { .. } => {
                let text = before.get(self.string_text_start..).unwrap_or("");
                match open_reference(text) {
                    Some(prefix) => At::Reference {
                        prop: self.string_prop.clone(),
                        prefix,
                    },
                    None => At::Value {
                        prop: self.string_prop.clone(),
                        prefix: text.to_owned(),
                    },
                }
            }
            Mode::Normal => match (&self.node, self.word.take()) {
                (None, word) => At::NodeName {
                    prefix: word.unwrap_or_default(),
                },
                (Some(_), Some(word)) => At::PropName { prefix: word },
                (Some(_), None) if self.pending_prop.is_some() => At::Nothing,
                (Some(_), None) => At::PropName {
                    prefix: String::new(),
                },
            },
        };
        let node = match &at {
            At::NodeName { .. } => None,
            _ => self.node.clone(),
        };
        Located {
            path: self.path,
            node,
            at,
        }
    }
}

/// The text after the last `{` of `text` that no `}` closed, `{{` aside.
fn open_reference(text: &str) -> Option<String> {
    let mut open: Option<usize> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        match ch {
            '{' if chars.peek().map(|(_, next)| *next) == Some('{') => {
                chars.next();
            }
            '{' => open = Some(at + 1),
            '}' => open = None,
            _ => {}
        }
    }
    open.map(|start| text[start..].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_cursor(marked: &str) -> Located {
        let offset = marked.find('|').expect("a cursor");
        let text = marked.replacen('|', "", 1);
        locate(&text, offset)
    }

    #[test]
    fn a_statement_start_offers_nodes_inside_their_parents() {
        let found = at_cursor("Stack gap=1 {\n    Row {\n        Pro|");
        assert_eq!(found.path, ["Stack", "Row"]);
        assert_eq!(
            found.at,
            At::NodeName {
                prefix: "Pro".into()
            }
        );
        assert_eq!(
            at_cursor("|").at,
            At::NodeName {
                prefix: String::new()
            }
        );
        let closed = at_cursor("Stack {\n    Text \"a\"\n}\n|");
        assert!(closed.path.is_empty());
    }

    #[test]
    fn after_a_node_name_its_properties_are_offered() {
        let found = at_cursor("Stack {\n    Progress |");
        assert_eq!(found.node.as_deref(), Some("Progress"));
        assert_eq!(
            found.at,
            At::PropName {
                prefix: String::new()
            }
        );
        let typing = at_cursor("Progress ratio=0.5 ro|");
        assert_eq!(
            typing.at,
            At::PropName {
                prefix: "ro".into()
            }
        );
    }

    #[test]
    fn inside_a_value_the_property_is_known() {
        let found = at_cursor("Text \"a\" role=\"mu|");
        assert_eq!(found.node.as_deref(), Some("Text"));
        assert_eq!(
            found.at,
            At::Value {
                prop: Some("role".into()),
                prefix: "mu".into()
            }
        );
        let argument = at_cursor("Text \"hel|");
        assert_eq!(
            argument.at,
            At::Value {
                prop: None,
                prefix: "hel".into()
            }
        );
    }

    #[test]
    fn inside_braces_a_reference_is_being_written() {
        let found = at_cursor("Text \"{current.na|");
        assert_eq!(
            found.at,
            At::Reference {
                prop: None,
                prefix: "current.na".into()
            }
        );
        let lookup = at_cursor("Mark role=\"{tone:|\"");
        assert_eq!(
            lookup.at,
            At::Reference {
                prop: Some("role".into()),
                prefix: "tone:".into()
            }
        );
        let escaped = at_cursor("Text \"{{literal}} {|");
        assert_eq!(
            escaped.at,
            At::Reference {
                prop: None,
                prefix: String::new()
            }
        );
        let closed = at_cursor("Text \"{a} b|");
        assert_eq!(
            closed.at,
            At::Value {
                prop: None,
                prefix: "{a} b".into()
            }
        );
    }

    #[test]
    fn comments_strings_and_raw_strings_do_not_move_the_path() {
        let found = at_cursor(
            "// Stack {\nStack { /* { */ Text \"}\"; given json=#\"{\"a\": 1}\"#\n    Ro|",
        );
        assert_eq!(found.path, ["Stack"]);
        assert_eq!(
            found.at,
            At::NodeName {
                prefix: "Ro".into()
            }
        );
        assert_eq!(at_cursor("// a comm|").at, At::Nothing);
    }
}
