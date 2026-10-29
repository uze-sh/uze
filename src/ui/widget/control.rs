//! A setting's value drawn as the control that changes it: a number
//! between `−` and `+`, one of several between `‹` and `›`, a switch, or a
//! field holding what was typed.
//!
//! Every control fills the width it is handed, so a column of them lines up
//! whatever each holds. It draws on the ground it stands on and answers the
//! pointer by colour alone: a lit part brightens and keeps its weight.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};
use uze_theme::Token;

use super::text;
use crate::ui::theme::{self, Symbol};

/// What a control shows.
pub(crate) enum Control {
    /// A number, stepped with `−` and `+`.
    Stepper(String),
    /// One of several values, moved with `‹` and `›`.
    Choice(String),
    /// On or off.
    Toggle(bool),
    /// A typed value, in a field; `quiet` for one that holds nothing yet.
    Field { text: String, quiet: bool },
}

/// Which parts of a control the pointer is over.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Lit {
    pub(crate) previous: bool,
    pub(crate) value: bool,
    pub(crate) next: bool,
}

/// Where a control's parts landed: its value, and the step either side of
/// it when it has them.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Placed {
    pub(crate) previous: Option<Rect>,
    pub(crate) value: Rect,
    pub(crate) next: Option<Rect>,
}

/// The columns a step takes: its glyph with a column of air either side,
/// so the pointer has more than one cell to aim at.
const STEP: u16 = 3;

impl Control {
    /// The columns this control needs to show its value whole.
    pub(crate) fn natural_width(&self) -> u16 {
        let value = (match self {
            Self::Stepper(value) | Self::Choice(value) => text::columns(value),
            Self::Toggle(_) => text::columns(&toggle_word(false)),
            Self::Field { text: value, .. } => text::columns(value),
        } + 2) as u16;
        match self {
            Self::Stepper(_) | Self::Choice(_) => value + STEP * 2,
            Self::Toggle(_) | Self::Field { .. } => value,
        }
    }

    /// Draws the control across `rect`, the keyboard's row brighter.
    pub(crate) fn render(
        &self,
        frame: &mut ratatui::Frame<'_>,
        rect: Rect,
        selected: bool,
        lit: Lit,
    ) -> Placed {
        let value_tone = if lit.value || selected {
            Token::TextBright
        } else {
            Token::TextPrimary
        };
        let step_tone = |lit: bool| {
            if lit {
                Token::TextBright
            } else if selected {
                Token::TextSecondary
            } else {
                Token::TextDim
            }
        };
        match self {
            Self::Stepper(value) | Self::Choice(value) => {
                let (previous, next) = match self {
                    Self::Stepper(_) => (Symbol::StepDecrease, Symbol::StepIncrease),
                    _ => (Symbol::StepPrevious, Symbol::StepNext),
                };
                let middle = usize::from(rect.width.saturating_sub(STEP * 2));
                let line = Line::from(vec![
                    Span::styled(
                        format!(" {} ", theme::glyph(previous)),
                        theme::fg(step_tone(lit.previous)),
                    ),
                    Span::styled(
                        format!("{:^middle$}", text::elide(value, middle)),
                        theme::fg(value_tone),
                    ),
                    Span::styled(
                        format!(" {} ", theme::glyph(next)),
                        theme::fg(step_tone(lit.next)),
                    ),
                ]);
                frame.render_widget(Paragraph::new(line), rect);
                let step = STEP.min(rect.width / 2);
                Placed {
                    previous: Some(Rect::new(rect.x, rect.y, step, 1)),
                    value: Rect::new(rect.x + step, rect.y, rect.width - step * 2, 1),
                    next: Some(Rect::new(rect.right() - step, rect.y, step, 1)),
                }
            }
            Self::Toggle(on) => {
                let width = usize::from(rect.width);
                let tone = if *on {
                    value_tone
                } else {
                    step_tone(lit.value)
                };
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        format!("{:^width$}", toggle_word(*on)),
                        theme::fg(tone),
                    )),
                    rect,
                );
                Placed {
                    previous: None,
                    value: rect,
                    next: None,
                }
            }
            Self::Field { text: value, quiet } => {
                let room = usize::from(rect.width.saturating_sub(2));
                let tone = if *quiet && !lit.value {
                    Token::TextDim
                } else {
                    value_tone
                };
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        format!(" {:<room$} ", text::elide(value, room)),
                        theme::fg(tone).bg(theme::color(Token::SurfaceRecessed)),
                    )),
                    rect,
                );
                Placed {
                    previous: None,
                    value: rect,
                    next: None,
                }
            }
        }
    }
}

/// A switch's word, after the mark that says it at a glance.
fn toggle_word(on: bool) -> String {
    if on {
        format!("{} on", theme::glyph(Symbol::MarkToggleOn))
    } else {
        format!("{} off", theme::glyph(Symbol::MarkToggleOff))
    }
}
