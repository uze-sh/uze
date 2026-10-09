//! The workspace service: slots, tasks, delivery, and the read models
//! presentation sees them through.
//!
//! Split out of `services.rs`, which had grown to carry eight capability
//! views plus every read model the largest of them answers with. This is
//! that largest one — the only service with a domain of its own rather
//! than a thin route into `uze-core`, which is why it is the one that
//! became a file.

use std::cell::OnceCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use uze_core::{Result, UzeError, UzeHome, anchor, manifest};

use uze_workspace::{
    checkout, client_layout,
    conversation::{self, Claim},
    landing::{self, Delivered, DeliveryFailure, Forge, Readiness},
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
        self.0
            .integrations
            .iter()
            .find(|candidate| candidate.id() == integration)
            .is_some_and(|integration| {
                uze_workspace::continuity::refresh(&self.0.home, claim, integration.as_ref())
            })
    }

    /// Recent prompts submitted into the agent tabs of `root`'s workspace.
    #[tracing::instrument(name = "workspace.prompt_history", skip_all, fields(root = %root.display(), limit))]
    pub fn prompt_history(&self, root: &Path, limit: usize) -> Vec<prompt_history::PromptEntry> {
        prompt_history::list_for_workspace(&self.0.home, root, limit)
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

    /// Forgets every prompt recorded for `root`'s workspace.
    #[tracing::instrument(name = "workspace.clear_prompt_history", skip_all, fields(root = %root.display()), err)]
    pub fn clear_prompt_history(&self, root: &Path) -> Result<()> {
        prompt_history::clear(&self.0.home, root)
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
    store: AgentStore,
}

impl Repository {
    fn target(&self) -> String {
        target_of(&self.primary, &self.policy)
    }

    fn views(&self) -> Vec<AgentView> {
        task_views(
            &self.primary,
            &self.store,
            self.policy.completion,
            &self.target(),
        )
    }
}

/// Whether an agent may still be at work on a task in `state`: live, and
/// not owned by a delivery in flight.
fn is_agents_turn(state: &WorkState) -> bool {
    checkout::is_live(state) && *state != WorkState::Integrating
}

/// Ends the children of an agent that ended: each holding nothing its
/// agent's branch lacks goes back to the pool, and each holding work is
/// parked — and then so is the agent, whose checkout is the only way that
/// work reaches the target. Returns whether any child was parked.
fn release_children(primary: &Path, store: &mut AgentStore, parent: &str) -> bool {
    let mut parked_a_child = false;
    for child in store.agents.iter_mut() {
        if child.parent.as_ref().map(AgentId::as_str) != Some(parent)
            || !checkout::is_live(&child.state)
        {
            continue;
        }
        let into = child
            .isolation()
            .map(|isolation| isolation.target.clone())
            .unwrap_or_default();
        if checkout::release(primary, child, &into) == checkout::SlotState::Parked {
            parked_a_child = true;
        }
    }
    if parked_a_child && let Some(agent) = task_mut(store, parent) {
        agent.state = WorkState::Parked;
    }
    parked_a_child
}

/// The branch this project delivers into: what it declared, else whatever
/// the primary checkout is on.
pub(super) fn target_of(primary: &Path, policy: &WorktreePolicy) -> String {
    policy
        .target
        .clone()
        .or_else(|| checkout::current_branch(primary))
        .unwrap_or_else(|| "HEAD".to_owned())
}

fn task_mut<'a>(store: &'a mut AgentStore, id: &str) -> Option<&'a mut Agent> {
    store.agent_mut(id)
}

fn task_views(
    primary: &Path,
    store: &AgentStore,
    completion: CompletionBehavior,
    target: &str,
) -> Vec<AgentView> {
    // A property of the repository, not of a row: asked once here rather
    // than once per agent, since every task of one project reaches the
    // same remote. Only where the completion publishes — the word this
    // buys is a word for a request, and a project that opens none has no
    // use for it.
    let forge = if completion == CompletionBehavior::Pr {
        landing::forge(primary)
    } else {
        Forge::default()
    };
    // Every recorded agent is drawn, closed ones included, on every pass:
    // one read of the refs answers what each would have asked Git apart.
    let tips = checkout::BranchTips::read(primary);
    store
        .agents
        .iter()
        .filter_map(|agent| AgentView::from_agent(primary, &tips, agent, completion, target, forge))
        .collect()
}

#[cfg(test)]
mod tests;
