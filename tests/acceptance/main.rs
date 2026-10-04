//! Acceptance suite (L3): the real `uze` binary, exercised through its
//! public CLI, inside a fully isolated `TestEnvironment`.
//!
//! Rule (tests/README.md): an acceptance test walks the public path —
//! `uze` binary → Application → Store/Engine → Integration — never internal
//! methods, and never against the developer's real HOME/UZE_HOME/PATH.

mod agent_surface;
mod canonical_package;
// Every case here drives shebang stand-ins: Unix-only until they dispatch
// through `uze-fake-harness` (windows-support task 9.2).
#[cfg(unix)]
mod engine;
mod fresh_project;
mod lifecycle;
mod multi_harness;
mod package_and_workspace;
mod package_only;
// Every case here drives shebang stand-ins: Unix-only until they dispatch
// through `uze-fake-harness` (windows-support task 9.2).
#[cfg(unix)]
mod runtime_shim;
// Every case here drives shebang stand-ins: Unix-only until they dispatch
// through `uze-fake-harness` (windows-support task 9.2).
#[cfg(unix)]
mod session_continuity;
mod util;
mod workspace_health;
