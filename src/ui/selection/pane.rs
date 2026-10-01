//! A pane's selection: the gesture is the client's, the text the
//! terminal server's.
//!
//! The server anchors what is covered to the lines under it, so it stays
//! on them while the view scrolls and copies what scrolled away — which is
//! why the client sends it the gesture as it goes rather than keeping a
//! range of its own.

use ratatui::layout::Rect;
use uze_terminal::{ClientRequest, PaneId, SelectionGesture};

use super::Gesture;

/// A press in a pane and where the pointer has carried it since, in the
/// pane's own 0-indexed cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PaneSelection {
    pub(crate) pane: PaneId,
    gesture: Gesture<(u16, u16)>,
}

impl PaneSelection {
    pub(crate) fn pressed(pane: PaneId, area: Rect, column: u16, row: u16) -> Self {
        Self {
            pane,
            gesture: Gesture::pressed(cell_in(area, column, row)),
        }
    }

    /// Follows the pointer, clamped to the pane: a drag that overshoots the
    /// edge still means "to the edge", which is how a selection reaches the
    /// last column without the pointer landing exactly on it. Past the top
    /// or the bottom it also scrolls by the overshoot, which is how a
    /// selection reaches text that is not on screen.
    pub(crate) fn follow(&mut self, area: Rect, column: u16, row: u16) -> Vec<ClientRequest> {
        let lines = if row < area.y {
            i32::from(area.y - row)
        } else if row >= area.bottom() {
            -i32::from(row - area.bottom() + 1)
        } else {
            0
        };
        let had_moved = self.gesture.moved();
        let carried = self.gesture.carry(cell_in(area, column, row));
        if lines != 0 {
            // Scrolling under the pointer covers new text even where the
            // pointer stays on its cell.
            self.gesture.begin();
        }
        if !carried && lines == 0 {
            return Vec::new();
        }
        let head = self.gesture.head();
        let gesture = if had_moved {
            SelectionGesture::Extend { head }
        } else if self.gesture.moved() {
            SelectionGesture::Begin {
                anchor: self.gesture.anchor(),
                head,
            }
        } else {
            return Vec::new();
        };
        let mut requests = Vec::new();
        if lines != 0 {
            requests.push(ClientRequest::Scroll {
                pane: self.pane,
                lines,
            });
        }
        requests.push(self.request(gesture));
        requests
    }

    /// What to say after the view moved under a held pointer: the cell it
    /// rests on now holds a different line.
    pub(crate) fn rescrolled(&self) -> Option<ClientRequest> {
        self.gesture.dragging().then(|| {
            self.request(SelectionGesture::Extend {
                head: self.gesture.head(),
            })
        })
    }

    /// The button came up; whether this was a drag rather than a click.
    pub(crate) fn release(&mut self) -> bool {
        self.gesture.release()
    }

    /// What to say when the selection is dropped: nothing for a click,
    /// which never reached the server.
    pub(crate) fn cleared(&self) -> Option<ClientRequest> {
        self.gesture
            .moved()
            .then(|| self.request(SelectionGesture::Clear))
    }

    fn request(&self, gesture: SelectionGesture) -> ClientRequest {
        ClientRequest::Select {
            pane: self.pane,
            gesture,
        }
    }
}

fn cell_in(area: Rect, column: u16, row: u16) -> (u16, u16) {
    let column = column.clamp(area.x, area.right().saturating_sub(1)) - area.x;
    let row = row.clamp(area.y, area.bottom().saturating_sub(1)) - area.y;
    (column, row)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 10,
        y: 5,
        width: 12,
        height: 3,
    };

    fn select(gesture: SelectionGesture) -> ClientRequest {
        ClientRequest::Select {
            pane: PaneId(1),
            gesture,
        }
    }

    #[test]
    fn a_press_that_never_moved_says_nothing() {
        let mut selection = PaneSelection::pressed(PaneId(1), AREA, 12, 6);
        assert!(selection.follow(AREA, 12, 6).is_empty());
        assert!(!selection.release());
        assert_eq!(selection.cleared(), None);
    }

    #[test]
    fn the_first_movement_begins_and_the_rest_extend() {
        let mut selection = PaneSelection::pressed(PaneId(1), AREA, 12, 6);
        assert_eq!(
            selection.follow(AREA, 14, 6),
            [select(SelectionGesture::Begin {
                anchor: (2, 1),
                head: (4, 1),
            })]
        );
        assert_eq!(
            selection.follow(AREA, 15, 7),
            [select(SelectionGesture::Extend { head: (5, 2) })]
        );
        assert!(selection.follow(AREA, 15, 7).is_empty());
    }

    #[test]
    fn a_drag_past_the_edge_selects_to_the_edge() {
        let mut selection = PaneSelection::pressed(PaneId(1), AREA, AREA.x, AREA.y + 1);
        assert_eq!(
            selection.follow(AREA, AREA.right() + 20, AREA.y + 1),
            [select(SelectionGesture::Begin {
                anchor: (0, 1),
                head: (11, 1),
            })]
        );
    }

    #[test]
    fn a_drag_past_the_top_or_bottom_scrolls_by_the_overshoot() {
        let mut selection = PaneSelection::pressed(PaneId(1), AREA, 12, 6);
        assert_eq!(
            selection.follow(AREA, 12, AREA.y - 2),
            [
                ClientRequest::Scroll {
                    pane: PaneId(1),
                    lines: 2,
                },
                select(SelectionGesture::Begin {
                    anchor: (2, 1),
                    head: (2, 0),
                }),
            ]
        );
        assert_eq!(
            selection.follow(AREA, 12, AREA.bottom()),
            [
                ClientRequest::Scroll {
                    pane: PaneId(1),
                    lines: -1,
                },
                select(SelectionGesture::Extend { head: (2, 2) }),
            ]
        );
    }

    #[test]
    fn the_view_moving_under_a_held_drag_extends_it_and_under_a_released_one_does_not() {
        let mut selection = PaneSelection::pressed(PaneId(1), AREA, 12, 6);
        assert_eq!(selection.rescrolled(), None);
        selection.follow(AREA, 14, 7);
        assert_eq!(
            selection.rescrolled(),
            Some(select(SelectionGesture::Extend { head: (4, 2) }))
        );
        assert!(selection.release());
        assert_eq!(selection.rescrolled(), None);
        assert_eq!(selection.cleared(), Some(select(SelectionGesture::Clear)));
    }
}
