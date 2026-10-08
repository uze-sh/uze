//! What happens to a checkout over its life: carried changes, collection, pruning, release, discard, removal and adoption.

use super::*;
use uze_core::path::Canonical as _;

/// Copies the changes `primary`'s working tree holds over its `HEAD`
/// into `slot`, leaving `primary` exactly as it was.
///
/// Copied, never moved. UZE cannot say which uncommitted change belongs
/// to which agent — the root's working tree is shared by the operator
/// and every agent standing in it — so taking them would take the
/// operator's own work out of their tree, which is the one thing
/// `add-portable-worktree-policy`'s invariant forbids.
///
/// What travels is everything the repository would report as a change:
/// the diff over `HEAD` for what it tracks, and a copy of each file it
/// does not track but does not ignore. A new file is the commonest shape
/// an agent's work has before its first commit, so leaving it behind
/// would take the agent's own module out from under it. What the
/// repository *ignores* stays where it is — a `target/` or a
/// `node_modules/` is the slot's own to build, and `link`/`setup` is
/// where a project says otherwise.
pub fn carry_changes(primary: &Path, slot: &Path) -> Result<(), String> {
    // Read rather than written, and taken *untrimmed*: a patch's final
    // newline is part of it, and `git apply` refuses one that lost it.
    let patch = uze_git::read(primary, &["diff", "HEAD"])
        .map_err(|error| error.to_string())?
        .successful()?;
    if !patch.trim().is_empty() {
        uze_git::write_with_stdin(slot, &["apply", "--"], &patch)
            .map_err(|error| error.to_string())?
            .successful()
            .map_err(|error| {
                format!("the changes could not be carried into the checkout: {error}")
            })?;
    }
    for relative in uze_git::read(
        primary,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )
    .map_err(|error| error.to_string())?
    .successful()?
    .split('\0')
    .filter(|entry| !entry.is_empty())
    {
        let from = primary.join(relative);
        let to = slot.join(relative);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("the checkout could not take `{relative}`: {error}"))?;
        }
        std::fs::copy(&from, &to)
            .map_err(|error| format!("`{relative}` could not be copied: {error}"))?;
    }
    keep_derived_instructions_behind(primary, slot)
}

/// Undoes, in a slot just given the primary's changes, a change to
/// `AGENTS.md` that lies only inside UZE's managed regions. The workspace
/// keeps its region in step in the primary checkout, where it is the
/// operator's to commit; carried into a slot it would be a change the agent
/// could commit and deliver, colliding with the same change still
/// uncommitted in the primary. A change of the operator's own beside the
/// regions is theirs, and is carried like any other.
pub(super) fn keep_derived_instructions_behind(primary: &Path, slot: &Path) -> Result<(), String> {
    use crate::project_context::AGENTS_MD_FILE_NAME;

    if !instructions_changed_only_in_regions(primary) {
        return Ok(());
    }
    let in_slot = slot.join(AGENTS_MD_FILE_NAME);
    match committed_text(slot, AGENTS_MD_FILE_NAME) {
        Committed::Text(text) => std::fs::write(&in_slot, text)
            .map_err(|error| format!("`{AGENTS_MD_FILE_NAME}` could not be restored: {error}")),
        Committed::Absent => match std::fs::remove_file(&in_slot) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(format!(
                "`{AGENTS_MD_FILE_NAME}` could not be left out: {error}"
            )),
            _ => Ok(()),
        },
        Committed::Unknown => Ok(()),
    }
}

/// What a collection removed, so a caller can say so.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Collected {
    pub branches: Vec<String>,
    pub slots: Vec<CheckoutId>,
}

/// The two removals that cannot lose work, as one critical section: a
/// branch whose every commit is in the target, and the directory of a free
/// slot the `pool` does not keep, its branch kept.
///
/// Both are safe on their own; taking the write lock once around them is
/// what keeps a branch from being pruned in the moment a concurrent
/// acquisition is creating it.
#[tracing::instrument(name = "checkout.collect", level = "debug", skip_all)]
pub fn collect(
    primary: &Path,
    store: &AgentStore,
    target: &str,
    pool: Pool,
    presence: &Presence,
) -> Collected {
    uze_git::locked(primary, uze_git::DEFAULT_WRITE_TIMEOUT, || Collected {
        branches: prune_integrated_branches(primary, store, target),
        slots: trim_free_slots(primary, store, pool, presence),
    })
    .unwrap_or_default()
}

