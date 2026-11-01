//! The health catalogue: everything UZE owns, declared once with the check
//! that observes it and the lane that puts it back.
//!
//! Three lanes, and every finding takes exactly one:
//!
//! - **automatic**: bounded maintenance puts it back on every `doctor` and
//!   in the workspace, from what the machine already holds (ADR-037);
//! - **repair**: `uze doctor --repair` puts it back, because doing so needs
//!   the network, a vendor CLI or a process UZE does not start unasked;
//! - **operator**: nothing UZE does on its own, for the reason the check
//!   declares — an edit is the person's, a record is the only copy of what
//!   it says.
//!
//! `doctor`, `doctor --repair` and the workspace read the same findings.
//! Coverage is by construction: every path [`UzeHome::owned_paths`] lists,
//! every kind of artifact a receipt owns and every kind of registration
//! UZE asks a harness for is named by one check
//! (`every_owned_path_has_a_health_check`), so a thing added to any of
//! them without a check fails the build. A later check — a collector, a
//! comparison of the home with its schema — joins [`CATALOGUE`] the same
//! way and names what it answers for.

use std::panic::{AssertUnwindSafe, catch_unwind};

use serde::Serialize;

use uze_core::{
    integration::{
        AttachmentState, IntegrationStatus, ManagedArtifact, RegistrationRead,
        VendorRegistrationKind, is_build_receipt,
    },
    reconciliation::ReconciliationReport,
    state,
};

use super::services::Health;
use super::*;

/// The command every finding of the repair lane names.
pub const REPAIR_COMMAND: &str = "uze doctor --repair";

/// What a finding is about.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Subject {
    /// What a receipt owns in a harness: links, entries, files, trees.
    Receipts,
    /// A package's build in a harness's vendor store.
    Builds,
    /// A package's bytes in `sources/`.
    PackageBytes,
    /// What an earlier version left that this one did not adopt.
    Leftovers,
    /// A registration UZE asked a harness to keep: its marketplace.
    Registrations,
    /// The terminal runtime's endpoint, observed by the runtime itself.
    TerminalEndpoint,
    /// The launchers the workspace starts each harness through.
    Shims,
    /// The records under `state/`.
    Records,
    /// The locks beside them.
    Locks,
    /// The operator's own files at the root.
    Authored,
    /// What is produced or observed again: `runtime/`, `cache/`.
    Derived,
    /// Paths an earlier release wrote and this one no longer reads.
    Retired,
}

/// Which repair a finding waits on.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    Automatic,
    Repair,
    Operator,
}

/// How much a finding stands in the way.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Something does not work until it is resolved.
    Problem,
    /// Everything works; something is degraded or waiting.
    Warning,
}

/// One declared check: what it answers for, and the lanes its findings
/// take, each with why.
#[derive(Clone, Copy, Debug)]
pub struct HealthCheck {
    pub subject: Subject,
    /// The owned paths it answers for, by the builder that composes them.
    pub paths: &'static [&'static str],
    /// The kinds of artifact a receipt owns that it answers for.
    pub artifacts: &'static [&'static str],
    pub registrations: &'static [VendorRegistrationKind],
    pub lanes: &'static [(Lane, &'static str)],
}

/// One thing a check found wrong.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HealthFinding {
    pub subject: Subject,
    pub lane: Lane,
    pub severity: Severity,
    /// What it is about, as the report names it: a package, a harness, a
    /// record.
    pub about: String,
    pub detail: String,
    /// The harness it concerns, by the name the report shows.
    pub harness: Option<String>,
    /// Where the artifact it concerns is, when it is one.
    pub location: Option<std::path::PathBuf>,
    /// The owned path it concerns, by the builder that composes it.
    pub path: Option<&'static str>,
    /// The command that resolves it, for a finding the operator resolves.
    pub command: Option<String>,
}

impl HealthFinding {
    /// What the person runs about it: `uze doctor --repair` for everything
    /// in the repair lane, the check's own command otherwise.
    pub fn action(&self) -> Option<&str> {
        match self.lane {
            Lane::Repair => Some(REPAIR_COMMAND),
            _ => self.command.as_deref(),
        }
    }

