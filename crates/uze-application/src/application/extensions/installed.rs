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
        assert_eq!(
            board.identity.to_string(),
            format!("board · {PLUGIN}@market")
        );
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
        uze_core::state::marketplace_link(&home, "market", &market).unwrap();
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
}
