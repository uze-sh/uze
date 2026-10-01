//! TUI — overlay state transitions and their rendering.

use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use uze_keys::Action;

use super::hit::Hit;
use super::model::{Confirmation, Focus, Overlay, TrustedRetry, TuiModel};
use super::worker::{Intent, TrustGrant};
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::dialog::{self, CANCEL, Dialog, Tone};
use crate::ui::widget::{Field, Surface, action_index, hint};

impl TuiModel {
    /// One action, answered by whichever overlay is open.
    ///
    /// Only the actions an overlay's own surface offers reach it — a
    /// question on screen is answered, dismissed, or left alone, and a
    /// keystroke that means nothing here no longer closes it by accident.
    pub(crate) fn overlay_action(&mut self, action: Action) -> Intent {
        if let Overlay::ReleaseNotes(modal) = &mut self.overlay {
            use crate::ui::release_notes::Outcome;
            return match modal.act(action) {
                Outcome::None => Intent::None,
                Outcome::OpenLink(url) => Intent::OpenLink(url),
                Outcome::Close => {
                    self.close_overlay();
                    Intent::None
                }
            };
        }
        let overlay = self.overlay.clone();
        match overlay {
            Overlay::None | Overlay::HarnessHelp | Overlay::Health | Overlay::ReleaseNotes(_) => {
                Intent::None
            }
            Overlay::ActionIndex {
                scopes,
                filter,
                selected,
            } => {
                let rows = self.action_index_rows(&scopes, &filter);
                match action {
                    Action::SelectNext => {
                        if let Overlay::ActionIndex { selected, .. } = &mut self.overlay {
                            *selected = (*selected + 1).min(rows.len().saturating_sub(1));
                        }
                        Intent::None
                    }
                    Action::SelectPrevious => {
                        if let Overlay::ActionIndex { selected, .. } = &mut self.overlay {
                            *selected = selected.saturating_sub(1);
                        }
                        Intent::None
                    }
                    Action::Activate => {
                        let chosen = rows.get(selected).map(|(action, _)| *action);
                        self.close_overlay();
                        match chosen {
                            // Performing from the index is performing: the
                            // row that did it is also the row that showed
                            // the key, which is how anyone learns one.
                            Some(action) => self.act(action),
                            None => Intent::None,
                        }
                    }
                    Action::Dismiss => {
                        self.close_overlay();
                        Intent::None
                    }
                    Action::EraseBack => self.erase_character(),
                    _ => Intent::None,
                }
            }
            Overlay::Confirm { kind, focus } => match action {
                Action::FocusNext | Action::FocusPrevious if focus.is_some() => {
                    self.overlay = Overlay::Confirm {
                        kind,
                        focus: focus.map(|focus| 1 - focus),
                    };
                    Intent::None
                }
                // A notice has no affirmative: `yes` is not an answer to it.
                Action::ConfirmYes if kind.is_notice() => Intent::None,
                Action::Activate if focus == Some(CANCEL) || kind.is_notice() => {
                    self.close_overlay();
                    Intent::None
                }
                Action::Activate | Action::ConfirmYes => {
                    self.close_overlay();
                    kind.intent(self)
                }
                Action::ConfirmNo | Action::Dismiss => {
                    self.close_overlay();
                    Intent::None
                }
                _ => Intent::None,
            },
            Overlay::ThemePicker { themes, selected } => match action {
                Action::SelectNext => {
                    let last = themes.len().saturating_sub(1);
                    self.overlay = Overlay::ThemePicker {
                        themes,
                        selected: (selected + 1).min(last),
                    };
                    Intent::None
                }
                Action::SelectPrevious => {
                    self.overlay = Overlay::ThemePicker {
                        themes,
                        selected: selected.saturating_sub(1),
                    };
                    Intent::None
                }
                Action::Activate => {
                    let chosen = themes.get(selected).cloned();
                    self.close_overlay();
                    match chosen {
                        Some((id, _)) => Intent::SelectTheme(id),
                        None => Intent::None,
                    }
                }
                Action::Dismiss => {
                    self.close_overlay();
                    Intent::None
                }
                _ => Intent::None,
            },
            Overlay::AddMarketplace(input) => match action {
                Action::Activate => {
                    let source = input.trim().to_owned();
                    self.close_overlay();
                    if source.is_empty() {
                        Intent::None
                    } else {
                        Intent::AddMarketplace(source)
                    }
                }
                Action::Dismiss => {
                    self.close_overlay();
                    Intent::None
                }
                Action::EraseBack => {
                    let mut input = input;
                    input.pop();
                    self.overlay = Overlay::AddMarketplace(input);
                    Intent::None
                }
                _ => Intent::None,
            },
            Overlay::NewProfile(input) => match action {
                Action::Activate => {
                    let id = slugify(&input);
                    self.close_overlay();
                    if id.is_empty() {
                        Intent::None
                    } else {
                        Intent::CreateProfile(id)
                    }
                }
                Action::Dismiss => {
                    self.close_overlay();
                    Intent::None
                }
                Action::EraseBack => {
                    let mut input = input;
                    input.pop();
                    self.overlay = Overlay::NewProfile(input);
                    Intent::None
                }
                _ => Intent::None,
            },
        }
    }

