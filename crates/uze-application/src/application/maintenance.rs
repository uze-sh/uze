//! Safe machine-level desired-state convergence for `doctor` and the TUI.
//!
//! This is deliberately narrower than install/update: it neither acquires
//! bytes nor expands trust. A receipt must first inspect as `Missing`, then
//! the owning integration must prove it can rebuild that exact artifact.

use std::collections::BTreeSet;

use serde::Serialize;

use uze_core::{
    harness_runtime,
    integration::{AttachmentState, ManagedArtifact, is_build_receipt},
    persistence::MutationLock,
    state,
};

use super::catalogue::Inspection;
use super::collection::{Collectable, Collected};

use super::lifecycle::remove::ReceiptTeardown;
use super::lifecycle::update::PinRestoration;
use super::services::Health;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MaintenanceOutcome {
    Repaired {
        plugin: String,
        integration: String,
        receipt: String,
    },
    /// Receipts found under a package id nothing in the Store answers to
    /// any more — not "missing an artifact" (that is `Repaired`'s job),
    /// but evidence for a package that is simply gone (removed, or
    /// renamed out from under them, e.g. this project's own
    /// marketplace-qualification: `flow` became `flow@ai`, orphaning
    /// every `flow`-keyed receipt). Detached and forgotten the same way
    /// `Plugins::remove` would, and only when that path is fully `Safe`
    /// (see `reconcile_orphaned_receipts`) — never for a foreign or
    /// ambiguous state.
    OrphanCleaned {
        plugin: String,
        ledger_keys: Vec<String>,
    },
    UpdateAvailable {
        plugin: String,
    },
    /// A plugin whose marketplace was unlinked holds the commit it was
    /// pinned to before the link again, read from the cached clone.
    PinRestored {
        plugin: String,
        commit: String,
    },
    /// What putting a pre-link commit back needs is not on this machine:
    /// the build from the checkout stays until the network is there.
    NeedsNetwork {
        plugin: String,
        reason: String,
    },
    /// A reference UZE wrote into a shared discovery root that points into
    /// `$UZE_HOME` at something gone, which no receipt claims. Removed
    /// rather than reported for a person to judge: it delivers nothing, it
    /// can only be UZE's — nothing else writes inside `$UZE_HOME` — and
    /// while it sits there it holds a name another package may need.
    DanglingReferenceRemoved {
        path: std::path::PathBuf,
    },
    NeedsHumanAction {
        plugin: String,
        integration: Option<String>,
        receipt: Option<String>,
        state: Option<AttachmentState>,
        reason: String,
    },
    Unavailable {
        integration: String,
        reason: String,
    },
}

/// One sentence per outcome, for the reports a person reads.
///
/// `doctor` used to render these through `Debug`, so the operator was handed
/// `NeedsHumanAction { plugin: "flow@ai", integration: None, receipt: None,
/// state: None, reason: "…" }` — a struct dump in which the only part that
/// was for them, `reason`, was the hardest to find.
impl std::fmt::Display for MaintenanceOutcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaintenanceOutcome::Repaired {
                plugin,
                integration,
                receipt,
            } => write!(
                formatter,
                "repaired {plugin} on {integration} (receipt {receipt})"
            ),
            MaintenanceOutcome::OrphanCleaned {
                plugin,
                ledger_keys,
            } => write!(
                formatter,
                "forgot {} orphaned receipt(s) left by {plugin}: {}",
                ledger_keys.len(),
                ledger_keys.join(", ")
            ),
            MaintenanceOutcome::DanglingReferenceRemoved { path } => write!(
                formatter,
                "removed {} — it pointed into UZE's own home at something gone and no receipt \
                 claimed it",
                path.display()
            ),
            MaintenanceOutcome::UpdateAvailable { plugin } => {
                write!(formatter, "{plugin} has an update available")
            }
            MaintenanceOutcome::PinRestored { plugin, commit } => write!(
                formatter,
                "put {plugin} back at {commit}, the commit it was pinned to before the link"
            ),
            MaintenanceOutcome::NeedsNetwork { reason, .. } => write!(formatter, "{reason}"),
            MaintenanceOutcome::NeedsHumanAction {
                plugin,
                integration,
                reason,
                ..
            } => match integration {
                Some(integration) => {
                    write!(formatter, "{plugin} on {integration} needs you: {reason}")
                }
                None => write!(formatter, "{plugin} needs you: {reason}"),
            },
            MaintenanceOutcome::Unavailable {
                integration,
                reason,
            } => write!(formatter, "{integration} could not be checked: {reason}"),
        }
    }
}

