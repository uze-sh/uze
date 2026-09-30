//! Text marked with the pointer in whatever the content is showing, and
//! the text a release hands the host to copy.
//!
//! The same gesture a pane answers — press, drag, release, and the release
//! copies — because the client owns the mouse on this surface too, and
//! took the host terminal's own selection away with it. Unlike a pane,
//! the selection is kept in the text's terms rather than the screen's: a
//! diff scrolls and wraps under it, and the characters are what was meant.

use super::{CodeView, ContentMode, diff::content_line, editor::column_at_cell};
use crate::view::{Caret, TextSelection};

/// Where a press on the text landed and where the pointer has carried it
/// since, both as positions in the lines on show.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Marking {
    anchor: Caret,
    head: Caret,
    /// Whether the button is still down, so a movement still extends it.
    held: bool,
}

impl CodeView {
    /// A press on the text: where a selection would start. Nothing is
    /// marked until the pointer moves, so a click is still only a click.
    pub(super) fn mark_from(&mut self, line: usize, cell: usize) {
        self.marking = self.position_at(line, cell).map(|at| Marking {
            anchor: at,
            head: at,
            held: true,
        });
    }

    /// The pointer carried to `line` and `cell` with the button held. An
    /// editor's caret goes with it, so typing after a drag lands where the
    /// drag ended rather than where it began.
    pub(super) fn mark_to(&mut self, line: usize, cell: usize) {
        let Some(head) = self.position_at(line, cell) else {
            return;
        };
        let Some(marking) = self.marking.as_mut().filter(|marking| marking.held) else {
            return;
        };
        marking.head = head;
        if self.content == ContentMode::Contents
            && let Some(open) = self.open.as_mut()
        {
            open.caret = head;
        }
    }

    /// The button came up: what the drag marked, to be copied. It stays
    /// drawn, so the reader sees what was taken, until anything else is
    /// done here. A press that never moved marked nothing, and says so.
    pub(super) fn let_go(&mut self) -> Option<String> {
        let marking = self.marking.as_mut().filter(|marking| marking.held)?;
        marking.held = false;
        let Some(selection) = self.text_selection() else {
            self.marking = None;
            return None;
        };
        Some(self.marked_text(selection)).filter(|text| !text.is_empty())
    }

    pub(super) fn text_selection(&self) -> Option<TextSelection> {
        let Marking { anchor, head, .. } = self.marking?;
        let key = |at: Caret| (at.line, at.column);
        let (from, to) = match key(anchor) <= key(head) {
            true => (anchor, head),
            false => (head, anchor),
        };
        (from != to).then_some(TextSelection { from, to })
    }

    fn marked_text(&self, selection: TextSelection) -> String {
        (selection.from.line..=selection.to.line)
            .filter_map(|line| {
                let text = self.line_text(line)?;
                let range = selection.on_line(line)?;
                Some(
                    text.chars()
                        .skip(range.start)
                        .take(range.end.saturating_sub(range.start))
                        .collect::<String>(),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The character a hit names, clamped to the lines there are: a drag
    /// past the last one means the end of it.
    fn position_at(&self, line: usize, cell: usize) -> Option<Caret> {
        let last = self.line_count().checked_sub(1)?;
        if line > last {
            let column = self.line_text(last)?.chars().count();
            return Some(Caret { line: last, column });
        }
        let text = self.line_text(line)?;
        Some(Caret {
            line,
            column: column_at_cell(&text, cell),
        })
    }

    fn line_count(&self) -> usize {
        match self.content {
            ContentMode::Diff => self.changes.diff.len(),
            ContentMode::Contents => self.open.as_ref().map_or(0, |open| open.lines.len()),
            ContentMode::Preview => self.open.as_ref().map_or(0, |open| open.preview(0, 0).0),
            ContentMode::Map => 0,
        }
    }

    /// One line's text as the content draws it, without its gutter.
    fn line_text(&self, line: usize) -> Option<String> {
        let spans = match self.content {
            ContentMode::Diff => content_line(self.changes.diff.get(line)?).spans,
            ContentMode::Contents => return self.open.as_ref()?.lines.get(line).cloned(),
            ContentMode::Preview => self.open.as_ref()?.preview(line, 1).1.pop()?.spans,
            ContentMode::Map => return None,
        };
        Some(spans.into_iter().map(|span| span.text).collect())
    }
}
