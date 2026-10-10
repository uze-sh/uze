//! Every checkout of a project, as the operator sees and acts on it.
//!
//! The slots are UZE's, but they share the repository with checkouts a
//! person made, ones an agent made by hand, and ones a harness keeps for
//! its own isolation. This is the one surface that lists all of them with
//! their facts, and the only one that removes a checkout UZE did not make —
//! each time because the operator chose it.

use std::path::{Path, PathBuf};
use std::time::SystemTime;
use uze_core::path::Canonical as _;

use uze_workspace::{
    checkout::{self, AccountedCheckout, CheckoutId, Owner, Presence, Refusal},
    task::{self, AgentStore, WorkState},
    worktree::{self, WORKTREES_DIRECTORY},
};

use super::Workspace;

impl Workspace<'_> {
    /// Every worktree of the repository `cwd` belongs to, grouped by who
    /// it belongs to, with the facts that decide what may be done to it.
    /// Walks every checkout to measure it, so it is a background read.
    /// `occupied` names directories a pane is known to sit in, before its
    /// shell reaches the process table.
    #[tracing::instrument(name = "workspace.checkouts", skip_all, fields(cwd = %cwd.display()))]
    pub fn checkouts(&self, cwd: &Path, occupied: &[PathBuf]) -> Option<CheckoutsView> {
        self.checkouts_seen(cwd, &Presence::observe_with(occupied))
    }

    /// Records a checkout somebody else made in the isolation directory as
    /// UZE's. Answers whether it is free for the next agent at once.
    #[tracing::instrument(name = "workspace.adopt_checkout", skip_all, fields(path = %path.display()))]
    pub fn adopt_checkout(
        &self,
        cwd: &Path,
        path: &Path,
    ) -> std::result::Result<AdoptedCheckout, CheckoutRefusal> {
        let (primary, target) = self.project_of(cwd)?;
        checkout::adopt(&primary, path).map_err(CheckoutRefusal)?;
        Ok(AdoptedCheckout {
            free: !checkout::holds_uncommitted_work(path)
                && checkout::is_integrated(path, &target, "HEAD"),
        })
    }

    /// Removes one checkout's directory, keeping its branch. Inspects it
    /// first, and refuses with the reason while it holds work, somebody is
    /// working in it, or it is not the operator's to remove.
    #[tracing::instrument(name = "workspace.remove_checkout", skip_all, fields(path = %path.display()))]
    pub fn remove_checkout(
        &self,
        cwd: &Path,
        path: &Path,
        occupied: &[PathBuf],
    ) -> std::result::Result<RemovedCheckout, CheckoutRefusal> {
        self.remove_checkout_seen(cwd, path, &Presence::observe_with(occupied))
    }

    /// Removes every checkout of the operator's that is clean, unused and
    /// whose work is already in the target, and says what it kept and why.
    /// A harness's own isolation is left to its harness, and UZE's own
    /// slots to the pool.
    #[tracing::instrument(name = "workspace.clean_up_checkouts", skip_all, fields(cwd = %cwd.display()))]
    pub fn clean_up_checkouts(&self, cwd: &Path, occupied: &[PathBuf]) -> CleanUp {
        self.clean_up_seen(cwd, &Presence::observe_with(occupied))
    }

    pub(super) fn checkouts_seen(&self, cwd: &Path, presence: &Presence) -> Option<CheckoutsView> {
        let (primary, target) = self.project_of(cwd).ok()?;
        let store = task::load(&self.0.home, &primary).unwrap_or_default();
        let accounted = checkout::account(&primary, &self.harness_worktree_dirs());
        let slots = checkout::slots(&primary, &store, presence);
        let pinned: Vec<(PathBuf, String)> = slots
            .iter()
            .filter_map(|slot| match &slot.state {
                checkout::SlotState::Pinned { reason } => {
                    Some((slot.path.clone(), reason.to_string()))
                }
                _ => None,
            })
            .collect();
        let free: Vec<&Path> = slots
            .iter()
            .filter(|slot| slot.state == checkout::SlotState::Free)
            .map(|slot| slot.path.as_path())
            .collect();
        let others: Vec<PathBuf> = accounted.iter().map(|found| found.path.clone()).collect();
        // Measured side by side: a walk is one system call per file, and a
        // project's checkouts hold hundreds of thousands of them between
        // their builds. One after another, the view waited for the sum.
        let measured: Vec<(u64, Option<SystemTime>)> = std::thread::scope(|scope| {
            let walks: Vec<_> = accounted
                .iter()
                .map(|found| scope.spawn(|| measure(&found.path, &others)))
                .collect();
            walks
                .into_iter()
                .map(|walk| walk.join().unwrap_or_default())
                .collect()
        });
        let checkouts: Vec<CheckoutView> = accounted
            .iter()
            .zip(measured)
            .map(|(found, (bytes, last_changed))| {
                let free = free.iter().any(|path| same(path, &found.path));
                let facts = Facts::read(&target, found, presence, free);
                CheckoutView {
                    name: display_name(&primary, &found.path),
                    path: found.path.clone(),
                    owner: self.owner_view(&found.owner, &store, &found.path),
                    task: slot_holder(&store, &found.path).map(|agent| agent.id.to_string()),
                    branch: found.branch.clone(),
                    dirty: facts.dirty,
                    in_target: facts.in_target,
                    held_by_a_branch: facts.held_by_a_branch,
                    ahead: facts.ahead,
                    in_use: facts.in_use,
                    last_changed,
                    bytes,
                    adoptable: is_adoptable(&primary, found),
                    removal_refusal: self
                        .removal_refusal(&primary, found, &store, presence)
                        .map(|refusal| refusal.to_string()),
                    kept_because: pinned
                        .iter()
                        .find(|(path, _)| same(path, &found.path))
                        .map(|(_, reason)| reason.clone()),
                }
            })
            .collect();
        Some(CheckoutsView {
            total_bytes: checkouts.iter().map(|checkout| checkout.bytes).sum(),
            primary,
            target,
            checkouts,
        })
    }

    pub(super) fn remove_checkout_seen(
        &self,
        cwd: &Path,
        path: &Path,
        presence: &Presence,
    ) -> std::result::Result<RemovedCheckout, CheckoutRefusal> {
        let (primary, _) = self.project_of(cwd)?;
        let store = task::load(&self.0.home, &primary).unwrap_or_default();
        let accounted = checkout::account(&primary, &self.harness_worktree_dirs());
        if let Some(found) = accounted.iter().find(|found| same(&found.path, path))
            && let Some(refusal) = self.removal_refusal(&primary, found, &store, presence)
        {
            return Err(CheckoutRefusal(refusal));
        }
        let others: Vec<PathBuf> = accounted.iter().map(|found| found.path.clone()).collect();
        let (bytes, _) = measure(path, &others);
        checkout::remove(&primary, path, presence).map_err(CheckoutRefusal)?;
        Ok(RemovedCheckout {
            name: display_name(&primary, path),
            bytes,
        })
    }

    pub(super) fn clean_up_seen(&self, cwd: &Path, presence: &Presence) -> CleanUp {
        let mut clean_up = CleanUp::default();
        let Ok((primary, target)) = self.project_of(cwd) else {
            return clean_up;
        };
        let accounted = checkout::account(&primary, &self.harness_worktree_dirs());
        let others: Vec<PathBuf> = accounted.iter().map(|found| found.path.clone()).collect();
        for found in &accounted {
            let name = display_name(&primary, &found.path);
            match &found.owner {
                Owner::Harness { .. } => clean_up.left_to_harness.push(name),
                Owner::Operator => {
                    let reason = checkout::removal_refusal(&primary, &found.path, presence)
                        .map(|refusal| refusal.to_string())
                        .or_else(|| {
                            (!checkout::is_integrated(&found.path, &target, "HEAD"))
                                .then(|| format!("its work is not in {target}"))
                        });
                    if let Some(reason) = reason {
                        clean_up.kept.push(KeptCheckout { name, reason });
                        continue;
                    }
                    let (bytes, _) = measure(&found.path, &others);
                    match checkout::remove(&primary, &found.path, presence) {
                        Ok(()) => clean_up.removed.push(RemovedCheckout { name, bytes }),
                        Err(refusal) => clean_up.kept.push(KeptCheckout {
                            name,
                            reason: refusal.to_string(),
                        }),
                    }
                }
                Owner::Agent | Owner::Subagent { .. } | Owner::Unreadable => {}
            }
        }
        clean_up
    }

    /// The repository `cwd` belongs to, and what it delivers onto.
    fn project_of(&self, cwd: &Path) -> std::result::Result<(PathBuf, String), CheckoutRefusal> {
        let primary = worktree::primary_checkout(cwd).ok_or_else(|| {
            CheckoutRefusal(Refusal::Failed(format!(
                "{} is not in a Git repository",
                cwd.display()
            )))
        })?;
        let target = self
            .policy(&primary)
            .ok()
            .and_then(|policy| policy.target)
            .or_else(|| checkout::current_branch(&primary))
            .unwrap_or_else(|| "HEAD".to_owned());
        Ok((primary, target))
    }

    /// Where each harness this installation knows keeps worktrees of its
    /// own, as data core classifies by: core never names a harness.
    fn harness_worktree_dirs(&self) -> Vec<(&'static str, &'static str)> {
        self.0
            .integrations
            .iter()
            .flat_map(|integration| {
                integration
                    .own_worktree_dirs()
                    .iter()
                    .map(|directory| (integration.id(), *directory))
            })
            .collect()
    }

    fn harness_name(&self, id: &str) -> String {
        self.0
            .integrations
            .iter()
            .find(|integration| integration.id() == id)
            .map_or_else(
                || id.to_owned(),
                |integration| integration.display_name().to_owned(),
            )
    }

    fn owner_view(&self, owner: &Owner, store: &AgentStore, path: &Path) -> CheckoutOwner {
        match owner {
            Owner::Agent => {
                let holder = slot_holder(store, path);
                CheckoutOwner::Agent {
                    holder: holder.map(|agent| agent.label.clone()),
                    live: holder.is_some_and(|agent| agent.is_live()),
                }
            }
            Owner::Subagent { parent } => {
                let agent = store.get(parent);
                let child = slot_holder(store, path).filter(|child| {
                    child.parent.as_ref() == Some(parent)
                        && (checkout::is_live(&child.state) || child.state == WorkState::Shelved)
                });
                CheckoutOwner::Subagent {
                    parent: agent.map_or_else(|| parent.to_string(), |agent| agent.label.clone()),
                    parent_id: parent.to_string(),
                    topic: child.map(|child| child.label.clone()),
                    joinable: child.is_some()
                        && agent.is_some_and(|agent| agent.state == WorkState::Shelved),
                }
            }
            Owner::Harness { harness } => CheckoutOwner::Harness {
                harness: self.harness_name(harness),
            },
            Owner::Unreadable => CheckoutOwner::Unreadable,
            Owner::Operator => CheckoutOwner::Operator,
        }
    }

    fn removal_refusal(
        &self,
        primary: &Path,
        found: &AccountedCheckout,
        store: &AgentStore,
        presence: &Presence,
    ) -> Option<Refusal> {
        match &found.owner {
            Owner::Harness { harness } => {
                return Some(Refusal::LeftToHarness(self.harness_name(harness)));
            }
            Owner::Agent | Owner::Subagent { .. } => {
                if let Some(holder) =
                    slot_holder(store, &found.path).filter(|agent| agent.is_live())
                {
                    return Some(Refusal::HeldBy(holder.label.clone()));
                }
            }
            Owner::Unreadable | Owner::Operator => {}
        }
        checkout::removal_refusal(primary, &found.path, presence)
    }
}

