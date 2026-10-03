//! Prose: a content line's roles as colour, and text folded to a width with its hanging indent.

use super::*;

/// The extension's palette, resolved. An extension names meaning; the host
/// names colour, exactly once, here.
pub(super) fn color(role: Role) -> Color {
    theme::color(token(role))
}

/// The token a role means. Kept apart from [`color`] because a role is
/// asked for two different things — the ink it draws in, and the hue it
/// lends a ground — and both have to answer from one mapping or a
/// selection stops matching what it is marking.
pub(super) fn token(role: Role) -> Token {
    match role {
        Role::Default => Token::TextBright,
        Role::Muted => Token::TextMuted,
        Role::Secondary => Token::TextSecondary,
        Role::Bright => Token::TextBright,
        Role::Inactive => Token::TextInactive,
        Role::Accent => Token::Accent,
        Role::Dim => Token::TextDim,
        Role::Faint => Token::TextFaint,
        Role::Info => Token::StateInfo,
        Role::Success => Token::StateSuccess,
        Role::Warning => Token::StateWarning,
        Role::Danger => Token::StateDanger,
    }
}

/// Rendered Markdown as lines a wrapped `Paragraph` can draw, styled the
/// way the code surface styles its preview.
pub(crate) fn prose(lines: &[ContentLine]) -> Vec<Line<'static>> {
    lines
        .iter()
        .map(|line| Line::from(line.spans.iter().map(styled).collect::<Vec<_>>()))
        .collect()
}

/// Rendered Markdown folded to `width` the way prose is read: at word
/// boundaries, with each continuation hung under where its line's text
/// began.
///
/// Apart from [`fold`]'s word breaking, which is what content a reader
/// may mark goes through: these rows are drawn for the plugins drawer and
/// the release notes, where nothing is marked yet. A continuation that
/// starts back at the edge reads as a new item: under a bullet it should
/// sit under the bullet's text, in a quote it should still be quoted, and
/// in a code block it should sit clear of the block's own indentation, so
/// it reads as the same line carried on rather than as a line of its own.
pub(crate) fn prose_rows(line: &ContentLine, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let hang = hanging_indent(line, width);
    let hang_width = hang.iter().map(TextSpan::width).sum::<usize>();

    let mut rows: Vec<Vec<TextSpan<'static>>> = vec![Vec::new()];
    let mut used = 0usize;
    for (word, style) in words(line) {
        let taken = word.chars().map(cell_width).sum::<usize>();
        let is_space = word.starts_with(' ');
        let fits = used + taken <= width;
        if !fits && used > hang_width && !(rows.len() == 1 && used == 0) {
            if is_space {
                // The space a row ends on is where it broke; carried over,
                // it would indent the continuation by one.
                continue;
            }
            break_row(&mut rows, &hang);
            used = hang_width;
        }
        if is_space && used == hang_width && rows.len() > 1 {
            continue;
        }
        // A word longer than a whole row — a URL, a path — is broken at the
        // cell, since no row would ever hold it.
        let mut piece = String::new();
        for character in word.chars() {
            let cells = cell_width(character);
            if used + cells > width && used > hang_width {
                rows.last_mut()
                    .expect("a row was pushed above")
                    .push(TextSpan::styled(std::mem::take(&mut piece), style));
                break_row(&mut rows, &hang);
                used = hang_width;
            }
            match character {
                '\t' => piece.push_str(&" ".repeat(TAB_WIDTH)),
                _ => piece.push(character),
            }
            used += cells;
        }
        if !piece.is_empty() {
            rows.last_mut()
                .expect("a row was pushed above")
                .push(TextSpan::styled(piece, style));
        }
    }
    rows.into_iter().map(Line::from).collect()
}

