//! A plugin's declared extensions checked before any install: compiled by
//! the rules the workspace hosts them with, their fixtures read the way the
//! workspace reads them, and their test files decoded.
//!
//! Binding a view to its fixture and running a test are the engine's and
//! the renderer's, which this crate does not reach; what compiled is handed
//! back with its fixtures, its tests and the text of its views, so the
//! caller binds and runs them and places what it finds. Every file may be
//! read from an editor's unsaved text instead of disk (an [`Overlay`]), so
//! the language server and the check say the same thing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use uze_contract::{CompiledExtension, Identity, TestFile};
use uze_core::capability::extension::{self, Diagnostic};

pub use uze_core::capability::extension::Overlay;

use super::compiled::{fixture_bytes, parse_fixture};
use crate::application::services::Extensions;

/// The marketplace an extension is compiled under when its plugin is
/// checked from a directory: the one a plugin installed from a path gets.
const CHECKED_MARKETPLACE: &str = "local";

/// One extension a plugin carries, as the check found it.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CheckedExtension {
    pub id: String,
    /// Where its files are.
    #[serde(skip)]
    pub dir: PathBuf,
    /// Each problem that keeps it from being hosted, located in its files.
    /// Empty is clean.
    #[serde(serialize_with = "as_lines")]
    pub errors: Vec<Diagnostic>,
    /// What still hosts but draws less than the author wrote; the caller
    /// adds what binding and its tests found.
    #[serde(serialize_with = "as_lines")]
    pub warnings: Vec<Diagnostic>,
    /// What compiled, when nothing kept it from compiling.
    #[serde(skip)]
    pub compiled: Option<CompiledExtension>,
    /// Each source's fixture that read as JSON, by source id.
    #[serde(skip)]
    pub fixtures: BTreeMap<String, serde_json::Value>,
    /// Each test file that decoded, by its path in the extension.
    #[serde(skip)]
    pub tests: BTreeMap<String, TestFile>,
    /// Each compiled view's and test file's text, by its file, to place a
    /// span in.
    #[serde(skip)]
    texts: BTreeMap<String, String>,
}

fn as_lines<S: serde::Serializer>(
    diagnostics: &[Diagnostic],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(diagnostics.iter().map(ToString::to_string))
}

impl CheckedExtension {
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }

    /// The one-based line and column `offset` falls at in `file`.
    pub fn place(&self, file: &str, offset: usize) -> Option<(usize, usize)> {
        let text = self.texts.get(file)?;
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

    /// A warning at `offset` of `file`, placed when the text is known.
    pub fn warn_at(&mut self, file: &str, offset: usize, message: String) {
        let (line, column) = self.place(file, offset).unwrap_or((0, 0));
        self.warnings.push(Diagnostic {
            file: file.to_owned(),
            line,
            column,
            message,
            suggestion: None,
        });
    }
}

impl Extensions<'_> {
    /// Every extension `plugin_dir` carries, compiled, with its fixtures
    /// read and its tests decoded. Compiles every time: an author checking
    /// is fixing what the last check said.
    #[tracing::instrument(name = "extensions.check", skip_all, fields(plugin = %plugin_dir.display()))]
    pub fn check(&self, plugin_dir: &Path) -> Vec<CheckedExtension> {
        let plugin = plugin_name(plugin_dir);
        extension::discover(plugin_dir)
            .into_iter()
            .map(|found| check_one(identity(&plugin, &found.id), &found.dir, &Overlay::new()))
            .collect()
    }

    /// The extension in `dir`, checked as [`Self::check`] does, with each
    /// file `overlay` holds read from it instead of disk. Its plugin is the
    /// directory two levels up, as `extensions/<id>/` sits in one.
    pub fn check_extension(&self, dir: &Path, overlay: &Overlay) -> CheckedExtension {
        let id = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let plugin = dir
            .parent()
            .and_then(Path::parent)
            .map(plugin_name)
            .unwrap_or_default();
        check_one(identity(&plugin, &id), dir, overlay)
    }
}

