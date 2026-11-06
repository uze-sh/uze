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
/// `plugin@marketplace`, and says it is set. A secret its record cannot
/// say is set is taken back out: forgetting the plugin finds a secret only
/// through that record, and would leave it behind.
pub fn store(home: &UzeHome, plugin: &str, id: &str, name: &str, value: &str) -> Result<()> {
    with_key(home, plugin, id, name, |key| secrets::store(key, value))?;
    mark(home, plugin, id, name, true).inspect_err(|_| {
        let _ = with_key(home, plugin, id, name, secrets::delete);
    })
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
/// keychain, and the directory of their files and records. A secret the
/// keychain keeps stays recorded as set, with that directory, so the next
/// forgetting tries it again, and the error names it: without its record
/// nothing would ever find it.
pub fn forget_plugin(home: &UzeHome, plugin: &str) -> Result<()> {
    forget_plugin_with(home, plugin, secrets::delete)
}

fn forget_plugin_with(
    home: &UzeHome,
    plugin: &str,
    delete: impl Fn(&SecretKey<'_>) -> std::io::Result<()>,
) -> Result<()> {
    let mut kept = Vec::new();
    for id in recorded_ids(home, plugin) {
        for name in set_names(home, plugin, &id) {
            match with_key(home, plugin, &id, &name, &delete) {
                Ok(()) => mark(home, plugin, &id, &name, false)?,
                Err(UzeError::Write { source, .. }) => {
                    kept.push(format!("`{id}/{name}` ({source})"))
                }
                Err(error) => kept.push(format!("`{id}/{name}` ({error})")),
            }
        }
    }
    let dir = home.extension_plugin_secrets_dir(plugin);
    if !kept.is_empty() {
        return Err(UzeError::Write {
            path: dir,
            source: std::io::Error::other(format!(
                "{} could not be removed from {}; their record is kept",
                kept.join(", "),
                kept_in()
            )),
        });
    }
    match std::fs::remove_dir_all(&dir) {
        Err(source) if source.kind() != std::io::ErrorKind::NotFound => {
            Err(UzeError::Write { path: dir, source })
        }
        _ => Ok(()),
    }
}

/// The rungs of this module's record's ladder, for the crate's
/// [`earlier`](crate::earlier) registry.
pub(crate) fn rungs() -> Vec<uze_document::earlier::Earlier> {
    uze_document::earlier::rungs::<SetRecord>(file!())
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

    /// A secret the keychain would not let go of stays recorded as set, so
    /// forgetting the plugin again tries it again, and the removal says so;
    /// the ones it did let go of are no longer recorded.
    #[test]
    fn a_secret_the_keychain_kept_stays_recorded_and_is_reported() {
        let root = uze_testkit::temp::scratch("workspace-secrets-kept");
        let home = UzeHome::at(root.join("uze"));
        mark(&home, "ci@studio", "ci-status", "token", true).unwrap();
        mark(&home, "ci@studio", "ci-status", "other", true).unwrap();
        mark(&home, "ci@studio", "deploy", "key", true).unwrap();

        let error = forget_plugin_with(&home, "ci@studio", |key| {
            if key.account == "ci@studio/ci-status/token" {
                Err(std::io::Error::other("the keychain is locked"))
            } else {
                Ok(())
            }
        })
        .unwrap_err()
        .to_string();

        assert!(error.contains("ci-status/token"), "{error}");
        assert!(error.contains("the keychain is locked"), "{error}");
        assert_eq!(
            set_names(&home, "ci@studio", "ci-status"),
            BTreeSet::from(["token".to_owned()])
        );
        assert!(set_names(&home, "ci@studio", "deploy").is_empty());

        forget_plugin_with(&home, "ci@studio", |_| Ok(())).unwrap();
        assert!(!home.extension_plugin_secrets_dir("ci@studio").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    // Only where secrets are kept in a file: on macOS and Windows this
    // would write to the operator's own Keychain or Credential Manager.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_secret_that_cannot_be_recorded_as_set_is_not_kept() {
        let root = uze_testkit::temp::scratch("workspace-secrets-unrecorded");
        let home = UzeHome::at(root.join("uze"));
        std::fs::create_dir_all(home.extension_secrets_set_path("ci@studio", "ci-status")).unwrap();

        assert!(store(&home, "ci@studio", "ci-status", "token", "ghp_value").is_err());

        assert_eq!(
            read(&home, "ci@studio", "ci-status", "token").unwrap(),
            None,
            "a secret nothing records as set is never forgotten"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
