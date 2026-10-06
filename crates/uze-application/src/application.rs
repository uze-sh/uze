//! Product-facing application boundary.
//!
//! CLI, TUI, and future presentation layers call this facade rather than
//! reaching into Store, integrations, vendor files, or lifecycle mechanics.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

use uze_core::{
    PackageSource, Result, UzeError, UzeHome, UzeStore,
    capability::CapabilityKind,
    context::{self as instruction_context},
    detection_cache::DetectionCache,
    exposure::{ExposureMechanism, ExposurePlan, PackageExposurePlan},
    integration::{
        AttachmentState, HarnessDetection, IntegrationPort, IntegrationStatus, PublicationStatus,
    },
    manifest::BUILT_IN_MARKETPLACE,
    preference::PreferencePort,
    provisioning::{
        ProcessRunner, ProvisionAction, ProvisionStatus, ProvisioningResult, SystemProcessRunner,
    },
    reconciliation::{
        PackageRemovalPlan, ReconciliationReport, reconcile_package, reconcile_package_with,
    },
    router::{CompatibilityRoute, HarnessCapabilities},
    state,
    store::StoredPackage,
    trust::{self, TrustOutcome, TrustRequest},
};
use uze_integrations::registry::IntegrationRegistry;

use crate::bootstrap;

mod agent_context;
mod authoring;
mod context;
mod doctor;
mod extensions;
mod freshness;
mod inspection_cache;
mod lifecycle;
mod maintenance;
mod managed_region;
mod marketplace;
mod marketplace_catalogue;
mod notifications;
pub mod offers;
mod overview;
mod profile;
mod project_environment;
mod read_models;
mod requirements;
mod runtime_shim;
pub mod services;
mod setup;
mod theme;

pub use agent_context::{AgentContextStatus, ResourceDelivery, UndeliveredReason};
pub use profile::{HarnessPreview, ProfileApplyResult, ProfilePreview, ProfileSummary};
pub use read_models::*;
pub use requirements::{PackageRequirements, RequirementLine, RequirementStatus};
pub use setup::ProvisionRoute;
pub use theme::{GlyphSetSummary, ThemeSummary};

pub use maintenance::{MaintenanceOutcome, MaintenanceReport};
pub use overview::{
    MarketplaceState, MemoryState, OverviewMarketplace, OverviewWorkspaceSummary,
    ProjectEnvironmentState, ProjectOverview,
};
use project_environment::ProjectEnvironmentPlan;
pub use project_environment::{
    InstallReport, ProjectLockStatus, RemoveProjectPluginReport, UpdateOutcome, UpdateReport,
    UpdateScope,
};
pub use uze_core::anchor::AnchorKind;

pub struct UzeApplication {
    home: UzeHome,
    store: UzeStore,
    integrations: Vec<Box<dyn IntegrationPort>>,
    /// Preference translation/apply adapters (Profiles feature). Empty by
    /// default from `new`/`new_with_runner` so the many existing call sites
    /// that construct fake `IntegrationPort`-only fixtures keep compiling
    /// unchanged; `from_env`/`from_env_with_runner` populate it from the
    /// same `IntegrationRegistry` that supplies `integrations`.
    preference_adapters: Vec<Box<dyn PreferencePort>>,
    runner: Box<dyn ProcessRunner>,
    detection_cache: DetectionCache,
    inspection_cache: crate::application::inspection_cache::InspectionCache,
    marketplace_catalogues: marketplace_catalogue::MarketplaceCatalogues,
    /// Mirrors this command already brought up to date. One command asks a
    /// remote one question once: updating three plugins from one
    /// marketplace fetched it three times, a second apart.
    mirrors_fetched: std::sync::Mutex<Vec<PathBuf>>,
}

impl UzeApplication {
    /// Starts one operation: what the last one fetched is not fresh for
    /// this one, whose own fetches are.
    pub(crate) fn begin_operation(&self) {
        if let Ok(mut fetched) = self.mirrors_fetched.lock() {
            fetched.clear();
        }
    }

    /// Production composition. The integration set comes from
    /// `IntegrationRegistry::builtin` — the one place that knows which
    /// harnesses exist; this layer only knows there are integrations.
    pub fn from_env(home: UzeHome) -> Result<Self> {
        let registry = IntegrationRegistry::builtin(&home)?;
        let (integrations, preference_adapters) = registry.into_parts();
        Ok(Self::new_with_runner_and_preferences(
            home,
            integrations,
            preference_adapters,
            Box::new(SystemProcessRunner),
        ))
    }

