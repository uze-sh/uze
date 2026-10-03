//! The local environment UZE touches outside its own state.
//!
//! Infrastructure rather than domain: [`home`] owns UZE's paths,
//! [`detection_cache`] remembers which harnesses are installed,
//! [`provisioning`] and [`subprocess`] are the discipline for running
//! something, [`process_cwd`] says where this user's processes are working,
//! [`harness_runtime`] is the experimental PATH shim, and [`features`]
//! says which unfinished surfaces this build offers.
//!
//! A module belongs here when it is about *this machine* — not about a
//! package, a capability, or a project.

pub mod detection_cache;
pub mod features;
pub mod harness_runtime;
pub mod home;
pub mod process_cwd;
pub mod provisioning;
pub mod subprocess;