/// Deletes every branch UZE can account for — one under the `agent/`
/// prefix, or one a recorded task names — whose commits are all reachable
/// from `target` and which no live task and no checkout is using. Returns
/// the branches removed.
///
/// A `target` this repository does not have is refused outright rather than
/// asked about branch by branch: the name comes from `workspace.target` in
/// `agents.yaml`, which is authored and committed and may well name a
/// branch this clone never fetched. Nothing is collectable against a
/// yardstick that does not exist.
pub fn prune_integrated_branches(primary: &Path, store: &AgentStore, target: &str) -> Vec<String> {
    if tip_of(primary, target).is_empty() {
        return Vec::new();
    }
    let checked_out: Vec<String> = linked_worktrees(primary)
        .into_iter()
        .filter_map(|(_, branch)| branch)
        .collect();
    let live: Vec<&str> = store
        .isolated()
        .filter(|agent| is_live(&agent.state))
        .filter_map(|agent| agent.isolation())
        .map(|isolation| isolation.branch.as_str())
        .collect();
    let tips = BranchTips::read(primary);
    let mut removed = Vec::new();
    // The prefix is no longer the whole answer: a named task's branch left
    // it behind (`worktree::BranchVocabulary`), and a branch nobody can
    // find is a branch nobody collects. The store names what it knows; the
    // prefix still catches what the store has forgotten.
    let mut candidates = agent_branches(primary);
    for isolation in store.isolated().filter_map(Agent::isolation) {
        if !candidates.contains(&isolation.branch) {
            candidates.push(isolation.branch.clone());
        }
    }
    for branch in candidates {
        // A record outlives its branch: one already gone has nothing left
        // to delete, and asking about it by name costs a process apiece.
        if !tips.contains(&branch)
            || checked_out.contains(&branch)
            || live.contains(&branch.as_str())
        {
            continue;
        }
        // A subagent's branch is done once its agent's branch has it.
        let into = store
            .isolated()
            .filter(|agent| agent.parent.is_some())
            .filter_map(Agent::isolation)
            .find(|isolation| isolation.branch == branch)
            .map_or(target, |isolation| isolation.target.as_str());
        if is_integrated_among(&tips, primary, into, &branch)
            && git(primary, &["branch", "-D", "--", &branch]).is_ok()
        {
            removed.push(branch);
        }
    }
    removed
}

/// Removes the directory of every free slot the `pool` does not keep —
/// beyond its spares, the most recently used first, or unused past its idle
/// age — keeping each one's branch. Returns the slots removed.
pub fn trim_free_slots(
    primary: &Path,
    store: &AgentStore,
    pool: Pool,
    presence: &Presence,
) -> Vec<CheckoutId> {
    let now = SystemTime::now();
    let mut free: Vec<(SystemTime, Slot)> = slots(primary, store, presence)
        .into_iter()
        .filter(|slot| slot.state == SlotState::Free)
        .map(|slot| (modified_at(&slot.path), slot))
        .collect();
    free.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    let mut removed = Vec::new();
    for (kept, (modified, slot)) in free.into_iter().enumerate() {
        let idle = now.duration_since(modified).unwrap_or_default();
        if kept < pool.spare && idle < pool.idle {
            // A spare is what the next placement switches, and one whose
            // files were written in the same second as its index reads as
            // possibly changed until the index is written again: every
            // status hashes every file, and the switch does too. Written
            // here, where nobody waits for it, instead of by the placement
            // that would otherwise pay for it.
            let _ = git(&slot.path, &["update-index", "-q", "--refresh"]);
            continue;
        }
        if set_aside(primary, &slot.path) {
            removed.push(slot.id);
        }
    }
    removed
}

/// Where a collected slot waits for its bytes to be deleted: inside the
/// isolation directory, so the move is a rename on the same filesystem,
/// and one level down, so it is never read as a slot.
pub(super) const TRASH_DIRECTORY: &str = ".trash";

