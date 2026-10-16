//! How a task's work reaches the target.
//!
//! Readiness is a Git fact read from the task's checkout — commits ahead
//! of the base on a clean tree — never something an agent announces.
//! Delivery is performed by UZE on an explicit operator action, one task at
//! a time under the repository write lock: rebase the branch onto the
//! target's tip inside the task's own checkout, run the declared gate on
//! the rebased commits, then do what the project's completion behaviour
//! says. The target is written here, in the fast-forward step of `merge`,
//! and nowhere else.
//!
//! A conflict or a failed gate leaves the target untouched, and the rebase
//! stays paused in the task's checkout with the markers in place, because
//! the agent that owns it is the only party holding the intent behind the
//! change. What that agent is told about it is the project's instruction
//! for the state, never a message of this module's.

use std::{
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{
    checkout::{self, REMOTE, commits_ahead, is_dirty},
    subprocess::run_shell_bounded,
    task::{Agent, Isolation, WorkState},
    worktree::CompletionBehavior,
};

mod deliver;
mod naming;
mod publish;
mod readiness;
mod request;
mod sync;

pub use deliver::*;
pub use naming::*;
use publish::*;
pub use readiness::*;
pub use request::*;
pub use sync::*;

/// A gate that has not finished in this long is a hung gate.
pub const GATE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::write(root, args)
        .map_err(|error| error.to_string())?
        .successful()
        .map(|stdout| stdout.trim().to_owned())
}

#[cfg(test)]
mod tests;
