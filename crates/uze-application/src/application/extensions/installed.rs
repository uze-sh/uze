//! Which declared extensions installed plugins carry, and where their
//! files are read from.

use std::collections::BTreeMap;
use std::path::PathBuf;

use uze_contract::Identity;
use uze_core::acquisition::marketplace::{MarketplaceSubpath, repository_of};
use uze_core::capability::extension;

use crate::application::services::Extensions;

/// An extension an installed plugin carries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledExtension {
    pub identity: Identity,
    /// The extension's own directory, holding its `extension.kdl`.
    pub dir: PathBuf,
    /// Read from the working tree of a marketplace this machine develops,
    /// so a save there is what the workspace draws next. Every other
    /// extension is read from the Store's copy and never watched.
    pub linked: bool,
}

impl Extensions<'_> {
    /// Every declared extension the installed plugins carry, by identity.
    ///
    /// A plugin from a linked marketplace is read from its place in the
    /// working tree, so an author's save is seen without reinstalling;
    /// every other one from the bytes the Store holds. Reads directory
    /// entries and the marketplace registry only: none of the per-plugin
    /// checks a listing makes, since this runs whenever the workspace
    /// opens. Lenient: an unreadable registry or plugin offers nothing
    /// rather than failing the rest.
    #[tracing::instrument(name = "extensions.installed", skip_all)]
    pub fn installed(&self) -> Vec<InstalledExtension> {
        let marketplaces = uze_core::state::marketplace_list(&self.0.home).unwrap_or_default();
        // Asked once per marketplace: a local source's subpath is a
        // question to Git.
        let mut subpaths: BTreeMap<String, Option<MarketplaceSubpath>> = BTreeMap::new();
        let mut found = Vec::new();
        for package in self.0.installed_packages() {
            let marketplace = package.id.marketplace().to_owned();
            let plugin = package.id.plugin_name().to_owned();
            let linked_dir = marketplaces
                .get(&marketplace)
                .and_then(|record| Some((record.link.clone()?, record)))
                .and_then(|(checkout, record)| {
                    let subpath = subpaths
                        .entry(marketplace.clone())
                        .or_insert_with(|| {
                            repository_of(&record.source)
                                .ok()
                                .map(|repository| repository.subpath)
                        })
                        .clone()?;
                    subpath.linked_plugin_dir(&checkout, &plugin).ok()
                })
                .filter(|dir| dir.is_dir());
            let (plugin_dir, linked) = match linked_dir {
                Some(dir) => (dir, true),
                None => (package.root.clone(), false),
            };
            found.extend(extension::discover(&plugin_dir).into_iter().map(|entry| {
                InstalledExtension {
                    identity: Identity {
                        marketplace: marketplace.clone(),
                        plugin: plugin.clone(),
                        id: entry.id,
                    },
                    dir: entry.dir,
                    linked,
                }
            }));
        }
        found.sort_by(|a, b| a.identity.cmp(&b.identity));
        found
    }

    /// The extensions the installed package `marketplace`/`plugin`
    /// declares, read where [`Self::installed`] reads them, so each is
    /// the identity the Extensions screen lists it under.
    pub(crate) fn declared_by(&self, marketplace: &str, plugin: &str) -> Vec<Identity> {
        self.installed()
            .into_iter()
            .map(|installed| installed.identity)
            .filter(|identity| identity.marketplace == marketplace && identity.plugin == plugin)
            .collect()
    }
}

