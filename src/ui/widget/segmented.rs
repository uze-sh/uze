//! A segmented control: a few views of one place side by side, choosing
//! which the place below shows, said in colour alone.
//!
//! Not a [`tabs`](super::tabs) strip: a tab is a screen reached by its
//! number, while a segment is a view of the same thing, two or three of
//! them, named and nothing else. It is drawn as a line rather than into a
//! rect so it can ride a surface's top border, where a dialog's title
//! would stand.

use ratatui::text::{Line, Span};
use uze_theme::Token;

use crate::ui::theme;

/// Between one segment's name and the next.
const GAP: &str = "  ";

/// One segment: its name, what a click on it answers (`None` for the one
/// in front, which a click leaves where it is), and whether the pointer
/// is on it.
pub(crate) struct Segment<'a, T> {
    pub(crate) label: &'a str,
    pub(crate) target: Option<T>,
    pub(crate) hovered: bool,
}

/// `segments` as one line, with a space of air at each end: the one at
/// `open` in the accent, the rest held back until the pointer lights
/// them. No ground and no weight tell them apart, only the ink. Answers
/// with the line and, for each segment with a target, its column from the
/// line's start and its width.
pub(crate) fn segmented<T: Clone>(
    segments: &[Segment<'_, T>],
    open: usize,
) -> (Line<'static>, Vec<(u16, u16, T)>) {
    let mut spans = vec![Span::raw(" ")];
    let mut at = 1u16;
    let mut placed = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(GAP));
            at += GAP.len() as u16;
        }
        let tone = match () {
            _ if index == open => Token::Accent,
            _ if segment.hovered => Token::TextBright,
            _ => Token::TextMuted,
        };
        let width = segment.label.chars().count() as u16;
        spans.push(Span::styled(segment.label.to_owned(), theme::fg(tone)));
        if let Some(target) = &segment.target {
            placed.push((at, width, target.clone()));
        }
        at += width;
    }
    spans.push(Span::raw(" "));
    (Line::from(spans), placed)
}