    /// Whether `doctor` already reports it on a row of its own: each
    /// plugin's deliveries and attachments, the ledger and the setup
    /// records. Every other finding is listed from here.
    pub fn has_own_row(&self) -> bool {
        matches!(self.subject, Subject::Receipts | Subject::Builds)
            || matches!(
                self.path,
                Some("registry_path" | "attachments_path" | "provisioning_state_path")
            )
    }

    /// The finding in one short sentence, for a list of alerts that shows
    /// the detail apart.
    pub fn headline(&self) -> String {
        match self.subject {
            Subject::PackageBytes => format!("{} is missing its bytes", self.about),
            Subject::Registrations => {
                format!("{} has uze's marketplace registered elsewhere", self.about)
            }
            Subject::TerminalEndpoint => "A stopped terminal server left its endpoint".to_owned(),
            Subject::Shims => format!("{} launcher needs placing again", self.about),
            Subject::Records => format!("{} cannot be read", self.about),
            _ => format!("{} needs attention", self.about),
        }
    }

    /// A terminal-runtime endpoint left by a server that is gone: what the
    /// binary, which alone reaches the runtime, reports.
    pub fn stale_endpoint(endpoint: &std::path::Path) -> Self {
        Self {
            subject: Subject::TerminalEndpoint,
            lane: Lane::Repair,
            severity: Severity::Warning,
            about: "workspace".to_owned(),
            detail: format!(
                "{} is left by a terminal server that is no longer running",
                endpoint.display()
            ),
            harness: None,
            location: None,
            path: None,
            command: None,
        }
    }

    fn could_not_check(subject: Subject, reason: &str) -> Self {
        Self {
            subject,
            lane: Lane::Operator,
            severity: Severity::Problem,
            about: format!("{subject:?}").to_lowercase(),
            detail: format!("could not be checked: {reason}"),
            harness: None,
            location: None,
            path: None,
            command: None,
        }
    }
}

/// How fresh an observation must be.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Inspection {
    /// What a plain `doctor` and the workspace afford: a `Matched` verdict
    /// may come from the inspection cache, and a harness's registration is
    /// read from its own record, never by starting it.
    Cached,
    /// What a repair plans from: every receipt inspected now, every
    /// registration asked of the harness itself.
    Live,
}

const OPERATOR_EDIT: &str = "uze never writes over what someone changed";
const RECORD: &str = "a record is the only copy of what it says; uze never writes over one it \
                      cannot read";

