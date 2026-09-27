//! Which built-in extensions the workspace offers, as it stands in this
//! process.
//!
//! Held process-wide for the reason the chime is: the management modal
//! switches, the workspace under it obeys, and neither owns the other's
//! model. The workspace compares its own copy against this one every
//! frame, so an extension switched off in the modal is gone — its keys,
//! its buttons, its sidebar section — by the time the modal closes.

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

use uze_application::UzeHome;
use uze_extensions::{architect, code, spec};
use uze_keys::Action;

use super::tui_application;

static DISABLED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

pub(crate) fn disabled() -> BTreeSet<String> {
    DISABLED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

pub(crate) fn set(id: &str, enabled: bool) {
    let mut disabled = DISABLED.lock().unwrap_or_else(PoisonError::into_inner);
    if enabled {
        disabled.remove(id);
    } else {
        disabled.insert(id.to_owned());
    }
}

/// What the operator chose, or everything on when it cannot be read: a
/// surface is never worth refusing to start over.
pub(crate) fn load(home: &UzeHome) {
    let chosen = tui_application(home.clone())
        .and_then(|app| app.extensions().disabled())
        .unwrap_or_default();
    *DISABLED.lock().unwrap_or_else(PoisonError::into_inner) = chosen;
}

/// The extension whose surface `action` opens, so that switching an
/// extension off takes its keys with it. `None` for every action that
/// belongs to the workspace itself; the actions *inside* a surface need no
/// entry, since a surface that cannot open can never be where they are
/// asked.
pub(crate) fn owner(action: Action) -> Option<&'static str> {
    match action {
        Action::ToggleChanges | Action::ToggleFiles => Some(code::CATALOG.id),
        Action::ToggleArchitect => Some(architect::CATALOG.id),
        Action::ToggleSpec => Some(spec::CATALOG.id),
        _ => None,
    }
}

/// Whether `action` is offered with `disabled` switched off.
pub(crate) fn offered(action: Action, disabled: &BTreeSet<String>) -> bool {
    owner(action).is_none_or(|id| !disabled.contains(id))
}

/// What a switch did, for the status line.
pub(crate) fn outcome(name: &str, enabled: bool) -> String {
    if enabled {
        format!("{name} is back in the workspace")
    } else {
        format!("{name} is out of the workspace")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_extension_owns_an_action_that_opens_it() {
        for extension in uze_extensions::registry::ExtensionRegistry::builtin().all() {
            assert!(
                uze_keys::ALL_ACTIONS
                    .iter()
                    .any(|action| owner(*action) == Some(extension.id)),
                "nothing opens `{}`, so switching it off would take nothing away",
                extension.id
            );
        }
    }

    #[test]
    fn switching_an_extension_off_withdraws_only_its_own_actions() {
        let disabled = BTreeSet::from([code::CATALOG.id.to_owned()]);
        assert!(!offered(Action::ToggleChanges, &disabled));
        assert!(!offered(Action::ToggleFiles, &disabled));
        assert!(offered(Action::ToggleSpec, &disabled));
        assert!(offered(Action::NewAgent, &disabled));
    }
}
