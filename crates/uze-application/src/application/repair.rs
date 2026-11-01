//! `uze doctor --repair`: the explicit lane of the health catalogue.
//!
//! The plan is the catalogue observed live — never a cached `Matched` — and
//! the repair runs it in dependency order under the mutation lock (design
//! D5 of `doctor-repair`):
//!
//! 1. package bytes, acquired again from the provenance the registry
//!    recorded and kept only on the digest recorded at ingest;
//! 2. builds and every other receipt, through the automatic lane, which now
//!    finds bytes to build from;
//! 3. registrations, moved back to the vendor store;
//! 4. the terminal runtime's endpoint, through the runtime itself;
//! 5. shims.
//!
//! Then the catalogue observes again, and that observation is the report:
//! what is gone was repaired, and what is left says why. It never writes
//! over what someone changed, never changes a revision, never asks for or
//! widens trust, never touches the receipt ledger or an authorization, and
//! never reaches into a project.

use std::collections::BTreeMap;

use serde::Serialize;

use uze_core::{
    PackageSource, ResolvedSource,
    acquisition::MaterializedPackage,
    integration::RegistrationRead,
    persistence::MutationLock,
    state,
    store::{RecordedDigest, Restored},
};

use super::catalogue::{HealthFinding, Inspection, Lane, Subject};
use super::marketplace::{MarketplaceRequest, MirrorAt};
use super::*;

/// What the binary, which alone reaches the terminal runtime, says of the
/// runtime's endpoint: `uze-application` does not depend on the runtime.
pub trait TerminalEndpoint {
    /// The endpoint a server that is no longer running left, if one did.
    fn stale(&self) -> Option<PathBuf>;
    /// Clears it, under the runtime's own workspace lock; answers whether
    /// there was one to clear.
    fn clear(&self) -> std::result::Result<bool, String>;
}

/// No terminal runtime to ask: what a caller with none passes.
pub struct NoTerminalEndpoint;

impl TerminalEndpoint for NoTerminalEndpoint {
    fn stale(&self) -> Option<PathBuf> {
        None
    }
    fn clear(&self) -> std::result::Result<bool, String> {
        Ok(false)
    }
}

/// What came of one finding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum RepairOutcome {
    Repaired,
    /// Left for the person, with why and what to run.
    NeedsHuman {
        reason: String,
        command: Option<String>,
    },
    /// Put back once the network is there; the rest went on without it.
    NeedsNetwork {
        reason: String,
    },
    /// A repair was tried and did not take.
    Unrepairable {
        reason: String,
    },
}

/// The live findings a repair starts from.
#[derive(Clone, Debug, Default, Serialize)]
pub struct RepairPlan {
    pub findings: Vec<HealthFinding>,
}

/// One finding and what came of it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RepairEntry {
    pub finding: HealthFinding,
    #[serde(flatten)]
    pub outcome: RepairOutcome,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RepairReport {
    pub entries: Vec<RepairEntry>,
    /// What the automatic lane did on the way, beyond the plan's findings.
    pub maintenance: MaintenanceReport,
}

impl RepairReport {
    /// `0` when the machine is healthy, `1` when anything waits on the
    /// person or would not take, `2` when only the network is missing.
    pub fn exit_status(&self) -> i32 {
        let waits = |wanted: fn(&RepairOutcome) -> bool| {
            self.entries.iter().any(|entry| wanted(&entry.outcome))
        };
        if waits(|outcome| {
            matches!(
                outcome,
                RepairOutcome::NeedsHuman { .. } | RepairOutcome::Unrepairable { .. }
            )
        }) {
            1
        } else if waits(|outcome| matches!(outcome, RepairOutcome::NeedsNetwork { .. })) {
            2
        } else {
            0
        }
    }

    pub fn repaired(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.outcome == RepairOutcome::Repaired)
            .count()
    }
}

/// What identifies a finding across the observation before a repair and
/// the one after it.
type FindingKey = (
    Subject,
    String,
    Option<String>,
    Option<PathBuf>,
    Option<&'static str>,
);

fn key_of(finding: &HealthFinding) -> FindingKey {
    (
        finding.subject,
        finding.about.clone(),
        finding.harness.clone(),
        finding.location.clone(),
        finding.path,
    )
}

impl UzeApplication {
    /// Every finding, observed now: each receipt inspected live, each
    /// registration asked of its harness, and the terminal runtime's
    /// endpoint as `endpoint` reports it.
    #[tracing::instrument(name = "repair.plan", skip_all)]
    pub fn repair_plan(&self, endpoint: &dyn TerminalEndpoint) -> RepairPlan {
        RepairPlan {
            findings: self.observe_live(endpoint),
        }
    }

    fn observe_live(&self, endpoint: &dyn TerminalEndpoint) -> Vec<HealthFinding> {
        let reconciliations: Vec<_> = self
            .installed_packages()
            .iter()
            .map(|package| self.reconcile(package.id.as_str()))
            .collect();
        let mut findings = self.health().observe(Inspection::Live, &reconciliations);
        if let Some(stale) = endpoint.stale() {
            findings.push(HealthFinding::stale_endpoint(&stale));
        }
        findings
    }

