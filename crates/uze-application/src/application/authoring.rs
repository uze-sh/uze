//! The authoring orchestration: scaffold → register → link, scaffold →
//! manifest entry, and the offline check, each one deterministic answer.
//!
//! Every verb here is machine-scoped by construction (ADR-019): the
//! marketplace record lives in the machine's own registry, the bytes stay
//! in the directory the author named, and no project file is touched. A
//! scaffolded marketplace is born registered **and linked** to the
//! directory it was scaffolded in, so installs read the author's working
//! tree — including not-yet-committed files — from the first moment.

use std::path::{Path, PathBuf};

use uze_core::{
    PackageSource, Result, UzeError,
    authoring::{self, ScaffoldCapabilities},
    manifest::BUILT_IN_MARKETPLACE,
    project_root,
};

use super::services::Project;

impl Project<'_> {
    /// Scaffolds a marketplace at `at`, registers it under `name`, and
    /// links it to the directory it was born in — one deterministic step.
    ///
    /// Registration goes through the same `market add` path an operator's
    /// registration takes, so a scaffolded marketplace is validated
    /// exactly like one anybody added by hand; the link reuses the
    /// registry's own machinery, which refuses a checkout that is some
    /// other repository.
    #[tracing::instrument(name = "authoring.marketplace_create", skip_all, fields(name = %name, at = %at.display()), err)]
    pub fn create_marketplace(
        &self,
        name: &str,
        description: Option<&str>,
        at: &Path,
    ) -> Result<MarketplaceCreated> {
        let _mutation = uze_core::persistence::MutationLock::acquire(&self.0.home)?;
        self.refuse_a_taken_name(name)?;
        let root = authoring::scaffold_marketplace(name, description, at)?;
        // The same registration path an operator's `market add` takes —
        // the manifest is validated the same way, and the marketplace is
        // named by what the manifest itself says. The source is handed
        // over typed: an author's directory is a local path, never a
        // shorthand to guess at.
        self.0
            .marketplace()
            .register_typed_source(&PackageSource::Local { path: root.clone() })?;
        uze_core::state::marketplace_link(&self.0.home, name, &root, Default::default())?;
        self.0.marketplace_catalogues.invalidate(name);
        Ok(MarketplaceCreated {
            name: name.to_owned(),
            root,
            committed: true,
        })
    }

    /// Scaffolds a **local** marketplace: the project the command runs in
    /// is itself the marketplace — `marketplace.json` at its root, plugins
    /// in `plugins_dir` — registered under `name` and linked to the project
    /// root, which is what makes installs read the project's working tree.
    ///
    /// No Git is touched and no commit is made: the marketplace's bytes are
    /// the project's own content, carried by the project's own commit flow.
    /// That is the local frontier's contract, and the one cost that comes
    /// with it — an author working in an isolated slot reaches the
    /// marketplace through delivery, like every other piece of project
    /// content.
    #[tracing::instrument(name = "authoring.local_marketplace_create", skip_all, fields(name = %name, root = %root.display()), err)]
    pub fn create_local_marketplace(
        &self,
        name: &str,
        description: Option<&str>,
        plugins_dir: &str,
        root: &Path,
    ) -> Result<MarketplaceCreated> {
        let Some(canonical) = project_root::resolve_project_root(root)? else {
            return Err(UzeError::NoProject {
                hint: "a local marketplace is the project's own — stand inside one, or use \
                       `--at <dir>` for a marketplace with a checkout of its own"
                    .to_owned(),
            });
        };
        let _mutation = uze_core::persistence::MutationLock::acquire(&self.0.home)?;
        self.refuse_a_taken_name(name)?;
        // The link needs a revision to read; asked now, a project with no
        // commit is told so before it carries a manifest it cannot use.
        uze_core::acquisition::marketplace::repository_of(&PackageSource::Local {
            path: canonical.clone(),
        })?;
        let (root, _) =
            authoring::scaffold_local_marketplace(name, description, &canonical, plugins_dir)?;
        self.0
            .marketplace()
            .register_typed_source(&PackageSource::Local { path: root.clone() })?;
        uze_core::state::marketplace_link(&self.0.home, name, &root, Default::default())?;
        self.0.marketplace_catalogues.invalidate(name);
        Ok(MarketplaceCreated {
            name: name.to_owned(),
            root,
            committed: false,
        })
    }

    /// A scaffold names a marketplace the registry does not know yet: the
    /// registration after it would otherwise fail with the directory
    /// already written and committed.
    fn refuse_a_taken_name(&self, name: &str) -> Result<()> {
        if name == BUILT_IN_MARKETPLACE {
            return Err(UzeError::ReservedMarketplace(name.to_owned()));
        }
        if uze_core::state::marketplace_get(&self.0.home, name)?.is_some() {
            return Err(UzeError::MarketplaceScaffold(format!(
                "`{name}` is already registered on this machine — choose another name, or \
                 author into it with `uze agent plugin create <plugin> --market {name}`"
            )));
        }
        Ok(())
    }

    /// Scaffolds one plugin inside a marketplace this machine knows by
    /// `name`, and adds the entry that makes it installable.
    #[tracing::instrument(name = "authoring.plugin_create", skip_all, fields(name = %name, market = %market), err)]
    pub fn create_plugin(
        &self,
        market: &str,
        name: &str,
        description: Option<&str>,
        category: Option<&str>,
        caps: ScaffoldCapabilities,
    ) -> Result<PluginCreated> {
        let checkout = self.marketplace_checkout(market)?;
        let root = authoring::scaffold_plugin(&checkout, name, description, category, &caps)?;
        Ok(PluginCreated {
            name: name.to_owned(),
            market: market.to_owned(),
            root,
        })
    }

    /// Where a registered marketplace's bytes are on this machine: a
    /// linked checkout answers first (an author edits it), else the
    /// registered source when it is a local path. A Git-registered
    /// marketplace has no directory to author in — its mirror is UZE's
    /// cache, not the author's own text.
    fn marketplace_checkout(&self, market: &str) -> Result<PathBuf> {
        let record = uze_core::state::marketplace_get(&self.0.home, market)?
            .ok_or_else(|| UzeError::UnknownMarketplace(market.to_owned()))?;
        let checkout = record.link.clone().ok_or_else(|| {
            UzeError::MarketplaceScaffold(format!(
                "`{market}` is not linked to a checkout on this machine — authoring needs the \
                 marketplace the author edits, so scaffold a marketplace or `uze market link \
                 {market} <checkout>` first"
            ))
        })?;
        uze_core::acquisition::marketplace::repository_of(&record.source)?
            .subpath
            .directory_in(&checkout)
    }

    /// The offline check: what the authored artifact would deliver, and
    /// every finding the install would have surfaced.
    #[tracing::instrument(name = "authoring.check", skip_all, fields(path = %path.display()), err)]
    pub fn check(&self, path: &Path, as_marketplace: bool) -> Result<authoring::ValidationReport> {
        if as_marketplace {
            return authoring::check_marketplace(path);
        }
        let mut report = authoring::check_plugin(path)?;
        self.check_per_harness(path, &mut report);
        Ok(report)
    }

    /// Every integration's own layer over the common check, prefixed with
    /// the harness it speaks for; and a `harness:` key that names no
    /// harness this build knows, which is left unused rather than refused,
    /// since the plugin may target one a later build adds.
    fn check_per_harness(&self, path: &Path, report: &mut authoring::ValidationReport) {
        let Ok(manifest) = uze_core::store::read_plugin_manifest(path) else {
            return;
        };
        let Ok(id) =
            uze_core::store::PackageId::from_plugin_name(&manifest.name, &path.join("plugin.json"))
        else {
            return;
        };
        let Ok(resources) = uze_core::engine::package_resources_at(&id, path) else {
            return;
        };
        let known: Vec<&str> = self
            .0
            .integrations
            .iter()
            .flat_map(|integration| integration.harness_keys())
            .collect();
        for resource in &resources {
            let located = resource.capability.path.display();
            if let Some((frontmatter, _)) = std::str::from_utf8(&resource.capability.payload)
                .ok()
                .and_then(uze_core::capability::harness::frontmatter_of)
                && let Ok(blocks) = uze_core::capability::harness::blocks(&frontmatter)
            {
                for key in blocks.keys() {
                    if !known.contains(&key.as_str()) {
                        report.warnings.push(format!(
                            "{located}: `harness.{key}` names no harness this build delivers \
                             to; it is left unused"
                        ));
                    }
                }
            }
            for integration in &self.0.integrations {
                let findings = integration.check_capability(resource);
                let harness = integration.display_name();
                report.findings.extend(
                    findings
                        .errors
                        .into_iter()
                        .map(|error| format!("{located}: {harness}: {error}")),
                );
                report.warnings.extend(
                    findings
                        .warnings
                        .into_iter()
                        .map(|warning| format!("{located}: {harness}: {warning}")),
                );
            }
        }
    }
}

