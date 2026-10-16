//! What happens to a checkout over its life: carried changes, collection, pruning, release, discard, removal and adoption.

use super::*;

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
    let untracked = crate::git::read(
        primary,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )
    .map_err(|error| error.to_string())?
    .successful()?;
    // One file that cannot travel leaves the rest to travel anyway, and the
    // managed region is still kept behind: an error here is a warning on a
    // checkout that already exists, never a reason to stop half-way.
    let mut left_behind: Vec<String> = untracked
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .filter_map(|relative| carry_untracked(primary, slot, relative).err())
        .collect();
    if let Err(reason) = keep_derived_instructions_behind(primary, slot) {
        left_behind.push(reason);
    }
    if left_behind.is_empty() {
        Ok(())
    } else {
        Err(left_behind.join("; "))
    }
}

/// Copies one untracked entry `ls-files` listed. Git lists a repository
/// nested in the tree as its directory, with a trailing `/`: its history is
/// not a change of this one, so it stays where it is. A link is made again
/// as the same link rather than followed, since what it points at is not
/// part of the change and may be a directory.
fn carry_untracked(primary: &Path, slot: &Path, relative: &str) -> Result<(), String> {
    if let Some(repository) = relative.strip_suffix('/') {
        return Err(format!(
            "repository `{repository}` is not carried into the checkout"
        ));
    }
    let from = primary.join(relative);
    let to = slot.join(relative);
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("the checkout could not take `{relative}`: {error}"))?;
    }
    let is_link = std::fs::symlink_metadata(&from)
        .map_err(|error| format!("`{relative}` could not be read: {error}"))?
        .file_type()
        .is_symlink();
    let copied = if is_link {
        std::fs::read_link(&from).and_then(|target| uze_platform::fs::symlink(&target, &to))
    } else {
        std::fs::copy(&from, &to).map(|_| ())
    };
    copied.map_err(|error| format!("`{relative}` could not be copied: {error}"))
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
        // Within the slot: a file committed as a link may name anywhere.
        Committed::Text(text) => {
            crate::persistence::write_atomic_within(slot, &in_slot, text.as_bytes())
                .map_err(|error| format!("`{AGENTS_MD_FILE_NAME}` could not be restored: {error}"))
        }
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
    /// The agents whose records went, whose conversations go with them.
    pub forgotten: Vec<AgentId>,
}

/// The removals that cannot lose work, as one critical section: the
/// directory of a free slot unused past the pool's idle age or last held
/// by a subagent, its branch kept; a branch whose every commit is in the target and that no shelf
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
        let forgotten = forget_finished(
            &refs,
            &branches,
            store,
            crate::task::RECORD_KEPT,
            SystemTime::now(),
        );
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
            forgotten,
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
pub(crate) fn forget_finished(
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
    let mut forgotten = Vec::new();
    store.agents.retain(|agent| {
        let forget = finished(agent);
        if forget {
            forgotten.push(agent.id.clone());
        }
        !forget
    });
    forgotten
}

/// [`prune_integrated_branches_among`] on refs read now, outside a
/// collection.
#[cfg(test)]
pub(super) fn prune_integrated_branches(
    primary: &Path,
    store: &AgentStore,
    target: &str,
) -> Vec<String> {
    prune_integrated_branches_among(&Refs::read(primary), primary, store, target)
}

