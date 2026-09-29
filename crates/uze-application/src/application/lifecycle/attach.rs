//! Delivering an installed package to one harness: the package-native plan
//! first, then each remaining resource, each artifact recorded by a
//! receipt.

use std::collections::BTreeSet;

use uze_core::{
    Result,
    capability::CapabilityKind,
    capability::Resource,
    exposure::ExposureMechanism,
    integration::{AttachmentReceipt, AttachmentState, IntegrationPort, ManagedArtifact},
    state,
    store::StoredPackage,
};

use super::super::*;

/// Whether an integration's own package-level plan may be used for this
/// delivery. `Skipped` when the derived view a native package reads failed
/// to refresh — attempting it would fail for a reason that has nothing to
/// do with the package, so delivery falls back to capability level.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeDelivery {
    Allowed,
    Skipped,
}

/// What one package's delivery to one integration produced: the native plan
/// it was delivered under, if any, where each recorded artifact landed, and
/// each capability something else already owned the name of.
#[derive(Default)]
pub(crate) struct PackageDelivery {
    pub plan: Option<uze_core::exposure::PackageExposurePlan>,
    /// The plan was offered and not used: capability receipts it would
    /// replace are not safe to detach, so they stay and nothing is added
    /// beside them.
    pub plan_set_aside: bool,
    pub attachments: Vec<AttachmentSummary>,
    pub blocked: Vec<BlockedDelivery>,
    /// Capabilities that reached the harness short of their canonical
    /// meaning, with the route and the evidence saying what was lost.
    pub shortfalls: Vec<CapabilityShortfall>,
}

/// One capability delivered on a route less than native.
#[derive(Clone, Debug)]
pub(crate) struct CapabilityShortfall {
    pub capability: String,
    pub route: uze_core::router::CompatibilityRoute,
    pub evidence: String,
}

impl CapabilityShortfall {
    pub(crate) fn of(
        resource: &Resource,
        (route, evidence): (uze_core::router::CompatibilityRoute, String),
    ) -> Option<Self> {
        (route != uze_core::router::CompatibilityRoute::Native).then(|| Self {
            capability: resource
                .resolved_exposure_name
                .clone()
                .or_else(|| resource.logical_capability_name())
                .unwrap_or_else(|| resource.identity()),
            route,
            evidence,
        })
    }
}

/// One capability that could not be delivered because the name it needs is
/// held by something UZE does not own.
///
/// Reported rather than propagated: a package's other capabilities are
/// unaffected by one name being taken, and failing the whole command over
/// it leaves the operator with nothing where they could have had all but
/// one. Only the three refusals that *are* about a single name qualify —
/// anything else (a write that failed, a vendor CLI that broke) is about
/// the machine, and still fails.
pub(crate) struct BlockedDelivery {
    pub integration: String,
    pub capability: String,
    pub reason: String,
}

/// Whether `error` refuses one name, or says something is wrong with the
/// machine. See [`BlockedDelivery`].
pub(crate) fn refuses_one_name(error: &uze_core::UzeError) -> bool {
    matches!(
        error,
        uze_core::UzeError::ManagedEntryDrift(_)
            | uze_core::UzeError::ManagedEntryConflict(_)
            | uze_core::UzeError::ProjectionConflict(_)
    )
}