/// The runtime projections, by whether their project is still there
/// (`harness_runtime::prune_projections`), and anything under `runtime/`
/// that is none of its tenants. Takes no lock, as
/// [`Health::prune_runtime_projections`] says why.
pub(crate) struct Projections;

impl Collectable for Projections {
    fn name(&self) -> &'static str {
        "projections"
    }

    fn roots(&self) -> &'static [&'static str] {
        &["runtime_dir", "runtime_project_dir"]
    }

    fn collect(&self, home: &uze_core::UzeHome) -> Collected {
        Collected::removed(self.name(), harness_runtime::prune_projection_paths(home))
    }
}

/// What each integration generated inside its own runtime projections, by
/// that integration's rule (`IntegrationPort::collect_runtime`): Codex's
/// role files, kept while the project still has their agent. Takes no
/// lock, as a launch refreshing a projection takes none.
pub(crate) struct IntegrationProjections<'a>(
    pub(crate) &'a [Box<dyn uze_core::integration::IntegrationPort>],
);

impl Collectable for IntegrationProjections<'_> {
    fn name(&self) -> &'static str {
        "harness projections"
    }

    fn roots(&self) -> &'static [&'static str] {
        &["runtime_projection_dir"]
    }

    fn collect(&self, home: &uze_core::UzeHome) -> Collected {
        Collected::removed(
            self.name(),
            self.0
                .iter()
                .flat_map(|integration| integration.collect_runtime(home))
                .collect(),
        )
    }
}

/// The context documents of conversations that ended, by the workspace's
/// rule (`conversation::collect_session_documents`): kept while an agent's
/// record names their conversation. Takes no lock: a launch records its
/// conversation before it writes the document.
pub(crate) struct ConversationSessions;

impl Collectable for ConversationSessions {
    fn name(&self) -> &'static str {
        "conversation sessions"
    }

    fn roots(&self) -> &'static [&'static str] {
        &["session_context_dir"]
    }

    fn collect(&self, home: &uze_core::UzeHome) -> Collected {
        Collected::removed(
            self.name(),
            uze_workspace::conversation::collect_session_documents(home),
        )
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct MaintenanceReport {
    pub outcomes: Vec<MaintenanceOutcome>,
    /// What each collector removed on the way.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub collected: Vec<super::Collected>,
}

impl MaintenanceReport {
    pub fn repaired_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| {
                matches!(
                    outcome,
                    MaintenanceOutcome::Repaired { .. }
                        | MaintenanceOutcome::OrphanCleaned { .. }
                        | MaintenanceOutcome::PinRestored { .. }
                )
            })
            .count()
    }
}

