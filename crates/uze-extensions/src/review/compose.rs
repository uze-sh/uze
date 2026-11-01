//! A comment being written, and a comment read: the text the operator
//! types under the lines they marked, and what that text looks like once
//! kept, its fenced blocks drawn as code.

use std::path::Path;

use super::record::Row;
use crate::view::{Caret, Rgb, Role, Span};

/// What an inline code span is drawn in: the badge hue, apart from the
/// prose around it and from every state colour.
const INLINE_CODE: Role = Role::Info;

/// A comment being written, under the lines it is about.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Composer {
    /// The text, a line each, never empty.
    pub(crate) lines: Vec<String>,
    /// Where the next character lands, counted in characters.
    pub(crate) caret: Caret,
    /// The lines the comment is about.
    pub(crate) rows: Vec<Row>,
    /// The text the operator selected in them, when they selected.
    pub(crate) excerpt: String,
    /// The comment being edited, when this is not a new one.
    pub(crate) editing: Option<u64>,
}

impl Composer {
    pub(crate) fn new(rows: Vec<Row>, excerpt: String) -> Self {
        Self {
            lines: vec![String::new()],
            rows,
            excerpt,
            ..Self::default()
        }
    }

    pub(crate) fn editing(id: u64, rows: Vec<Row>, excerpt: String, text: &str) -> Self {
        let lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
        let line = lines.len() - 1;
        let column = lines[line].chars().count();
        Self {
            lines,
            caret: Caret { line, column },
            rows,
            excerpt,
            editing: Some(id),
        }
    }

    pub(crate) fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub(crate) fn is_blank(&self) -> bool {
        self.lines.iter().all(|line| line.trim().is_empty())
    }

    fn byte_at(line: &str, column: usize) -> usize {
        line.char_indices()
            .nth(column)
            .map_or(line.len(), |(at, _)| at)
    }

    fn len(&self, line: usize) -> usize {
        self.lines[line].chars().count()
    }

    pub(crate) fn type_char(&mut self, character: char) {
        let Caret { line, column } = self.caret;
        let at = Self::byte_at(&self.lines[line], column);
        self.lines[line].insert(at, character);
        self.caret.column += 1;
    }

    pub(crate) fn insert(&mut self, text: &str) {
        for character in text.chars() {
            match character {
                '\n' => self.newline(),
                '\r' => {}
                character if character.is_control() => {}
                character => self.type_char(character),
            }
        }
    }

    pub(crate) fn newline(&mut self) {
        let Caret { line, column } = self.caret;
        let at = Self::byte_at(&self.lines[line], column);
        let rest = self.lines[line].split_off(at);
        self.lines.insert(line + 1, rest);
        self.caret = Caret {
            line: line + 1,
            column: 0,
        };
    }

    pub(crate) fn erase_back(&mut self) {
        let Caret { line, column } = self.caret;
        if column > 0 {
            let at = Self::byte_at(&self.lines[line], column - 1);
            self.lines[line].remove(at);
            self.caret.column -= 1;
        } else if line > 0 {
            let rest = self.lines.remove(line);
            let column = self.len(line - 1);
            self.lines[line - 1].push_str(&rest);
            self.caret = Caret {
                line: line - 1,
                column,
            };
        }
    }

    pub(crate) fn erase_forward(&mut self) {
        let Caret { line, column } = self.caret;
        if column < self.len(line) {
            let at = Self::byte_at(&self.lines[line], column);
            self.lines[line].remove(at);
        } else if line + 1 < self.lines.len() {
            let next = self.lines.remove(line + 1);
            self.lines[line].push_str(&next);
        }
    }

    pub(crate) fn left(&mut self) {
        if self.caret.column > 0 {
            self.caret.column -= 1;
        } else if self.caret.line > 0 {
            self.caret.line -= 1;
            self.caret.column = self.len(self.caret.line);
        }
    }

    pub(crate) fn right(&mut self) {
        if self.caret.column < self.len(self.caret.line) {
            self.caret.column += 1;
        } else if self.caret.line + 1 < self.lines.len() {
            self.caret.line += 1;
            self.caret.column = 0;
        }
    }

    pub(crate) fn up(&mut self) {
        if self.caret.line > 0 {
            self.caret.line -= 1;
            self.caret.column = self.caret.column.min(self.len(self.caret.line));
        }
    }

    pub(crate) fn down(&mut self) {
        if self.caret.line + 1 < self.lines.len() {
            self.caret.line += 1;
            self.caret.column = self.caret.column.min(self.len(self.caret.line));
        }
    }

    pub(crate) fn line_start(&mut self) {
        self.caret.column = 0;
    }

    pub(crate) fn line_end(&mut self) {
        self.caret.column = self.len(self.caret.line);
    }

    pub(crate) fn place(&mut self, line: usize, column: usize) {
        let line = line.min(self.lines.len() - 1);
        self.caret = Caret {
            line,
            column: column.min(self.len(line)),
        };
    }

    /// The marked lines, as they read now, fenced as a block on lines of
    /// their own at the caret: what the operator then edits into what the
    /// lines should say.
    pub(crate) fn insert_block(&mut self) {
        let code: Vec<String> = self
            .rows
            .iter()
            .filter(|row| row.kind != super::record::RowKind::Removed)
            .map(|row| row.text.clone())
            .collect();
        if !self.lines[self.caret.line].is_empty() {
            self.line_end();
            self.newline();
        }
        self.insert("```\n");
        for line in &code {
            self.insert(line);
            self.newline();
        }
        self.insert("```");
        self.newline();
    }

