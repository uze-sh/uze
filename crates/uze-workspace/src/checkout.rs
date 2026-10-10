//! Isolated checkouts as slots.
//!
//! A slot is a long-lived working tree under the primary checkout's fixed
//! isolation directory, named by an identifier that never changes, and
//! reused by one task after another. Reuse is what makes isolation cheap:
//! ignored artifacts — build caches, dependency directories — survive from
//! one task to the next, and the number of directories is bounded by peak
//! concurrency rather than by history.
//!
//! Which directories are slots is not inferred from where they sit or what
//! they are called: the isolation directory is shared with people and their
//! agents, who add worktrees there too. A slot is a checkout carrying the
//! record UZE writes when it makes one ([`record`]); every other worktree is
//! somebody else's, and nothing here resets, reuses or removes it. Beyond
//! that record, a slot's state is derived from the directories Git
//! registers, the tasks recorded for the project, and whether any process
//! is working inside it ([`Presence`]).
//!
//! Nothing that can hold work is removed here on any automatic path, and
//! no checkout keeps work for an agent that has gone: its uncommitted
//! changes go on a shelf ([`shelf`]) and its commits stay on its branch, so
//! the checkout itself is only capacity. The removals that are safe — a
//! branch fully contained in the target, a shelf the target already has,
//! and the *directory* of a free slot nobody used within the pool's idle
//! age, its branch kept — are the only ones offered.

mod accounting;
mod anchor;
mod git_facts;
mod integration;
mod lifecycle;
mod pool;
mod reconcile;

pub use accounting::*;
pub use anchor::{admin_dir, anchored, guard};
pub use git_facts::*;
pub use integration::*;
pub use lifecycle::*;
pub use pool::*;
pub use reconcile::*;

pub mod record;
pub mod shelf;
pub mod subagent;

use std::{
    collections::HashMap,
    fmt, fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

pub use crate::process_cwd::Presence;
use crate::{
    task::{Agent, AgentId, AgentStore, Base, Isolation, WorkState},
    worktree::{BRANCH_PREFIX, WORKTREES_DIRECTORY, WorktreePolicy, label_of},
};
use record::{CheckoutRecord, Recorded};

fn git(root: &Path, args: &[&str]) -> Result<String, AcquireError> {
    crate::git::write(root, args)
        .map_err(|error| AcquireError::Git(error.to_string()))?
        .successful()
        .map(|stdout| stdout.trim().to_owned())
        .map_err(AcquireError::Git)
}

#[cfg(test)]
mod accounting_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod naming_collection_tests;

#[cfg(test)]
mod shelf_tests;