    pub(crate) fn close_overlay(&mut self) {
        self.overlay = Overlay::None;
        self.focus = Focus::Content;
    }
}

/// Everything that can be done here, each with the key that reaches it.
///
/// This is the help and the command palette at once, because they answer
/// the same question and two lists would eventually disagree. Nothing here
/// is written down: every row's words come from the action and every key
/// from the keymap, so a rebound key is right here without anyone editing
/// this function — which is exactly what the hand-typed list it replaced
/// could not promise, and had already broken for nine of its bindings.
pub(crate) fn render_action_index(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    scopes: &[uze_keys::Scope],
    filter: &str,
    selected: usize,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let rows = model.action_index_rows(scopes, filter);
    let reachable = model.action_index_rows(scopes, "").len();
    let entries = action_index::render(
        frame,
        area,
        &rows,
        reachable,
        filter,
        selected,
        Hit::ActionIndexEntry,
    );
    // Prepended, so the list underneath cannot answer a click meant here.
    hits.splice(0..0, entries);
}

/// The Harnesses screen's glossary — everything that screen's compact
/// glyphs/labels stand for, written out in plain language. Kept separate
/// from the generic `Help` keybinding overlay: this is reference material
/// about what the data *means*, not what a key *does*.
pub(crate) fn render_harness_help(frame: &mut ratatui::Frame<'_>, area: Rect) {
    // Width covers the longest label ("Not implemented", 16 chars) plus at
    // least one separating space — `{:<N}` never truncates or forces a gap
    // once content already reaches N, so anything shorter than the longest
    // label here would glue straight into the detail text that follows.
    let scopes = [
        uze_keys::Scope::Global,
        uze_keys::Scope::Management,
        uze_keys::Scope::Harnesses,
    ];
    let keymap = uze_keys::active();
    let key = |action| keymap.chord_for(action, &scopes);
    let setup_note = match key(Action::SetupHarness) {
        Some(chord) => format!(
            "Not on this machine, or on it and never handed to UZE — press {chord} to run setup."
        ),
        None => {
            "Not on this machine, or on it and never handed to UZE — set it up from its drawer."
                .to_owned()
        }
    };
    let reconcile_keys: Vec<String> = [
        (Action::AnalyzeContext, "analyze"),
        (Action::ApplyContextPlan, "apply"),
    ]
    .into_iter()
    .filter_map(|(action, verb)| key(action).map(|chord| format!("{chord} to {verb}")))
    .collect();
    let reconcile_note = if reconcile_keys.is_empty() {
        "AGENTS.md bridge needs reconciliation.".to_owned()
    } else {
        format!(
            "AGENTS.md bridge needs reconciliation — {}.",
            reconcile_keys.join(", ")
        )
    };
    let entry = |symbol: Symbol, label: &str, color: Color, detail: &str| {
        Line::from(vec![
            Span::styled(
                format!("{} {label:<18}", theme::glyph(symbol)),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(detail.to_owned(), theme::fg(Token::TextMuted)),
        ])
    };
    let heading = |text: &str| {
        Line::from(Span::styled(
            text.to_owned(),
            Style::default()
                .fg(theme::color(Token::TextBright))
                .add_modifier(Modifier::BOLD),
        ))
    };
    let lines = vec![
        heading("STATUS"),
        // Words rather than marks: a card says its state at its foot,
        // beside the id, and the legend names the words found there.
        // Which of its two shapes a harness not configured is in —
        // missing, or here and not set up — is in its own drawer.
        Line::from(vec![
            Span::styled(
                format!("{:<20}", "Enabled"),
                Style::default()
                    .fg(theme::color(Token::StateSuccess))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "UZE has set it up — ready to receive plugins.",
                theme::fg(Token::TextMuted),
            ),
        ]),
        Line::from(Span::styled(
            format!("{:<20}{}", "Not configured", setup_note),
            theme::fg(Token::TextMuted),
        )),
        Line::from(""),
        heading("COMPATIBILITY (per capability, in the detail panel)"),
        entry(
            Symbol::MarkNative,
            "Native",
            theme::color(Token::Accent),
            "Works directly, no adaptation needed.",
        ),
        entry(
            Symbol::MarkNative,
            "Bridged",
            theme::color(Token::Accent),
            "Routed through UZE's managed AGENTS.md bridge file.",
        ),
        entry(
            Symbol::MarkAttention,
            "Missing/Drifted",
            theme::color(Token::StateWarning),
            &reconcile_note,
        ),
        entry(
            Symbol::MarkClose,
            "Conflict/Blocked",
            theme::color(Token::StateDanger),
            "AGENTS.md bridge has unresolved content UZE won't overwrite.",
        ),
        entry(
            Symbol::MarkAdapted,
            "Adapted",
            theme::color(Token::StateWarning),
            "Works, converted from a different format.",
        ),
        entry(
            Symbol::MarkAdapted,
            "Degraded",
            theme::color(Token::StateWarning),
            "Works, but with reduced fidelity.",
        ),
        entry(
            Symbol::MarkUnsupported,
            "Not supported",
            theme::color(Token::StateDanger),
            "This harness has no route for it.",
        ),
        entry(
            Symbol::MarkUnsupported,
            "Not implemented",
            theme::color(Token::TextMuted),
            "UZE doesn't route this capability anywhere yet.",
        ),
        Line::from(""),
        Line::from(Span::styled(
            "any key to close",
            theme::fg(Token::TextMuted),
        )),
    ];
    // `render_modal`'s fixed 76-column cap was built for the short one-line
    // confirmations every other overlay uses — this glossary's longest
    // lines need more room than that to avoid wrapping and losing the
    // label/detail alignment, so size the popup off its own content instead
    // (still bounded by the real terminal width on anything narrower).
    let width = (lines.iter().map(Line::width).max().unwrap_or(0) as u16 + 4).min(area.width);
    let height = (lines.len() as u16 + 4).min(area.height.saturating_sub(2));
    let popup = area.centered(Constraint::Length(width), Constraint::Length(height));
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines)
            .block(modal(" Harness status ").into_block())
            .wrap(ratatui::widgets::Wrap { trim: true }),
        popup,
    );
}

