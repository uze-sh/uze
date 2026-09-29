//! What a package's delivery to one harness would be, computed without
//! attaching anything: the effective view `uze inspect` shows and the
//! intent `uze doctor` checks the machine against.
//!
//! It takes every decision [`deliver_package_to`] takes, in the same order
//! and through the same integration calls, so the view and the install
//! report cannot disagree about a route or a shortfall.
//!
//! [`deliver_package_to`]: UzeApplication::deliver_package_to

use std::collections::BTreeSet;

use uze_core::{
    capability::{CapabilityKind, Resource},
    exposure::{ExposureMechanism, PackageExposurePlan},
    integration::{AttachmentState, IntegrationPort},
    router::CompatibilityRoute,
    state,
    store::StoredPackage,
};

use super::super::*;
use super::attach::{CapabilityShortfall, refuses_one_name};

/// One package's delivery to one harness, as planned.
pub(crate) struct PlannedDelivery {
    pub package_plan: Option<PackageExposurePlan>,
    pub route: DeliveryRoute,
    pub capabilities: Vec<CapabilityDelivery>,
    pub shortfalls: Vec<CapabilityShortfall>,
}

impl PlannedDelivery {
    /// The capabilities the harness is expected to hold once the package
    /// is delivered: every one with a route, that nothing blocks, and that
    /// is not project context (which reaches a harness through `AGENTS.md`).
    pub(crate) fn expected(&self) -> impl Iterator<Item = &CapabilityDelivery> {
        self.capabilities.iter().filter(|capability| {
            capability.kind != CapabilityKind::Instruction
                && capability.route != CompatibilityRoute::Unsupported
                && capability.blocked.is_none()
        })
    }
}

impl From<CapabilityShortfall> for CapabilityShortfallReport {
    fn from(shortfall: CapabilityShortfall) -> Self {
        Self {
            capability: shortfall.capability,
            route: shortfall.route,
            evidence: shortfall.evidence,
        }
    }
}

/// The route a delivery takes when the package plan was offered or not,
/// and set aside or not. Shared with the install report.
pub(crate) fn delivery_route_of(
    plan: Option<&PackageExposurePlan>,
    plan_set_aside: bool,
) -> DeliveryRoute {
    match plan {
        Some(_) if plan_set_aside => DeliveryRoute::CapabilityByCapability {
            reason: "capabilities already delivered one by one could not be safely replaced by \
                     the package"
                .to_owned(),
        },
        Some(plan) => DeliveryRoute::Package {
            envelope: plan.envelope,
            route: plan.route,
            evidence: plan.evidence.clone(),
        },
        None => DeliveryRoute::CapabilityByCapability {
            reason: "the harness has no package-level delivery for this package".to_owned(),
        },
    }
}

impl UzeApplication {
    /// The read-only twin of `deliver_package_to`.
    pub(crate) fn plan_delivery_to(
        &self,
        package: &StoredPackage,
        resources: &[&Resource],
        integration: &dyn IntegrationPort,
    ) -> PlannedDelivery {
        let package_plan = integration.package_exposure_plan(package, resources);
        let package_receipt = state::receipts(&self.home, Some(package.id.as_str()))
            .unwrap_or_default()
            .into_iter()
            .find(|receipt| {
                receipt.integration == integration.id()
                    && receipt.resource_identity.is_none()
                    && integration.package_receipt_serves(receipt)
            });
        let plan_set_aside = package_plan.as_ref().is_some_and(|plan| {
            package_receipt.is_none() && self.covered_receipts_unsafe(package, integration, plan)
        });
        let provided: BTreeSet<String> = package_plan
            .as_ref()
            .filter(|_| !plan_set_aside)
            .map(|plan| plan.provided_resource_identities.clone())
            .unwrap_or_default();
        let package_location = package_receipt
            .as_ref()
            .map(|receipt| receipt.artifact.location());
        let mut capabilities = Vec::new();
        let mut shortfalls = Vec::new();
        for resource in resources {
            let identity = resource.identity();
            if provided.contains(&identity) {
                let (route, evidence) = integration
                    .packaged_shortfall(package, resource)
                    .unwrap_or_else(|| {
                        (
                            CompatibilityRoute::Native,
                            "loaded by the harness from the package".to_owned(),
                        )
                    });
                shortfalls.extend(CapabilityShortfall::of(resource, (route, evidence.clone())));
                capabilities.push(CapabilityDelivery {
                    identity,
                    kind: resource.capability.kind,
                    provided_by_package: true,
                    plan: None,
                    exposed_name: integration.packaged_exposure_name(package, resource),
                    route,
                    location: package_location.clone(),
                    evidence,
                    blocked: None,
                });
                continue;
            }
            let resolved = match self.resolve_exposure_name(resource, integration) {
                Ok(resolved) => resolved,
                Err(error) => {
                    let plan = integration.exposure_plan(resource);
                    capabilities.push(CapabilityDelivery {
                        identity,
                        kind: resource.capability.kind,
                        provided_by_package: false,
                        route: plan.route,
                        evidence: plan.evidence.clone(),
                        plan: Some(plan),
                        exposed_name: None,
                        location: None,
                        blocked: Some(if refuses_one_name(&error) {
                            error.to_string()
                        } else {
                            format!("its name could not be resolved: {error}")
                        }),
                    });
                    continue;
                }
            };
            let plan = integration.exposure_plan(&resolved);
            let (route, location, artifact_name) = match &plan.mechanism {
                ExposureMechanism::Managed(artifact) => (
                    plan.route,
                    Some(artifact.location()),
                    artifact.exposure_name(),
                ),
                ExposureMechanism::Unsupported { .. } => {
                    (CompatibilityRoute::Unsupported, None, None)
                }
            };
            if resolved.capability.kind != CapabilityKind::Instruction {
                shortfalls.extend(CapabilityShortfall::of(
                    &resolved,
                    (route, plan.evidence.clone()),
                ));
            }
            capabilities.push(CapabilityDelivery {
                identity,
                kind: resource.capability.kind,
                provided_by_package: false,
                exposed_name: (route != CompatibilityRoute::Unsupported)
                    .then(|| resolved.resolved_exposure_name.clone().or(artifact_name))
                    .flatten(),
                route,
                location,
                evidence: plan.evidence.clone(),
                plan: Some(plan),
                blocked: None,
            });
        }
        PlannedDelivery {
            route: delivery_route_of(package_plan.as_ref(), plan_set_aside),
            package_plan,
            capabilities,
            shortfalls,
        }
    }

    /// Whether a capability receipt the package plan would replace is one
    /// delivery must leave alone, which keeps the package from attaching.
    fn covered_receipts_unsafe(
        &self,
        package: &StoredPackage,
        integration: &dyn IntegrationPort,
        plan: &PackageExposurePlan,
    ) -> bool {
        self.covered_receipts(package, integration, plan)
            .unwrap_or_default()
            .iter()
            .any(|receipt| {
                matches!(
                    integration.inspect_receipt(receipt).state,
                    AttachmentState::Drifted | AttachmentState::Conflict | AttachmentState::Blocked
                )
            })
    }
}
