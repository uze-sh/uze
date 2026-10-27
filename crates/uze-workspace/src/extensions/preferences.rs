//! The operator's settings for each declared extension
//! (`extension-preferences`): one object of values per extension, kept on
//! the machine as their own answer and read against what the extension
//! declares, so a value it no longer may hold reads as its default.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Result, UzeError, home::UzeHome, persistence::write_atomic};

const LOCK_PATIENCE: Duration = Duration::from_secs(2);

#[derive(Debug, Deserialize, Serialize)]
struct PreferencesRecord {
    schema_version: u32,
    values: Map<String, Value>,
}

/// A record: only the operator's answer says it.
impl uze_document::Shaped for PreferencesRecord {
    const SHAPE: u32 = uze_document::FIRST_SHAPE;
    const KIND: &'static str = "extension preferences";
}

/// What the operator set for extension `id` of `plugin@marketplace`, as
/// written: unread, absent or unreadable is nothing set.
pub fn read(home: &UzeHome, plugin: &str, id: &str) -> Map<String, Value> {
    uze_document::read::<PreferencesRecord>(&home.extension_preferences_path(plugin, id))
        .ok()
        .and_then(uze_document::Carried::record)
        .map(|record| record.values)
        .unwrap_or_default()
}

/// Sets `name` to `value` for extension `id` of `plugin@marketplace`, and
/// says what is set now. The caller has checked the value against the
/// declaration.
pub fn set(
    home: &UzeHome,
    plugin: &str,
    id: &str,
    name: &str,
    value: Value,
) -> Result<Map<String, Value>> {
    let path = home.extension_preferences_path(plugin, id);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|source| UzeError::Write {
            path: dir.to_path_buf(),
            source,
        })?;
    }
    let _held = crate::file_lock::Held::acquire(&path.with_extension("lock"), LOCK_PATIENCE)?;
    let mut values = read(home, plugin, id);
    values.insert(name.to_owned(), value);
    let record = PreferencesRecord {
        schema_version: <PreferencesRecord as uze_document::Shaped>::SHAPE,
        values,
    };
    write_atomic(
        &path,
        &serde_json::to_vec_pretty(&record).expect("preferences serialize"),
    )?;
    Ok(record.values)
}

/// Removes every preference record of the plugin `plugin@marketplace`'s
/// extensions, as it leaves the machine.
pub fn forget_plugin(home: &UzeHome, plugin: &str) -> Result<()> {
    let dir = home.extension_plugin_preferences_dir(plugin);
    match std::fs::remove_dir_all(&dir) {
        Err(source) if source.kind() != std::io::ErrorKind::NotFound => {
            Err(UzeError::Write { path: dir, source })
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preference_is_kept_per_extension_and_leaves_with_its_plugin() {
        let root = uze_testkit::temp::scratch("extension-preferences");
        let home = UzeHome::at(root.join("uze"));
        assert!(read(&home, "gpu@proving", "gpu-meter").is_empty());
        set(
            &home,
            "gpu@proving",
            "gpu-meter",
            "warn-at",
            serde_json::json!(50),
        )
        .unwrap();
        set(
            &home,
            "gpu@proving",
            "gpu-meter",
            "unit",
            serde_json::json!("GiB"),
        )
        .unwrap();
        let values = read(&home, "gpu@proving", "gpu-meter");
        assert_eq!(values["warn-at"], 50);
        assert_eq!(values["unit"], "GiB");
        assert!(read(&home, "gpu@proving", "other").is_empty());
        forget_plugin(&home, "gpu@proving").unwrap();
        assert!(read(&home, "gpu@proving", "gpu-meter").is_empty());
    }
}
