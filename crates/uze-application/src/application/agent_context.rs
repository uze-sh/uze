//! How one harness actually receives a project's portable context, at one
//! directory, right now.
//!
//! This is the read model behind the workspace's per-agent support popup
//! and the Harnesses drawer. It exists because the older answer was
//! assembled at the call site out of three unrelated pieces — a
//! project-scoped `Context::inspect` resolved at whatever root the TUI
//! happened to attach to, a machine-scoped `HarnessHealth`, and a
//! `needed` flag that actually meant "some installed package contributed a
//! managed region". A harness could be receiving its context perfectly
//! through the runtime shim while every view reported "not loaded" or "not
//! needed", and the answer changed depending on which directory `uze` was
//! launched from.
//!
//! Two properties fix that, and both are structural rather than
//! conventional: the resolution starts from a directory the *caller* names
//! (an agent pane's own cwd, not a session-wide root), and each portable
//! resource (`AGENTS.md`, `.agents/skills`, `.agents/agents`) is answered
//! independently, naming the mechanism that delivers it.

use std::path::{Path, PathBuf};

use serde::Serialize;
use uze_core::{
    Result, UzeError,
    harness_runtime::RuntimeContext,
    integration::{ContextDelivery, IntegrationPort},
    project_context::{self, AgentsDirectoryResource},
};

use super::{ContextMechanism, RuntimeProjection, UzeApplication, services::Workspace};

/// The mechanism actually carrying one portable resource into one harness.
/// Every variant names a mechanism or a specific reason there is none —
/// deliberately never a bare boolean, because "false" was what previously
/// let "the project has no `.agents/`" and "this harness cannot receive
/// one" render as the same misleading row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "delivery", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResourceDelivery {
    /// The project does not carry this resource at all. Not a gap: there
    /// is nothing to deliver.
    AbsentFromProject,
    /// The harness's own binary reads it straight out of the project.
    Native,
    /// UZE's runtime PATH shim projects it into the session at launch,
    /// without writing anything into the project.
    Projected,
    /// The project carries it, but nothing currently delivers it here.
    Undelivered(UndeliveredReason),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "reason", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UndeliveredReason {
    /// The harness is not installed on this machine.
    HarnessAbsent,
    /// A real harness binary resolves ahead of UZE's shim on this
    /// process's `PATH`, so a launch from here bypasses the projection.
    /// An environment fact, not a defect in the harness or the project.
    ShimShadowed,
    /// The harness reads `file`, its own instructions file, in place of
    /// `AGENTS.md`.
    ShadowedBy { file: PathBuf },
    /// No delivery strategy exists for this harness and this resource.
    Unsupported,
}

/// One harness's context delivery, resolved against one directory.
#[derive(Clone, Debug, Serialize)]
pub struct AgentContextStatus {
    pub integration: String,
    pub display_name: String,
    /// Whether the harness binary is on this machine at all.
    pub present: bool,
    /// The project root every field below was resolved against — one rule
    /// (`uze_core::project_context`), so a caller can show the reader
    /// exactly which project the answer is about.
    pub root: PathBuf,
    /// Delivery of the shared `AGENTS.md`.
    pub instructions: ResourceDelivery,
    /// Delivery of the project's `.agents/skills`.
    pub project_skills: ResourceDelivery,
    /// Delivery of the project's `.agents/agents`.
    pub project_agents: ResourceDelivery,
}