/// What the footer's health status stands for: every problem an operator
/// can act on, worst first, or a line saying there is none.
pub(crate) fn render_health(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    alerts: &[crate::ui::view::health::Alert],
) {
    use crate::ui::view::health::Severity;
    let mut sorted: Vec<_> = alerts.iter().collect();
    sorted.sort_by_key(|alert| alert.severity);
    let mut lines: Vec<Line<'static>> = if sorted.is_empty() {
        vec![Line::from(Span::styled(
            "Nothing needs attention.",
            theme::fg(Token::TextMuted),
        ))]
    } else {
        sorted
            .into_iter()
            .map(|alert| {
                let (symbol, hue) = match alert.severity {
                    Severity::High => (Symbol::MarkClose, Token::StateDanger),
                    Severity::Medium => (Symbol::MarkAttention, Token::StateWarning),
                    Severity::Low => (Symbol::MarkDot, Token::Accent),
                };
                Line::from(vec![
                    Span::styled(format!("{} ", theme::glyph(symbol)), theme::fg(hue)),
                    Span::styled(alert.label.clone(), theme::fg(Token::TextBright)),
                    Span::styled(format!(" — {}", alert.detail), theme::fg(Token::TextMuted)),
                ])
            })
            .collect()
    };
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "any key to close",
        theme::fg(Token::TextMuted),
    )));
    let width = (lines.iter().map(Line::width).max().unwrap_or(0) as u16 + 4)
        .max(40)
        .min(area.width);
    let height = (lines.len() as u16 + 4).min(area.height.saturating_sub(2));
    let popup = area.centered(Constraint::Length(width), Constraint::Length(height));
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines)
            .block(modal(" Health ").into_block())
            .wrap(ratatui::widgets::Wrap { trim: true }),
        popup,
    );
}

