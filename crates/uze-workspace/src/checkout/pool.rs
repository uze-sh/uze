//! The pool of slots under `.worktrees/`, and acquiring, resuming and materializing one.

use super::*;
use uze_core::shell::ShellCommand;

use crate::{approval::Consent, worktree::PolicyStep};

/// What a checkout placed without its setup says about it.
pub const SETUP_AWAITS_APPROVAL: &str = "setup was not run: this project's commands await your \
     approval (`uze workspace allow` in the project, or Review in the workspace)";

/// What a resumed agent is told when its own checkout's path was taken and
/// it was placed somewhere else: what its harness kept there stays there.
pub const CONVERSATION_NOT_CARRIED: &str = "another checkout stands where this agent worked, so it \
     was placed in a new one and its conversation could not be carried";

/// How long a free slot is kept. Decided from the slots as they stand,
/// never from a history of how many were used: a rule with no memory has
/// nothing to get wrong about the past. With no count to keep, the slots a
/// project has are the most it had in use at once within that age.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pool {
    /// Unused this long since it was last placed or released, a free slot
    /// gives its disk back.
    pub idle: Duration,
}

impl Default for Pool {
    fn default() -> Self {
        Self {
            idle: Duration::from_secs(3 * 24 * 60 * 60),
        }
    }
}

impl Pool {
    pub fn declared_by(policy: Option<&WorktreePolicy>) -> Self {
        let default = Self::default();
        Self {
            idle: policy
                .and_then(|policy| policy.idle_days)
                .map_or(default.idle, |days| {
                    Duration::from_secs(days.saturating_mul(24 * 60 * 60))
                }),
        }
    }
}

/// The name of a checkout — the directory under the isolation directory.
/// It is the name of the agent that holds it: harnesses key their
/// conversations by the directory, so an agent comes back to the same one,
/// and a slot handed to another agent is renamed to that agent's
/// ([`take`]). A name is generated only where an agent's own was taken, and
/// a checkout made by an earlier build keeps the name it already has.
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
    /// Held by something that has to clear before it can be handed on.
    Pinned { reason: Pin },
}

/// What keeps a checkout nobody is assigned to from being handed on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pin {
    /// A process is working inside it.
    InUse,
    /// Git has an operation paused in it.
    Paused(&'static str),
    /// Its agent's work could not be shelved, and is still in it.
    Unshelved(String),
    /// Its agent left work in it that no release has kept yet.
    AwaitingRelease,
}

impl fmt::Display for Pin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InUse => formatter.write_str("a process is working inside it"),
            Self::Paused(operation) => write!(formatter, "{operation} is paused in it"),
            Self::Unshelved(reason) => formatter.write_str(reason),
            Self::AwaitingRelease => formatter.write_str("its agent's work is about to be kept"),
        }
    }
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
    /// Another checkout stood at the agent's own path, so it was placed at
    /// a generated name: whatever a harness keeps by directory, the
    /// conversation included, is not there.
    pub displaced: bool,
}

