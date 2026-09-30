//! A project's portable instruction context: the shared `AGENTS.md`, the
//! regions UZE manages in it, and the bridges projected from it.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use uze_core::{
    Result, UzeError,
    authoring::region as authoring_region,
    context::{self as instruction_context, InstructionContribution},
    integration::{AttachmentState, ContextDelivery},
    manifest,
    project_context::{AGENTS_MD_FILE_NAME, AgentsMdGuard},
    text_region,
};

use super::managed_region::{self, Desired, plan_action_for_region};

use super::services::Context;
use super::*;

/// `PERSISTENT CONTEXT DELIVERY STRATEGY`: the region a harness that reads
/// `AGENTS.md` only through a bridge gets in its own native file inside the
/// project (see `docs/capabilities/context-manager.md`). Which harness needs
/// one is each integration's own `context_delivery()` declaration; this is
/// only the bridge protocol they share.
///
/// Kept alongside the `EXPERIMENTAL RUNTIME DELIVERY STRATEGY` (the PATH
/// shim): whether runtime projection ever replaces this bridge waits on an
/// empirical interactive comparison, and neither is folded into the other
/// before it.
///
/// Package-independent: the bridge serves however many packages contribute
/// to `AGENTS.md` and is owned by none of them.
pub(super) const INSTRUCTION_BRIDGE_IDENTITY: &str = "instruction-bridge";

/// The import syntax a bridge-needing harness uses to pull another Markdown
/// file into its own native instructions file.
pub(super) const INSTRUCTION_BRIDGE_CONTENT: &str = "@AGENTS.md";

