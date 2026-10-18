//! A detail drawer: a column off the right of a screen holding the
//! selected row in full, with a hairline on its left that is its drag
//! handle.
//!
//! The manage modal's screens drew this alone until the space overview
//! needed the same column — its second file. What it was dragged to and
//! whether it is being dragged arrive as arguments; neither client's model
//! is read here.

use ratatui::{layout::Rect, widgets::Clear};
use uze_theme::Token;

use super::{Edge, Rule};

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
