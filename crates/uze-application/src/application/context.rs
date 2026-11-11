//! A project's portable instruction context: the shared `AGENTS.md`, the
//! harnesses' own files beside it, and how each harness reads it.
//!
//! The file is the operator's. Nothing here writes it, except to take out
//! what an earlier release left there (`leftover_regions`).

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use uze_core::{
    Result, UzeError,
    integration::ContextDelivery,
    project_context::{AGENTS_MD_FILE_NAME, AgentsMdGuard},
    text_region,
};

use super::earlier::regions;

use super::services::Context;
use super::*;

impl Context<'_> {
    #[tracing::instrument(name = "context.inspect", skip_all, fields(project_root = %project_root.display()), err)]
    pub fn inspect(&self, project_root: &Path) -> Result<ProjectContextStatus> {
        let canonical = self.scope(project_root)?;
        let agents_md_exists = canonical.join(AGENTS_MD_FILE_NAME).is_file();

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
                let delivery = self.harness_context_delivery(integration.as_ref())?;
                Some(HarnessContextStatus {
                    integration: integration.id().to_owned(),
                    display_name: integration.display_name().to_owned(),
                    delivery,
                })
            })
            .collect::<Vec<_>>();

        let portability = derive_portability(agents_md_exists, &sources);
        let warnings = derive_warnings(agents_md_exists, &sources);
        let leftover_regions = regions::pending(self.0, &canonical);

        Ok(ProjectContextStatus {
            root: project_root.to_path_buf(),
            canonical,
            sources,
            leftover_regions,
            harnesses,
            portability,
            warnings,
        })
    }

    #[tracing::instrument(name = "context.plan", skip_all, fields(project_root = %project_root.display()), err)]
    pub fn plan(&self, project_root: &Path) -> Result<ContextPlan> {
        let canonical = self.scope(project_root)?;
        Ok(ContextPlan {
            agents_md: canonical.join(AGENTS_MD_FILE_NAME),
            leftover_regions: regions::pending(self.0, &canonical),
        })
    }

    /// Takes out what an earlier release wrote into the project's files,
    /// in a main working tree. Nothing is written into them otherwise.
    #[tracing::instrument(name = "context.reconcile", skip_all, fields(project_root = %project_root.display()), err)]
    pub fn reconcile(&self, project_root: &Path) -> Result<ContextReconciliationReport> {
        let canonical = self.scope(project_root)?;
        let files = self.0.region_files(&canonical);
        let leftover_regions = if regions::applies_at(&canonical) {
            // A whole-file read-modify-write, so one pass at a time per project.
            let _guard = AgentsMdGuard::acquire(&self.0.home, &canonical)?;
            regions::remove(&files)?
        } else {
            None
        };
        Ok(ContextReconciliationReport {
            agents_md: files.agents_md,
            leftover_regions,
        })
    }

    /// The project `project_root` belongs to.
    fn scope(&self, project_root: &Path) -> Result<PathBuf> {
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
        Ok(canonical)
    }

    /// How one harness receives this project's `AGENTS.md`, or `None` for a
    /// harness that declares no instructions delivery at all — excluded
    /// rather than reported as a gap it was never claimed to close.
    fn harness_context_delivery(
        &self,
        integration: &dyn IntegrationPort,
    ) -> Option<HarnessContextDelivery> {
        match integration.context_delivery() {
            ContextDelivery::None => None,
            _ if !self.0.detect_cached(integration).present => {
                Some(HarnessContextDelivery::NotDetected)
            }
            ContextDelivery::Native { .. } => Some(HarnessContextDelivery::Native),
            ContextDelivery::Bridge { .. } => Some(HarnessContextDelivery::ReadsNatively),
        }
    }
}

/// A vendor-specific instructions file carrying content of its own.
fn carries_vendor_content(source: &InstructionSourceObservation) -> bool {
    source.file_name != AGENTS_MD_FILE_NAME && source.exists && source.has_user_content
}