/// Every check, once. A new owned path, receipt kind or registration kind
/// joins one of these, or a check of its own.
pub const CATALOGUE: &[HealthCheck] = &[
    HealthCheck {
        subject: Subject::Receipts,
        paths: &[
            "runtime_attachments_dir",
            "generated_attachments_dir",
            "delivered_packages_dir",
            "delivered_package_dir",
        ],
        artifacts: &ManagedArtifact::KINDS,
        registrations: &[],
        lanes: &[
            (
                Lane::Automatic,
                "a missing artifact whose receipt carries it whole is put back",
            ),
            (Lane::Operator, OPERATOR_EDIT),
        ],
    },
    HealthCheck {
        subject: Subject::Builds,
        paths: &["store_dir", "vendor_store_dir", "build_dir"],
        artifacts: &["integration_owned"],
        registrations: &[],
        lanes: &[
            (
                Lane::Automatic,
                "a missing build is rendered again from the package's bytes",
            ),
            (Lane::Operator, OPERATOR_EDIT),
        ],
    },
    HealthCheck {
        subject: Subject::PackageBytes,
        paths: &["sources_dir", "plugins_dir", "plugin_dir", "restoring_dir"],
        artifacts: &[],
        registrations: &[],
        lanes: &[(
            Lane::Repair,
            "acquired again from where they came from, kept only when their digest matches",
        )],
    },
    HealthCheck {
        subject: Subject::Leftovers,
        paths: &["superseded_dir"],
        artifacts: &[],
        registrations: &[],
        lanes: &[(
            Lane::Operator,
            "the only copy of a revision a failed restore left; nothing reads it",
        )],
    },
    HealthCheck {
        subject: Subject::Registrations,
        paths: &[],
        artifacts: &[],
        registrations: &VendorRegistrationKind::ALL,
        lanes: &[(
            Lane::Repair,
            "registered again where the vendor store is, through the harness's own CLI",
        )],
    },
    HealthCheck {
        subject: Subject::TerminalEndpoint,
        paths: &["terminal_runtime_dir"],
        artifacts: &[],
        registrations: &[],
        lanes: &[(
            Lane::Repair,
            "cleared by the terminal runtime itself, under its workspace lock",
        )],
    },
    HealthCheck {
        subject: Subject::Shims,
        paths: &["shims_dir", "shim_path"],
        artifacts: &[],
        registrations: &[],
        lanes: &[(Lane::Repair, "placed again, pointing at this uze")],
    },
    HealthCheck {
        subject: Subject::Records,
        paths: &[
            "state_dir",
            "registry_path",
            "attachments_path",
            "undelivered_path",
            "provisioning_state_path",
            "marketplaces_path",
            "hosts_path",
            "launch_overrides_path",
            "extensions_record_path",
            "extension_stores_dir",
            "extension_plugin_stores_dir",
            "extension_store_path",
            "extension_preferences_path",
            "extension_plugin_preferences_dir",
            "extension_secret_path",
            "extension_plugin_secrets_dir",
            "client_layout_path",
            "binary_path",
            "install_receipt_path",
            "machine_usage_path",
            "machine_memory_dir",
            "proposed_memory_dir",
            "projects_dir",
            "project_dir",
            "project_marker_path",
            "tasks_path",
            "approved_commands_path",
            "conversations_dir",
            "conversation_path",
            "review_path",
            "prompt_history_path",
            "project_memory_dir",
        ],
        artifacts: &[],
        registrations: &[],
        lanes: &[(Lane::Operator, RECORD)],
    },
    HealthCheck {
        subject: Subject::Locks,
        paths: &[
            "mutation_lock_path",
            "config_lock_path",
            "launch_overrides_lock_path",
            "extensions_record_lock_path",
            "extension_store_lock_path",
            "extension_preferences_lock_path",
            "binary_check_lock_path",
            "marketplace_fetch_lock_path",
            "marketplace_mirror_lock_path",
            "machine_usage_lock_path",
            "agents_lock_path",
            "delivery_lock_path",
            "agents_md_lock_path",
            "conversations_lock_path",
            "review_lock_path",
            "prompt_history_lock_path",
            "memory_lock_path",
            "agent_reports_lock_path",
        ],
        artifacts: &[],
        registrations: &[],
        lanes: &[(
            Lane::Operator,
            "a kernel lock cannot go stale, and clearing one would let two holders in",
        )],
    },
    HealthCheck {
        subject: Subject::Authored,
        paths: &[
            "config_path",
            "keymap_path",
            "theme_overrides_path",
            "themes_dir",
        ],
        artifacts: &[],
        registrations: &[],
        lanes: &[(
            Lane::Operator,
            "the operator's own text: one that does not parse is reported, never written over",
        )],
    },
    HealthCheck {
        subject: Subject::Derived,
        paths: &[
            "runtime_dir",
            "runtime_projects_dir",
            "runtime_project_dir",
            "runtime_projection_dir",
            "agent_context_dir",
            "runtime_sessions_dir",
            "session_context_dir",
            "runtime_rules_dir",
            "rules_sessions_dir",
            "compiled_extensions_dir",
            "compiled_extension_path",
            "extension_scratch_root",
            "extension_scratch_dir",
            "extension_pictures_dir",
            "extension_costs_path",
            "agent_reports_path",
            "cache_dir",
            "harnesses_cache_path",
            "harness_detection_cache_path",
            "requirement_probe_cache_path",
            "shell_observation_cache_path",
            "machine_overview_cache_path",
            "machine_usage_cache_path",
            "inspection_cache_path",
            "appearance_cache_path",
            "release_notes_cache_path",
            "marketplace_cache_dir",
            "collection_record_path",
            "logs_dir",
            "gate_log_path",
        ],
        artifacts: &[],
        registrations: &[],
        lanes: &[(
            Lane::Automatic,
            "produced or observed again whenever it is missing or cannot be read",
        )],
    },
    HealthCheck {
        subject: Subject::Retired,
        paths: &[
            "pre_sources_layout",
            "retired_profiles_path",
            "scattered_project_records",
        ],
        artifacts: &[],
        registrations: &[],
        lanes: &[(
            Lane::Automatic,
            "carried across or removed by the next bootstrap",
        )],
    },
];