/// The extensions the package at `plugin_dir` declares, under the identity
/// it would carry once installed from `marketplace` as `plugin`.
pub(crate) fn declared_in(
    plugin_dir: &std::path::Path,
    marketplace: &str,
    plugin: &str,
) -> Vec<Identity> {
    extension::discover(plugin_dir)
        .into_iter()
        .map(|entry| Identity {
            marketplace: marketplace.to_owned(),
            plugin: plugin.to_owned(),
            id: entry.id,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::UzeApplication;
    use uze_core::UzeHome;

    const PLUGIN: &str = "uze-agent-skill-conformance";

    /// The conformance plugin with a `board` extension, staged as the one
    /// plugin of a marketplace repository at `market`.
    fn stage_market(market: &Path) -> std::path::PathBuf {
        let plugin = market.join("plugins").join(PLUGIN);
        uze_testkit::fixtures::copy_tree(&crate::application::tests::fixture(), &plugin);
        let extension = plugin.join("extensions/board");
        fs::create_dir_all(&extension).unwrap();
        fs::write(extension.join("extension.kdl"), "protocol 1\n").unwrap();
        // A directory without a manifest is not an extension.
        fs::create_dir_all(plugin.join("extensions/notes")).unwrap();
        let manifest = serde_json::json!({
            "name": "market",
            "plugins": [{ "name": PLUGIN, "source": format!("./plugins/{PLUGIN}") }],
        });
        fs::write(market.join("marketplace.json"), manifest.to_string()).unwrap();
        uze_testkit::git::commit_everything_in(market);
        plugin
    }

    fn install(app: &UzeApplication, plugin: &Path) {
        let acquired =
            uze_core::acquisition::acquire(&uze_core::PackageSource::local(plugin)).unwrap();
        app.plugins()
            .install_materialized(
                acquired,
                "market",
                None,
                &uze_core::trust::AlwaysTrust,
                &uze_core::naming::NoNameCollisionAuthority,
                uze_core::trust::SourceOrigin::Operator,
            )
            .unwrap();
    }

    fn canonical(path: &Path) -> std::path::PathBuf {
        path.canonicalize().unwrap()
    }

    #[test]
    fn a_plugin_from_a_marketplace_nobody_links_is_read_from_the_store() {
        let root = uze_testkit::temp::scratch("installed-extensions-store");
        let plugin = stage_market(&root.join("market"));
        let app = UzeApplication::new(UzeHome::at(root.join("home")), Vec::new());
        install(&app, &plugin);

        let installed = app.extensions().installed();
        assert_eq!(installed.len(), 1, "{installed:?}");
        let board = &installed[0];
        assert_eq!(board.identity.to_string(), format!("{PLUGIN}@market/board"));
        assert!(!board.linked);
        let stored = app.package_by_name(PLUGIN).unwrap().root;
        assert_eq!(
            canonical(&board.dir),
            canonical(&stored.join("extensions/board"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_plugin_from_a_linked_marketplace_is_read_from_its_working_tree() {
        let root = uze_testkit::temp::scratch("installed-extensions-linked");
        let market = root.join("market");
        let plugin = stage_market(&market);
        let home = UzeHome::at(root.join("home"));
        uze_core::state::marketplace_add(
            &home,
            "market",
            uze_core::PackageSource::Local {
                path: market.clone(),
            },
        )
        .unwrap();
        uze_core::state::marketplace_link(&home, "market", &market, Default::default()).unwrap();
        let app = UzeApplication::new(home, Vec::new());
        install(&app, &plugin);

        let installed = app.extensions().installed();
        assert_eq!(installed.len(), 1, "{installed:?}");
        assert!(installed[0].linked);
        assert_eq!(
            canonical(&installed[0].dir),
            canonical(&plugin.join("extensions/board")),
            "the author's own files, not the Store's copy"
        );
        fs::remove_dir_all(root).unwrap();
    }

    /// What a package provides is read from its contents: the extension it
    /// declares stands beside its skills, on offer and once installed, under
    /// the identity the Extensions screen lists it by.
    #[test]
    fn a_package_says_which_extensions_it_declares_on_offer_and_installed() {
        let root = uze_testkit::temp::scratch("declared-extensions");
        let market = root.join("market");
        let plugin = stage_market(&market);
        let home = UzeHome::at(root.join("home"));
        uze_core::state::marketplace_add(
            &home,
            "market",
            uze_core::PackageSource::Local {
                path: market.clone(),
            },
        )
        .unwrap();
        let app = UzeApplication::new(home, Vec::new());
        let board = format!("{PLUGIN}@market/board");

        let offered = app.marketplace().inspect_plugin("market", PLUGIN).unwrap();
        let said: Vec<String> = offered
            .declared_extensions
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(said, vec![board.clone()], "on offer, before any install");
        assert!(
            !offered.capabilities.is_empty(),
            "its skills stand beside it"
        );

        install(&app, &plugin);
        let installed = app.plugins().inspect(PLUGIN).unwrap();
        let said: Vec<String> = installed
            .declared_extensions
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            said,
            vec![board],
            "installed, as the Extensions screen lists it"
        );
        fs::remove_dir_all(root).unwrap();
    }

    /// A package that carries only extensions provides them, and nothing
    /// for a harness: what it brings is not nothing.
    #[test]
    fn an_extension_only_package_provides_its_extensions_and_no_capability() {
        let root = uze_testkit::temp::scratch("declared-extensions-only");
        let market = root.join("market");
        uze_testkit::fixtures::copy_tree(
            &uze_testkit::fixtures::canonical("gsd"),
            &market.join("plugins/gsd"),
        );
        let manifest = serde_json::json!({
            "name": "market",
            "plugins": [{ "name": "gsd", "source": "./plugins/gsd" }],
        });
        fs::write(market.join("marketplace.json"), manifest.to_string()).unwrap();
        uze_testkit::git::commit_everything_in(&market);
        let home = UzeHome::at(root.join("home"));
        uze_core::state::marketplace_add(
            &home,
            "market",
            uze_core::PackageSource::Local {
                path: market.clone(),
            },
        )
        .unwrap();
        let app = UzeApplication::new(home, Vec::new());

        let offered = app.marketplace().inspect_plugin("market", "gsd").unwrap();
        let said: Vec<String> = offered
            .declared_extensions
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(said, vec!["gsd@market/gsd-band", "gsd@market/gsd-panel"]);
        assert!(
            offered.capabilities.is_empty(),
            "{:?}",
            offered.capabilities
        );
        fs::remove_dir_all(root).unwrap();
    }
}