impl Health<'_> {
    /// Sweeps the runtime projections of projects that no longer exist —
    /// most of them the checkout of an agent UZE placed, deleted when its
    /// work was delivered or discarded.
    ///
    /// Machine-scoped like the rest of this module, and deliberately not
    /// hung off the checkout lifecycle that produces the garbage: that
    /// would put a machine-state write inside a project-scoped operation
    /// (ADR-019), to save kilobytes for the remainder of one session. The
    /// sweep is a `readdir` and a `stat` per project, so the moment it runs
    /// is free to be whichever one is cheapest to reason about — the
    /// client's first occupancy pass — rather than whichever is soonest.
    ///
    /// Takes no mutation lock: it touches nothing the Store, the ledger or
    /// any receipt describes, and a projection it raced would be rebuilt by
    /// its own next launch.
    #[tracing::instrument(name = "health.prune_runtime_projections", skip_all)]
    pub fn prune_runtime_projections(&self) -> Vec<String> {
        Projections
            .collect(&self.0.home)
            .removed
            .iter()
            .filter_map(|path| Some(path.file_name()?.to_string_lossy().into_owned()))
            .collect()
    }

    /// Bounded, local maintenance used by health presenters. It only repairs
    /// receipt-proven missing artifacts and stale derived views. Every other
    /// state remains evidence for a person to decide on.
    #[tracing::instrument(name = "health.maintain", skip_all)]
    pub fn maintain(&self) -> MaintenanceReport {
        // Before the lock, which putting a pin back takes itself; asking
        // nobody, so a pinned commit that would run something new waits for
        // a command that can ask.
        let restored = self.restore_pins();
        // Before the lock too: each collector takes its own.
        let collected = self.collect();
        let Ok(_mutation) = MutationLock::acquire(&self.0.home) else {
            return MaintenanceReport {
                outcomes: vec![MaintenanceOutcome::Unavailable {
                    integration: "environment".to_owned(),
                    reason: "another UZE mutation is in progress".to_owned(),
                }],
                collected,
            };
        };
        let mut report = self.maintain_locked(restored, Inspection::Cached);
        report.collected = collected;
        report
    }

    /// The pass [`maintain`](Self::maintain) makes once it holds the
    /// mutation lock, for a caller already holding it: a repair, which
    /// inspects every receipt live rather than trusting a cached `Matched`.
    pub(crate) fn maintain_locked(
        &self,
        restored: Vec<MaintenanceOutcome>,
        inspection: Inspection,
    ) -> MaintenanceReport {
        let mut report = MaintenanceReport {
            outcomes: restored,
            collected: Vec::new(),
        };
        // Prunes any Store registration whose directory is gone and cleans
        // every receipt left behind under an id nothing answers to any
        // more — first, so `installed_packages` below never treats a
        // ghost registration as real.
        report.outcomes.extend(self.reconcile_orphaned_receipts());
        let packages = match self.0.installed_packages_checked() {
            Ok(packages) => Some(packages),
            Err(error) => {
                report.outcomes.push(MaintenanceOutcome::Unavailable {
                    integration: "environment".to_owned(),
                    reason: format!("the installed plugins could not be read: {error}"),
                });
                None
            }
        };

        if let Some(packages) = &packages {
            report.outcomes.extend(
                self.0
                    .republish_unpublished(packages)
                    .into_iter()
                    .filter_map(|outcome| {
                        Some(MaintenanceOutcome::Unavailable {
                            reason: outcome.error?,
                            integration: outcome.integration,
                        })
                    }),
            );
        }

        let ledger = state::receipts(&self.0.home, None);
        for package in packages.iter().flatten() {
            if let uze_core::PackageSource::Embedded { id } = &package.provenance.requested
                && crate::bootstrap::has_update(id, &package.root).unwrap_or(false)
            {
                report.outcomes.push(MaintenanceOutcome::UpdateAvailable {
                    plugin: package.id.as_str().to_owned(),
                });
            }

            let entries: Vec<_> = match &ledger {
                // Builds first: what is delivered inside one is written
                // again once the build it sits in is.
                Ok(receipts) => {
                    let mut entries: Vec<_> = receipts
                        .iter()
                        .filter(|receipt| receipt.package_id == package.id.as_str())
                        .cloned()
                        .collect();
                    entries.sort_by_key(|receipt| !is_build_receipt(receipt));
                    entries
                }
                Err(error) => {
                    report.outcomes.push(MaintenanceOutcome::NeedsHumanAction {
                        plugin: package.id.as_str().to_owned(),
                        integration: None,
                        receipt: None,
                        state: Some(AttachmentState::Blocked),
                        reason: error.to_string(),
                    });
                    continue;
                }
            };

            // Maintenance only probes receipt kinds it knows how to restore
            // exactly: what a receipt carries whole, a build, rendered again
            // from the package's bytes, and what a delivery wrote inside a
            // vendor store, attached again from them. Other attachments are
            // still inspected once by the subsequent doctor report, avoiding
            // a second vendor CLI call on every anomalous report.
            let in_store = |receipt: &uze_core::integration::AttachmentReceipt| {
                !is_build_receipt(receipt)
                    && receipt.resource_identity.is_some()
                    && receipt
                        .artifact
                        .location()
                        .starts_with(self.0.home.store_dir())
            };
            for receipt in entries.into_iter().filter(|receipt| {
                matches!(
                    receipt.artifact,
                    ManagedArtifact::SymlinkReference { .. }
                        | ManagedArtifact::ManagedTextRegion { .. }
                        | ManagedArtifact::GeneratedFile { .. }
                ) || is_build_receipt(receipt)
                    || in_store(receipt)
            }) {
                let build = is_build_receipt(&receipt);
                // What is built from a package whose bytes are gone is kept
                // as it is until they return: that is the repair's to bring
                // back, not this.
                if (build || in_store(&receipt)) && !package.root.is_dir() {
                    continue;
                }
                let ledger_key = receipt.cache_key();
                let Some(integration) = self
                    .0
                    .integrations
                    .iter()
                    .find(|candidate| candidate.id() == receipt.integration)
                else {
                    report.outcomes.push(MaintenanceOutcome::NeedsHumanAction {
                        plugin: package.id.as_str().to_owned(),
                        integration: Some(receipt.integration.clone()),
                        receipt: Some(ledger_key),
                        state: Some(AttachmentState::Blocked),
                        reason: "the receipt's integration is unavailable".to_owned(),
                    });
                    continue;
                };
                // A build's inspection digests every file it holds, so a
                // plain pass takes a fresh `Matched` from the cache the
                // report fills; a repair never does.
                let inspection = match inspection {
                    Inspection::Cached if build => self
                        .0
                        .inspect_receipt_cached(&receipt, integration.as_ref()),
                    _ => integration.inspect_receipt(&receipt),
                };
                let repaired = |receipt: &uze_core::integration::AttachmentReceipt| {
                    if build {
                        self.0
                            .publish_build(package, integration.as_ref())
                            .map(|_| true)
                    } else if in_store(receipt) {
                        self.attach_again(package, integration.as_ref(), receipt)
                    } else {
                        integration.repair_missing_receipt(receipt)
                    }
                };
                match inspection.state {
                    AttachmentState::Matched => {}
                    AttachmentState::Missing => match repaired(&receipt) {
                        Ok(true) => {
                            // A build rendered again is owned by the receipt
                            // that rendering recorded.
                            let current = if build {
                                state::receipts(&self.0.home, Some(package.id.as_str()))
                                    .ok()
                                    .and_then(|receipts| {
                                        receipts.into_iter().find(|candidate| {
                                            candidate.integration == receipt.integration
                                                && is_build_receipt(candidate)
                                        })
                                    })
                                    .unwrap_or_else(|| receipt.clone())
                            } else {
                                receipt.clone()
                            };
                            let after = integration.inspect_receipt(&current);
                            if after.state == AttachmentState::Matched {
                                report.outcomes.push(MaintenanceOutcome::Repaired {
                                    plugin: package.id.as_str().to_owned(),
                                    integration: integration.id().to_owned(),
                                    receipt: ledger_key,
                                });
                            } else {
                                report.outcomes.push(MaintenanceOutcome::Unavailable {
                                    integration: integration.id().to_owned(),
                                    reason: format!(
                                        "repair of `{}` did not converge: {}",
                                        ledger_key, after.reason
                                    ),
                                });
                            }
                        }
                        Ok(false) => report.outcomes.push(MaintenanceOutcome::NeedsHumanAction {
                            plugin: package.id.as_str().to_owned(),
                            integration: Some(integration.id().to_owned()),
                            receipt: Some(ledger_key),
                            state: Some(AttachmentState::Missing),
                            reason: format!(
                                "{}; this attachment cannot be safely rebuilt from its receipt",
                                inspection.reason
                            ),
                        }),
                        Err(error) => report.outcomes.push(MaintenanceOutcome::Unavailable {
                            integration: integration.id().to_owned(),
                            reason: format!("repair of `{ledger_key}` failed: {error}"),
                        }),
                    },
                    state @ (AttachmentState::Drifted
                    | AttachmentState::Conflict
                    | AttachmentState::Blocked) => {
                        report.outcomes.push(MaintenanceOutcome::NeedsHumanAction {
                            plugin: package.id.as_str().to_owned(),
                            integration: Some(receipt.integration),
                            receipt: Some(ledger_key),
                            state: Some(state),
                            reason: inspection.reason,
                        });
                    }
                }
            }
        }

        for reference in self.dangling_references() {
            match uze_core::leftovers::remove_dangling(&self.0.home, &reference) {
                Ok(true) => report
                    .outcomes
                    .push(MaintenanceOutcome::DanglingReferenceRemoved {
                        path: reference.path,
                    }),
                // It stopped being dangling between the sweep and the
                // removal, which is the re-check doing its job.
                Ok(false) => {}
                Err(error) => report.outcomes.push(MaintenanceOutcome::Unavailable {
                    integration: "environment".to_owned(),
                    reason: format!("{} could not be removed: {error}", reference.path.display()),
                }),
            }
        }

        if report.repaired_count() > 0 {
            self.0.inspection_cache.invalidate();
        }
        report
    }

    /// Attaches again the resource a missing receipt inside a vendor store
    /// delivered, from the package's bytes, recording what it attached:
    /// the store is UZE's own tier, so nothing there is anyone else's to
    /// keep. `false` when the package no longer carries that resource.
    fn attach_again(
        &self,
        package: &uze_core::store::StoredPackage,
        integration: &dyn uze_core::integration::IntegrationPort,
        receipt: &uze_core::integration::AttachmentReceipt,
    ) -> uze_core::Result<bool> {
        let resources = uze_core::engine::package_resources(package)?;
        let Some(resource) = resources
            .iter()
            .find(|resource| receipt.resource_identity.as_deref() == Some(&resource.identity()))
        else {
            return Ok(false);
        };
        let Some(attached) = integration.attach_receipt(resource)? else {
            return Ok(false);
        };
        state::record_receipt(&self.0.home, attached)?;
        Ok(true)
    }

    /// Puts back the commit each plugin of an unlinked marketplace was
    /// pinned to before the link ([`PinRestoration`]). A pin whose commit
    /// would run something new, or whose detach is refused, is left for an
    /// update to settle.
    pub(crate) fn restore_pins(&self) -> Vec<MaintenanceOutcome> {
        let pending = state::marketplace_list(&self.0.home).is_ok_and(|marketplaces| {
            marketplaces
                .values()
                .any(|record| record.link.is_none() && !record.pinned_before_link.is_empty())
        });
        if !pending {
            return Vec::new();
        }
        let authority = uze_core::trust::NoTrustAuthority;
        self.0
            .installed_packages()
            .iter()
            .filter_map(|package| {
                let restoration = self.0.plugins().restore_pinned(package, &authority)?;
                let plugin = package.id.as_str().to_owned();
                Some(match restoration {
                    Ok(PinRestoration::Restored {
                        report: crate::application::UpdatePluginReport::Updated { .. },
                        commit,
                    }) => MaintenanceOutcome::PinRestored {
                        plugin,
                        commit: super::lifecycle::update::short_commit(&commit).to_owned(),
                    },
                    Ok(PinRestoration::Restored { .. }) => MaintenanceOutcome::NeedsHumanAction {
                        plugin,
                        integration: None,
                        receipt: None,
                        state: Some(AttachmentState::Drifted),
                        reason: "the commit pinned before the link was not put back: managed \
                                 state has drifted"
                            .to_owned(),
                    },
                    Ok(PinRestoration::NeedsNetwork(error)) => MaintenanceOutcome::NeedsNetwork {
                        plugin,
                        reason: error.to_string(),
                    },
                    Err(error) => MaintenanceOutcome::NeedsHumanAction {
                        plugin,
                        integration: None,
                        receipt: None,
                        state: None,
                        reason: format!(
                            "the commit pinned before the link was not put back: {error}"
                        ),
                    },
                })
            })
            .collect()
    }

    /// First prunes any Store registration whose directory is gone
    /// (`UzeStore::prune_ghost_registrations`), then detaches and forgets
    /// every receipt keyed by a package id nothing currently installed
    /// answers to — evidence for a package that no longer exists, most
    /// often left behind by a rename (a store id format change, or a
    /// plugin reinstalled under a different marketplace) rather than an
    /// explicit remove. Left alone, a physical slot an orphan still
    /// occupies (e.g. a shared-root Skill entry, keyed by stable label
    /// rather than the full qualified id) permanently blocks the live
    /// package's own attach with a projection conflict — this is what
    /// lets that resolve itself instead of requiring a person to notice
    /// and hand-clean the ledger.
    ///
    /// Reuses exactly the safety rule `Plugins::remove` enforces: an orphan
    /// is only ever touched when every one of its receipts reconciles as
    /// cleanly `Safe` to remove (see `plan_remove`) — `Drifted`,
    /// `Conflict`, `Blocked`, or an unrecoverable ledger all fall through
    /// to `NeedsHumanAction` untouched, same as a manual remove would.
    pub(crate) fn reconcile_orphaned_receipts(&self) -> Vec<MaintenanceOutcome> {
        let mut outcomes = Vec::new();
        // A registry entry is the Store's sole claim that a package is
        // installed; prune false claims before anything below asks it.
        // An entry a harness still reads a build of is missing its bytes, not
        // a ghost: it keeps its builds, its catalogue entries and its
        // receipts until the bytes return.
        // A ledger that cannot be read has not said which have builds, so
        // nothing is pruned on its word.
        let built: Option<BTreeSet<String>> =
            state::receipts(&self.0.home, None).ok().map(|receipts| {
                receipts
                    .into_iter()
                    .filter(uze_core::integration::is_build_receipt)
                    .map(|receipt| receipt.package_id)
                    .collect()
            });
        if let Some(built) = built
            && let Ok(pruned) = self
                .0
                .store
                .prune_ghost_registrations(|id| built.contains(id.as_str()))
            && !pruned.is_empty()
        {
            self.0.inspection_cache.invalidate();
        }
        // Read strictly: an unreadable registry would make every receipt look
        // orphaned, and each would then be detached.
        let Ok(installed) = self.0.installed_packages_checked() else {
            return outcomes;
        };
        let known_ids: BTreeSet<String> = installed
            .into_iter()
            .map(|package| package.id.as_str().to_owned())
            .collect();

        let Ok(all_receipts) = state::receipts(&self.0.home, None) else {
            // The ledger-read failure is already surfaced elsewhere in the
            // report this feeds into; nothing new to say about it here.
            return outcomes;
        };
        let orphan_ids: BTreeSet<String> = all_receipts
            .into_iter()
            .map(|receipt| receipt.package_id)
            // What setup delivered on its own account is no package's, and
            // so never one whose package is gone.
            .filter(|id| id != state::SETUP_OWNER && !known_ids.contains(id.as_str()))
            .collect();

        for package_id in orphan_ids {
            let (detached_receipts, final_report) = match self.0.detach_owned_receipts(&package_id)
            {
                Ok(ReceiptTeardown::Detached {
                    detached_receipts,
                    final_report,
                    ..
                }) => (detached_receipts, final_report),
                Ok(ReceiptTeardown::Refused { .. }) => {
                    outcomes.push(MaintenanceOutcome::NeedsHumanAction {
                        plugin: package_id,
                        integration: None,
                        receipt: None,
                        state: None,
                        reason: "orphaned receipts exist for a package no longer installed, \
                                 and are not all safely removable automatically"
                            .to_owned(),
                    });
                    continue;
                }
                Ok(ReceiptTeardown::Incomplete { .. }) | Err(_) => {
                    outcomes.push(MaintenanceOutcome::NeedsHumanAction {
                        plugin: package_id,
                        integration: None,
                        receipt: None,
                        state: None,
                        reason: "orphaned receipts did not fully detach".to_owned(),
                    });
                    continue;
                }
            };
            // A detached receipt was forgotten as it went; what is left
            // are the ones already missing before the teardown began.
            let mut ledger_keys = detached_receipts;
            ledger_keys.extend(
                final_report
                    .receipts
                    .iter()
                    .filter(|reconciled| {
                        state::forget_receipt(&self.0.home, &reconciled.receipt).is_ok()
                    })
                    .map(|reconciled| reconciled.ledger_key.clone()),
            );
            if !ledger_keys.is_empty() {
                outcomes.push(MaintenanceOutcome::OrphanCleaned {
                    plugin: package_id,
                    ledger_keys,
                });
            }
        }
        outcomes
    }
}

