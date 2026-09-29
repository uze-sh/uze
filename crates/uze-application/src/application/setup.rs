//! Bringing this machine's harnesses up: seeding the default plugins,
//! provisioning and preparing each detected harness, and `uze setup`.

use super::*;

impl UzeApplication {
    /// Ensures every plugin `bootstrap::DEFAULT_PLUGIN_IDS` names is present
    /// in the Store. Each default plugin's *first install* goes through the
    /// exact same lifecycle any other install does
    /// (`Plugins::install_materialized`), so Store, Engine, Router and every
    /// `IntegrationPort` stay unaware any of this is a "default" rather than
    /// an ordinary installed plugin.
    ///
    /// This is BOOTSTRAP, not UPDATE: an already-installed default plugin is
    /// never touched here, no matter how its content compares to the
    /// embedded marketplace snapshot — this runs on every CLI invocation
    /// (including read-only ones like `doctor`/`list`), and an observational
    /// command must not mutate installed plugin content. A newer snapshot is
    /// surfaced as `PluginSummary::update_available` (a pure read) for an
    /// explicit `Plugins::update` to act on later, not applied silently. See
    /// `docs/architecture/invariants.md`'s "Official marketplace" section.
    ///
    /// Idempotent. Returns `true` if it installed at least one Store entry.
    ///
    /// This is deliberately not called from `from_env`/`new` so contract
    /// tests can construct isolated worlds with no default plugins. The CLI
    /// (`src/main.rs`) and `setup` call this explicitly.
    ///
    /// A mutation lock another UZE holds skips the whole pass rather than
    /// failing the command it runs ahead of: the next command seeds instead.
    /// Nothing to seed or republish takes no lock at all, because taking one
    /// writes its holder's pid and a warm bootstrap writes nothing.
    pub fn ensure_default_plugins(&self) -> Result<bool> {
        if self.bootstrap_is_settled() {
            let _span = tracing::info_span!("bootstrap.ensure_default_plugins").entered();
            let _ = self.prepare_detected_integrations();
            return Ok(false);
        }
        let _mutation = match uze_core::persistence::MutationLock::acquire(&self.home) {
            Ok(mutation) => mutation,
            Err(UzeError::MutationInProgress { .. }) => return Ok(false),
            Err(error) => return Err(error),
        };
        self.ensure_default_plugins_locked()
    }

    /// Every default plugin is in the Store and every derived view matches
    /// the installed set — read, never written.
    fn bootstrap_is_settled(&self) -> bool {
        let Ok(packages) = self.installed_packages_checked() else {
            return false;
        };
        let seeded = bootstrap::DEFAULT_PLUGIN_IDS.iter().all(|id| {
            let qualified = format!("{id}@{BUILT_IN_MARKETPLACE}");
            packages
                .iter()
                .any(|package| package.id.as_str() == qualified)
        });
        seeded
            && self.integrations.iter().all(|integration| {
                !matches!(
                    integration.publication(&packages),
                    PublicationStatus::Unpublished(_)
                )
            })
    }

    /// `ensure_default_plugins` for a caller already holding the mutation
    /// lock, which is not reentrant.
    fn ensure_default_plugins_locked(&self) -> Result<bool> {
        let _span = tracing::info_span!("bootstrap.ensure_default_plugins").entered();
        let mut installed_any = false;
        for &id in bootstrap::DEFAULT_PLUGIN_IDS {
            installed_any |= self.ensure_default_plugin_installed(id)?;
        }
        // Prepares detected harnesses (creating `~/.claude/skills` etc.) so a
        // harness detected since the last run does not wait for an explicit
        // `uze setup`. Best-effort: `setup`/`doctor` surface a failure.
        let _ = self.prepare_detected_integrations();
        // A Generated Native Package's own catalogue is written by
        // republishing, and a vendor CLI reading a missing one fails
        // outright. Only a view that no longer matches the installed set is
        // rewritten: this runs before every command, and rewriting every
        // catalogue (each a synced atomic write) to say what it already said
        // was most of what a read-only command cost.
        if let Ok(packages) = self.installed_packages_checked() {
            let _ = self.republish_unpublished(&packages);
        }
        Ok(installed_any)
    }

    /// Installs default plugin `id` if it is not already in the Store.
    /// Never touches an already-installed copy — see `ensure_default_plugins`.
    pub(crate) fn ensure_default_plugin_installed(&self, id: &str) -> Result<bool> {
        let already_installed = self
            .store
            .package_ids()?
            .iter()
            .any(|package_id| package_id.as_str() == format!("{id}@{BUILT_IN_MARKETPLACE}"));
        if already_installed {
            return Ok(false);
        }
        let materialized = bootstrap::materialize(id)?;
        match self.plugins().install_materialized(
            materialized,
            BUILT_IN_MARKETPLACE,
            None,
            &trust::NoTrustAuthority,
            &uze_core::naming::NoNameCollisionAuthority,
        ) {
            Ok(_) => Ok(true),
            Err(error) => {
                // Production resilience: a foreign-state failure on one
                // harness must not abort bootstrap for the others nor fail
                // the whole `setup` on a user's real machine. But "installed"
                // is a fact about the Store, not a consolation: trust can be
                // refused, a harness can refuse to be prepared, and the ingest
                // itself can fail, all of them before a byte is written. Ask
                // the Store instead of assuming.
                let installed = self.store.package_ids().is_ok_and(|ids| {
                    ids.iter().any(|package_id| {
                        package_id.as_str() == format!("{id}@{BUILT_IN_MARKETPLACE}")
                    })
                });
                tracing::warn!(
                    plugin = id,
                    installed,
                    error = %error,
                    "a default plugin could not be installed completely"
                );
                Ok(installed)
            }
        }
    }

