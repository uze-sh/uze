//! The pool of slots under `.worktrees/`, and acquiring, resuming and materializing one.

use super::*;
use uze_core::shell::ShellCommand;

use crate::worktree::PolicyStep;

/// How many free slots are kept, and for how long. Decided from the slots
/// as they stand, never from a history of how many were used: a rule with
/// no memory has nothing to get wrong about the past.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pool {
    /// Free slots kept warm for the next agents, the most recently used.
    pub spare: usize,
    /// Unused this long, even a spare gives its disk back.
    pub idle: Duration,
}

impl Default for Pool {
    fn default() -> Self {
        Self {
            spare: 2,
            idle: Duration::from_secs(3 * 24 * 60 * 60),
        }
    }
}

impl Pool {
    pub fn declared_by(policy: Option<&WorktreePolicy>) -> Self {
        let default = Self::default();
        Self {
            spare: policy
                .and_then(|policy| policy.spare)
                .unwrap_or(default.spare),
            idle: policy
                .and_then(|policy| policy.idle_days)
                .map_or(default.idle, |days| {
                    Duration::from_secs(days.saturating_mul(24 * 60 * 60))
                }),
        }
    }
}

/// The generated, immutable name of a slot — the directory under the
/// isolation directory. Never derived from a task or a label, so a slot
/// outlives every task that runs in it. A legacy checkout keeps the name it
/// already has.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct CheckoutId(String);

impl CheckoutId {
    pub fn generate() -> Self {
        Self(crate::task::generated_identifier(b"checkout"))
    }

    pub fn adopted(name: &str) -> Self {
        Self(name.to_owned())
    }

    /// Whether the name is the `agent-<n>` the builds before slots gave
    /// their checkouts, which carry no record of their own.
    pub(super) fn is_legacy(&self) -> bool {
        self.0.strip_prefix("agent-").is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        })
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The slot's directory under `primary`, whether or not it exists.
    pub fn directory(&self, primary: &Path) -> PathBuf {
        crate::worktree::slot_directory(primary, self.as_str())
    }
}

impl fmt::Display for CheckoutId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SlotState {
    /// An agent is here: its task is live, or a pane still sits in the
    /// directory after the task ended — a delivered task whose agent has
    /// not left is still somebody's checkout.
    Occupied { task: AgentId },
    /// Clean, and everything on its branch is in the target or was
    /// declared done: the next agent may take it.
    Free,
    /// Holds work nobody delivered: uncommitted changes, or commits the
    /// target lacks. Only the operator moves it on.
    Parked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Slot {
    pub id: CheckoutId,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub state: SlotState,
}

/// What acquiring a slot produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Acquired {
    pub id: CheckoutId,
    pub path: PathBuf,
    pub branch: String,
    /// `false` when a free slot was reused.
    pub created: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcquireError {
    /// Every slot is occupied or parked and the declared cap is reached.
    CapReached { cap: usize },
    /// The repository has no commit to branch from, or Git refused.
    Git(String),
}

impl fmt::Display for AcquireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapReached { cap } => write!(
                formatter,
                "every one of the {cap} declared checkouts is in use; deliver or park a task first"
            ),
            Self::Git(reason) => formatter.write_str(reason),
        }
    }
}

impl std::error::Error for AcquireError {}

/// The slots of `primary` and their state, derived from Git, `store`, and
/// `presence` — where somebody is working. The task record alone cannot
/// say whether an agent is still there: a task ends when its work is
/// delivered, and the agent that delivered it is usually still in the
/// checkout, so a slot read without the process table would be handed to
/// the next agent under the feet of the last.
///
/// Only a recorded checkout in the isolation directory is a slot: reuse
/// resets and cleans one and collection removes it, which is only safe
/// for a directory UZE made.
pub fn slots(primary: &Path, store: &AgentStore, presence: &Presence) -> Vec<Slot> {
    isolated_checkouts(primary)
        .into_iter()
        .filter(|(path, _)| matches!(record::read(path), Recorded::Ours(_)))
        .map(|(path, branch)| {
            let id = CheckoutId::adopted(&slot_name(&path));
            let state = slot_state(primary, &path, branch.as_deref(), &id, store, presence);
            Slot {
                id,
                path,
                branch,
                state,
            }
        })
        .collect()
}

