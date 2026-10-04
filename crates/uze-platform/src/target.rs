//! What this build was compiled for, in the words release tables and
//! vendors' download pages use: `linux`, `macos`, `windows`; `x86_64`,
//! `aarch64`. A table that picks an archive or a package matches on these,
//! as data; nothing reads them to decide how to do something, which is what
//! the rest of this crate is for.

/// The operating system, as Rust names it (`std::env::consts::OS`).
pub const OS: &str = std::env::consts::OS;

/// The processor architecture, as Rust names it (`std::env::consts::ARCH`).
pub const ARCH: &str = std::env::consts::ARCH;

/// Whether the C library this build links is musl rather than glibc: a
/// running binary knows what an installer has to ask `ldd`.
pub const MUSL: bool = cfg!(target_env = "musl");
