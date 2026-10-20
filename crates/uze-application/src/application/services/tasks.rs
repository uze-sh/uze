//! The workspace service: slots, tasks, delivery, and the read models
//! presentation sees them through.
//!
//! Split out of `services.rs`, which had grown to carry eight capability
//! views plus every read model the largest of them answers with. This is
//! that largest one — the only service with a domain of its own rather
//! than a thin route into `uze-core`, which is why it is the one that
//! became a file.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use uze_core::{Result, UzeError, UzeHome, anchor, integration::IntegrationPort, manifest};

use uze_workspace::{
    checkout, client_layout,
    conversation::{self, Claim},
    declaration::{self, ActionState},
    landing::{self, Delivered, DeliveryFailure, Readiness},
    prompt_history,
    task::{self, Agent, AgentId, AgentStore, Base, Isolation, WorkState},
    worktree::{self, BranchVocabulary, CompletionBehavior, NameRefusal, WorktreePolicy},
};

use super::{AgentIdentity, CommandsAwaitingApproval, Workspace, WorkspaceEntry};

mod delivery;
mod evaluation;
mod naming;
mod occupancy;
mod placement;
mod repository;
mod switch;
mod views;

pub use naming::*;
pub use views::*;

impl Workspace<'_> {
    /// The workspace root a directory belongs to, or the directory itself.
    ///
    /// One repository is one terminal server, and this is the answer both
    /// the server key and the prompt history are keyed on — resolved once,
    /// here, rather than twice at two call sites.
    #[tracing::instrument(name = "workspace.root", skip_all, fields(cwd = %cwd.display()))]
    pub fn root(&self, cwd: &Path) -> PathBuf {
        anchor::anchor_root_or_self(cwd)
    }

    /// The harnesses this installation can recognize, as descriptors.
    #[tracing::instrument(name = "workspace.agent_identities", skip_all)]
    pub fn agent_identities(&self) -> Vec<AgentIdentity> {
        self.0
            .integrations
            .iter()
            .map(|integration| {
                let binary = integration
                    .aliases()
                    .first()
                    .copied()
                    .unwrap_or(integration.id());
                let (launch, continuity_gap) = self.launcher(integration.as_ref(), binary);
                AgentIdentity {
                    binary,
                    integration: integration.id(),
                    display_name: integration.display_name(),
                    launch,
                    continuity_gap,
                    configured: self.is_set_up(integration.as_ref()),
                }
            })
            .collect()
    }

    /// What the workspace needs before it opens. Asks each harness whether
    /// it is on this machine, which the detection cache answers without a
    /// probe after the first time.
    #[tracing::instrument(name = "workspace.entry", skip_all)]
    pub fn entry(&self) -> WorkspaceEntry {
        let installed: Vec<&dyn uze_core::integration::IntegrationPort> = self
            .0
            .integrations
            .iter()
            .map(|integration| integration.as_ref())
            .filter(|integration| self.0.detect_cached(*integration).present)
            .collect();
        if installed.is_empty() {
            return WorkspaceEntry::Choose;
        }
        let to_set_up: Vec<String> = installed
            .into_iter()
            .filter(|integration| !self.is_set_up(*integration))
            .map(|integration| integration.id().to_owned())
            .collect();
        if to_set_up.is_empty() {
            WorkspaceEntry::Ready
        } else {
            WorkspaceEntry::SetUp(to_set_up)
        }
    }

    /// Set up for the workspace: a setup verified the executable, and the
    /// launcher it placed is still there. Read from the record and the
    /// shims directory, never from the detection cache — what the package
    /// manager prepared on its way through a command is a harness that can
    /// receive plugins, not one the workspace can launch.
    fn is_set_up(&self, integration: &dyn uze_core::integration::IntegrationPort) -> bool {
        uze_core::state::provisioning(&self.0.home, integration.id())
            .ok()
            .flatten()
            .is_some_and(|provisioning| {
                provisioning.status == uze_core::provisioning::ProvisionStatus::Verified
            })
            && self.0.runtime_shim_is_active(integration)
    }

    /// What to launch an agent of `integration` by, and what that costs.
    ///
    /// UZE's own launcher is what decides, per launch, whether an agent
    /// resumes its task's conversation or starts one, so naming it here by
    /// path is what makes continuity independent of the operator's `PATH`.
    /// Setup places it, and the workspace runs setup before it opens for
    /// every harness installed and not set up ([`Self::entry`]), so this
    /// is found missing only for a harness whose setup could not place it.
    /// Without it the harness still starts — on its plain name, with no
    /// conversation carried over, and the reason said rather than silently
    /// missing.
    fn launcher(
        &self,
        integration: &dyn uze_core::integration::IntegrationPort,
        binary: &str,
    ) -> (PathBuf, Option<String>) {
        let bare = PathBuf::from(binary);
        if integration.session_continuity() == uze_core::integration::SessionContinuity::Unsupported
        {
            return (
                bare,
                Some("this harness offers no way to continue a conversation".to_owned()),
            );
        }
        let launcher = self.0.home.shim_path(integration.shim_name());
        if launcher.exists() {
            return (launcher, None);
        }
        (
            bare,
            Some(
                "UZE's launcher is not installed for this harness, so its conversation is not \
                 carried over"
                    .to_owned(),
            ),
        )
    }

    /// Writes back which conversation the agent a claim names is actually
    /// in.
    ///
    /// Answers whether anything changed. Runs off whatever thread the
    /// caller gives it — one of these asks a harness about its own
    /// records, which can mean spawning it — and is silent about every
    /// way of having nothing to say.
    #[tracing::instrument(name = "workspace.refresh_conversation", skip_all, fields(integration = %integration, agent = %claim.id, cwd = %claim.cwd.display()))]
    pub fn refresh_conversation(&self, integration: &str, claim: Claim<'_>) -> bool {
        self.integration(integration).is_some_and(|integration| {
            uze_workspace::continuity::refresh(&self.0.home, claim, integration)
        })
    }

    /// The registered integration whose id is `id`.
    pub(super) fn integration(&self, id: &str) -> Option<&dyn IntegrationPort> {
        self.0
            .integrations
            .iter()
            .find(|candidate| candidate.id() == id)
            .map(|found| found.as_ref())
    }

    /// Recent prompts `agent` was given in `root`'s workspace.
    #[tracing::instrument(name = "workspace.prompt_history", skip_all, fields(root = %root.display(), limit))]
    pub fn prompt_history(
        &self,
        root: &Path,
        agent: &str,
        limit: usize,
    ) -> Vec<prompt_history::PromptEntry> {
        prompt_history::list_for_agent(&self.0.home, root, agent, limit)
    }

    /// Records one prompt submitted into an agent tab of `root`'s
    /// workspace. Best-effort by construction: an empty prompt is ignored
    /// rather than refused.
    ///
    /// The span carries the prompt's length, never the prompt. What this
    /// writes is `prompt-history.json`, which [`prompt_history`] opens and
    /// keeps at `0600` on purpose; the journal beside it is world-readable
    /// and is attached to bug reports. A field interpolating the text put
    /// the same bytes in both places, and only one of them was protected.
    #[tracing::instrument(
        name = "workspace.record_prompt",
        skip_all,
        fields(root = %root.display(), bytes = prompt.len()),
        err
    )]
    pub fn record_prompt(
        &self,
        root: &Path,
        origin: &prompt_history::PromptOrigin,
        prompt: &str,
    ) -> Result<()> {
        prompt_history::record(&self.0.home, root, origin, prompt)
    }

    /// Forgets every prompt `agent` was given in `root`'s workspace.
    #[tracing::instrument(name = "workspace.clear_prompt_history", skip_all, fields(root = %root.display()), err)]
    pub fn clear_prompt_history(&self, root: &Path, agent: &str) -> Result<()> {
        prompt_history::clear_agent(&self.0.home, root, agent)
    }

    /// What the TUI was last left looking like, in both of its modes.
    /// Best-effort: unreadable state answers with the defaults rather
    /// than failing.
    #[tracing::instrument(name = "workspace.client_layout", skip_all)]
    pub fn client_layout(&self) -> client_layout::ClientLayout {
        client_layout::load(&self.0.home)
    }

    /// Remembers the TUI's shape for the next run.
    #[tracing::instrument(name = "workspace.save_client_layout", skip_all, err)]
    pub fn save_client_layout(&self, layout: &client_layout::ClientLayout) -> Result<()> {
        client_layout::save(&self.0.home, layout)
    }
}

