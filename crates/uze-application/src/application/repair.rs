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
//! Every collector runs first, each under its own lock, and the home is
//! then compared with its schema (`schema`), before it is measured again
//! for plain `doctor` to show.
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
use super::collection::CollectionRecord;
use super::marketplace::{MarketplaceRequest, MirrorAt};
use super::schema::SchemaEntry;
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
    /// What the home holds that its schema does not account for, and what
    /// of it was removed.
    pub home: Vec<SchemaEntry>,
    /// The home measured once the repair was done, as plain `doctor` shows
    /// it from then on.
    pub measured: Option<CollectionRecord>,
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
    pub fn repair(
        &self,
        plan: RepairPlan,
        endpoint: &dyn TerminalEndpoint,
        discard_leftovers: bool,
    ) -> RepairReport {
        self.begin_operation();
        // Putting a pin back takes the lock itself, as plain maintenance
        // runs it.
        let restored = self
            .health()
            .restore_pins(super::marketplace::Fetch::Allowed);
        // Each collector takes its own lock, so all of them run before the
        // repair takes the mutation lock.
        let collected = self.health().collect();
        let mut tried: BTreeMap<FindingKey, RepairOutcome> = BTreeMap::new();
        let mut home = Vec::new();
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
                home = super::schema::compare(&self.home, discard_leftovers);
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
            home,
            measured: Some(CollectionRecord::measure(&self.home)),
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
            (
                ResolvedSource::Git {
                    commit,
                    subdirectory,
                    ..
                },
                Some(record),
            ) => {
                let entry = self.marketplace_entry(marketplace, &record.source, subdirectory);
                MarketplaceRequest::of(&record.source)
                    .map_err(|error| super::marketplace::CachedMiss::Unreached(Some(error)))
                    .and_then(|request| {
                        request.materialize_cached(
                            entry.as_deref().unwrap_or(plugin),
                            MirrorAt {
                                home: &self.home,
                                marketplace,
                                recent: None,
                                fetched: &self.mirrors_fetched,
                                catalogues: &self.marketplace_catalogues,
                            },
                            commit,
                            super::marketplace::Fetch::Allowed,
                        )
                    })
                    .map_err(|miss| match miss {
                        super::marketplace::CachedMiss::Unreached(Some(error)) => offline(error),
                        super::marketplace::CachedMiss::Unreached(None) => {
                            RepairOutcome::NeedsNetwork {
                                reason: "the commit is not in the marketplace's cached clone"
                                    .to_owned(),
                            }
                        }
                        // The marketplace answered, and what was installed
                        // is no longer in it: no network brings it back.
                        super::marketplace::CachedMiss::Absent(error) => RepairOutcome::NeedsHuman {
                            reason: format!(
                                "commit {} is no longer in its marketplace ({error}): update it, \
                                 or remove it with uze remove -m {}",
                                super::lifecycle::update::short_commit(commit),
                                installed.id.as_str()
                            ),
                            command: Some(format!("uze update {}", installed.id.as_str())),
                        },
                    })?
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

    /// The name `marketplace`'s manifest lists the plugin under whose bytes
    /// sit at `subdirectory` of its repository: a package is registered by
    /// the name its own manifest gives, which a marketplace may list under
    /// another. `None` when the catalogue cannot be read or names none.
    fn marketplace_entry(
        &self,
        marketplace: &str,
        source: &PackageSource,
        subdirectory: &Option<PathBuf>,
    ) -> Option<String> {
        let subdirectory = subdirectory.as_ref()?;
        let catalogue = self.marketplace_catalogues.read(marketplace, source).ok()?;
        catalogue
            .manifest
            .plugins
            .iter()
            .find(|entry| {
                uze_core::acquisition::marketplace::plugin_subdirectory(
                    &catalogue.manifest,
                    &entry.name,
                )
                .is_ok_and(|within| subdirectory.ends_with(within.trim_start_matches("./")))
            })
            .map(|entry| entry.name.clone())
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::Path;
    use std::sync::Mutex;

    use uze_core::integration::{
        HarnessDetection, IntegrationStatus, VendorRegistration, VendorRegistrationKind,
    };
    use uze_core::router::HarnessCapabilities;
    use uze_core::trust::AlwaysTrust;

    use super::*;
    use crate::application::catalogue::Severity;

    fn finding(subject: Subject, lane: Lane, about: &str) -> HealthFinding {
        HealthFinding {
            subject,
            lane,
            severity: Severity::Warning,
            about: about.to_owned(),
            detail: format!("{about} is not as it should be"),
            harness: None,
            location: None,
            path: None,
            command: None,
        }
    }

    fn entry(outcome: RepairOutcome) -> RepairEntry {
        RepairEntry {
            finding: HealthFinding::stale_endpoint(Path::new("endpoint")),
            outcome,
        }
    }

    fn report(outcomes: Vec<RepairOutcome>) -> RepairReport {
        RepairReport {
            entries: outcomes.into_iter().map(entry).collect(),
            ..RepairReport::default()
        }
    }

    fn human() -> RepairOutcome {
        RepairOutcome::NeedsHuman {
            reason: "yours to decide".to_owned(),
            command: None,
        }
    }

    fn network() -> RepairOutcome {
        RepairOutcome::NeedsNetwork {
            reason: "offline".to_owned(),
        }
    }

    fn unrepairable() -> RepairOutcome {
        RepairOutcome::Unrepairable {
            reason: "did not take".to_owned(),
        }
    }

    #[test]
    fn the_exit_status_says_healthy_waiting_on_the_person_or_only_on_the_network() {
        assert_eq!(report(Vec::new()).exit_status(), 0, "nothing to repair");
        assert_eq!(
            report(vec![RepairOutcome::Repaired]).exit_status(),
            0,
            "everything repaired"
        );
        assert_eq!(report(vec![network()]).exit_status(), 2);
        assert_eq!(report(vec![RepairOutcome::Repaired, network()]).exit_status(), 2);
        assert_eq!(report(vec![human()]).exit_status(), 1);
        assert_eq!(report(vec![unrepairable()]).exit_status(), 1);
        assert_eq!(
            report(vec![network(), human()]).exit_status(),
            1,
            "what waits on the person outranks the network"
        );
    }

    #[test]
    fn repaired_counts_only_the_findings_that_were_put_right() {
        assert_eq!(report(Vec::new()).repaired(), 0);
        let mixed = report(vec![
            RepairOutcome::Repaired,
            human(),
            RepairOutcome::Repaired,
            network(),
            unrepairable(),
        ]);
        assert_eq!(mixed.repaired(), 2);
    }

    #[test]
    fn a_delivery_waiting_on_its_bytes_stands_or_falls_with_them() {
        let receipts = finding(Subject::Receipts, Lane::Repair, "flow@local");
        let builds = finding(Subject::Builds, Lane::Repair, "flow@local");
        let flow_bytes = finding(Subject::PackageBytes, Lane::Repair, "flow@local");
        let other_bytes = finding(Subject::PackageBytes, Lane::Repair, "std@local");
        let planned = vec![receipts.clone(), other_bytes.clone(), flow_bytes.clone()];
        let tried = BTreeMap::from([
            (key_of(&other_bytes), RepairOutcome::Repaired),
            (key_of(&flow_bytes), network()),
        ]);

        assert_eq!(
            waits_on_bytes(&tried, &planned, &receipts),
            Some(&network()),
            "a receipt takes what came of its own package's bytes"
        );
        assert_eq!(waits_on_bytes(&tried, &planned, &builds), Some(&network()));

        let operator = finding(Subject::Receipts, Lane::Operator, "flow@local");
        assert_eq!(
            waits_on_bytes(&tried, &planned, &operator),
            None,
            "a finding the person resolves waits on no bytes"
        );
        let records = finding(Subject::Records, Lane::Repair, "flow@local");
        assert_eq!(
            waits_on_bytes(&tried, &planned, &records),
            None,
            "only what is built or delivered from the bytes waits on them"
        );
        let untried = finding(Subject::Receipts, Lane::Repair, "lone@local");
        assert_eq!(
            waits_on_bytes(&tried, &planned, &untried),
            None,
            "a package whose bytes were never planned waits on nothing"
        );
    }

    #[test]
    fn no_terminal_runtime_leaves_no_endpoint_to_clear() {
        assert_eq!(NoTerminalEndpoint.stale(), None);
        assert_eq!(NoTerminalEndpoint.clear(), Ok(false));
    }

    /// A terminal runtime whose endpoint is stale until it is cleared, or
    /// whose clearing is refused.
    struct StaleEndpoint {
        cleared: Cell<bool>,
        refuse: Option<&'static str>,
    }

    impl StaleEndpoint {
        fn clearing() -> Self {
            Self {
                cleared: Cell::new(false),
                refuse: None,
            }
        }

        fn refusing(reason: &'static str) -> Self {
            Self {
                cleared: Cell::new(false),
                refuse: Some(reason),
            }
        }
    }

    impl TerminalEndpoint for StaleEndpoint {
        fn stale(&self) -> Option<PathBuf> {
            (!self.cleared.get()).then(|| PathBuf::from("terminal.sock"))
        }

        fn clear(&self) -> std::result::Result<bool, String> {
            if let Some(reason) = self.refuse {
                return Err(reason.to_owned());
            }
            self.cleared.set(true);
            Ok(true)
        }
    }

    fn application(label: &str) -> (UzeApplication, PathBuf) {
        let base = uze_testkit::temp::scratch(label);
        let app = UzeApplication::new(UzeHome::at(base.join("home")), Vec::new());
        (app, base)
    }

    fn outcome_for(report: &RepairReport, subject: Subject) -> Vec<&RepairOutcome> {
        report
            .entries
            .iter()
            .filter(|entry| entry.finding.subject == subject)
            .map(|entry| &entry.outcome)
            .collect()
    }

    #[test]
    fn a_stale_terminal_endpoint_is_planned_and_cleared_by_the_runtime() {
        let (app, base) = application("repair-endpoint");
        let endpoint = StaleEndpoint::clearing();

        let plan = app.repair_plan(&endpoint);
        assert!(
            plan.findings
                .iter()
                .any(|finding| finding.subject == Subject::TerminalEndpoint),
            "the plan is observed live, endpoint included: {:?}",
            plan.findings
        );

        let report = app.repair(plan, &endpoint, false);
        assert!(endpoint.cleared.get(), "the runtime was asked to clear it");
        assert_eq!(
            outcome_for(&report, Subject::TerminalEndpoint),
            vec![&RepairOutcome::Repaired]
        );
        assert_eq!(report.exit_status(), 0);
        assert!(
            report.measured.is_some(),
            "the home is measured once the repair is done"
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn an_endpoint_the_runtime_will_not_clear_is_reported_once_with_its_reason() {
        let (app, base) = application("repair-endpoint-refused");
        let endpoint = StaleEndpoint::refusing("the workspace lock is held");

        let plan = app.repair_plan(&endpoint);
        let report = app.repair(plan, &endpoint, false);
        assert_eq!(
            outcome_for(&report, Subject::TerminalEndpoint),
            vec![&RepairOutcome::Unrepairable {
                reason: "the workspace lock is held".to_owned()
            }],
            "one entry per finding, carrying why the repair did not take"
        );
        assert_eq!(report.exit_status(), 1);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn missing_package_bytes_are_acquired_again_from_where_they_came() {
        let (app, base) = application("repair-bytes");
        let source = base.join("flow");
        fs::create_dir_all(source.join("skills/demo")).unwrap();
        fs::write(source.join("plugin.json"), r#"{"name": "flow"}"#).unwrap();
        fs::write(source.join("skills/demo/SKILL.md"), "# Demo\n").unwrap();
        app.plugins()
            .add(PackageSource::Local { path: source }, &AlwaysTrust)
            .unwrap();
        let installed = app.installed_packages().remove(0);
        fs::remove_dir_all(&installed.root).unwrap();

        let plan = app.repair_plan(&NoTerminalEndpoint);
        assert!(
            plan.findings
                .iter()
                .any(|finding| finding.subject == Subject::PackageBytes),
            "{:?}",
            plan.findings
        );
        let report = app.repair(plan, &NoTerminalEndpoint, false);
        assert_eq!(
            outcome_for(&report, Subject::PackageBytes),
            vec![&RepairOutcome::Repaired],
            "{:?}",
            report.entries
        );
        assert!(installed.root.join("plugin.json").is_file());
        fs::remove_dir_all(&base).ok();
    }

    /// A harness here and set up, holding one registration away from where
    /// UZE keeps it, and whose launcher has no real binary to start.
    struct Misplaced {
        registered: Mutex<PathBuf>,
        expected: PathBuf,
    }

    impl IntegrationPort for Misplaced {
        fn id(&self) -> &'static str {
            "misplaced"
        }
        fn capabilities(&self) -> HarnessCapabilities {
            HarnessCapabilities::default()
        }
        fn exposure_plan(
            &self,
            _resource: &uze_core::capability::Resource,
        ) -> uze_core::exposure::ExposurePlan {
            panic!("nothing is delivered")
        }
        fn detect(&self) -> HarnessDetection {
            HarnessDetection {
                present: true,
                version: None,
            }
        }
        fn status(&self, _home: &UzeHome) -> IntegrationStatus {
            IntegrationStatus::InstalledUnverified
        }
        fn supports_runtime_integration(&self) -> bool {
            true
        }
        fn shim_name(&self) -> &'static str {
            "uze-repair-fixture-without-binary"
        }
        fn registrations(&self, _read: RegistrationRead) -> Vec<VendorRegistration> {
            vec![VendorRegistration {
                kind: VendorRegistrationKind::Marketplace,
                name: "fixture".to_owned(),
                expected: self.expected.clone(),
                registered: Some(self.registered.lock().unwrap().clone()),
            }]
        }
        fn repoint_registration(&self, _registration: &VendorRegistration) -> Result<()> {
            *self.registered.lock().unwrap() = self.expected.clone();
            Ok(())
        }
    }

    #[test]
    fn a_misplaced_registration_is_moved_back_and_a_launcher_without_a_binary_says_why() {
        let base = uze_testkit::temp::scratch("repair-harness");
        let app = UzeApplication::new(
            UzeHome::at(base.join("home")),
            vec![Box::new(Misplaced {
                registered: Mutex::new(base.join("elsewhere")),
                expected: base.join("home/marketplace"),
            })],
        );

        let plan = app.repair_plan(&NoTerminalEndpoint);
        let report = app.repair(plan, &NoTerminalEndpoint, false);
        assert_eq!(
            outcome_for(&report, Subject::Registrations),
            vec![&RepairOutcome::Repaired],
            "{:?}",
            report.entries
        );
        let shims = outcome_for(&report, Subject::Shims);
        assert!(
            matches!(
                shims.as_slice(),
                [RepairOutcome::Unrepairable { reason }] if reason.contains("executable")
            ),
            "the launcher's own refusal is the reason given: {shims:?}"
        );
        fs::remove_dir_all(&base).ok();
    }
}
