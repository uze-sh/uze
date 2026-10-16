//! The hint line: what can be done here, and the keys that do it.
//!
//! Read from the live keymap rather than written out, so a rebound key
//! says its new chord everywhere without anyone remembering to update a
//! caption. An action with no chord in these scopes is left out entirely —
//! a hint naming a key that does not exist is worse than no hint.
//!
//! A hint of one key is also the action it names: it comes back as a
//! target, and a click on it is the caller's to perform exactly as the key
//! would be. That is why the line draws itself rather than being handed
//! over as text — only what placed it knows where each hint landed.

use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use uze_keys::{Action, Chord, Scope};
use uze_theme::Token;

use super::text;
use crate::ui::theme::{self, Symbol};

/// A row of hints for one place, built entry by entry and then drawn.
///
/// The one way a surface may name a key.
#[derive(Clone, Debug)]
pub(crate) struct Hints<'a> {
    scopes: &'a [Scope],
    entries: Vec<Entry>,
    hovered: Option<Action>,
    tone: Option<Token>,
}

/// One hint: a key and what it does, or a run of keys that do the same
/// thing to different ends — up and down move, the digits pick a tab —
/// written once as the two ends of the run.
///
/// A run is the keyboard's own way through a list, and is no target: the
/// pointer walks the same list by pointing at what it wants.
#[derive(Clone, Debug)]
enum Entry {
    Key(Action, String),
    Run(Action, Action, String),
}

/// Where one hint of the line landed, in columns from its start.
struct Placed {
    offset: u16,
    width: u16,
    action: Action,
}

impl<'a> Hints<'a> {
    /// No hints yet, each key to be read from `scopes`.
    pub(crate) fn new(scopes: &'a [Scope]) -> Self {
        Self {
            scopes,
            entries: Vec::new(),
            hovered: None,
            tone: None,
        }
    }

    /// `action`, under its own name.
    pub(crate) fn key(self, action: Action) -> Self {
        self.named(action, action.label())
    }

    /// `action`, under the name the caller gives it: one key that does a
    /// different thing on each row says the thing it does here.
    pub(crate) fn named(mut self, action: Action, name: impl Into<String>) -> Self {
        self.entries.push(Entry::Key(action, name.into()));
        self
    }

    /// A run from `first` to `last`, written as its two ends. Read, never
    /// pointed at.
    pub(crate) fn run(mut self, first: Action, last: Action, name: impl Into<String>) -> Self {
        self.entries.push(Entry::Run(first, last, name.into()));
        self
    }

    /// The action under the pointer: its hint is drawn lit.
    pub(crate) fn hovered(mut self, action: Option<Action>) -> Self {
        self.hovered = action;
        self
    }

    /// The whole line in `tone` rather than the accent: a line that asks
    /// something rather than listing what can be done.
    pub(crate) fn tone(mut self, tone: Token) -> Self {
        self.tone = Some(tone);
        self
    }

    /// The columns the whole line takes, for a caller sizing the slot it
    /// will go in.
    pub(crate) fn width(&self) -> u16 {
        self.layout(u16::MAX).0.width() as u16
    }

    /// Draws what fits of the line into `rect`, answering with where each
    /// key's hint landed, its name included. A hint left out for want of
    /// room is no target.
    pub(crate) fn render(&self, frame: &mut ratatui::Frame<'_>, rect: Rect) -> Vec<(Rect, Action)> {
        if rect.width == 0 || rect.height == 0 {
            return Vec::new();
        }
        let (line, placed) = self.layout(rect.width);
        frame.render_widget(Paragraph::new(line), rect);
        placed
            .into_iter()
            .map(|hint| {
                (
                    Rect::new(rect.x + hint.offset, rect.y, hint.width, 1),
                    hint.action,
                )
            })
            .collect()
    }

    /// The line as text, for a hint written inside text that scrolls: there
    /// is no fixed place to point at, so it answers no click.
    pub(crate) fn line(&self) -> Line<'static> {
        self.layout(u16::MAX).0
    }

    /// The line, holding only what fits in `width`, and where its keys sit.
    ///
    /// A row of hints is a list, and a list cut at the edge ends in half a
    /// word — `· m` where `m map` was meant, which reads as a key nobody
    /// can press. So the last one that does not fit is left out instead,
    /// with its separator: what is offered here is already a subset of
    /// what the surface can do, and a pointer reaches the rest.
    fn layout(&self, width: u16) -> (Line<'static>, Vec<Placed>) {
        let keymap = uze_keys::active();
        let separator = theme::glyph(Symbol::HintSeparator);
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut placed = Vec::new();
        let mut used = 0usize;
        for entry in &self.entries {
            let Some((keys, action, name)) = self.resolve(entry, &keymap) else {
                continue;
            };
            let lead = match spans.is_empty() {
                true => String::new(),
                false => format!(" {separator} "),
            };
            let label = format!(" {}", name.to_lowercase());
            let lead_width = text::columns(&lead);
            let hint_width = text::columns(&keys) + text::columns(&label);
            let cost = lead_width + hint_width;
            if used + cost > width as usize {
                break;
            }
            if !lead.is_empty() {
                spans.push(Span::styled(lead, theme::fg(Token::TextFaint)));
            }
            // The name is as much the target as the key: the pointer aims
            // at the words as often as at the letter.
            if let Some(action) = action {
                placed.push(Placed {
                    offset: (used + lead_width) as u16,
                    width: hint_width as u16,
                    action,
                });
            }
            let lit = action.is_some() && action == self.hovered;
            spans.push(Span::styled(keys, self.key_style()));
            spans.push(Span::styled(label, self.label_style(lit)));
            used += cost;
        }
        (Line::from(spans), placed)
    }

    /// What `entry` draws as its keys, the action a click on it performs,
    /// and its name. `None` when a key it names is bound to nothing here.
    fn resolve<'e>(
        &self,
        entry: &'e Entry,
        keymap: &uze_keys::Keymap,
    ) -> Option<(String, Option<Action>, &'e str)> {
        match entry {
            Entry::Key(action, name) => {
                let chord = keymap.chord_for(*action, self.scopes)?;
                Some((key_text(chord), Some(*action), name))
            }
            Entry::Run(first, last, name) => {
                let first = keymap.chord_for(*first, self.scopes)?;
                let last = keymap.chord_for(*last, self.scopes)?;
                Some((run_text(first, last), None, name))
            }
        }
    }

    fn hue(&self) -> Token {
        self.tone.unwrap_or(Token::Accent)
    }

    fn key_style(&self) -> Style {
        theme::fg_bold(self.hue())
    }

    /// A lit label takes its key's colour and nothing else of it, so the
    /// whole hint answers the pointer, the same way on every surface,
    /// without its width or weight moving under the pointer.
    fn label_style(&self, lit: bool) -> Style {
        match (lit, self.tone) {
            (true, _) => theme::fg(self.hue()),
            (false, Some(tone)) => theme::fg(tone),
            (false, None) => theme::fg(Token::TextMuted),
        }
    }
}

/// A key as a hint spells it: an arrow key as the arrow the theme draws,
/// which is what a run of them reads as; anything else as its chord.
fn key_text(chord: Chord) -> String {
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
fn run_text(first: Chord, last: Chord) -> String {
    let (first_text, last_text) = (key_text(first), key_text(last));
    let arrows = first_text != first.to_string() && last_text != last.to_string();
    if arrows {
        format!("{first_text}{last_text}")
    } else {
        format!("{first_text}–{last_text}")
    }
}