/// What a dialog asking for one line of text says: its heading, what
/// answering does, what the field is waiting for, and the affirmative's own
/// word.
pub(crate) struct TextPrompt<'a> {
    pub(crate) title: &'a str,
    pub(crate) body: &'a str,
    pub(crate) placeholder: &'a str,
    pub(crate) confirm: &'a str,
}

/// A dialog that asks for one line of text — the same dialog every
/// question is, with a field between what it means and the answers.
pub(crate) fn render_text_prompt(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    prompt: &TextPrompt<'_>,
    input: &str,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let keys = dialog::Keys {
        scopes: &[uze_keys::Scope::Global, uze_keys::Scope::TextPrompt],
        yes: Action::Activate,
        no: Action::Dismiss,
    };
    let answers = dialog::render(
        frame,
        area,
        &Dialog {
            tone: Tone::Neutral,
            title: prompt.title,
            subject: None,
            body: vec![prompt.body.to_owned()],
            confirm: Some(prompt.confirm),
            focus: None,
            field: Some(Field::new(input, prompt.placeholder)),
        },
        &keys,
        Hit::OfferedAction(Action::Dismiss),
        Hit::OfferedAction(Action::Activate),
    );
    // The body after the buttons it holds, so a click on the field keeps
    // what was typed rather than closing over it.
    hits.splice(
        0..0,
        answers
            .buttons
            .into_iter()
            .chain([(answers.popup, Hit::OverlayBody)]),
    );
}

/// Normalizes free-text input into a profile-id slug: lowercase, runs of
/// whitespace/underscores collapsed to one `-`, everything else outside
/// `[a-z0-9-]` dropped. Trims leading/trailing `-`. Mirrors
/// `profile_state::validate_id`'s accepted charset (plus `_`, folded into
/// `-` here rather than rejected, since typing a space is the most likely
/// way a user would separate words).
fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;
    for ch in input.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch.is_whitespace() || ch == '_' {
            pending_dash = true;
        }
    }
    slug
}

