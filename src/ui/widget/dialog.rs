//! A dialog: a question the operator answers, or a notice they dismiss,
//! centred over whatever asked it.
//!
//! It lived in the management surface, which meant a surface in the
//! workspace had no way to ask before doing something it cannot undo but
//! a row of a menu, which reads as one more option rather than a question.

use ratatui::{
    layout::{Constraint, Rect},
    text::{Line, Span},
    widgets::{Clear, Padding, Paragraph},
};
use uze_theme::Token;

use super::{Align, Button, Field, Surface, button_row, text};
use crate::ui::theme::{self, Symbol};

/// Which of a focus-carrying dialog's two answers is the way out.
pub(crate) const CANCEL: usize = 0;

/// How much is at stake in a dialog's answer. It colours the thing being
/// acted on and the button that acts — the only two places the answer
/// lands — and nothing else, so the dialog reads calm until the eye
/// reaches what it would do.
#[derive(Clone, Copy)]
pub(crate) enum Tone {
    Neutral,
    Caution,
    Danger,
}

impl Tone {
    pub(crate) fn token(self) -> Token {
        match self {
            Self::Neutral => Token::Accent,
            Self::Caution => Token::StateWarning,
            Self::Danger => Token::StateDanger,
        }
    }
}

/// A question the operator answers, or a notice they dismiss.
///
/// Every dialog is the same four things in the same order — what is being
/// asked, of what, what it means, and the answers — so that someone who
/// has read one knows where to look in the next.
pub(crate) struct Dialog<'a> {
    pub(crate) tone: Tone,
    /// What is being asked, as a heading: "Delete profile".
    pub(crate) title: &'a str,
    /// The thing it would happen to, when there is one: the profile's id.
    pub(crate) subject: Option<Line<'static>>,
    /// What answering yes does, one paragraph per entry.
    pub(crate) body: Vec<String>,
    /// The affirmative, in its own word — "Delete", not "OK". `None` makes
    /// the dialog a notice with one way out.
    pub(crate) confirm: Option<&'a str>,
    /// Which answer the keyboard is on, for the dialogs that carry one.
    pub(crate) focus: Option<usize>,
    /// What the answer needs typed, for the dialogs that ask for a line of
    /// text: drawn after the explanation and before the answers, so it is
    /// read, then filled, then answered.
    pub(crate) field: Option<Field<'a>>,
}

/// Where a dialog landed: the whole of it, which a click on is not an
/// answer, and each of its buttons with the tag it was given, the way out
/// first.
pub(crate) struct Answers<T> {
    pub(crate) popup: Rect,
    pub(crate) buttons: Vec<(Rect, T)>,
}

/// The widest a dialog is drawn: a sentence across a whole terminal is
/// read as a strip, not a sentence.
const WIDTH: u16 = 60;
/// The breathing room between the border and everything inside it.
const PAD_X: u16 = 3;

/// A dialog, laid out from its content: a heading and its subject, the
/// explanation wrapped to the dialog's measure, and the answers on the
/// right, the affirmative last — where the eye ends up after reading. How
/// to answer from the keyboard sits in the bottom border, out of the way
/// of the reading, looked up in `scopes` — the asker's to say, since the
/// same answer is a different chord on a different screen. The height
/// follows the wrapped text, so nothing is cut.
///
/// The caller registers the buttons ahead of whatever the dialog covers.
pub(crate) fn render<T: Clone>(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    dialog: &Dialog<'_>,
    scopes: &[uze_keys::Scope],
    cancel: T,
    confirm: T,
) -> Answers<T> {
    let width = WIDTH.min(area.width.saturating_sub(4));
    let measure = usize::from(width.saturating_sub(2 + PAD_X * 2).max(1));
    let hue = dialog.tone.token();

    let mut lines = vec![
        Line::default(),
        Line::from(Span::styled(
            dialog.title.to_owned(),
            theme::fg_bold(Token::TextBright),
        )),
    ];
    if let Some(subject) = &dialog.subject {
        let mut subject = subject.clone();
        if let Some(first) = subject.spans.first_mut() {
            first.style = theme::fg_bold(hue);
        }
        lines.push(subject);
    }
    for (index, paragraph) in dialog.body.iter().enumerate() {
        lines.push(Line::default());
        let style = if index == 0 {
            theme::fg(Token::TextSecondary)
        } else {
            theme::fg(Token::TextMuted)
        };
        lines.extend(
            text::fold(paragraph, measure)
                .into_iter()
                .map(|line| Line::from(Span::styled(line, style))),
        );
    }
    lines.push(Line::default());
    // The field's text row, then the rule under it that makes it a field.
    let field_row = dialog.field.as_ref().map(|_| {
        let row = lines.len() as u16;
        lines.extend([Line::default(), Line::default(), Line::default()]);
        row
    });
    let buttons_row = lines.len() as u16;
    // The row the buttons are drawn over, then the same air below them as
    // above the heading.
    lines.push(Line::default());
    lines.push(Line::default());

    let height = (lines.len() as u16 + 2).min(area.height.saturating_sub(2));
    let popup = area.centered(Constraint::Length(width), Constraint::Length(height));
    frame.render_widget(Clear, popup);
    // Wider than a popup's own inset and with no row above: a dialog's
    // first line is a question, and it is read across rather than down.
    let inner = Surface::floating()
        .hint(hint_line(dialog, scopes))
        .padding(Padding::horizontal(PAD_X))
        .render(frame, popup);
    frame.render_widget(Paragraph::new(lines), inner);
    if let (Some(field), Some(row)) = (&dialog.field, field_row)
        && row + 2 <= inner.height
    {
        field.render(frame, Rect::new(inner.x, inner.y + row, inner.width, 2));
    }
    let buttons = if buttons_row < inner.height {
        buttons(
            frame,
            Rect::new(inner.x, inner.y + buttons_row, inner.width, 1),
            dialog,
            cancel,
            confirm,
        )
    } else {
        Vec::new()
    };
    Answers { popup, buttons }
}

