//! Text marked with the pointer, wherever the workspace client draws text
//! somebody came to read — and the clipboard it goes to.
//!
//! The client owns the mouse (it has to, for its own chrome), which takes
//! the host terminal's native selection away from everything it draws.
//! This gives it back the way every terminal does it: press, drag,
//! release — and the release copies, so selecting is the whole gesture.
//! One gesture, held here once, whatever is under it; what differs is only
//! who holds the text:
//!
//! - a pane's text is the terminal server's, so the gesture is sent there
//!   as it goes ([`pane`]);
//! - text the client lays out itself — an extension's [`Content::Lines`]
//!   — is marked in the text's own terms, a line and a character, so the
//!   marking survives a scroll and a wrap; the frame that drew it says
//!   where each character landed ([`TextRow`]), and the release asks the
//!   surface for the lines it covers, since those may have scrolled out
//!   of what was drawn.
//!
//! The chrome — sidebar, strip, headers, footers — records no rows, so a
//! press there is only ever a click; Shift still hands a drag to the host
//! terminal's own selection, as it does in any program that owns the mouse.
//!
//! [`Content::Lines`]: uze_extensions::view::Content::Lines

pub(crate) mod pane;

use ratatui::layout::Rect;
use uze_extensions::view::{Caret, ScrollDirection};

pub(crate) use pane::PaneSelection;

/// The one selection there is: whatever was last marked, in whichever
/// source it was marked in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Selection {
    Pane(PaneSelection),
    Text(TextSelection),
}

/// A press and where the pointer has carried it since, in whatever terms
/// the source names a place.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Gesture<P> {
    anchor: P,
    head: P,
    /// Whether the pointer has left the place it was pressed on. A press
    /// that never moved is a click, and a click selects nothing.
    moved: bool,
    /// Whether the button is still down, so a movement still extends it.
    held: bool,
}

impl<P: Copy + Eq> Gesture<P> {
    pub(crate) fn pressed(at: P) -> Self {
        Self {
            anchor: at,
            head: at,
            moved: false,
            held: true,
        }
    }

    /// The pointer carried to `to` with the button held; whether that is
    /// somewhere new.
    pub(crate) fn carry(&mut self, to: P) -> bool {
        if !self.held || to == self.head {
            return false;
        }
        self.head = to;
        self.moved |= to != self.anchor;
        true
    }

    /// Counts as a drag from here on, though the pointer has not left its
    /// place — the text under it moved instead.
    pub(crate) fn begin(&mut self) {
        self.moved = true;
    }

    /// The button came up; whether this was a drag rather than a click.
    pub(crate) fn release(&mut self) -> bool {
        self.held = false;
        self.moved
    }

    pub(crate) fn anchor(&self) -> P {
        self.anchor
    }

    pub(crate) fn head(&self) -> P {
        self.head
    }

    pub(crate) fn moved(&self) -> bool {
        self.moved
    }

    pub(crate) fn held(&self) -> bool {
        self.held
    }

    /// Held and already marking something.
    pub(crate) fn dragging(&self) -> bool {
        self.held && self.moved
    }
}

/// One row of text a frame drew, and which character landed in each of
/// its cells — recorded by the walk that drew them, so the pointer is
/// resolved against what is on screen rather than against a second guess
/// at how the line was folded.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TextRow {
    /// The whole row, gutter included: a press on a line's number starts
    /// marking at its first character.
    pub(crate) area: Rect,
    /// Which line of the content, counted from its start.
    pub(crate) line: usize,
    /// The characters drawn on this row, left to right.
    pub(crate) glyphs: Vec<Glyph>,
    /// The character a pointer to the right of the last glyph means: the
    /// line's end on the row it ends on, and the row's own last character
    /// on a row the line was folded from.
    pub(crate) beyond: usize,
}

/// Where one character was drawn on a [`TextRow`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Glyph {
    pub(crate) x: u16,
    pub(crate) width: u16,
    /// Its index among the characters of its line.
    pub(crate) index: usize,
}

impl TextRow {
    /// The character under `column`, or the nearest one on the row.
    fn index_at(&self, column: u16) -> usize {
        match self.glyphs.first() {
            Some(first) if column < first.x => first.index,
            _ => self
                .glyphs
                .iter()
                .find(|glyph| column < glyph.x + glyph.width)
                .map_or(self.beyond, |glyph| glyph.index),
        }
    }
}

