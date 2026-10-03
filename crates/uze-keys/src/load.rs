//! The built-in keymap, and what an operator's file does to it.
//!
//! The split between an error and a warning is the same one the theme
//! loader documents, for the same reason. A file uze cannot make sense of —
//! a chord that is not a chord, two actions reaching for one mnemonic — is
//! an error, and the keymap already in force stays in force, because an
//! operator without a keyboard cannot fix the file that took it away. A file
//! that names something *this build* does not know is a warning: a keymap
//! written for a newer uze must still load on an older one, or every keymap
//! in the wild breaks the first time the vocabulary grows.

use std::{fs, io, path::Path, sync::OnceLock};

use crate::{
    action::Action,
    chord::Chord,
    file::KeymapFile,
    keymap::{Binding, Keymap},
    scope::Scope,
};

/// Something worth telling the operator about their keymap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Problem {
    pub severity: Severity,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    /// The file is unusable as written; the keymap in force is unchanged.
    Error,
    /// The file loaded; this entry did not, and the built-in default
    /// applies for it.
    Warning,
}

impl Problem {
    fn error(message: impl Into<String>) -> Problem {
        Problem {
            severity: Severity::Error,
            message: message.into(),
        }
    }

    fn warning(message: impl Into<String>) -> Problem {
        Problem {
            severity: Severity::Warning,
            message: message.into(),
        }
    }
}

/// A keymap that loaded, with whatever was worth saying about it.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub keymap: Keymap,
    pub problems: Vec<Problem>,
}

/// The keymap uze ships with. Everything an operator writes is a
/// difference from this.
pub fn default_keymap() -> &'static Keymap {
    static DEFAULT: OnceLock<Keymap> = OnceLock::new();
    DEFAULT.get_or_init(|| {
        Keymap::new(default_bindings()).expect("the built-in keymap has no conflicts")
    })
}

/// Reads a keymap file, or `None` when there is none — which is not a
/// problem: the built-in default is a complete keymap on its own.
pub fn read(path: &Path) -> io::Result<Option<KeymapFile>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    serde_json::from_str(&contents)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))
}

