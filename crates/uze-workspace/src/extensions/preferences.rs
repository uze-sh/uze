//! The operator's settings for each extension, built-in or declared
//! (`extension-preferences`): `[preferences."<id>"]` in `config.toml`,
//! keyed by the id the extension's switch in `[extensions]` uses, so one
//! extension has one key in the file.
//!
//! Authored, not a record: a preference is a choice only the operator
//! makes, read and edited by hand as readily as on the screen, and kept
//! when its plugin leaves, as an editor keeps an uninstalled extension's
//! settings. This module keeps the values; what each may hold is read
//! against the extension's declaration by whoever reads them, so a value
//! it no longer admits reads as its default. A secret is never here.

use serde_json::{Map, Value};

use crate::{Result, config, home::UzeHome};

const SECTION: &str = "preferences";

/// What the operator set for the extension `id`, as written.
pub fn read(home: &UzeHome, id: &str) -> Result<Map<String, Value>> {
    config::scalars_in_table(home, SECTION, id)
}

/// Sets `name` to `value` for the extension `id`. The caller has checked
/// the value against the declaration.
pub fn set(home: &UzeHome, id: &str, name: &str, value: &Value) -> Result<()> {
    config::set_scalar_in_table(home, SECTION, id, name, value)
}

/// The rungs of this module's record's ladder, for the crate's
/// [`earlier`](crate::earlier) registry.
pub(crate) fn rungs() -> Vec<uze_document::earlier::Earlier> {
    uze_document::earlier::rungs::<PreferencesRecord>(file!())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preference_is_kept_per_extension_in_the_config_file() {
        let home = UzeHome::at(uze_testkit::temp::scratch("extension-preferences"));
        assert!(read(&home, "gpu@proving/gpu-meter").unwrap().is_empty());
        set(&home, "gpu@proving/gpu-meter", "warn-at", &50.into()).unwrap();
        set(&home, "gpu@proving/gpu-meter", "unit", &"GiB".into()).unwrap();
        set(&home, "review", "mark-viewed-automatically", &false.into()).unwrap();

        let values = read(&home, "gpu@proving/gpu-meter").unwrap();
        assert_eq!(values["warn-at"], 50);
        assert_eq!(values["unit"], "GiB");
        assert!(read(&home, "gpu@proving/other").unwrap().is_empty());
        let text = std::fs::read_to_string(home.config_path()).unwrap();
        assert!(
            text.contains("[preferences.\"gpu@proving/gpu-meter\"]"),
            "{text}"
        );
        assert!(
            text.contains("[preferences.review]\nmark-viewed-automatically = false"),
            "{text}"
        );
    }
}
