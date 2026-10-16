//! A chip: a short filled label that stands where a control stands, and
//! the status beside the header's controls, which wears no ground at all.
//!
//! A ground is what a control is drawn on, so a report never gets one:
//! told apart by the shade of a fill, a status a click answers and one it
//! does not read alike in a terminal that barely renders the difference.
//! [`Status`] says it with weight and hue instead.
//!
//! It lived in the workspace's own renderer, which meant the management
//! screens could not draw one — the second header that needed a control
//! of this shape would have built a third.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uze_theme::Token;

use super::row;
use crate::ui::theme;

/// The column of air each side of a chip's label. Part of the control: it
/// is filled, hovered and clicked exactly as the glyphs are, which is why
/// the rect is measured from it rather than around it.
pub(crate) const PAD: u16 = 1;

/// What the pointer is doing to a chip.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChipState {
    Resting,
    Hovered,
    Pressed,
}

impl ChipState {
    /// The label colour and the ground under it, for a chip of `hue`.
    ///
    /// Both together rather than leaving the label to the caller: pressing
    /// inverts — the hue becomes the fill and the label drops to the
    /// backdrop — and that is the one state which overrules a label's own
    /// colour.
    pub(crate) fn skin(self, hue: Color) -> (Color, Color) {
        match self {
            Self::Resting => (hue, theme::color(Token::SurfaceRaised)),
            Self::Hovered => (hue, theme::color(Token::SurfaceHover)),
            Self::Pressed => (theme::color(Token::SurfaceBackground), hue),
        }
    }
}

/// Whether a click answers a status, and what the pointer is doing to it
/// when one does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StatusKind {
    /// Said and nothing more: no click answers it, so nothing about it
    /// moves under the pointer either.
    Report,
    Action {
        hovered: bool,
    },
}

/// A work state said in the header: its words in its hue, on no ground.
///
/// At rest a report wears the state's muted hue at regular weight and an
/// action the full hue in bold, so the two read apart before the pointer
/// is anywhere near them. Under the pointer an action lifts toward the
/// brightest text. A press paints nothing of its own: what answers it is
/// the toast that says what was sent, and the state the work moves to.
#[derive(Clone, Debug)]
pub(crate) struct Status {
    label: String,
    hue: Token,
    muted: Token,
    kind: StatusKind,
}

impl Status {
    pub(crate) fn new(
        label: impl Into<String>,
        hue: Token,
        muted: Token,
        kind: StatusKind,
    ) -> Self {
        Self {
            label: label.into(),
            hue,
            muted,
            kind,
        }
    }

    /// The columns this status occupies, label plus its padding.
    pub(crate) fn width(&self) -> u16 {
        Span::raw(&self.label).width() as u16 + 2 * PAD
    }

    /// Where this status sits when its right edge is `right`.
    pub(crate) fn rect_ending_at(&self, right: u16, row: u16) -> Rect {
        let width = self.width();
        Rect::new(right.saturating_sub(width), row, width, 1)
    }

    pub(crate) fn style(&self) -> Style {
        match self.kind {
            StatusKind::Report => theme::fg(self.muted),
            StatusKind::Action { hovered: true } => Style::default()
                .fg(theme::lifted(self.hue))
                .add_modifier(Modifier::BOLD),
            StatusKind::Action { hovered: false } => theme::fg_bold(self.hue),
        }
    }

    pub(crate) fn render(&self, frame: &mut ratatui::Frame<'_>, rect: Rect) {
        let pad = " ".repeat(PAD as usize);
        let spans = vec![
            Span::raw(pad.clone()),
            Span::styled(self.label.clone(), self.style()),
            Span::raw(pad),
        ];
        frame.render_widget(Paragraph::new(Line::from(spans)), rect);
    }
}

/// One header control.
#[derive(Clone, Debug)]
pub(crate) struct Chip {
    label: String,
    /// Resolved rather than a [`Token`]: a chip's hue is usually the
    /// answer of a state the caller owns, which has already resolved it.
    hue: Color,
    state: ChipState,
}

impl Chip {
    pub(crate) fn new(label: impl Into<String>, hue: Color, state: ChipState) -> Self {
        Self {
            label: label.into(),
            hue,
            state,
        }
    }

    /// The columns this chip occupies, label plus its padding.
    pub(crate) fn width(&self) -> u16 {
        Span::raw(&self.label).width() as u16 + 2 * PAD
    }

    /// Where this chip sits when its right edge is `right` — how a header
    /// lays its controls out, from the edge inward.
    pub(crate) fn rect_ending_at(&self, right: u16, row: u16) -> Rect {
        let width = self.width();
        Rect::new(right.saturating_sub(width), row, width, 1)
    }

    pub(crate) fn render(&self, frame: &mut ratatui::Frame<'_>, rect: Rect) {
        let (label, ground) = self.state.skin(self.hue);
        let mut spans = vec![
            Span::raw(" ".repeat(PAD as usize)),
            Span::styled(
                self.label.clone(),
                Style::default().fg(label).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" ".repeat(PAD as usize)),
        ];
        row::pad_to(&mut spans, rect.width, ground);
        frame.render_widget(Paragraph::new(Line::from(spans)), rect);
    }
}