/// Where a slot's branch comes from when the slot is taken.
#[derive(Clone, Copy)]
pub(super) enum Start<'a> {
    /// A new branch, cut from this tip.
    Branching { base_tip: &'a str },
    /// A branch that already exists, checked out as it stands.
    Existing,
}

/// Takes a slot for `task`, branching from `base_tip`: a free slot first,
/// a new directory only when none is free and `cap` allows it. Runs as one
/// critical section under the repository write lock.
pub fn acquire(
    primary: &Path,
    store: &AgentStore,
    isolation: &Isolation,
    base_tip: &str,
    cap: Option<usize>,
    presence: &Presence,
) -> Result<Acquired, AcquireError> {
    take(
        primary,
        store,
        &isolation.branch,
        Start::Branching { base_tip },
        cap,
        presence,
    )
}

/// Takes a slot for a task whose branch already exists and whose checkout
/// is gone — removed outside UZE, or swept as idle — so an agent can pick
/// the work up where the branch stands. Same choice of slot as
/// [`acquire`]; the difference is that nothing is cut and nothing is
/// reset past the branch's own tip.
pub fn resume(
    primary: &Path,
    store: &AgentStore,
    isolation: &Isolation,
    cap: Option<usize>,
    presence: &Presence,
) -> Result<Acquired, AcquireError> {
    if !branch_exists(primary, &isolation.branch) {
        return Err(AcquireError::Git(format!(
            "branch {} no longer exists; there is nothing to resume",
            isolation.branch
        )));
    }
    take(
        primary,
        store,
        &isolation.branch,
        Start::Existing,
        cap,
        presence,
    )
}

pub(super) fn take(
    primary: &Path,
    store: &AgentStore,
    branch: &str,
    start: Start<'_>,
    cap: Option<usize>,
    presence: &Presence,
) -> Result<Acquired, AcquireError> {
    // Reuse resets a slot's working tree to this commit, so a base nobody
    // could resolve is refused before a slot is chosen rather than handed
    // to `git reset --hard`.
    if let Start::Branching { base_tip } = start
        && base_tip.trim().is_empty()
    {
        return Err(AcquireError::Git(
            "the commit to branch from could not be resolved".to_owned(),
        ));
    }
    uze_git::locked(primary, uze_git::DEFAULT_WRITE_TIMEOUT, || {
        let existing = slots(primary, store, presence);
        if let Some(free) = existing
            .iter()
            .filter(|slot| slot.state == SlotState::Free)
            .max_by_key(|slot| modified_at(&slot.path))
        {
            return reuse(free, branch, start);
        }
        if let Some(cap) = cap
            && existing.len() >= cap
        {
            return Err(AcquireError::CapReached { cap });
        }
        create(primary, branch, start)
    })
    .map_err(|error| AcquireError::Git(error.to_string()))?
}

pub(super) fn reuse(slot: &Slot, branch: &str, start: Start<'_>) -> Result<Acquired, AcquireError> {
    let root = &slot.path;
    match start {
        Start::Branching { base_tip } => {
            git(root, &["switch", "--quiet", "-c", branch, "--", base_tip])?
        }
        Start::Existing => git(root, &["switch", "--quiet", "--", branch])?,
    };
    // `HEAD`, not the tip spelled out again: the switch above has just put
    // HEAD on it, and `git reset` is the one destructive command with no
    // option terminator at all — `--` means pathspec ("Cannot do hard reset
    // with paths"), and `--end-of-options` is refused outright ("must come
    // before non-option arguments", git 2.43). A literal that can never be
    // read as an option is the only spelling left that cannot be steered by
    // a ref name.
    git(root, &["reset", "--quiet", "--hard", "HEAD"])?;
    // Without `-x` on purpose: ignored artifacts are what make the slot
    // worth keeping.
    git(root, &["clean", "--quiet", "-fd"])?;
    // Rewritten, not kept: a subagent's record names an agent this new
    // holder is not the child of.
    record::write(root, &CheckoutRecord::made_at(root)).map_err(AcquireError::Git)?;
    Ok(Acquired {
        id: slot.id.clone(),
        path: root.clone(),
        branch: branch.to_owned(),
        created: false,
    })
}