/// Applies an operator's file over the built-in default.
///
/// `Err` carries the reasons the file cannot be used at all; the caller
/// keeps whatever keymap it already had. `Ok` carries the resolved keymap
/// and the entries that were skipped.
pub fn resolve(file: &KeymapFile) -> Result<Loaded, Vec<Problem>> {
    let mut problems = Vec::new();
    let mut errors = Vec::new();
    let mut bindings = default_bindings();

    if let Some(version) = file.version
        && version > crate::file::CURRENT_VERSION
    {
        problems.push(Problem::warning(format!(
            "this keymap is written for schema version {version}; this build understands {}",
            crate::file::CURRENT_VERSION
        )));
    }

    for (scope_name, actions) in &file.bindings {
        let Some(scope) = Scope::parse(scope_name) else {
            problems.push(Problem::warning(format!(
                "`{scope_name}` is not a surface this build knows; its bindings are ignored"
            )));
            continue;
        };
        for (action_name, chords) in actions {
            let Some(action) = Action::parse(action_name) else {
                problems.push(Problem::warning(format!(
                    "`{action_name}` is not an action this build knows; it is ignored"
                )));
                continue;
            };
            let mut written = Vec::new();
            let mut readable = true;
            for text in chords {
                match Chord::parse(text) {
                    Ok(chord) => written.push(chord),
                    Err(problem) => {
                        readable = false;
                        errors.push(Problem::error(format!(
                            "{scope_name}.{action_name}: {problem}"
                        )));
                    }
                }
            }
            if !readable {
                continue;
            }
            // Declaring an action replaces every default chord it had in
            // that surface — including with nothing, which is how an
            // operator says "no key, the button is enough".
            bindings.retain(|binding| !(binding.action == action && binding.scope == scope));
            bindings.extend(written.into_iter().map(|chord| Binding {
                scope,
                chord,
                action,
            }));
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }
    match Keymap::new(bindings) {
        Ok(keymap) => Ok(Loaded { keymap, problems }),
        Err(conflicts) => Err(conflicts
            .into_iter()
            .map(|conflict| Problem::error(conflict.to_string()))
            .collect()),
    }
}

/// What an operator's file would have to say to produce `keymap` —
/// only the surfaces and actions that differ from the built-in default.
///
/// This is what makes a later release able to move a chord nobody had an
/// opinion about: an entry appears here because someone changed it, not
/// because the screen that changed one had to write the whole keyboard
/// out.
pub fn difference_from_default(keymap: &Keymap) -> KeymapFile {
    let mut file = KeymapFile::default();
    let default = default_keymap();
    let mut pairs: Vec<(Scope, Action)> = default
        .bindings()
        .iter()
        .chain(keymap.bindings())
        .map(|binding| (binding.scope, binding.action))
        .collect();
    pairs.sort();
    pairs.dedup();
    for (scope, action) in pairs {
        let chords = |source: &Keymap| -> Vec<String> {
            source
                .bindings()
                .iter()
                .filter(|binding| binding.scope == scope && binding.action == action)
                .map(|binding| binding.chord.to_string())
                .collect()
        };
        let theirs = chords(keymap);
        if theirs != chords(default) {
            file.set(scope.name(), &action.name(), theirs);
        }
    }
    file
}

fn bind(scope: Scope, chord: &str, action: Action) -> Binding {
    Binding {
        scope,
        chord: Chord::parse(chord).expect("a built-in chord parses"),
        action,
    }
}

/// The keymap uze ships with.
///
/// Four rules shape it, and every entry is an instance of one of them:
///
/// 1. **In management, uze owns the keyboard**, so an action may hold a
///    bare letter — and a letter names one action, everywhere. A bare
///    letter always acts on the row you are on, which is why refreshing
///    carries a function key instead.
/// 2. **In the workspace, uze is a guest.** Every bare key belongs to the
///    program in the pane, and every chord uze takes is one an agent's
///    input loses. Function keys and modified navigation keys are the
///    register, and the five `Ctrl` letters that were here before this
///    design stay because they are already in people's fingers.
/// 3. **An action with no chord is finished, not unfinished.** Analyzing
///    context, adding a space, installing what a project declares: all are
///    offered by a button, a row's actions and the index, and none needs a
///    key invented for it.
/// 4. **One key per meaning.** An action holds one chord on a surface, not
///    a second spelling of it (no `j` beside `down`, no `tab` beside
///    `right`); what Enter already does to a row is not repeated on a
///    letter; and no letter answers a question — the arrows choose between
///    its answers, enter takes one, esc leaves.
fn default_bindings() -> Vec<Binding> {
    let mut bindings = vec![
        // --- Everywhere -------------------------------------------------
        // F1, and only F1: in the workspace `?` belongs to whatever the
        // agent is typing into, and a function key is the one register a
        // terminal program almost never claims. Management had a `?` of
        // its own for a while, which meant the way to help was a different
        // key depending on which mode you were in — and since a surface
        // prints the innermost chord it finds, the one key that works in
        // both was the one never shown.
        bind(Scope::Global, "f1", Action::OpenActionIndex),
        bind(Scope::Global, "ctrl+o", Action::SwitchMode),
        bind(Scope::Global, "ctrl+q", Action::Quit),
        // --- Management, screen-wide ------------------------------------
        bind(Scope::Management, "down", Action::SelectNext),
        bind(Scope::Management, "up", Action::SelectPrevious),
        bind(Scope::Management, "tab", Action::FocusNext),
        bind(Scope::Management, "shift+tab", Action::FocusPrevious),
        bind(Scope::Management, "enter", Action::Activate),
        bind(Scope::Management, "esc", Action::Dismiss),
        bind(Scope::Management, "left", Action::FocusSidebar),
        bind(Scope::Management, "right", Action::FocusContent),
        // Not a letter: refreshing is not a thing you do to the row you
        // are on, and every bare letter here acts on a selection.
        bind(Scope::Management, "f5", Action::Refresh),
        bind(Scope::Management, "/", Action::StartFilter),
        // --- Management, per screen -------------------------------------
        // Enter performs a plugin's first offer — installing what is not
        // there, removing what is — so only the offer it never makes
        // holds a letter of its own.
        bind(Scope::Plugins, "u", Action::UpdatePlugin),
        bind(Scope::Plugins, "m", Action::AddMarketplace),
        bind(Scope::Plugins, "pagedown", Action::ScrollPageDown),
        bind(Scope::Plugins, "pageup", Action::ScrollPageUp),
        bind(Scope::Harnesses, "s", Action::SetupHarness),
        bind(Scope::Profiles, "n", Action::NewProfile),
        bind(Scope::Profiles, "d", Action::DeleteProfile),
        bind(Scope::Profiles, "space", Action::ToggleProfileHarness),
        bind(Scope::Profiles, "v", Action::PreviewProfile),
        bind(Scope::ProfileEditor, "left", Action::PreviousValue),
        bind(Scope::ProfileEditor, "right", Action::NextValue),
        // --- Management, the surfaces that seal -------------------------
        bind(Scope::Filter, "enter", Action::Activate),
        bind(Scope::Filter, "esc", Action::Dismiss),
        bind(Scope::Filter, "backspace", Action::EraseBack),
        bind(Scope::TextPrompt, "enter", Action::Activate),
        bind(Scope::TextPrompt, "esc", Action::Dismiss),
        bind(Scope::TextPrompt, "backspace", Action::EraseBack),
        // A question is answered the way a row of buttons is: the arrows
        // move between the answers, enter takes the one the keyboard is
        // on, and esc is always the way out. No letter answers one, so
        // there is no second, invisible way to say yes.
        bind(Scope::Confirm, "enter", Action::Activate),
        bind(Scope::Confirm, "esc", Action::Dismiss),
        bind(Scope::Confirm, "left", Action::FocusPrevious),
        bind(Scope::Confirm, "right", Action::FocusNext),
        bind(Scope::KeyCapture, "esc", Action::Dismiss),
        // --- The index, in either mode ----------------------------------
        bind(Scope::ActionIndex, "down", Action::SelectNext),
        bind(Scope::ActionIndex, "up", Action::SelectPrevious),
        bind(Scope::ActionIndex, "enter", Action::Activate),
        bind(Scope::ActionIndex, "esc", Action::Dismiss),
        bind(Scope::ActionIndex, "backspace", Action::EraseBack),
        // --- Release notes, in either mode -----------------------------
        // The notes are read, so every key that moves moves them.
        bind(Scope::ReleaseNotes, "down", Action::SelectNext),
        bind(Scope::ReleaseNotes, "up", Action::SelectPrevious),
        bind(Scope::ReleaseNotes, "pagedown", Action::ScrollPageDown),
        bind(Scope::ReleaseNotes, "pageup", Action::ScrollPageUp),
        bind(Scope::ReleaseNotes, "enter", Action::Activate),
        bind(Scope::ReleaseNotes, "esc", Action::Dismiss),
        // --- Workspace, the container -----------------------------------
        bind(Scope::Workspace, "ctrl+t", Action::NewShellTab),
        bind(Scope::Workspace, "ctrl+w", Action::CloseTab),
        // The doors stay doors: the one already showing closes, another
        // switches — so a surface is left for another by its own chord,
        // never by closing it first.
        bind(Scope::Surfaces, "alt+g", Action::ToggleChanges),
        bind(Scope::Surfaces, "alt+e", Action::ToggleFiles),
        bind(Scope::Surfaces, "alt+a", Action::ToggleArchitect),
        // Under the thumb that is already on Alt, beside the other doors.
        // Not `z`: GPU overlays claim it system-wide (NVIDIA's; AMD's takes
        // `r`, the first choice here), so the terminal never receives it.
        // Not `c`, `d`, `f` or `b`: a shell in the pane capitalises, deletes
        // and moves by word on those, and the workspace scope takes a chord
        // before the pane ever sees it.
        bind(Scope::Surfaces, "alt+x", Action::ToggleSpec),
        bind(Scope::Workspace, "alt+n", Action::NewAgent),
        // The container of agents, beside the agent's own chord: `s` for
        // space, on the same modifier.
        bind(Scope::Workspace, "alt+s", Action::NewSpace),
        bind(Scope::Workspace, "f2", Action::RenameSelection),
        // The sidebar is vertical and holds spaces; the strip is
        // horizontal and holds tabs. Ctrl walks the container, Alt walks
        // what is inside it.
        bind(Scope::Workspace, "ctrl+up", Action::PreviousSpace),
        bind(Scope::Workspace, "ctrl+down", Action::NextSpace),
        bind(Scope::Workspace, "alt+up", Action::PreviousAgent),
        bind(Scope::Workspace, "alt+down", Action::NextAgent),
        // --- Workspace, the work ----------------------------------------
        bind(Scope::Workspace, "alt+i", Action::DeliverTask),
        bind(Scope::Workspace, "alt+p", Action::ToggleWork),
        // --- Workspace, the surfaces that seal --------------------------
        bind(Scope::Code, "esc", Action::Dismiss),
        bind(Scope::Code, "tab", Action::FocusNext),
        bind(Scope::Code, "down", Action::SelectNext),
        bind(Scope::Code, "up", Action::SelectPrevious),
        bind(Scope::Code, "left", Action::Collapse),
        bind(Scope::Code, "right", Action::Expand),
        bind(Scope::Code, "enter", Action::Activate),
        bind(Scope::Code, "pagedown", Action::ScrollPageDown),
        bind(Scope::Code, "pageup", Action::ScrollPageUp),
        bind(Scope::Architect, "esc", Action::Dismiss),
        // The arrows move the board, all four ways: on a surface whose
        // whole point is a drawing larger than the screen, that is what
        // an arrow means, and the list of diagrams is a row of tabs.
        bind(Scope::Architect, "left", Action::PanLeft),
        bind(Scope::Architect, "right", Action::PanRight),
        bind(Scope::Architect, "up", Action::PanUp),
        bind(Scope::Architect, "down", Action::PanDown),
        bind(Scope::Architect, "tab", Action::NextDiagram),
        bind(Scope::Architect, "shift+tab", Action::PreviousDiagram),
        // Free letters, both: `v` previews a profile and `a` analyses a
        // context elsewhere, and a letter names one action everywhere.
        bind(Scope::Architect, "g", Action::NextRendering),
        // One key for the two lists: `o` opens the artifacts, which is the
        // list reached for most, and left and right step between it and
        // the areas once either is open. Areas keep no key of their own —
        // the free letters left name nothing, and a key nobody can guess
        // is a key nobody presses.
        bind(Scope::Architect, "o", Action::ChooseArtifact),
        // Enter goes *into* the selected box — the level below it, or the
        // code it names — and backspace comes back out. Shift turns the
        // arrows from moving the board to moving the selection on it.
        bind(Scope::Architect, "enter", Action::Activate),
        bind(Scope::Architect, "backspace", Action::LevelUp),
        bind(Scope::Architect, "shift+left", Action::SelectBoxLeft),
        bind(Scope::Architect, "shift+right", Action::SelectBoxRight),
        bind(Scope::Architect, "shift+up", Action::SelectBoxUp),
        bind(Scope::Architect, "shift+down", Action::SelectBoxDown),
        // A list with a document beside it: the code surface's keys, for
        // the moves the two share, so a hand that knows one knows the other.
        bind(Scope::Spec, "esc", Action::Dismiss),
        bind(Scope::Spec, "tab", Action::FocusNext),
        bind(Scope::Spec, "down", Action::SelectNext),
        bind(Scope::Spec, "up", Action::SelectPrevious),
        bind(Scope::Spec, "left", Action::Collapse),
        bind(Scope::Spec, "right", Action::Expand),
        bind(Scope::Spec, "enter", Action::Activate),
        bind(Scope::Spec, "pagedown", Action::ScrollPageDown),
        bind(Scope::Spec, "pageup", Action::ScrollPageUp),
        bind(Scope::Spec, "p", Action::TogglePreview),
        bind(Scope::Code, "e", Action::EditFile),
        bind(Scope::Code, "p", Action::TogglePreview),
        bind(Scope::Code, "m", Action::ToggleMap),
        bind(Scope::Code, "d", Action::DeleteFile),
        // Not a letter: it opens a list of actions rather than naming one,
        // and a letter names one action everywhere.
        bind(Scope::Code, ".", Action::OpenMenu),
        // Typing has a scope of its own so nothing behind it answers a
        // letter — the same reason the action index has one.
        bind(Scope::CodeEditing, "esc", Action::Dismiss),
        bind(Scope::CodeEditing, "ctrl+s", Action::SaveFile),
        bind(Scope::CodeEditing, "up", Action::SelectPrevious),
        bind(Scope::CodeEditing, "down", Action::SelectNext),
        bind(Scope::CodeEditing, "left", Action::CaretLeft),
        bind(Scope::CodeEditing, "right", Action::CaretRight),
        bind(Scope::CodeEditing, "home", Action::CaretLineStart),
        bind(Scope::CodeEditing, "end", Action::CaretLineEnd),
        bind(Scope::CodeEditing, "enter", Action::InsertNewline),
        bind(Scope::CodeEditing, "tab", Action::InsertIndent),
        bind(Scope::CodeEditing, "pageup", Action::ScrollPageUp),
        bind(Scope::CodeEditing, "pagedown", Action::ScrollPageDown),
        bind(Scope::CodeEditing, "backspace", Action::EraseBack),
        bind(Scope::CodeEditing, "delete", Action::EraseForward),
        // The work modal: one keyboard for every row, since each key
        // means one thing whichever row is selected, and a row it does
        // not apply to says why.
        bind(Scope::Work, "esc", Action::Dismiss),
        bind(Scope::Work, "alt+p", Action::ToggleWork),
        bind(Scope::Work, "right", Action::NextProject),
        bind(Scope::Work, "left", Action::PreviousProject),
        bind(Scope::Work, "down", Action::SelectNext),
        bind(Scope::Work, "up", Action::SelectPrevious),
        bind(Scope::Work, "enter", Action::Activate),
        // The row's everyday moves hold a letter; the rarer ones —
        // marking done, joining, adopting, cleaning up — are its buttons
        // and rows of the index, which is where a keyboard reaches them.
        bind(Scope::Work, "r", Action::ResumeTask),
        bind(Scope::Work, "i", Action::DeliverTask),
        bind(Scope::Work, "d", Action::DiscardTask),
        bind(Scope::AgentPicker, "down", Action::SelectNext),
        bind(Scope::AgentPicker, "up", Action::SelectPrevious),
        bind(Scope::AgentPicker, "enter", Action::Activate),
        bind(Scope::AgentPicker, "esc", Action::Dismiss),
        bind(Scope::AgentDrawer, "down", Action::SelectNext),
        bind(Scope::AgentDrawer, "up", Action::SelectPrevious),
        bind(Scope::AgentDrawer, "enter", Action::Activate),
        bind(Scope::AgentDrawer, "esc", Action::Dismiss),
        // Whose prompts: this agent's or the whole space's. Two choices
        // side by side, walked the way the root picker's chips are.
        bind(Scope::AgentDrawer, "right", Action::FocusNext),
        bind(Scope::AgentDrawer, "left", Action::FocusPrevious),
        bind(Scope::AgentDrawer, "x", Action::ClearPromptHistory),
        bind(Scope::ContextMenu, "down", Action::SelectNext),
        bind(Scope::ContextMenu, "up", Action::SelectPrevious),
        bind(Scope::ContextMenu, "enter", Action::Activate),
        bind(Scope::ContextMenu, "esc", Action::Dismiss),
        bind(Scope::RootPicker, "down", Action::SelectNext),
        bind(Scope::RootPicker, "up", Action::SelectPrevious),
        // Tab walks into the highlighted directory, so a root several
        // levels down is reached by narrowing rather than by typing.
        bind(Scope::RootPicker, "tab", Action::Expand),
        // The kind chips under the directory: two choices side by side, so
        // the keys that walk sideways walk them.
        bind(Scope::RootPicker, "left", Action::FocusPrevious),
        bind(Scope::RootPicker, "right", Action::FocusNext),
        bind(Scope::RootPicker, "enter", Action::Activate),
        bind(Scope::RootPicker, "esc", Action::Dismiss),
        bind(Scope::RootPicker, "backspace", Action::EraseBack),
        bind(Scope::Rename, "enter", Action::Activate),
        bind(Scope::Rename, "esc", Action::Dismiss),
        bind(Scope::Rename, "backspace", Action::EraseBack),
        bind(Scope::Rename, "delete", Action::EraseForward),
        bind(Scope::Rename, "left", Action::CaretLeft),
        bind(Scope::Rename, "right", Action::CaretRight),
        bind(Scope::Rename, "home", Action::CaretLineStart),
        bind(Scope::Rename, "end", Action::CaretLineEnd),
    ];
    for index in 1..=9u8 {
        bindings.push(bind(
            Scope::Workspace,
            &format!("alt+{index}"),
            Action::SelectTab(index),
        ));
        // Bare digits: management owns the keyboard, and the screens are
        // numbered on the tab strip that heads the modal.
        bindings.push(bind(
            Scope::Management,
            &index.to_string(),
            Action::SelectTab(index),
        ));
    }
    bindings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        action::ALL_ACTIONS,
        chord::Tier,
        keymap::Resolution,
        scope::{ALL_SCOPES, Scope},
    };

    /// The editor's own tests hand it characters directly, so only this
    /// can see the keymap drop them: a letter typed into an open file
    /// once resolved to nothing, and the file could be navigated but
    /// never written in.
    #[test]
    fn typing_into_a_file_reaches_it_as_text() {
        let scopes = [Scope::Global, Scope::Workspace, Scope::CodeEditing];
        for typed in ["a", "q", "E", "shift+e", "space", "/"] {
            let chord = crate::chord::Chord::parse(typed).expect(typed);
            assert_eq!(
                default_keymap().resolve(chord, &scopes),
                Resolution::Text,
                "{typed} is text while a file is being edited"
            );
        }
        let tab = crate::chord::Chord::parse("tab").expect("tab");
        assert_eq!(
            default_keymap().resolve(tab, &scopes),
            Resolution::Act(Action::InsertIndent)
        );
    }

    #[test]
    fn the_built_in_keymap_has_no_conflicts() {
        // `Keymap::new` refuses one, so this passing is the whole claim.
        default_keymap();
    }

    #[test]
    fn every_action_is_either_bound_or_deliberately_unbound() {
        // The list is the design: an action here is one the product offers
        // by pointer and does not spend a chord on. Adding an action
        // without a chord means adding it here, which is where someone
        // asks whether that was intended.
        let unbound: Vec<String> = ALL_ACTIONS
            .iter()
            .filter(|action| {
                !default_keymap()
                    .bindings()
                    .iter()
                    .any(|binding| binding.action == **action)
            })
            .map(|action| action.name())
            .collect();
        assert_eq!(
            unbound,
            vec![
                // Enter on the Keys screen already asks for a key, and a
                // reset is rare enough to live on its button and menu.
                "change-key",
                "reset-key",
                // Enter performs a plugin's first offer, which is one of
                // these two; the other is its button and its index row.
                "install-plugin",
                "remove-plugin",
                // Removing a whole marketplace is rare and asked about:
                // its heading's button and the index carry it.
                "remove-marketplace",
                // Enter switches an extension whichever way it is not.
                "enable-extension",
                "disable-extension",
                "install-project-environment",
                // Rare, and project-wide rather than about the harness the
                // keyboard is on: the index lists both on that screen.
                "analyze-context",
                "apply-context-plan",
                "open-glossary",
                "apply-profile",
                // Isolating an agent is a decision taken once, on one
                // agent, from the menu that already names that agent — a
                // chord would fire on whichever row happened to be under
                // the cursor.
                "isolate-agent",
                "isolate-agent-at-commit",
                // The list of areas is one step left of the list of
                // artifacts, which has the key: `o`, then the arrow. A
                // letter of its own would be one of the four nobody could
                // guess.
                "choose-area",
                // Delivering every task at once is rare and wide: the
                // index offers it wherever there is a space to deliver.
                "deliver-all-tasks",
                // The work modal's rarer moves: each is a button on the
                // row it applies to, and a row of the index while it is.
                "finish-task",
                // The work modal's own key opens it; this is the space
                // menu's way straight to that space's project.
                "show-space-work",
                "adopt-checkout",
                "join-checkout",
                "clean-up-checkouts",
            ],
            "an action gained or lost a chord; say so here on purpose"
        );
    }

    #[test]
    fn a_destructive_action_on_a_bare_key_is_a_decision_someone_made() {
        // A bare key is one keystroke away at all times, so putting a
        // destructive action on one is a choice that has to be defended —
        // each of these does ask before it acts. The list is here rather
        // than in a comment so that adding another fails the build and
        // makes someone say why.
        //
        // The code surface's `d` is the same shape as preserved work's: it
        // only raises the question, which is answered with the arrows and
        // enter, never with another letter. Neither is live outside its
        // surface — the scope is sealed while it is open — and the code
        // surface's cannot reach a directory.
        //
        // The agent drawer's `x` asks first too, and the second `x` is the
        // answer: its footer says so while the question stands.
        let bare: Vec<String> = default_keymap()
            .bindings()
            .iter()
            .filter(|binding| {
                binding.action.destructive() && !binding.chord.mods.ctrl && !binding.chord.mods.alt
            })
            .map(|binding| {
                format!(
                    "{}.{}={}",
                    binding.scope.name(),
                    binding.action,
                    binding.chord
                )
            })
            .collect();
        assert_eq!(
            bare,
            vec![
                "profiles.delete-profile=d",
                "code.delete-file=d",
                "preserved-work.discard-task=d",
                "agent-drawer.clear-prompt-history=x",
            ]
        );
    }

    #[test]
    fn leaving_uze_is_never_one_bare_keystroke_away() {
        // Management owns the whole keyboard, which is why its actions may
        // hold bare letters — but it is a modal over a session full of
        // running agents, and the one action that cannot be taken back is
        // the one that must not be a single letter. `q` there used to end
        // the session; it now closes the modal, like the theme picker's.
        let bare: Vec<String> = default_keymap()
            .bindings()
            .iter()
            .filter(|binding| binding.action == Action::Quit)
            .filter(|binding| !binding.chord.mods.ctrl && !binding.chord.mods.alt)
            .map(|binding| format!("{}={}", binding.scope.name(), binding.chord))
            .collect();
        assert!(bare.is_empty(), "quitting is one keystroke away: {bare:?}");
    }

    #[test]
    fn the_workspace_spends_only_what_it_means_to() {
        // Every workspace chord is one an agent's input loses, so the set
        // is small on purpose and every member carries a modifier or is a
        // function key — a bare letter there would be stolen typing.
        for binding in default_keymap().bindings() {
            if binding.scope != Scope::Workspace {
                continue;
            }
            let carries_a_modifier = binding.chord.mods.ctrl || binding.chord.mods.alt;
            let is_function_key = matches!(binding.chord.key, crate::chord::Key::F(_));
            assert!(
                carries_a_modifier || is_function_key,
                "`{}` is a bare key the pane would never receive",
                binding.chord
            );
        }
    }

    #[test]
    fn no_default_chord_needs_a_protocol_the_operator_may_not_have() {
        for binding in default_keymap().bindings() {
            assert_ne!(
                binding.chord.tier(),
                Tier::EnhancementOnly,
                "`{}` is unbindable on a plain terminal, so it cannot be a default",
                binding.chord
            );
        }
    }

    #[test]
    fn every_scope_that_seals_can_be_left() {
        for scope in ALL_SCOPES.iter().filter(|scope| scope.seals()) {
            let leaves = default_keymap()
                .bindings()
                .iter()
                .any(|binding| binding.scope == *scope && binding.action == Action::Dismiss);
            assert!(leaves, "{} seals with no way out", scope.name());
        }
    }

    #[test]
    fn an_operators_file_replaces_only_what_it_names() {
        let mut file = KeymapFile::default();
        file.set("workspace", "new-shell-tab", vec!["f4".to_owned()]);
        let loaded = resolve(&file).expect("resolves");
        assert!(loaded.problems.is_empty());
        assert_eq!(
            loaded.keymap.resolve(
                Chord::parse("f4").unwrap(),
                &[Scope::Workspace, Scope::Pane]
            ),
            Resolution::Act(Action::NewShellTab)
        );
        assert_eq!(
            loaded.keymap.resolve(
                Chord::parse("ctrl+t").unwrap(),
                &[Scope::Workspace, Scope::Pane]
            ),
            Resolution::Fallthrough,
            "declaring an action replaces its chords rather than adding to them"
        );
        assert_eq!(
            loaded.keymap.resolve(
                Chord::parse("ctrl+w").unwrap(),
                &[Scope::Workspace, Scope::Pane]
            ),
            Resolution::Act(Action::CloseTab),
            "everything unnamed keeps whatever the default says"
        );
    }

    #[test]
    fn an_empty_list_unbinds_and_the_action_survives_it() {
        let mut file = KeymapFile::default();
        file.set("workspace", "close-tab", Vec::new());
        let loaded = resolve(&file).expect("resolves");
        assert_eq!(
            loaded
                .keymap
                .chord_for(Action::CloseTab, &[Scope::Workspace]),
            None
        );
        assert!(
            ALL_ACTIONS.contains(&Action::CloseTab),
            "unbinding takes the key, never the action"
        );
    }

    #[test]
    fn a_keymap_written_for_a_newer_uze_still_loads() {
        let mut file = KeymapFile::default();
        file.set("workspace", "teleport", vec!["f7".to_owned()]);
        file.set("holodeck", "close-tab", vec!["f8".to_owned()]);
        file.set("workspace", "new-shell-tab", vec!["f4".to_owned()]);
        file.version = Some(crate::file::CURRENT_VERSION + 1);

        let loaded = resolve(&file).expect("the rest of the file still applies");
        assert_eq!(loaded.problems.len(), 3, "{:?}", loaded.problems);
        assert!(
            loaded
                .problems
                .iter()
                .all(|problem| problem.severity == Severity::Warning)
        );
        assert_eq!(
            loaded
                .keymap
                .chord_for(Action::NewShellTab, &[Scope::Workspace]),
            Chord::parse("f4").ok(),
            "the entries this build understands are still applied"
        );
    }

    #[test]
    fn a_file_that_cannot_be_used_leaves_the_keyboard_alone() {
        let mut refused = KeymapFile::default();
        refused.set("workspace", "close-tab", vec!["ctrl+m".to_owned()]);
        let problems = resolve(&refused).expect_err("ctrl+m is Enter");
        assert!(problems[0].message.contains("Enter"), "{problems:?}");
        assert_eq!(problems[0].severity, Severity::Error);

        let mut conflicting = KeymapFile::default();
        conflicting.set("plugins", "install-plugin", vec!["u".to_owned()]);
        let problems = resolve(&conflicting).expect_err("`u` already updates");
        assert!(problems[0].message.contains("`u`"), "{problems:?}");
    }

    /// The screen that rebinds writes only what someone changed, so a
    /// later release can still move a chord nobody had an opinion about.
    #[test]
    fn only_what_differs_is_written_down() {
        assert!(
            difference_from_default(default_keymap()).is_empty(),
            "an untouched keyboard writes nothing at all"
        );

        let moved = default_keymap()
            .rebind(
                Action::NewShellTab,
                Scope::Workspace,
                Chord::parse("f4").ok(),
            )
            .expect("no conflict");
        let file = difference_from_default(&moved);
        assert_eq!(
            file.bindings["workspace"]["new-shell-tab"],
            vec!["f4".to_owned()]
        );
        assert_eq!(file.bindings["workspace"].len(), 1, "and nothing else");

        // And it round-trips: what the screen writes is what the loader
        // reads back.
        assert_eq!(resolve(&file).expect("resolves").keymap, moved);
    }

    #[test]
    fn an_unbinding_survives_the_round_trip() {
        let unbound = default_keymap()
            .rebind(Action::CloseTab, Scope::Workspace, None)
            .expect("no conflict");
        let file = difference_from_default(&unbound);
        assert_eq!(
            file.bindings["workspace"]["close-tab"],
            Vec::<String>::new(),
            "an empty list is how `no key` is written"
        );
        assert_eq!(resolve(&file).expect("resolves").keymap, unbound);
    }

    #[test]
    fn a_missing_file_is_not_a_problem() {
        let missing = std::env::temp_dir().join("uze-keys-there-is-no-such-file.json");
        assert_eq!(read(&missing).expect("no error"), None);
    }
}
