//! A modal: a titled surface over a frame the scrim has pushed back.
//!
//! One shape, the one the manage modal set: inside a plain border, a header
//! row (the title on the left, the dismiss key's hint against the right)
//! over a hairline, the body, and a footer row (the keys that act here)
//! under a hairline. The way
//! out is the header's `esc close`, a key like any other: never a mark on
//! the border, and never a key in the footer. Every key drawn is a target,
//! answered as the key it names. [`Modal`] draws it; the rest of this
//! module says where the manage modal sits on the frame.

use ratatui::{
    layout::{Constraint, Rect},
    text::Line,
    widgets::{Clear, Padding, Paragraph},
};
use uze_keys::{Action, Scope};

use super::{Edge, Rule, Surface, footer, hint::Hints};

/// Below this width a modal keeps only a sliver of the frame behind it:
/// the screens inside are already giving things up, and a wide margin
/// makes them give up more.
pub(crate) const NARROW_WIDTH: u16 = 90;

/// From this width the margin widens: the screens have the columns they
/// need, and more of the workspace behind says where the modal came from.
pub(crate) const WIDE_WIDTH: u16 = 160;

/// Below this many rows the modal takes the whole frame: its own chrome
/// is a header, two rules and a footer, and a margin on top of that
/// leaves the screen inside nothing to draw a list in.
pub(crate) const ROOMY_HEIGHT: u16 = 24;

/// Where a modal sits over `frame`: inset by eight columns and two rows
/// on a wide terminal, four and two on an ordinary one, two and one on a
/// narrow one, and not at all on one too short for a margin.
pub(crate) fn area(frame: Rect) -> Rect {
    if frame.height < ROOMY_HEIGHT {
        return frame;
    }
    let (horizontal, vertical) = match frame.width {
        width if width >= WIDE_WIDTH => (8, 2),
        width if width >= NARROW_WIDTH => (4, 2),
        _ => (2, 1),
    };
    Rect::new(
        frame.x + horizontal,
        frame.y + vertical,
        frame.width.saturating_sub(2 * horizontal),
        frame.height.saturating_sub(2 * vertical),
    )
}

/// What a modal's content is drawn in: inside the border, and one blank
/// row under the title, so the first row of a menu never sits against it.
pub(crate) fn inside(area: Rect) -> Rect {
    Rect::new(
        area.x + 1,
        area.y + 2,
        area.width.saturating_sub(2),
        area.height.saturating_sub(3),
    )
}

/// The rectangles a drawn modal leaves for input handling: the modal
/// itself, and the mark on its title that closes it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Chrome {
    pub(crate) area: Rect,
    pub(crate) close: Rect,
}

/// How far the header's and the footer's text sit inside the border, the
/// body's as well: the hairlines reach one column further out, so the
/// words read as hung inside the rules rather than ending with them.
pub(crate) const INSET: u16 = 3;
const RULE_INSET: u16 = 1;

/// Rows a modal spends around its body: the two borders, the header, its
/// hairline and a row of air, then a row of air, the footer's hairline
/// and the footer.
pub(crate) const CHROME_ROWS: u16 = 8;

/// Columns a modal spends around its body: the two borders and the inset
/// on each side.
pub(crate) const CHROME_COLUMNS: u16 = 2 + 2 * INSET;

/// A modal to draw: its title, and the keys that answer it.
pub(crate) struct Modal<'a> {
    title: Line<'static>,
    scopes: &'a [Scope],
    keys: Hints<'a>,
    hovered: Option<Action>,
}

/// What a drawn modal left: its rect, the room for its body, and where
/// each of its keys landed (the dismiss key's included), for the caller to
/// register as the keys they name.
pub(crate) struct Drawn {
    pub(crate) popup: Rect,
    pub(crate) body: Rect,
    pub(crate) keys: Vec<(Rect, Action)>,
}

impl<'a> Modal<'a> {
    /// A modal headed `title`, its keys read from `scopes`.
    pub(crate) fn new(title: impl Into<Line<'static>>, scopes: &'a [Scope]) -> Self {
        Self {
            title: title.into(),
            scopes,
            keys: Hints::new(scopes),
            hovered: None,
        }
    }