pub(super) fn create(
    primary: &Path,
    branch: &str,
    start: Start<'_>,
) -> Result<Acquired, AcquireError> {
    // A checkout removed outside UZE leaves its registry entry behind, and
    // `worktree add` then refuses the path. Adoption already looked at
    // every directory, so pruning here drops only entries with nothing
    // behind them.
    let _ = git(primary, &["worktree", "prune"]);
    let id = CheckoutId::generate();
    let relative = format!("{WORKTREES_DIRECTORY}/{id}");
    match start {
        Start::Branching { base_tip } => git(
            primary,
            &[
                "worktree", "add", "--quiet", "-b", branch, "--", &relative, base_tip,
            ],
        )?,
        Start::Existing => git(
            primary,
            &["worktree", "add", "--quiet", "--", &relative, branch],
        )?,
    };
    exclude_isolation_directory(primary)?;
    let path = primary.join(relative);
    // Unrecorded, the directory would be nobody's to reuse or remove for
    // good; one that cannot carry its record is not kept at all.
    if let Err(reason) = record::write(&path, &CheckoutRecord::made_at(&path)) {
        let _ = git(
            primary,
            &[
                "worktree",
                "remove",
                "--force",
                "--",
                &path.to_string_lossy(),
            ],
        );
        return Err(AcquireError::Git(reason));
    }
    Ok(Acquired {
        id,
        path,
        branch: branch.to_owned(),
        created: true,
    })
}

/// A checkout's preparation, in order: links from the primary, then the
/// declared setup command. Every problem is a warning — a checkout without
/// its `.env` or its dependencies is still better than no agent — and the
/// warnings are what the tab shows. A gate this machine cannot run is said
/// here too, when the work starts, rather than first at its delivery.
pub const SETUP_TIMEOUT: Duration = Duration::from_secs(20 * 60);

pub fn materialize(primary: &Path, slot: &Path, policy: &WorktreePolicy) -> Vec<String> {
    let mut warnings = Vec::new();
    for link in &policy.link {
        let source = primary.join(link);
        let destination = slot.join(link);
        if !source.exists() {
            warnings.push(format!(
                "`{}` is not in the primary checkout; not linked",
                link.display()
            ));
            continue;
        }
        if destination.exists() || fs::symlink_metadata(&destination).is_ok() {
            continue;
        }
        if let Some(parent) = destination.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            warnings.push(format!("could not prepare `{}`: {error}", link.display()));
            continue;
        }
        if let Err(error) = uze_platform::fs::link_entry(&source, &destination) {
            warnings.push(format!("could not link `{}`: {error}", link.display()));
        }
    }
    // In order, and stopping at the first failure: a later step almost
    // always assumes the earlier one ran, so continuing would produce a
    // second, more confusing warning about the same cause.
    for step in &policy.setup {
        // Never run in a shell it was not written for: the checkout is
        // still placed, and says which step it went without.
        let Some(line) = step.here() else {
            warnings.push(format!(
                "setup `{step}` has no {} spelling; not run",
                ShellCommand::platform()
            ));
            break;
        };
        let (passed, output) = crate::subprocess::run_shell_bounded(slot, line, SETUP_TIMEOUT);
        if !passed {
            let tail = output.lines().last().unwrap_or("").to_owned();
            warnings.push(format!("setup `{step}` failed: {tail}"));
            break;
        }
    }
    warnings.extend(
        policy
            .steps_not_spelled_here()
            .into_iter()
            .filter(|(step, _)| *step == PolicyStep::Gate)
            .map(|(_, gate)| {
                format!(
                    "gate `{gate}` has no {} spelling; this work cannot be delivered from here",
                    ShellCommand::platform()
                )
            }),
    );
    warnings
}