/// Each case restores a receipt-owned symlink, which a Windows account
/// without the right to create one never gets (delivery copies there).
#[cfg(all(test, unix))]
mod tests {
    use std::{fs, path::PathBuf};

    use crate::UzeApplication;
    use uze_core::UzeHome;

    use uze_core::{
        PackageSource,
        integration::{AttachmentReceipt, ManagedArtifact},
        state,
        trust::AlwaysTrust,
    };

    use super::*;

    #[test]
    fn restores_a_missing_receipt_owned_symlink_without_touching_plugin_bytes() {
        let root = uze_testkit::temp::scratch("maintenance");
        let home = UzeHome::at(&root);
        let target = root.join("store-target");
        let link = root.join("harness/skill");
        fs::create_dir_all(&target).unwrap();
        let app = UzeApplication::new(home.clone(), vec![Box::new(TestIntegration)]);
        app.plugins()
            .add(
                PackageSource::local(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../../tests/_fixtures/canonical/skill-plugin"),
                ),
                &AlwaysTrust,
            )
            .unwrap();
        let package_id = app.plugins().list().unwrap().remove(0).id;
        let recorded = AttachmentReceipt {
            package_id,
            resource_identity: Some("skill:fixture".to_owned()),
            integration: "fixture".to_owned(),
            artifact: ManagedArtifact::SymlinkReference {
                path: link.clone(),
                target: target.clone(),
            },
        };
        // The name a report gives an attachment is derived from the
        // receipt's own fields now, rather than being a key the ledger
        // stored beside them.
        let named = recorded.cache_key();
        state::record_receipt(&home, recorded).unwrap();
        let report = app.health().maintain();
        assert!(
            report.outcomes.iter().any(|outcome| matches!(
                outcome,
                MaintenanceOutcome::Repaired { receipt, .. } if *receipt == named
            )),
            "{:?}",
            report.outcomes
        );
        assert_eq!(fs::read_link(link).unwrap(), target);
    }

