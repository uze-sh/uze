//! The person's desktop: what opens a web address in their browser.

/// The programs that open a web address in the person's browser, in the
/// order they are tried, each taking the address as its last argument.
pub const URL_OPENERS: &[&str] = imp::URL_OPENERS;

#[cfg(target_os = "macos")]
mod imp {
    /// `open` is the one opener there.
    pub(super) const URL_OPENERS: &[&str] = &["open"];
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    /// `explorer.exe` is how WSL reaches the Windows browser.
    pub(super) const URL_OPENERS: &[&str] = &["xdg-open", "sensible-browser", "explorer.exe"];
}

#[cfg(windows)]
mod imp {
    /// `explorer.exe` hands an address to the default browser, with none of
    /// the `cmd /C start` parsing that splits one at its `&`.
    pub(super) const URL_OPENERS: &[&str] = &["explorer.exe"];
}
