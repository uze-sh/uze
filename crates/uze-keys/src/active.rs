//! The keymap in force, right now.
//!
//! A keymap is process-wide and cannot change in the middle of a keystroke,
//! so it lives here rather than as a parameter threaded through every
//! dispatcher and every surface that prints a key — none of which could
//! legitimately disagree about it. The lock is what lets the Keys screen
//! rebind without restarting: the write happens between events, and every
//! read on the input path is uncontended.
//!
//! There is no initialisation step. Before anything is loaded — in a test,
//! in a CLI invocation that never reads a keymap file, in code that runs
//! before `$UZE_HOME` is resolved — [`active`] answers with the built-in
//! default, which needs no I/O.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock, RwLock};

use crate::{ExtensionTable, Keymap, load::default_keymap};

/// The operator's keymap, the extensions registered over it, and the two
/// resolved together: what [`active`] answers.
///
/// Kept apart so either can change alone. A rebind on the Keys screen
/// puts a new keymap in force without forgetting which extensions are
/// enabled, and an extension enabled or disabled leaves the operator's
/// keymap as it was.
struct InForce {
    keymap: Arc<Keymap>,
    extensions: BTreeMap<String, ExtensionTable>,
    resolved: Arc<Keymap>,
}

impl InForce {
    fn resolve(&mut self) {
        let tables = self.extensions.values().cloned().collect();
        self.resolved = Arc::new(self.keymap.with_extensions(tables));
    }
}

fn cell() -> &'static RwLock<InForce> {
    static ACTIVE: OnceLock<RwLock<InForce>> = OnceLock::new();
    ACTIVE.get_or_init(|| {
        let keymap = Arc::new(default_keymap().clone());
        RwLock::new(InForce {
            resolved: Arc::clone(&keymap),
            keymap,
            extensions: BTreeMap::new(),
        })
    })
}

fn change(update: impl FnOnce(&mut InForce)) {
    let mut in_force = match cell().write() {
        Ok(in_force) => in_force,
        Err(poisoned) => poisoned.into_inner(),
    };
    update(&mut in_force);
    in_force.resolve();
}

/// The keymap in force, with the extensions registered over it. Cheap
/// enough to call per keystroke: it clones an `Arc`.
pub fn active() -> Arc<Keymap> {
    match cell().read() {
        Ok(in_force) => Arc::clone(&in_force.resolved),
        // A panic while a keymap was being swapped must not take the
        // operator's keyboard with it — the default beats nothing.
        Err(poisoned) => Arc::clone(&poisoned.into_inner().resolved),
    }
}

/// Puts a keymap in force. Every keystroke after this resolves against it,
/// and against the extensions registered.
pub fn set_active(keymap: Keymap) {
    change(|in_force| in_force.keymap = Arc::new(keymap));
}

/// Registers what an enabled extension offers the keyboard, replacing what
/// the same extension registered before.
pub fn register_extension(table: ExtensionTable) {
    change(|in_force| {
        in_force.extensions.insert(table.owner.clone(), table);
    });
}

/// Drops what `owner` registered: its keys go with it.
pub fn unregister_extension(owner: &str) {
    change(|in_force| {
        in_force.extensions.remove(owner);
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{Action, Chord, Scope, keymap::Resolution};

    /// The active keymap is process-wide, so tests that read or replace it
    /// take turns — otherwise one test's swap is another's flake.
    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn a_keymap_is_active_before_anything_is_loaded() {
        let _turn = SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Not a test of what it binds — a test that asking costs no setup,
        // which is what lets a unit test or a CLI path resolve a key at all.
        assert_eq!(
            active().resolve(
                Chord::parse("ctrl+o").unwrap(),
                &[Scope::Workspace, Scope::Pane]
            ),
            Resolution::Act(Action::SwitchMode)
        );
    }

    #[test]
    fn what_is_put_in_force_is_what_answers() {
        let _turn = SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let moved = default_keymap()
            .rebind(
                Action::NewShellTab,
                Scope::Workspace,
                Chord::parse("f4").ok(),
            )
            .expect("no conflict");
        set_active(moved);
        assert_eq!(
            active().resolve(
                Chord::parse("f4").unwrap(),
                &[Scope::Workspace, Scope::Pane]
            ),
            Resolution::Act(Action::NewShellTab)
        );
        set_active(default_keymap().clone());
    }

    #[test]
    fn a_registered_extension_outlives_a_keymap_put_in_force() {
        let _turn = SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let owner = "active@m/x";
        let action = crate::extension::intern(&crate::extension::action_name(owner, "go"));
        register_extension(ExtensionTable {
            owner: owner.to_owned(),
            offered: vec![crate::Offered {
                action,
                surface: "main".to_owned(),
                suggested: Some(Chord::parse("f9")),
            }],
        });
        let open = [Scope::Global, Scope::Workspace, Scope::Declared];
        let front = Some(crate::Front {
            owner,
            surface: "main",
        });
        let f9 = Chord::parse("f9").unwrap();
        set_active(default_keymap().clone());
        assert_eq!(
            active().resolve_for(f9, &open, front),
            Resolution::Act(Action::Extension(action)),
            "a rebind elsewhere keeps the extension's keys"
        );
        unregister_extension(owner);
        assert_eq!(
            active().resolve_for(f9, &open, front),
            Resolution::Fallthrough
        );
    }
}