/// What a checkout holds, read once per row.
struct Facts {
    dirty: bool,
    in_target: bool,
    held_by_a_branch: bool,
    ahead: usize,
    in_use: bool,
}

impl Facts {
    /// `free` is the pool's own answer for one of UZE's slots: a free slot
    /// detached at commits its target lacks is detached where a branch
    /// keeps them, which is what releasing it left behind.
    fn read(target: &str, found: &AccountedCheckout, presence: &Presence, free: bool) -> Self {
        let in_target = checkout::is_integrated(&found.path, target, "HEAD");
        Self {
            dirty: checkout::holds_uncommitted_work(&found.path),
            in_target,
            held_by_a_branch: found.branch.is_none() && !in_target && free,
            ahead: if in_target {
                0
            } else {
                checkout::commits_ahead(&found.path, target, "HEAD")
            },
            in_use: presence.inside(&found.path),
        }
    }
}

fn slot_holder<'a>(store: &'a AgentStore, path: &Path) -> Option<&'a task::Agent> {
    let name = path.file_name()?.to_str()?;
    store.slot_owner(&CheckoutId::adopted(name))
}

fn is_adoptable(primary: &Path, found: &AccountedCheckout) -> bool {
    found.owner == Owner::Operator
        && found
            .path
            .parent()
            .is_some_and(|parent| same(parent, &primary.join(WORKTREES_DIRECTORY)))
}

