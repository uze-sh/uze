//! What an agent does with its subagents' checkouts: split one off, join
//! its work back, list them.
//!
//! Each answers the agent that asked, identified the way `work name`
//! identifies it — the launch identity its environment carries, inside the
//! checkout that identity owns — and each refusal says which condition
//! held, because it is the agent's only feedback.

use std::path::{Path, PathBuf};
use uze_core::path::Canonical as _;

use uze_core::{Result, UzeError};

use uze_workspace::{
    checkout,
    checkout::{
        Presence,
        record::{self, CheckoutRecord, Recorded},
        subagent::{self, JoinOutcome},
    },
    conversation::{self, Claim},
    landing,
    task::{self, Agent, AgentId, AgentStore, Base, WorkState},
    worktree::{self, WorktreePolicy},
};

use super::{Workspace, tasks::target_of};

/// A checkout an agent gave one of its subagents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubagentCheckout {
    pub topic: String,
    pub path: PathBuf,
    pub branch: String,
    /// Uncommitted changes in it.
    pub dirty: bool,
    /// Commits it holds that its agent's branch lacks.
    pub ahead: usize,
}

/// What splitting produced: the checkout, and what preparing it warned of.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitWork {
    pub path: PathBuf,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JoinedWork {
    Joined {
        commits: usize,
    },
    /// Paused in the subagent's checkout on these files.
    Conflicted {
        checkout: PathBuf,
        paths: Vec<PathBuf>,
    },
}