/// Ends the row being filled and starts the next with `hang`. The spaces
/// the row ended on go with it: they are where it broke, and left in they
/// would carry a block's ground past its last word.
pub(super) fn break_row(rows: &mut Vec<Vec<TextSpan<'static>>>, hang: &[TextSpan<'static>]) {
    if let Some(row) = rows.last_mut() {
        while let Some(last) = row.last_mut() {
            let kept = last.content.trim_end_matches(' ').len();
            if kept > 0 {
                last.content.to_mut().truncate(kept);
                break;
            }
            row.pop();
        }
    }
    rows.push(hang.to_vec());
}

/// A line's runs, split into words and the spaces between them, each
/// carrying the style of the span it came from.
pub(super) fn words(line: &ContentLine) -> Vec<(String, Style)> {
    let mut words = Vec::new();
    for span in &line.spans {
        let style = styled(span).style;
        let mut current = String::new();
        let mut in_space = None;
        for character in span.text.chars() {
            let space = character == ' ';
            if in_space.is_some_and(|was| was != space) {
                words.push((std::mem::take(&mut current), style));
            }
            in_space = Some(space);
            current.push(character);
        }
        if !current.is_empty() {
            words.push((current, style));
        }
    }
    words
}

/// What a continuation of `line` starts with: blanks as wide as a list
/// item's marker, the quote's own decoration again, or a code line's
/// indentation and two more — nothing for a plain paragraph, and nothing when the hang would
/// leave no room for the text it is hung for.
pub(super) fn hanging_indent(line: &ContentLine, width: usize) -> Vec<TextSpan<'static>> {
    let Some(first) = line.spans.first() else {
        return Vec::new();
    };
    let is_code = line.spans.iter().any(|span| span.color.is_some());
    // A dimmed run opening the line is the renderer's own decoration — a
    // quote's bars — and the rows that carry the line on are just as
    // quoted.
    let hang = if matches!(first.role, Role::Dim) && !first.text.trim().is_empty() {
        return vec![styled(first)];
    } else if is_code {
        let text: String = line.spans.iter().map(|span| span.text.as_str()).collect();
        text.chars()
            .take_while(|character| *character == ' ')
            .count()
            + 2
    } else if matches!(first.role, Role::Muted) && is_list_marker(&first.text) {
        first.text.chars().map(cell_width).sum()
    } else {
        0
    };
    if hang == 0 || hang * 2 > width {
        return Vec::new();
    }
    vec![TextSpan::raw(" ".repeat(hang))]
}

/// `• `, `2. `, each after any nesting indent — what the renderer opens a
/// list item with.
pub(super) fn is_list_marker(text: &str) -> bool {
    let marker = text.trim_start();
    marker == "• "
        || marker
            .strip_suffix(". ")
            .is_some_and(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
}

pub(super) fn styled(span: &Span) -> TextSpan<'static> {
    let mut style = Style::default().fg(span
        .color
        .map(|rgb| theme::content(rgb.0, rgb.1, rgb.2))
        .unwrap_or_else(|| color(span.role)));
    // A ground the span named: its own hue, let into the surface far
    // enough to mark the area and not far enough to compete with what is
    // written on it — the same strength a selected row wears.
    if let Some(ground) = span.ground {
        style = style.bg(theme::tinted(token(ground), Token::SurfaceBackground));
    }
    // Emphasis is stated both ways round, never left to whatever the
    // span is drawn inside. A ratatui title carries a style its spans
    // are patched over, so a span that only *omits* bold comes out bold
    // on a frame's edge and plain everywhere else — the same span
    // meaning two things depending on where it landed.
    style = match span.bold {
        true => style.add_modifier(Modifier::BOLD),
        false => style.remove_modifier(Modifier::BOLD),
    };
    style = match span.italic {
        true => style.add_modifier(Modifier::ITALIC),
        false => style.remove_modifier(Modifier::ITALIC),
    };
    TextSpan::styled(without_tabs(&span.text), style)
}

/// `text` with each tab spelled as the [`TAB_WIDTH`] spaces it occupies.
///
/// ratatui drops control characters as it draws, so a tab left in is an
/// indentation that silently is not there — and a caret counted past it
/// sits a column short of the character it names.
pub(super) fn without_tabs(text: &str) -> String {
    match text.contains('\t') {
        true => text.replace('\t', &" ".repeat(TAB_WIDTH)),
        false => text.to_owned(),
    }
}