/// A repository as the task operations see it: its primary checkout, the
/// project's policy, and the recorded tasks.
struct Repository {
    primary: PathBuf,
    policy: WorktreePolicy,
    instructions: Instructions,
    store: AgentStore,
}

/// What the project asks an agent at each state it declares one for.
pub(super) type Instructions = BTreeMap<ActionState, String>;

impl Repository {
    fn target(&self) -> String {
        target_of(&self.primary, &self.policy)
    }

    fn views(&self) -> Vec<AgentView> {
        task_views(
            &self.primary,
            &self.store,
            self.policy.completion,
            &self.instructions,
            &self.target(),
        )
    }
}

/// Whether an agent may still be at work on a task in `state`: live, and
/// not owned by a delivery in flight.
fn is_agents_turn(state: &WorkState) -> bool {
    checkout::is_live(state) && *state != WorkState::Integrating
}

/// Ends the children of an agent that ended, each giving its checkout back
/// with its work kept on its branch and shelf. A child holding work leaves
/// its agent unfinished too, whatever the agent's own branch holds: the
/// agent is the only way that work reaches the target. Returns whether any
/// child ended unfinished.
fn release_children(
    primary: &Path,
    store: &mut AgentStore,
    parent: &str,
    observe: &dyn Fn() -> checkout::Presence,
) -> bool {
    let mut unfinished_child = false;
    for child in store.agents.iter_mut() {
        let holds_a_checkout = child
            .isolation()
            .is_some_and(|isolation| isolation.checkout.is_some());
        if child.parent.as_ref().map(AgentId::as_str) != Some(parent)
            || !(checkout::is_live(&child.state) || holds_a_checkout)
        {
            continue;
        }
        let into = delivers_into(child, "");
        checkout::release(primary, child, &into, observe);
        if child.state == WorkState::Shelved {
            unfinished_child = true;
        }
    }
    if unfinished_child && let Some(agent) = task_mut(store, parent) {
        agent.state = WorkState::Shelved;
    }
    unfinished_child
}