/// Where the pointer is in the text, and the way the text has to scroll
/// for it to be there — past the top or the bottom of what was drawn, a
/// drag means the line beyond the edge.
///
/// `None` where the frame drew no text at all.
pub(crate) fn locate(
    rows: &[TextRow],
    column: u16,
    row: u16,
) -> Option<(Caret, Option<ScrollDirection>)> {
    let top = rows.iter().min_by_key(|text| text.area.y)?;
    let bottom = rows.iter().max_by_key(|text| text.area.y)?;
    if row < top.area.y {
        let line = top.line.saturating_sub(1);
        return Some((Caret { line, column: 0 }, Some(ScrollDirection::Up)));
    }
    if row >= bottom.area.bottom() {
        let past = Caret {
            line: bottom.line + 1,
            column: usize::MAX,
        };
        return Some((past, Some(ScrollDirection::Down)));
    }
    let nearest = rows
        .iter()
        .min_by_key(|text| text.area.y.abs_diff(row))
        .expect("there is a top row");
    Some((
        Caret {
            line: nearest.line,
            column: nearest.index_at(column),
        },
        None,
    ))
}

/// Whether a point is on text a frame drew.
pub(crate) fn on_text(rows: &[TextRow], column: u16, row: u16) -> bool {
    rows.iter().any(|text| {
        text.area
            .contains(ratatui::layout::Position::new(column, row))
    })
}

/// Text marked in content the client laid out, in the text's own terms.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TextSelection {
    gesture: Gesture<Caret>,
    /// The heading of the content it was made on. Positions only mean
    /// something in the text they were taken from, and a surface that
    /// moved on to another file under a released selection would
    /// otherwise show the new one marked where the old one was.
    pub(crate) heading: String,
}

impl TextSelection {
    pub(crate) fn pressed(at: Caret, heading: String) -> Self {
        Self {
            gesture: Gesture::pressed(at),
            heading,
        }
    }

    /// The pointer carried to `at` with the button held.
    pub(crate) fn carry(&mut self, at: Caret) {
        self.gesture.carry(at);
    }

    /// The button came up: what the drag marked, or nothing for a click.
    pub(crate) fn release(&mut self) -> Option<Marked> {
        self.gesture.release();
        self.marked()
    }

    /// Whether the button is still down, so a movement still extends it.
    pub(crate) fn held(&self) -> bool {
        self.gesture.held()
    }

    /// What is marked, once the pointer has moved: nothing for a click.
    pub(crate) fn marked(&self) -> Option<Marked> {
        self.gesture.moved().then(|| {
            let (anchor, head) = (self.gesture.anchor(), self.gesture.head());
            let key = |at: Caret| (at.line, at.column);
            let (from, to) = match key(anchor) <= key(head) {
                true => (anchor, head),
                false => (head, anchor),
            };
            Marked { from, to }
        })
    }

    /// What is marked in the content called `heading`, if it is the one
    /// the selection was made on.
    pub(crate) fn marked_in(&self, heading: &str) -> Option<Marked> {
        (self.heading == heading).then(|| self.marked()).flatten()
    }
}

/// A marked run of text, ordered, and inclusive at both ends: the
/// character under the pointer is taken whichever way the drag went, as
/// a terminal's selection takes it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Marked {
    pub(crate) from: Caret,
    pub(crate) to: Caret,
}

impl Marked {
    /// The lines it reaches, as a range to ask the content for.
    pub(crate) fn lines(&self) -> std::ops::Range<usize> {
        self.from.line..self.to.line.saturating_add(1)
    }

    /// Which characters of `line` are marked; `None` for a line it does
    /// not reach.
    pub(crate) fn on_line(&self, line: usize) -> Option<std::ops::RangeInclusive<usize>> {
        if line < self.from.line || line > self.to.line {
            return None;
        }
        let start = if line == self.from.line {
            self.from.column
        } else {
            0
        };
        let end = if line == self.to.line {
            self.to.column
        } else {
            usize::MAX
        };
        Some(start..=end)
    }