impl UzeApplication {
    pub(crate) fn attach_stored_packages_to(
        &self,
        integration: &dyn IntegrationPort,
    ) -> Result<()> {
        let mut first_error: Option<uze_core::UzeError> = None;
        for package_id in self.store.package_ids()? {
            let package = match self.store.package(&package_id) {
                Ok(pkg) => pkg,
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                    continue;
                }
            };
            if let Err(error) = self.attach_package_to(&package, integration)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn attach_package_to(
        &self,
        package: &StoredPackage,
        integration: &dyn IntegrationPort,
    ) -> Result<()> {
        let resources = uze_core::engine::package_resources(package)?;
        let resources: Vec<_> = resources.iter().collect();
        self.deliver_package_to(package, &resources, integration, NativeDelivery::Allowed)?;
        state::forget_undelivered(&self.home, package.id.as_str(), Some(integration.id()))
    }

    /// The one receipt-safe delivery of a package to a single integration:
    /// native package plan first when the integration offers one, then a
    /// capability-level attachment for every resource the plan does not
    /// provide. Returns what it produced so an install can report it —
    /// `attach` and `install` must never grow two copies of this.
    pub(crate) fn deliver_package_to(
        &self,
        package: &StoredPackage,
        resources: &[&Resource],
        integration: &dyn IntegrationPort,
        native_delivery: NativeDelivery,
    ) -> Result<PackageDelivery> {
        let _span = tracing::info_span!(
            "integration.attach",
            integration = integration.id(),
            package = %package.id.as_str()
        )
        .entered();
        // Before any harness is handed a path into it: `${PLUGIN_ROOT}`
        // names this copy, never the Store.
        uze_core::delivered_root::materialize(&self.home, package)?;
        let mut delivery = PackageDelivery::default();
        let mut provided = BTreeSet::new();
        if let Some(plan) = integration
            .package_exposure_plan(package, resources)
            .filter(|_| native_delivery == NativeDelivery::Allowed)
        {
            delivery.plan = Some(plan.clone());
            // Idempotency guard: a package-level receipt already recorded
            // for this integration that still inspects Matched means the
            // vendor install verb already ran. Re-running it is not safely
            // idempotent — Antigravity's preflight refuses an existing
            // same-name import UZE has to prove it owns again, and a
            // truthful `agy plugin list` would turn every second
            // `setup`/attach into a hard failure (found by the acceptance
            // suite with a truthful fake CLI).
            let package_receipts: Vec<uze_core::integration::AttachmentReceipt> =
                state::receipts(&self.home, Some(package.id.as_str()))?
                    .into_iter()
                    .filter(|receipt| {
                        receipt.integration == integration.id()
                            && receipt.resource_identity.is_none()
                    })
                    .collect();
            // A route an earlier build took and this one no longer does is
            // taken back off before the current one is attached.
            for receipt in package_receipts
                .iter()
                .filter(|receipt| !integration.package_receipt_serves(receipt))
            {
                self.retire_receipt(integration, receipt)?;
            }
            let already_attached = package_receipts.into_iter().find(|receipt| {
                integration.package_receipt_serves(receipt)
                    && integration.inspect_receipt(receipt).state == AttachmentState::Matched
            });
            if let Some(receipt) = already_attached {
                delivery.attachments.push(AttachmentSummary {
                    integration: integration.id().to_owned(),
                    location: receipt.artifact.location(),
                });
                // A plan that grew since the package was attached (agents
                // joining the plugin) covers capabilities still delivered
                // on their own beside it: each safely detachable one goes,
                // so no capability is offered twice.
                for receipt in self.covered_receipts(package, integration, &plan)? {
                    self.retire_receipt(integration, &receipt)?;
                }
                provided = plan.provided_resource_identities;
            } else {
                // The package was delivered capability-by-capability before
                // this integration had a native plan for it. Detach the
                // capability receipts the plan now covers, but only while
                // every one of them is safely detachable: a single
                // Drifted/Conflict/Blocked leaves decomposed delivery in
                // place rather than adding a native copy beside it.
                let covered_existing = self.covered_receipts(package, integration, &plan)?;
                let native_blocked = covered_existing.iter().any(|receipt| {
                    matches!(
                        integration.inspect_receipt(receipt).state,
                        AttachmentState::Drifted
                            | AttachmentState::Conflict
                            | AttachmentState::Blocked
                    )
                });
                if native_blocked {
                    // Keep decomposed delivery; do not attach native to avoid duplication.
                    delivery.plan_set_aside = true;
                    provided = BTreeSet::new();
                } else {
                    for receipt in covered_existing {
                        self.retire_receipt(integration, &receipt)?;
                    }
                    if let Some(receipt) = integration.attach_package(package, &plan)? {
                        let location = receipt.artifact.location();
                        state::record_receipt(&self.home, receipt)?;
                        delivery.attachments.push(AttachmentSummary {
                            integration: integration.id().to_owned(),
                            location,
                        });
                    }
                    // A native plan that returns no receipt explicitly
                    // declined UZE ownership (for example, a same-name
                    // plugin already imported by the harness). Its native
                    // delivery remains authoritative, so avoid adding
                    // duplicate capability-level fallbacks beside it.
                    provided = plan.provided_resource_identities;
                }
            }
        }
        for resource in resources {
            if provided.contains(&resource.identity()) {
                delivery.shortfalls.extend(
                    integration
                        .packaged_shortfall(package, resource)
                        .and_then(|plan| CapabilityShortfall::of(resource, plan)),
                );
            } else {
                if let Some(held) = self.retire_superseded_receipts(resource, integration)? {
                    delivery.blocked.push(held);
                    continue;
                }
                let attached =
                    self.resolve_exposure_name(resource, integration)
                        .and_then(|resolved| {
                            let receipt = integration.attach_receipt(&resolved)?;
                            // Delivered short of native, or not delivered at
                            // all because the harness has nothing for it: both
                            // are said. Project instructions reach a harness
                            // through the context, never through this path.
                            if resolved.capability.kind != CapabilityKind::Instruction {
                                let plan = integration.exposure_plan(&resolved);
                                let route = if receipt.is_none()
                                    && matches!(
                                        plan.mechanism,
                                        uze_core::exposure::ExposureMechanism::Unsupported { .. }
                                    ) {
                                    uze_core::router::CompatibilityRoute::Unsupported
                                } else {
                                    plan.route
                                };
                                delivery.shortfalls.extend(CapabilityShortfall::of(
                                    &resolved,
                                    (route, plan.evidence),
                                ));
                            }
                            Ok(receipt)
                        });
                let receipt = match attached {
                    Ok(receipt) => receipt,
                    Err(error) if refuses_one_name(&error) => {
                        tracing::warn!(
                            integration = integration.id(),
                            capability = %resource.identity(),
                            reason = %error,
                            "a capability's name is held by something UZE does not own"
                        );
                        delivery.blocked.push(BlockedDelivery {
                            integration: integration.id().to_owned(),
                            capability: resource.identity(),
                            reason: error.to_string(),
                        });
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                if let Some(receipt) = receipt {
                    let location = receipt.artifact.location();
                    state::record_receipt(&self.home, receipt)?;
                    delivery.attachments.push(AttachmentSummary {
                        integration: integration.id().to_owned(),
                        location,
                    });
                }
            }
        }
        Ok(delivery)
    }

    /// The capability receipts this integration holds for the package that
    /// `plan` now provides.
    pub(crate) fn covered_receipts(
        &self,
        package: &StoredPackage,
        integration: &dyn IntegrationPort,
        plan: &uze_core::exposure::PackageExposurePlan,
    ) -> Result<Vec<uze_core::integration::AttachmentReceipt>> {
        Ok(state::receipts(&self.home, Some(package.id.as_str()))?
            .into_iter()
            .filter(|receipt| {
                receipt.integration == integration.id()
                    && receipt.resource_identity.as_ref().is_some_and(|identity| {
                        plan.provided_resource_identities.contains(identity)
                    })
            })
            .collect())
    }

    /// Takes one receipt's artifact back off, inspecting first: a matched
    /// artifact is detached and forgotten, a missing one only forgotten, and
    /// anything else is left exactly where it is. Returns whether the
    /// receipt is gone.
    fn retire_receipt(
        &self,
        integration: &dyn IntegrationPort,
        receipt: &uze_core::integration::AttachmentReceipt,
    ) -> Result<bool> {
        // An entry another integration also holds a receipt for (a skills
        // root two harnesses share) is still that one's: only this receipt
        // goes.
        let shared = state::receipts(&self.home, None)?.iter().any(|other| {
            other.integration != receipt.integration
                && other.artifact.location() == receipt.artifact.location()
        });
        if shared {
            state::forget_receipt(&self.home, receipt)?;
            return Ok(true);
        }
        match integration.inspect_receipt(receipt).state {
            AttachmentState::Matched => {
                let detached = integration.detach_receipt(receipt)?;
                if detached.state == AttachmentState::Missing {
                    state::forget_receipt(&self.home, receipt)?;
                    return Ok(true);
                }
                Ok(false)
            }
            AttachmentState::Missing => {
                state::forget_receipt(&self.home, receipt)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Retires what an earlier build attached for `resource` in a form this
    /// build no longer gives it: under another name (an agent named after
    /// its file before it carried its plugin's label, an MCP server
    /// registered under a name its harness refuses), or as another kind of
    /// artifact (a skill linked into the generated tier before it was a
    /// directory of its own). Without this, "an existing receipt wins" would
    /// keep the old form forever. One someone changed since is left in
    /// place and the capability is held back, never offered a second time
    /// beside it.
    fn retire_superseded_receipts(
        &self,
        resource: &Resource,
        integration: &dyn IntegrationPort,
    ) -> Result<Option<BlockedDelivery>> {
        let named_by_candidates = resource.capability.kind.is_invoked_by_label()
            || resource.capability.kind == CapabilityKind::Mcp;
        if !named_by_candidates || integration.exposure_name_candidates(resource).is_empty() {
            return Ok(None);
        }
        let current: BTreeSet<String> = integration
            .exposure_name_candidates(resource)
            .into_iter()
            .collect();
        let planned_artifact = planned_artifact(integration, resource);
        let planned = planned_artifact.as_ref().map(std::mem::discriminant);
        let identity = resource.identity();
        let receipts = state::receipts(&self.home, Some(resource.package_id.as_str()))?;
        for receipt in &receipts {
            if receipt.integration != integration.id()
                || receipt.resource_identity.as_deref() != Some(identity.as_str())
            {
                continue;
            }
            let renamed = receipt
                .artifact
                .exposure_name()
                .is_some_and(|name| !current.contains(&name));
            let reshaped =
                planned.is_some_and(|kind| std::mem::discriminant(&receipt.artifact) != kind);
            let restated = planned_artifact
                .as_ref()
                .is_some_and(|planned| config_entry_restated(&receipt.artifact, planned));
            if !(renamed || reshaped || restated) {
                continue;
            }
            if reshaped {
                self.release_obsolete_co_holders(resource, receipt, &receipts)?;
            }
            if !self.retire_receipt(integration, receipt)? {
                return Ok(Some(BlockedDelivery {
                    integration: integration.id().to_owned(),
                    capability: identity,
                    reason: format!(
                        "an earlier delivery at {} changed since it was made; it is left in \
                         place, and this capability waits until it is removed",
                        receipt.artifact.location().display()
                    ),
                }));
            }
        }
        Ok(None)
    }

    /// An earlier build let two harnesses share one skill entry, each with
    /// its own receipt. When the entry is being replaced by another kind of
    /// artifact, a co-holder whose own plan also moved on no longer holds
    /// it, so its receipt is forgotten and the entry can be taken off.
    fn release_obsolete_co_holders(
        &self,
        resource: &Resource,
        receipt: &AttachmentReceipt,
        receipts: &[AttachmentReceipt],
    ) -> Result<()> {
        for other in receipts.iter().filter(|other| {
            other.integration != receipt.integration
                && other.resource_identity == receipt.resource_identity
                && other.artifact.location() == receipt.artifact.location()
        }) {
            let moved_on = self
                .integrations
                .iter()
                .find(|candidate| candidate.id() == other.integration)
                .and_then(|candidate| planned_artifact(candidate.as_ref(), resource))
                .is_some_and(|planned| {
                    std::mem::discriminant(&other.artifact) != std::mem::discriminant(&planned)
                });
            if moved_on {
                state::forget_receipt(&self.home, other)?;
            }
        }
        Ok(())
    }

    /// Resolves `resource`'s physical exposure name for `integration`,
    /// immediately before an attach call — the one place a naming decision
    /// happens. Returns a clone of `resource` with `resolved_exposure_name`
    /// set; `resource` itself is never mutated.
    ///
    /// "Existing receipt wins": a receipt for this exact
    /// `resource.identity()` hands back its already-recorded physical name
    /// verbatim, which is what makes re-add/setup idempotent. Every
    /// discovery directory has one owner, so only this integration's
    /// receipts are consulted.
    ///
    /// Only a resource with no receipt asks the integration for ordered
    /// candidates (`exposure_name_candidates`) and takes the first this
    /// integration has not already claimed. It resolves purely from the ledger, never
    /// the filesystem, so it never decides a foreign-artifact conflict;
    /// attach's own structural check remains the last word on that.
    pub(crate) fn resolve_exposure_name(
        &self,
        resource: &Resource,
        integration: &dyn IntegrationPort,
    ) -> Result<Resource> {
        if !resource.capability.kind.is_invoked_by_label()
            && resource.capability.kind != CapabilityKind::Mcp
        {
            return Ok(resource.clone());
        }
        let mut resolved = resource.clone();
        let Ok(all_receipts) = state::receipts(&self.home, None) else {
            return Ok(resolved);
        };
        let resource_id = resource.identity();
        // Existing receipt wins: a resource already attached keeps the
        // physical entry it was given, so re-running attach never renames
        // or duplicates it.
        if let Some(existing) = all_receipts.iter().find(|receipt| {
            receipt.resource_identity.as_deref() == Some(resource_id.as_str())
                && receipt.integration == integration.id()
        }) {
            resolved.resolved_exposure_name = existing.artifact.exposure_name();
            return Ok(resolved);
        }
        // A name is taken only where this resource would be written: a skill
        // directory and an agent file of the same label live side by side in
        // two different places and never contend.
        let planned = match integration.exposure_plan(resource).mechanism {
            uze_core::exposure::ExposureMechanism::Managed(artifact) => Some(artifact),
            _ => None,
        };
        let contends = |receipt: &uze_core::integration::AttachmentReceipt| {
            planned
                .as_ref()
                .is_none_or(|planned| same_name_space(planned, &receipt.artifact))
        };
        let claimed: BTreeSet<String> = all_receipts
            .iter()
            .filter(|receipt| receipt.integration == integration.id())
            .filter(|receipt| contends(receipt))
            .filter_map(|receipt| receipt.artifact.exposure_name())
            .collect();
        let candidates = integration.exposure_name_candidates(resource);
        if let Some(free) = candidates
            .iter()
            .find(|candidate| !claimed.contains(*candidate))
            .cloned()
        {
            resolved.resolved_exposure_name = Some(free);
            return Ok(resolved);
        }
        // Every candidate is already claimed. The reuse path above already
        // returned for the same resource, so a claimed name here belongs to
        // a DIFFERENT canonical resource: two
        // distinct resources converging on one label — one physical entry,
        // incompatible representations. That is a projection ownership
        // conflict, not drift; report it deterministically before any
        // attach, instead of handing the conflicting name back and failing
        // later with a misleading `ManagedEntryDrift` (ADR-029).
        // Nothing in `IntegrationPort` forbids an empty candidate list, and a
        // lifecycle operation is the wrong place to discover that: report the
        // integration that could not name the resource, the way the ledger
        // drift a few lines below degrades rather than panics.
        let Some(entry) = candidates.last().cloned() else {
            return Err(UzeError::ExposureUnavailable(format!(
                "`{}` offers no name to expose `{}` under",
                integration.id(),
                resource.name()
            )));
        };
        let claimant = all_receipts
            .iter()
            .filter(|receipt| receipt.integration == integration.id())
            .filter(|receipt| contends(receipt))
            .find(|receipt| receipt.artifact.exposure_name().as_deref() == Some(entry.as_str()));
        let Some(claimant) = claimant else {
            // Defensive fallback (should be unreachable): retain the
            // previous behavior rather than panicking on ledger drift.
            resolved.resolved_exposure_name = Some(entry);
            return Ok(resolved);
        };
        let requested_target = match &planned {
            Some(ManagedArtifact::SymlinkReference { target, .. }) => target.clone(),
            _ => resource.capability.path.clone(),
        };
        let entry_path = planned
            .as_ref()
            .and_then(|artifact| artifact.location().parent().map(|dir| dir.join(&entry)))
            .unwrap_or_else(|| PathBuf::from(&entry));
        Err(UzeError::ProjectionConflict(Box::new(
            uze_core::error::ProjectionConflictDetails {
                entry: entry_path,
                requested: resource.identity(),
                requested_integration: integration.id().to_owned(),
                requested_target,
                existing: claimant
                    .resource_identity
                    .clone()
                    .unwrap_or_else(|| claimant.package_id.clone()),
                existing_integration: claimant.integration.clone(),
                existing_target: artifact_owned_target(claimant),
            },
        )))
    }
}

/// The physical artifact a receipt's entry points at — the symlink target
/// for a reference, the file itself for a managed file, and the receipt
/// location for anything else (diagnostic-only, never owned data).
fn artifact_owned_target(receipt: &AttachmentReceipt) -> PathBuf {
    match &receipt.artifact {
        ManagedArtifact::SymlinkReference { target, .. } => target.clone(),

        _ => receipt.artifact.location(),
    }
}

impl UzeApplication {
    /// Refreshes every integration's derived view of the installed package
    /// set. Collects failures instead of propagating them: publication is not
    /// part of package ownership, so one harness failing to rebuild its view
    /// leaves the package installed and the other harnesses unaffected.
    ///
    /// An installed set that cannot be read rebuilds nothing, and says so
    /// for every integration.
    pub(crate) fn republish_all(&self) -> Vec<PublicationOutcome> {
        let packages = match self.installed_packages_checked() {
            Ok(packages) => packages,
            Err(error) => {
                return self
                    .integrations
                    .iter()
                    .map(|integration| PublicationOutcome {
                        integration: integration.id().to_owned(),
                        error: Some(format!("the installed plugins could not be read: {error}")),
                    })
                    .collect();
            }
        };
        self.integrations
            .iter()
            .map(|integration| PublicationOutcome {
                integration: integration.id().to_owned(),
                error: {
                    let _span = tracing::info_span!(
                        "integration.republish",
                        integration = integration.id()
                    )
                    .entered();
                    integration
                        .republish_packages(&packages)
                        .err()
                        .map(|error| error.to_string())
                },
            })
            .collect()
    }

    /// `republish_all` for a caller with no report to carry the outcome in:
    /// a view that failed to rebuild is still said, in the log.
    pub(crate) fn republish_all_reporting(&self) {
        for outcome in self.republish_all() {
            if let Some(error) = outcome.error {
                tracing::warn!(
                    integration = %outcome.integration,
                    %error,
                    "a derived view could not be rebuilt"
                );
            }
        }
    }

    /// `republish_all`, for the integrations whose derived view no longer
    /// matches `packages`: one outcome per view it rebuilt.
    pub(crate) fn republish_unpublished(
        &self,
        packages: &[StoredPackage],
    ) -> Vec<PublicationOutcome> {
        self.integrations
            .iter()
            .filter(|integration| {
                matches!(
                    integration.publication(packages),
                    PublicationStatus::Unpublished(_)
                )
            })
            .map(|integration| {
                let _span =
                    tracing::info_span!("integration.republish", integration = integration.id())
                        .entered();
                PublicationOutcome {
                    integration: integration.id().to_owned(),
                    error: integration
                        .republish_packages(packages)
                        .err()
                        .map(|error| error.to_string()),
                }
            })
            .collect()
    }
}

/// The kind of artifact `integration` now plans for `resource`, if any.
fn planned_artifact(
    integration: &dyn IntegrationPort,
    resource: &Resource,
) -> Option<ManagedArtifact> {
    match integration.exposure_plan(resource).mechanism {
        ExposureMechanism::Managed(artifact) => Some(artifact),
        ExposureMechanism::Unsupported { .. } => None,
    }
}

/// Whether a config entry UZE wrote now says something else under the same
/// name — a server whose package root moved, say. A harness's config entry
/// is written only where nothing or the identical entry stands, so the one
/// UZE owns has to come off first. The name is left out: which candidate an
/// entry holds is the rename rule's question.
fn config_entry_restated(recorded: &ManagedArtifact, planned: &ManagedArtifact) -> bool {
    match (recorded, planned) {
        (
            ManagedArtifact::VendorConfigEntry {
                transport,
                command,
                args,
                cwd,
                environment,
                enabled,
                ..
            },
            ManagedArtifact::VendorConfigEntry {
                transport: planned_transport,
                command: planned_command,
                args: planned_args,
                cwd: planned_cwd,
                environment: planned_environment,
                enabled: planned_enabled,
                ..
            },
        ) => {
            (transport, command, args, cwd, environment, enabled)
                != (
                    planned_transport,
                    planned_command,
                    planned_args,
                    planned_cwd,
                    planned_environment,
                    planned_enabled,
                )
        }
        _ => false,
    }
}

/// Whether two artifacts compete for one name: an entry in the same place —
/// one discovery directory, one vendor registry, one hook file. A link and a
/// directory of the same name in one directory are one name.
fn same_name_space(
    a: &uze_core::integration::ManagedArtifact,
    b: &uze_core::integration::ManagedArtifact,
) -> bool {
    use uze_core::integration::ManagedArtifact as Artifact;
    match (a, b) {
        (
            Artifact::SymlinkReference { path: a, .. } | Artifact::GeneratedTree { path: a, .. },
            Artifact::SymlinkReference { path: b, .. } | Artifact::GeneratedTree { path: b, .. },
        )
        | (Artifact::GeneratedFile { path: a, .. }, Artifact::GeneratedFile { path: b, .. })
        | (Artifact::ManagedHookFile { path: a }, Artifact::ManagedHookFile { path: b }) => {
            a.parent() == b.parent()
        }
        (Artifact::VendorConfigEntry { .. }, Artifact::VendorConfigEntry { .. }) => true,
        (
            Artifact::HookConfigEntry { config_file: a, .. },
            Artifact::HookConfigEntry { config_file: b, .. },
        ) => a == b,
        _ => false,
    }
}
