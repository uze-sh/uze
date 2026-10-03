//! Acceptance suite (L3): the real `uze` binary, exercised through its
//! public CLI, inside a fully isolated `TestEnvironment`.
//!
//! Rule (tests/README.md): an acceptance test walks the public path —
//! `uze` binary → Application → Store/Engine → Integration — never internal
//! methods, and never against the developer's real HOME/UZE_HOME/PATH.

mod agent_surface;
mod canonical_package;
mod engine;
mod fresh_project;
mod lifecycle;
mod multi_harness;
mod package_and_workspace;
mod package_only;
mod runtime_shim;
mod session_continuity;
mod util;
mod workspace_health;