/// Takes a free slot out of the pool in the time a rename takes, leaving
/// its bytes for [`empty_trash`].
///
/// A slot can hold gigabytes of build output, and deleting them under the
/// repository lock held every placement in the project for as long as the
/// disk took. Moved aside, the directory is gone from Git's registry once
/// `prune` finds its path empty, and what is left is a plain directory
/// nothing refers to. Asked again first, because the slot was read as
/// clean before this and `worktree remove` used to refuse a tree that
/// became dirty since; a rename has no such check of its own. Where the
/// rename is refused — a file held open, on a platform that forbids moving
/// it — the slot is removed in place, as it always was.
pub(super) fn set_aside(primary: &Path, slot: &Path) -> bool {
    if holds_uncommitted_work(slot) {
        return false;
    }
    let trash = primary.join(WORKTREES_DIRECTORY).join(TRASH_DIRECTORY);
    let destination = trash.join(slot_name(slot));
    let moved = fs::create_dir_all(&trash).is_ok()
        && !destination.exists()
        && fs::rename(slot, &destination).is_ok();
    if moved {
        return git(primary, &["worktree", "prune"]).is_ok();
    }
    let path = slot.to_string_lossy().into_owned();
    git(primary, &["worktree", "remove", "--", &path]).is_ok()
}

/// Deletes what [`trim_free_slots`] set aside, with no lock held: nothing
/// refers to it any more, and a removal the process did not live to finish
/// is finished by the next one.
#[tracing::instrument(name = "checkout.empty_trash", level = "debug", skip_all)]
pub fn empty_trash(primary: &Path) {
    let trash = primary.join(WORKTREES_DIRECTORY).join(TRASH_DIRECTORY);
    let Ok(entries) = fs::read_dir(&trash) else {
        return;
    };
    for entry in entries.flatten() {
        let _ = fs::remove_dir_all(entry.path());
    }
}

/// Ends `task` because nothing is in front of its checkout any more: the
/// slot goes back to the pool when it holds nothing, and is parked for the
/// operator when it holds work.
///
/// This is what an agent's departure means for its slot, and the only
/// transition besides delivery that frees one. Without it a task stays live
/// for as long as its record does — its slot occupied, its directory never
/// reused, and every new agent paying for a working tree of its own.
pub fn release(primary: &Path, agent: &mut Agent, target: &str) -> SlotState {
    let Some(isolation) = agent.isolation() else {
        return SlotState::Free;
    };
    let directory = isolation
        .checkout
        .as_ref()
        .map(|checkout| checkout.directory(primary))
        .filter(|path| path.is_dir());
    let holds_work = directory
        .is_some_and(|path| holds_uncommitted_work(&path) || holds_unbranched_commits(&path))
        || (branch_exists(primary, &isolation.branch)
            && !is_integrated(primary, target, &isolation.branch));
    if is_live(&agent.state) {
        agent.state = if holds_work {
            WorkState::Parked
        } else {
            WorkState::Closed
        };
    }
    // Work the operator declared done keeps its branch and frees its slot,
    // exactly as `slot_state` reads it: the commits are not lost, they are
    // simply nobody's turn any more.
    if holds_work && agent.state != WorkState::Integrated {
        SlotState::Parked
    } else {
        SlotState::Free
    }
}

/// The one removal that loses work, and therefore the one only an operator
/// takes on a named task: the checkout directory, forced, and the branch.
pub fn discard(primary: &Path, isolation: &Isolation) -> Result<(), String> {
    uze_git::locked(primary, uze_git::DEFAULT_WRITE_TIMEOUT, || {
        if let Some(checkout) = &isolation.checkout {
            let path = checkout.directory(primary);
            if path.is_dir() {
                git(
                    primary,
                    &[
                        "worktree",
                        "remove",
                        "--force",
                        "--",
                        &path.to_string_lossy(),
                    ],
                )
                .map_err(|error| error.to_string())?;
            }
        }
        if branch_exists(primary, &isolation.branch) {
            git(primary, &["branch", "-D", "--", &isolation.branch])
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    })
    .map_err(|error| error.to_string())?
}

/// Why the operator's adoption or removal of a checkout was refused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// The project's own checkout: the operator's, never a slot.
    Primary,
    /// Git does not register it as a worktree of this repository.
    NotRegistered,
    /// Only a checkout directly in the isolation directory can be a slot.
    OutsideIsolationDirectory,
    AlreadyRecorded,
    /// Its record is one this build cannot read, a newer build's.
    Unreadable,
    /// A harness keeps it, and only that harness drives it.
    LeftToHarness(String),
    /// A live agent holds it.
    HeldBy(String),
    UncommittedWork,
    UnbranchedCommits,
    InUse,
    Failed(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Primary => formatter.write_str("it is the project's own checkout"),
            Self::NotRegistered => formatter.write_str("Git does not list it as a worktree"),
            Self::OutsideIsolationDirectory => write!(
                formatter,
                "only a checkout directly under {WORKTREES_DIRECTORY}/ can be a slot"
            ),
            Self::AlreadyRecorded => formatter.write_str("it is already UZE's"),
            Self::Unreadable => {
                formatter.write_str("its record was written by a newer UZE and is left as it is")
            }
            Self::LeftToHarness(harness) => write!(formatter, "it is left to {harness}"),
            Self::HeldBy(agent) => write!(formatter, "{agent} is working in it"),
            Self::UncommittedWork => formatter.write_str("it holds uncommitted work"),
            Self::UnbranchedCommits => formatter.write_str("it holds commits no branch reaches"),
            Self::InUse => formatter.write_str("a process is working inside it"),
            Self::Failed(reason) => formatter.write_str(reason),
        }
    }
}

