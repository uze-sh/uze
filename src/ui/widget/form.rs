//! The form components: every key a person sets, on any screen, is drawn
//! from here, as a web page draws its inputs from one component library.
//!
//! A form is groups of keys. A group is a ruled [`heading`] with
//! [`GROUP_AIR`] above it and [`KEY_AIR`] between its keys; a [`key`] is a
//! label in a column ([`Layout`]) and the [`Control`] that changes its
//! value; a list of choices marks each row with a [`check`]. The
//! workspace's settings and an extension's preferences are the same form,
//! and a new screen that asks for values is that form too: a kind of input
//! this does not have yet is a new [`Control`] here, never a control drawn
//! at its call site.
//!
//! A component answers with its spans and where each of its parts landed
//! ([`Part`]); what a click on a part means is the caller's, as for every
//! widget.

use ratatui::text::{Line, Span};
use uze_theme::Token;

use super::{RowState, row, text};
use crate::ui::theme::{self, Symbol};

/// How far a label sits in under its group's heading, and the column the
/// keyboard's mark stands in.
pub(crate) const LABEL_INDENT: usize = 2;

/// The blank rows above a group that follows another, so groups read
/// apart.
pub(crate) const GROUP_AIR: usize = 2;

/// The blank rows between a heading and its first key, and between keys,
/// so each key reads as its own.
pub(crate) const KEY_AIR: usize = 1;

/// The narrowest and widest a label column fitted to its labels is.
const LABEL_FIT: (usize, usize) = (12, 36);

/// The least a control keeps beside a fitted label column: a stepper of
/// two digits, two short options side by side.
const CONTROL_FLOOR: usize = 14;

/// The width a stepper and a segmented choice take, so the two kinds end
/// in the same column.
pub(crate) const CONTROL: usize = 24;

/// Where a form's keys put their parts: the label column, the width of a
/// stepper or a choice, the width of a box holding a value, and the room
/// the row has right of its label, which a choice of short words may run
/// into past its share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    pub(crate) label: u16,
    pub(crate) control: usize,
    pub(crate) field: usize,
    pub(crate) room: usize,
}

impl Layout {
    /// A label column as wide as `labels` need, within bounds, leaving the
    /// control at least [`CONTROL_FLOOR`] of `width`: a narrow room cuts the
    /// control's share before a label. Controls and boxes share what is
    /// left, up to [`CONTROL`].
    pub(crate) fn fit<'a>(labels: impl IntoIterator<Item = &'a str>, width: usize) -> Self {
        let label = labels
            .into_iter()
            .map(|label| text::columns(label) + LABEL_INDENT + 2)
            .max()
            .unwrap_or(0)
            .clamp(LABEL_FIT.0, LABEL_FIT.1)
            .min(width.saturating_sub(CONTROL_FLOOR));
        let room = width.saturating_sub(label);
        let control = CONTROL.min(room);
        Self {
            label: label as u16,
            control,
            field: control,
            room,
        }
    }
}

/// A key's value, as the input that changes it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Control<'a> {
    /// A number between `−` and `+`; `editable` false draws the steps
    /// faint.
    Stepper {
        value: &'a str,
        tone: Token,
        editable: bool,
    },
    /// A few options side by side, `chosen` in the accent. Only while
    /// [`fits`] says they do; otherwise the value goes in a box.
    Choice {
        options: &'a [&'a str],
        chosen: &'a str,
    },
    /// A typed value in its recessed box.
    Boxed { value: &'a str, tone: Token },
    /// A value picked from a list that opens: the box, with the mark that
    /// says so.
    Combo { value: &'a str, tone: Token },
    /// The box being typed into: the text either side of the caret.
    Typing { before: &'a str, after: &'a str },
    /// The way to add an entry to a list, where its first would stand.
    Add,
}

/// A part of a control a click lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    /// A stepper's `−`.
    Decrease,
    /// A stepper's number, or the whole of a box: what Enter answers.
    Value,
    /// A stepper's `+`.
    Increase,
    /// One of a choice's options, by its place.
    Option(usize),
}

/// A key drawn: its line, and each part of its control by its column from
/// the line's start, its width and what it is.
pub(crate) struct Key {
    pub(crate) line: Line<'static>,
    pub(crate) parts: Vec<(u16, u16, Part)>,
}

/// A key: `label` in the layout's column, the keys' mark ahead of it when
/// they are on it, then `control`; `lit` is the part under the pointer.
pub(crate) fn key(
    label: &str,
    state: RowState,
    control: Control<'_>,
    layout: Layout,
    lit: Option<Part>,
) -> Key {
    let mut spans = vec![self::label(label, state, layout)];
    let (drawn, parts) = draw(control, layout, lit, state == RowState::Selected);
    spans.extend(drawn);
    Key {
        line: Line::from(spans),
        parts: parts
            .into_iter()
            .map(|(x, width, part)| (layout.label + x, width, part))
            .collect(),
    }
}