impl Context<'_> {
    #[tracing::instrument(name = "context.inspect", skip_all, fields(project_root = %project_root.display()), err)]
    pub fn inspect(&self, project_root: &Path) -> Result<ProjectContextStatus> {
        let ContextScope {
            canonical,
            agents_md: agents_md_path,
            contributions,
        } = self.scope(project_root)?;
        let observation = instruction_context::inspect_agents_md(&agents_md_path, &contributions);
        let agents_md_exists = agents_md_path.is_file();

        // The observed project files are the shared canonical `AGENTS.md`
        // plus whatever each registered integration declares through
        // `context_delivery` (its bridge file or additional native files) —
        // never an Application-owned list of filenames.
        let mut source_names = vec![AGENTS_MD_FILE_NAME];
        for integration in &self.0.integrations {
            match integration.context_delivery() {
                ContextDelivery::Bridge { file_name } => source_names.push(file_name),
                ContextDelivery::Native { files } => source_names.extend(files),
                ContextDelivery::None => {}
            }
        }
        source_names.dedup();
        let sources: Vec<InstructionSourceObservation> = source_names
            .iter()
            .map(|file_name| {
                let path = canonical.join(file_name);
                let exists = path.is_file();
                InstructionSourceObservation {
                    file_name: (*file_name).to_owned(),
                    path: path.clone(),
                    exists,
                    has_user_content: exists
                        && text_region::has_content_outside_managed_regions(&path),
                    managed_region_identities: if exists {
                        text_region::region_identities_present(&path)
                            .into_iter()
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect()
                    } else {
                        Vec::new()
                    },
                }
            })
            .collect();

        let harnesses = self
            .0
            .integrations
            .iter()
            .filter_map(|integration| {
                let delivery = self.harness_context_delivery(
                    integration.as_ref(),
                    project_root,
                    &canonical,
                    observation.has_any_matched_contribution(),
                )?;
                Some(HarnessContextStatus {
                    integration: integration.id().to_owned(),
                    display_name: integration.display_name().to_owned(),
                    delivery,
                })
            })
            .collect::<Vec<_>>();

        let portability = derive_portability(agents_md_exists, &sources, &harnesses);
        let warnings = derive_warnings(agents_md_exists, &sources);

        Ok(ProjectContextStatus {
            root: project_root.to_path_buf(),
            canonical,
            sources,
            contributions: observation
                .packages
                .into_iter()
                .map(|(package_id, inspection)| PackageInstructionStatus {
                    package_id: package_id.as_str().to_owned(),
                    state: inspection.state,
                    reason: inspection.reason,
                })
                .collect(),
            orphaned_regions: observation.orphaned_regions,
            malformed_regions: observation.malformed_regions,
            harnesses,
            portability,
            warnings,
        })
    }

    #[tracing::instrument(name = "context.plan", skip_all, fields(project_root = %project_root.display()), err)]
    pub fn plan(&self, project_root: &Path) -> Result<ContextPlan> {
        let ContextScope {
            canonical,
            agents_md,
            contributions,
        } = self.scope(project_root)?;
        let agents_md_plan = instruction_context::plan_agents_md(&agents_md, &contributions);

        // Bridge planning asks the question `reconcile` asks — would
        // AGENTS.md end up with a matched contribution — read from the plan.
        let would_have_contribution = agents_md_plan.contributions.iter().any(|plan| {
            matches!(
                plan.action,
                instruction_context::PlannedAction::Attach
                    | instruction_context::PlannedAction::NoChange
            )
        });

        let bridges = self
            .detected_bridges(&canonical)
            .map(|(integration, bridge_file)| {
                let state = text_region::inspect(
                    &bridge_file,
                    INSTRUCTION_BRIDGE_IDENTITY,
                    INSTRUCTION_BRIDGE_CONTENT,
                )
                .state;
                BridgePlan {
                    integration: integration.id().to_owned(),
                    file: bridge_file,
                    action: plan_action_for_region(would_have_contribution, state, "bridge"),
                }
            })
            .collect();

        let authoring = authoring_desired(&canonical);
        let authoring_region = authoring.is_some().then(|| {
            managed_region::plan(
                &agents_md,
                authoring_region::owns_region,
                &authoring,
                "plugin authoring",
            )
        });

        Ok(ContextPlan {
            agents_md,
            agents_md_plan,
            authoring_region,
            bridges,
        })
    }

    #[tracing::instrument(name = "context.reconcile", skip_all, fields(project_root = %project_root.display()), err)]
    pub fn reconcile(&self, project_root: &Path) -> Result<ContextReconciliationReport> {
        let ContextScope {
            canonical,
            agents_md,
            contributions,
        } = self.scope(project_root)?;
        // Two owners rewrite this file (the package manager here, the
        // workspace its policy region), each as a whole-file read-modify-write.
        let _guard = AgentsMdGuard::acquire(&self.0.home, &canonical)?;
        let agents_md_report = instruction_context::reconcile_agents_md(&agents_md, &contributions);

        // Reconciled before the bridges, for the same reason package
        // contributions are: a bridge's own desired-state question is
        // answered by what `AGENTS.md` ends up carrying.
        let authoring = authoring_desired(&canonical);
        let authoring_region = (authoring.is_some()
            || !managed_region::in_step(&agents_md, authoring_region::owns_region, &None))
        .then(|| managed_region::converge(&agents_md, authoring_region::owns_region, authoring));

        let bridges = self
            .detected_bridges(&canonical)
            .map(|(integration, bridge_file)| {
                let inspection = text_region::reconcile(
                    &bridge_file,
                    INSTRUCTION_BRIDGE_IDENTITY,
                    INSTRUCTION_BRIDGE_CONTENT,
                    agents_md_report.has_any_matched_contribution(),
                );
                BridgeStatus {
                    integration: integration.id().to_owned(),
                    file: bridge_file,
                    state: inspection.state,
                    reason: inspection.reason,
                }
            })
            .collect();

        Ok(ContextReconciliationReport {
            agents_md,
            packages: agents_md_report
                .packages
                .into_iter()
                .map(|(package_id, inspection)| PackageInstructionStatus {
                    package_id: package_id.as_str().to_owned(),
                    state: inspection.state,
                    reason: inspection.reason,
                })
                .collect(),
            removed_orphans: agents_md_report.removed_orphans,
            blocked_orphans: agents_md_report.blocked_orphans,
            failed: agents_md_report
                .failed
                .into_iter()
                .map(|(package_id, reason)| (package_id.as_str().to_owned(), reason))
                .collect(),
            authoring_region,
            bridges,
        })
    }

    /// The project `project_root` belongs to, its shared instruction file,
    /// and what the installed packages contribute to it.
    fn scope(&self, project_root: &Path) -> Result<ContextScope> {
        if !project_root.is_dir() {
            return Err(UzeError::NotDirectory(project_root.to_path_buf()));
        }
        // Resolves upward the same way every project-scoped command does: a
        // caller pointing at a subdirectory must land on the same root every
        // other project-scoped command finds, not treat the subdirectory as
        // a project with no context at all.
        let canonical =
            uze_core::project_root::resolve_project_root(project_root)?.ok_or_else(|| {
                UzeError::NoProject {
                    hint: "context is a project's; stand inside one".to_owned(),
                }
            })?;
        Ok(ContextScope {
            agents_md: canonical.join(AGENTS_MD_FILE_NAME),
            canonical,
            contributions: self.instruction_contributions()?,
        })
    }

    /// Every detected harness that reads a bridge, with the bridge file it
    /// reads in this project.
    fn detected_bridges<'s>(
        &'s self,
        canonical: &'s Path,
    ) -> impl Iterator<Item = (&'s dyn IntegrationPort, PathBuf)> + 's {
        self.0.integrations.iter().filter_map(move |integration| {
            let ContextDelivery::Bridge { file_name } = integration.context_delivery() else {
                return None;
            };
            self.0
                .detect_cached(integration.as_ref())
                .present
                .then(|| (integration.as_ref(), canonical.join(file_name)))
        })
    }

    /// How one harness receives this project's `AGENTS.md`, or `None` for a
    /// harness that declares no instructions delivery at all — excluded
    /// rather than reported as a gap it was never claimed to close.
    fn harness_context_delivery(
        &self,
        integration: &dyn IntegrationPort,
        project_root: &Path,
        canonical: &Path,
        bridge_needed: bool,
    ) -> Option<HarnessContextDelivery> {
        let file_name = match integration.context_delivery() {
            ContextDelivery::None => return None,
            _ if !self.0.detect_cached(integration).present => {
                return Some(HarnessContextDelivery::NotDetected);
            }
            ContextDelivery::Native { .. } => return Some(HarnessContextDelivery::Native),
            ContextDelivery::Bridge { file_name } => file_name,
        };
        let projection = self.0.runtime_projection_at(integration, project_root);
        if ContextMechanism::for_instructions(integration, projection)
            == ContextMechanism::RuntimeShim
        {
            return Some(HarnessContextDelivery::Projected);
        }
        let state = text_region::inspect(
            &canonical.join(file_name),
            INSTRUCTION_BRIDGE_IDENTITY,
            INSTRUCTION_BRIDGE_CONTENT,
        )
        .state;
        Some(HarnessContextDelivery::Bridge {
            needed: bridge_needed,
            state,
        })
    }

    fn instruction_contributions(&self) -> Result<Vec<InstructionContribution>> {
        let mut contributions = Vec::new();
        for package_id in self.0.store.package_ids()? {
            let package = self.0.store.package(&package_id)?;
            let resources = uze_core::engine::package_resources_at(&package_id, &package.root)?;
            for resource in resources {
                if resource.capability.kind != CapabilityKind::Instruction {
                    continue;
                }
                contributions.push(InstructionContribution {
                    package_id: package_id.clone(),
                    content: String::from_utf8_lossy(&resource.capability.payload).into_owned(),
                });
            }
        }
        Ok(contributions)
    }
}

