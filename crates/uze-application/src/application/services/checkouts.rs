//! Every checkout of a project, read for the facts the agents holding
//! them are shown with.
//!
//! The slots are UZE's, but they share the repository with checkouts a
//! person made, ones an agent made by hand, and ones a harness keeps for
//! its own isolation. Each is classified by its owner so a reader can
//! tell a subagent's checkout, waiting to be joined, from the rest.

use std::path::{Path, PathBuf};
use std::time::SystemTime;
use uze_core::integration::WorktreePlace;

use uze_workspace::{
    checkout::{self, AccountedCheckout, CheckoutId, Owner},
    task::{self, AgentStore, WorkState},
    worktree,
};

use super::{Workspace, tasks};

impl Workspace<'_> {
    /// Every worktree of the repository `cwd` belongs to, with who it
    /// belongs to and the facts it is shown with. Walks every checkout to
    /// measure it, so it is a background read.
    #[tracing::instrument(name = "workspace.checkouts", skip_all, fields(cwd = %cwd.display()))]
    pub fn checkouts(&self, cwd: &Path) -> Option<CheckoutsView> {
        let (primary, target) = self.project_of(cwd)?;
        let store = task::load(&self.0.home, &primary).unwrap_or_default();
        let accounted = checkout::account(&primary, &self.harness_worktree_places());
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
            .map(|(found, (bytes, last_changed))| CheckoutView {
                path: found.path.clone(),
                owner: self.owner_view(&found.owner, &store, &found.path),
                task: slot_holder(&store, &found.path).map(|agent| agent.id.to_string()),
                branch: found.branch.clone(),
                dirty: checkout::holds_uncommitted_work(&found.path),
                ahead: ahead_of(&judged_against(&store, &found.path, &target), found),
                last_changed,
                bytes,
            })
            .collect();
        Some(CheckoutsView { primary, checkouts })
    }

    /// Removes the free slot at `slot` in the repository `cwd` belongs to,
    /// its branch kept, once the operator has answered for it. Judged
    /// again under the locks, so a slot taken, written in or entered since
    /// it was shown is refused with the reason rather than removed.
    /// `occupied` is every checkout a pane of the caller stands in.
    #[tracing::instrument(name = "workspace.remove_free_checkout", skip_all, fields(slot = %slot.display()))]
    pub fn remove_free_checkout(
        &self,
        cwd: &Path,
        slot: &Path,
        occupied: &[PathBuf],
    ) -> std::result::Result<(), String> {
        let primary = worktree::primary_checkout(cwd)
            .ok_or_else(|| "it is not in a repository any more".to_owned())?;
        let removed = task::locked(&self.0.home, &primary, |store| {
            Ok(checkout::remove_free_slot(&primary, store, slot, &|| {
                checkout::Presence::observe_with(occupied)
            }))
        })
        .map_err(|error| error.to_string())?;
        checkout::empty_trash(&primary);
        removed.map(|_| ()).map_err(|kept| kept.to_string())
    }

    /// The repository `cwd` belongs to, and what it delivers onto. A
    /// manifest that does not parse answers nothing rather than judging
    /// work against a branch the project may not deliver into.
    fn project_of(&self, cwd: &Path) -> Option<(PathBuf, String)> {
        let primary = worktree::primary_checkout(cwd)?;
        let policy = self.policy(&primary).ok()?;
        let target = tasks::target_of(&primary, &policy);
        Some((primary, target))
    }

    /// Where each harness this installation knows keeps worktrees of its
    /// own, as data core classifies by: core never names a harness.
    fn harness_worktree_places(&self) -> Vec<(&'static str, WorktreePlace)> {
        self.0
            .integrations
            .iter()
            .flat_map(|integration| {
                integration
                    .own_worktree_places()
                    .into_iter()
                    .map(|place| (integration.id(), place))
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
                    // What the join itself asks: an unfinished agent that
                    // still keeps a checkout to join into. One released
                    // without it was offered here and refused when pressed.
                    joinable: child.is_some()
                        && agent.is_some_and(|agent| {
                            agent.state == WorkState::Shelved
                                && agent
                                    .isolation()
                                    .is_some_and(|isolation| isolation.checkout.is_some())
                        }),
                }
            }
            Owner::Harness { harness } => CheckoutOwner::Harness {
                harness: self.harness_name(harness),
            },
            Owner::Unreadable => CheckoutOwner::Unreadable,
            Owner::Operator => CheckoutOwner::Operator,
        }
    }
}

/// Commits the checkout's `HEAD` has that the target lacks; zero once its
/// work is in the target, even when a squash left them counted.
fn ahead_of(target: &str, found: &AccountedCheckout) -> usize {
    if checkout::is_integrated(&found.path, target, "HEAD") {
        0
    } else {
        checkout::commits_ahead(&found.path, target, "HEAD")
    }
}

/// The branch a checkout's commits are counted ahead of: the one its
/// holder delivers into — its agent's branch, for a subagent, whose work
/// counted against the project's target read as undelivered for as long
/// as the squash it reached the target in matched none of its patches.
fn judged_against(store: &AgentStore, path: &Path, project_target: &str) -> String {
    slot_holder(store, path)
        .and_then(task::Agent::isolation)
        .map_or_else(
            || project_target.to_owned(),
            |isolation| isolation.target.clone(),
        )
}

fn slot_holder<'a>(store: &'a AgentStore, path: &Path) -> Option<&'a task::Agent> {
    let name = path.file_name()?.to_str()?;
    store.slot_owner(&CheckoutId::adopted(name))
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

/// Every worktree of one project.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CheckoutsView {
    pub primary: PathBuf,
    pub checkouts: Vec<CheckoutView>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckoutView {
    pub path: PathBuf,
    pub owner: CheckoutOwner,
    /// The task the store records in this checkout, by id: how a list of
    /// kept work finds the checkout each task left behind.
    pub task: Option<String>,
    /// `None` for a detached `HEAD`.
    pub branch: Option<String>,
    /// Uncommitted work, as a slot is shelved or freed by: content UZE
    /// derives and can write again is not work.
    pub dirty: bool,
    /// Commits its `HEAD` has that the target lacks; zero once it is in
    /// the target, even when a squash left them counted.
    pub ahead: usize,
    pub last_changed: Option<SystemTime>,
    pub bytes: u64,
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

#[cfg(test)]
mod tests;
