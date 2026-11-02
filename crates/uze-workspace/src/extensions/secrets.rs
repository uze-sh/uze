//! The secret preferences of declared extensions (`extension-growth` 2.1),
//! kept by `uze-platform` in the operating system's keychain, or in a
//! `0600` file under `state/extensions/secrets/` where there is none. A
//! value is written and read here and nowhere else in the workspace:
//! whether one is set is a record of its own beside the secrets' files,
//! never the value, so the Extensions screen says "set" without reading
//! the keychain. Never in `config.toml`, which holds the other
//! preferences: nothing about a secret is in the operator's text.

use std::{collections::BTreeSet, time::Duration};

use serde::{Deserialize, Serialize};
use uze_platform::secrets::{self, SecretKey};

use crate::{Result, UzeError, home::UzeHome, persistence::write_atomic};

const LOCK_PATIENCE: Duration = Duration::from_secs(2);

#[derive(Debug, Default, Deserialize, Serialize)]
struct SetRecord {
    schema_version: u32,
    set: BTreeSet<String>,
}

/// A record: only the operator's typing says it.
impl uze_document::Shaped for SetRecord {
    const SHAPE: u32 = uze_document::FIRST_SHAPE;
    const KIND: &'static str = "extension secrets set";
}

/// The service a keychain files every UZE secret under.
const SERVICE: &str = "uze";

/// What the screen says secrets are kept in on this machine.
pub fn kept_in() -> &'static str {
    secrets::kept_in()
}

fn with_key<T>(
    home: &UzeHome,
    plugin: &str,
    id: &str,
    name: &str,
    then: impl FnOnce(&SecretKey<'_>) -> std::io::Result<T>,
) -> Result<T> {
    let file = home.extension_secret_path(plugin, id, name);
    let account = format!("{plugin}/{id}/{name}");
    then(&SecretKey {
        service: SERVICE,
        account: &account,
        file: &file,
    })
    .map_err(|source| UzeError::Write { path: file, source })
}

/// Keeps `value` as the secret `name` of extension `id` of
/// `plugin@marketplace`, and says it is set.
pub fn store(home: &UzeHome, plugin: &str, id: &str, name: &str, value: &str) -> Result<()> {
    with_key(home, plugin, id, name, |key| secrets::store(key, value))?;
    mark(home, plugin, id, name, true)
}

/// Forgets the secret `name`, and says it is not set.
pub fn delete(home: &UzeHome, plugin: &str, id: &str, name: &str) -> Result<()> {
    with_key(home, plugin, id, name, secrets::delete)?;
    mark(home, plugin, id, name, false)
}

/// The names of extension `id`'s secrets that are set: unread, absent or
/// unreadable is none.
pub fn set_names(home: &UzeHome, plugin: &str, id: &str) -> BTreeSet<String> {
    uze_document::read::<SetRecord>(&home.extension_secrets_set_path(plugin, id))
        .ok()
        .and_then(uze_document::Carried::record)
        .map(|record| record.set)
        .unwrap_or_default()
}

fn mark(home: &UzeHome, plugin: &str, id: &str, name: &str, set: bool) -> Result<()> {
    let path = home.extension_secrets_set_path(plugin, id);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|source| UzeError::Write {
            path: dir.to_path_buf(),
            source,
        })?;
    }
    let _held = crate::file_lock::Held::acquire(
        &home.extension_secrets_set_lock_path(plugin, id),
        LOCK_PATIENCE,
    )?;
    let mut names = set_names(home, plugin, id);
    if set {
        names.insert(name.to_owned());
    } else {
        names.remove(name);
    }
    let record = SetRecord {
        schema_version: <SetRecord as uze_document::Shaped>::SHAPE,
        set: names,
    };
    write_atomic(
        &path,
        &serde_json::to_vec_pretty(&record).expect("secrets set serialize"),
    )
}

/// Every extension of the plugin `plugin@marketplace` with a record of
/// its secrets.
fn recorded_ids(home: &UzeHome, plugin: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(home.extension_plugin_secrets_dir(plugin)) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            Some(name.strip_suffix(".json")?.to_owned())
        })
        .collect()
}

/// The secret `name`'s value, `None` when it is not set.
pub fn read(home: &UzeHome, plugin: &str, id: &str, name: &str) -> Result<Option<String>> {
    with_key(home, plugin, id, name, secrets::read)
}

/// Forgets every secret the plugin `plugin@marketplace`'s extensions keep,
/// as it leaves the machine: each one its record says is set, from the
/// keychain, and the directory of their files and records.
pub fn forget_plugin(home: &UzeHome, plugin: &str) -> Result<()> {
    for id in recorded_ids(home, plugin) {
        for name in set_names(home, plugin, &id) {
            let _ = with_key(home, plugin, &id, &name, secrets::delete);
        }
    }
    let dir = home.extension_plugin_secrets_dir(plugin);
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

    // Only where secrets are kept in a file: on macOS and Windows this
    // would write to the operator's own Keychain or Credential Manager.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_secret_is_kept_out_of_the_config_file_and_leaves_with_its_plugin() {
        let root = uze_testkit::temp::scratch("workspace-secrets");
        let home = UzeHome::at(root.join("uze"));
        store(&home, "ci@studio", "ci-status", "token", "ghp_value").unwrap();
        assert_eq!(
            read(&home, "ci@studio", "ci-status", "token")
                .unwrap()
                .as_deref(),
            Some("ghp_value")
        );
        assert!(set_names(&home, "ci@studio", "ci-status").contains("token"));
        let record =
            std::fs::read_to_string(home.extension_secrets_set_path("ci@studio", "ci-status"))
                .unwrap();
        assert!(!record.contains("ghp_value"), "{record}");
        assert!(!home.config_path().exists(), "a secret reached config.toml");
        forget_plugin(&home, "ci@studio").unwrap();
        assert!(!home.extension_plugin_secrets_dir("ci@studio").exists());
        assert_eq!(
            read(&home, "ci@studio", "ci-status", "token").unwrap(),
            None
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