    /// The marked text out of `lines`, the content's lines from
    /// [`Self::lines`]'s start, joined as they were broken.
    pub(crate) fn text(&self, lines: &[String]) -> String {
        lines
            .iter()
            .enumerate()
            .filter_map(|(offset, text)| {
                let range = self.on_line(self.from.line + offset)?;
                let taken = range.end().saturating_sub(*range.start()).saturating_add(1);
                Some(
                    text.chars()
                        .skip(*range.start())
                        .take(taken)
                        .collect::<String>(),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The OSC 52 sequence that sets the system clipboard to `text` through the
/// host terminal. The terminal is the only thing that can reach the
/// clipboard of the machine the operator sits at — over SSH, from WSL into
/// Windows — so writing it there rather than calling a platform tool is
/// what makes the copy land where the reader will paste it.
pub(crate) fn osc52(text: &str) -> Vec<u8> {
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

    fn at(line: usize, column: usize) -> Caret {
        Caret { line, column }
    }

    /// Two rows of one line folded at four cells — `abcd` and `ef` —
    /// above a row of the next, which starts with a double-width glyph.
    fn rows() -> Vec<TextRow> {
        let glyphs = |x: u16, indices: std::ops::Range<usize>| {
            indices
                .enumerate()
                .map(|(offset, index)| Glyph {
                    x: x + offset as u16,
                    width: 1,
                    index,
                })
                .collect()
        };
        vec![
            TextRow {
                area: Rect::new(10, 5, 4, 1),
                line: 3,
                glyphs: glyphs(10, 0..4),
                beyond: 3,
            },
            TextRow {
                area: Rect::new(10, 6, 4, 1),
                line: 3,
                glyphs: glyphs(10, 4..6),
                beyond: 6,
            },
            TextRow {
                area: Rect::new(10, 7, 4, 1),
                line: 4,
                glyphs: vec![
                    Glyph {
                        x: 10,
                        width: 2,
                        index: 0,
                    },
                    Glyph {
                        x: 12,
                        width: 1,
                        index: 1,
                    },
                ],
                beyond: 2,
            },
        ]
    }

    #[test]
    fn the_pointer_names_the_character_drawn_under_it() {
        let rows = rows();
        assert_eq!(locate(&rows, 12, 5), Some((at(3, 2), None)));
        assert_eq!(locate(&rows, 11, 6), Some((at(3, 5), None)));
        // Either half of a wide glyph is that glyph.
        assert_eq!(locate(&rows, 11, 7), Some((at(4, 0), None)));
    }

    /// Right of the text: a folded row's own last character, and the
    /// line's end on the row it ends on — so a drag there takes the line
    /// whole without reaching into the row below.
    #[test]
    fn beside_the_text_is_the_nearest_end_of_it() {
        let rows = rows();
        assert_eq!(locate(&rows, 30, 5), Some((at(3, 3), None)));
        assert_eq!(locate(&rows, 30, 6), Some((at(3, 6), None)));
        assert_eq!(locate(&rows, 0, 7), Some((at(4, 0), None)));
    }

    #[test]
    fn past_the_top_or_bottom_is_the_line_beyond_and_scrolls_there() {
        let rows = rows();
        assert_eq!(
            locate(&rows, 12, 2),
            Some((at(2, 0), Some(ScrollDirection::Up)))
        );
        assert_eq!(
            locate(&rows, 12, 9),
            Some((at(5, usize::MAX), Some(ScrollDirection::Down)))
        );
        assert_eq!(locate(&[], 12, 5), None);
    }

    #[test]
    fn a_press_that_never_moved_marks_nothing() {
        let mut selection = TextSelection::pressed(at(1, 2), "a.rs".to_owned());
        assert!(!selection.gesture.carry(at(1, 2)));
        assert!(!selection.gesture.release());
        assert_eq!(selection.marked(), None);
    }

    /// Whichever way the drag went, both ends are taken.
    #[test]
    fn a_drag_marks_from_the_earlier_end_to_the_later_inclusive() {
        let lines = ["first".to_owned(), "second".to_owned()];
        for (anchor, head) in [(at(0, 2), at(1, 2)), (at(1, 2), at(0, 2))] {
            let mut selection = TextSelection::pressed(anchor, "a.rs".to_owned());
            selection.gesture.carry(head);
            let marked = selection.marked().expect("it moved");
            assert_eq!(marked.text(&lines), "rst\nsec");
        }
    }

    #[test]
    fn a_drag_past_the_end_of_a_line_takes_the_rest_of_it() {
        let marked = Marked {
            from: at(0, 2),
            to: at(1, usize::MAX),
        };
        assert_eq!(
            marked.text(&["first".to_owned(), "second".to_owned()]),
            "rst\nsecond"
        );
        assert_eq!(marked.lines(), 0..2);
    }

    #[test]
    fn a_selection_is_drawn_only_on_the_content_it_was_made_on() {
        let mut selection = TextSelection::pressed(at(0, 0), "a.rs".to_owned());
        selection.gesture.carry(at(0, 3));
        assert!(selection.marked_in("a.rs").is_some());
        assert_eq!(selection.marked_in("b.rs"), None);
    }

    #[test]
    fn osc52_carries_the_text_base64_encoded() {
        assert_eq!(osc52("hi!"), b"\x1b]52;c;aGkh\x07");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