    /// Runs only selected, detected setup routines. No integration knowledge
    /// leaks to the caller beyond stable ids and reported facts.
    ///
    /// Resilience contract (production environments): a single harness's
    /// attach or shim failure never aborts the whole `setup` run. Failures
    /// are collected per-harness into `SetupResult::attach_error` /
    /// `shim_error` and surfaced as warnings — the caller still gets a
    /// `Vec<SetupResult>` with one entry per harness, and `doctor` shows the
    /// same facts via reconciliation.
    pub fn setup(&self, requested: Option<&str>) -> Result<Vec<SetupResult>> {
        let _mutation = uze_core::persistence::MutationLock::acquire(&self.home)?;
        // Seed the default marketplace plugins before any provisioning, so a
        // fresh `UZE_HOME` gets the Skill without a manual `uze add` and so
        // an updated binary heals its attachment on next `setup`.
        let _ = self.ensure_default_plugins_locked();
        let wanted = requested
            .map(|name| self.resolve_integration_id(name))
            .transpose()?;
        let mut results = self.provision_and_prepare(wanted);
        // `setup` is the documented way to repair a derived view that
        // failed to publish, so it always rebuilds them.
        let _ = self.republish_all();
        for result in results.iter_mut().filter(|result| result.configured) {
            if let Some(integration) = self
                .integrations
                .iter()
                .find(|integration| integration.id() == result.integration)
            {
                match self.attach_stored_packages_to(integration.as_ref()) {
                    Ok(()) => {}
                    Err(error) => {
                        result.attach_error = Some(error.to_string());
                    }
                }
                match self.ensure_runtime_shim(
                    integration.as_ref(),
                    result.provisioning.located_outside_path.as_deref(),
                ) {
                    Ok(shim) => result.runtime_shim = shim,
                    Err(error) => {
                        result.shim_error = Some(error.to_string());
                    }
                }
            }
        }
        Ok(results)
    }

    /// Explicit setup is the only path allowed to provision or update an
    /// executable. `add` deliberately calls only `prepare_detected_*`.
    ///
    /// One entry per harness whatever happens to it: a harness that fails
    /// to provision or to be prepared is reported as failed, never as the
    /// end of everybody else's setup.
    pub(crate) fn provision_and_prepare(&self, requested: Option<&str>) -> Vec<SetupResult> {
        self.integrations
            .iter()
            .filter(|integration| requested.is_none_or(|id| integration.id() == id))
            .map(|integration| self.provision_and_prepare_one(integration.as_ref()))
            .collect()
    }

    fn provision_and_prepare_one(&self, integration: &dyn IntegrationPort) -> SetupResult {
        let provisioning = {
            let _span =
                tracing::info_span!("integration.provision", integration = integration.id())
                    .entered();
            integration
                .provision(self.runner.as_ref())
                .unwrap_or_else(|error| {
                    ProvisioningResult::failed(
                        ProvisionAction::None,
                        "provision",
                        error.to_string(),
                    )
                })
        };
        let prepared = state::record_provisioning(&self.home, integration.id(), &provisioning)
            .and_then(|()| {
                if provisioning.status != ProvisionStatus::Verified {
                    return Ok(());
                }
                integration.install(&self.home, &provisioning.detection)?;
                // Write-through (ADR 018 decision 3): `provision()`
                // already verified this result, so record it in the
                // cache directly instead of leaving the pre-action
                // entry to be caught later by a read-time fingerprint
                // check — a UZE-driven install/update has no stale
                // window, and no separate probe is spent to get that.
                self.detection_cache.put(
                    integration.id(),
                    &integration.detection_program_candidates(),
                    provisioning.detection.clone(),
                );
                Ok(())
            });
        let provisioning = match prepared {
            Ok(()) => provisioning,
            Err(error) => ProvisioningResult {
                status: ProvisionStatus::Failed,
                reason: Some(error.to_string()),
                ..provisioning
            },
        };
        SetupResult {
            integration: integration.id().to_owned(),
            detection: provisioning.detection.clone(),
            configured: provisioning.status == ProvisionStatus::Verified,
            provisioning,
            // Only explicit `setup()` wires up `ensure_runtime_shim` —
            // this helper also backs `add`'s implicit preparation,
            // which must never silently create a PATH shim.
            runtime_shim: None,
            attach_error: None,
            shim_error: None,
        }
    }

    /// Prepares integrations only when their real executable is present.
    /// This is the shared bridge between explicit `setup` and implicit
    /// preparation during an install; neither presentation layer needs to
    /// know which directories/configuration an integration owns.
    pub(crate) fn prepare_detected_integrations(&self) -> Result<()> {
        for integration in &self.integrations {
            let detection = self.detect_cached(integration.as_ref());
            if detection.present {
                let _span =
                    tracing::debug_span!("integration.install", integration = integration.id())
                        .entered();
                integration.install(&self.home, &detection)?;
            }
        }
        Ok(())
    }
}
