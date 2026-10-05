//! Markdown, rendered rather than shown.
//!
//! The contents mode already colours a `.md` file — syntect knows the
//! language — but colouring markup is not reading it. A heading stays a
//! line beginning with hashes, a link is still its own brackets and
//! parentheses, and a fenced block of Rust is one undifferentiated
//! string. This turns the document into what it describes.
//!
//! # It still describes; the host still draws
//!
//! Nothing here draws. It answers [`ContentLine`]s the same way the diff
//! does, so every rule the surface lives under holds: chrome colour is a
//! [`Role`], geometry is the host's, and the wrap is applied where the
//! line is laid out rather than guessed at here.
//!
//! Two things the vocabulary had to grow for it, both small and both
//! genuinely about meaning rather than about markdown: [`Span::italic`],
//! because emphasis has two weights and a role cannot carry the
//! difference, and nothing else.
//!
//! # Frontmatter is a table
//!
//! A `SKILL.md`, an agent definition or a docs page opens with a block of
//! YAML (or TOML) that is data about the document rather than part of it.
//! Shown as prose it is a horizontal rule followed by one long run of
//! `key: value` pairs; it is drawn instead as the key/value table it is.
//! It is read structurally and forgivingly, never parsed: a preview has
//! no business refusing a document, and the keys stay in the order the
//! author wrote them.
//!
//! # Why `pulldown-cmark`
//!
//! It is what rustdoc parses with, it is a pull parser (so a document is
//! a stream of events rather than a tree to walk twice), and with default
//! features off it brings one crate the workspace does not already have.
//! The terminal renderers — `termimad`, `tui-markdown` — were the obvious
//! reach and the wrong one: both *draw*, which is the one thing an
//! extension may not do.

mod frontmatter;

use std::path::Path;

use pulldown_cmark::{
    Alignment, CodeBlockKind, Event, HeadingLevel, MetadataBlockKind, Options, Parser, Tag, TagEnd,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    shared::highlight,
    view::{ContentLine, LineTone, Role, Span},
};

/// Whether `path` is a document this can render.
pub(crate) fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown")
        })
}

/// The document `text` describes, as lines.
///
/// `theme_name` is the host's syntax theme, used for fenced code blocks —
/// a block of Rust inside a README is highlighted as Rust, which is most
/// of what a preview is for in a repository.
///
/// `width` is the columns the host lays the document out in: a table is
/// the one block that cannot be wrapped after the fact, so it is fitted
/// here.
pub fn render(text: &str, theme_name: &str, width: usize) -> Vec<ContentLine> {
    let mut options = Options::empty();
    // The three GitHub extensions a README actually uses. They are
    // parser options rather than cargo features, so they cost nothing.
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    options.insert(Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS);

    let mut document = Document::new(theme_name, width);
    for event in Parser::new_ext(text, options) {
        document.absorb(event);
    }
    document.finish()
}

/// What the current run of text looks like.
#[derive(Clone, Copy, Default)]
struct Emphasis {
    bold: bool,
    italic: bool,
    struck: bool,
    /// Inside a link's text: a reference, drawn in the hue inline code
    /// wears, since both point at something rather than say it.
    link: bool,
}

/// The document being built, one event at a time.
struct Document {
    theme: String,
    width: usize,
    lines: Vec<ContentLine>,
    /// The line being assembled. Flushed at every block boundary.
    pending: Vec<Span>,
    emphasis: Emphasis,
    /// How deep in lists we are, and whether each is numbered — the
    /// counter is the list's, so a nested list does not disturb it.
    lists: Vec<Option<u64>>,
    /// Block quotes are cumulative: a quote inside a quote indents twice.
    quotes: usize,
    /// The language of the fenced block being read, when inside one.
    fence: Option<String>,
    /// A table being collected: its rows, and the row being filled.
    table: Option<Table>,
    /// Set between a heading's start and end, so its text is styled as
    /// one rather than span by span.
    heading: Option<HeadingLevel>,
    /// The frontmatter being collected, and the syntax it is written in.
    metadata: Option<(MetadataBlockKind, String)>,
    /// Whether an HTML comment opened on an earlier line is still open.
    in_comment: bool,
}