/// How many cells `character` takes: a tab its [`TAB_WIDTH`], anything
/// else what Unicode says, and never nothing — a zero-width character the
/// caret can stand on still needs a cell to be seen standing there.
pub(super) fn cell_width(character: char) -> usize {
    match character {
        '\t' => TAB_WIDTH,
        _ => unicode_width::UnicodeWidthChar::width(character)
            .unwrap_or(0)
            .max(1),
    }
}

/// Where a line that does not fit may be broken: code at any cell, since
/// every character of it is the text and a caret stands on each; prose
/// only between its words.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Breaks {
    Cells,
    Words,
}

impl Breaks {
    /// Numbered lines are code, and unnumbered ones prose — the same
    /// line [`gutter_width`] draws.
    pub(super) fn for_gutter(gutter: u16) -> Self {
        match gutter {
            0 => Self::Words,
            _ => Self::Cells,
        }
    }
}

/// Folds `line` at `width` cells, calling `visit` with the row and cell
/// each character lands on, and answers where the next one would.
///
/// One walk for everything that must agree about it — which row a
/// character is drawn on, how many rows the line takes, where the caret
/// stands and which character a pointer is over — because a paragraph
/// wrap that broke at words while the rest counted cells drew prose on
/// rows nothing else knew it was on.
///
/// Prose breaks before a word that would not fit, and a space that would
/// not fit hangs past the edge rather than opening the next row; a word
/// wider than the whole row is broken at the cell, as code is.
pub(super) fn fold(
    line: &ContentLine,
    width: usize,
    breaks: Breaks,
    mut visit: impl FnMut(usize, usize, &Span, char),
) -> (usize, usize) {
    let width = width.max(1);
    let characters: Vec<(&Span, char)> = line
        .spans
        .iter()
        .flat_map(|span| span.text.chars().map(move |character| (span, character)))
        .collect();
    let (mut row, mut cell) = (0usize, 0usize);
    for (index, &(span, character)) in characters.iter().enumerate() {
        let taken = cell_width(character);
        let overflows = match breaks {
            Breaks::Cells => cell + taken > width,
            Breaks::Words if character.is_whitespace() => false,
            Breaks::Words => {
                let starts_a_word = index == 0 || characters[index - 1].1.is_whitespace();
                let word: usize = match starts_a_word {
                    true => characters[index..]
                        .iter()
                        .take_while(|(_, character)| !character.is_whitespace())
                        .map(|&(_, character)| cell_width(character))
                        .sum(),
                    false => taken,
                };
                cell + word.min(width) > width
            }
        };
        if cell > 0 && overflows {
            row += 1;
            cell = 0;
        }
        visit(row, cell, span, character);
        cell += taken;
    }
    (row, cell)
}

/// `line`'s spans, styled and folded into the rows [`fold`] puts them on.
pub(super) fn folded_rows(
    line: &ContentLine,
    width: usize,
    breaks: Breaks,
) -> Vec<Vec<TextSpan<'static>>> {
    let mut rows: Vec<Vec<TextSpan<'static>>> = vec![Vec::new()];
    let mut last: Option<*const Span> = None;
    fold(line, width, breaks, |row, _, span, character| {
        if rows.len() <= row {
            rows.push(Vec::new());
            last = None;
        }
        let spans = rows.last_mut().expect("a row was pushed above");
        let text = match character {
            '\t' => " ".repeat(TAB_WIDTH),
            _ => character.to_string(),
        };
        match spans.last_mut() {
            Some(piece) if last == Some(std::ptr::from_ref(span)) => {
                piece.content.to_mut().push_str(&text);
            }
            _ => {
                spans.push(TextSpan::styled(text, styled(span).style));
                last = Some(std::ptr::from_ref(span));
            }
        }
    });
    rows
}