/// Removes every shelf whose work the target already has, unless a live
/// task is about to restore it. Returns the tasks whose shelf went.
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
            let isolation = store.agent(&found.task).and_then(Agent::isolation);
            let into = isolation.map_or(target, |isolation| isolation.target.as_str());
            let branch = isolation.map(|isolation| isolation.branch.as_str());
            refs.carries(primary, &found.commit, into, branch)
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
    // could only be restored by recreating it. Named by the task the shelf
    // is for as well as by the shelf itself, which recorded the name the
    // branch had when it was cut and knows nothing of a rename since.
    let mut shelved: Vec<String> = refs
        .shelves
        .iter()
        .flat_map(|found| {
            let renamed = store
                .agent(&found.task)
                .and_then(Agent::isolation)
                .map(|isolation| isolation.branch.clone());
            std::iter::once(found.branch.clone()).chain(renamed)
        })
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
/// age, or last held by a subagent, keeping each one's branch, and keeps
/// the index of every other free slot fresh. Returns the slots removed.
pub(crate) fn trim_free_slots(
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
        if idle < pool.idle && !last_held_by_a_subagent(store, &slot.id) {
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

/// Whether the agents that last held `slot` were all subagents. A
/// subagent's checkout lasts as long as its work: kept for the idle age, a
/// burst of them would size the pool at the burst for days, each slot
/// carrying its own build output, while the pool exists for the agents an
/// operator opens one after another.
fn last_held_by_a_subagent(store: &AgentStore, slot: &CheckoutId) -> bool {
    let mut holders = store
        .agents
        .iter()
        .filter(|agent| {
            agent
                .isolation()
                .is_some_and(|isolation| isolation.last_checkout.as_ref() == Some(slot))
        })
        .peekable();
    holders.peek().is_some() && holders.all(|agent| agent.parent.is_some())
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
const TRASH_DIRECTORY: &str = ".trash";

pub(super) fn trash_directory(primary: &Path) -> PathBuf {
    primary.join(WORKTREES_DIRECTORY).join(TRASH_DIRECTORY)
}

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
    let trash = trash_directory(primary);
    let destination = trash.join(slot_name(slot));
    let moved = fs::create_dir_all(&trash).is_ok()
        && !destination.exists()
        && fs::rename(slot, &destination).is_ok();
    if moved {
        record::settle_moves(primary);
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
    let trash = trash_directory(primary);
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
pub fn release(
    primary: &Path,
    agent: &mut Agent,
    target: &str,
    observe: &dyn Fn() -> Presence,
) -> Released {
    let outcome = release_checkout(primary, agent, target, observe);
    if outcome != Released::InUse {
        agent.end();
    }
    outcome
}

/// [`release`] short of recording the ending.
fn release_checkout(
    primary: &Path,
    agent: &mut Agent,
    target: &str,
    observe: &dyn Fn() -> Presence,
) -> Released {
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
        shelve_and_reset(primary, &directory, observe, &id, &label, &branch, target)
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
///
/// Reset to the target's tip, not left where the branch ended: a free slot
/// is what the next placement switches from, and the target is where it
/// switches to; and a slot left on a branch's last commit sat on a tip no
/// branch reached once that branch was pruned. The branch's commits are
/// still its own, and a detached commit worth keeping is on the shelf.
fn shelve_and_reset(
    primary: &Path,
    directory: &Path,
    observe: &dyn Fn() -> Presence,
    task: &str,
    label: &str,
    branch: &str,
    target: &str,
) -> Released {
    if observe().inside(directory) {
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
    // A check that could not be made is not somebody at work: answered
    // `InUse`, it was asked again on every retry, forever. The work stays
    // where it is and the operator is told why.
    if let shelf::Shelving::Kept(kept) = &shelving {
        match kept.still_matches(directory) {
            Ok(true) => {}
            Ok(false) => {
                take_back(primary, task, kept);
                return Released::InUse;
            }
            Err(reason) => {
                take_back(primary, task, kept);
                return pin_with(
                    primary,
                    directory,
                    format!("its work could not be checked after it was kept: {reason}"),
                );
            }
        }
    }
    // By the target's name, which Git resolves in the same process; a
    // target this clone cannot resolve leaves the slot where it stands.
    let detached = detach(directory, target).is_ok() || detach(directory, "HEAD").is_ok();
    if !detached || git(directory, &["clean", "--quiet", "-fd"]).is_err() {
        if let shelf::Shelving::Kept(kept) = &shelving {
            take_back(primary, task, kept);
        }
        return pin_with(
            primary,
            directory,
            "its checkout could not be reset after its work was kept".to_owned(),
        );
    }
    let _ = record::write(primary, directory, &CheckoutRecord::made_at(directory));
    stamp(directory);
    Released::Freed { shelved }
}

/// Detaches `directory` at `commit`, discarding what the tree holds.
fn detach(directory: &Path, commit: &str) -> Result<(), AcquireError> {
    git(
        directory,
        &[
            "switch",
            "--quiet",
            "--detach",
            "--discard-changes",
            "--",
            commit,
        ],
    )
    .map(|_| ())
}

/// Takes a shelf back after the checkout turned out not to be resettable:
/// the work is still in the checkout, and a shelf of it beside would read
/// as unfinished work twice.
fn take_back(primary: &Path, task: &str, kept: &shelf::Kept) {
    if let Err(reason) = kept.take_back(primary, task) {
        tracing::warn!(task, %reason, "a shelf could not be taken back");
    }
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
pub(crate) fn holds_work_among(refs: &Refs, primary: &Path, agent: &Agent, target: &str) -> bool {
    let Some(isolation) = agent.isolation() else {
        return false;
    };
    (refs.tips.contains(&isolation.branch)
        && !is_integrated_among(&refs.tips, primary, target, &isolation.branch))
        || refs
            .shelf_of(agent.id.as_str())
            .is_some_and(|commit| !refs.carries(primary, commit, target, Some(&isolation.branch)))
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
    /// target resolved from these tips at its [`target_reach`] for the
    /// branch the shelf was cut beside, where they name it.
    pub fn carries(&self, primary: &Path, shelf: &str, target: &str, branch: Option<&str>) -> bool {
        let branch = branch.and_then(|branch| self.tips.local(branch));
        match self.tips.target_reach(primary, target, branch) {
            Some(commit) => shelf::is_in_commit(primary, shelf, &commit),
            None => shelf::is_in_target(primary, shelf, target),
        }
    }
}

/// Records how a task that holds no checkout ended, from what its branch
/// and its shelf hold: shelved while either holds work; integrated when it
/// had work that is in the target now; closed when it never had any.
pub(crate) fn settle_without_checkout(primary: &Path, target: &str, agent: &mut Agent) {
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
        let reset = git(
            slot,
            &["switch", "--quiet", "--detach", "--discard-changes", "HEAD"],
        )
        .is_ok()
            && git(slot, &["clean", "--quiet", "-fd"]).is_ok();
        // Left holding what a failed resume put back, it is not free: it
        // is kept, and says why, rather than handed to the next agent.
        if reset {
            let _ = record::write(primary, slot, &CheckoutRecord::made_at(slot));
        } else {
            pin_with(
                primary,
                slot,
                "it could not be reset after a resume that failed".to_owned(),
            );
        }
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

#[cfg(test)]
pub(super) fn same_directory(left: &Path, right: &Path) -> bool {
    use uze_core::path::Canonical as _;
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
