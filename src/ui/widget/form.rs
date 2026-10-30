//! A form's controls, as a settings form draws them: a label in its
//! column, then the value as the box that changes it. A number between
//! `−` and `+` on raised steps, a few choices side by side with the
//! chosen one in the accent, a typed value in a recessed box, a value
//! picked from a list in that box with the mark that says it opens.
//!
//! It joined when an extension's preferences, in the Extensions drawer,
//! came to need the same controls the workspace's settings had drawn
//! alone: one look for every form in the product. Each control answers
//! with spans a caller places on its own row, so it knows nothing of where
//! the row stands or what a click on a part of it means.

use ratatui::text::{Line, Span};
use uze_theme::Token;

use super::{RowState, row, text};
use crate::ui::theme::{self, Symbol};

/// How far a label sits in under its group's heading, and the column the
/// keyboard's mark stands in.
pub(crate) const LABEL_INDENT: usize = 2;

/// A key's label, `width` columns with the keyboard's mark ahead of it
/// when the keys are on its row; bright on that row, lit under the
/// pointer, quiet otherwise.
pub(crate) fn label(said: &str, state: RowState, width: u16) -> Span<'static> {
    let marker = match state {
        RowState::Selected => format!("{} ", theme::glyph(Symbol::Prompt)),
        RowState::Hovered | RowState::Resting => String::new(),
    };
    let room =
        usize::from(width).saturating_sub(Line::from(marker.as_str()).width().max(LABEL_INDENT));
    let shown = text::elide(said, room.saturating_sub(1));
    let label = format!("{marker:>LABEL_INDENT$}{shown:<room$}");
    let style = match state {
        RowState::Selected => theme::fg_bold(Token::TextBright),
        RowState::Hovered => theme::fg(Token::TextPrimary),
        RowState::Resting => theme::fg(Token::TextMuted),
    };
    Span::styled(label, style)
}

/// A value in its box, `width` columns wide.
pub(crate) fn boxed(said: &str, tone: Token, width: usize) -> Span<'static> {
    let width = width.max(2);
    Span::styled(
        format!(" {:<pad$}", text::elide(said, width - 2), pad = width - 1),
        theme::fg(tone).bg(theme::color(Token::SurfaceRecessed)),
    )
}

/// A value chosen from a list: the box a typed value sits in, with the
/// mark that says it opens.
pub(crate) fn combo(said: &str, tone: Token, width: usize) -> Vec<Span<'static>> {
    let mark = theme::glyph(Symbol::Dropdown);
    let room = width.saturating_sub(2 + Line::from(mark.as_str()).width() + 1);
    vec![
        Span::styled(
            format!(" {:<room$} ", text::elide(said, room)),
            theme::fg(tone).bg(theme::color(Token::SurfaceRecessed)),
        ),
        Span::styled(
            format!("{mark} "),
            theme::fg(Token::TextMuted).bg(theme::color(Token::SurfaceRecessed)),
        ),
    ]
}

/// What is being typed into a box `width` wide: the text either side of
/// the caret, on the box's ground, padded so nothing under it shows.
pub(crate) fn typing(before: &str, after: &str, width: u16) -> Vec<Span<'static>> {
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(before.to_owned(), theme::fg(Token::TextBright)),
        super::field::caret(),
        Span::styled(after.to_owned(), theme::fg(Token::TextBright)),
    ];
    row::pad_to(&mut spans, width, theme::color(Token::SurfaceRecessed));
    spans
}

/// A number between its − and +, `width` wide with the number in the
/// middle. `lit` says which of the two steps the pointer is over; the
/// widths of the two steps come back with the spans, so a caller knows
/// where each landed.
pub(crate) fn stepper(
    shown: &str,
    tone: Token,
    editable: bool,
    lit: [bool; 2],
    width: usize,
) -> (Vec<Span<'static>>, Steps) {
    let step = |symbol: Symbol, lit: bool| {
        let (fg, ground) = match (editable, lit) {
            (false, _) => (Token::TextFaint, Token::SurfaceRaised),
            (true, true) => (Token::TextBright, Token::SurfaceHover),
            (true, false) => (Token::TextSecondary, Token::SurfaceRaised),
        };
        Span::styled(
            format!(" {} ", theme::glyph(symbol)),
            theme::fg_bold(fg).bg(theme::color(ground)),
        )
    };
    let minus = step(Symbol::StepDecrease, lit[0]);
    let plus = step(Symbol::StepIncrease, lit[1]);
    let minus_width = minus.width() as u16;
    let plus_width = plus.width() as u16;
    let room = width.saturating_sub(usize::from(minus_width + plus_width));
    let middle = Span::styled(
        format!("{:^room$}", text::elide(shown, room)),
        theme::fg_bold(tone).bg(theme::color(Token::SurfaceRecessed)),
    );
    let middle_width = middle.width() as u16;
    (
        vec![minus, middle, plus],
        Steps {
            minus: (0, minus_width),
            plus: (minus_width + middle_width, plus_width),
        },
    )
}

/// Where a stepper's − and + stand from its first column: each one's
/// offset and width.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Steps {
    pub(crate) minus: (u16, u16),
    pub(crate) plus: (u16, u16),
}

/// How one of a segmented choice's options is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SegmentState {
    Chosen,
    Hovered,
    Resting,
}

/// One option of a segmented choice, `width` columns with its word in the
/// middle: the chosen one filled with the accent, the others on a raised
/// ground that lifts under the pointer.
pub(crate) fn segment(word: &str, state: SegmentState, width: usize) -> Span<'static> {
    let style = match state {
        SegmentState::Chosen => {
            theme::fg_bold(Token::SurfaceBackground).bg(theme::color(Token::Accent))
        }
        SegmentState::Hovered => theme::fg(Token::TextBright).bg(theme::color(Token::SurfaceHover)),
        SegmentState::Resting => {
            theme::fg(Token::TextSecondary).bg(theme::color(Token::SurfaceRaised))
        }
    };
    Span::styled(format!("{:^width$}", text::elide(word, width)), style)
}

/// The width each of `options` takes in a choice `width` wide: an even
/// share, never less than its widest word with a column either side.
pub(crate) fn segment_width(options: &[&str], width: usize) -> usize {
    let widest = options
        .iter()
        .map(|option| Line::from(*option).width() + 2)
        .max()
        .unwrap_or(0);
    (width / options.len().max(1)).max(widest)
}

/// A group's heading, `title` with its first letter raised, then a rule
/// to `width`: what parts one group of a form's keys from the next.
pub(crate) fn heading(title: &str, width: usize) -> Line<'static> {
    let mut letters = title.chars();
    let title: String = letters
        .next()
        .map(|first| first.to_uppercase().chain(letters).collect())
        .unwrap_or_default();
    ruled(title, width)
}

/// [`heading`] for a title spelled as it is, such as a file's name.
pub(crate) fn ruled(title: String, width: usize) -> Line<'static> {
    let rule = width.saturating_sub(Line::from(title.as_str()).width() + 1);
    Line::from(vec![
        Span::styled(title, theme::fg_bold(Token::TextPrimary)),
        Span::raw(" "),
        Span::styled(
            theme::glyph(Symbol::TreeDivider).repeat(rule),
            theme::fg(Token::BorderFaint),
        ),
    ])
}
