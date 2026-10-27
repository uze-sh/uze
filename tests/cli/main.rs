//! CLI layer tests (L1/L2): the real `uze` binary against isolated
//! environments — grammar/precedence (ADR-019) and machine-scoped
//! commands. Project-scoped CLI semantics live in `workspace/` and
//! `acceptance/`.

mod budget;
// Landlock and seccomp are Linux's: the kernel the helper is proved
// against; macOS's profile is proved as text in uze-platform.
#[cfg(target_os = "linux")]
mod confine;
mod extensions;
mod grammar;
mod machine;
mod portable_shell;
