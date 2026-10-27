//! Secrets the operator typed once, kept where the operating system keeps
//! them (`extension-growth` 2.1): the macOS Keychain through the system's
//! `security`, the Windows Credential Manager, and elsewhere a file only
//! its owner can read, at the path the caller names.
//!
//! Not [`crate::secret`], which makes unpredictable bytes: this keeps a
//! value somebody else made. A value is one line of text; it is written
//! and read here and nowhere logged.

use std::{io, path::Path};

#[cfg(all(unix, not(target_os = "macos")))]
mod file;
#[cfg(target_os = "macos")]
mod keychain;
#[cfg(windows)]
mod windows;

#[cfg(all(unix, not(target_os = "macos")))]
use file as imp;
#[cfg(target_os = "macos")]
use keychain as imp;
#[cfg(windows)]
use windows as imp;

/// Which secret: the service and account a keychain files it under, and
/// the file that holds it where there is no keychain.
#[derive(Clone, Copy, Debug)]
pub struct SecretKey<'a> {
    pub service: &'a str,
    pub account: &'a str,
    /// Where it is kept on a machine with no keychain; a keychain never
    /// writes it.
    pub file: &'a Path,
}

/// Where this machine keeps secrets, as a person reads it.
pub fn kept_in() -> &'static str {
    imp::KEPT_IN
}

/// Keeps `value` for `key`, replacing what was kept.
pub fn store(key: &SecretKey<'_>, value: &str) -> io::Result<()> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a secret is one line of text",
        ));
    }
    imp::store(key, value)
}

/// What is kept for `key`, `None` when nothing is.
pub fn read(key: &SecretKey<'_>) -> io::Result<Option<String>> {
    imp::read(key)
}

/// Forgets what is kept for `key`; nothing kept is not an error.
pub fn delete(key: &SecretKey<'_>) -> io::Result<()> {
    imp::delete(key)
}

#[cfg(all(test, unix, not(target_os = "macos")))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    #[test]
    fn a_secret_is_kept_where_only_its_owner_reads_it() {
        let root = std::env::temp_dir().join(format!("uze-secrets-{}", std::process::id()));
        let file = root.join("plugin@market/ci-status/token");
        let key = SecretKey {
            service: "uze",
            account: "plugin@market/ci-status/token",
            file: &file,
        };
        assert_eq!(read(&key).unwrap(), None);
        store(&key, "ghp_first").unwrap();
        store(&key, "ghp_second").unwrap();
        assert_eq!(read(&key).unwrap().as_deref(), Some("ghp_second"));
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        let dir_mode = std::fs::metadata(file.parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(dir_mode & 0o077, 0, "{dir_mode:o}");
        assert!(store(&key, "a\nb").is_err());
        assert!(store(&key, "").is_err());
        delete(&key).unwrap();
        delete(&key).unwrap();
        assert_eq!(read(&key).unwrap(), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_link_in_its_place_is_not_followed() {
        let root = std::env::temp_dir().join(format!("uze-secrets-link-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let elsewhere = root.join("elsewhere");
        std::fs::write(&elsewhere, "operator's").unwrap();
        let file = root.join("token");
        std::os::unix::fs::symlink(&elsewhere, &file).unwrap();
        let key = SecretKey {
            service: "uze",
            account: "token",
            file: &file,
        };
        assert!(read(&key).is_err());
        store(&key, "ghp_x").unwrap();
        assert_eq!(std::fs::read_to_string(&elsewhere).unwrap(), "operator's");
        assert_eq!(read(&key).unwrap().as_deref(), Some("ghp_x"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
