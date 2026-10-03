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

/// Clears `area` and draws the modal's border, its name on the top edge
/// and the close mark at the other end of it.
pub(crate) fn render(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    name: &str,
    close_hovered: bool,
) -> Chrome {
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
    let mark_width = theme::width(Symbol::MarkClose);
    let close = Rect::new(
        title.right().saturating_sub(mark_width + 1),
        title.y,
        (mark_width + 2).min(area.width),
        1.min(area.height),
    );
    close_mark(frame, close, close_hovered);
    Chrome { area, close }
}

/// The mark that closes a modal, drawn into `rect` with a cell of room
/// either side. It goes red under the pointer: at rest it is quiet chrome,
/// and the hover is where it says that a click here throws the modal away.
pub(crate) fn close_mark(frame: &mut ratatui::Frame<'_>, rect: Rect, hovered: bool) {
    let tone = match hovered {
        true => Token::StateDanger,
        false => Token::TextMuted,
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!(" {} ", theme::glyph(Symbol::MarkClose)),
            theme::fg(tone),
        )),
        rect,
    );
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
    /// The close mark is quiet at rest and red under the pointer: the
    /// hover is where it says a click there throws the modal away.
    #[test]
    fn the_close_mark_goes_red_only_under_the_pointer() {
        use ratatui::{Terminal, backend::TestBackend};

        let mark_tone = |hovered: bool| {
            let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
            let mut close = Rect::default();
            terminal
                .draw(|frame| close = render(frame, frame.area(), "work", hovered).close)
                .unwrap();
            let buffer = terminal.backend().buffer().clone();
            buffer[(close.x + 1, close.y)].fg
        };

        assert_eq!(mark_tone(false), theme::color(Token::TextMuted));
        assert_eq!(mark_tone(true), theme::color(Token::StateDanger));
    }

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
