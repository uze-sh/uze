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
    // Plumbing, with binary content in full: porcelain `diff` follows the
    // operator's `diff.noprefix`, external drivers and text conversions,
    // any of which makes a patch `apply` cannot read back.
    let patch = crate::git::read(
        primary,
        &[
            "diff-index",
            "-p",
            "--binary",
            "--full-index",
            "--no-renames",
            "HEAD",
        ],
    )
    .map_err(|error| error.to_string())?
    .successful()?;
    if !patch.trim().is_empty() {
        crate::git::write_with_stdin(slot, &["apply", "--"], &patch)
            .map_err(|error| error.to_string())?
            .successful()
            .map_err(|error| {
                format!("the changes could not be carried into the checkout: {error}")
            })?;
    }
    for relative in crate::git::read(
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
    pub shelves: Vec<String>,
}

/// The removals that cannot lose work, as one critical section: the
/// directory of a free slot unused past the pool's idle age, its branch
/// kept; a branch whose every commit is in the target and that no shelf
/// stands on; and a shelf whose every change the target already carries.
///
/// Slots first, so a trimmed slot's integrated branch is no longer checked
/// out by the time branches are pruned, and goes in the same pass. Taking
/// the write lock once around all three is what keeps a branch from being
/// pruned in the moment a concurrent acquisition is creating it.
#[tracing::instrument(name = "checkout.collect", level = "debug", skip_all)]
pub fn collect(
    primary: &Path,
    store: &mut AgentStore,
    target: &str,
    pool: Pool,
    presence: &Presence,
) -> Collected {
    uze_git::locked(primary, uze_git::DEFAULT_WRITE_TIMEOUT, || {
        let slots = trim_free_slots(primary, store, pool, presence);
        // Read once for both: a shelf collected here leaves its branch
        // protected for one more pass, which costs nothing but that.
        let refs = Refs::read(primary);
        let shelves = collect_shelves_among(&refs, primary, store, target);
        let branches = prune_integrated_branches_among(&refs, primary, store, target);
        // A task whose branch just went for being in the target is settled
        // here, while the branch's fate is still known: read later, a
        // branch that is gone says nothing about where its commits went.
        for agent in store.agents.iter_mut().filter(|agent| {
            agent.state == WorkState::Shelved
                && agent
                    .isolation()
                    .is_some_and(|isolation| branches.contains(&isolation.branch))
        }) {
            let into = agent
                .isolation()
                .map_or_else(|| target.to_owned(), |isolation| isolation.target.clone());
            settle_without_checkout(primary, &into, agent);
        }
        let forgotten = forget_finished(&refs, &branches, store, pool.idle, SystemTime::now());
        if !forgotten.is_empty() {
            tracing::info!(
                count = forgotten.len(),
                "tasks with nothing left were forgotten"
            );
        }
        Collected {
            branches,
            slots,
            shelves,
        }
    })
    .unwrap_or_default()
}

/// Removes from the store every task that ended more than `idle` ago and
/// left nothing anywhere: no checkout, no branch, no shelf, and no
/// subagent naming it. Every pass walks every record, so a store that only
/// grew made each pass slower with every agent the project ever ran, and a
/// record of nothing is not history anyone can act on. Returns the tasks
/// forgotten.
pub fn forget_finished(
    refs: &Refs,
    pruned: &[String],
    store: &mut AgentStore,
    idle: Duration,
    now: SystemTime,
) -> Vec<AgentId> {
    let now = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let parents: std::collections::BTreeSet<AgentId> = store
        .agents
        .iter()
        .filter_map(|agent| agent.parent.clone())
        .collect();
    let finished = |agent: &Agent| {
        let ended_long_ago = agent
            .ended_at_unix
            .is_some_and(|ended| now.saturating_sub(ended) >= idle.as_secs());
        let holds_nothing = match agent.isolation() {
            None => true,
            Some(isolation) => {
                matches!(agent.state, WorkState::Closed | WorkState::Integrated)
                    && isolation.checkout.is_none()
                    && (!refs.tips.contains(&isolation.branch)
                        || pruned.contains(&isolation.branch))
                    && refs.shelf_of(agent.id.as_str()).is_none()
            }
        };
        ended_long_ago && holds_nothing && !parents.contains(&agent.id)
    };
    let forgotten: Vec<AgentId> = store
        .agents
        .iter()
        .filter(|agent| finished(agent))
        .map(|agent| agent.id.clone())
        .collect();
    store.agents.retain(|agent| !forgotten.contains(&agent.id));
    forgotten
}

