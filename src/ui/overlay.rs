//! TUI — overlay state transitions and their rendering.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use uze_keys::Action;

use super::hit::Hit;
use super::model::{Confirmation, Focus, Overlay, TrustedRetry, TuiModel};
use super::worker::{Intent, TrustGrant};
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::dialog::{self, CANCEL, Dialog, Tone};
use crate::ui::widget::{Field, action_index, text};

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
            // The arrows move between the two answers — a question with no
            // focus yet stands on its affirmative, which is how it is drawn
            // — and enter takes the one the keyboard is on.
            Overlay::Confirm { kind, focus } => match action {
                Action::FocusNext | Action::FocusPrevious if !kind.is_notice() => {
                    self.overlay = Overlay::Confirm {
                        kind,
                        focus: Some(1 - focus.unwrap_or(1)),
                    };
                    Intent::None
                }
                Action::Activate if focus == Some(CANCEL) || kind.is_notice() => {
                    self.close_overlay();
                    Intent::None
                }
                Action::Activate => {
                    self.close_overlay();
                    kind.intent(self)
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
            Overlay::Preferences(editor) => self.preferences_action(editor, action),
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

    /// A key on an extension's preferences: the arrows move between the
    /// rows, Enter answers the row (flips, moves on, or starts typing), and
    /// Esc puts the typing away, then the editor.
    fn preferences_action(
        &mut self,
        mut editor: crate::ui::model::PreferencesEditor,
        action: Action,
    ) -> Intent {
        let Some(row) = editor.rows.get(editor.selected).cloned() else {
            self.close_overlay();
            return Intent::None;
        };
        let set = |editor: &mut crate::ui::model::PreferencesEditor, value: serde_json::Value| {
            // A secret's row keeps only whether it is set: what was typed
            // goes to the keychain and is never shown again.
            editor.rows[editor.selected].value = if row.is_secret() {
                serde_json::Value::Bool(value.as_str().is_some_and(|typed| !typed.is_empty()))
            } else {
                value.clone()
            };
            Intent::SetPreference {
                identity: editor.identity.clone(),
                name: row.name.clone(),
                label: row.label.clone(),
                value,
            }
        };
        let intent = match (&mut editor.typing, action) {
            (Some(_), Action::Dismiss) => {
                editor.typing = None;
                Intent::None
            }
            (Some(typed), Action::EraseBack) => {
                typed.pop();
                Intent::None
            }
            (Some(typed), Action::Activate) => match row.typed(typed) {
                Some(value) => {
                    editor.typing = None;
                    set(&mut editor, value)
                }
                None => Intent::None,
            },
            (None, Action::SelectNext) => {
                editor.selected = (editor.selected + 1).min(editor.rows.len().saturating_sub(1));
                Intent::None
            }
            (None, Action::SelectPrevious) => {
                editor.selected = editor.selected.saturating_sub(1);
                Intent::None
            }
            (None, Action::Activate) => match row.pressed() {
                Some(value) => set(&mut editor, value),
                None => {
                    editor.typing = Some(if row.is_secret() {
                        String::new()
                    } else {
                        row.said()
                    });
                    Intent::None
                }
            },
            (None, Action::Dismiss) => {
                self.close_overlay();
                return Intent::None;
            }
            _ => Intent::None,
        };
        self.overlay = Overlay::Preferences(editor);
        intent
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
    let mut answering = scopes.to_vec();
    answering.push(uze_keys::Scope::ActionIndex);
    let hint = dialog::border_hint(
        &answering,
        &[(Action::Activate, "run"), (Action::Dismiss, "close")],
    );
    let entries = action_index::render(
        frame,
        area,
        &rows,
        reachable,
        filter,
        selected,
        hint,
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
    const WIDTH: u16 = 84;
    const LABEL: usize = 20;
    let measure = usize::from(WIDTH.saturating_sub(dialog::shell_chrome_width()));
    // A label, then its meaning hung beside it: folded under itself rather
    // than back under the label, so the column of labels stays a column.
    let entry = |label: Span<'static>, detail: &str| -> Vec<Line<'static>> {
        text::fold(detail, measure.saturating_sub(LABEL).max(1))
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let lead = if index == 0 {
                    Span::styled(format!("{:<LABEL$}", label.content), label.style)
                } else {
                    Span::raw(" ".repeat(LABEL))
                };
                Line::from(vec![lead, Span::styled(row, theme::fg(Token::TextMuted))])
            })
            .collect()
    };
    let mark = |symbol: Symbol, label: &str, hue: Token| {
        Span::styled(
            format!("{} {label}", theme::glyph(symbol)),
            theme::fg_bold(hue),
        )
    };
    let mut sections = vec![crate::ui::view::section_label("status")];
    sections.extend(entry(
        Span::styled("Enabled", theme::fg_bold(Token::StateSuccess)),
        "UZE has set it up — ready to receive plugins.",
    ));
    sections.extend(entry(
        Span::styled("Not configured", theme::fg_bold(Token::TextSecondary)),
        &setup_note,
    ));
    sections.push(Line::from(""));
    sections.push(crate::ui::view::section_label(
        "compatibility, per capability, in the detail panel",
    ));
    for (symbol, label, hue, detail) in [
        (
            Symbol::MarkNative,
            "Native",
            Token::StateSuccess,
            "Works directly, no adaptation needed.",
        ),
        (
            Symbol::MarkNative,
            "Bridged",
            Token::StateSuccess,
            "Routed through UZE's managed AGENTS.md bridge file.",
        ),
        (
            Symbol::MarkAttention,
            "Missing/Drifted",
            Token::StateWarning,
            reconcile_note.as_str(),
        ),
        (
            Symbol::MarkClose,
            "Conflict/Blocked",
            Token::StateDanger,
            "AGENTS.md bridge has unresolved content UZE won't overwrite.",
        ),
        (
            Symbol::MarkAdapted,
            "Adapted",
            Token::StateWarning,
            "Works, converted from a different format.",
        ),
        (
            Symbol::MarkAdapted,
            "Degraded",
            Token::StateWarning,
            "Works, but with reduced fidelity.",
        ),
        (
            Symbol::MarkUnsupported,
            "Not supported",
            Token::StateDanger,
            "This harness has no route for it.",
        ),
        (
            Symbol::MarkUnsupported,
            "Not implemented",
            Token::TextMuted,
            "UZE doesn't route this capability anywhere yet.",
        ),
    ] {
        sections.extend(entry(mark(symbol, label, hue), detail));
    }
    let shell = dialog::shell(
        frame,
        area,
        WIDTH,
        sections.len() as u16,
        "Harness status",
        dialog::border_hint(&scopes, &[(Action::Dismiss, "close")]),
    );
    frame.render_widget(Paragraph::new(sections), shell.body);
}

/// What the footer's health status stands for: every problem an operator
/// can act on, worst first, or a line saying there is none.
pub(crate) fn render_health(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    alerts: &[crate::ui::view::health::Alert],
) {
    crate::ui::widget::alerts::render(
        frame,
        area,
        "Health",
        alerts,
        dialog::border_hint(
            &[uze_keys::Scope::Global, uze_keys::Scope::Management],
            &[(Action::Dismiss, "close")],
        ),
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
/// An extension's preferences: a card of rows, each its label and value,
/// the selected one filled; or, while one is typed, the field for it.
pub(crate) fn render_preferences(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    editor: &crate::ui::model::PreferencesEditor,
    hits: &mut Vec<(Rect, Hit)>,
) {
    if let (Some(typed), Some(row)) = (&editor.typing, editor.rows.get(editor.selected)) {
        let title = format!("{} · {}", editor.name, row.label);
        let secret_body;
        let body = match row.kind {
            "number" => "A number.",
            "secret" => {
                secret_body = format!(
                    "Kept in {} and never shown again. Leave it empty to forget it.",
                    uze_application::secrets_kept_in()
                );
                &secret_body
            }
            _ => "Text.",
        };
        // A secret is drawn as one dot per character typed.
        let masked: String;
        let shown = if row.is_secret() {
            masked = crate::ui::theme::glyph(uze_theme::Symbol::CursorMasked)
                .repeat(typed.chars().count());
            &masked
        } else {
            typed
        };
        render_text_prompt(
            frame,
            area,
            &TextPrompt {
                title: &title,
                body,
                placeholder: &row.label,
                confirm: "Set",
            },
            shown,
            hits,
        );
        return;
    }
    crate::ui::widget::scrim::render(frame, area);
    let labels: Vec<String> = editor
        .rows
        .iter()
        .map(|row| format!("{}  {}", row.label, row.said()))
        .collect();
    let entries: Vec<crate::ui::widget::menu::Entry<'_>> = labels
        .iter()
        .map(|label| crate::ui::widget::menu::Entry::action(label))
        .collect();
    let center = Rect {
        x: area.x + area.width / 3,
        y: area.y + area.height / 3,
        width: area.width / 3,
        height: 1,
    };
    let rects = crate::ui::widget::menu::render(frame, area, center, &entries, editor.selected);
    let body = rects.iter().copied().reduce(Rect::union);
    hits.splice(
        0..0,
        rects
            .into_iter()
            .enumerate()
            .map(|(index, rect)| (rect, Hit::ExtensionPreferenceRow(index)))
            .chain(body.map(|body| (body, Hit::OverlayBody))),
    );
}

pub(crate) fn render_text_prompt(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    prompt: &TextPrompt<'_>,
    input: &str,
    hits: &mut Vec<(Rect, Hit)>,
) {
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
        &[uze_keys::Scope::Global, uze_keys::Scope::TextPrompt],
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

/// What allowing an extension means, then each grant it wants with what
/// it lets it do: what the operator reads before anything of it runs.
fn extension_grants_body(
    here: &crate::ui::model::ExtensionHere,
    grants: &[String],
    new_grants: &[String],
    band_rows: Option<u8>,
) -> Vec<String> {
    let project = here.project.file_name().map_or_else(
        || here.project.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let mut body = vec![if new_grants.is_empty() && here.reaches_agents {
        format!(
            "It runs a script inside the workspace, in {project} only, since it reaches its \
             agents, and no further than this:"
        )
    } else if new_grants.is_empty() {
        "It runs a script inside the workspace, in every project, and reaches no further than \
         this:"
            .to_owned()
    } else {
        "Its update wants more than you accepted. Nothing of it runs until you allow it:".to_owned()
    }];
    let shown = if new_grants.is_empty() {
        grants
    } else {
        new_grants
    };
    if shown.is_empty() {
        body.push("nothing beyond drawing what its script answers".to_owned());
    }
    body.extend(
        shown
            .iter()
            .map(|grant| format!("{grant} · {}", uze_application::grant_meaning(grant))),
    );
    body.extend(band_rows.map(band_rows_said));
    body
}

/// What a band costs every agent of the project, as a consent step says
/// it beside the grants.
pub(crate) fn band_rows_said(rows: u8) -> String {
    match rows {
        1 => "draws 1 row over each agent's pane".to_owned(),
        rows => format!("draws {rows} rows over each agent's pane"),
    }
}

/// Each program an extension runs, where it was found, and what running
/// it means on this machine: the second step of allowing an extension.
fn programs_body(step: &crate::ui::model::ProgramsStep) -> Vec<String> {
    programs_lines(&step.programs, step.confined)
}

/// [`programs_body`] for the programs `programs`, confined or not: what
/// every consent step for programs says, on the Extensions screen and in
/// the workspace's offer alike.
pub(crate) fn programs_lines(
    programs: &[uze_application::ProgramPin],
    confined: bool,
) -> Vec<String> {
    let step = crate::ui::model::ProgramsStep {
        programs: programs.to_vec(),
        confined,
    };
    let mut body = vec![if step.confined {
        "Each runs confined: it reads this checkout and its own files, writes only a scratch \
         folder UZE removes, and reaches no network."
            .to_owned()
    } else {
        "This machine cannot confine programs.".to_owned()
    }];
    for program in &step.programs {
        let name = &program.name;
        body.push(match &program.path {
            Some(path) => format!("{name} · {}", path.display()),
            None if program.packaged => format!("{name} · packaged with it"),
            None => format!(
                "{name} · {}",
                program.unresolved.as_deref().unwrap_or("not found")
            ),
        });
        if !step.confined {
            body.push(format!(
                "`{name}` runs as you, with everything you can reach: your files, your keys, the \
                 network."
            ));
        } else if program.runs_anything {
            body.push(format!(
                "`{name}` can run anything, the same as running any code as you, inside what it \
                 is confined to."
            ));
        }
        if !program.reads.is_empty() {
            body.push(format!("it also reads {}", program.reads.join(", ")));
        }
    }
    body
}

impl Confirmation {
    /// Whether this only explains, with one way out and nothing to agree to.
    fn is_notice(&self) -> bool {
        matches!(self, Self::ProtectedPlugin(_))
    }

    /// What agreeing asks for. Allowing an extension's grants asks about
    /// its programs next, on a step of their own, before anything is
    /// recorded: one answer for the two would be agreeing to what was not
    /// shown.
    fn intent(self, model: &mut TuiModel) -> Intent {
        match self {
            Self::EnableExtension {
                here,
                attribution,
                programs: Some(step),
                ..
            } => {
                model.overlay = Overlay::Confirm {
                    kind: Self::AllowPrograms {
                        here,
                        attribution,
                        step,
                    },
                    focus: Some(1),
                };
                Intent::None
            }
            Self::EnableExtension { here, .. } | Self::AllowPrograms { here, .. } => {
                Intent::ExtensionHere(here)
            }
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
            Self::AllowPrograms {
                attribution, step, ..
            } => Dialog {
                body: programs_body(step),
                ..dialog(
                    if step.confined {
                        Tone::Caution
                    } else {
                        Tone::Danger
                    },
                    if step.confined {
                        "Allow these programs to run, confined?"
                    } else {
                        "Allow these programs to run as you?"
                    },
                    named(attribution),
                    "",
                    Some("Allow and enable"),
                )
            },
            Self::EnableExtension {
                here,
                attribution,
                grants,
                new_grants,
                band_rows,
                ..
            } => Dialog {
                body: extension_grants_body(here, grants, new_grants, *band_rows),
                ..dialog(
                    Tone::Caution,
                    if new_grants.is_empty() {
                        "Allow this extension to run?"
                    } else {
                        "Allow what its update wants?"
                    },
                    named(attribution),
                    "",
                    Some("Allow and enable"),
                )
            },
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
    let targets = dialog::render(
        frame,
        area,
        &kind.dialog(focus),
        &[uze_keys::Scope::Global, uze_keys::Scope::Confirm],
        Hit::Answer(false),
        Hit::Answer(true),
    );
    // Prepended, because the dialog is drawn over whatever was behind it
    // and that is still in the hit list underneath.
    hits.splice(0..0, targets.buttons);
}