/// A cell keeps its spans, so inline code or emphasis inside a table is
/// drawn inside the table rather than after it.
type Cell = Vec<Span>;

#[derive(Default)]
struct Table {
    alignments: Vec<Alignment>,
    rows: Vec<Vec<Cell>>,
    row: Vec<Cell>,
    cell: Cell,
}

impl Document {
    fn new(theme: &str, width: usize) -> Self {
        Self {
            theme: theme.to_owned(),
            width,
            lines: Vec::new(),
            pending: Vec::new(),
            emphasis: Emphasis::default(),
            lists: Vec::new(),
            quotes: 0,
            fence: None,
            table: None,
            heading: None,
            metadata: None,
            in_comment: false,
        }
    }

    fn finish(mut self) -> Vec<ContentLine> {
        self.flush();
        if self.lines.is_empty() {
            self.lines.push(blank());
        }
        self.lines
    }

    fn absorb(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => {
                // Inline code keeps its backticks' meaning without their
                // characters: it is the one run of text that is quoted
                // rather than emphasised, so it wears the hue that
                // classifies rather than the accent — under a monochrome
                // theme the accent is the ink, and a quoted name drawn in
                // it read as nothing at all.
                self.push(Span::new(code.to_string(), Role::Info));
            }
            Event::SoftBreak => self.push(Span::new(" ", Role::Default)),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.flush();
                self.lines.push(ContentLine {
                    gutter: " ".to_owned(),
                    number: String::new(),
                    tone: LineTone::Neutral,
                    spans: vec![Span::new("─".repeat(48), Role::Faint)],
                });
                self.lines.push(blank());
            }
            Event::TaskListMarker(done) => {
                self.push(Span::new(
                    if done { "[x] " } else { "[ ] " },
                    if done { Role::Success } else { Role::Muted },
                ));
            }
            // Raw HTML, footnotes and the rest are shown as the source
            // says them rather than dropped: a preview that silently
            // loses part of a document is worse than one that shows it
            // plainly.
            Event::Html(raw) => self.html_block(&raw),
            Event::InlineHtml(raw) => {
                let comment = raw.starts_with("<!--");
                let span = Span::new(raw.to_string(), Role::Faint);
                self.push(if comment { span.italic() } else { span });
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush();
                self.heading = Some(level);
            }
            Tag::Paragraph => self.flush(),
            Tag::Emphasis => self.emphasis.italic = true,
            Tag::Strong => self.emphasis.bold = true,
            Tag::Strikethrough => self.emphasis.struck = true,
            Tag::Link { .. } => self.emphasis.link = true,
            Tag::BlockQuote(_) => {
                self.flush();
                self.quotes += 1;
            }
            Tag::List(first) => {
                self.flush();
                self.lists.push(first);
            }
            Tag::Item => {
                self.flush();
                let depth = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut() {
                    Some(Some(number)) => {
                        let marker = format!("{number}. ");
                        *number += 1;
                        marker
                    }
                    _ => "• ".to_owned(),
                };
                self.pending.push(Span::new(
                    format!("{}{marker}", "  ".repeat(depth)),
                    Role::Muted,
                ));
            }
            Tag::CodeBlock(kind) => {
                self.flush();
                self.fence = Some(match kind {
                    CodeBlockKind::Fenced(language) => language.to_string(),
                    CodeBlockKind::Indented => String::new(),
                });
            }
            Tag::Table(alignments) => {
                self.flush();
                self.table = Some(Table {
                    alignments,
                    ..Table::default()
                });
            }
            Tag::MetadataBlock(kind) => {
                self.flush();
                self.metadata = Some((kind, String::new()));
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.flush();
                self.lines.push(blank());
                self.heading = None;
            }
            TagEnd::Paragraph => {
                self.flush();
                self.lines.push(blank());
            }
            TagEnd::Emphasis => self.emphasis.italic = false,
            TagEnd::Strong => self.emphasis.bold = false,
            TagEnd::Strikethrough => self.emphasis.struck = false,
            TagEnd::Link => self.emphasis.link = false,
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quotes = self.quotes.saturating_sub(1);
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
                if self.lists.is_empty() {
                    self.lines.push(blank());
                }
            }
            TagEnd::Item => self.flush(),
            TagEnd::CodeBlock => {
                self.fence = None;
                self.lines.push(blank());
            }
            TagEnd::Table => self.finish_table(),
            TagEnd::MetadataBlock(_) => self.finish_metadata(),
            TagEnd::HtmlBlock => {
                self.flush();
                self.lines.push(blank());
            }
            TagEnd::TableCell => {
                if let Some(table) = self.table.as_mut() {
                    let cell = std::mem::take(&mut table.cell);
                    table.row.push(cell);
                }
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    let row = std::mem::take(&mut table.row);
                    table.rows.push(row);
                }
            }
            _ => {}
        }
    }

    fn text(&mut self, text: &str) {
        if let Some((_, source)) = self.metadata.as_mut() {
            source.push_str(text);
            return;
        }
        if let Some(language) = self.fence.clone() {
            self.push_code(text, &language);
            return;
        }
        self.push(Span::new(text.to_owned(), Role::Default));
    }

    /// A fenced block, highlighted as whatever it says it is. One
    /// highlighter for the whole block, because syntect carries state
    /// across lines and a block is one stream.
    fn push_code(&mut self, text: &str, language: &str) {
        let mut highlighter = highlight::highlighter_for_language(language, &self.theme);
        for line in text.lines() {
            let mut spans = vec![Span::new("  ".to_owned(), Role::Default)];
            spans.extend(
                highlight::line(&mut highlighter, line, &self.theme)
                    .into_iter()
                    .map(|(colour, piece)| Span::new(piece, Role::Default).coloured(colour)),
            );
            self.lines.push(ContentLine {
                gutter: " ".to_owned(),
                number: String::new(),
                tone: LineTone::Neutral,
                spans,
            });
        }
    }

    /// Adds a run of text to the line being built, wearing whatever
    /// emphasis is currently open.
    fn push(&mut self, span: Span) {
        let mut span = span;
        if let Some(level) = self.heading {
            span.bold = true;
            // Weight and brightness carry the hierarchy, not hue: the
            // accent is the ink under a monochrome theme and would put a
            // heading level on a par with body text.
            span.role = match level {
                HeadingLevel::H1 | HeadingLevel::H2 => Role::Bright,
                _ => Role::Secondary,
            };
        } else {
            span.bold |= self.emphasis.bold;
            span.italic |= self.emphasis.italic;
            if span.role == Role::Default {
                if self.emphasis.link {
                    span.role = Role::Info;
                } else if self.emphasis.bold {
                    span.role = Role::Bright;
                }
            }
            if self.emphasis.struck {
                // No strikethrough in the vocabulary, and one variant for
                // one extension is not the bar: dimming says "this no
                // longer counts", which is what the markup means.
                span.role = Role::Faint;
            }
        }
        if let Some(table) = self.table.as_mut() {
            table.cell.push(span);
            return;
        }
        if self.pending.is_empty() && self.quotes > 0 {
            self.pending
                .push(Span::new("│ ".repeat(self.quotes), Role::Dim));
        }
        self.pending.push(span);
    }

    fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let spans = std::mem::take(&mut self.pending);
        self.lines.push(ContentLine {
            gutter: " ".to_owned(),
            number: String::new(),
            tone: LineTone::Neutral,
            spans,
        });
    }

    /// A block of raw HTML, a line at a time: its source arrives with
    /// its newlines, and a span carrying one draws as a broken row. A
    /// comment is set apart from markup by slant, the way a highlighter
    /// sets a code comment apart.
    fn html_block(&mut self, raw: &str) {
        for line in raw.lines() {
            let opens = line.trim_start().starts_with("<!--");
            let comment = self.in_comment || opens;
            if opens || self.in_comment {
                self.in_comment = !line.contains("-->");
            }
            let span = Span::new(line.to_owned(), Role::Faint);
            self.push(if comment { span.italic() } else { span });
            self.flush();
        }
    }

    /// A table, once every cell is in: columns as wide as their widest
    /// cell, so a table reads as one.
    fn finish_table(&mut self) {
        let Some(mut table) = self.table.take() else {
            return;
        };
        // The head is the row that names the columns, so it is the one
        // that is emphasised — the separator markdown writes under it is
        // layout the parser already consumed.
        if let Some(head) = table.rows.first_mut() {
            for span in head.iter_mut().flatten() {
                span.bold = true;
                if span.role == Role::Default {
                    span.role = Role::Secondary;
                }
            }
        }
        // Every row is a group of its own: a folded cell makes a row more
        // than one line tall, and only the rule says where it ends.
        self.grid(&table.rows, |_| true, &table.alignments);
        self.lines.push(blank());
    }

    /// The frontmatter, as the key/value table it is.
    fn finish_metadata(&mut self) {
        let Some((kind, source)) = self.metadata.take() else {
            return;
        };
        let entries = match kind {
            MetadataBlockKind::YamlStyle => frontmatter::yaml(&source),
            MetadataBlockKind::PlusesStyle => frontmatter::toml(&source),
        };
        if entries.is_empty() {
            return;
        }
        // A top-level key and the keys nested under it are one group.
        let groups: Vec<bool> = entries.iter().map(|entry| entry.depth == 0).collect();
        let rows: Vec<Vec<Cell>> = entries
            .into_iter()
            .map(|entry| {
                let key = format!("{}{}", "  ".repeat(entry.depth), entry.key);
                let role = frontmatter::value_role(&entry.value);
                vec![
                    vec![Span::new(key, Role::Secondary)],
                    vec![Span::new(entry.value, role)],
                ]
            })
            .collect();
        self.grid(&rows, |index| groups[index], &[]);
        self.lines.push(blank());
    }

    /// `rows` in a box, a rule above each row `starts_group` names (the
    /// first excepted), fitted to the width the document is laid out in: a
    /// column that does not fit folds its cells onto more rows, because a
    /// row the host has to wrap is a row that breaks the box.
    fn grid(
        &mut self,
        rows: &[Vec<Cell>],
        starts_group: impl Fn(usize) -> bool,
        alignments: &[Alignment],
    ) {
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns == 0 {
            return;
        }
        let natural: Vec<usize> = (0..columns)
            .map(|column| {
                rows.iter()
                    .filter_map(|row| row.get(column))
                    .map(|cell| cell_width(cell))
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        // `│ ` before each cell, ` │` after the last, ` ` between.
        let chrome = 3 * columns + 1;
        let widths = fit_columns(&natural, self.width.saturating_sub(chrome));
        let edge = |left: &str, middle: &str, right: &str| {
            let inner = widths
                .iter()
                .map(|width| "─".repeat(width + 2))
                .collect::<Vec<_>>()
                .join(middle);
            border(format!("{left}{inner}{right}"))
        };
        self.lines.push(edge("╭", "┬", "╮"));
        for (index, row) in rows.iter().enumerate() {
            if index > 0 && starts_group(index) {
                self.lines.push(edge("├", "┼", "┤"));
            }
            let folded: Vec<Vec<Cell>> = widths
                .iter()
                .enumerate()
                .map(|(column, &width)| {
                    let cell = row.get(column).map(Vec::as_slice).unwrap_or_default();
                    fold(cell, width)
                })
                .collect();
            let height = folded.iter().map(Vec::len).max().unwrap_or(1);
            for line in 0..height {
                let mut spans = Vec::new();
                for (column, &width) in widths.iter().enumerate() {
                    spans.push(Span::new(
                        if column == 0 { "│ " } else { " │ " },
                        Role::Faint,
                    ));
                    let piece = folded[column]
                        .get(line)
                        .map(Vec::as_slice)
                        .unwrap_or_default();
                    let slack = width.saturating_sub(cell_width(piece));
                    let (before, after) = match alignments.get(column) {
                        Some(Alignment::Right) => (slack, 0),
                        Some(Alignment::Center) => (slack / 2, slack - slack / 2),
                        _ => (0, slack),
                    };
                    if before > 0 {
                        spans.push(Span::new(" ".repeat(before), Role::Default));
                    }
                    spans.extend(piece.iter().cloned());
                    if after > 0 {
                        spans.push(Span::new(" ".repeat(after), Role::Default));
                    }
                }
                spans.push(Span::new(" │", Role::Faint));
                self.lines.push(ContentLine {
                    gutter: " ".to_owned(),
                    number: String::new(),
                    tone: LineTone::Neutral,
                    spans,
                });
            }
        }
        self.lines.push(edge("╰", "┴", "╯"));
    }
}

/// The narrowest a column is squeezed to before the table stops fitting
/// and is drawn at its natural width for the host to wrap.
const MIN_COLUMN_WIDTH: usize = 6;

/// Column widths that sum to no more than `room`. Narrow columns keep
/// their natural width and the wide ones share what is left equally — so
/// a frontmatter's keys stay on one line and its values fold.
fn fit_columns(natural: &[usize], room: usize) -> Vec<usize> {
    if natural.iter().sum::<usize>() <= room || room < MIN_COLUMN_WIDTH * natural.len() {
        return natural.to_vec();
    }
    let mut order: Vec<usize> = (0..natural.len()).collect();
    order.sort_by_key(|&column| natural[column]);
    let mut widths = vec![0; natural.len()];
    let mut left = room;
    for (taken, &column) in order.iter().enumerate() {
        let share = left / (natural.len() - taken);
        widths[column] = natural[column].min(share).max(MIN_COLUMN_WIDTH);
        left = left.saturating_sub(widths[column]);
    }
    widths
}

fn cell_width(cell: &[Span]) -> usize {
    cell.iter().map(|span| span.text.width()).sum()
}

/// `cell` folded at word boundaries into lines no wider than `width`,
/// each word keeping the look of the span it came from. A word wider than
/// a whole line is cut, since the box cannot grow to hold it.
fn fold(cell: &[Span], width: usize) -> Vec<Cell> {
    let width = width.max(1);
    let mut lines: Vec<Cell> = Vec::new();
    let mut line: Cell = Vec::new();
    let mut used = 0;
    // A cell's leading indentation is meaning — a frontmatter key nested
    // under another — so it is kept, once, ahead of the first line.
    if let Some(first) = cell.first() {
        let indent = first.text.len() - first.text.trim_start_matches(' ').len();
        if indent > 0 && indent < width {
            line.push(Span {
                text: " ".repeat(indent),
                ..first.clone()
            });
            used = indent;
        }
    }
    let indented = used > 0;
    // Whether the source had a space before the next word: two spans that
    // meet without one (`**a**b`) are one word and must not be parted.
    let mut spaced = false;
    for (position, span) in cell.iter().enumerate() {
        spaced |= span.text.starts_with(char::is_whitespace) && !(indented && position == 0);
        for (index, word) in span.text.split_whitespace().enumerate() {
            let mut word = word.to_owned();
            let spaced_here = spaced || index > 0;
            loop {
                let gap = usize::from(used > 0 && spaced_here);
                if used + gap + word.width() <= width {
                    if gap == 1 {
                        line.push(Span::new(" ", Role::Default));
                    }
                    used += gap + word.width();
                    line.push(Span {
                        text: word,
                        ..span.clone()
                    });
                    break;
                }
                if used > 0 {
                    lines.push(std::mem::take(&mut line));
                    used = 0;
                    continue;
                }
                let cut = cut_at(&word, width);
                let rest = word.split_off(cut);
                lines.push(vec![Span {
                    text: word,
                    ..span.clone()
                }]);
                word = rest;
            }
        }
        spaced = span.text.ends_with(char::is_whitespace);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// The byte index where `word`'s first `width` columns end.
fn cut_at(word: &str, width: usize) -> usize {
    let mut used = 0;
    for (index, character) in word.char_indices() {
        let columns = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
        if used + columns > width && index > 0 {
            return index;
        }
        used += columns;
    }
    word.len()
}

fn border(text: String) -> ContentLine {
    ContentLine {
        gutter: " ".to_owned(),
        number: String::new(),
        tone: LineTone::Neutral,
        spans: vec![Span::new(text, Role::Faint)],
    }
}

fn blank() -> ContentLine {
    ContentLine {
        gutter: " ".to_owned(),
        number: String::new(),
        tone: LineTone::Neutral,
        spans: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::highlight::FALLBACK_SYNTAX_THEME;

    fn rendered(source: &str) -> Vec<ContentLine> {
        render(source, FALLBACK_SYNTAX_THEME, 100)
    }

    fn text_of(line: &ContentLine) -> String {
        line.spans.iter().map(|span| span.text.as_str()).collect()
    }

    fn lines_of(source: &str) -> Vec<String> {
        rendered(source).iter().map(text_of).collect()
    }

    #[test]
    fn a_document_is_only_a_markdown_one_by_its_extension() {
        assert!(is_markdown(Path::new("README.md")));
        assert!(is_markdown(Path::new("notes.MARKDOWN")));
        assert!(!is_markdown(Path::new("main.rs")));
        assert!(!is_markdown(Path::new("mdbook")));
    }

    /// The point of the mode: the markup stops being text and starts
    /// being what it describes.
    #[test]
    fn the_markup_becomes_the_document_rather_than_staying_on_screen() {
        let lines = lines_of("# Title\n\nSome **bold** and *thin* words.\n");

        assert!(
            lines.iter().any(|line| line == "Title"),
            "a heading loses its hashes: {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("bold") && !line.contains('*')),
            "and emphasis loses its asterisks: {lines:?}"
        );
    }

    #[test]
    fn each_kind_of_text_wears_the_role_of_what_it_is() {
        let lines = rendered(
            "## Steps\n\n- [x] call `reconcile` from [the guide](https://x.dev) **now**\n- [ ] then ship\n",
        );
        let spans: Vec<&Span> = lines.iter().flat_map(|line| &line.spans).collect();
        let role_of = |text: &str| {
            spans
                .iter()
                .find(|span| span.text.trim() == text)
                .map(|span| span.role)
                .unwrap_or_else(|| panic!("{text:?} is drawn: {spans:?}"))
        };

        assert_eq!(
            role_of("Steps"),
            Role::Bright,
            "a section heading is bright, not the accent"
        );
        assert_eq!(role_of("reconcile"), Role::Info, "inline code classifies");
        assert_eq!(
            role_of("the guide"),
            Role::Info,
            "and so does a link's text"
        );
        assert_eq!(
            role_of("now"),
            Role::Bright,
            "strong text is brighter as well as heavier"
        );
        assert_eq!(role_of("[x]"), Role::Success, "a done step is done");
        assert_eq!(role_of("[ ]"), Role::Muted, "an open one waits");
        assert_eq!(
            role_of("•"),
            Role::Muted,
            "and the marker stays muted: the host hangs a wrapped item under it by that role"
        );
    }

    #[test]
    fn emphasis_survives_as_weight_rather_than_as_punctuation() {
        let lines = rendered("Some **bold** and *thin* words.\n");
        let spans: Vec<&Span> = lines.iter().flat_map(|line| &line.spans).collect();

        assert!(
            spans.iter().any(|span| span.text == "bold" && span.bold),
            "strong is bold"
        );
        assert!(
            spans.iter().any(|span| span.text == "thin" && span.italic),
            "and emphasis is italic — a role could not carry the difference"
        );
    }

    /// A README's code block is most of what a preview is for in a
    /// repository, so it is highlighted as the language it says it is.
    #[test]
    fn a_fenced_block_is_highlighted_as_what_it_says_it_is() {
        let lines = rendered("```rust\nfn main() {}\n```\n");
        let coloured = lines
            .iter()
            .flat_map(|line| &line.spans)
            .filter(|span| span.color.is_some())
            .count();

        assert!(coloured > 1, "the block is coloured as Rust, not as prose");
        assert!(
            lines
                .iter()
                .map(text_of)
                .any(|line| line.contains("fn main")),
            "and its text survives"
        );
    }

    #[test]
    fn a_list_is_marked_and_a_nested_one_is_indented() {
        let lines = lines_of("- one\n- two\n  - deep\n");

        assert!(lines.iter().any(|line| line == "• one"), "{lines:?}");
        assert!(lines.iter().any(|line| line == "  • deep"), "{lines:?}");
    }

    #[test]
    fn a_numbered_list_counts_and_a_nested_one_does_not_disturb_it() {
        let lines = lines_of("1. one\n2. two\n");
        assert!(lines.iter().any(|line| line == "1. one"), "{lines:?}");
        assert!(lines.iter().any(|line| line == "2. two"), "{lines:?}");
    }

    #[test]
    fn a_table_lines_its_columns_up() {
        let lines = lines_of("| a | long header |\n|---|---|\n| x | y |\n");
        let header = lines
            .iter()
            .find(|line| line.contains("long header"))
            .expect("the head is drawn");
        let body = lines
            .iter()
            .find(|line| line.starts_with("│ x"))
            .expect("the body is drawn");

        assert_eq!(
            header.chars().count(),
            body.chars().count(),
            "columns as wide as their widest cell, so the rows line up"
        );
    }

    /// Inline code and emphasis are part of the cell they are written in,
    /// not text that happens to follow the table.
    #[test]
    fn a_cell_keeps_its_inline_markup_inside_the_table() {
        let lines = rendered("| a | b |\n|---|---|\n| `code` | **bold** |\n");
        let row = lines
            .iter()
            .find(|line| text_of(line).contains("code"))
            .expect("the row is drawn");

        assert!(text_of(row).contains("bold"), "both cells on one row");
        assert!(
            row.spans
                .iter()
                .any(|span| span.text == "code" && span.role == Role::Info),
            "inline code is still quoted"
        );
        assert!(
            row.spans
                .iter()
                .any(|span| span.text == "bold" && span.bold),
            "and emphasis still has its weight"
        );
    }

    #[test]
    fn a_column_is_aligned_the_way_its_separator_says() {
        let lines = lines_of("| n |\n|--:|\n| 1 |\n| 100 |\n");
        assert!(lines.iter().any(|line| line == "│   1 │"), "{lines:?}");
    }

    /// Frontmatter is data about the document, so it is drawn as the
    /// table it is rather than as a rule and a run-on line of prose.
    #[test]
    fn frontmatter_is_drawn_as_a_key_value_table() {
        let lines = lines_of("---\nname: greet\ninvoke:\n  model: true\n---\n\n# Greet\n");

        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("│ name ") && line.contains("greet")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("│   model ") && line.contains("true")),
            "a nested key is indented under its parent: {lines:?}"
        );
        assert!(
            !lines.iter().any(|line| line.starts_with('─')),
            "no stray rule where the fences were: {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line == "Greet"),
            "the body follows: {lines:?}"
        );
    }

    /// A rule ends each group, so a folded value reads as one entry: a
    /// top-level key starts a group, and the keys nested under it stay in
    /// it.
    #[test]
    fn a_rule_parts_frontmatter_groups_and_not_nested_keys() {
        let lines = lines_of("---\nname: greet\ninvoke:\n  model: true\nslash: true\n---\n");
        let row = |key: &str| {
            lines
                .iter()
                .position(|line| line.starts_with(&format!("│ {key} ")))
                .unwrap_or_else(|| panic!("{key}: {lines:?}"))
        };

        assert!(lines[row("name") + 1].starts_with('├'), "{lines:?}");
        assert_eq!(row("invoke") + 1, row("  model"), "{lines:?}");
        assert!(lines[row("  model") + 1].starts_with('├'), "{lines:?}");
    }

    /// The box only survives if no row of it is wider than the pane, so a
    /// long value folds inside its own cell.
    #[test]
    fn a_long_frontmatter_value_folds_inside_its_cell() {
        let description = "word ".repeat(40);
        let lines = lines_of(&format!("---\ndescription: {description}\n---\n"));
        let rows: Vec<&String> = lines.iter().filter(|line| line.starts_with('│')).collect();

        assert!(rows.len() > 1, "{lines:?}");
        let width = rows[0].chars().count();
        assert!(
            rows.iter().all(|row| row.chars().count() == width),
            "{lines:?}"
        );
    }

    /// The table follows the room it is drawn in: no row wider than the
    /// width it was given, and more of it used when there is more.
    #[test]
    fn a_table_is_fitted_to_the_width_it_is_given() {
        let source = format!(
            "---\nname: architect\ndescription: {}\n---\n\n| a | b |\n|---|---|\n| {} | {} |\n",
            "word ".repeat(60),
            "left ".repeat(20),
            "right ".repeat(20),
        );
        let widest = |width: usize| {
            let lines: Vec<String> = render(&source, FALLBACK_SYNTAX_THEME, width)
                .iter()
                .map(text_of)
                .collect();
            for line in lines
                .iter()
                .filter(|line| line.starts_with(['│', '╭', '├', '╰']))
            {
                assert!(line.chars().count() <= width, "{width}: {line:?}");
            }
            lines
                .iter()
                .filter(|line| line.starts_with('╭'))
                .map(|line| line.chars().count())
                .max()
                .unwrap_or_default()
        };

        let narrow = widest(48);
        let wide = widest(140);
        assert!(
            wide > narrow,
            "a wider pane gets a wider table: {narrow} vs {wide}"
        );
        assert_eq!(wide, 140, "a long value fills the room there is");
    }

    #[test]
    fn folding_a_cell_keeps_words_the_markup_glued_together() {
        let cell = rendered(&format!("| h |\n|---|\n| **a**b {} |\n", "x".repeat(10)));
        let row = cell
            .iter()
            .map(text_of)
            .find(|line| line.contains("ab"))
            .unwrap_or_default();
        assert!(row.contains("ab "), "{row:?}");
    }

    #[test]
    fn toml_frontmatter_is_a_table_too() {
        let lines = lines_of("+++\ntitle = \"Hi\"\n+++\n");
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("│ title ") && line.contains("Hi")),
            "{lines:?}"
        );
    }

    /// An HTML comment spanning lines arrives as one event with its
    /// newlines in it; each line is a row, set apart by slant.
    #[test]
    fn an_html_comment_is_a_row_per_line_and_reads_as_a_comment() {
        let lines = rendered("<!-- one\ntwo -->\n\ntext\n");
        let comment: Vec<&ContentLine> = lines
            .iter()
            .filter(|line| line.spans.iter().any(|span| span.italic))
            .collect();

        assert_eq!(
            comment.len(),
            2,
            "{:?}",
            lines.iter().map(text_of).collect::<Vec<_>>()
        );
        assert!(
            lines
                .iter()
                .flat_map(|line| &line.spans)
                .all(|span| !span.text.contains('\n')),
            "no span carries a newline"
        );
    }

    /// A comment inside a fenced block is coloured as one, including when
    /// the info string says more than the language.
    #[test]
    fn a_comment_in_a_fenced_block_is_highlighted() {
        for source in [
            "```rust\n// note\nfn main() {}\n```\n",
            "```rust,ignore\n// note\nfn main() {}\n```\n",
            "```shell\n# note\necho hi\n```\n",
        ] {
            let lines = rendered(source);
            let comment = lines
                .iter()
                .flat_map(|line| &line.spans)
                .find(|span| span.text.contains("note"))
                .and_then(|span| span.color);
            let code = lines
                .iter()
                .flat_map(|line| &line.spans)
                .find(|span| span.text.contains("fn") || span.text.contains("echo"))
                .and_then(|span| span.color);

            assert!(comment.is_some(), "{source}");
            assert_ne!(comment, code, "the comment is coloured apart in {source}");
        }
    }

    /// A preview that silently drops part of a document is worse than one
    /// that shows it plainly.
    #[test]
    fn raw_html_is_shown_rather_than_swallowed() {
        let lines = lines_of("<div align=\"center\">hi</div>\n");
        assert!(lines.iter().any(|line| line.contains("div")), "{lines:?}");
    }

    #[test]
    fn an_empty_document_still_has_a_line() {
        assert_eq!(rendered("").len(), 1);
    }
}
