//! A modal: a titled card over a frame the scrim has pushed back, with the
//! mark on its title that closes it.
//!
//! Two modals are drawn this way — the management surface and the work
//! modal — and a reader tells them apart by their names alone.

use ratatui::{
    layout::Rect,
    text::Span,
    widgets::{Clear, Paragraph},
};
use uze_theme::Token;

use super::Surface;
use crate::ui::theme::{self, Symbol};

/// The frame a modal is willing to spend a margin out of. Under either
/// measure it fills the screen instead.
///
/// The width is what the management surface itself asks for: a menu, a
/// list at its minimum and a drawer at its minimum come to the high
/// seventies, and its own `narrow` fold sits at 90. Below that the
/// screens are already giving things up, and a margin makes them give up
/// more. The height is two rows of cards plus the chrome around them.
pub(crate) const ROOMY_WIDTH: u16 = 100;
pub(crate) const ROOMY_HEIGHT: u16 = 30;

/// Where a modal sits over `frame`.
///
/// Below [`ROOMY_WIDTH`]×[`ROOMY_HEIGHT`] it takes the whole frame. The
/// scaling inset answered the wrong question there: it kept the margin
/// proportional while the thing inside it was already short of room, so
/// a small laptop paid a border, two rules and a backdrop out of the
/// columns a menu, a list and a drawer were sharing. Showing the
/// workspace behind is worth a margin only once the screens in front do
/// not need it.
pub(crate) fn area(frame: Rect) -> Rect {
    if frame.width < ROOMY_WIDTH || frame.height < ROOMY_HEIGHT {
        return frame;
    }
    let horizontal = (frame.width / 16).min(6);
    let vertical = (frame.height / 12).min(2);
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

/// Clears `area` and draws the modal's border, its name on the top edge
/// and the close mark at the other end of it.
pub(crate) fn render(frame: &mut ratatui::Frame<'_>, area: Rect, name: &str) -> Chrome {
    frame.render_widget(Clear, area);
    Surface::card().render(frame, area);

    // Drawn by hand rather than as the block's own title so the close mark
    // has a rect the click can be tested against.
    let title = Rect::new(area.x + 2, area.y, area.width.saturating_sub(4), 1);
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!(" {name} "),
            theme::fg_bold(Token::TextBright),
        )),
        title,
    );
    // Only the close mark on the right: the key that closes the modal is
    // the one that opened it, and the index lists it for anyone asking.
    let mark = theme::glyph(Symbol::MarkClose);
    let mark_width = theme::width(Symbol::MarkClose);
    let close = Rect::new(
        title.right().saturating_sub(mark_width + 1),
        title.y,
        (mark_width + 2).min(area.width),
        1.min(area.height),
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!(" {mark} "),
            theme::fg(Token::TextMuted),
        )),
        close,
    );
    Chrome { area, close }
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
    fn the_modal_fills_a_small_frame_and_insets_a_roomy_one() {
        let roomy = Rect::new(0, 0, ROOMY_WIDTH, ROOMY_HEIGHT);
        let inset = area(roomy);
        assert!(inset.x > roomy.x, "a margin beside it: {inset:?}");
        assert!(inset.width < roomy.width, "and narrower for it: {inset:?}");

        for cramped in [
            Rect::new(0, 0, ROOMY_WIDTH - 1, ROOMY_HEIGHT),
            Rect::new(0, 0, ROOMY_WIDTH, ROOMY_HEIGHT - 1),
            Rect::new(0, 0, 80, 24),
        ] {
            assert_eq!(
                area(cramped),
                cramped,
                "either measure short of roomy takes the frame: {cramped:?}"
            );
        }
    }
}
