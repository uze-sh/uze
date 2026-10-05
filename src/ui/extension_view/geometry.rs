//! Where a surface's navigator, content and footer go in the frame it is handed.

use super::*;

pub(crate) fn clamp_navigator_width(width: u16, total_width: u16) -> u16 {
    let max = total_width
        .saturating_sub(MIN_EXTENSION_CONTENT_WIDTH)
        .clamp(MIN_NAVIGATOR_WIDTH, MAX_NAVIGATOR_WIDTH);
    width.clamp(MIN_NAVIGATOR_WIDTH, max)
}

/// The navigator/content split, derived from the outer overlay area so
/// drawing and hit-testing always share the exact same geometry. This
/// splits horizontally first, so the navigator column spans the entire
/// inner height and its right-hand divider reaches edge to edge; only the
/// content side is split again to carve out a footer that belongs to that
/// column alone rather than reading as a global app bar.
pub(crate) fn content_columns(
    frame_area: Rect,
    navigator_width_override: Option<u16>,
) -> (Rect, Rect, Rect) {
    // No frame and no margin: the surface stands where the pane is, and
    // the pane's own edges — the sidebar's divider, the strip's rule —
    // already say where it begins, the way they do for a shell.
    let inner = frame_area;
    let navigator_width = navigator_width_override
        .map(|width| clamp_navigator_width(width, inner.width))
        .unwrap_or_else(|| (inner.width / 4).clamp(MIN_NAVIGATOR_WIDTH, MAX_NAVIGATOR_WIDTH));
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(navigator_width), Constraint::Min(10)])
        .split(inner);
    let content_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(FOOTER_ROWS)])
        .split(columns[1]);
    (columns[0], content_rows[0], content_rows[1])
}

/// [`content_columns`] for a view with no navigator: no column for it,
/// and the content and the keys across the whole width.
pub(super) fn whole_width(frame_area: Rect) -> (Rect, Rect, Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(FOOTER_ROWS)])
        .split(frame_area);
    (
        Rect {
            width: 0,
            ..frame_area
        },
        rows[0],
        rows[1],
    )
}

/// A board's rows, top to bottom: its menu, the board itself and the
/// footer. The board gets everything the other two do not need — no
/// navigator column, no blank row under the title, no reading margin, and
/// one row of menu rather than one per level of it.
///
/// The menu sits straight under the title because there is nothing up
/// there to crowd: the frame's top edge carries the surface's name and
/// nothing else, and where it is open is written along the foot. A row
/// of chips under one word is a row of chips, not a continuation.
pub(crate) fn board_rows(frame_area: Rect) -> (Rect, Rect, Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(FOOTER_ROWS),
        ])
        .split(frame_area);
    (rows[0], rows[1], rows[2])
}

/// How many cells a board has to show its drawing in — exact, unlike
/// [`content_space`]: the extension cuts the screen it hands over to this.
pub(crate) fn board_space(frame_area: Rect) -> Size {
    let (_, board, _) = board_rows(frame_area);
    Size {
        width: board.width,
        height: board.height,
    }
}

/// How much room the content column has, for an extension deciding how
/// much to produce.
/// The room the code surface has. Which of the two it is depends on what
/// the surface is showing, and only the surface knows: the map is a
/// picture of the whole checkout and takes the frame, while everything
/// else is read in the column beside the tree.
pub(crate) fn code_space(
    frame_area: Rect,
    navigator_width_override: Option<u16>,
    code: Option<&uze_extensions::code::CodeView>,
) -> Size {
    match code.map(uze_extensions::code::CodeView::showing) {
        Some(uze_extensions::code::ContentMode::Map) => board_space(frame_area),
        _ => content_space(frame_area, navigator_width_override),
    }
}

/// The keys that act here, on one row: with no frame beneath it there is
/// no edge the row needs keeping off.
pub(super) const FOOTER_ROWS: u16 = 1;

pub(crate) fn content_space(frame_area: Rect, navigator_width_override: Option<u16>) -> Size {
    let (_, content, _) = content_columns(frame_area, navigator_width_override);
    Size {
        width: content.width,
        height: content.height,
    }
}

/// Which half of the overlay the pointer is over — the host's answer,
/// because the host owns the layout.
pub(crate) fn scroll_target(
    frame_area: Rect,
    navigator_width_override: Option<u16>,
    column: u16,
    row: u16,
) -> Option<ScrollTarget> {
    let (navigator, content, _) = content_columns(frame_area, navigator_width_override);
    let pointer = ratatui::layout::Position::new(column, row);
    if navigator.contains(pointer) {
        Some(ScrollTarget::Navigator)
    } else if content.contains(pointer) {
        Some(ScrollTarget::Content)
    } else {
        None
    }
}

/// Draws the overlay across the entire frame — every other row this frame
/// would otherwise have drawn is skipped by the caller rather than drawn
/// and covered.
/// Where the navigator's list is scrolled to, which the host keeps
/// because only the host knows how many rows fit (see
/// [`Navigator::anchor`]). Handed into a frame and handed back settled:
/// clamped to the rows that exist, and moved just far enough to show an
/// anchor the extension has changed since the last frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct NavigatorScroll {
    /// The first row shown.
    pub(crate) first: usize,
    /// The anchor the list was last scrolled to reveal, so the same
    /// anchor asked for again does not pull the list back to it.
    pub(crate) revealed: Option<usize>,
}

impl NavigatorScroll {
    pub(crate) fn scrolled(self, direction: uze_extensions::view::ScrollDirection) -> Self {
        use uze_extensions::view::ScrollDirection;
        Self {
            first: match direction {
                ScrollDirection::Up => self.first.saturating_sub(1),
                ScrollDirection::Down => self.first.saturating_add(1),
            },
            ..self
        }
    }

    /// The scroll a list of `rows` rows in `visible` lines settles at.
    pub(super) fn settled(self, anchor: Option<usize>, rows: usize, visible: usize) -> Self {
        let mut first = self.first.min(rows.saturating_sub(visible));
        if anchor != self.revealed
            && let Some(anchor) = anchor
        {
            if anchor < first {
                first = anchor;
            } else if anchor >= first + visible {
                first = (anchor + 1).saturating_sub(visible);
            }
        }
        Self {
            first,
            revealed: anchor,
        }
    }
}
