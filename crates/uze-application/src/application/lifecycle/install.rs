//! Installing a plugin: its bytes, the trust question they raise, the Store
//! entry and the delivery to every detected harness.

use std::collections::BTreeSet;

use uze_core::{
    PackageSource, Result, UzeError,
    integration::{AttachmentReceipt, AttachmentState},
    naming::{
        NameCollisionAuthority, NameCollisionRequest, NameCollisionResolution,
        NoNameCollisionAuthority,
    },
    state,
    trust::TrustAuthority,
};

use crate::bootstrap;

use super::attach::{NativeDelivery, PackageDelivery};
use crate::application::services::Plugins;
use crate::application::*;

impl Plugins<'_> {
    pub(crate) fn acquire(&self, source: &PackageSource) -> Result<uze_core::MaterializedPackage> {
        match source {
            PackageSource::Embedded { id } => bootstrap::materialize(id),
            _ => uze_core::acquisition::acquire(source),
        }
    }

    /// Installs a package straight from a source, under the `local`
    /// marketplace. Refuses, without asking, a bare plugin name already
    /// active under another marketplace (ADR-036).
    #[tracing::instrument(name = "plugins.add", skip_all, err)]
    pub fn add(
        &self,
        source: PackageSource,
        authority: &dyn TrustAuthority,
    ) -> Result<AddPluginReport> {
        let _mutation = uze_core::persistence::MutationLock::acquire(&self.0.home)?;
        // Acquisition brings the bytes to a local directory and owns their
        // cleanup; the Store only ever sees a materialized package.
        let materialized = self.acquire(&source)?;
        self.install_materialized(
            materialized,
            "local",
            None,
            authority,
            &NoNameCollisionAuthority,
        )
    }

    /// Installs bytes that already exist locally: asks `authority` about any
    /// execution they introduce, then ingests and delivers them.
    ///
    /// Deliberately takes no lock: every caller holds one already, and
    /// `MutationLock` is not reentrant.
    ///
    /// `active_name` requests a local name other than the package's own bare
    /// plugin name (ADR-036); an update uses it to keep an alias a past
    /// collision resolution gave the package.
    pub(crate) fn install_materialized(
        &self,
        materialized: uze_core::MaterializedPackage,
        marketplace: &str,
        active_name: Option<&str>,
        authority: &dyn TrustAuthority,
        name_authority: &dyn NameCollisionAuthority,
    ) -> Result<AddPluginReport> {
        // Trust is decided here — after the package is materialized and can
        // be inspected honestly, and strictly before anything is written to
        // the Store or shown to a harness. Neither the Store nor any
        // integration knows this question exists.
        self.0.authorize(&materialized, authority, &[], false)?;
        self.install_authorized(materialized, marketplace, active_name, name_authority)
    }

    /// `install_materialized` for bytes whose trust question was already
    /// answered — an update asks it against the revision it replaces.
    pub(super) fn install_authorized(
        &self,
        materialized: uze_core::MaterializedPackage,
        marketplace: &str,
        active_name: Option<&str>,
        name_authority: &dyn NameCollisionAuthority,
    ) -> Result<AddPluginReport> {
        // A guard this platform cannot run is refused before anything of the
        // package is stored or attached: delivered without it, the package
        // would let through every operation the guard was there to check.
        let unspelled = uze_core::hook::guards_unspelled_here(materialized.root())?;
        if !unspelled.is_empty() {
            let package = uze_core::acquisition::inspect_capabilities(&materialized)?.package_id;
            return Err(UzeError::GuardUnspelledHere {
                package,
                groups: unspelled,
                platform: uze_core::shell::ShellCommand::platform(),
            });
        }
        // Any installation changes vendor-visible state; cached inspection
        // verdicts must not outlive it (ADR 018).
        self.0.inspection_cache.invalidate();
        // Deliberately does NOT run `reconcile_orphaned_receipts` here.
        // Attach's own conflict detection needs the first look at whatever
        // occupies a shared projection slot: a receipt that is Matched but
        // keyed by an id nothing installs under any more is exactly as
        // consistent with "the previous generation's incompatible wrapper,
        // a real conflict a person must see" as it is with "a plain
        // rename/removal, safe to clean" — the two are indistinguishable
        // from here, and only the second is safe to resolve without a
        // person looking. `uze doctor` (`Health::maintain`) is the
        // explicit, narrower place that reconciliation belongs; a blocked
        // install's `ProjectionConflict` is the correct, honest outcome
        // when the ambiguity can't be resolved silently.

        // `uze add` is deliberately enough for a harness the user already
        // has.  Preparing a detected integration only creates UZE's own
        // prerequisites (such as a user-scope discovery directory) and
        // records its setup state; it never installs, upgrades, or launches
        // the vendor executable.  Do it before ingesting so a preparation
        // failure cannot leave a newly installed package with no reported
        // delivery attempt.
        self.0.prepare_detected_integrations()?;

        let held_before: BTreeSet<String> = self
            .0
            .store
            .package_ids()?
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect();
        let installed = self.ingest_resolving_name_collision(
            &materialized,
            marketplace,
            active_name,
            name_authority,
        )?;

        // Derived views refresh before attachment: a native package delivery
        // reads the view it was just given. A failure here is recorded, never
        // propagated — the package is installed, and one integration's view
        // being stale does not make the installation invalid.
        let publications = self.0.republish_all();
        let unpublished: BTreeSet<&str> = publications
            .iter()
            .filter(|outcome| outcome.error.is_some())
            .map(|outcome| outcome.integration.as_str())
            .collect();

        // The Store hands an already-held package back for the same origin,
        // so only a package this call brought in may be taken back out.
        let created = !held_before.contains(installed.id.as_str());
        let receipts_before = state::receipts(&self.0.home, Some(installed.id.as_str()))?;

        let resources = uze_core::engine::package_resources(&installed)?;
        let resources: Vec<_> = resources.iter().collect();
        // A package must remain installable on a machine that has only a
        // subset of UZE's peer harnesses. `add` prepares and attaches to
        // detected harnesses; an absent executable is neither a package
        // incompatibility nor a reason to invoke its vendor CLI. Native
        // delivery reads the view; attempting it against a view that failed
        // to publish would fail for a reason that has nothing to do with
        // this package.
        let targets: Vec<(&dyn IntegrationPort, NativeDelivery)> = self
            .0
            .integrations
            .iter()
            .filter(|integration| self.0.detect_cached(integration.as_ref()).present)
            .map(|integration| {
                let native = if unpublished.contains(integration.id()) {
                    NativeDelivery::Skipped
                } else {
                    NativeDelivery::Allowed
                };
                (integration.as_ref(), native)
            })
            .collect();
        if !targets.is_empty() {
            let names: Vec<&str> = targets
                .iter()
                .map(|(integration, _)| integration.display_name())
                .collect();
            tracing::info!(
                target: uze_core::acquisition::git::STEP,
                step = "deliver",
                harness = names.join(", ")
            );
        }
        // Every harness at once: each delivery is its own vendor CLI's
        // startup and work, and they share nothing but the receipt ledger,
        // which serializes its own writes. Collected in the registry's order,
        // so the report — and the first error — read the same as before.
        let parent = tracing::Span::current();
        let deliveries: Vec<_> = std::thread::scope(|scope| {
            let running: Vec<_> = targets
                .iter()
                .map(|(integration, native)| {
                    let parent = parent.clone();
                    let (installed, resources) = (&installed, &resources);
                    scope.spawn(move || {
                        parent.in_scope(|| {
                            self.0
                                .deliver_package_to(installed, resources, *integration, *native)
                        })
                    })
                })
                .collect();
            running
                .into_iter()
                .zip(&targets)
                .map(|(delivery, (integration, _))| {
                    delivery.join().unwrap_or_else(|_| {
                        Err(UzeError::HarnessCommand(format!(
                            "delivery to `{}` stopped unexpectedly",
                            integration.id()
                        )))
                    })
                })
                .collect()
        });
        let mut attachments = Vec::new();
        let mut package_plans = Vec::new();
        let mut blocked = Vec::new();
        let mut reports = Vec::new();
        let mut failures = Vec::new();
        for ((integration, native), delivery) in targets.iter().zip(deliveries) {
            let delivery = match delivery {
                Ok(delivery) => delivery,
                Err(error) => {
                    failures.push((*integration, error));
                    continue;
                }
            };
            let route = delivery_route(&delivery, *native, integration.id(), &publications);
            let own_blocked: Vec<BlockedCapability> = delivery
                .blocked
                .into_iter()
                .map(|one| BlockedCapability {
                    integration: one.integration,
                    capability: one.capability,
                    reason: one.reason,
                })
                .collect();
            reports.push(HarnessDeliveryReport {
                integration: integration.id().to_owned(),
                display_name: integration.display_name().to_owned(),
                outcome: HarnessDeliveryOutcome::Delivered {
                    route,
                    attachments: delivery
                        .attachments
                        .iter()
                        .map(|attachment| attachment.location.clone())
                        .collect(),
                    blocked: own_blocked.clone(),
                    shortfalls: delivery
                        .shortfalls
                        .iter()
                        .cloned()
                        .map(CapabilityShortfallReport::from)
                        .collect(),
                },
            });
            if let Some(plan) = delivery.plan {
                package_plans.push((integration.id().to_owned(), plan));
            }
            attachments.extend(delivery.attachments);
            blocked.extend(own_blocked);
        }
        let delivered: Vec<String> = reports
            .iter()
            .map(|report| report.integration.clone())
            .collect();
        for integration in &delivered {
            state::forget_undelivered(&self.0.home, installed.id.as_str(), Some(integration))?;
        }
        if !failures.is_empty() {
            self.answer_for_failed_deliveries(
                &installed,
                created,
                !delivered.is_empty(),
                &receipts_before,
                failures,
                &mut reports,
            )?;
        }
        // Reported in the registry's order, whichever harness failed.
        reports.sort_by_key(|report| {
            targets
                .iter()
                .position(|(integration, _)| integration.id() == report.integration)
        });
        Ok(AddPluginReport {
            plugin: self.0.plugin_summary(&installed)?,
            package_plans,
            attachments,
            publications,
            blocked,
            declared: false,
            deliveries: reports,
        })
    }

    /// Settles an install whose delivery failed on at least one harness.
    ///
    /// Each failing harness first loses what the attempt attached to it: a
    /// harness is delivered or it is not, never half. Then, when no harness
    /// took the package at all, a package this install brought into the
    /// Store leaves it again and the whole install fails — a failed install
    /// is not an installed package. Otherwise the package stays — it was
    /// there before, another harness took it, or part of the attempt could
    /// not be taken back — and each
    /// failing harness is recorded against it so every listing says so
    /// until a later delivery succeeds.
    fn answer_for_failed_deliveries(
        &self,
        installed: &uze_core::StoredPackage,
        created: bool,
        delivered_somewhere: bool,
        receipts_before: &[AttachmentReceipt],
        failures: Vec<(&dyn IntegrationPort, UzeError)>,
        reports: &mut Vec<HarnessDeliveryReport>,
    ) -> Result<()> {
        let package_id = installed.id.as_str();
        let mut lines = Vec::new();
        let mut left_behind = Vec::new();
        for (integration, error) in &failures {
            let label = integration.display_name();
            lines.push(format!("  {label}: {error}"));
            left_behind.extend(
                self.take_back_attempt(package_id, *integration, receipts_before)
                    .into_iter()
                    .map(|what| format!("  {label}: {what}")),
            );
        }
        let failed = lines.join("\n");
        // UZE's own plugin is the exception: it re-seeds itself whenever it
        // is absent, so taking it back out would retry the refusing harness
        // on every command rather than say once that it was refused.
        let removable = created
            && !delivered_somewhere
            && left_behind.is_empty()
            && !Self::is_protected_package(installed);
        if removable {
            self.0.store.remove_package(&installed.id)?;
            state::forget_undelivered(&self.0.home, package_id, None)?;
            self.0.republish_all_reporting();
            return Err(UzeError::DeliveryFailed(format!(
                "`{package_id}` could not be delivered:\n{failed}\nNothing was installed."
            )));
        }
        for (integration, error) in &failures {
            state::record_undelivered(
                &self.0.home,
                package_id,
                integration.id(),
                &error.to_string(),
            )?;
        }
        if !left_behind.is_empty() {
            return Err(UzeError::DeliveryFailed(format!(
                "`{package_id}` could not be delivered:\n{failed}\nWhat the attempt attached \
                 could not all be taken back off, so the package stays installed:\n{}",
                left_behind.join("\n")
            )));
        }
        if !delivered_somewhere {
            return Err(UzeError::DeliveryFailed(format!(
                "`{package_id}` could not be delivered:\n{failed}\nThe package stays \
                 installed, recorded as not delivered to the harnesses above."
            )));
        }
        reports.extend(
            failures
                .into_iter()
                .map(|(integration, error)| HarnessDeliveryReport {
                    integration: integration.id().to_owned(),
                    display_name: integration.display_name().to_owned(),
                    outcome: HarnessDeliveryOutcome::Failed {
                        error: error.to_string(),
                    },
                }),
        );
        Ok(())
    }

    /// Takes off everything `integration` holds for `package_id` that
    /// `before` did not, under the same inspection any removal obeys, and
    /// says what it could not.
    fn take_back_attempt(
        &self,
        package_id: &str,
        integration: &dyn IntegrationPort,
        before: &[AttachmentReceipt],
    ) -> Vec<String> {
        let recorded = match state::receipts(&self.0.home, Some(package_id)) {
            Ok(recorded) => recorded,
            Err(error) => return vec![format!("the attachment ledger could not be read: {error}")],
        };
        let mut left = Vec::new();
        // What this attempt attached is what `before` held no receipt for:
        // judged by what a receipt is about, not by its bytes, since a
        // delivery the person already had may have been rewritten by this
        // build and is still theirs.
        let held_before = |receipt: &AttachmentReceipt| {
            before.iter().any(|earlier| {
                earlier.package_id == receipt.package_id
                    && earlier.integration == receipt.integration
                    && earlier.resource_identity == receipt.resource_identity
            })
        };
        for receipt in recorded
            .iter()
            .filter(|receipt| receipt.integration == integration.id() && !held_before(receipt))
        {
            let location = receipt.artifact.location();
            let gone = match integration.inspect_receipt(receipt).state {
                AttachmentState::Missing => Ok(true),
                AttachmentState::Matched => integration
                    .detach_receipt(receipt)
                    .map(|inspection| inspection.state == AttachmentState::Missing),
                other => {
                    left.push(format!("{} is {other:?}", location.display()));
                    continue;
                }
            };
            match gone.and_then(|gone| {
                if gone {
                    state::forget_receipt(&self.0.home, receipt)?;
                }
                Ok(gone)
            }) {
                Ok(true) => {}
                Ok(false) => left.push(format!("{} is still there", location.display())),
                Err(error) => left.push(format!("{}: {error}", location.display())),
            }
        }
        left
    }

    /// Ingests `materialized` under `requested_active_name` (or its own bare
    /// plugin name, when `None` — every ordinary install), asking
    /// `name_authority` to resolve a collision with an already-active,
    /// differently-marketplaced package instead of failing outright
    /// (ADR-036). `Alias` retries the ingest under the chosen local name.
    /// `Replace` removes the existing active package first — only once that
    /// is proven `Safe`, exactly the rule `Plugins::remove` enforces, so a
    /// `Blocked` removal aborts the whole replace with the existing package
    /// left exactly as it was — then retries the ingest under the name it
    /// just freed. Any other ingest error (an unrelated `PackageConflict`, a
    /// bad manifest) is never routed through the authority at all.
    fn ingest_resolving_name_collision(
        &self,
        materialized: &uze_core::MaterializedPackage,
        marketplace: &str,
        requested_active_name: Option<&str>,
        name_authority: &dyn NameCollisionAuthority,
    ) -> Result<uze_core::StoredPackage> {
        let (name, existing, requested) =
            match self
                .0
                .store
                .ingest(materialized, marketplace, requested_active_name)
            {
                Ok(installed) => return Ok(installed),
                Err(UzeError::PluginNameCollision {
                    name,
                    existing,
                    requested,
                }) => (name, existing, requested),
                Err(other) => return Err(other),
            };
        let request = NameCollisionRequest {
            name: name.clone(),
            existing: existing.clone(),
            requested: requested.clone(),
        };
        match name_authority.resolve(&request) {
            NameCollisionResolution::Abort => Err(UzeError::PluginNameCollision {
                name,
                existing,
                requested,
            }),
            NameCollisionResolution::Alias(alias) => {
                self.0.store.ingest(materialized, marketplace, Some(&alias))
            }
            NameCollisionResolution::Replace => match self.detach_and_remove(&existing, false)? {
                RemovePluginReport::Removed { .. } | RemovePluginReport::AlreadyAbsent { .. } => {
                    self.0
                        .store
                        .ingest(materialized, marketplace, requested_active_name)
                }
                RemovePluginReport::Blocked { .. } => Err(UzeError::PluginNameCollision {
                    name,
                    existing,
                    requested,
                }),
            },
        }
    }
}

