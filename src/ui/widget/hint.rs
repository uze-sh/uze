//! The hint line: what can be done here, and the keys that do it.
//!
//! Read from the live keymap rather than written out, so a rebound key
//! says its new chord everywhere without anyone remembering to update a
//! caption. An action with no chord in these scopes is left out entirely —
//! a hint naming a key that does not exist is worse than no hint.

use ratatui::text::{Line, Span};
use uze_theme::Token;

use crate::ui::theme::{self, Symbol};

/// A hint line for `actions`, each printed with the key that reaches it
/// in `scopes`.
///
/// The one way a surface may name a key. An action with no chord in these
/// scopes is skipped rather than printed keyless: a hint is a list of
/// shortcuts, and what has none is offered somewhere a pointer can reach.
pub(crate) fn line(scopes: &[uze_keys::Scope], actions: &[uze_keys::Action]) -> Line<'static> {
    within(u16::MAX, scopes, actions)
}

/// The same line, holding only what fits in `width`.
///
/// A row of hints is a list, and a list cut at the edge ends in half a
/// word — `· m` where `m map` was meant, which reads as a key nobody can
/// press. So the last one that does not fit is left out instead, with its
/// separator: what is offered here is already a subset of what the
/// surface can do, and a pointer reaches the rest.
pub(crate) fn within(
    width: u16,
    scopes: &[uze_keys::Scope],
    actions: &[uze_keys::Action],
) -> Line<'static> {
    let named: Vec<(uze_keys::Action, String)> = actions
        .iter()
        .map(|action| (*action, action.label().to_owned()))
        .collect();
    named_within(width, scopes, &named)
}

/// The same line, each action under the name the caller gives it: one key
/// that does a different thing on each row says the thing it does here.
pub(crate) fn named_within(
    width: u16,
    scopes: &[uze_keys::Scope],
    actions: &[(uze_keys::Action, String)],
) -> Line<'static> {
    let entries: Vec<Entry> = actions
        .iter()
        .map(|(action, name)| Entry::Key(*action, name.clone()))
        .collect();
    entries_within(width, scopes, &entries)
}

/// One hint: a key and what it does, or a run of keys that do the same
/// thing to different ends — up and down move, the digits pick a tab —
/// written once as the two ends of the run.
#[derive(Clone, Debug)]
pub(crate) enum Entry {
    Key(uze_keys::Action, String),
    Run(uze_keys::Action, uze_keys::Action, String),
}

/// The line for `entries`, holding only what fits in `width`.
pub(crate) fn entries_within(
    width: u16,
    scopes: &[uze_keys::Scope],
    entries: &[Entry],
) -> Line<'static> {
    let keymap = uze_keys::active();
    let separator = theme::glyph(Symbol::HintSeparator);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    for entry in entries {
        let (keys, name) = match entry {
            Entry::Key(action, name) => match keymap.chord_for(*action, scopes) {
                Some(chord) => (key_text(chord), name),
                None => continue,
            },
            Entry::Run(first, last, name) => {
                match (
                    keymap.chord_for(*first, scopes),
                    keymap.chord_for(*last, scopes),
                ) {
                    (Some(first), Some(last)) => (run_text(first, last), name),
                    _ => continue,
                }
            }
        };
        let lead = match spans.is_empty() {
            true => String::new(),
            false => format!(" {separator} "),
        };
        let label = format!(" {}", name.to_lowercase());
        let cost = lead.chars().count() + keys.chars().count() + label.chars().count();
        if used + cost > width as usize {
            break;
        }
        used += cost;
        if !lead.is_empty() {
            spans.push(Span::styled(lead, theme::fg(Token::TextFaint)));
        }
        spans.push(Span::styled(keys, theme::fg_bold(Token::Accent)));
        spans.push(Span::styled(label, theme::fg(Token::TextMuted)));
    }
    Line::from(spans)
}

/// A key as a hint spells it: an arrow key as the arrow the theme draws,
/// which is what a run of them reads as; anything else as its chord.
fn key_text(chord: uze_keys::Chord) -> String {
    let arrow = match chord.to_string().as_str() {
        "up" => Some(Symbol::ArrowUp),
        "down" => Some(Symbol::ArrowDown),
        "left" => Some(Symbol::ArrowLeft),
        "right" => Some(Symbol::ArrowRight),
        _ => None,
    };
    arrow.map_or_else(|| chord.to_string(), theme::glyph)
}

/// A run's two ends: arrows side by side, which is how a pair of them is
/// read, and anything else as a range.
fn run_text(first: uze_keys::Chord, last: uze_keys::Chord) -> String {
    let (first_text, last_text) = (key_text(first), key_text(last));
    let arrows = first_text != first.to_string() && last_text != last.to_string();
    if arrows {
        format!("{first_text}{last_text}")
    } else {
        format!("{first_text}–{last_text}")
    }
}