impl Workspace<'_> {
    /// Gives the calling agent a checkout for a subagent working on `topic`.
    #[tracing::instrument(name = "workspace.split_work", skip_all, fields(agent = %claim.id, topic), err)]
    pub fn split_work(
        &self,
        claim: Claim<'_>,
        topic: &str,
        occupied: &[PathBuf],
    ) -> Result<SplitWork> {
        let topic = valid_topic(topic)?;
        let caller = self.caller(claim)?;
        if !caller.isolated {
            return Err(refused(
                "this agent works in the operator's checkout, on the operator's branch; a \
                 subagent's work joins a branch of the agent's own",
            ));
        }
        let policy = self.policy_of(&caller)?;
        let store = task::load(&self.0.home, &caller.primary)?;
        let parent = parent_checkout(&store, &caller.agent, &caller.primary)?;
        if let Some(existing) = live_child(&store, &caller.agent, topic) {
            return Ok(SplitWork {
                path: child_directory(existing, &caller.primary)?,
                warnings: Vec::new(),
            });
        }
        if subagent::operation_in_progress(&parent.directory) {
            return Err(refused(
                "a rebase or a merge is under way in this checkout; finish it first",
            ));
        }
        let head = checkout::tip_of(&parent.directory, "HEAD");
        let target = target_of(&caller.primary, &policy);
        let acquired = task::locked(&self.0.home, &caller.primary, |store| {
            checkout::reconcile(&caller.primary, store, &target);
            let mut child = Agent::isolated(
                &parent.harness,
                None,
                Base::Ref(parent.branch.clone()),
                head.clone(),
                parent.branch.clone(),
            );
            child.label = topic.to_owned();
            child.parent = Some(caller.agent.clone());
            let isolation = child
                .isolation_mut()
                .expect("an agent built isolated carries its isolation");
            let acquired = checkout::acquire(
                &caller.primary,
                store,
                isolation,
                &head,
                policy.slots,
                &Presence::observe_with(occupied),
            )
            .map_err(|refusal| refused(&refusal.to_string()))?;
            isolation.checkout = Some(acquired.id.clone());
            record::write(
                &caller.primary,
                &acquired.path,
                &CheckoutRecord {
                    parent: Some(caller.agent.clone()),
                    split_at: Some(head.clone()),
                    ..CheckoutRecord::made_at(&acquired.path)
                },
            )
            .map_err(|reason| refused(&reason))?;
            store.upsert(child);
            Ok(acquired)
        })?;
        let consent = self.consent(&caller.primary, &policy);
        let warnings = checkout::materialize(&caller.primary, &acquired.path, &consent);
        Ok(SplitWork {
            path: acquired.path,
            warnings,
        })
    }

    /// Brings the commits of the calling agent's subagent on `topic` onto
    /// the agent's own branch, and gives its checkout back to the pool.
    #[tracing::instrument(name = "workspace.join_work", skip_all, fields(agent = %claim.id, topic), err)]
    pub fn join_work(&self, claim: Claim<'_>, topic: &str) -> Result<JoinedWork> {
        let caller = self.caller(claim)?;
        self.join_child(&caller.primary, &caller.agent, topic)
    }

    /// The operator's join: an unfinished agent's subagent on `topic`, joined
    /// into that agent's kept checkout. A running agent joins its own, so
    /// only an unfinished one is joined from outside.
    #[tracing::instrument(name = "workspace.join_unfinished_work", skip_all, fields(cwd = %cwd.display(), parent, topic), err)]
    pub fn join_unfinished_work(
        &self,
        cwd: &Path,
        parent: &str,
        topic: &str,
    ) -> Result<JoinedWork> {
        let primary = worktree::primary_checkout(cwd)
            .ok_or_else(|| refused("not inside a Git working tree"))?;
        let store = task::load(&self.0.home, &primary)?;
        let agent = store
            .agents
            .iter()
            .find(|agent| agent.id.as_str() == parent)
            .ok_or_else(|| refused("the agent this subagent belongs to is no longer recorded"))?;
        if checkout::is_live(&agent.state) {
            return Err(refused(&format!(
                "{} is still running; it joins its own subagents",
                agent.label
            )));
        }
        if agent.state != WorkState::Shelved {
            return Err(refused(&format!(
                "{} has ended and no longer keeps a checkout to join into",
                agent.label
            )));
        }
        // An unfinished agent keeps no checkout: its work is on its branch
        // and shelf. Resuming it brings its subagents back beside it, and
        // joining them is then its own to do.
        if agent
            .isolation()
            .is_none_or(|isolation| isolation.checkout.is_none())
        {
            return Err(refused(&format!(
                "{} keeps no checkout now; resume it, and it joins its own subagents",
                agent.label
            )));
        }
        let parent = agent.id.clone();
        self.join_child(&primary, &parent, topic)
    }

    fn join_child(
        &self,
        primary: &Path,
        parent_agent: &AgentId,
        topic: &str,
    ) -> Result<JoinedWork> {
        task::locked(&self.0.home, primary, |store| {
            let parent = parent_checkout(store, parent_agent, primary)?;
            let child = own_child(store, parent_agent, topic)?;
            let isolation = child
                .isolation()
                .ok_or_else(|| refused("the subagent's record has no checkout"))?;
            let directory = child_directory(child, primary)?;
            let branch = isolation.branch.clone();
            for checkout in [&parent.directory, &directory] {
                checkout::anchored(primary, checkout).map_err(|reason| refused(&reason))?;
            }
            if checkout::is_dirty(&parent.directory) {
                return Err(refused(
                    "this checkout has uncommitted changes; commit them before joining",
                ));
            }
            if landing::paused_rebase(&directory).is_some() {
                return Err(refused(&format!(
                    "a replay is paused in `{topic}`'s checkout at {}; resolve it there, run \
                     `git rebase --continue`, and join again",
                    directory.display()
                )));
            }
            if checkout::is_dirty(&directory) {
                return Err(refused(&format!(
                    "`{topic}`'s checkout has uncommitted changes; commit them there first"
                )));
            }
            let found = checkout::current_branch(&directory);
            if found.as_deref() != Some(branch.as_str()) {
                return Err(refused(&format!(
                    "`{topic}`'s checkout is on {}, not on its branch `{branch}`; put it back \
                     on `{branch}` first",
                    found.map_or_else(
                        || "a detached HEAD".to_owned(),
                        |found| format!("`{found}`")
                    )
                )));
            }
            if Presence::observe().inside(&directory) {
                return Err(refused(&format!(
                    "something is still working in `{topic}`'s checkout; let the subagent \
                     finish first"
                )));
            }
            let split_at = match record::read(primary, &directory) {
                Recorded::Ours(CheckoutRecord {
                    split_at: Some(split_at),
                    ..
                }) => split_at,
                _ => isolation.base_commit.clone(),
            };
            let child_id = child.id.clone();
            let outcome =
                subagent::join(primary, &parent.directory, &directory, &branch, &split_at)
                    .map_err(|failure| refused(&failure.to_string()))?;
            match outcome {
                JoinOutcome::Conflicted { paths } => Ok(JoinedWork::Conflicted {
                    checkout: directory,
                    paths,
                }),
                JoinOutcome::Joined { commits, split_at } => {
                    let child = store.get_mut(&child_id).expect("the child was found above");
                    if let Some(split_at) = split_at {
                        if let Some(isolation) = child.isolation_mut() {
                            isolation.base_commit = split_at.clone();
                        }
                        let _ = record::write(
                            primary,
                            &directory,
                            &CheckoutRecord {
                                parent: Some(parent_agent.clone()),
                                split_at: Some(split_at),
                                ..CheckoutRecord::made_at(&directory)
                            },
                        );
                    }
                    let into = parent.branch.clone();
                    // Joined, it holds nothing its agent's branch lacks, and
                    // the release records it so.
                    checkout::release(primary, child, &into, &Presence::observe());
                    Ok(JoinedWork::Joined { commits })
                }
            }
        })
    }

    /// The calling agent's subagents' checkouts.
    #[tracing::instrument(name = "workspace.list_work", skip_all, fields(agent = %claim.id), err)]
    pub fn list_work(&self, claim: Claim<'_>) -> Result<Vec<SubagentCheckout>> {
        let caller = self.caller(claim)?;
        let store = task::load(&self.0.home, &caller.primary)?;
        Ok(store
            .agents
            .iter()
            .filter(|agent| is_child_of(agent, &caller.agent))
            .filter(|agent| holds_its_checkout(agent))
            .filter_map(|child| {
                let isolation = child.isolation()?;
                let path = child_directory(child, &caller.primary).ok()?;
                Some(SubagentCheckout {
                    topic: child.label.clone(),
                    dirty: checkout::is_dirty(&path),
                    ahead: checkout::commits_ahead(
                        &caller.primary,
                        &isolation.target,
                        &isolation.branch,
                    ),
                    branch: isolation.branch.clone(),
                    path,
                })
            })
            .collect())
    }

    /// The agent asking, read from its task store alone: a refusal, and a
    /// listing, never reach for Git.
    fn caller(&self, claim: Claim<'_>) -> Result<Caller> {
        let Some(owner) = conversation::owner_of(&self.0.home, claim) else {
            let inside_a_child = conversation::project_of(&self.0.home, claim.cwd)
                .and_then(|project| {
                    let store = task::load(&self.0.home, &project).ok()?;
                    Some(inside_a_child(&store, claim, &project))
                })
                .unwrap_or(false);
            return Err(if inside_a_child {
                refused(
                    "this is a subagent's checkout; its agent splits, joins and lists \
                     subagents from its own",
                )
            } else {
                refused("this process is not an agent UZE launched")
            });
        };
        Ok(Caller {
            primary: owner.project_root,
            agent: owner.agent,
            isolated: owner.isolated,
        })
    }

    fn policy_of(&self, caller: &Caller) -> Result<WorktreePolicy> {
        self.repository_context(&caller.primary)
            .map(|(_, policy)| policy)
            .ok_or_else(|| refused("not inside a Git working tree"))
    }
}

