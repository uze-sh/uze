//! How one harness actually receives a project's portable context, at one
//! directory, right now.
//!
//! This is the read model behind the workspace's per-agent support popup
//! and the Harnesses drawer. It exists because the older answer was
//! assembled at the call site out of unrelated pieces — a project-scoped
//! `Context::inspect` resolved at whatever root the TUI happened to attach
//! to, and a machine-scoped `HarnessHealth`. A harness could be receiving `AGENTS.md` perfectly
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
    integration::IntegrationPort,
    project_context::{self, AgentsDirectoryResource},
};

use super::{ContextMechanism, RuntimeProjection, UzeApplication, services::Context};

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
    /// What `uze setup` delivered into the harness (the rules engine, a
    /// plugin) hands it over, in every session of the harness.
    SetupDelivered,
    /// The project carries it, but nothing currently delivers it here.
    Undelivered(UndeliveredReason),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "reason", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UndeliveredReason {
    /// The harness is not installed on this machine.
    HarnessAbsent,
    /// The workspace would carry it through a shim `uze setup` has not
    /// created yet.
    ShimMissing,
    /// What `uze setup` delivers would hand it over, but setup has not
    /// delivered it to this harness yet.
    SetupMissing,
    /// What `uze setup` delivers would hand it over, but a file UZE did not
    /// write holds its path.
    Blocked,
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
    /// Delivery of the project's `.agents/rules`.
    pub project_rules: ResourceDelivery,
}

impl Context<'_> {
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

    /// The single-harness slice of [`Context::agent_context`] — what
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
            project_rules: rules_delivery(
                integration,
                context,
                present,
                self.0.rules_engine_delivered(integration),
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
    delivered_by(ContextMechanism::for_instructions(integration))
}

/// The project's rules, read where they live: the checkout root's
/// `.agents/rules` and `.agents/rules.json`, which every harness and the
/// engine read.
fn rules_delivery(
    integration: &dyn IntegrationPort,
    context: &project_context::ProjectContext,
    present: bool,
    engine_delivered: bool,
) -> ResourceDelivery {
    let root =
        uze_core::rules::checkout_root(&context.root).unwrap_or_else(|_| context.root.clone());
    if !uze_core::rules::has_rules(&root) {
        return ResourceDelivery::AbsentFromProject;
    }
    if !present {
        return ResourceDelivery::Undelivered(UndeliveredReason::HarnessAbsent);
    }
    delivered_by(ContextMechanism::for_rules(integration, engine_delivered))
}

