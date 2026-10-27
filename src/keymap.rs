//! Putting the operator's keyboard in force.
//!
//! The same shape as `theme::install`, for the same reasons. A keymap that
//! will not load is not silently ignored — an operator whose file has a
//! typo would otherwise press a key, get nothing, and have no idea why —
//! but the entries that merely name something this build has never heard
//! of are the loader's warnings, and those belong on the Keys screen
//! rather than above the output of every command for the rest of the
//! file's life.

use uze_application::{CompiledExtension, ExtensionIdentity, UzeApplication, UzeHome};
use uze_contract::{Clean, KeySuggestion};
use uze_keys::extension::{self, Described};
use uze_keys::{Chord, ChordProblem, ExtensionTable, Offered};

/// Reads `keys.json` and puts what it says in force, returning only what
/// stopped it from being applied.
///
/// A missing file is not a problem: the built-in keymap is a complete
/// keyboard on its own, and most people will never write one.
pub fn install(home: &UzeHome) -> Vec<String> {
    let path = home.keymap_path();
    let file = match uze_keys::load::read(&path) {
        Ok(Some(file)) => file,
        Ok(None) => return Vec::new(),
        Err(error) => return vec![format!("{}: {error}", path.display())],
    };
    match uze_keys::load::resolve(&file) {
        Ok(loaded) => {
            uze_keys::set_active(loaded.keymap);
            Vec::new()
        }
        // The keymap already in force stays in force — which is the
        // built-in one, so the operator still has a keyboard to fix the
        // file with.
        Err(problems) => problems
            .into_iter()
            .map(|problem| format!("{}: {}", path.display(), problem.message))
            .collect(),
    }
}

/// The owner an extension's actions are named under in a keymap:
/// `plugin@marketplace/id`.
pub fn extension_owner(identity: &ExtensionIdentity) -> String {
    format!(
        "{}@{}/{}",
        identity.plugin, identity.marketplace, identity.id
    )
}

/// A manifest's `key` in the keymap's grammar: `ctrl-shift-r` is
/// `ctrl+shift+r`. A chord a terminal cannot tell apart from another
/// (`ctrl-m` is Enter) is refused here as it is in `keys.json`.
pub fn suggested_chord(suggestion: &KeySuggestion) -> Result<Chord, ChordProblem> {
    let mut text = String::new();
    for (on, name) in [
        (suggestion.ctrl, "ctrl"),
        (suggestion.alt, "alt"),
        (suggestion.shift, "shift"),
    ] {
        if on {
            text.push_str(name);
            text.push('+');
        }
    }
    text.push_str(&suggestion.key);
    Chord::parse(&text)
}

/// What a compiled extension offers the keyboard: each action it declares,
/// interned under its name and given the manifest's words, cleaned, with
/// the chord it suggests.
pub fn extension_table(compiled: &CompiledExtension) -> ExtensionTable {
    let owner = extension_owner(&compiled.identity);
    let offered = compiled
        .manifest
        .actions
        .iter()
        .map(|action| {
            let id = extension::intern(&extension::action_name(&owner, &action.id));
            let label = Clean::label(&action.label).into_string();
            extension::describe(
                id,
                Described {
                    description: Clean::label(&format!("{label}, from {}", compiled.identity))
                        .into_string(),
                    label,
                    destructive: action.confirm.is_some(),
                },
            );
            Offered {
                action: id,
                surface: action.on.clone(),
                suggested: action.key.as_ref().map(suggested_chord),
            }
        })
        .chain(overlay_doors(compiled, &owner))
        .collect();
    ExtensionTable { owner, offered }
}

/// The door of each overlay an `open` action with a key names, offered on
/// every surface and over the pane: the key opens the overlay from
/// anywhere (`extension-agent-bridge`'s `jump`). Its words are the ones the index
/// gives the door.
fn overlay_doors<'a>(
    compiled: &'a CompiledExtension,
    owner: &'a str,
) -> impl Iterator<Item = Offered> + 'a {
    compiled.manifest.actions.iter().filter_map(move |action| {
        let uze_contract::Does::Open(target) = &action.does else {
            return None;
        };
        let key = action.key.as_ref()?;
        let surface = compiled.manifest.surface(target)?;
        if surface.slot != uze_contract::Slot::Overlay {
            return None;
        }
        let door = extension::intern(&extension::action_name(
            owner,
            &format!("{}{target}", extension::DOOR),
        ));
        Some(Offered {
            action: door,
            surface: extension::EVERYWHERE.to_owned(),
            suggested: Some(suggested_chord(key)),
        })
    })
}

/// What the keymap in force says about the installed extensions' keys:
/// each suggestion the host already holds, each chord two actions of one
/// surface reach, each name `keys.json` writes for an extension that is
/// not installed. One that does not compile offers no key, and says so
/// where it is checked, not here.
pub fn extension_collisions(app: &UzeApplication) -> Vec<String> {
    let extensions = app.extensions();
    let tables = extensions
        .installed()
        .iter()
        .filter_map(|installed| extensions.compile(installed).ok())
        .map(|compiled| extension_table(&compiled))
        .collect();
    uze_keys::active()
        .with_extensions(tables)
        .extension_problems()
        .iter()
        .map(|problem| problem.message.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uze_keys::{Action, Scope};

    /// The active keymap is process-wide, so tests that replace it take
    /// turns — otherwise one test's swap is another's flake.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn home() -> (uze_testkit::temp::TempDir, UzeHome) {
        let scratch = uze_testkit::temp::TempDir::new("keymap");
        let home = UzeHome::at(scratch.path().to_path_buf());
        (scratch, home)
    }

    #[test]
    fn no_file_is_no_problem() {
        let _turn = SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (_scratch, home) = home();
        assert!(install(&home).is_empty());
    }

    #[test]
    fn what_the_operator_wrote_is_in_force_before_anything_is_drawn() {
        let _turn = SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (_scratch, home) = home();
        std::fs::write(
            home.keymap_path(),
            r#"{"bindings":{"workspace":{"new-shell-tab":["f4"]}}}"#,
        )
        .expect("write");

        assert!(install(&home).is_empty());
        assert_eq!(
            uze_keys::active().chord_for(Action::NewShellTab, &[Scope::Workspace]),
            Chord::parse("f4").ok()
        );
        uze_keys::set_active(uze_keys::default_keymap().clone());
    }

    #[test]
    fn a_file_that_would_take_the_keyboard_away_says_so_and_changes_nothing() {
        let _turn = SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (_scratch, home) = home();
        // `ctrl+m` is Enter on a terminal; binding it would take Enter.
        std::fs::write(
            home.keymap_path(),
            r#"{"bindings":{"workspace":{"close-tab":["ctrl+m"]}}}"#,
        )
        .expect("write");

        let problems = install(&home);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("Enter"), "{problems:?}");
        assert_eq!(
            uze_keys::active().chord_for(Action::CloseTab, &[Scope::Workspace]),
            Chord::parse("ctrl+w").ok(),
            "the keyboard in force is untouched, so the file can be fixed"
        );
    }

    #[test]
    fn a_manifests_key_reads_as_the_keymaps_chord() {
        let read = |text: &str| suggested_chord(&KeySuggestion::parse(text).expect(text));
        assert_eq!(read("ctrl-shift-r"), Chord::parse("ctrl+shift+r"));
        assert_eq!(read("f5"), Chord::parse("f5"));
        assert_eq!(read("ctrl--"), Chord::parse("ctrl+-"));
        assert_eq!(read("d"), Chord::parse("d"));
        assert!(read("ctrl-m").is_err(), "Enter on a terminal is no chord");
    }
}
