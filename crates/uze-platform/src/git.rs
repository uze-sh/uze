//! What Git needs on this platform: how a person installs it, and the
//! settings UZE gives it on the command line so a checkout means the same
//! thing on every machine.

/// How a person installs Git here, as a command they can type.
pub const INSTALL_HINT: &str = imp::INSTALL_HINT;

/// The Git settings under which a checkout holds a repository's files as
/// they were committed, on this platform: no line-ending conversion
/// anywhere; on Windows also no links Git cannot make without a privilege
/// (each becomes a file holding its target), paths longer than the Win32
/// limit, and no executable bit read from a file system that keeps none.
/// Pushed rather than left to a Git build's defaults, which differ between
/// builds.
pub const FAITHFUL_CHECKOUT: &[(&str, &str)] = imp::FAITHFUL_CHECKOUT;

/// What every Git invocation needs on this platform, whatever repository it
/// reads, and nothing about how the person's own files are held: on
/// Windows, paths past the Win32 limit, which `.worktrees/<id>/` brings
/// closer to every file of the repository. Given on the command line, as a
/// `-c`, so a person's configuration is never written to.
pub const EVERY_INVOCATION: &[(&str, &str)] = imp::EVERY_INVOCATION;

#[cfg(unix)]
mod imp {
    pub(super) const INSTALL_HINT: &str = "install it with your package manager";

    pub(super) const FAITHFUL_CHECKOUT: &[(&str, &str)] =
        &[("core.autocrlf", "false"), ("core.eol", "lf")];

    pub(super) const EVERY_INVOCATION: &[(&str, &str)] = &[];
}

#[cfg(windows)]
mod imp {
    pub(super) const INSTALL_HINT: &str = "winget install Git.Git";

    pub(super) const FAITHFUL_CHECKOUT: &[(&str, &str)] = &[
        ("core.autocrlf", "false"),
        ("core.eol", "lf"),
        ("core.symlinks", "false"),
        ("core.longpaths", "true"),
        // The file system keeps no executable bit for Git to compare.
        ("core.fileMode", "false"),
    ];

    pub(super) const EVERY_INVOCATION: &[(&str, &str)] = &[("core.longpaths", "true")];
}