    /// Runs `plan` in dependency order under the mutation lock, then
    /// observes again: each finding is reported repaired, or why not.
    #[tracing::instrument(name = "repair.run", skip_all)]
    pub fn repair(&self, plan: RepairPlan, endpoint: &dyn TerminalEndpoint) -> RepairReport {
        self.begin_operation();
        // Putting a pin back takes the lock itself, as plain maintenance
        // runs it.
        let restored = self.health().restore_pins();
        // Each collector takes its own lock, so all of them run before the
        // repair takes the mutation lock.
        let collected = self.health().collect();
        let mut tried: BTreeMap<FindingKey, RepairOutcome> = BTreeMap::new();
        let maintenance = match MutationLock::acquire(&self.home) {
            Ok(_mutation) => {
                for finding in plan.findings.iter().filter(|f| f.lane == Lane::Repair) {
                    if finding.subject == Subject::PackageBytes {
                        tried.insert(key_of(finding), self.reacquire(&finding.about));
                    }
                }
                let maintenance = self.health().maintain_locked(restored, Inspection::Live);
                for finding in plan.findings.iter().filter(|f| f.lane == Lane::Repair) {
                    let outcome = match finding.subject {
                        Subject::Registrations => self.repoint(&finding.about),
                        Subject::TerminalEndpoint => match endpoint.clear() {
                            Ok(_) => None,
                            Err(reason) => Some(RepairOutcome::Unrepairable { reason }),
                        },
                        Subject::Shims => self.place_shim(&finding.about),
                        _ => None,
                    };
                    if let Some(outcome) = outcome {
                        tried.insert(key_of(finding), outcome);
                    }
                }
                maintenance
            }
            Err(error) => {
                for finding in &plan.findings {
                    tried.insert(
                        key_of(finding),
                        RepairOutcome::NeedsHuman {
                            reason: format!("nothing was repaired: {error}"),
                            command: Some(super::catalogue::REPAIR_COMMAND.to_owned()),
                        },
                    );
                }
                MaintenanceReport::default()
            }
        };
        self.inspection_cache.invalidate();
        // The repair wrote what the harnesses read itself, so what each
        // vendor CLI answered a moment ago no longer stands.
        uze_integrations::forget_vendor_answers();

        let after = self.observe_live(endpoint);
        let mut entries: Vec<RepairEntry> = plan
            .findings
            .iter()
            .map(|before| {
                let still = after
                    .iter()
                    .find(|now| key_of(now) == key_of(before))
                    .cloned();
                let outcome = match &still {
                    None => RepairOutcome::Repaired,
                    Some(now) => tried
                        .get(&key_of(before))
                        .or_else(|| waits_on_bytes(&tried, &plan.findings, now))
                        .cloned()
                        .unwrap_or_else(|| left_as_it_is(now)),
                };
                RepairEntry {
                    finding: still.unwrap_or_else(|| before.clone()),
                    outcome,
                }
            })
            .collect();
        // What the repair itself turned up, or what only showed once
        // something before it was put back.
        entries.extend(
            after
                .iter()
                .filter(|now| {
                    !plan
                        .findings
                        .iter()
                        .any(|before| key_of(before) == key_of(now))
                })
                .map(|now| RepairEntry {
                    finding: now.clone(),
                    outcome: left_as_it_is(now),
                }),
        );
        let mut maintenance = maintenance;
        maintenance.collected = collected;
        RepairReport {
            entries,
            maintenance,
        }
    }

    /// Acquires the bytes of `package` again from the provenance the
    /// registry recorded, keeping them only on the digest recorded at
    /// ingest. The bundled plugin comes from this binary's own snapshot,
    /// offline, and records the snapshot's digest. Asks for no trust: bytes
    /// that match are the bytes that were authorized.
    fn reacquire(&self, package: &str) -> RepairOutcome {
        let installed = match self.package_by_name(package) {
            Ok(installed) => installed,
            Err(error) => {
                return RepairOutcome::Unrepairable {
                    reason: error.to_string(),
                };
            }
        };
        let update = Some(format!("uze update {}", installed.id.as_str()));
        let (materialized, digest) = match self.acquire_again(&installed) {
            Ok(acquired) => acquired,
            Err(outcome) => return outcome,
        };
        match self
            .store
            .restore_bytes(&installed.id, &materialized, digest)
        {
            Ok(Restored::Restored | Restored::AlreadyThere) => RepairOutcome::Repaired,
            Ok(Restored::DigestDiffers { .. }) => RepairOutcome::NeedsHuman {
                reason: "where it came from now holds other bytes than the ones installed; \
                         every harness keeps what was delivered of it"
                    .to_owned(),
                command: update,
            },
            Ok(Restored::NothingToCompare) => RepairOutcome::NeedsHuman {
                reason: "no digest was recorded when it was installed, so the bytes found \
                         cannot be proven to be the same"
                    .to_owned(),
                command: update,
            },
            Err(error) => RepairOutcome::Unrepairable {
                reason: error.to_string(),
            },
        }
    }