impl Health<'_> {
    /// Every check's findings, each check isolated from another's failure:
    /// one that cannot run is a finding of its own, never the end of the
    /// report. Receipts are judged from `reconciliations`, the inspection
    /// the caller already made at the freshness `inspection` names.
    pub(crate) fn observe(
        &self,
        inspection: Inspection,
        reconciliations: &[ReconciliationReport],
    ) -> Vec<HealthFinding> {
        let mut findings = Vec::new();
        findings.extend(isolated(Subject::Receipts, || {
            Ok(self.receipt_findings(reconciliations))
        }));
        findings.extend(isolated(Subject::PackageBytes, || {
            self.missing_bytes().map_err(|error| error.to_string())
        }));
        findings.extend(isolated(Subject::Registrations, || {
            Ok(self.misplaced_registrations(inspection))
        }));
        findings.extend(isolated(Subject::Shims, || Ok(self.missing_shims())));
        findings.extend(isolated(Subject::Records, || Ok(self.unreadable_records())));
        findings
    }

    /// Every installed package whose bytes are gone from `sources/`.
    fn missing_bytes(&self) -> Result<Vec<HealthFinding>> {
        Ok(self
            .0
            .store
            .packages()?
            .into_iter()
            .filter(|package| !package.root.is_dir())
            .map(|package| HealthFinding {
                subject: Subject::PackageBytes,
                lane: Lane::Repair,
                severity: Severity::Warning,
                about: package.id.as_str().to_owned(),
                detail: "its bytes are missing: every harness keeps what was delivered of it, \
                         and it is not built again until they return"
                    .to_owned(),
                harness: None,
                location: Some(package.root.clone()),
                path: Some("plugin_dir"),
                command: None,
            })
            .collect())
    }

    /// Every registration a detected harness holds somewhere other than
    /// where UZE keeps what it names.
    pub(crate) fn misplaced_registrations(&self, inspection: Inspection) -> Vec<HealthFinding> {
        let read = match inspection {
            Inspection::Cached => RegistrationRead::Recorded,
            Inspection::Live => RegistrationRead::Live,
        };
        self.0
            .integrations
            .iter()
            .map(|integration| integration.as_ref())
            .filter(|integration| self.0.detect_cached(*integration).present)
            .flat_map(|integration| {
                integration
                    .registrations(read)
                    .into_iter()
                    .filter(|registration| registration.misplaced())
                    .map(move |registration| HealthFinding {
                        subject: Subject::Registrations,
                        lane: Lane::Repair,
                        severity: Severity::Problem,
                        about: integration.display_name().to_owned(),
                        detail: format!(
                            "`{}` is registered at {}, not at {}",
                            registration.name,
                            registration
                                .registered
                                .as_deref()
                                .unwrap_or(std::path::Path::new(""))
                                .display(),
                            registration.expected.display()
                        ),
                        harness: Some(integration.display_name().to_owned()),
                        location: registration.registered.clone(),
                        path: None,
                        command: None,
                    })
            })
            .collect()
    }

    /// Every launcher the workspace starts a set-up harness through that is
    /// missing, or that runs something other than this uze.
    pub(crate) fn missing_shims(&self) -> Vec<HealthFinding> {
        let program = std::env::current_exe().ok();
        self.expected_shims()
            .into_iter()
            .filter_map(|integration| {
                let shim = self.0.home.shim_path(integration.shim_name());
                let detail = if !shim.exists() && std::fs::symlink_metadata(&shim).is_err() {
                    format!("its launcher {} is missing", shim.display())
                } else if program.as_deref().is_some_and(|program| {
                    !uze_platform::executable::is_launcher_of(program, &shim)
                }) {
                    format!("its launcher {} does not start this uze", shim.display())
                } else {
                    return None;
                };
                Some(HealthFinding {
                    subject: Subject::Shims,
                    lane: Lane::Repair,
                    severity: Severity::Warning,
                    about: integration.display_name().to_owned(),
                    detail,
                    harness: Some(integration.display_name().to_owned()),
                    location: Some(shim),
                    path: Some("shim_path"),
                    command: None,
                })
            })
            .collect()
    }

    /// The harnesses the workspace starts through a launcher of UZE's: set
    /// up, here, and opted into runtime integration.
    pub(crate) fn expected_shims(&self) -> Vec<&dyn IntegrationPort> {
        self.0
            .integrations
            .iter()
            .map(|integration| integration.as_ref())
            .filter(|integration| integration.supports_runtime_integration())
            .filter(|integration| {
                integration.status(&self.0.home) != IntegrationStatus::NotConfigured
            })
            .filter(|integration| self.0.detect_cached(*integration).present)
            .collect()
    }

    /// Every machine-wide record that does not read, each through its own
    /// reader: one unreadable record never hides another.
    fn unreadable_records(&self) -> Vec<HealthFinding> {
        let home = &self.0.home;
        let reads: [RecordRead<'_>; 8] = [
            (
                "registry_path",
                "registry",
                Box::new(|| self.0.store.package_ids().err().map(|e| e.to_string())),
            ),
            (
                "attachments_path",
                "ledger",
                Box::new(|| state::receipts(home, None).err().map(|e| e.to_string())),
            ),
            (
                "provisioning_state_path",
                "setup records",
                Box::new(|| {
                    self.0.integrations.iter().find_map(|integration| {
                        state::provisioning(home, integration.id())
                            .err()
                            .map(|e| e.to_string())
                    })
                }),
            ),
            (
                "marketplaces_path",
                "marketplaces",
                Box::new(|| state::marketplace_list(home).err().map(|e| e.to_string())),
            ),
            (
                "undelivered_path",
                "undelivered",
                Box::new(|| state::undelivered(home, "").err().map(|e| e.to_string())),
            ),
            (
                "hosts_path",
                "hosts",
                Box::new(|| self.0.marketplace().hosts().err().map(|e| e.to_string())),
            ),
            (
                "extensions_record_path",
                "extension consent",
                Box::new(|| unparsed(&home.extensions_record_path())),
            ),
            (
                "launch_overrides_path",
                "launch overrides",
                Box::new(|| unparsed(&home.launch_overrides_path())),
            ),
        ];
        reads
            .into_iter()
            .filter_map(|(path, about, read)| {
                let error = match catch_unwind(AssertUnwindSafe(read)) {
                    Ok(error) => error?,
                    Err(_) => "its reader stopped unexpectedly".to_owned(),
                };
                Some(HealthFinding {
                    subject: Subject::Records,
                    lane: Lane::Operator,
                    severity: Severity::Problem,
                    about: about.to_owned(),
                    detail: error,
                    harness: None,
                    location: None,
                    path: Some(path),
                    command: None,
                })
            })
            .collect()
    }
}