/// Removes every shelf whose work the target already has, unless a live
/// task is about to restore it. Returns the tasks whose shelf went.
pub fn collect_shelves(primary: &Path, store: &AgentStore, target: &str) -> Vec<String> {
    collect_shelves_among(&Refs::read(primary), primary, store, target)
}

fn collect_shelves_among(
    refs: &Refs,
    primary: &Path,
    store: &AgentStore,
    target: &str,
) -> Vec<String> {
    if refs.shelves.is_empty() || !refs.resolves(primary, target) {
        return Vec::new();
    }
    refs.shelves
        .iter()
        .filter(|found| {
            !store
                .agent(&found.task)
                .is_some_and(|agent| is_live(&agent.state))
        })
        .filter(|found| {
            let into = store
                .agent(&found.task)
                .and_then(Agent::isolation)
                .map_or(target, |isolation| isolation.target.as_str());
            refs.carries(primary, &found.commit, into)
        })
        .filter(|found| shelf::drop_shelf(primary, &found.task, &found.commit).is_ok())
        .map(|found| found.task.clone())
        .collect()
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
    prune_integrated_branches_among(&Refs::read(primary), primary, store, target)
}

fn prune_integrated_branches_among(
    refs: &Refs,
    primary: &Path,
    store: &AgentStore,
    target: &str,
) -> Vec<String> {
    if !refs.resolves(primary, target) {
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
    // A branch a shelf stands on is the shelf's base: pruned, the shelf
    // could only be restored by recreating it.
    let mut shelved: Vec<String> = refs
        .shelves
        .iter()
        .map(|found| found.branch.clone())
        .collect();
    // So is the branch a subagent's work is to be joined into: the agent's
    // own, kept while any child of it still holds work.
    shelved.extend(
        store
            .isolated()
            .filter(|child| child.parent.is_some())
            .filter_map(|child| {
                let isolation = child.isolation()?;
                holds_work_among(refs, primary, child, &isolation.target)
                    .then(|| isolation.target.clone())
            }),
    );
    let tips = &refs.tips;
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
            || shelved.contains(&branch)
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
        if is_integrated_among(tips, primary, into, &branch)
            && git(primary, &["branch", "-D", "--", &branch]).is_ok()
        {
            removed.push(branch);
        }
    }
    removed
}

/// Removes the directory of every free slot unused past the pool's idle
/// age, keeping each one's branch, and keeps the index of every other free
/// slot fresh. Returns the slots removed.
pub fn trim_free_slots(
    primary: &Path,
    store: &AgentStore,
    pool: Pool,
    presence: &Presence,
) -> Vec<CheckoutId> {
    let now = SystemTime::now();
    let mut removed = Vec::new();
    for slot in slots(primary, store, presence)
        .into_iter()
        .filter(|slot| slot.state == SlotState::Free)
    {
        let idle = now
            .duration_since(modified_at(&slot.path))
            .unwrap_or_default();
        if idle < pool.idle {
            // A kept slot is what the next placement switches, and one
            // whose files were written in the same second as its index
            // reads as possibly changed until the index is written again:
            // every status hashes every file, and the switch does too.
            // Written here, where nobody waits for it, instead of by the
            // placement that would otherwise pay for it; and once, since an
            // index written well after the slot was last stamped has no
            // file of that second left to doubt.
            if index_may_be_racy(&slot.path) {
                let _ = git(&slot.path, &["update-index", "-q", "--refresh"]);
            }
            continue;
        }
        if set_aside(primary, &slot.path) {
            removed.push(slot.id);
        }
    }
    removed
}

fn index_may_be_racy(slot: &Path) -> bool {
    let Ok(git_dir) = uze_git::repository::git_dir(slot) else {
        return true;
    };
    modified_at(&git_dir.join("index")) <= modified_at(slot) + Duration::from_secs(1)
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

/// What releasing an agent's checkout did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Released {
    /// The checkout went back to the pool; `shelved` when its work had to
    /// be kept on a shelf first. The task holds no checkout any more.
    Freed { shelved: bool },
    /// Something keeps the checkout as it is; the task is ended and still
    /// holds it, so the next pass tries again.
    Pinned(Pin),
    /// Somebody is working in the checkout: nothing was done, and the task
    /// was not ended.
    InUse,
}