/// `enter delete · esc cancel` — the dialog's own words for what each key
/// does now. Enter takes the answer the keyboard is on, so while that is
/// the way out the hint says so, and names the key that reaches the other.
fn hint_line(dialog: &Dialog<'_>, scopes: &[uze_keys::Scope]) -> Line<'static> {
    use uze_keys::Action;
    let Some(confirm) = dialog.confirm.map(str::to_lowercase) else {
        return border_hint(scopes, &[(Action::Dismiss, "close")]);
    };
    let answers = if dialog.focus == Some(CANCEL) {
        [
            (Action::Activate, "cancel"),
            (Action::FocusNext, confirm.as_str()),
        ]
    } else {
        [
            (Action::Activate, confirm.as_str()),
            (Action::Dismiss, "cancel"),
        ]
    };
    border_hint(scopes, &answers)
}

/// The keys a dialog answers to, as its bottom border carries them.
pub(crate) fn border_hint(
    scopes: &[uze_keys::Scope],
    answers: &[(uze_keys::Action, &str)],
) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    spans.extend(answer_spans(scopes, answers));
    spans.push(Span::raw(" "));
    Line::from(spans)
}

/// Where a dialog drawn by [`shell`] puts its content: the rows under its
/// title.
pub(crate) struct Shell {
    pub(crate) body: Rect,
}

/// Rows a [`shell`] spends around its body: the two borders, the air
/// above the title, the title, and the air on either side of the body.
pub(crate) const SHELL_ROWS: u16 = 6;

/// The frame every dialog that is not a question is drawn in — a list to
/// pick from, a glossary, a report — so it reads as the same object a
/// question does: a plain border, a row of air, the title in bold inside
/// rather than on the border, the content `PAD_X` in from the sides, and
/// the keys that answer it in the bottom border.
///
/// `width` is the dialog's own measure, bounded by `area`; `body_rows` is
/// how many rows its content wants, bounded the same way.
pub(crate) fn shell(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    width: u16,
    body_rows: u16,
    title: &str,
    hint: Line<'static>,
) -> Shell {
    let width = width.min(area.width.saturating_sub(4));
    let height = (body_rows + SHELL_ROWS).min(area.height.saturating_sub(2));
    let popup = area.centered(Constraint::Length(width), Constraint::Length(height));
    frame.render_widget(Clear, popup);
    let inner = Surface::floating()
        .hint(hint)
        .padding(Padding::horizontal(PAD_X))
        .render(frame, popup);
    if inner.height > 1 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                title.to_owned(),
                theme::fg_bold(Token::TextBright),
            )),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
    }
    let body = Rect::new(
        inner.x,
        inner.y + 3,
        inner.width,
        inner.height.saturating_sub(4),
    );
    Shell { body }
}

/// The columns a [`shell`] keeps between its border and its content, on
/// each side — what a caller sizing its rows to the body measures with.
pub(crate) const fn shell_chrome_width() -> u16 {
    2 + PAD_X * 2
}

/// A dialog's answers in its own words, each after the key that reaches it
/// in `scopes`. An answer with no key there is left out rather than
/// printed keyless: it is still a button.
pub(crate) fn answer_spans(
    scopes: &[uze_keys::Scope],
    answers: &[(uze_keys::Action, &str)],
) -> Vec<Span<'static>> {
    let keymap = uze_keys::active();
    let mut spans = Vec::new();
    for (action, word) in answers {
        let Some(chord) = keymap.chord_for(*action, scopes) else {
            continue;
        };
        if !spans.is_empty() {
            spans.push(Span::styled(
                format!(" {} ", theme::glyph(Symbol::HintSeparator)),
                theme::fg(Token::TextDim),
            ));
        }
        spans.push(Span::styled(
            chord.to_string(),
            theme::fg(Token::TextSecondary),
        ));
        spans.push(Span::styled(
            format!(" {word}"),
            theme::fg(Token::TextMuted),
        ));
    }
    spans
}

/// The answers, right-aligned, as targets: the way out first, the
/// affirmative last. The one the keyboard is on is drawn solid, the other
/// soft — the same two weights every button in the product has. With no
/// focus to carry, the affirmative is the solid one, which is what a
/// question looks like before anything has moved.
fn buttons<T: Clone>(
    frame: &mut ratatui::Frame<'_>,
    row: Rect,
    dialog: &Dialog<'_>,
    cancel: T,
    confirm: T,
) -> Vec<(Rect, T)> {
    let on_cancel = dialog.focus == Some(CANCEL) || dialog.confirm.is_none();
    let mut buttons = vec![(
        Button::new(
            if dialog.confirm.is_some() {
                "Cancel"
            } else {
                "Close"
            },
            Token::TextSecondary,
        )
        .strong(on_cancel),
        cancel,
    )];
    if let Some(label) = dialog.confirm {
        buttons.push((
            Button::new(label, dialog.tone.token()).strong(!on_cancel),
            confirm,
        ));
    }
    button_row(frame, row, &buttons, Align::Right)
}