fn display_name(primary: &Path, path: &Path) -> String {
    path.strip_prefix(primary).map_or_else(
        |_| path.display().to_string(),
        |relative| relative.display().to_string(),
    )
}

fn same(left: &Path, right: &Path) -> bool {
    let canonical = |path: &Path| path.canonical().unwrap_or_else(|_| path.to_path_buf());
    canonical(left) == canonical(right)
}

/// The bytes `root` takes on disk and the newest change inside it. Symbolic
/// links are counted as themselves and never followed, and a checkout
/// nested inside this one — a harness's own, kept under a slot — is left
/// to its own row, so no byte is counted twice.
fn measure(root: &Path, others: &[PathBuf]) -> (u64, Option<SystemTime>) {
    let mut bytes = 0;
    let mut newest: Option<SystemTime> = None;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            // `DirEntry::metadata` never follows a link, like
            // `symlink_metadata`, without building the path to ask it.
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            bytes += allocated(&metadata);
            if let Ok(modified) = metadata.modified() {
                newest = Some(newest.map_or(modified, |seen| seen.max(modified)));
            }
            if metadata.is_dir() {
                let path = entry.path();
                if !others.contains(&path) {
                    pending.push(path);
                }
            }
        }
    }
    (bytes, newest)
}

fn allocated(metadata: &std::fs::Metadata) -> u64 {
    uze_platform::fs::allocated_size(metadata)
}

