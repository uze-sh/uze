//! Whether a checkout's work is ready to deliver, and the policy and outcomes of a delivery.

use super::*;

/// What the task's checkout says about the task.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Readiness {
    /// No commits beyond the base yet, or no checkout to read.
    Running,
    /// Uncommitted changes in the checkout.
    Uncommitted,
    /// A rebase is paused in the checkout on these files.
    Rebasing { files: Vec<PathBuf> },
    /// Commits ahead of `base` on a clean tree.
    Ready { ahead: usize, base: String },
}

/// The project's say in delivery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Policy<'a> {
    pub completion: CompletionBehavior,
    /// What runs in the task's checkout on the rebased commits, in order;
    /// the first non-zero exit refuses delivery.
    pub gate: &'a [String],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Delivered {
    /// The branch is left for the operator.
    Handoff,
    /// The target now points at the branch's tip.
    Merged { target_tip: String },
    /// The branch was pushed under its readable name and the forge
    /// already has `request` open for it: a sync, which is Git alone.
    Published { branch: String, request: u32 },
    /// The branch was pushed and no request is open for it yet. Opening
    /// one is the owning agent's, which is why this carries the words to
    /// hand it: only that agent knows what the change is for, and only it
    /// can reach whichever forge this remote is.
    AwaitingRequest { branch: String, instruction: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeliveryFailure {
    NotReady(Readiness),
    /// The rebase stopped on these files and stays paused in the checkout;
    /// `target_moved` is how many commits the target gained since the task's
    /// base.
    Conflict {
        files: Vec<PathBuf>,
        target_moved: usize,
    },
    GateFailed {
        /// The step that failed — named, because a gate of several steps
        /// that reports only output leaves the reader diffing the output
        /// against the manifest to find out which one.
        command: String,
        output: String,
    },
    /// The operator has uncommitted changes to files the task changed.
    Overlap {
        files: Vec<PathBuf>,
    },
    /// The target already carries the branch's work under commits of its
    /// own — a squash or rebase merge made elsewhere.
    AlreadyDelivered,
    NoRemote,
    Git(String),
}

impl fmt::Display for DeliveryFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotReady(Readiness::Running) => formatter.write_str("nothing to deliver yet"),
            Self::NotReady(Readiness::Uncommitted) => {
                formatter.write_str("the checkout has uncommitted changes")
            }
            Self::NotReady(Readiness::Rebasing { .. }) => {
                formatter.write_str("a rebase is still paused in the checkout")
            }
            Self::NotReady(Readiness::Ready { .. }) => formatter.write_str("ready"),
            Self::Conflict { files, .. } => {
                write!(formatter, "conflicts in {}", join_paths(files))
            }
            Self::GateFailed { command, .. } => write!(formatter, "the gate failed at `{command}`"),
            Self::Overlap { files } => write!(
                formatter,
                "the primary checkout has uncommitted changes to {}",
                join_paths(files)
            ),
            Self::AlreadyDelivered => formatter.write_str("the target already carries this work"),
            Self::NoRemote => formatter.write_str("the repository has no `origin` remote"),
            Self::Git(reason) => formatter.write_str(reason),
        }
    }
}

impl std::error::Error for DeliveryFailure {}

pub(super) fn join_paths(files: &[PathBuf]) -> String {
    files
        .iter()
        .map(|file| file.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The task's checkout directory, when it has one.
pub fn slot_path(primary: &Path, isolation: &Isolation) -> Option<PathBuf> {
    let checkout = isolation.checkout.as_ref()?;
    let path = checkout.directory(primary);
    path.is_dir().then_some(path)
}

/// Reads where an isolated agent's work stands, from its own checkout.
///
/// The base is the newest of the recorded one and the target's local tip
/// when the branch already descends from it — the case after an agent
/// finished a paused rebase itself — so `ahead` counts the task's own
/// commits and never the target's.
pub fn readiness(primary: &Path, isolation: &Isolation) -> Readiness {
    let Some(slot) = slot_path(primary, isolation) else {
        return Readiness::Running;
    };
    if let Some(files) = paused_rebase(&slot) {
        return Readiness::Rebasing { files };
    }
    if is_dirty(&slot) {
        return Readiness::Uncommitted;
    }
    let base = effective_base(primary, isolation);
    let ahead = commits_ahead(primary, &base, &isolation.branch);
    if ahead == 0 {
        Readiness::Running
    } else {
        Readiness::Ready { ahead, base }
    }
}

/// Where the work stands in a checkout nobody cut for an agent — the
/// project's own root, where an agent sits beside the operator.
///
/// The same three questions readiness asks of a slot, asked of a directory
/// instead: a rebase paused there, a dirty tree, commits the target lacks.
/// What it cannot ask is whose they are, and it deliberately does not try:
/// the answer is about the checkout, and every agent in it reads the same
/// one. That is the truth about where they are, and a better answer than
/// the nothing they used to get.
pub fn readiness_of_checkout(checkout: &Path, target: &str) -> Readiness {
    if let Some(files) = paused_rebase(checkout) {
        return Readiness::Rebasing { files };
    }
    if is_dirty(checkout) {
        return Readiness::Uncommitted;
    }
    let Some(branch) = checkout::current_branch(checkout) else {
        // Mid-rebase, or a detached head: nothing to be ahead of.
        return Readiness::Running;
    };
    let ahead = commits_ahead(checkout, target, &branch);
    if ahead == 0 {
        Readiness::Running
    } else {
        Readiness::Ready {
            ahead,
            base: target.to_owned(),
        }
    }
}