/// A control on a line of its own under its key's label (a list's
/// entries), from the line's start.
pub(crate) fn control(
    control: Control<'_>,
    layout: Layout,
    lit: Option<Part>,
) -> (Vec<Span<'static>>, Vec<(u16, u16, Part)>) {
    draw(control, layout, lit, false)
}

/// The label column left blank, under a label.
pub(crate) fn indent(layout: Layout) -> Span<'static> {
    Span::raw(" ".repeat(usize::from(layout.label)))
}

/// The columns a choice of `options` wants: each its widest word with a
/// column of air either side, and never less than a form's control.
pub(crate) fn choice_width(options: &[&str]) -> usize {
    (segment_width(options, CONTROL) * options.len()).max(CONTROL)
}

/// Whether `options` stand side by side in `layout`: each its share of
/// the control, within the row's room.
pub(crate) fn fits(options: &[&str], layout: Layout) -> bool {
    !options.is_empty() && segment_width(options, layout.control) * options.len() <= layout.room
}

/// A row's mark in a list of choices: `◉` in the accent while it is
/// taken, `○` quiet while not, each a stronger version of itself under the
/// pointer.
pub(crate) fn check(on: bool, lit: bool) -> Span<'static> {
    let (symbol, tone) = if on {
        (Symbol::MarkToggleOn, Token::Accent)
    } else {
        (Symbol::MarkToggleOff, Token::TextMuted)
    };
    let style = if lit {
        ratatui::style::Style::default().fg(theme::lifted(tone))
    } else {
        theme::fg(tone)
    };
    Span::styled(theme::glyph(symbol).to_string(), style)
}

/// The columns [`check`] takes, the wider of its two marks.
pub(crate) fn check_width() -> u16 {
    theme::width(Symbol::MarkToggleOn).max(theme::width(Symbol::MarkToggleOff))
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

/// A key's label alone, in the layout's column with the keys' mark ahead
/// of it when they are on its row; bright on that row, lit under the
/// pointer, quiet otherwise. For a key whose value is not one control (a
/// box of prose), [`key`] for every other.
pub(crate) fn label(said: &str, state: RowState, layout: Layout) -> Span<'static> {
    let width = layout.label;
    let marker = match state {
        RowState::Selected => format!("{} ", theme::glyph(Symbol::Prompt)),
        RowState::Hovered | RowState::Resting => String::new(),
    };
    let room =
        usize::from(width).saturating_sub(Line::from(marker.as_str()).width().max(LABEL_INDENT));
    let shown = text::elide(said, room.saturating_sub(1));
    let label = format!("{marker:>LABEL_INDENT$}{}", pad_end(&shown, room));
    let style = match state {
        RowState::Selected => theme::fg_bold(Token::TextBright),
        RowState::Hovered => theme::fg(Token::TextPrimary),
        RowState::Resting => theme::fg(Token::TextMuted),
    };
    Span::styled(label, style)
}

type Drawn = (Vec<Span<'static>>, Vec<(u16, u16, Part)>);

fn draw(control: Control<'_>, layout: Layout, lit: Option<Part>, focused: bool) -> Drawn {
    // The value answers a click, so it lifts under the pointer.
    let ink = |tone: Token| {
        if lit == Some(Part::Value) {
            ratatui::style::Style::default().fg(theme::lifted(tone))
        } else {
            theme::fg(tone)
        }
    };
    let whole = |spans: Vec<Span<'static>>| {
        let width = spans.iter().map(Span::width).sum::<usize>() as u16;
        (spans, vec![(0, width, Part::Value)])
    };
    match control {
        Control::Stepper {
            value,
            tone,
            editable,
        } => stepper(value, tone, editable, lit, layout.control),
        Control::Choice { options, chosen } if fits(options, layout) => {
            choice(options, chosen, lit, layout.control)
        }
        Control::Choice { chosen, .. } => {
            whole(vec![boxed(chosen, ink(Token::TextPrimary), layout.field)])
        }
        Control::Boxed { value, tone } => whole(vec![boxed(value, ink(tone), layout.field)]),
        Control::Combo { value, tone } => whole(combo(value, ink(tone), layout.field)),
        Control::Typing { before, after } => whole(typing(before, after, layout.field as u16)),
        Control::Add => whole(vec![Span::styled(
            "+ add",
            theme::fg(if focused || lit == Some(Part::Value) {
                Token::TextSecondary
            } else {
                Token::TextDim
            }),
        )]),
    }
}

