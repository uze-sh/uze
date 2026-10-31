//! Lifecycle tests (L1): install/update/remove/receipts/drift at the
//! application layer, independent of any specific harness.

mod declared_extension;
mod extension_consent;
mod extension_only;
// Its probe is a POSIX `sh` script, which a Windows machine does not have;
// Windows proves the same reaches with its own `cmd.exe`.
#[cfg(unix)]
mod extension_programs;
// Windows' counterpart, its probe the system's cmd.exe.
#[cfg(windows)]
mod extension_programs_windows;
mod install;
mod manifest_and_lock;
mod subdirectory_marketplace;