/// Whether nothing is in front of `agent` any more and its checkout is
/// still to be given back: a live agent no pane holds and no tab was
/// launched for, or an ended one still naming a checkout nobody sits in.
/// A delivery in flight owns its agent until it answers, and a subagent's
/// checkout ends with its agent rather than on its own — or, when it was
/// still in use as its agent ended, on a later pass, once its agent has.
fn is_abandoned(
    primary: &Path,
    store: &AgentStore,
    agent: &Agent,
    occupied: &[PathBuf],
    echoed: &[String],
) -> bool {
    let Some(task) = agent.isolation() else {
        return false;
    };
    let its_agent_is_at_work = agent.parent.as_ref().is_some_and(|parent| {
        store
            .agent(parent.as_str())
            .is_some_and(|parent| checkout::is_live(&parent.state))
    });
    if its_agent_is_at_work || agent.state == WorkState::Integrating {
        return false;
    }
    let in_its_slot = landing::slot_path(primary, task)
        .is_some_and(|slot| occupied.iter().any(|pane| pane.starts_with(&slot)));
    let launched_for = echoed.iter().any(|echo| echo == agent.id.as_str());
    if in_its_slot || launched_for {
        return false;
    }
    checkout::is_live(&agent.state) || task.checkout.is_some()
}

/// The branch this project delivers into: what it declared, else whatever
/// the primary checkout is on. `None` for a primary on no branch that
/// declared none; what that means is each caller's to say.
pub(super) fn declared_target(primary: &Path, policy: &WorktreePolicy) -> Option<String> {
    policy
        .target
        .clone()
        .or_else(|| checkout::current_branch(primary))
}

/// [`declared_target`], or `HEAD` where it names no branch.
pub(in crate::application::services) fn target_of(
    primary: &Path,
    policy: &WorktreePolicy,
) -> String {
    declared_target(primary, policy).unwrap_or_else(|| "HEAD".to_owned())
}

/// The branch `agent`'s work is judged against and delivered into: the one
/// recorded when it was isolated — its agent's branch, for a subagent —
/// else the project's.
fn delivers_into(agent: &Agent, project_target: &str) -> String {
    agent.isolation().map_or_else(
        || project_target.to_owned(),
        |isolation| isolation.target.clone(),
    )
}

fn task_mut<'a>(store: &'a mut AgentStore, id: &str) -> Option<&'a mut Agent> {
    store.agent_mut(id)
}

fn task_views(
    primary: &Path,
    store: &AgentStore,
    completion: Option<CompletionBehavior>,
    instructions: &Instructions,
    target: &str,
) -> Vec<AgentView> {
    // Every recorded agent is drawn, closed ones included, on every pass:
    // one read of the refs answers what each would have asked Git apart.
    let tips = checkout::BranchTips::read(primary);
    let root = RootStanding::default();
    store
        .agents
        .iter()
        .map(|agent| {
            AgentView::from_agent(
                primary,
                &tips,
                &root,
                agent,
                completion,
                instructions,
                target,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests;
