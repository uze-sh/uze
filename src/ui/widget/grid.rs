//! A grid of cards: equal boxes laid a row at a time, left to right, and
//! only ever whole ones.
//!
//! Here because two files lay cards out: the manage screens' catalog and
//! the space overview's worktrees. Where the grid starts is the caller's —
//! each keeps its own scroll — and so is what a card says.

use ratatui::layout::Rect;

/// The column between two cards, and the line between two rows of them.
pub(crate) const GAP: u16 = 1;

/// How a list of cards falls into rows: how many share one, and how tall
/// each is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Shape {
    pub(crate) columns: u16,
    pub(crate) height: u16,
}

impl Shape {
    /// As many columns at least `least` wide as `width` holds, never fewer
    /// than one nor more than `most`.
    pub(crate) fn fitting(width: u16, least: u16, most: u16, height: u16) -> Self {
        let columns = ((width + GAP) / (least + GAP)).clamp(1, most.max(1));
        Self { columns, height }
    }

    /// The rows `count` cards take.
    pub(crate) fn rows(self, count: usize) -> usize {
        count.div_ceil(usize::from(self.columns.max(1)))
    }

    /// The rows that fit whole in `height` lines; one at the least, so a
    /// short area still says which row it is on.
    pub(crate) fn visible(self, height: u16) -> usize {
        usize::from((height + GAP) / (self.height + GAP)).max(1)
    }

    /// The row the card at `position` sits in.
    pub(crate) fn row_of(self, position: usize) -> usize {
        position / usize::from(self.columns.max(1))
    }

    /// `count` cards laid over `area` from row `first`. A row is drawn
    /// only when all of it fits: a card cut by the edge reads as a
    /// different, shorter card.
    pub(crate) fn lay(self, area: Rect, count: usize, first: usize) -> Grid {
        let columns = self.columns.max(1);
        let width = area.width.saturating_sub(GAP * (columns - 1)) / columns;
        let pitch = self.height + GAP;
        let mut grid = Grid {
            cards: Vec::new(),
            rows: self.rows(count),
            visible: self.visible(area.height),
            first,
            bottom: area.y,
        };
        for position in first * usize::from(columns)..count {
            let shown_row = (self.row_of(position) - first) as u16;
            let rect = Rect::new(
                area.x + (position % usize::from(columns)) as u16 * (width + GAP),
                area.y + shown_row * pitch,
                width,
                self.height,
            );
            if rect.bottom() > area.bottom() {
                break;
            }
            grid.bottom = rect.bottom() + GAP;
            grid.cards.push((position, rect));
        }
        grid
    }
}

/// The cards of a list laid out over an area: which are drawn and where,
/// and the window of card rows they were drawn from, for its scrollbar.
#[derive(Debug)]
pub(crate) struct Grid {
    /// Each card on screen, by its position in the list.
    pub(crate) cards: Vec<(usize, Rect)>,
    /// How many rows of cards the whole list takes, and how many fit.
    pub(crate) rows: usize,
    pub(crate) visible: usize,
    /// The first row drawn.
    pub(crate) first: usize,
    /// The line under the last card drawn.
    pub(crate) bottom: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Columns follow the room: as many of the least width as fit, capped.
    #[test]
    fn columns_follow_the_room() {
        assert_eq!(Shape::fitting(20, 30, 4, 6).columns, 1, "never none");
        assert_eq!(Shape::fitting(61, 30, 4, 6).columns, 2);
        assert_eq!(Shape::fitting(500, 30, 4, 6).columns, 4, "capped");
    }

    /// A row the area cannot hold whole is left out rather than cut.
    #[test]
    fn only_whole_rows_are_laid() {
        let shape = Shape {
            columns: 2,
            height: 6,
        };
        let grid = shape.lay(Rect::new(0, 0, 40, 16), 6, 1);
        let positions: Vec<usize> = grid.cards.iter().map(|(position, _)| *position).collect();
        assert_eq!(positions, vec![2, 3, 4, 5]);
        assert!(grid.cards.iter().all(|(_, rect)| rect.bottom() <= 16));
        assert_eq!(grid.rows, 3);
        assert_eq!(grid.visible, 2);
    }
}