/// Ends `task` because nothing is in front of its checkout any more, and
/// gives the checkout back to the pool with its work kept: uncommitted
/// changes on a shelf, commits on the branch.
///
/// The only path that resets a slot, and so the only one that asks the
/// machine rather than a client: under the repository's write lock, right
/// before anything changes, a process working inside — an agent another
/// client launched, one still running its `setup`, a shell somebody left —
/// leaves the checkout and the task exactly as they were.
pub fn release(primary: &Path, agent: &mut Agent, target: &str, presence: &Presence) -> Released {
    let id = agent.id.as_str().to_owned();
    let label = agent.label.clone();
    let Some(isolation) = agent.isolation() else {
        return Released::Freed { shelved: false };
    };
    let branch = isolation.branch.clone();
    let checkout = isolation.checkout.clone();
    let directory = checkout
        .as_ref()
        .map(|checkout| checkout.directory(primary))
        .filter(|path| path.is_dir());
    let Some(directory) = directory else {
        let isolation = agent.isolation_mut().expect("checked isolated above");
        isolation.last_checkout = isolation.checkout.take();
        settle_without_checkout(primary, target, agent);
        return Released::Freed { shelved: false };
    };
    let outcome = uze_git::locked(primary, uze_git::DEFAULT_WRITE_TIMEOUT, || {
        shelve_and_reset(primary, &directory, presence, &id, &label, &branch)
    })
    .unwrap_or(Released::InUse);
    match &outcome {
        Released::InUse => {}
        Released::Pinned(_) => {
            if is_live(&agent.state) {
                agent.state = WorkState::Shelved;
            }
        }
        Released::Freed { .. } => {
            let isolation = agent.isolation_mut().expect("checked isolated above");
            isolation.last_checkout = isolation.checkout.take();
            settle_without_checkout(primary, target, agent);
        }
    }
    outcome
}

/// The release's critical section, under the repository's write lock:
/// nobody inside, no operation paused, the work shelved and the shelf
/// still what the checkout holds, then the checkout reset and stamped.
fn shelve_and_reset(
    primary: &Path,
    directory: &Path,
    presence: &Presence,
    task: &str,
    label: &str,
    branch: &str,
) -> Released {
    if presence.inside(directory) || Presence::observe().inside(directory) {
        return Released::InUse;
    }
    if let Some(operation) = paused_operation(directory) {
        return Released::Pinned(Pin::Paused(operation));
    }
    let shelving = match shelf::shelve(directory, task, label, branch) {
        Ok(shelving) => shelving,
        Err(refusal) => return pin_with(primary, directory, refusal.to_string()),
    };
    let shelved = matches!(shelving, shelf::Shelving::Kept(_));
    // The tree is read again against what was kept: anything written
    // between the shelf and now is somebody at work, and is not reset.
    // The shelf goes back to what it was, or a task still at work would
    // carry a snapshot it has since moved past as unfinished work.
    if let shelf::Shelving::Kept(kept) = &shelving
        && kept.still_matches(directory) != Ok(true)
    {
        let _ = kept.take_back(primary, task);
        return Released::InUse;
    }
    if git(
        directory,
        &["switch", "--quiet", "--detach", "--discard-changes", "HEAD"],
    )
    .is_err()
        || git(directory, &["clean", "--quiet", "-fd"]).is_err()
    {
        return Released::Pinned(Pin::Unshelved(
            "its checkout could not be reset after its work was kept".to_owned(),
        ));
    }
    let _ = record::write(primary, directory, &CheckoutRecord::made_at(directory));
    stamp(directory);
    Released::Freed { shelved }
}

/// Keeps `directory` as it is for `reason`, written on its record so the
/// pool and the operator's list both say why.
fn pin_with(primary: &Path, directory: &Path, reason: String) -> Released {
    let mut pinned = CheckoutRecord::made_at(directory);
    if let Recorded::Ours(recorded) = record::read(primary, directory) {
        pinned = recorded;
    }
    pinned.pinned = Some(reason.clone());
    let _ = record::write(primary, directory, &pinned);
    Released::Pinned(Pin::Unshelved(reason))
}