struct Caller {
    primary: PathBuf,
    agent: AgentId,
    isolated: bool,
}

struct ParentCheckout {
    directory: PathBuf,
    branch: String,
    harness: String,
}

fn parent_checkout(store: &AgentStore, agent: &AgentId, primary: &Path) -> Result<ParentCheckout> {
    let not_an_agent = || refused("this process is not an agent UZE launched");
    let parent = store.get(agent).ok_or_else(not_an_agent)?;
    let isolation = parent.isolation().ok_or_else(not_an_agent)?;
    let directory = isolation
        .checkout
        .as_ref()
        .map(|checkout| checkout.directory(primary))
        .ok_or_else(not_an_agent)?;
    Ok(ParentCheckout {
        directory,
        branch: isolation.branch.clone(),
        harness: parent.harness.clone(),
    })
}

fn is_child_of(agent: &Agent, parent: &AgentId) -> bool {
    agent.parent.as_ref() == Some(parent)
}

fn holds_its_checkout(agent: &Agent) -> bool {
    checkout::is_live(&agent.state) || agent.state == WorkState::Shelved
}

fn live_child<'a>(store: &'a AgentStore, parent: &AgentId, topic: &str) -> Option<&'a Agent> {
    store.agents.iter().find(|agent| {
        is_child_of(agent, parent) && agent.label == topic && checkout::is_live(&agent.state)
    })
}

fn own_child<'a>(store: &'a AgentStore, parent: &AgentId, topic: &str) -> Result<&'a Agent> {
    let named = |agent: &&Agent| {
        agent.parent.is_some() && agent.label == topic && holds_its_checkout(agent)
    };
    if let Some(child) = store
        .agents
        .iter()
        .filter(named)
        .find(|agent| is_child_of(agent, parent))
    {
        return Ok(child);
    }
    Err(refused(
        &if store.agents.iter().any(|agent| named(&agent)) {
            format!("`{topic}` is another agent's subagent; only its own agent joins it")
        } else {
            format!("this agent has no subagent working on `{topic}`")
        },
    ))
}

fn child_directory(child: &Agent, primary: &Path) -> Result<PathBuf> {
    child
        .isolation()
        .and_then(|isolation| isolation.checkout.as_ref())
        .map(|checkout| checkout.directory(primary))
        .ok_or_else(|| refused("the subagent's record has no checkout"))
}

fn inside_a_child(store: &AgentStore, claim: Claim<'_>, primary: &Path) -> bool {
    let cwd = claim
        .cwd
        .canonical()
        .unwrap_or_else(|_| claim.cwd.to_path_buf());
    store
        .agents
        .iter()
        .filter(|agent| agent.parent.as_ref().map(AgentId::as_str) == Some(claim.id))
        .filter_map(|child| child_directory(child, primary).ok())
        .any(|directory| cwd.starts_with(directory.canonical().unwrap_or(directory)))
}

/// A topic names a subagent to its agent and labels its row; it is kept to
/// what reads as one word in both places.
fn valid_topic(topic: &str) -> Result<&str> {
    let topic = topic.trim();
    let readable = !topic.is_empty()
        && topic.len() <= 48
        && topic
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character));
    if readable {
        Ok(topic)
    } else {
        Err(refused(
            "a topic is one word of letters, digits, `-`, `_` or `.`, at most 48 characters",
        ))
    }
}

fn refused(reason: &str) -> UzeError {
    UzeError::AgentWork(reason.to_owned())
}

#[cfg(test)]
mod tests;
