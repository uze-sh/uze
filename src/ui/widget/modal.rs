//! A modal: a titled card over a frame the scrim has pushed back — where
//! it sits on the frame, and the rect left inside its border.

use ratatui::layout::Rect;

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