    /// A fresh acquisition of `installed`'s recorded source: the binary's
    /// snapshot, the marketplace's clone at the recorded commit, the linked
    /// checkout, or the local path, as the registry says it came.
    fn acquire_again(
        &self,
        installed: &StoredPackage,
    ) -> std::result::Result<(MaterializedPackage, RecordedDigest), RepairOutcome> {
        let plugin = installed.id.plugin_name();
        let marketplace = installed.id.marketplace();
        let gone = |reason: String| RepairOutcome::NeedsHuman {
            reason,
            command: Some(format!("uze remove -m {}", installed.id.as_str())),
        };
        let offline = |error: UzeError| RepairOutcome::NeedsNetwork {
            reason: error.to_string(),
        };
        if let PackageSource::Embedded { id } = &installed.provenance.requested {
            return crate::bootstrap::materialize(id)
                .map(|materialized| (materialized, RecordedDigest::Replaced))
                .map_err(|error| RepairOutcome::Unrepairable {
                    reason: error.to_string(),
                });
        }
        let record = state::marketplace_get(&self.home, marketplace)
            .ok()
            .flatten();
        let materialized = match (&installed.provenance.resolved, record) {
            (ResolvedSource::Local { .. }, Some(record)) if record.link.is_some() => {
                let checkout = record.link.clone().unwrap_or_default();
                MarketplaceRequest::of(&record.source)
                    .and_then(|request| request.materialize_from_link(plugin, &checkout))
                    .map_err(|error| {
                        gone(format!(
                            "the checkout it is linked to cannot be read: {error}"
                        ))
                    })?
            }
            (ResolvedSource::Local { path }, _) => {
                uze_core::acquisition::acquire(&PackageSource::Local { path: path.clone() })
                    .map_err(|error| gone(format!("where it came from cannot be read: {error}")))?
            }
            (ResolvedSource::Git { commit, .. }, Some(record)) => {
                MarketplaceRequest::of(&record.source)
                    .and_then(|request| {
                        request.materialize_cached(
                            plugin,
                            MirrorAt {
                                home: &self.home,
                                marketplace,
                                recent: None,
                                fetched: &self.mirrors_fetched,
                                catalogues: &self.marketplace_catalogues,
                            },
                            commit,
                        )
                    })
                    .map_err(offline)?
            }
            (
                ResolvedSource::Git {
                    url,
                    commit,
                    subdirectory,
                },
                None,
            ) => uze_core::acquisition::acquire(&PackageSource::Git {
                url: url.clone(),
                reference: Some(commit.clone()),
                subdirectory: subdirectory.clone(),
            })
            .map_err(offline)?,
            (ResolvedSource::Embedded { .. }, _) => {
                return Err(RepairOutcome::Unrepairable {
                    reason: "an embedded package was recorded with another request".to_owned(),
                });
            }
        };
        Ok((materialized, RecordedDigest::Kept))
    }

    /// Moves the registrations of the harness `about` names back to where
    /// UZE keeps what they name.
    fn repoint(&self, about: &str) -> Option<RepairOutcome> {
        let integration = self.integration_named(about)?;
        integration
            .registrations(RegistrationRead::Live)
            .into_iter()
            .filter(|registration| registration.misplaced())
            .find_map(|registration| {
                integration
                    .repoint_registration(&registration)
                    .err()
                    .map(|error| RepairOutcome::Unrepairable {
                        reason: error.to_string(),
                    })
            })
    }

    /// Places the launcher of the harness `about` names again.
    fn place_shim(&self, about: &str) -> Option<RepairOutcome> {
        let integration = self.integration_named(about)?;
        self.ensure_runtime_shim(integration, None)
            .err()
            .map(|error| RepairOutcome::Unrepairable {
                reason: error.to_string(),
            })
    }
}

/// What came of the bytes a delivery's finding waits on: a receipt that
/// cannot be judged while its package's bytes are gone stands or falls with
/// them.
fn waits_on_bytes<'a>(
    tried: &'a BTreeMap<FindingKey, RepairOutcome>,
    planned: &[HealthFinding],
    finding: &HealthFinding,
) -> Option<&'a RepairOutcome> {
    if finding.lane != Lane::Repair
        || !matches!(finding.subject, Subject::Receipts | Subject::Builds)
    {
        return None;
    }
    planned
        .iter()
        .find(|bytes| bytes.subject == Subject::PackageBytes && bytes.about == finding.about)
        .and_then(|bytes| tried.get(&key_of(bytes)))
}

/// What a finding still standing after a repair is: the person's when its
/// lane is theirs, otherwise a repair that did not take.
fn left_as_it_is(finding: &HealthFinding) -> RepairOutcome {
    match finding.lane {
        Lane::Operator => RepairOutcome::NeedsHuman {
            reason: finding.detail.clone(),
            command: finding.command.clone(),
        },
        Lane::Automatic | Lane::Repair => RepairOutcome::Unrepairable {
            reason: finding.detail.clone(),
        },
    }
}