    /// A pair of backticks with the caret between them.
    pub(crate) fn insert_inline(&mut self) {
        self.insert("``");
        self.left();
    }
}

/// One line of a kept comment as it is drawn.
pub(crate) type Rich = Vec<Span>;

/// `text` as it is drawn: a fenced block is one line of code per line,
/// coloured as the file at `path` is, and a backticked span is code within
/// the prose. Control and escape characters never reach the terminal.
pub(crate) fn rich(text: &str, path: &Path, theme: &str) -> Vec<Rich> {
    let mut drawn = Vec::new();
    let mut fenced: Option<Vec<String>> = None;
    for line in text.split('\n') {
        let line = clean(line);
        match fenced.as_mut() {
            None if line.trim_start().starts_with("```") => fenced = Some(Vec::new()),
            None => drawn.push(prose(&line)),
            Some(_) if line.trim_start().starts_with("```") => {
                drawn.extend(code(&fenced.take().unwrap_or_default(), path, theme));
            }
            Some(block) => block.push(line),
        }
    }
    if let Some(block) = fenced {
        drawn.extend(code(&block, path, theme));
    }
    drawn
}

/// `text` with every control character taken out: what was typed in a
/// terminal, or answered by an agent, is drawn as text and nothing else.
pub(crate) fn clean(text: &str) -> String {
    text.chars()
        .map(|character| if character == '\t' { ' ' } else { character })
        .filter(|character| !character.is_control())
        .collect()
}

fn prose(line: &str) -> Rich {
    let mut spans = Vec::new();
    for (at, part) in line.split('`').enumerate() {
        if part.is_empty() {
            continue;
        }
        let role = if at % 2 == 1 {
            INLINE_CODE
        } else {
            Role::Default
        };
        spans.push(Span::new(part, role));
    }
    if spans.is_empty() {
        spans.push(Span::new(String::new(), Role::Default));
    }
    spans
}

fn code(block: &[String], path: &Path, theme: &str) -> Vec<Rich> {
    let mut highlighter = crate::shared::highlight::highlighter(path, None, theme);
    block
        .iter()
        .map(|line| {
            let coloured: Vec<(Rgb, String)> =
                crate::shared::highlight::line(&mut highlighter, line, theme);
            let mut spans: Rich = coloured
                .into_iter()
                .map(|(colour, text)| Span::new(text, Role::Default).coloured(colour))
                .collect();
            if spans.is_empty() {
                spans.push(Span::new(String::new(), Role::Default));
            }
            for span in &mut spans {
                span.ground = Some(Role::Faint);
            }
            spans
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::record::RowKind;

    fn typed(composer: &mut Composer, text: &str) {
        composer.insert(text);
    }

    #[test]
    fn typing_splits_and_joins_lines_at_the_caret() {
        let mut composer = Composer::new(Vec::new(), String::new());
        typed(&mut composer, "ab\ncd");
        assert_eq!(composer.lines, ["ab", "cd"]);
        composer.line_start();
        composer.erase_back();
        assert_eq!(composer.lines, ["abcd"]);
        assert_eq!(composer.caret, Caret { line: 0, column: 2 });
        composer.erase_forward();
        assert_eq!(composer.text(), "abd");
    }

    #[test]
    fn the_marked_lines_become_a_fenced_block_without_their_removals() {
        let rows = vec![
            Row {
                kind: RowKind::Removed,
                line: 3,
                text: "old();".to_owned(),
            },
            Row {
                kind: RowKind::Added,
                line: 3,
                text: "new();".to_owned(),
            },
        ];
        let mut composer = Composer::new(rows, String::new());
        typed(&mut composer, "Rather:");
        composer.insert_block();
        assert_eq!(composer.text(), "Rather:\n```\nnew();\n```\n");
    }

    #[test]
    fn inline_code_leaves_the_caret_between_the_backticks() {
        let mut composer = Composer::new(Vec::new(), String::new());
        composer.insert_inline();
        typed(&mut composer, "x");
        assert_eq!(composer.text(), "`x`");
    }

    #[test]
    fn a_fenced_block_is_drawn_as_code_and_a_backticked_span_as_inline_code() {
        let drawn = rich(
            "Use `rev-parse`:\n```\nlet a = 1;\n```",
            Path::new("a.rs"),
            crate::shared::highlight::FALLBACK_SYNTAX_THEME,
        );
        assert_eq!(drawn.len(), 2, "one prose line and one code line");
        assert!(
            drawn[0]
                .iter()
                .any(|span| span.text == "rev-parse" && span.role == INLINE_CODE)
        );
        assert!(drawn[1].iter().all(|span| span.ground == Some(Role::Faint)));
        assert_eq!(
            drawn[1]
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>(),
            "let a = 1;"
        );
    }

    #[test]
    fn an_escape_sequence_is_drawn_as_text() {
        let drawn = rich("done\u{1b}[2J now", Path::new("a.rs"), "x");
        let text: String = drawn[0].iter().map(|span| span.text.as_str()).collect();
        assert_eq!(text, "done[2J now");
    }
}