/// Where one context read starts: the resolved project, its shared
/// instruction file, and the installed packages' contributions to it.
struct ContextScope {
    canonical: PathBuf,
    agents_md: PathBuf,
    contributions: Vec<InstructionContribution>,
}

/// A vendor-specific instructions file carrying content of its own.
fn carries_vendor_content(source: &InstructionSourceObservation) -> bool {
    source.file_name != AGENTS_MD_FILE_NAME && source.exists && source.has_user_content
}

fn derive_portability(
    agents_md_exists: bool,
    sources: &[InstructionSourceObservation],
    harnesses: &[HarnessContextStatus],
) -> Portability {
    if !agents_md_exists {
        let vendor_files: Vec<PathBuf> = sources
            .iter()
            .filter(|source| carries_vendor_content(source))
            .map(|source| source.path.clone())
            .collect();
        return if vendor_files.is_empty() {
            Portability::NoContext
        } else {
            Portability::VendorLocked {
                files: vendor_files,
            }
        };
    }
    let gaps: Vec<String> = harnesses
        .iter()
        .filter_map(|harness| match &harness.delivery {
            HarnessContextDelivery::Bridge {
                needed: true,
                state,
            } if *state != AttachmentState::Matched => {
                Some(format!("{}: bridge {:?}", harness.integration, state))
            }
            _ => None,
        })
        .collect();
    if gaps.is_empty() {
        Portability::Portable
    } else {
        Portability::PartiallyPortable { gaps }
    }
}

fn derive_warnings(
    agents_md_exists: bool,
    sources: &[InstructionSourceObservation],
) -> Vec<String> {
    let mut warnings = Vec::new();
    let vendor_specific_with_content: Vec<&InstructionSourceObservation> = sources
        .iter()
        .filter(|source| carries_vendor_content(source))
        .collect();
    if !agents_md_exists && vendor_specific_with_content.len() >= 2 {
        let names: Vec<&str> = vendor_specific_with_content
            .iter()
            .map(|source| source.file_name.as_str())
            .collect();
        warnings.push(format!(
            "{} each carry their own content with no shared AGENTS.md — these are observed as \
             independent, potentially divergent vendor-specific sources; UZE does not compare or \
             consolidate them.",
            names.join(" and ")
        ));
    }
    if agents_md_exists {
        for source in &vendor_specific_with_content {
            warnings.push(format!(
                "{} carries content beyond the shared bridge — this is expected and supported \
                 (vendor-specific instructions alongside portable ones), not a gap.",
                source.file_name
            ));
        }
    }
    warnings
}

/// The authoring region a project is owed: one for every project with an
/// `agents.yaml`, whether or not the workspace is ever used there.
pub(super) fn authoring_desired(canonical: &Path) -> Desired {
    manifest::manifest_path_for(canonical).is_file().then(|| {
        (
            authoring_region::identity(),
            authoring_region::instructions(),
        )
    })
}