/// What would keep the operator from removing `path`, read from the
/// checkout as it stands: the first reason, or none.
pub fn removal_refusal(primary: &Path, path: &Path, presence: &Presence) -> Option<Refusal> {
    if same_directory(primary, path) || path.join(".git").is_dir() {
        return Some(Refusal::Primary);
    }
    if !linked_worktrees(primary)
        .iter()
        .any(|(registered, _)| same_directory(registered, path))
    {
        return Some(Refusal::NotRegistered);
    }
    if record::read(path) == Recorded::Unreadable {
        return Some(Refusal::Unreadable);
    }
    if presence.inside(path) {
        return Some(Refusal::InUse);
    }
    if holds_uncommitted_work(path) {
        return Some(Refusal::UncommittedWork);
    }
    if holds_unbranched_commits(path) {
        return Some(Refusal::UnbranchedCommits);
    }
    None
}

/// Removes the checkout at `path` for the operator, inspecting it again
/// under the repository's write lock first. Its branch is kept, so nothing
/// committed is lost; only content UZE derives and can write again may be
/// left uncommitted in it, which is the one case `--force` is passed for.
pub fn remove(primary: &Path, path: &Path, presence: &Presence) -> Result<(), Refusal> {
    uze_git::locked(primary, uze_git::DEFAULT_WRITE_TIMEOUT, || {
        if let Some(refusal) = removal_refusal(primary, path, presence) {
            return Err(refusal);
        }
        let target = path.to_string_lossy().into_owned();
        let mut args = vec!["worktree", "remove"];
        if is_dirty(path) {
            args.push("--force");
        }
        args.extend(["--", target.as_str()]);
        git(primary, &args)
            .map(|_| ())
            .map_err(|error| Refusal::Failed(error.to_string()))
    })
    .map_err(|error| Refusal::Failed(error.to_string()))?
}

/// Records a checkout somebody else made in the isolation directory as
/// UZE's, so it is treated as any slot from then on — a clean one is free
/// for the next agent at once.
pub fn adopt(primary: &Path, path: &Path) -> Result<(), Refusal> {
    if same_directory(primary, path) {
        return Err(Refusal::Primary);
    }
    let container = primary.join(WORKTREES_DIRECTORY);
    let Some(registered) = isolated_checkouts(primary)
        .into_iter()
        .map(|(registered, _)| registered)
        .find(|registered| same_directory(registered, path))
    else {
        return Err(if path.parent() == Some(container.as_path()) {
            Refusal::NotRegistered
        } else {
            Refusal::OutsideIsolationDirectory
        });
    };
    uze_git::locked(primary, uze_git::DEFAULT_WRITE_TIMEOUT, || {
        match record::read(&registered) {
            Recorded::Ours(_) => return Err(Refusal::AlreadyRecorded),
            Recorded::Unreadable => return Err(Refusal::Unreadable),
            Recorded::Absent => {}
        }
        record::write(&registered, &CheckoutRecord::made_at(&registered)).map_err(Refusal::Failed)
    })
    .map_err(|error| Refusal::Failed(error.to_string()))?
}

pub(super) fn same_directory(left: &Path, right: &Path) -> bool {
    let canonical = |path: &Path| path.canonical().unwrap_or_else(|_| path.to_path_buf());
    canonical(left) == canonical(right)
}

/// Whether `state` means an agent may still be writing.
pub fn is_live(state: &WorkState) -> bool {
    !matches!(
        state,
        WorkState::Integrated | WorkState::Parked | WorkState::Closed
    )
}
