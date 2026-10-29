//! A segmented control: a few views of one place side by side, choosing
//! which the place below shows, said in colour alone and parted as a hint
//! bar's keys are.
//!
//! Not a [`tabs`](super::tabs) strip: a tab is a screen reached by its
//! number, while a segment is a view of the same thing, two or three of
//! them, named and nothing else. It is drawn as a line rather than into a
//! rect so it can ride a surface's top border, where a dialog's title
//! would stand.

use ratatui::text::{Line, Span};
use uze_theme::Token;

use crate::ui::theme::{self, Symbol};

/// One segment: its name, and what a click on it answers (`None` for the
/// one in front, which a click leaves where it is).
pub(crate) struct Segment<'a, T> {
    pub(crate) label: &'a str,
    pub(crate) target: Option<T>,
}

/// `segments` as one line, with a space of air at each end, parted by the
/// hint bar's separator: the one at `open` in the accent, the rest held
/// back. No ground, no weight and no hover tell them apart, only the ink
/// of the one in front. Answers with the line and, for each segment with a
/// target, its column from the line's start and its width.
pub(crate) fn segmented<T: Clone>(
    segments: &[Segment<'_, T>],
    open: usize,
) -> (Line<'static>, Vec<(u16, u16, T)>) {
    let mut spans = vec![Span::raw(" ")];
    let mut at = 1u16;
    let mut placed = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        if index > 0 {
            // The separator alone, its own air and no more: a hint bar's
            // keys are spread wider, views on a border sit close.
            let gap = theme::glyph(Symbol::HintSeparator);
            at += gap.chars().count() as u16;
            spans.push(Span::styled(gap, theme::fg(Token::TextFaint)));
        }
        let tone = if index == open {
            Token::Accent
        } else {
            Token::TextMuted
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