/// What a resource the project carries gets from `mechanism`.
fn delivered_by(mechanism: ContextMechanism) -> ResourceDelivery {
    match mechanism {
        ContextMechanism::Native => ResourceDelivery::Native,
        ContextMechanism::RuntimeShim => ResourceDelivery::Projected,
        ContextMechanism::SetupDelivered => ResourceDelivery::SetupDelivered,
        ContextMechanism::ShimMissing => {
            ResourceDelivery::Undelivered(UndeliveredReason::ShimMissing)
        }
        ContextMechanism::SetupMissing => {
            ResourceDelivery::Undelivered(UndeliveredReason::SetupMissing)
        }
        ContextMechanism::Blocked => ResourceDelivery::Undelivered(UndeliveredReason::Blocked),
        ContextMechanism::Unsupported => {
            ResourceDelivery::Undelivered(UndeliveredReason::Unsupported)
        }
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
    delivered_by(ContextMechanism::for_project_resource(
        integration,
        resource,
        projection,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uze_core::UzeHome;
    use uze_core::capability::Resource;
    use uze_core::exposure::{ExposureMechanism, ExposurePlan};
    use uze_core::integration::{
        AttachmentReceipt, ContextDelivery, HarnessDetection, ProjectResourceRoute, RulesRoute,
    };
    use uze_core::router::{CompatibilityRoute, HarnessCapabilities};

    /// A harness that reads instructions and rules itself and has its
    /// project resources projected at launch through its shim.
    struct ProjectedHarness {
        present: bool,
        projects_here: bool,
    }

    impl IntegrationPort for ProjectedHarness {
        fn id(&self) -> &'static str {
            "projected"
        }

        fn capabilities(&self) -> HarnessCapabilities {
            HarnessCapabilities::default()
        }

        fn detect(&self) -> HarnessDetection {
            HarnessDetection {
                present: self.present,
                version: None,
            }
        }

        fn exposure_plan(&self, _resource: &Resource) -> ExposurePlan {
            ExposurePlan {
                route: CompatibilityRoute::Adaptable,
                mechanism: ExposureMechanism::Unsupported {
                    rationale: "context only".to_owned(),
                },
                evidence: "test".to_owned(),
            }
        }

        fn attach_receipt(&self, _resource: &Resource) -> Result<Option<AttachmentReceipt>> {
            Ok(None)
        }

        fn runtime_projects_project_context(&self) -> bool {
            true
        }

        fn runtime_contribution_would_activate(&self, _ctx: &RuntimeContext) -> bool {
            self.projects_here
        }

        fn context_delivery(&self) -> ContextDelivery {
            ContextDelivery::Bridge {
                file_name: "VENDOR.md",
            }
        }

        fn project_resource_route(
            &self,
            _resource: AgentsDirectoryResource,
        ) -> ProjectResourceRoute {
            ProjectResourceRoute::RuntimeProjection
        }

        fn rules_route(&self) -> RulesRoute {
            RulesRoute::native("reads the rules itself")
        }
    }

    struct World {
        root: PathBuf,
        app: UzeApplication,
    }

    impl World {
        fn new(label: &str, harness: ProjectedHarness) -> Self {
            let root = uze_testkit::temp::scratch(label);
            let home = UzeHome::at(root.join("uze"));
            let shim = home.shim_path(harness.shim_name());
            fs::create_dir_all(shim.parent().unwrap()).unwrap();
            fs::write(&shim, "shim").unwrap();
            let app = UzeApplication::new(home, vec![Box::new(harness)]);
            let project = root.join("project");
            fs::create_dir_all(&project).unwrap();
            uze_testkit::git::mark_repository(&project);
            Self { root, app }
        }

        fn project(&self) -> PathBuf {
            self.root.join("project")
        }

        /// A project carrying every resource the status reports on.
        fn furnished(self) -> Self {
            let project = self.project();
            fs::write(project.join("AGENTS.md"), "Be kind.\n").unwrap();
            fs::create_dir_all(project.join(".agents/skills")).unwrap();
            fs::create_dir_all(project.join(".agents/agents")).unwrap();
            fs::create_dir_all(project.join(".agents/rules")).unwrap();
            self
        }

        fn status(&self) -> AgentContextStatus {
            let mut statuses = self.app.context().agent_context(&self.project());
            assert_eq!(statuses.len(), 1, "one status per registered harness");
            statuses.remove(0)
        }
    }

    impl Drop for World {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_present_harness_reads_what_it_reads_and_is_projected_the_rest() {
        let world = World::new(
            "agent-context-projected",
            ProjectedHarness {
                present: true,
                projects_here: true,
            },
        )
        .furnished();

        let status = world.status();

        assert!(status.present);
        assert_eq!(status.instructions, ResourceDelivery::Native);
        assert_eq!(status.project_skills, ResourceDelivery::Projected);
        assert_eq!(status.project_agents, ResourceDelivery::Projected);
        assert_eq!(status.project_rules, ResourceDelivery::Native);
    }

    #[test]
    fn a_launch_that_would_project_nothing_here_claims_no_projection() {
        let world = World::new(
            "agent-context-nothing-to-project",
            ProjectedHarness {
                present: true,
                projects_here: false,
            },
        )
        .furnished();

        let status = world.status();

        let unsupported = ResourceDelivery::Undelivered(UndeliveredReason::Unsupported);
        assert_eq!(status.project_skills, unsupported);
        assert_eq!(status.project_agents, unsupported);
    }

    #[test]
    fn what_the_project_carries_is_undelivered_to_an_absent_harness() {
        let world = World::new(
            "agent-context-absent-harness",
            ProjectedHarness {
                present: false,
                projects_here: true,
            },
        )
        .furnished();

        let status = world.status();

        let absent = ResourceDelivery::Undelivered(UndeliveredReason::HarnessAbsent);
        assert!(!status.present);
        assert_eq!(status.instructions, absent);
        assert_eq!(status.project_skills, absent);
        assert_eq!(status.project_agents, absent);
        assert_eq!(status.project_rules, absent);
    }

    #[test]
    fn what_the_project_does_not_carry_is_absent_from_it_whatever_the_harness() {
        for present in [true, false] {
            let world = World::new(
                "agent-context-bare-project",
                ProjectedHarness {
                    present,
                    projects_here: true,
                },
            );

            let status = world.status();

            assert_eq!(status.instructions, ResourceDelivery::AbsentFromProject);
            assert_eq!(status.project_skills, ResourceDelivery::AbsentFromProject);
            assert_eq!(status.project_agents, ResourceDelivery::AbsentFromProject);
            assert_eq!(status.project_rules, ResourceDelivery::AbsentFromProject);
        }
    }
}