/// The theme picker: what UZE can be drawn in, and which it is drawn in now.
///
/// Deliberately a plain list with no preview. A preview would have to draw a
/// second palette inside a frame already painted in the first one, which is
/// the one thing a terminal cannot do convincingly — and the real preview is
/// free: pressing enter repaints everything.
pub(crate) fn render_theme_picker(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    themes: &[(String, bool)],
    selected: usize,
) {
    let width = 46.min(area.width.saturating_sub(4));
    let height = (themes.len() as u16 + 4).min(area.height.saturating_sub(2));
    let popup = area.centered(Constraint::Length(width), Constraint::Length(height));
    frame.render_widget(Clear, popup);
    let inner = modal(" Theme ").render(frame, popup);

    let mut lines: Vec<Line<'static>> = themes
        .iter()
        .enumerate()
        .map(|(index, (id, in_force))| {
            let cursor = if index == selected {
                theme::glyph(Symbol::ChevronCollapsed)
            } else {
                " ".to_owned()
            };
            Line::from(vec![
                Span::styled(format!("{cursor} "), theme::fg(Token::Accent)),
                Span::styled(
                    id.clone(),
                    if index == selected {
                        theme::fg_bold(Token::TextBright)
                    } else {
                        theme::fg(Token::TextPrimary)
                    },
                ),
                Span::styled(
                    if *in_force {
                        format!("  {} in use", theme::glyph(Symbol::StatusSelected))
                    } else {
                        String::new()
                    },
                    theme::fg(Token::TextDim),
                ),
            ])
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(hint::line(
        &[uze_keys::Scope::Global, uze_keys::Scope::ThemePicker],
        &[
            uze_keys::Action::SelectNext,
            uze_keys::Action::Activate,
            uze_keys::Action::Dismiss,
        ],
    ));
    frame.render_widget(Paragraph::new(lines), inner);
}

impl Confirmation {
    /// Whether this only explains, with one way out and nothing to agree to.
    fn is_notice(&self) -> bool {
        matches!(self, Self::ProtectedPlugin(_))
    }

    /// What agreeing asks for.
    fn intent(self, model: &TuiModel) -> Intent {
        match self {
            Self::RemovePlugin(id) => Intent::Remove(id),
            Self::RemoveMarketplace(name) => Intent::RemoveMarketplace(name),
            Self::UpdatePlugin(id) => Intent::Update(id, TrustGrant::Ask),
            Self::InstallPlugin { name, marketplace } => Intent::Install {
                name,
                marketplace,
                grant: TrustGrant::Ask,
            },
            Self::ApplyContext => Intent::ContextApply(model.workspace_root()),
            Self::ProtectedPlugin(_) => Intent::None,
            Self::DeleteProfile(id) => Intent::DeleteProfile(id),
            Self::Trust { retry, .. } => match retry {
                TrustedRetry::Install { name, marketplace } => Intent::Install {
                    name,
                    marketplace,
                    grant: TrustGrant::Granted,
                },
                TrustedRetry::Update(id) => Intent::Update(id, TrustGrant::Granted),
            },
        }
    }

    /// The question as it is drawn.
    fn dialog(&self, focus: Option<usize>) -> Dialog<'_> {
        let dialog = |tone, title, subject: Option<Line<'static>>, body: &str, confirm| Dialog {
            tone,
            title,
            subject,
            body: vec![body.to_owned()],
            confirm,
            focus,
            field: None,
        };
        let named = |id: &str| Some(Line::from(id.to_owned()));
        match self {
            Self::RemovePlugin(id) => dialog(
                Tone::Danger,
                "Remove plugin",
                named(id),
                "Takes back everything it delivered to each harness. If any of it was changed \
                 by hand, nothing is removed.",
                Some("Remove"),
            ),
            Self::RemoveMarketplace(name) => dialog(
                Tone::Danger,
                "Remove marketplace",
                named(name),
                "Removes every plugin it delivered, then the marketplace itself. If any of it \
                 was changed by hand, that plugin and the marketplace stay.",
                Some("Remove"),
            ),
            Self::UpdatePlugin(id) => dialog(
                Tone::Neutral,
                "Update plugin",
                named(id),
                "Moves it to the latest revision its marketplace publishes.",
                Some("Update"),
            ),
            Self::InstallPlugin { name, marketplace } => dialog(
                Tone::Neutral,
                "Install plugin",
                Some(Line::from(vec![
                    Span::raw(name.to_owned()),
                    Span::styled(format!("  from {marketplace}"), theme::fg(Token::TextMuted)),
                ])),
                "Delivered to every harness on this machine, from one copy.",
                Some("Install"),
            ),
            Self::ApplyContext => dialog(
                Tone::Caution,
                "Apply context changes",
                None,
                "Reconciles AGENTS.md and the bridge each harness reads.",
                Some("Apply"),
            ),
            Self::ProtectedPlugin(id) => dialog(
                Tone::Caution,
                "Protected plugin",
                named(id),
                "An official marketplace plugin can't be removed from here. Install it from a \
                 custom source to make it removable.",
                None,
            ),
            Self::DeleteProfile(id) => dialog(
                Tone::Danger,
                "Delete profile",
                named(id),
                "Removes UZE's own record of this profile. No harness configuration is touched.",
                Some("Delete"),
            ),
            Self::Trust { plugin, detail, .. } => Dialog {
                body: vec![
                    "It declares an executable capability that was not trusted before:".to_owned(),
                    detail.clone(),
                ],
                ..dialog(
                    Tone::Caution,
                    "Trust required",
                    named(plugin),
                    "",
                    Some("Trust and continue"),
                )
            },
        }
    }
}

/// A question on screen, drawn with the answer the keyboard is on.
pub(crate) fn render_confirmation(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    kind: &Confirmation,
    focus: Option<usize>,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let keys = dialog::Keys {
        scopes: &[uze_keys::Scope::Global, uze_keys::Scope::Confirm],
        yes: Action::ConfirmYes,
        no: Action::Dismiss,
    };
    let targets = dialog::render(
        frame,
        area,
        &kind.dialog(focus),
        &keys,
        Hit::OfferedAction(Action::ConfirmNo),
        Hit::OfferedAction(Action::ConfirmYes),
    );
    // Prepended, because the dialog is drawn over whatever was behind it
    // and that is still in the hit list underneath.
    hits.splice(0..0, targets.buttons);
}

/// The titled modal surface. Callers must render `Clear` over the rect
/// first so leftover content underneath cannot bleed through.
///
/// It carried its own `Padding::new(1, 1, 1, 0)` and took the title's
/// colour as an argument. Every caller passed the accent, and the inset
/// was the one in the UI that did not reach for [`POPUP_H_PAD`] — both are
/// [`Surface`]'s now.
fn modal(title: impl Into<Line<'static>>) -> Surface {
    Surface::floating().title(title)
}
