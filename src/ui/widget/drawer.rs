//! A detail drawer: a column off the right of a screen holding the
//! selected row in full, with a hairline on its left that is its drag
//! handle.
//!
//! The manage modal's screens drew this alone until the space overview
//! needed the same column — its second file. What it was dragged to and
//! whether it is being dragged arrive as arguments; neither client's model
//! is read here.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
};
use uze_theme::{Symbol, Token};

use super::{Align, Button, Edge, Rule, button_row};
use crate::ui::theme;

/// A drawer's width at rest, and on a terminal wide enough that the list
/// beside it has columns to spare and a paragraph reads in fewer rows.
pub(crate) const DEFAULT_WIDTH: u16 = 44;
pub(crate) const WIDE_WIDTH: u16 = 56;
const WIDE_FROM: u16 = 150;
/// The narrowest a drawer, or the list beside it, is ever drawn.
pub(crate) const MIN_WIDTH: u16 = 24;

/// How far a drawer's text sits inside it: past its rule and three columns
/// of air on the left, three on the right — the inset every screen's own
/// content keeps from the modal's border.
const PAD_LEFT: u16 = 4;
const PAD_RIGHT: u16 = 3;

/// How wide a drawer is drawn beside `room` columns of content: where it
/// was dragged to (`requested`), or its width at rest, never squeezing
/// itself or the list beside it past the minimum, and never taking more
/// than half of what there is.
///
/// The half is the part the minimum alone could not do. Leaving the list
/// its 24 columns is a floor, not a share: on a narrow screen a drawer at
/// its default took two thirds of the content and the list it is a detail
/// *of* got what was left over. A detail panel is read a paragraph at a
/// time and the list beside it is read as a whole, so when there is not
/// enough for both, the list is the one that keeps its shape.
pub(crate) fn width(requested: Option<u16>, room: u16) -> u16 {
    let ceiling = (room / 2)
        .min(room.saturating_sub(MIN_WIDTH))
        .max(MIN_WIDTH);
    let rest = if room >= WIDE_FROM {
        WIDE_WIDTH
    } else {
        DEFAULT_WIDTH
    };
    requested.unwrap_or(rest).clamp(MIN_WIDTH, ceiling)
}

/// Draws the drawer's shell over `area` — the whole column, its rule
/// included — on the same ground as the screen, since a drawer is a column
/// of the screen, not a slab laid over it. The rule is lit while
/// `dragging`. Answers with the padded rect its content goes in and the
/// rect of the drag handle, for the caller to register first so the rule
/// answers the pointer before the rows behind it do.
pub(crate) fn render(frame: &mut ratatui::Frame<'_>, area: Rect, dragging: bool) -> (Rect, Rect) {
    frame.render_widget(Clear, area);
    super::fill(frame, area, Token::SurfaceBackground);
    Rule::draggable(Edge::Left, dragging).render(frame, area);
    let handle = Rect::new(area.x, area.y, 1.min(area.width), area.height);
    let inner = Rect::new(
        area.x + PAD_LEFT,
        area.y,
        area.width.saturating_sub(PAD_LEFT + PAD_RIGHT),
        area.height,
    );
    (inner, handle)
}

/// Where a drawer's selected thing stands, in the words its foot prints: a
/// coloured headline and a muted note beneath it.
pub(crate) struct Status<'a> {
    pub color: Color,
    pub headline: &'a str,
    pub subtitle: &'a str,
}

/// How many rows a drawer's note may wrap onto before it is elided.
///
/// Two, because it is a sentence and the drawer is a width the operator
/// drags. On one row it was clipped mid-word the moment the drawer was
/// narrower than the longest note anyone had written — "setting it up
/// installs it" became "setting it up install", which is not a shorter
/// sentence but a broken one.
const NOTE_ROWS: u16 = 2;

/// Rows [`foot`] needs: a row of air, the headline and the rows its note
/// may take, plus a gap and a row of buttons when there are any.
pub(crate) fn foot_height(buttons: bool) -> u16 {
    if buttons {
        NOTE_ROWS + 4
    } else {
        NOTE_ROWS + 2
    }
}

/// A drawer's content split into its body and, along its bottom, the foot
/// [`foot`] draws.
pub(crate) fn split(inner: Rect, buttons: bool) -> (Rect, Rect) {
    let height = foot_height(buttons).min(inner.height);
    let body = Rect {
        height: inner.height - height,
        ..inner
    };
    (body, Rect::new(inner.x, body.bottom(), inner.width, height))
}

/// Every detail drawer ends the same way, at its bottom: where the thing
/// stands, then what can be done about it, as buttons. Answers with the
/// rect each button took, for the caller to register.
///
/// The drawer is the one place a row's actions are performed with the
/// pointer, so a caller passes only the buttons that can run now, each
/// `strong` while the pointer is on it: the step from soft to full is what
/// says it can be clicked.
pub(crate) fn foot<T: Clone>(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    status: Status<'_>,
    buttons: &[(Button, T)],
) -> Vec<(Rect, T)> {
    // A row of air where a rule used to stand: the foot is set apart by
    // space, the way the Marketplace panel's action is.
    let inner = Rect {
        y: area.y + 1.min(area.height),
        height: area.height.saturating_sub(1),
        ..area
    };
    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", theme::glyph(Symbol::StatusSelected)),
                Style::default().fg(status.color),
            ),
            Span::styled(
                status.headline.to_owned(),
                Style::default()
                    .fg(status.color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            status.subtitle.to_owned(),
            theme::fg(Token::TextMuted),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }),
        Rect {
            height: inner.height.min(1 + NOTE_ROWS),
            ..inner
        },
    );
    let row_y = inner.y + 1 + NOTE_ROWS;
    if row_y >= area.bottom() {
        return Vec::new();
    }
    let row = Rect::new(inner.x, row_y, inner.width, 1);
    button_row(frame, row, buttons, Align::Left)
}
