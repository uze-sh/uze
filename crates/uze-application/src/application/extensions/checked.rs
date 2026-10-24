//! A plugin's declared extensions checked before any install: compiled by
//! the rules the workspace hosts them with, and their fixtures read the way
//! the workspace reads them.
//!
//! Binding a view to its fixture is the engine's, which this crate does not
//! reach; what compiled is handed back with its fixtures and the text of
//! its views, so the caller binds them and places what it finds.

use std::collections::BTreeMap;
use std::path::Path;

use uze_contract::{CompiledExtension, Identity};
use uze_core::capability::extension;

use super::compiled::read_fixture;
use crate::application::services::Extensions;

/// The marketplace an extension is compiled under when its plugin is
/// checked from a directory: the one a plugin installed from a path gets.
const CHECKED_MARKETPLACE: &str = "local";

/// One extension a plugin carries, as the check found it.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CheckedExtension {
    pub id: String,
    /// Each problem that keeps it from being hosted, located in its files
    /// as `file:line:column: message (suggestion)`. Empty is clean.
    pub errors: Vec<String>,
    /// What still hosts but draws less than the author wrote; the caller
    /// adds what binding found.
    pub warnings: Vec<String>,
    /// What compiled, when nothing kept it from compiling.
    #[serde(skip)]
    pub compiled: Option<CompiledExtension>,
    /// Each source's fixture that read as JSON, by source id.
    #[serde(skip)]
    pub fixtures: BTreeMap<String, serde_json::Value>,
    /// Each compiled view's text, by its file, to place a span in.
    #[serde(skip)]
    views: BTreeMap<String, String>,
}

impl CheckedExtension {
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }

    /// The one-based line and column `offset` falls at in `view`.
    pub fn place(&self, view: &str, offset: usize) -> Option<(usize, usize)> {
        let text = self.views.get(view)?;
        let before = text.get(..offset)?;
        let line = before.matches('\n').count() + 1;
        let column = before
            .rsplit_once('\n')
            .map_or(before, |(_, last)| last)
            .chars()
            .count()
            + 1;
        Some((line, column))
    }
}

impl Extensions<'_> {
    /// Every extension `plugin_dir` carries, compiled and with its
    /// fixtures read. Compiles every time: an author checking is fixing
    /// what the last check said.
    #[tracing::instrument(name = "extensions.check", skip_all, fields(plugin = %plugin_dir.display()))]
    pub fn check(&self, plugin_dir: &Path) -> Vec<CheckedExtension> {
        let plugin = uze_core::store::read_plugin_manifest(plugin_dir)
            .map(|manifest| manifest.name)
            .unwrap_or_else(|_| {
                plugin_dir
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
        extension::discover(plugin_dir)
            .into_iter()
            .map(|found| {
                let identity = Identity {
                    marketplace: CHECKED_MARKETPLACE.to_owned(),
                    plugin: plugin.clone(),
                    id: found.id.clone(),
                };
                check_one(identity, &found.dir)
            })
            .collect()
    }
}

fn check_one(identity: Identity, dir: &Path) -> CheckedExtension {
    let id = identity.id.clone();
    let compiled = match extension::compile(identity, dir) {
        Ok(compiled) => compiled,
        Err(diagnostics) => {
            return CheckedExtension {
                id,
                errors: diagnostics.iter().map(ToString::to_string).collect(),
                warnings: Vec::new(),
                compiled: None,
                fixtures: BTreeMap::new(),
                views: BTreeMap::new(),
            };
        }
    };
    let mut errors = Vec::new();
    let mut fixtures = BTreeMap::new();
    for source in &compiled.manifest.sources {
        let Some(fixture) = source.fixture.as_deref() else {
            continue;
        };
        match read_fixture(dir, fixture) {
            Ok(data) => {
                fixtures.insert(source.id.clone(), data);
            }
            Err(why) => errors.push(why),
        }
    }
    // A view compiled from this text a moment ago; one gone since places
    // nothing rather than failing the check.
    let views = compiled
        .views
        .keys()
        .filter_map(|view| Some((view.clone(), std::fs::read_to_string(dir.join(view)).ok()?)))
        .collect();
    CheckedExtension {
        id,
        errors,
        warnings: Vec::new(),
        compiled: Some(compiled),
        fixtures,
        views,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::UzeApplication;
    use uze_core::UzeHome;

    const MANIFEST: &str = r#"protocol 1
id "board"
name "Board"
description "A board"
surface "board" slot="view" view="board.kdl" source="board"
source "board" { fixture "fixtures/board.json"; }
"#;

    fn plugin_with(root: &std::path::Path, manifest: &str, fixture: &str) -> std::path::PathBuf {
        let plugin = root.join("greet");
        let extension = plugin.join("extensions/board");
        fs::create_dir_all(extension.join("fixtures")).unwrap();
        fs::write(extension.join("extension.kdl"), manifest).unwrap();
        fs::write(
            extension.join("board.kdl"),
            "Stack {\n    Text \"{title}\"\n}\n",
        )
        .unwrap();
        fs::write(extension.join("fixtures/board.json"), fixture).unwrap();
        plugin
    }

    #[test]
    fn a_clean_extension_comes_back_compiled_with_its_fixture() {
        let root = uze_testkit::temp::scratch("extensions-check-clean");
        let plugin = plugin_with(&root, MANIFEST, r#"{"title": "Week"}"#);
        let app = UzeApplication::new(UzeHome::at(root.join("home")), Vec::new());

        let checked = app.extensions().check(&plugin);
        assert_eq!(checked.len(), 1);
        let board = &checked[0];
        assert!(board.is_clean(), "{:?}", board.errors);
        assert!(board.compiled.is_some());
        assert_eq!(
            board.fixtures["board"],
            serde_json::json!({"title": "Week"})
        );
        assert_eq!(board.place("board.kdl", 12), Some((2, 5)), "the `Text`");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn every_diagnostic_and_a_broken_fixture_are_errors() {
        let root = uze_testkit::temp::scratch("extensions-check-broken");
        let broken = MANIFEST.replace("slot=\"view\"", "slot=\"panel\"");
        let plugin = plugin_with(&root, &broken, "{}");
        let app = UzeApplication::new(UzeHome::at(root.join("home")), Vec::new());
        let checked = app.extensions().check(&plugin);
        let errors = &checked[0].errors;
        assert!(
            errors
                .iter()
                .any(|error| error.starts_with("extension.kdl:5:") && error.contains("panel")),
            "{errors:?}"
        );

        let plugin = plugin_with(&root.join("again"), MANIFEST, "{ \"title\": }");
        let checked = app.extensions().check(&plugin);
        assert!(checked[0].compiled.is_some());
        assert!(
            checked[0].errors[0].starts_with("fixtures/board.json:1:"),
            "{:?}",
            checked[0].errors
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_plugin_without_extensions_has_none_to_check() {
        let root = uze_testkit::temp::scratch("extensions-check-none");
        fs::create_dir_all(root.join("greet")).unwrap();
        let app = UzeApplication::new(UzeHome::at(root.join("home")), Vec::new());
        assert!(app.extensions().check(&root.join("greet")).is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