/// Whether `agent` still holds work nobody delivered: commits on its branch
/// the target lacks, or a shelf with changes the target lacks. The one
/// answer release, settlement, pruning and delivery all ask.
pub fn holds_work(primary: &Path, agent: &Agent, target: &str) -> bool {
    holds_work_among(&Refs::read(primary), primary, agent, target)
}

/// [`holds_work`] for a pass that asks it of many tasks: answered from
/// `refs`, read once, instead of by a Git process per task per question —
/// which grew every pass with every task the project ever recorded.
pub fn holds_work_among(refs: &Refs, primary: &Path, agent: &Agent, target: &str) -> bool {
    let Some(isolation) = agent.isolation() else {
        return false;
    };
    (refs.tips.contains(&isolation.branch)
        && !is_integrated_among(&refs.tips, primary, target, &isolation.branch))
        || refs
            .shelf_of(agent.id.as_str())
            .is_some_and(|commit| !refs.carries(primary, commit, target))
}

/// The repository's branches and shelves as one pass reads them: two
/// processes, however many tasks the pass then asks about.
pub struct Refs {
    pub tips: BranchTips,
    pub shelves: Vec<shelf::Shelf>,
}

impl Refs {
    pub fn read(primary: &Path) -> Self {
        Self {
            tips: BranchTips::read(primary),
            shelves: shelf::list(primary),
        }
    }

    /// The commit `task`'s shelf points at, if it has one.
    pub fn shelf_of(&self, task: &str) -> Option<&str> {
        self.shelves
            .iter()
            .find(|shelf| shelf.task == task)
            .map(|shelf| shelf.commit.as_str())
    }

    /// Whether `target` names a commit: a local branch these tips hold, or
    /// anything else Git resolves.
    pub fn resolves(&self, primary: &Path, target: &str) -> bool {
        self.tips.local(target).is_some() || !tip_of(primary, target).is_empty()
    }

    /// Whether `target` already carries everything `shelf` holds, with the
    /// target resolved from these tips where they name it.
    pub fn carries(&self, primary: &Path, shelf: &str, target: &str) -> bool {
        match self.tips.local(target) {
            Some(commit) => shelf::is_in_commit(primary, shelf, commit),
            None => shelf::is_in_target(primary, shelf, target),
        }
    }
}

/// Records how a task that holds no checkout ended, from what its branch
/// and its shelf hold: shelved while either holds work; integrated when it
/// had work that is in the target now; closed when it never had any.
pub fn settle_without_checkout(primary: &Path, target: &str, agent: &mut Agent) {
    let had_work = agent.state == WorkState::Shelved
        || agent.state == WorkState::Integrated
        || agent.isolation().is_some_and(|isolation| {
            let tip = tip_of(primary, &isolation.branch);
            !tip.is_empty() && tip != isolation.base_commit
        });
    agent.state = if holds_work(primary, agent, target) {
        WorkState::Shelved
    } else if had_work {
        WorkState::Integrated
    } else {
        WorkState::Closed
    };
}

/// Gives back a slot a placement took but could not use, without touching
/// any ref: unlike [`discard`], the task's shelf and branch are its own and
/// stay.
pub fn untake(primary: &Path, slot: &Path) {
    let _ = uze_git::locked(primary, uze_git::DEFAULT_WRITE_TIMEOUT, || {
        let _ = git(
            slot,
            &["switch", "--quiet", "--detach", "--discard-changes", "HEAD"],
        );
        let _ = git(slot, &["clean", "--quiet", "-fd"]);
        let _ = record::write(primary, slot, &CheckoutRecord::made_at(slot));
    });
}

/// The one removal that loses work, and therefore the one only an operator
/// takes on a named task: the checkout directory, forced, the branch and
/// the shelf.
pub fn discard(primary: &Path, task: &str, isolation: &Isolation) -> Result<(), String> {
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
        if let Some(commit) = shelf::shelf_of(primary, task) {
            shelf::drop_shelf(primary, task, &commit)?;
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
    if record::read(primary, path) == Recorded::Unreadable {
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
        match record::read(primary, &registered) {
            Recorded::Ours(_) => return Err(Refusal::AlreadyRecorded),
            Recorded::Unreadable => return Err(Refusal::Unreadable),
            Recorded::Absent => {}
        }
        record::write(primary, &registered, &CheckoutRecord::made_at(&registered))
            .map_err(Refusal::Failed)
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
        WorkState::Integrated | WorkState::Shelved | WorkState::Closed
    )
}
