//! Text selected in a pane with the pointer, and the clipboard it goes to.
//!
//! The client owns the mouse (it has to, for its own chrome), which takes
//! the host terminal's native selection away from the panes it draws. This
//! gives it back the way every terminal does it: press, drag, release — and
//! the release copies, so selecting is the whole gesture and no key a pane's
//! program might bind has to be taken from it.

use ratatui::layout::Rect;
use uze_terminal::{ClientRequest, PaneId, SelectionGesture};

/// A press in a pane and where the pointer has carried it since, in the
/// pane's own 0-indexed cells. Only the gesture lives here: what it covers
/// is the terminal server's, which anchors it to the lines under it, so it
/// stays on them while the view scrolls and copies what scrolled away.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PaneSelection {
    pub(super) pane: PaneId,
    anchor: (u16, u16),
    head: (u16, u16),
    /// Whether the pointer has left the cell it was pressed in. A press
    /// that never moved is a click, and a click selects nothing.
    moved: bool,
    /// Whether the button is still down, so the view moving means the
    /// selection's end moves with it.
    held: bool,
}

impl PaneSelection {
    pub(super) fn pressed(pane: PaneId, area: Rect, column: u16, row: u16) -> Self {
        let at = cell_in(area, column, row);
        Self {
            pane,
            anchor: at,
            head: at,
            moved: false,
            held: true,
        }
    }

    /// Follows the pointer, clamped to the pane: a drag that overshoots the
    /// edge still means "to the edge", which is how a selection reaches the
    /// last column without the pointer landing exactly on it. Past the top
    /// or the bottom it also scrolls by the overshoot, which is how a
    /// selection reaches text that is not on screen.
    pub(super) fn follow(&mut self, area: Rect, column: u16, row: u16) -> Vec<ClientRequest> {
        let head = cell_in(area, column, row);
        let lines = if row < area.y {
            i32::from(area.y - row)
        } else if row >= area.bottom() {
            -i32::from(row - area.bottom() + 1)
        } else {
            0
        };
        if head == self.head && lines == 0 {
            return Vec::new();
        }
        self.head = head;
        let gesture = if self.moved {
            SelectionGesture::Extend { head }
        } else if head != self.anchor || lines != 0 {
            self.moved = true;
            SelectionGesture::Begin {
                anchor: self.anchor,
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
    pub(super) fn rescrolled(&self) -> Option<ClientRequest> {
        (self.held && self.moved)
            .then(|| self.request(SelectionGesture::Extend { head: self.head }))
    }

    /// The button came up; whether this was a drag rather than a click.
    pub(super) fn release(&mut self) -> bool {
        self.held = false;
        self.moved
    }

    /// What to say when the selection is dropped: nothing for a click,
    /// which never reached the server.
    pub(super) fn cleared(&self) -> Option<ClientRequest> {
        self.moved.then(|| self.request(SelectionGesture::Clear))
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

/// The OSC 52 sequence that sets the system clipboard to `text` through the
/// host terminal. The terminal is the only thing that can reach the
/// clipboard of the machine the operator sits at — over SSH, from WSL into
/// Windows — so writing it there rather than calling a platform tool is
/// what makes the copy land where the reader will paste it.
pub(super) fn osc52(text: &str) -> Vec<u8> {
    let mut sequence = b"\x1b]52;c;".to_vec();
    sequence.extend(base64(text.as_bytes()).into_bytes());
    sequence.extend(b"\x07");
    sequence
}

fn base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let bytes = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let triple = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
        for (position, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if position <= chunk.len() {
                encoded.push(ALPHABET[(triple >> shift & 0x3f) as usize] as char);
            } else {
                encoded.push('=');
            }
        }
    }
    encoded
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

    #[test]
    fn osc52_carries_the_text_base64_encoded() {
        assert_eq!(osc52("hi!"), b"\x1b]52;c;aGkh\x07");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