/// Every worktree of one project, as the operator's checkouts view reads it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CheckoutsView {
    pub primary: PathBuf,
    /// What "in the target" is measured against.
    pub target: String,
    pub checkouts: Vec<CheckoutView>,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckoutView {
    pub path: PathBuf,
    /// The path relative to the project, or whole when it lies elsewhere.
    pub name: String,
    pub owner: CheckoutOwner,
    /// The task the store records in this checkout, by id: how a list of
    /// kept work finds the checkout each task left behind.
    pub task: Option<String>,
    /// `None` for a detached `HEAD`.
    pub branch: Option<String>,
    /// Uncommitted work, as a slot is shelved or freed by: content UZE
    /// derives and can write again is not work.
    pub dirty: bool,
    /// Everything its `HEAD` carries is already in the target.
    pub in_target: bool,
    /// Detached at a commit some branch reaches: what its `HEAD` carries
    /// is kept on that branch, not here — a slot released after its agent
    /// committed, ready for the next.
    pub held_by_a_branch: bool,
    /// Commits its `HEAD` has that the target lacks; zero once it is in
    /// the target, even when a squash left them counted.
    pub ahead: usize,
    /// A live process is working inside it.
    pub in_use: bool,
    pub last_changed: Option<SystemTime>,
    pub bytes: u64,
    /// Somebody else's checkout in the isolation directory, which the
    /// operator may record as UZE's.
    pub adoptable: bool,
    /// Why removing it would be refused as it stands now; `None` when it
    /// may be removed. Removal inspects again before it acts.
    pub removal_refusal: Option<String>,
    /// Why UZE keeps one of its own checkouts as it is rather than handing
    /// it to the next agent, when nobody is assigned to it: a paused
    /// operation, somebody working inside, or work a shelf could not take.
    pub kept_because: Option<String>,
}

/// Who a checkout belongs to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckoutOwner {
    /// A slot UZE made; `holder` is the agent the task store says last
    /// held it, and `live` whether that agent is still working there.
    Agent { holder: Option<String>, live: bool },
    /// A checkout UZE made for one of `parent`'s subagents.
    Subagent {
        /// The agent's label, as the operator knows it.
        parent: String,
        /// The agent's identity, which an operator's join names it by.
        parent_id: String,
        /// The subagent's topic, while it still holds this checkout.
        topic: Option<String>,
        /// Its agent ended unfinished, so the operator may join it there.
        joinable: bool,
    },
    /// A harness's own isolation, left to that harness.
    Harness { harness: String },
    /// Everyone else's: a person's, or one an agent made by hand.
    Operator,
    /// UZE's, with a record this build cannot read.
    Unreadable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdoptedCheckout {
    /// Clean and level with the target: the next agent may be placed there
    /// at once, and its directory reset for it.
    pub free: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovedCheckout {
    pub name: String,
    /// What removing it gave back.
    pub bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeptCheckout {
    pub name: String,
    pub reason: String,
}

/// What one clean-up removed, and what it left and why.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CleanUp {
    pub removed: Vec<RemovedCheckout>,
    pub kept: Vec<KeptCheckout>,
    pub left_to_harness: Vec<String>,
}

impl CleanUp {
    pub fn freed(&self) -> u64 {
        self.removed.iter().map(|removed| removed.bytes).sum()
    }
}

/// Why the operator's adoption or removal of a checkout was refused, said
/// in words.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckoutRefusal(Refusal);

impl std::fmt::Display for CheckoutRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for CheckoutRefusal {}

#[cfg(test)]
mod tests;
