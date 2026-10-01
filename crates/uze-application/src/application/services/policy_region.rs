//! The workspace's region of a project's `AGENTS.md`: the text that tells an
//! agent the workspace launched how its checkout, its branch and its
//! delivery work.
//!
//! The workspace keeps it in step itself, while it runs, and only in the
//! primary checkout: in a slot the file belongs to the agent's branch, and a
//! region written there would become a change the agent could commit and
//! deliver. The package manager never writes, removes or counts it.

use std::path::{Path, PathBuf};

use serde::Serialize;

use uze_core::{
    Result,
    integration::AttachmentState,
    project_context::{AGENTS_MD_FILE_NAME, AgentsMdGuard},
    text_region,
};
use uze_workspace::{
    declaration,
    worktree::{self, CompletionBehavior, WorktreePolicy},
};

use super::Workspace;
use crate::application::{ManagedRegionStatus, managed_region};

/// The workspace's region as it stands in the primary checkout's
/// `AGENTS.md`, beside the declaration it renders.
#[derive(Clone, Debug, Serialize)]
pub struct PolicyRegionView {
    /// The primary checkout whose `AGENTS.md` carries the region.
    pub primary: PathBuf,
    /// The declared completion behavior, or `None` when the project
    /// declares no policy.
    pub completion: Option<CompletionBehavior>,
    /// Whether the file carries exactly what the declaration renders.
    pub in_step: bool,
    /// The region's state when one is declared: `Drifted` is a region
    /// somebody edited by hand, which is reported and never rewritten.
    pub state: Option<AttachmentState>,
}

impl Workspace<'_> {
    /// Brings the policy region in the primary checkout `cwd` belongs to in
    /// step with what the project declares: written when a policy is
    /// declared, superseded versions removed, and every version removed when
    /// none is. `None` outside a Git repository. Writes nothing when the file
    /// already says the right thing, and never rewrites a region edited by
    /// hand.
    #[tracing::instrument(name = "workspace.sync_policy_region", skip_all, fields(cwd = %cwd.display()), err)]
    pub fn sync_policy_region(&self, cwd: &Path) -> Result<Option<ManagedRegionStatus>> {
        let Some(primary) = worktree::primary_checkout(cwd) else {
            return Ok(None);
        };
        let desired = desired_region(&primary)?;
        let agents_md = primary.join(AGENTS_MD_FILE_NAME);
        // The workspace keeps its section of a file the project has; it
        // never creates one. A project without `AGENTS.md` has given its
        // agents no instructions yet, and a tracked file appearing in the
        // operator's checkout because a screen opened is not the workspace's
        // call. `uze install` creates it, as it always has.
        if !agents_md.is_file() {
            return Ok(Some(ManagedRegionStatus {
                file: agents_md,
                state: AttachmentState::Missing,
                reason: "the project has no AGENTS.md; `uze install` creates it".to_owned(),
                removed_superseded: Vec::new(),
                blocked_superseded: Vec::new(),
            }));
        }
        if managed_region::in_step(&agents_md, WorktreePolicy::owns_region, &desired) {
            let (state, reason) = match &desired {
                Some((identity, content)) => {
                    let inspection = text_region::inspect(&agents_md, identity, content);
                    (inspection.state, inspection.reason)
                }
                None => (AttachmentState::Missing, "no policy is declared".to_owned()),
            };
            return Ok(Some(ManagedRegionStatus {
                file: agents_md,
                state,
                reason,
                removed_superseded: Vec::new(),
                blocked_superseded: Vec::new(),
            }));
        }
        let _guard = AgentsMdGuard::acquire(&self.0.home, &primary)?;
        Ok(Some(managed_region::converge(
            &agents_md,
            WorktreePolicy::owns_region,
            desired,
        )))
    }

    /// Brings the region in step before an agent starts in the primary
    /// checkout, and says what an agent about to read the file should have
    /// had: nothing when it is in step, a warning when it could not be.
    /// Never a reason not to launch.
    pub(super) fn policy_region_warnings(&self, primary: &Path) -> Vec<String> {
        match self.sync_policy_region(primary) {
            Ok(Some(region)) if region.state == AttachmentState::Drifted => vec![format!(
                "AGENTS.md's workspace section was edited by hand, so it was left as it is: {}",
                region.reason
            )],
            Ok(Some(region)) if region.state == AttachmentState::Blocked => vec![format!(
                "AGENTS.md's workspace section could not be brought up to date: {}",
                region.reason
            )],
            Ok(_) => Vec::new(),
            Err(error) => vec![format!(
                "AGENTS.md's workspace section could not be brought up to date: {error}"
            )],
        }
    }

    /// The region's standing, read without writing, for the client to say
    /// when the file is behind the declaration or edited by hand.
    #[tracing::instrument(name = "workspace.policy_region", skip_all, fields(cwd = %cwd.display()))]
    pub fn policy_region(&self, cwd: &Path) -> Option<PolicyRegionView> {
        let primary = worktree::primary_checkout(cwd)?;
        let declared = declaration::declared(&primary).ok()?;
        let desired = declared
            .as_ref()
            .map(|policy| (policy.region_identity(), policy.instructions()));
        let agents_md = primary.join(AGENTS_MD_FILE_NAME);
        let state = desired
            .as_ref()
            .map(|(identity, content)| text_region::inspect(&agents_md, identity, content).state);
        Some(PolicyRegionView {
            in_step: managed_region::in_step(&agents_md, WorktreePolicy::owns_region, &desired)
                && state.is_none_or(|state| state == AttachmentState::Matched),
            completion: declared.map(|policy| policy.completion),
            state,
            primary,
        })
    }
}

/// The region the declaration renders, or `None` when nothing is declared.
fn desired_region(primary: &Path) -> Result<managed_region::Desired> {
    Ok(declaration::declared(primary)?
        .map(|policy| (policy.region_identity(), policy.instructions())))
}

impl Workspace<'_> {
    /// The names a harness the workspace launches through a shim runs
    /// under: the shim's own name and the aliases its real binary may carry.
    /// What a client compares a pane's foreground process against to tell a
    /// harness that bypassed the workspace's shim — so only a harness whose
    /// shim exists is named: without one, starting on its plain name is the
    /// only way it could have started, and nothing was bypassed.
    #[tracing::instrument(name = "workspace.launcher_names", skip_all)]
    pub fn launcher_names(&self) -> Vec<String> {
        self.0
            .integrations
            .iter()
            .filter(|integration| integration.supports_runtime_integration())
            .filter(|integration| self.0.runtime_shim_is_active(integration.as_ref()))
            .flat_map(|integration| {
                std::iter::once(integration.shim_name())
                    .chain(integration.runtime_executable_aliases().iter().copied())
            })
            .map(str::to_owned)
            .collect()
    }
}