/// One record's read: the builder naming it, how the report names it, and
/// why it does not read, if it does not.
type RecordRead<'a> = (
    &'static str,
    &'static str,
    Box<dyn FnOnce() -> Option<String> + 'a>,
);

/// Why a record with no reader of its own here does not read as JSON, if
/// it does not; one that is not there reads.
fn unparsed(path: &std::path::Path) -> Option<String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => return Some(format!("{}: {error}", path.display())),
    };
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .err()
        .map(|error| format!("{}: {error}", path.display()))
}

/// Runs one check, turning its failure — an error or a panic — into a
/// finding of its own.
fn isolated(
    subject: Subject,
    check: impl FnOnce() -> std::result::Result<Vec<HealthFinding>, String>,
) -> Vec<HealthFinding> {
    match catch_unwind(AssertUnwindSafe(check)) {
        Ok(Ok(findings)) => findings,
        Ok(Err(reason)) => vec![HealthFinding::could_not_check(subject, &reason)],
        Err(_) => vec![HealthFinding::could_not_check(
            subject,
            "the check stopped unexpectedly",
        )],
    }
}

impl Health<'_> {
    /// What the receipts' inspection says is wrong, in the lane that puts
    /// each back: a missing artifact its receipt carries whole, a missing
    /// build, or one a delivery wrote inside a vendor store, is
    /// maintenance's; anything edited is the person's.
    fn receipt_findings(&self, reconciliations: &[ReconciliationReport]) -> Vec<HealthFinding> {
        let store = self.0.home.store_dir();
        let mut findings = Vec::new();
        for report in reconciliations {
            // An inspection that compares what was delivered with the
            // package cannot answer while the package's bytes are gone: it
            // waits on them, and so on the repair that brings them back.
            let bytes_missing = self
                .0
                .package_by_name(&report.package_id)
                .is_ok_and(|package| !package.root.is_dir());
            for entry in &report.receipts {
                let build = is_build_receipt(&entry.receipt);
                let subject = if build {
                    Subject::Builds
                } else {
                    Subject::Receipts
                };
                let location = entry.receipt.artifact.location();
                let local = build
                    || restored_from_receipt(&entry.receipt)
                    || (entry.receipt.resource_identity.is_some() && location.starts_with(&store));
                let (lane, command) = match entry.inspection.state {
                    AttachmentState::Matched => continue,
                    AttachmentState::Blocked if bytes_missing => (Lane::Repair, None),
                    AttachmentState::Missing if local => (Lane::Automatic, None),
                    AttachmentState::Missing => (
                        Lane::Operator,
                        Some(format!("uze setup {}", entry.receipt.integration)),
                    ),
                    AttachmentState::Drifted
                    | AttachmentState::Conflict
                    | AttachmentState::Blocked => (
                        Lane::Operator,
                        Some(format!("uze inspect {}", report.package_id)),
                    ),
                };
                findings.push(HealthFinding {
                    subject,
                    lane,
                    severity: Severity::Problem,
                    about: report.package_id.clone(),
                    detail: format!(
                        "{:?} at {}: {}",
                        entry.inspection.state,
                        location.display(),
                        entry.inspection.reason
                    ),
                    harness: Some(
                        self.0
                            .integration_named(&entry.receipt.integration)
                            .map_or_else(
                                || entry.receipt.integration.clone(),
                                |integration| integration.display_name().to_owned(),
                            ),
                    ),
                    location: Some(location),
                    path: build.then_some("build_dir"),
                    command,
                });
            }
        }
        findings
    }
}