fn plugin_name(plugin_dir: &Path) -> String {
    uze_core::store::read_plugin_manifest(plugin_dir)
        .map(|manifest| manifest.name)
        .unwrap_or_else(|_| {
            plugin_dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
}

fn identity(plugin: &str, id: &str) -> Identity {
    Identity {
        marketplace: CHECKED_MARKETPLACE.to_owned(),
        plugin: plugin.to_owned(),
        id: id.to_owned(),
    }
}

fn check_one(identity: Identity, dir: &Path, overlay: &Overlay) -> CheckedExtension {
    let mut checked = CheckedExtension {
        id: identity.id.clone(),
        dir: dir.to_path_buf(),
        errors: Vec::new(),
        warnings: Vec::new(),
        compiled: None,
        fixtures: BTreeMap::new(),
        tests: BTreeMap::new(),
        texts: BTreeMap::new(),
    };
    let text_of = |file: &str| {
        overlay
            .get(file)
            .cloned()
            .or_else(|| std::fs::read_to_string(dir.join(file)).ok())
    };
    match extension::compile_overlaid(identity, dir, overlay) {
        Ok(compiled) => {
            for source in &compiled.manifest.sources {
                let Some(fixture) = source.fixture.as_deref() else {
                    continue;
                };
                match fixture_data(dir, fixture, overlay) {
                    Ok(data) => {
                        checked.fixtures.insert(source.id.clone(), data);
                    }
                    Err(why) => checked.errors.push(why),
                }
            }
            // A view compiled from this text a moment ago; one gone since
            // places nothing rather than failing the check.
            for view in compiled.views.keys() {
                if let Some(text) = text_of(view) {
                    checked.texts.insert(view.clone(), text);
                }
            }
            checked.compiled = Some(compiled);
        }
        Err(diagnostics) => checked.errors.extend(diagnostics),
    }
    let mut files = extension::test_files(dir);
    files.extend(
        overlay
            .keys()
            .filter(|file| {
                file.starts_with(&format!("{}/", extension::TESTS_DIR))
                    && file.ends_with(extension::TEST_SUFFIX)
            })
            .cloned(),
    );
    files.sort();
    files.dedup();
    for file in files {
        match extension::compile_test(dir, &file, overlay) {
            Ok(tests) => {
                if let Some(text) = text_of(&file) {
                    checked.texts.insert(file.clone(), text);
                }
                checked.tests.insert(file, tests);
            }
            Err(diagnostics) => checked.errors.extend(diagnostics),
        }
    }
    checked
}

/// A source's fixture, from the editor's text when it holds one, with where
/// it stopped reading when it is not JSON.
fn fixture_data(
    dir: &Path,
    fixture: &str,
    overlay: &Overlay,
) -> Result<serde_json::Value, Diagnostic> {
    let bytes = match overlay.get(fixture) {
        Some(text) => text.clone().into_bytes(),
        None => fixture_bytes(dir, fixture).map_err(|why| {
            Diagnostic::whole(
                fixture,
                why.trim_start_matches(&format!("{fixture}: ")),
                None,
            )
        })?,
    };
    parse_fixture(&bytes).map_err(|(line, column, message)| Diagnostic {
        file: fixture.to_owned(),
        line,
        column,
        message,
        suggestion: None,
    })
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
                .any(|error| error.to_string().starts_with("extension.kdl:5:")
                    && error.message.contains("panel")),
            "{errors:?}"
        );

        let plugin = plugin_with(&root.join("again"), MANIFEST, "{ \"title\": }");
        let checked = app.extensions().check(&plugin);
        assert!(checked[0].compiled.is_some());
        assert!(
            checked[0].errors[0]
                .to_string()
                .starts_with("fixtures/board.json:1:"),
            "{:?}",
            checked[0].errors
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_editors_text_is_read_in_place_of_the_file_and_tests_are_decoded() {
        let root = uze_testkit::temp::scratch("extensions-check-overlay");
        let plugin = plugin_with(&root, MANIFEST, r#"{"title": "Week"}"#);
        let app = UzeApplication::new(UzeHome::at(root.join("home")), Vec::new());
        let dir = plugin.join("extensions/board");
        let overlay = super::Overlay::from([
            ("board.kdl".to_owned(), "Tabel {\n}\n".to_owned()),
            (
                "tests/board.test.kdl".to_owned(),
                "test \"a\" {\n    surface \"board\"\n    expect-text \"Week\"\n}\n".to_owned(),
            ),
        ]);
        let checked = app.extensions().check_extension(&dir, &overlay);
        assert!(
            checked
                .errors
                .iter()
                .any(|error| error.file == "board.kdl" && error.message.contains("Tabel")),
            "{:?}",
            checked.errors
        );
        assert_eq!(checked.tests["tests/board.test.kdl"].tests[0].name, "a");
        assert_eq!(
            checked.place("tests/board.test.kdl", 15),
            Some((2, 5)),
            "`surface`"
        );
        let saved = app
            .extensions()
            .check_extension(&dir, &super::Overlay::new());
        assert!(saved.is_clean(), "{:?}", saved.errors);
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
