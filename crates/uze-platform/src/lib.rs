//! What UZE asks of the operating system, answered once per platform.
//!
//! Every module here is one concept with one API, and an implementation per
//! platform selected at the module boundary: nothing that uses it branches
//! on which operating system it runs on. A leaf crate naming no domain, no
//! UZE path and no harness, so `uze-core`, `uze-git` and the terminal
//! runtime can all build on it without depending on one another. A platform
//! that cannot answer says *unknown*, never *no*.

pub mod clock;
pub mod desktop;
pub mod executable;
pub mod fs;
pub mod fs_name;
pub mod home;
pub mod interrupt;
pub mod lock;
pub mod path;
pub mod probe;
pub mod process;
pub mod shell;
pub mod stdio;
pub mod tools;
#[cfg(windows)]
pub mod win;