/// Whether a missing artifact is put back from its receipt alone, as
/// `IntegrationPort::repair_missing_receipt` does by default.
fn restored_from_receipt(receipt: &uze_core::integration::AttachmentReceipt) -> bool {
    matches!(
        receipt.artifact,
        ManagedArtifact::SymlinkReference { .. }
            | ManagedArtifact::ManagedTextRegion { .. }
            | ManagedArtifact::GeneratedFile { .. }
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// Every path the map of what UZE owns lists, every kind of artifact a
    /// receipt can own and every kind of registration UZE asks a harness
    /// for is answered by exactly one check, and every lane a check takes
    /// says why. A thing added to any of them without a check fails here,
    /// naming it.
    #[test]
    fn every_owned_path_has_a_health_check() {
        let mut checked: BTreeMap<&str, Vec<Subject>> = BTreeMap::new();
        for check in CATALOGUE {
            for path in check.paths {
                checked.entry(path).or_default().push(check.subject);
            }
        }
        let unchecked: Vec<&str> = UzeHome::owned_paths()
            .iter()
            .map(|owned| owned.name)
            .filter(|name| !checked.contains_key(name))
            .collect();
        assert!(
            unchecked.is_empty(),
            "owned paths no health check answers for: {unchecked:?}"
        );
        let twice: Vec<_> = checked
            .iter()
            .filter(|(_, subjects)| subjects.len() > 1)
            .collect();
        assert!(twice.is_empty(), "paths two checks answer for: {twice:?}");
        let unknown: Vec<&&str> = checked
            .keys()
            .filter(|name| UzeHome::owned_path(name).is_none())
            .collect();
        assert!(
            unknown.is_empty(),
            "checks name paths the map does not list: {unknown:?}"
        );

        for kind in ManagedArtifact::KINDS {
            assert!(
                CATALOGUE
                    .iter()
                    .any(|check| check.artifacts.contains(&kind)),
                "no health check answers for the receipt kind {kind}"
            );
        }
        for kind in VendorRegistrationKind::ALL {
            assert!(
                CATALOGUE
                    .iter()
                    .any(|check| check.registrations.contains(&kind)),
                "no health check answers for the registration kind {kind:?}"
            );
        }
        for check in CATALOGUE {
            assert!(!check.lanes.is_empty(), "{:?} names no lane", check.subject);
            for (lane, why) in check.lanes {
                assert!(
                    !why.trim().is_empty(),
                    "{:?} {lane:?} says no why",
                    check.subject
                );
            }
        }
    }

    /// A detected harness whose `uze-store` is registered under the runtime
    /// tree, as an earlier release left it.
    struct Misregistered {
        home: UzeHome,
    }

    impl IntegrationPort for Misregistered {
        fn id(&self) -> &'static str {
            "misregistered"
        }
        fn display_name(&self) -> &'static str {
            "Misregistered"
        }
        fn capabilities(&self) -> uze_core::router::HarnessCapabilities {
            uze_core::router::HarnessCapabilities::default()
        }
        fn exposure_plan(
            &self,
            _resource: &uze_core::capability::Resource,
        ) -> uze_core::exposure::ExposurePlan {
            uze_core::exposure::ExposurePlan {
                route: uze_core::router::CompatibilityRoute::Unsupported,
                mechanism: uze_core::exposure::ExposureMechanism::Unsupported {
                    rationale: "test does not attach".to_owned(),
                },
                evidence: "test".to_owned(),
            }
        }
        fn detect(&self) -> HarnessDetection {
            HarnessDetection {
                present: true,
                version: None,
            }
        }
        fn inspect_receipt(
            &self,
            _receipt: &uze_core::integration::AttachmentReceipt,
        ) -> uze_core::integration::AttachmentInspection {
            uze_core::integration::AttachmentInspection {
                state: AttachmentState::Matched,
                reason: "test".to_owned(),
            }
        }
        fn registrations(
            &self,
            _read: RegistrationRead,
        ) -> Vec<uze_core::integration::VendorRegistration> {
            vec![uze_core::integration::VendorRegistration {
                kind: VendorRegistrationKind::Marketplace,
                name: "uze-store".to_owned(),
                expected: self.home.vendor_store_dir("misregistered"),
                registered: Some(self.home.generated_attachments_dir("misregistered")),
            }]
        }
    }

    /// A machine with one plugin whose bytes are gone and a harness whose
    /// marketplace is registered elsewhere: two findings no record holds.
    fn damaged(name: &str) -> (UzeApplication, UzeHome, std::path::PathBuf) {
        let base = uze_testkit::temp::scratch(name);
        let home = UzeHome::at(base.join("home"));
        let app = UzeApplication::new(
            home.clone(),
            vec![Box::new(Misregistered { home: home.clone() })],
        );
        let plugin = base.join("flow");
        std::fs::create_dir_all(plugin.join("skills/flow")).unwrap();
        std::fs::write(plugin.join("plugin.json"), r#"{"name": "flow"}"#).unwrap();
        std::fs::write(plugin.join("skills/flow/SKILL.md"), "# Flow\n").unwrap();
        app.plugins()
            .add(
                uze_core::PackageSource::Local { path: plugin },
                &uze_core::trust::AlwaysTrust,
            )
            .unwrap();
        let installed = app.plugins().list().unwrap().remove(0);
        // A build a harness still reads is what keeps an entry whose bytes
        // are gone from being pruned as a ghost.
        state::record_receipt(
            &home,
            uze_core::integration::AttachmentReceipt {
                package_id: installed.id.clone(),
                resource_identity: Some(uze_core::integration::BUILD_IDENTITY.to_owned()),
                integration: "misregistered".to_owned(),
                artifact: ManagedArtifact::IntegrationOwned {
                    kind: uze_core::integration::BUILD_KIND.to_owned(),
                    selector: installed.id.clone(),
                    detail: BTreeMap::new(),
                },
            },
        )
        .unwrap();
        std::fs::remove_dir_all(&installed.store_path).unwrap();
        (app, home, base)
    }

    fn has(findings: &[HealthFinding], subject: Subject) -> bool {
        findings.iter().any(|finding| finding.subject == subject)
    }

    #[test]
    fn doctor_finds_missing_bytes_and_a_misplaced_registration_for_the_repair() {
        let (app, _home, base) = damaged("catalogue-findings");
        let report = app.health().report();
        for subject in [Subject::PackageBytes, Subject::Registrations] {
            let finding = report
                .health
                .iter()
                .find(|finding| finding.subject == subject)
                .unwrap_or_else(|| panic!("{subject:?}: {:?}", report.health));
            assert_eq!(finding.lane, Lane::Repair);
            assert_eq!(finding.action(), Some(REPAIR_COMMAND));
        }
        let _ = std::fs::remove_dir_all(base);
    }

    /// Every machine-wide record corrupted in turn: `doctor` still answers,
    /// names the record that does not read, and keeps every other check's
    /// findings.
    #[test]
    fn doctor_reports_every_check_whichever_record_is_corrupt() {
        let records: [(&str, fn(&UzeHome) -> std::path::PathBuf); 8] = [
            ("registry_path", UzeHome::registry_path),
            ("attachments_path", UzeHome::attachments_path),
            ("provisioning_state_path", UzeHome::provisioning_state_path),
            ("marketplaces_path", UzeHome::marketplaces_path),
            ("undelivered_path", UzeHome::undelivered_path),
            ("hosts_path", UzeHome::hosts_path),
            ("extensions_record_path", UzeHome::extensions_record_path),
            ("launch_overrides_path", UzeHome::launch_overrides_path),
        ];
        for (name, path) in records {
            let (app, home, base) = damaged(&format!("catalogue-corrupt-{name}"));
            std::fs::write(path(&home), "{ not a record").unwrap();
            let report = app.health().report();
            assert!(
                report.health.iter().any(
                    |finding| finding.subject == Subject::Records && finding.path == Some(name)
                ),
                "{name}: the corrupt record is not named: {:?}",
                report.health
            );
            assert!(
                has(&report.health, Subject::Registrations),
                "{name}: {:?}",
                report.health
            );
            // The registry is what says a package's bytes are missing.
            if name != "registry_path" {
                assert!(
                    has(&report.health, Subject::PackageBytes),
                    "{name}: {:?}",
                    report.health
                );
            }
            let _ = std::fs::remove_dir_all(base);
        }
    }

    /// A finding the repair lane holds names `uze doctor --repair` as what
    /// to run, whatever else it carries.
    #[test]
    fn a_repair_finding_names_the_repair() {
        let finding = HealthFinding::stale_endpoint(std::path::Path::new("/uze/state/terminal/s"));
        assert_eq!(finding.action(), Some(REPAIR_COMMAND));
        assert!(!finding.has_own_row());
    }
}