/// The checkout an agent owns: the one it holds, else the one it last
/// held, else one named after the agent. An agent placed by an earlier
/// build keeps the generated name it last held, so an upgrade never moves
/// a slot out from under an agent that would still find it there.
pub fn own_checkout(agent: &AgentId, isolation: &Isolation) -> CheckoutId {
    isolation
        .checkout
        .clone()
        .or_else(|| isolation.last_checkout.clone())
        .unwrap_or_else(|| CheckoutId::adopted(agent.as_str()))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcquireError {
    /// Every slot is in use or pinned and the declared cap is reached.
    CapReached { cap: usize },
    /// The repository has no commit to branch from, or Git refused.
    Git(String),
}

impl fmt::Display for AcquireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapReached { cap } => write!(
                formatter,
                "every one of the {cap} declared checkouts is in use; close an agent to free one"
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
#[tracing::instrument(name = "checkout.slots", level = "debug", skip_all)]
pub fn slots(primary: &Path, store: &AgentStore, presence: &Presence) -> Vec<Slot> {
    let tips = BranchTips::read(primary);
    isolated_checkouts(primary)
        .into_iter()
        .filter(|(path, _)| matches!(record::read(primary, path), Recorded::Ours(_)))
        .map(|(path, branch)| {
            let id = CheckoutId::adopted(&slot_name(&path));
            let state = slot_state(
                primary,
                &tips,
                &path,
                branch.as_deref(),
                &id,
                store,
                presence,
            );
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

/// Takes a slot for `owner`, branching from `base_tip`: at the checkout it
/// owns ([`own_checkout`]), reusing a free slot first and a new directory
/// only when none can be taken and `cap` allows it. Runs as one critical
/// section under the repository write lock.
pub fn acquire(
    primary: &Path,
    store: &AgentStore,
    owner: &AgentId,
    isolation: &Isolation,
    base_tip: &str,
    cap: Option<usize>,
    observe: &dyn Fn() -> Presence,
) -> Result<Acquired, AcquireError> {
    take(
        primary,
        store,
        &isolation.branch,
        Start::Branching { base_tip },
        cap,
        observe,
        &own_checkout(owner, isolation),
    )
}

/// Takes a slot for a task whose branch already exists and whose checkout
/// is gone — removed outside UZE, or swept as idle — so an agent can pick
/// the work up where the branch stands, at the checkout it owns. Same
/// choice of slot as [`acquire`]; the difference is that nothing is cut and
/// nothing is reset past the branch's own tip.
///
/// `fresh` is where a branch that held nothing comes back: an agent that
/// ended with nothing to deliver had its empty branch removed, and
/// resuming it is resuming the conversation, on a branch cut again there.
pub fn resume(
    primary: &Path,
    store: &AgentStore,
    task: &AgentId,
    isolation: &Isolation,
    cap: Option<usize>,
    observe: &dyn Fn() -> Presence,
    fresh: Option<&str>,
) -> Result<Acquired, AcquireError> {
    if !branch_exists(primary, &isolation.branch) {
        // A branch deleted by hand while its work sat on the shelf comes
        // back where the shelf was cut, which is where that work stands on.
        let start = match (shelf::shelf_of(primary, task.as_str()), fresh) {
            (Some(shelf), _) => format!("{shelf}^1"),
            (None, Some(tip)) if !tip.trim().is_empty() => tip.to_owned(),
            (None, _) => {
                return Err(AcquireError::Git(format!(
                    "branch {} no longer exists; there is nothing to resume",
                    isolation.branch
                )));
            }
        };
        git(primary, &["branch", "--", &isolation.branch, &start])?;
    }
    take(
        primary,
        store,
        &isolation.branch,
        Start::Existing,
        cap,
        observe,
        &own_checkout(task, isolation),
    )
}

/// The checkout at `home`: kept when it is a free slot already there; a
/// free slot moved to it otherwise, preferring one whose last agent will
/// not come back to it; created there when no slot can be moved. When
/// something else already stands at `home`, a checkout of a generated name
/// is made instead and says so.
#[tracing::instrument(name = "checkout.take", level = "debug", skip_all, fields(home = %home))]
pub(super) fn take(
    primary: &Path,
    store: &AgentStore,
    branch: &str,
    start: Start<'_>,
    cap: Option<usize>,
    observe: &dyn Fn() -> Presence,
    home: &CheckoutId,
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
        // Read here, under the lock, right before a slot is moved or reset:
        // a process that arrived while the lock was awaited is still seen.
        let presence = observe();
        let existing = slots(primary, store, &presence);
        let mut home = home.clone();
        let mut displaced = false;
        if fs::symlink_metadata(home.directory(primary)).is_ok() {
            if let Some(slot) = existing
                .iter()
                .find(|slot| slot.id == home && slot.state == SlotState::Free)
            {
                return reuse(primary, slot, branch, start);
            }
            home = CheckoutId::generate();
            displaced = true;
        }
        let mut candidates: Vec<&Slot> = existing
            .iter()
            .filter(|slot| slot.state == SlotState::Free)
            .collect();
        candidates.sort_by_key(|slot| {
            (
                comes_back(store, &slot.id),
                std::cmp::Reverse(modified_at(&slot.path)),
            )
        });
        let mut set_aside = 0;
        // Git refuses a locked checkout and one with an initialized
        // submodule before it renames anything, so a refusal is a slot set
        // aside like one somebody works in.
        for slot in candidates {
            // Every process, not only the attended ones a slot's state
            // counts: a daemon standing in a slot would follow the rename
            // into the next agent's checkout without a word.
            if presence.anyone_inside(&slot.path) {
                set_aside += 1;
                continue;
            }
            if let Ok(moved) = move_slot(primary, slot, &home) {
                let mut taken = reuse(primary, &moved, branch, start)?;
                taken.displaced = displaced;
                return Ok(taken);
            }
            set_aside += 1;
        }
        // A slot that could not move is no slot an agent can have, and
        // counting it would tell the operator every one is in use.
        if let Some(cap) = cap
            && existing.len() - set_aside >= cap
        {
            return Err(AcquireError::CapReached { cap });
        }
        let mut created = create(primary, branch, start, &home)?;
        created.displaced = displaced;
        Ok(created)
    })
    .map_err(|error| AcquireError::Git(error.to_string()))?
}

/// How likely the agent that last held `slot` is to come back to it, so the
/// slot it would come back to is the last one handed on: nobody (a
/// generated name, or an agent the store forgot), an agent whose work was
/// delivered, an agent that ended with nothing, and anything else.
fn comes_back(store: &AgentStore, slot: &CheckoutId) -> u8 {
    let holder = store.agents.iter().find(|agent| {
        agent.isolation().is_some_and(|isolation| {
            isolation.checkout.as_ref() == Some(slot)
                || isolation.last_checkout.as_ref() == Some(slot)
        })
    });
    match holder.map(|agent| &agent.state) {
        None | Some(WorkState::Integrated) => 0,
        Some(WorkState::Closed) => 1,
        Some(_) => 2,
    }
}

/// Renames `slot` to `home` with Git's own worktree move, its record saying
/// so first: the intent is written before Git renames the directory and
/// cleared once the record names the new path, so a crash in between is
/// completed by [`record::settle_moves`] rather than disowning the slot.
fn move_slot(primary: &Path, slot: &Slot, home: &CheckoutId) -> Result<Slot, AcquireError> {
    let target = home.directory(primary);
    let Recorded::Ours(mut recorded) = record::read(primary, &slot.path) else {
        return Err(AcquireError::Git(format!(
            "{} carries no record of its own",
            slot.path.display()
        )));
    };
    recorded.moving_to = Some(target.clone());
    record::write(primary, &slot.path, &recorded).map_err(AcquireError::Git)?;
    let moved = git(
        primary,
        &[
            "worktree",
            "move",
            &slot.path.to_string_lossy(),
            &target.to_string_lossy(),
        ],
    );
    if let Err(refused) = moved {
        recorded.moving_to = None;
        let _ = record::write(primary, &slot.path, &recorded);
        return Err(refused);
    }
    recorded.path = target.clone();
    recorded.moving_to = None;
    record::write(primary, &target, &recorded).map_err(AcquireError::Git)?;
    Ok(Slot {
        id: home.clone(),
        path: target,
        branch: slot.branch.clone(),
        state: SlotState::Free,
    })
}

pub(super) fn reuse(
    primary: &Path,
    slot: &Slot,
    branch: &str,
    start: Start<'_>,
) -> Result<Acquired, AcquireError> {
    let root = &slot.path;
    // `--discard-changes` because a free slot may still hold edits that
    // never held it — UZE's own regions of `AGENTS.md`, a lock that moved
    // no pin — and a plain switch refuses over them or carries them into
    // the next task. The index and the tree land on the new tip in the
    // same step, which is what a `reset --hard` after it used to do.
    match start {
        Start::Branching { base_tip } => git(
            root,
            &[
                "switch",
                "--quiet",
                "--discard-changes",
                "-c",
                branch,
                "--",
                base_tip,
            ],
        )?,
        Start::Existing => git(
            root,
            &["switch", "--quiet", "--discard-changes", "--", branch],
        )?,
    };
    // Without `-x` on purpose: ignored artifacts are what make the slot
    // worth keeping.
    git(root, &["clean", "--quiet", "-fd"])?;
    // Rewritten, not kept: a subagent's record names an agent this new
    // holder is not the child of.
    record::write(primary, root, &CheckoutRecord::made_at(root)).map_err(AcquireError::Git)?;
    stamp(root);
    Ok(Acquired {
        id: slot.id.clone(),
        path: root.clone(),
        branch: branch.to_owned(),
        created: false,
        displaced: false,
    })
}

pub(super) fn create(
    primary: &Path,
    branch: &str,
    start: Start<'_>,
    id: &CheckoutId,
) -> Result<Acquired, AcquireError> {
    // A checkout removed outside UZE leaves its registry entry behind, and
    // `worktree add` then refuses the path. Adoption already looked at
    // every directory, so pruning here drops only entries with nothing
    // behind them.
    record::settle_moves(primary);
    let _ = git(primary, &["worktree", "prune"]);
    let id = id.clone();
    let path = id.directory(primary);
    isolation_directory_is_the_projects_own(primary, &path)?;
    let spelled = path.to_string_lossy();
    let mut add = vec!["worktree", "add", "--quiet"];
    match start {
        Start::Branching { base_tip } => add.extend(["-b", branch, "--", &spelled, base_tip]),
        Start::Existing => add.extend(["--", &spelled, branch]),
    }
    git(primary, &add)?;
    exclude_isolation_directory(primary)?;
    // Unrecorded, the directory would be nobody's to reuse or remove for
    // good; one that cannot carry its record is not kept at all.
    if let Err(reason) = record::write(primary, &path, &CheckoutRecord::made_at(&path)) {
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
    stamp(&path);
    Ok(Acquired {
        id,
        path,
        branch: branch.to_owned(),
        created: true,
        displaced: false,
    })
}

/// Refuses to place a slot anywhere but a directory of the primary's own.
///
/// The isolation directory is a name inside a repository somebody else may
/// have authored: committed as a link, it would send every new checkout
/// (and the project's `setup`, which runs in it) wherever the link points;
/// committed as content, the slots would sit among tracked files a reset of
/// the primary rewrites. So it must be a real directory or nothing, Git must
/// track nothing under it, and the slot it names must resolve inside the
/// primary.
fn isolation_directory_is_the_projects_own(
    primary: &Path,
    slot: &Path,
) -> Result<(), AcquireError> {
    let directory = primary.join(WORKTREES_DIRECTORY);
    let refuse = |why: &str| {
        Err(AcquireError::Git(format!(
            "`{WORKTREES_DIRECTORY}` in {} {why}; UZE places checkouts only in a directory of the \
             project's own, so remove it from the repository and try again",
            primary.display()
        )))
    };
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.file_type().is_symlink() => return refuse("is a link"),
        Ok(metadata) if !metadata.is_dir() => return refuse("is not a directory"),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(AcquireError::Git(format!(
                "could not read {}: {error}",
                directory.display()
            )));
        }
    }
    let tracked = crate::git::read(
        primary,
        &["ls-files", "-z", "--cached", "--", WORKTREES_DIRECTORY],
    )
    .map_err(AcquireError::Git)?
    .successful()
    .map_err(AcquireError::Git)?;
    if !tracked.is_empty() {
        return refuse("is tracked by the repository");
    }
    let inside = uze_platform::path::resolves_within(slot, primary)
        .map_err(|error| AcquireError::Git(error.to_string()))?;
    if !inside {
        return refuse("leads outside the project");
    }
    Ok(())
}

/// A checkout's preparation, in order: links from the primary, then the
/// declared setup command. Every problem is a warning — a checkout without
/// its `.env` or its dependencies is still better than no agent — and the
/// warnings are what the tab shows. A gate this machine cannot run is said
/// here too, when the work starts, rather than first at its delivery.
///
/// Setup runs only as the operator approved it (see [`crate::approval`]):
/// a checkout whose project's commands wait for an answer is placed
/// without them, and says so.
pub const SETUP_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[tracing::instrument(name = "checkout.materialize", level = "debug", skip_all)]
pub fn materialize(primary: &Path, slot: &Path, consent: &Consent<'_>) -> Vec<String> {
    let policy = consent.policy();
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
    let setup = match consent.setup() {
        Some(approved) => approved.commands(),
        None => {
            warnings.push(SETUP_AWAITS_APPROVAL.to_owned());
            &[]
        }
    };
    for step in setup {
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

/// Marks a slot as used now: its idle age counts from here, since nothing
/// an agent does below the slot's top directory moves that directory's own
/// time.
pub(super) fn stamp(slot: &Path) {
    let _ = uze_platform::fs::set_modified(slot, SystemTime::now());
}