impl Workspace<'_> {
    /// Resolves how every registered harness receives `cwd`'s project
    /// context. `cwd` is a real working directory — an agent pane's own,
    /// typically — never a pre-resolved root: resolving it here is the
    /// point, so two callers looking at the same directory can never
    /// disagree about which project it belongs to.
    #[tracing::instrument(name = "workspace.agent_context", skip_all, fields(cwd = %cwd.display()))]
    pub fn agent_context(&self, cwd: &Path) -> Vec<AgentContextStatus> {
        let context = project_context::resolve(cwd);
        self.0
            .integrations
            .iter()
            .map(|integration| self.resolve_agent_context(integration.as_ref(), cwd, &context))
            .collect()
    }

    /// The single-harness slice of [`Workspace::agent_context`] — what
    /// an agent pane running one known harness needs, without paying for
    /// the others.
    #[tracing::instrument(name = "workspace.agent_context_for", skip_all, fields(integration_id = %integration_id, cwd = %cwd.display()), err)]
    pub fn agent_context_for(
        &self,
        integration_id: &str,
        cwd: &Path,
    ) -> Result<AgentContextStatus> {
        let integration = self.0.integration_named(integration_id).ok_or_else(|| {
            UzeError::UnknownPackage(format!("harness `{integration_id}` not found"))
        })?;
        let context = project_context::resolve(cwd);
        Ok(self.resolve_agent_context(integration, cwd, &context))
    }

    fn resolve_agent_context(
        &self,
        integration: &dyn IntegrationPort,
        cwd: &Path,
        context: &project_context::ProjectContext,
    ) -> AgentContextStatus {
        let present = self.0.detect_cached(integration).present;
        let projection = self.0.runtime_projection_at(integration, cwd);
        AgentContextStatus {
            integration: integration.id().to_owned(),
            display_name: integration.display_name().to_owned(),
            present,
            root: context.root.clone(),
            instructions: instruction_delivery(integration, context, present),
            project_skills: project_resource_delivery(
                integration,
                context,
                AgentsDirectoryResource::Skills,
                present,
                projection,
            ),
            project_agents: project_resource_delivery(
                integration,
                context,
                AgentsDirectoryResource::Agents,
                present,
                projection,
            ),
        }
    }
}

impl UzeApplication {
    /// The runtime projection a launch of `integration` from `cwd` gets.
    ///
    /// Asked at the caller's `cwd`, not at a resolved root: this is the same
    /// question the shim answers when it actually execs the harness from
    /// that directory, so a status can never claim a projection the next
    /// real launch would not perform.
    pub(super) fn runtime_projection_at(
        &self,
        integration: &dyn IntegrationPort,
        cwd: &Path,
    ) -> RuntimeProjection {
        match RuntimeProjection::of(integration, self.runtime_shim_is_active(integration)) {
            RuntimeProjection::Active
                if !integration.runtime_contribution_would_activate(&RuntimeContext {
                    cwd,
                    home: &self.home,
                }) =>
            {
                RuntimeProjection::Inactive
            }
            projection => projection,
        }
    }
}

fn instruction_delivery(
    integration: &dyn IntegrationPort,
    context: &project_context::ProjectContext,
    present: bool,
) -> ResourceDelivery {
    if context.agents_md.is_none() {
        return ResourceDelivery::AbsentFromProject;
    }
    if !present {
        return ResourceDelivery::Undelivered(UndeliveredReason::HarnessAbsent);
    }
    let ContextDelivery::Native { shadowed_by, .. } = integration.context_delivery() else {
        return ResourceDelivery::Undelivered(UndeliveredReason::Unsupported);
    };
    match project_context::shadowing_file(&context.root, shadowed_by) {
        Some(file) => ResourceDelivery::Undelivered(UndeliveredReason::ShadowedBy { file }),
        None => ResourceDelivery::Native,
    }
}

fn project_resource_delivery(
    integration: &dyn IntegrationPort,
    context: &project_context::ProjectContext,
    resource: AgentsDirectoryResource,
    present: bool,
    projection: RuntimeProjection,
) -> ResourceDelivery {
    if context.resource_directory(resource).is_none() {
        return ResourceDelivery::AbsentFromProject;
    }
    if !present {
        return ResourceDelivery::Undelivered(UndeliveredReason::HarnessAbsent);
    }
    match ContextMechanism::for_project_resource(integration, resource, projection) {
        ContextMechanism::Native => ResourceDelivery::Native,
        ContextMechanism::RuntimeShim => ResourceDelivery::Projected,
        ContextMechanism::ShimShadowed => {
            ResourceDelivery::Undelivered(UndeliveredReason::ShimShadowed)
        }
        ContextMechanism::Unsupported => {
            ResourceDelivery::Undelivered(UndeliveredReason::Unsupported)
        }
    }
}
