//! The secret preferences of declared extensions (`extension-growth` 2.1),
//! kept by `uze-platform` in the operating system's keychain, or in a
//! `0600` file under `state/extensions/secrets/` where there is none. A
//! value is written and read here and nowhere else in the workspace:
//! whether one is set is kept beside the other preferences as `true`,
//! never the value, so the Extensions screen says "set" without reading
//! the keychain.

use uze_platform::secrets::{self, SecretKey};

use crate::{Result, UzeError, home::UzeHome};

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
    super::preferences::set(home, plugin, id, name, serde_json::Value::Bool(true))?;
    Ok(())
}

/// Forgets the secret `name`, and says it is not set.
pub fn delete(home: &UzeHome, plugin: &str, id: &str, name: &str) -> Result<()> {
    with_key(home, plugin, id, name, secrets::delete)?;
    super::preferences::set(home, plugin, id, name, serde_json::Value::Bool(false))?;
    Ok(())
}

/// The secret `name`'s value, `None` when it is not set.
pub fn read(home: &UzeHome, plugin: &str, id: &str, name: &str) -> Result<Option<String>> {
    with_key(home, plugin, id, name, secrets::read)
}

/// Forgets every secret the plugin `plugin@marketplace`'s extensions keep,
/// as it leaves the machine: each one its preferences say is set, from
/// the keychain, and the directory of their files.
pub fn forget_plugin(home: &UzeHome, plugin: &str) -> Result<()> {
    for (id, values) in super::preferences::of_plugin(home, plugin) {
        for (name, _) in values
            .iter()
            .filter(|(_, value)| **value == serde_json::Value::Bool(true))
        {
            let _ = with_key(home, plugin, &id, name, secrets::delete);
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

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_secret_is_kept_apart_from_the_preferences_and_leaves_with_its_plugin() {
        let root = uze_testkit::temp::scratch("workspace-secrets");
        let home = UzeHome::at(root.join("uze"));
        store(&home, "ci@studio", "ci-status", "token", "ghp_value").unwrap();
        assert_eq!(
            read(&home, "ci@studio", "ci-status", "token")
                .unwrap()
                .as_deref(),
            Some("ghp_value")
        );
        let preferences = super::super::preferences::read(&home, "ci@studio", "ci-status");
        assert_eq!(
            preferences.get("token"),
            Some(&serde_json::Value::Bool(true))
        );
        let record =
            std::fs::read_to_string(home.extension_preferences_path("ci@studio", "ci-status"))
                .unwrap();
        assert!(!record.contains("ghp_value"), "{record}");
        forget_plugin(&home, "ci@studio").unwrap();
        assert!(!home.extension_plugin_secrets_dir("ci@studio").exists());
        assert_eq!(
            read(&home, "ci@studio", "ci-status", "token").unwrap(),
            None
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