    #[test]
    fn preserves_a_drifted_receipt_owned_symlink() {
        let root = uze_testkit::temp::scratch("maintenance-drift");
        let home = UzeHome::at(&root);
        let expected = root.join("expected");
        let foreign = root.join("foreign");
        let link = root.join("harness/skill");
        fs::create_dir_all(&expected).unwrap();
        fs::create_dir_all(&foreign).unwrap();
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&foreign, &link).unwrap();
        let app = UzeApplication::new(home.clone(), vec![Box::new(TestIntegration)]);
        app.plugins()
            .add(
                PackageSource::local(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../../tests/_fixtures/canonical/skill-plugin"),
                ),
                &AlwaysTrust,
            )
            .unwrap();
        let package_id = app.plugins().list().unwrap().remove(0).id;
        state::record_receipt(
            &home,
            AttachmentReceipt {
                package_id,
                resource_identity: Some("skill:fixture".to_owned()),
                integration: "fixture".to_owned(),
                artifact: ManagedArtifact::SymlinkReference {
                    path: link.clone(),
                    target: expected,
                },
            },
        )
        .unwrap();

        let report = app.health().maintain();
        assert!(report.outcomes.iter().any(|outcome| matches!(
            outcome,
            MaintenanceOutcome::NeedsHumanAction {
                state: Some(AttachmentState::Drifted),
                ..
            }
        )));
        assert_eq!(fs::read_link(link).unwrap(), foreign);
    }

    #[test]
    fn an_orphaned_receipt_is_cleaned_and_frees_the_slot_it_occupied() {
        // Reproduces the real failure a store id format change (this
        // project's own marketplace-qualification: `flow` -> `flow@ai`)
        // leaves behind: an old receipt still recorded under a package id
        // nothing installs under any more, still physically occupying a
        // shared slot a live install now needs.
        let root = uze_testkit::temp::scratch("maintenance-orphan");
        let home = UzeHome::at(&root);
        let target = root.join("store-target");
        let link = root.join("harness/skill");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let app = UzeApplication::new(home.clone(), vec![Box::new(TestIntegration)]);

        // No package named "old-git" is installed — this receipt is
        // already an orphan the moment it is recorded.
        state::record_receipt(
            &home,
            AttachmentReceipt {
                package_id: "old-git".to_owned(),
                resource_identity: None,
                integration: "fixture".to_owned(),
                artifact: ManagedArtifact::SymlinkReference {
                    path: link.clone(),
                    target: target.clone(),
                },
            },
        )
        .unwrap();

        let report = app.health().maintain();
        assert!(
            report.outcomes.iter().any(|outcome| matches!(
                outcome,
                MaintenanceOutcome::OrphanCleaned { plugin, .. } if plugin == "old-git"
            )),
            "expected an OrphanCleaned outcome for old-git, got {:?}",
            report.outcomes
        );
        assert!(
            state::receipts(&home, Some("old-git")).unwrap().is_empty(),
            "the orphan's ledger entry must be forgotten"
        );
        assert!(
            !link.exists(),
            "the orphan's physical slot must be freed, not just forgotten in the ledger"
        );

        let _ = fs::remove_dir_all(root);
    }

    struct TestIntegration;

    impl uze_core::integration::IntegrationPort for TestIntegration {
        fn id(&self) -> &'static str {
            "fixture"
        }

        fn capabilities(&self) -> uze_core::router::HarnessCapabilities {
            uze_core::router::HarnessCapabilities::default()
        }

        fn exposure_plan(
            &self,
            _resource: &uze_core::Resource,
        ) -> uze_core::exposure::ExposurePlan {
            panic!("not used")
        }
    }
}