/// A value in its box, `width` columns wide, in `ink`.
fn boxed(said: &str, ink: ratatui::style::Style, width: usize) -> Span<'static> {
    let width = width.max(2);
    Span::styled(
        format!(" {}", pad_end(&text::elide(said, width - 2), width - 1)),
        ink.bg(theme::color(Token::SurfaceRecessed)),
    )
}

/// A value chosen from a list: the box a typed value sits in, with the
/// mark that says it opens.
fn combo(said: &str, ink: ratatui::style::Style, width: usize) -> Vec<Span<'static>> {
    let mark = theme::glyph(Symbol::Dropdown);
    let room = width.saturating_sub(2 + Line::from(mark.as_str()).width() + 1);
    vec![
        Span::styled(
            format!(" {} ", pad_end(&text::elide(said, room), room)),
            ink.bg(theme::color(Token::SurfaceRecessed)),
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
/// middle.
fn stepper(shown: &str, tone: Token, editable: bool, lit: Option<Part>, width: usize) -> Drawn {
    let step = |symbol: Symbol, part: Part| {
        let (fg, ground) = match (editable, lit == Some(part)) {
            (false, _) => (Token::TextFaint, Token::SurfaceRaised),
            (true, true) => (Token::TextBright, Token::SurfaceHover),
            (true, false) => (Token::TextSecondary, Token::SurfaceRaised),
        };
        Span::styled(
            format!(" {} ", theme::glyph(symbol)),
            theme::fg_bold(fg).bg(theme::color(ground)),
        )
    };
    let minus = step(Symbol::StepDecrease, Part::Decrease);
    let plus = step(Symbol::StepIncrease, Part::Increase);
    let minus_width = minus.width() as u16;
    let plus_width = plus.width() as u16;
    let room = width.saturating_sub(usize::from(minus_width + plus_width));
    let middle = Span::styled(
        centred(&text::elide(shown, room), room),
        if lit == Some(Part::Value) {
            theme::fg_bold(tone).fg(theme::lifted(tone))
        } else {
            theme::fg_bold(tone)
        }
        .bg(theme::color(Token::SurfaceRecessed)),
    );
    let middle_width = middle.width() as u16;
    (
        vec![minus, middle, plus],
        vec![
            (0, minus_width, Part::Decrease),
            (minus_width, middle_width, Part::Value),
            (minus_width + middle_width, plus_width, Part::Increase),
        ],
    )
}

/// Options side by side, each an even share of `width`: the chosen one
/// filled with the accent, the others on a raised ground that lifts under
/// the pointer.
fn choice(options: &[&str], chosen: &str, lit: Option<Part>, width: usize) -> Drawn {
    let each = segment_width(options, width);
    let mut spans = Vec::with_capacity(options.len());
    let mut parts = Vec::with_capacity(options.len());
    let mut x = 0;
    for (option, word) in options.iter().enumerate() {
        let style = if *word == chosen {
            theme::fg_bold(Token::SurfaceBackground).bg(theme::color(Token::Accent))
        } else if lit == Some(Part::Option(option)) {
            theme::fg(Token::TextBright).bg(theme::color(Token::SurfaceHover))
        } else {
            theme::fg(Token::TextSecondary).bg(theme::color(Token::SurfaceRaised))
        };
        let cell = Span::styled(centred(&text::elide(word, each), each), style);
        let cell_width = cell.width() as u16;
        parts.push((x, cell_width, Part::Option(option)));
        x += cell_width;
        spans.push(cell);
    }
    (spans, parts)
}

/// The width each of `options` takes in a choice `width` wide: an even
/// share, never less than its widest word with a column either side.
fn segment_width(options: &[&str], width: usize) -> usize {
    let widest = options
        .iter()
        .map(|option| Line::from(*option).width() + 2)
        .max()
        .unwrap_or(0);
    (width / options.len().max(1)).max(widest)
}

/// `said` padded to `width` terminal columns, which `format!`'s width
/// cannot do: it counts characters, and a wide one takes two columns.
fn pad_end(said: &str, width: usize) -> String {
    format!(
        "{said}{}",
        " ".repeat(width.saturating_sub(text::columns(said)))
    )
}

/// `said` in the middle of `width` terminal columns, the odd column of air
/// after it.
fn centred(said: &str, width: usize) -> String {
    let air = width.saturating_sub(text::columns(said));
    format!("{}{said}{}", " ".repeat(air / 2), " ".repeat(air - air / 2))
}