    /// The keys that act on the body, for the footer's left end. The
    /// dismiss key is the header's: it is not one of these.
    pub(crate) fn keys(mut self, keys: Hints<'a>) -> Self {
        self.keys = keys;
        self
    }

    /// The key under the pointer, drawn lit wherever it stands.
    pub(crate) fn hovered(mut self, action: Option<Action>) -> Self {
        self.hovered = action;
        self
    }

    /// Draws the modal centred in `area`, `width` wide and with `body_rows`
    /// for its body, each bounded by the room `area` has.
    #[must_use = "register each key as a hit, so a click performs it"]
    pub(crate) fn render_centred(
        self,
        frame: &mut ratatui::Frame<'_>,
        area: Rect,
        width: u16,
        body_rows: u16,
    ) -> Drawn {
        let width = width.min(area.width.saturating_sub(4));
        let height = (body_rows + CHROME_ROWS).min(area.height.saturating_sub(2));
        let popup = area.centered(Constraint::Length(width), Constraint::Length(height));
        self.render(frame, popup)
    }

    /// Draws the modal filling `popup`.
    #[must_use = "register each key as a hit, so a click performs it"]
    pub(crate) fn render(self, frame: &mut ratatui::Frame<'_>, popup: Rect) -> Drawn {
        frame.render_widget(Clear, popup);
        // No room of the surface's own: the rows below are the modal's
        // whole layout, the header's inset and the hairlines included.
        let inner = Surface::floating()
            .padding(Padding::ZERO)
            .render(frame, popup);
        let row = |y: u16| Rect::new(inner.x, y, inner.width, 1.min(inner.height));
        let inset = |rect: Rect, by: u16| Rect {
            x: rect.x + by.min(rect.width),
            width: rect.width.saturating_sub(2 * by),
            ..rect
        };

        let header = inset(row(inner.y), INSET);
        let close = Hints::new(self.scopes)
            .key(Action::Dismiss)
            .hovered(self.hovered);
        let close_width = close.width().min(header.width);
        let mut keys = close.render(
            frame,
            Rect::new(header.right() - close_width, header.y, close_width, 1),
        );
        frame.render_widget(
            Paragraph::new(self.title),
            Rect {
                width: header.width.saturating_sub(close_width + 1),
                ..header
            },
        );
        if inner.height > 1 {
            Rule::new(Edge::Top).render(frame, inset(row(inner.y + 1), RULE_INSET));
        }

        let footer_y = inner.bottom().saturating_sub(2).max(inner.y);
        if inner.height >= CHROME_ROWS - 2 {
            let drawn = footer::render(
                frame,
                Rect {
                    height: 2,
                    ..inset(row(footer_y), RULE_INSET)
                },
                INSET - RULE_INSET,
                footer::Leading::Hints(self.keys.hovered(self.hovered)),
                Vec::new(),
            );
            keys.extend(drawn.hints);
        }

        let body = Rect::new(
            inner.x + INSET.min(inner.width),
            inner.y + 3,
            inner.width.saturating_sub(2 * INSET),
            inner.height.saturating_sub(CHROME_ROWS - 2),
        );
        Drawn { popup, body, keys }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A terminal with room to spare keeps the backdrop visible around
    /// the modal; one without gives the whole frame to what is in front.
    ///
    /// The inset used to scale and never reach zero, which answered the
    /// wrong question on a small screen: it held the margin proportional
    /// while a menu, a list and a drawer were already short of columns,
    /// so the border and the backdrop were paid for out of them.
    #[test]
    fn the_margin_follows_the_width_and_a_short_frame_takes_it_all() {
        let inset = |width, height| {
            let frame = Rect::new(0, 0, width, height);
            let modal = area(frame);
            (modal.x, modal.y)
        };
        assert_eq!(inset(190, 44), (8, 2), "wide");
        assert_eq!(inset(120, 36), (4, 2), "ordinary");
        assert_eq!(inset(86, 30), (2, 1), "narrow");
        let short = Rect::new(0, 0, 120, ROOMY_HEIGHT - 1);
        assert_eq!(area(short), short, "too short for a margin");
    }
}
