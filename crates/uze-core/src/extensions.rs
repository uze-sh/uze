//! The `[extensions]` section: which of the workspace's built-in
//! extensions the operator switched off.
//!
//! Keyed by the extension's id, and only ever holding a `false` worth
//! keeping: an extension ships switched on, so a machine that never chose
//! has nothing written, and one that switched something back on has
//! `true` beside it — the operator's text, left as they would expect to
//! find it. What an extension *is* belongs to the client that draws it;
//! the domain holds the choice and nothing about what it switches.
//!
//! Machine-scoped, like the theme: which surfaces a person wants in their
//! workspace is theirs, not the project's.

use std::collections::BTreeSet;

use crate::{Result, config, home::UzeHome};

const SECTION: &str = "extensions";

/// The ids switched off. Anything but a boolean reads as switched on: an
/// unrecognised value is not a reason to take a surface away.
pub fn disabled(home: &UzeHome) -> Result<BTreeSet<String>> {
    Ok(config::bools(home, SECTION)?
        .into_iter()
        .filter_map(|(id, enabled)| (!enabled).then_some(id))
        .collect())
}

pub fn set_enabled(home: &UzeHome, id: &str, enabled: bool) -> Result<()> {
    config::set_bool(home, SECTION, id, enabled)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn home(label: &str) -> UzeHome {
        UzeHome::at(uze_testkit::temp::scratch(label))
    }

    #[test]
    fn a_machine_that_never_chose_has_everything_on() {
        let home = home("extensions-none");
        assert!(disabled(&home).expect("readable").is_empty());
    }

    #[test]
    fn switching_off_and_back_on_round_trips() {
        let home = home("extensions-round-trip");
        set_enabled(&home, "architect", false).expect("written");
        assert_eq!(
            disabled(&home).expect("readable"),
            BTreeSet::from(["architect".to_owned()])
        );
        set_enabled(&home, "architect", true).expect("written");
        assert!(disabled(&home).expect("readable").is_empty());
    }

    #[test]
    fn a_value_that_is_not_a_boolean_leaves_the_extension_on() {
        let home = home("extensions-unknown");
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        fs::write(
            home.config_path(),
            "[extensions]\ncode = \"off\"\nspec = false\n",
        )
        .unwrap();
        assert_eq!(
            disabled(&home).expect("readable"),
            BTreeSet::from(["spec".to_owned()])
        );
    }
}