/// A scaffolded marketplace's answer: where the bytes are, and that the
/// machine registry now carries it linked.
#[derive(Clone, Debug, serde::Serialize)]
pub struct MarketplaceCreated {
    pub name: String,
    pub root: PathBuf,
    /// The initial commit the scaffold made — what makes this directory a
    /// marketplace at all.
    pub committed: bool,
}

/// A scaffolded plugin: where it is, and the marketplace it is installable
/// from right now.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PluginCreated {
    pub name: String,
    pub market: String,
    pub root: PathBuf,
}

#[cfg(test)]
mod tests {
    use crate::{UzeApplication, UzeHome};
    use uze_core::integration::IntegrationPort;
    use uze_core::{PackageSource, UzeError};

    /// A harness this build knows as `known`.
    struct Known;

    impl IntegrationPort for Known {
        fn id(&self) -> &'static str {
            "known"
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
    }

    /// A name already registered is refused before anything is written,
    /// with a way forward; the bundled marketplace's name is reserved.
    #[test]
    fn a_marketplace_is_not_scaffolded_under_a_taken_name() {
        let root = uze_testkit::temp::scratch("authoring-taken-name");
        let home = UzeHome::at(root.join("uze"));
        uze_core::state::marketplace_add(
            &home,
            "taken",
            PackageSource::Local {
                path: root.join("elsewhere"),
            },
        )
        .unwrap();
        let app = UzeApplication::new(home, Vec::new());
        let at = root.join("new-market");

        let refused = app
            .project()
            .create_marketplace("taken", None, &at)
            .expect_err("the name is taken");
        assert!(
            matches!(refused, UzeError::MarketplaceScaffold(_)),
            "{refused:?}"
        );
        assert!(!at.exists(), "nothing was written");
        let reserved = app
            .project()
            .create_marketplace(uze_core::manifest::BUILT_IN_MARKETPLACE, None, &at)
            .expect_err("the bundled name is reserved");
        assert!(
            matches!(reserved, UzeError::ReservedMarketplace(_)),
            "{reserved:?}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// A plugin is scaffolded inside the checkout its marketplace is
    /// linked to.
    #[test]
    fn a_plugin_is_scaffolded_in_the_linked_checkout() {
        let root = uze_testkit::temp::scratch("authoring-plugin-create");
        let checkout = root.join("market");
        std::fs::create_dir_all(&checkout).unwrap();
        std::fs::write(
            checkout.join("marketplace.json"),
            r#"{"name":"market","plugins":[]}"#,
        )
        .unwrap();
        uze_testkit::git::commit_everything_in(&checkout);
        let home = UzeHome::at(root.join("uze"));
        uze_core::state::marketplace_add(
            &home,
            "market",
            PackageSource::Local {
                path: checkout.clone(),
            },
        )
        .unwrap();
        uze_core::state::marketplace_link(&home, "market", &checkout, Default::default()).unwrap();
        let app = UzeApplication::new(home, Vec::new());

        let created = app
            .project()
            .create_plugin("market", "flow", None, None, Default::default())
            .unwrap();
        assert_eq!(
            uze_platform::path::canonical(&created.root).unwrap(),
            uze_platform::path::canonical(&checkout.join("plugins").join("flow")).unwrap()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// A per-harness block naming a harness this build does not deliver to
    /// is warned about and left unused; one naming a known harness is not.
    #[test]
    fn a_harness_block_for_an_unknown_harness_is_warned_about() {
        let root = uze_testkit::temp::scratch("authoring-harness-keys");
        let plugin = root.join("flow");
        std::fs::create_dir_all(plugin.join("skills/flow")).unwrap();
        std::fs::write(plugin.join("plugin.json"), r#"{"name":"flow"}"#).unwrap();
        std::fs::write(
            plugin.join("skills/flow/SKILL.md"),
            "---\nname: flow\ndescription: d\nharness:\n  known:\n    model: a\n  elsewhere:\n    model: b\n---\n\nFlow.\n",
        )
        .unwrap();
        let app = UzeApplication::new(UzeHome::at(root.join("uze")), vec![Box::new(Known)]);

        let report = app.project().check(&plugin, false).unwrap();
        let about = |key: &str| {
            report
                .warnings
                .iter()
                .any(|warning| warning.contains(&format!("harness.{key}")))
        };
        assert!(about("elsewhere"), "{:?}", report.warnings);
        assert!(!about("known"), "{:?}", report.warnings);
        let _ = std::fs::remove_dir_all(root);
    }
}
