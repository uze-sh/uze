//! What UZE asks the operating system about processes and files, answered
//! once per platform.
//!
//! A leaf crate naming no domain, no path and no harness: `uze-core`,
//! `uze-git` and the terminal runtime all need a lock that holds across
//! processes and a way to end a process with everything it started, and
//! none of them may depend on another for it. Each platform's answer is
//! written here once; a platform that cannot answer says *unknown*, never
//! *no*.

pub mod lock;
#[cfg(windows)]
pub mod windows;