fn derive_portability(
    agents_md_exists: bool,
    sources: &[InstructionSourceObservation],
) -> Portability {
    if agents_md_exists {
        return Portability::Portable;
    }
    let vendor_files: Vec<PathBuf> = sources
        .iter()
        .filter(|source| carries_vendor_content(source))
        .map(|source| source.path.clone())
        .collect();
    if vendor_files.is_empty() {
        Portability::NoContext
    } else {
        Portability::VendorLocked {
            files: vendor_files,
        }
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
                "{} carries instructions of its own beside AGENTS.md — this is expected and \
                 supported (vendor-specific instructions alongside portable ones), not a gap.",
                source.file_name
            ));
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uze_core::capability::Resource;
    use uze_core::exposure::{ExposureMechanism, ExposurePlan};
    use uze_core::integration::{AttachmentReceipt, HarnessDetection};
    use uze_core::router::{CompatibilityRoute, HarnessCapabilities};

    fn observed(
        file_name: &str,
        exists: bool,
        has_user_content: bool,
    ) -> InstructionSourceObservation {
        InstructionSourceObservation {
            file_name: file_name.to_owned(),
            path: PathBuf::from(file_name),
            exists,
            has_user_content,
            managed_region_identities: Vec::new(),
        }
    }

    #[test]
    fn two_vendor_files_with_content_and_no_agents_md_are_named_together_as_divergent() {
        let sources = [
            observed("ONE.md", true, true),
            observed("TWO.md", true, true),
        ];

        let warnings = derive_warnings(false, &sources);

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].starts_with("ONE.md and TWO.md each carry"),
            "{warnings:?}"
        );
        assert!(matches!(
            derive_portability(false, &sources),
            Portability::VendorLocked { files } if files.len() == 2
        ));
    }

    #[test]
    fn a_single_vendor_file_is_vendor_locked_without_a_divergence_warning() {
        let sources = [
            observed("ONE.md", true, true),
            observed("TWO.md", true, false),
        ];

        assert!(derive_warnings(false, &sources).is_empty());
        assert!(matches!(
            derive_portability(false, &sources),
            Portability::VendorLocked { files } if files == [PathBuf::from("ONE.md")]
        ));
    }

    #[test]
    fn a_vendor_file_beside_agents_md_is_noted_once_and_agents_md_itself_never() {
        let sources = [
            observed(AGENTS_MD_FILE_NAME, true, true),
            observed("ONE.md", true, true),
        ];

        let warnings = derive_warnings(true, &sources);

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].starts_with("ONE.md carries instructions of its own beside AGENTS.md"));
        assert!(matches!(
            derive_portability(true, &sources),
            Portability::Portable
        ));
    }

    #[test]
    fn agents_md_alone_is_no_vendor_content() {
        let sources = [observed(AGENTS_MD_FILE_NAME, true, true)];

        assert!(derive_warnings(true, &sources).is_empty());
        assert!(matches!(
            derive_portability(false, &sources),
            Portability::NoContext
        ));
    }

    /// A harness that reads `AGENTS.md` beside a file of its own, or that
    /// declares no instructions delivery at all.
    struct InstructedHarness {
        id: &'static str,
        delivery: fn() -> ContextDelivery,
        present: bool,
    }

    impl IntegrationPort for InstructedHarness {
        fn id(&self) -> &'static str {
            self.id
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

        fn context_delivery(&self) -> ContextDelivery {
            (self.delivery)()
        }
    }

    fn bridged() -> ContextDelivery {
        ContextDelivery::Bridge {
            file_name: "VENDOR.md",
        }
    }

    #[test]
    fn each_harness_is_reported_by_how_it_reads_agents_md_and_one_without_a_delivery_is_left_out() {
        let root = uze_testkit::temp::scratch("context-inspect-harnesses");
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        uze_testkit::git::mark_repository(&project);
        let app = UzeApplication::new(
            UzeHome::at(root.join("uze")),
            vec![
                Box::new(InstructedHarness {
                    id: "present",
                    delivery: bridged,
                    present: true,
                }),
                Box::new(InstructedHarness {
                    id: "missing",
                    delivery: bridged,
                    present: false,
                }),
                Box::new(InstructedHarness {
                    id: "silent",
                    delivery: || ContextDelivery::None,
                    present: true,
                }),
            ],
        );

        let status = app.context().inspect(&project).unwrap();

        let deliveries: Vec<(&str, &HarnessContextDelivery)> = status
            .harnesses
            .iter()
            .map(|harness| (harness.integration.as_str(), &harness.delivery))
            .collect();
        assert!(
            matches!(
                deliveries[..],
                [
                    ("present", HarnessContextDelivery::ReadsNatively),
                    ("missing", HarnessContextDelivery::NotDetected),
                ]
            ),
            "{deliveries:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_directory_standing_where_a_vendor_file_would_be_carries_no_content() {
        let root = uze_testkit::temp::scratch("context-inspect-directory");
        let project = root.join("project");
        fs::create_dir_all(project.join("VENDOR.md")).unwrap();
        uze_testkit::git::mark_repository(&project);
        let app = UzeApplication::new(
            UzeHome::at(root.join("uze")),
            vec![Box::new(InstructedHarness {
                id: "present",
                delivery: bridged,
                present: true,
            })],
        );

        let status = app.context().inspect(&project).unwrap();

        let vendor = status
            .sources
            .iter()
            .find(|source| source.file_name == "VENDOR.md")
            .expect("the harness's file is observed");
        assert!(!vendor.exists);
        assert!(!vendor.has_user_content);
        assert!(matches!(status.portability, Portability::NoContext));
        let _ = fs::remove_dir_all(root);
    }
}
