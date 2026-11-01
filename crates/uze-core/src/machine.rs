//! The local environment UZE touches outside its own state.
//!
//! Infrastructure rather than domain: [`home`] owns UZE's paths,
//! [`detection_cache`] remembers which harnesses are installed,
//! [`provisioning`] and [`subprocess`] are the discipline for running
//! something, [`process_cwd`] says where this user's processes are working,
//! and [`harness_runtime`] is the experimental PATH shim.
//!
//! A module belongs here when it is about *this machine* — not about a
//! package, a capability, or a project.

pub mod detection_cache;
pub mod extension_program;
pub mod harness_runtime;
pub mod home;
pub mod inventory;
pub mod launcher;
pub mod mcp;
pub mod path;
pub mod portable_shell;
pub mod process_cwd;
pub mod provisioning;
pub mod requirement_check;
pub mod shell;
pub mod subprocess;
pub mod usage;