impl UzeApplication {
    /// Asks the supplied authority about any capability that would introduce
    /// process execution, and refuses to proceed without a grant.
    ///
    /// Returns `Ok(())` immediately when the package declares nothing
    /// executable: a purely declarative package needs no consent beyond the
    /// decision to install it.
    pub(crate) fn authorize(
        &self,
        materialized: &uze_core::MaterializedPackage,
        authority: &dyn TrustAuthority,
        already_trusted: &[trust::ExecutableCapability],
        replacing_installed: bool,
    ) -> Result<()> {
        let provenance = materialized.provenance();
        if !provenance.requested.crosses_trust_boundary() {
            return Ok(());
        }
        let inspected = uze_core::acquisition::inspect_capabilities(materialized)?;
        let resources: Vec<&uze_core::Resource> = inspected.resources.iter().collect();
        let executable = trust::executable_capabilities(&resources);
        if executable.is_empty() || !trust::introduces_new_execution(already_trusted, &executable) {
            return Ok(());
        }
        let request = TrustRequest {
            package_id: inspected.package_id.clone(),
            requested_source: provenance.requested.display(),
            resolved_source: provenance.resolved.display(),
            executable,
            // The operator is being asked about a *change* to something they
            // already have, not about a first install. Derived from the fact
            // of an existing installation rather than from whether the
            // previous revision happened to execute anything — a declarative
            // package gaining an MCP server is exactly the case that must
            // read as a change.
            previously_trusted: replacing_installed,
        };
        match authority.authorize(&request) {
            TrustOutcome::Granted => Ok(()),
            TrustOutcome::Denied => Err(UzeError::TrustDenied(request.package_id)),
            TrustOutcome::Unavailable => Err(UzeError::TrustRequired {
                package: request.package_id.clone(),
                detail: request
                    .executable
                    .iter()
                    .map(|capability| {
                        format!(
                            "{} -> {} {}",
                            capability.name,
                            capability.command,
                            capability.arguments.join(" ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; "),
            }),
        }
    }
}

/// Why a delivery took the route it did — the one question an author
/// debugging a release cannot answer from the harness side.
fn delivery_route(
    delivery: &PackageDelivery,
    native: NativeDelivery,
    integration: &str,
    publications: &[PublicationOutcome],
) -> DeliveryRoute {
    if native == NativeDelivery::Skipped {
        let error = publications
            .iter()
            .find(|outcome| outcome.integration == integration)
            .and_then(|outcome| outcome.error.as_deref())
            .unwrap_or("unknown error");
        return DeliveryRoute::CapabilityByCapability {
            reason: format!("the package view could not be published: {error}"),
        };
    }
    super::effective::delivery_route_of(delivery.plan.as_ref(), delivery.plan_set_aside)
}