    /// Dependency-injected constructor for deterministic contract tests or
    /// embedded clients. It has the same application behavior as `from_env`.
    pub fn new(home: UzeHome, integrations: Vec<Box<dyn IntegrationPort>>) -> Self {
        Self::new_with_runner(home, integrations, Box::new(SystemProcessRunner))
    }

    /// Same production integration set as `from_env`, with an explicit
    /// process runner instead of the default `SystemProcessRunner`. For a
    /// caller that owns the terminal itself (the TUI's alternate screen), a
    /// vendor installer's inherited-output progress would otherwise print
    /// straight onto the real terminal and corrupt whatever is rendered
    /// there.
    pub fn from_env_with_runner(home: UzeHome, runner: Box<dyn ProcessRunner>) -> Result<Self> {
        let registry = IntegrationRegistry::builtin(&home)?;
        let (integrations, preference_adapters) = registry.into_parts();
        Ok(Self::new_with_runner_and_preferences(
            home,
            integrations,
            preference_adapters,
            runner,
        ))
    }

    /// Test and embedding composition point for the process runner used only
    /// by explicit harness provisioning. Package lifecycle remains entirely
    /// independent of process execution.
    pub fn new_with_runner(
        home: UzeHome,
        integrations: Vec<Box<dyn IntegrationPort>>,
        runner: Box<dyn ProcessRunner>,
    ) -> Self {
        Self::new_with_runner_and_preferences(home, integrations, Vec::new(), runner)
    }

    /// Like `new_with_runner`, additionally wiring preference adapters for
    /// the Profiles feature's `Profiles::apply`.
    pub fn new_with_runner_and_preferences(
        home: UzeHome,
        integrations: Vec<Box<dyn IntegrationPort>>,
        preference_adapters: Vec<Box<dyn PreferencePort>>,
        runner: Box<dyn ProcessRunner>,
    ) -> Self {
        Self {
            store: UzeStore::new(home.clone()),
            detection_cache: DetectionCache::new(&home),
            inspection_cache: inspection_cache::InspectionCache::new(&home),
            marketplace_catalogues: marketplace_catalogue::MarketplaceCatalogues::new(&home),
            mirrors_fetched: std::sync::Mutex::new(Vec::new()),
            home,
            integrations,
            preference_adapters,
            runner,
        }
    }

    /// The cached path for `IntegrationPort::detect()`: an in-process hit
    /// or a still-fresh on-disk entry (see `detection_cache::
    /// DetectionCache`) is returned with no subprocess spawned; only a
    /// genuine cache miss falls through to a live probe, whose result is
    /// then written through both cache tiers for the next caller — in
    /// this run and in the next CLI invocation alike. Every internal
    /// caller on a path that should stay fast (see
    /// `specs/cli-performance/spec.md`) must go through this rather than
    /// `integration.detect()` directly.
    pub(crate) fn detect_cached(&self, integration: &dyn IntegrationPort) -> HarnessDetection {
        let id = integration.id();
        let candidates = integration.detection_program_candidates();
        if let Some(cached) = self.detection_cache.get(id, &candidates) {
            return cached;
        }
        let _span = tracing::info_span!("integration.detect", integration = id).entered();
        let live = integration.detect();
        self.detection_cache.put(id, &candidates, live.clone());
        live
    }

    pub(crate) fn package_by_name(&self, name: &str) -> Result<StoredPackage> {
        // A plugin is addressable by its active local name (ADR-036) first —
        // its own bare plugin name unless an install-time alias resolved a
        // collision, in which case only one installed package ever answers
        // to a given name at all, so this can never be ambiguous. Falls
        // through to the qualified-id/bare-plugin-name lookup only for a
        // name nothing is currently active under (defensive: normal install
        // flows never leave two packages sharing a bare `plugin_name()`
        // with neither of them active under it).
        if let Some(id) = self.store.find_by_active_name(name)? {
            return self.store.package(&id);
        }
        let matches: Vec<_> = self
            .store
            .package_ids()?
            .into_iter()
            .filter(|id| id.as_str() == name || id.plugin_name() == name)
            .collect();
        match matches.as_slice() {
            [id] => self.store.package(id),
            [] => Err(UzeError::UnknownPackage(name.to_owned())),
            _ => Err(UzeError::ExposureUnavailable(format!(
                "plugin `{name}` is installed from multiple marketplaces; use `plugin@marketplace`"
            ))),
        }
    }

    pub(crate) fn plugin_summary(&self, package: &StoredPackage) -> Result<PluginSummary> {
        let resources = uze_core::engine::package_resources(package)?;
        Ok(PluginSummary {
            id: package.id.as_str().to_owned(),
            active_name: package.active_name.clone(),
            source: package.provenance.requested.display(),
            store_path: package.root.clone(),
            commit: match &package.provenance.resolved {
                uze_core::ResolvedSource::Git { commit, .. } => Some(commit.clone()),
                _ => None,
            },
            capability_count: resources.len(),
            freshness: self.freshness_of(package),
            installed_at_unix: package.written_at_unix(),
            requirement_gaps: self
                .requirement_check()
                .of(package)?
                .gaps()
                .cloned()
                .collect(),
            undelivered: state::undelivered(&self.home, package.id.as_str())?
                .into_iter()
                .map(|(integration, error)| UndeliveredHarness {
                    display_name: self.integration_named(&integration).map_or_else(
                        || integration.clone(),
                        |known| known.display_name().to_owned(),
                    ),
                    integration,
                    error,
                })
                .collect(),
        })
    }

    /// What the marketplace registered as `name` at `source` offers, from
    /// the catalogue cache (see `marketplace_catalogue`).
    pub(crate) fn catalogue(
        &self,
        name: &str,
        source: &PackageSource,
    ) -> Result<marketplace_catalogue::Catalogue> {
        self.marketplace_catalogues.read(name, source)
    }

    /// `catalogue`, for a reader that must answer without reaching a
    /// remote — every path a keystroke or a click is waiting on.
    pub(crate) fn catalogue_as_it_stands(
        &self,
        name: &str,
        source: &PackageSource,
    ) -> Result<marketplace_catalogue::Catalogue> {
        self.marketplace_catalogues.read_as_it_stands(name, source)
    }

    /// What the Store holds, read leniently: an unreadable registry lists as
    /// nothing. For listings only — anything that writes from the answer
    /// uses [`Self::installed_packages_checked`], since publishing "nothing"
    /// empties every harness's catalogue.
    pub(crate) fn installed_packages(&self) -> Vec<StoredPackage> {
        self.installed_packages_checked().unwrap_or_default()
    }

    pub(crate) fn installed_packages_checked(&self) -> Result<Vec<StoredPackage>> {
        self.store.packages()
    }

    /// The registered integration a person or a record names: by its stable
    /// id (`claude-code`), an alias people type (`claude`), or the label UZE
    /// shows back (`Claude Code`). There is deliberately no central list of
    /// vendors: an integration declares its own names, so registering one is
    /// the only step needed to make it selectable.
    pub(crate) fn integration_named(&self, name: &str) -> Option<&dyn IntegrationPort> {
        self.integrations
            .iter()
            .map(|integration| integration.as_ref())
            .find(|integration| {
                integration.id() == name
                    || integration.aliases().contains(&name)
                    || integration.display_name() == name
            })
    }

    pub(crate) fn resolve_integration_id(&self, requested: &str) -> Result<&'static str> {
        self.integration_named(requested)
            .map(|integration| integration.id())
            .ok_or_else(|| UzeError::UnknownHarness {
                requested: requested.to_owned(),
                known: self
                    .integrations
                    .iter()
                    .map(|integration| integration.id())
                    .collect::<Vec<_>>()
                    .join(", "),
            })
    }

    pub(crate) fn reconcile(&self, package_id: &str) -> ReconciliationReport {
        reconcile_package(&self.home, package_id, &self.integration_ports())
    }

    fn integration_ports(&self) -> Vec<&dyn IntegrationPort> {
        self.integrations
            .iter()
            .map(|integration| integration.as_ref() as &dyn IntegrationPort)
            .collect()
    }

    /// The READ-ONLY cousin of `reconcile`: same report shape, but each
    /// receipt's `Matched` verdict may come from the inspection cache
    /// (ADR 018) instead of a live vendor-CLI probe. Anomalies are never
    /// cached, so the report's warnings are always fresh. This is for
    /// report/health surfaces only — removal planning and detach MUST keep
    /// going through [`reconcile`](Self::reconcile), whose live verdict is
    /// what makes ownership checks trustworthy.
    pub(crate) fn reconcile_cached_report(&self, package_id: &str) -> ReconciliationReport {
        reconcile_package_with(
            &self.home,
            package_id,
            &self.integration_ports(),
            |ledger_key, receipt, integration| {
                let fingerprint = receipt.artifact.fingerprint();
                if let Some(cached) = self
                    .inspection_cache
                    .get(ledger_key, fingerprint.as_deref())
                {
                    return cached;
                }
                let _span = tracing::info_span!(
                    "integration.inspect",
                    integration = %receipt.integration,
                    receipt = %ledger_key
                )
                .entered();
                let live = integration.inspect_receipt(receipt);
                self.inspection_cache.put(ledger_key, &live, fingerprint);
                live
            },
        )
    }
}

#[cfg(test)]
mod setup_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tracing_tests;
