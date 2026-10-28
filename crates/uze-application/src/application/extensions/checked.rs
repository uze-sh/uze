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
        text.get(..offset)?;
        Some(line_and_column(text, offset))
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
            .map(|found| {
                check_one(
                    identity(&plugin, &found.id),
                    &found.dir,
                    &Overlay::new(),
                    &self.harness_programs(),
                )
            })
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
        check_one(
            identity(&plugin, &id),
            dir,
            overlay,
            &self.harness_programs(),
        )
    }

    /// The programs a harness is started by: no extension is granted one,
    /// since running an agent's harness from a surface would run it as the
    /// operator, outside the workspace that owns its panes.
    pub fn harness_programs(&self) -> Vec<&'static str> {
        self.0
            .integrations
            .iter()
            .map(|integration| integration.shim_name())
            .collect()
    }
}

impl Extensions<'_> {
    /// A fixture of the extension in `dir`, read as the workspace reads one:
    /// from inside its directory, bounded, with where it stopped reading
    /// when it is not JSON.
    pub fn fixture(&self, dir: &Path, fixture: &str) -> Result<serde_json::Value, Diagnostic> {
        fixture_data(dir, fixture, &Overlay::new())
    }

    /// Writes `data` as the fixture `fixture` of `identity`, which the
    /// author of a linked extension asked for from its inspector: the one
    /// file the workspace writes into an extension, never into one from a
    /// marketplace that is not linked, and never outside its `fixtures/`.
    /// Answers the file written.
    #[tracing::instrument(
        name = "extensions.record_fixture",
        skip_all,
        fields(fixture),
        err(Display)
    )]
    pub fn record_fixture(
        &self,
        identity: &Identity,
        fixture: &str,
        data: &serde_json::Value,
    ) -> Result<PathBuf, String> {
        let Some(installed) = self
            .installed()
            .into_iter()
            .find(|installed| installed.identity == *identity)
        else {
            return Err(format!("no installed plugin carries {identity}"));
        };
        if !installed.linked {
            return Err("only a linked extension's fixtures are written".to_owned());
        }
        let relative = Path::new(fixture);
        let inside_fixtures = relative.starts_with(FIXTURES_DIR)
            && relative
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)));
        if !inside_fixtures
            || relative
                .extension()
                .is_none_or(|extension| extension != "json")
        {
            return Err(format!("{fixture}: not a JSON file under {FIXTURES_DIR}/"));
        }
        let payload = serde_json::to_vec_pretty(data).unwrap_or_default();
        if payload.len() as u64 > super::FIXTURE_LIMIT {
            return Err(format!("{fixture}: larger than 1 MiB"));
        }
        let fixtures = installed.dir.join(FIXTURES_DIR);
        std::fs::create_dir_all(&fixtures).map_err(|error| format!("{fixture}: {error}"))?;
        let inside = match (fixtures.canonicalize(), installed.dir.canonicalize()) {
            (Ok(fixtures), Ok(root)) => fixtures.starts_with(root),
            _ => false,
        };
        if !inside {
            return Err(format!("{fixture}: not a path inside the extension"));
        }
        let path = installed.dir.join(relative);
        if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(format!("{fixture}: not a path inside the extension"));
        }
        // Replaced whole: the extension's own watch recompiles on the
        // save, and a half-written fixture is one it would read.
        uze_core::persistence::write_atomic_unsynced(&path, &payload)
            .map_err(|error| format!("{fixture}: {error}"))?;
        Ok(path)
    }
}

/// Where an extension keeps its fixtures, and the one place the workspace
/// writes one.
const FIXTURES_DIR: &str = "fixtures";

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

fn check_one(
    identity: Identity,
    dir: &Path,
    overlay: &Overlay,
    harnesses: &[&str],
) -> CheckedExtension {
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
                let Some(fixture) = source.fixture() else {
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
            checked.errors.extend(scripts_hold_up(&compiled, &text_of));
            checked
                .warnings
                .extend(scripts_nothing_reaches(&compiled, &text_of));
            checked
                .errors
                .extend(no_harness_is_granted(&compiled, harnesses));
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

/// Each `exec:` grant naming a harness's own program, refused.
fn no_harness_is_granted(compiled: &CompiledExtension, harnesses: &[&str]) -> Vec<Diagnostic> {
    compiled
        .manifest
        .programs()
        .filter(|program| harnesses.contains(program))
        .map(|program| {
            Diagnostic::whole(
                extension::MANIFEST_FILE,
                format!("`exec:{program}` is never granted: it starts an agent's harness outside the workspace"),
                Some(format!("remove \"exec:{program}\" from `wants`")),
            )
        })
        .collect()
}

/// What a script is asked to be: what it must define follows from it
/// (`extension-components` D5).
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Role {
    /// A source's or a `script` action's: `answer(input, context)`.
    Answers,
    /// A `call`'s: the named function.
    Called(String),
    /// A resident source's: its entry points.
    Resident,
}

/// Every script `compiled` reaches, each with what it is asked to be: the
/// sources', the actions' and the calls', and the residents'.
fn reached_scripts(compiled: &CompiledExtension) -> Vec<(String, Role)> {
    let manifest = &compiled.manifest;
    let mut reached: Vec<(String, Role)> = manifest
        .sources
        .iter()
        .filter_map(|source| match &source.kind {
            uze_contract::SourceKind::Script(file) => Some((file.clone(), Role::Answers)),
            uze_contract::SourceKind::Resident(file) => Some((file.clone(), Role::Resident)),
            _ => None,
        })
        .chain(manifest.actions.iter().filter_map(|action| {
            let (file, function) = manifest.script_of(action)?;
            Some(match &action.does {
                uze_contract::Does::Call { .. } => {
                    (file.to_owned(), Role::Called(function.to_owned()))
                }
                _ => (file.to_owned(), Role::Answers),
            })
        }))
        .collect();
    reached.sort();
    reached.dedup();
    reached
}

/// Each script `compiled` reaches, compiled now: a syntax error, a function
/// its role needs and it lacks, and each host function it names that the
/// manifest's grants do not register, located where it is written. The
/// engine would refuse each at the first run; the author should hear it
/// from the check instead. Only a component's `<script>` is read, so a
/// view's words are never taken for a call (`extension-components` D8).
fn scripts_hold_up(
    compiled: &CompiledExtension,
    text_of: &dyn Fn(&str) -> Option<String>,
) -> Vec<Diagnostic> {
    let grants: uze_script::Grants = compiled.manifest.wants.iter().cloned().collect();
    let reached = reached_scripts(compiled);
    let mut diagnostics = Vec::new();
    let mut scanned = std::collections::BTreeSet::new();
    for (file, role) in &reached {
        let Some(text) = text_of(file) else {
            continue;
        };
        let at = |error: &uze_script::ScriptError| {
            let (message, location) = match error {
                uze_script::ScriptError::Compile { message, at } => (message.clone(), *at),
                other => (other.to_string(), None),
            };
            Diagnostic {
                file: file.clone(),
                line: location.map_or(1, |at| at.line),
                column: location.map_or(1, |at| at.column),
                message,
                suggestion: None,
            }
        };
        let script = match uze_script::script_of(file, text.as_bytes()) {
            Ok(bytes) => String::from_utf8(bytes).unwrap_or_default(),
            Err(error) => {
                diagnostics.push(at(&error));
                continue;
            }
        };
        let defines = match role {
            Role::Resident => uze_script::ResidentScript::compile(script.as_bytes())
                .map(|_| None)
                .map_err(|error| at(&error)),
            Role::Answers | Role::Called(_) => uze_script::Script::compile(script.as_bytes())
                .map(|compiled| {
                    let function = match role {
                        Role::Called(function) => function.as_str(),
                        _ => "answer",
                    };
                    (!compiled.defines(function, 2)).then(|| function.to_owned())
                })
                .map_err(|error| at(&error)),
        };
        match defines {
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                continue;
            }
            Ok(Some(function)) => diagnostics.push(Diagnostic {
                file: file.clone(),
                line: 1,
                column: 1,
                message: format!("its script defines no `fn {function}(input, context)`"),
                suggestion: Some(match role {
                    Role::Called(_) => "define it, or call a function the script has".to_owned(),
                    _ => {
                        "a source's or a `script` action's script answers with `answer`".to_owned()
                    }
                }),
            }),
            Ok(None) => {}
        }
        if !scanned.insert(file.clone()) {
            continue;
        }
        for (function, grant) in uze_script::ungranted_calls(&script, &grants) {
            let offset = script
                .find(&format!("{function}("))
                .or_else(|| script.find(&function))
                .unwrap_or(0);
            let (line, column) = line_and_column(&script, offset);
            // `fetch` is granted by a host of the author's choosing.
            let wanted = if grant == uze_script::ANY_HOST {
                "net:<host>".to_owned()
            } else {
                grant.name().into_owned()
            };
            diagnostics.push(Diagnostic {
                file: file.clone(),
                line,
                column,
                message: format!(
                    "`{function}` needs the `{wanted}` grant, which the manifest does not want"
                ),
                suggestion: Some(format!(
                    "add `wants \"{wanted}\"` to {}",
                    extension::MANIFEST_FILE
                )),
            });
        }
    }
    diagnostics
}

/// Each component a surface draws whose `<script>` no source, action or
/// call reaches: its author most likely believes it feeds the view.
fn scripts_nothing_reaches(
    compiled: &CompiledExtension,
    text_of: &dyn Fn(&str) -> Option<String>,
) -> Vec<Diagnostic> {
    use uze_contract::component::{BlockKind, Component, is_component};
    let reached: std::collections::BTreeSet<String> = reached_scripts(compiled)
        .into_iter()
        .map(|(file, _)| file)
        .collect();
    let mut drawn: Vec<&str> = compiled
        .manifest
        .surfaces
        .iter()
        .map(|surface| surface.view.as_str())
        .collect();
    drawn.sort_unstable();
    drawn.dedup();
    drawn
        .into_iter()
        .filter(|file| is_component(file) && !reached.contains(*file))
        .filter_map(|file| {
            let text = text_of(file)?;
            let script = Component::read(&text).ok()?.block(BlockKind::Script)?;
            let (line, column) = line_and_column(&text, script.tag);
            Some(Diagnostic {
                file: file.to_owned(),
                line,
                column,
                message: "its `<script>` is run by no source, action or call".to_owned(),
                suggestion: Some(format!(
                    "name `{file}` in a source's `script`, or `call` one of its functions"
                )),
            })
        })
        .collect()
}

fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let before = text.get(..offset).unwrap_or_default();
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit_once('\n')
        .map_or(before, |(_, last)| last)
        .chars()
        .count()
        + 1;
    (line, column)
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
surface "board" slot="view" view="board.uze" source="board"
source "board" { fixture "fixtures/board.json"; }
"#;

    fn plugin_with(root: &std::path::Path, manifest: &str, fixture: &str) -> std::path::PathBuf {
        let plugin = root.join("greet");
        let extension = plugin.join("extensions/board");
        fs::create_dir_all(extension.join("fixtures")).unwrap();
        fs::write(extension.join("extension.kdl"), manifest).unwrap();
        fs::write(
            extension.join("board.uze"),
            "<view>\nStack {\n    Text \"{title}\"\n}\n</view>\n",
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
        assert_eq!(board.place("board.uze", 19), Some((3, 5)), "the `Text`");
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
            (
                "board.uze".to_owned(),
                "<view>\nTabel {\n}\n</view>\n".to_owned(),
            ),
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
                .any(|error| error.file == "board.uze" && error.message.contains("Tabel")),
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
    fn every_reached_script_is_compiled_and_one_nothing_runs_is_warned_of() {
        let root = uze_testkit::temp::scratch("extensions-check-scripts");
        let manifest = format!(
            "{MANIFEST}\naction \"poke\" label=\"Poke\" on=\"board\" {{ call \"poke\"; }}\n"
        );
        let plugin = plugin_with(&root, &manifest, r#"{"title": "Week"}"#);
        let component = plugin.join("extensions/board/board.uze");
        let app = UzeApplication::new(UzeHome::at(root.join("home")), Vec::new());

        // The surface's component has no script: the call has nothing to run.
        let errors: Vec<String> = app.extensions().check(&plugin)[0]
            .errors
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("holds no `<script>`")),
            "{errors:?}"
        );

        // A script without the function, and a syntax error, each said.
        fs::write(
            &component,
            "<view>\nText \"{title}\"\n</view>\n<script>\nfn other(input, context) { #{ data: () } }\n</script>\n",
        )
        .unwrap();
        let errors: Vec<String> = app.extensions().check(&plugin)[0]
            .errors
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("defines no `fn poke(input, context)`")),
            "{errors:?}"
        );
        fs::write(
            &component,
            "<view>\nText \"{title}\"\n</view>\n<script>\nfn poke(input, context) { #{ data: ( }\n</script>\n",
        )
        .unwrap();
        let errors: Vec<String> = app.extensions().check(&plugin)[0]
            .errors
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(
            errors.iter().any(|error| error.starts_with("board.uze:5:")),
            "{errors:?}"
        );

        // A script nobody runs: no call, no source names it.
        fs::write(component.with_file_name("extension.kdl"), MANIFEST).unwrap();
        fs::write(
            &component,
            "<view>\nText \"{title}\"\n</view>\n<script>\nfn answer(input, context) { #{ data: () } }\n</script>\n",
        )
        .unwrap();
        let checked = &app.extensions().check(&plugin)[0];
        assert!(checked.is_clean(), "{:?}", checked.errors);
        assert!(
            checked
                .warnings
                .iter()
                .any(|warning| warning.message.contains("run by no source, action or call")),
            "{:?}",
            checked.warnings
        );
    }

    #[test]
    fn a_script_calling_a_function_its_grants_do_not_register_is_refused() {
        let root = uze_testkit::temp::scratch("extensions-check-ungranted");
        let manifest = MANIFEST.replace(
            "source \"board\" { fixture \"fixtures/board.json\"; }",
            "source \"board\" { script \"board.uze\"; }",
        );
        let plugin = plugin_with(&root, &manifest, "{}");
        let script = plugin.join("extensions/board/board.uze");
        fs::write(
            &script,
            "<view>\nStack {\n    Text \"{title}\"\n}\n</view>\n\n<script>\n// counts the plan\nfn answer(input, context) {\n    let text = read_checkout(\"PLAN.md\");\n    #{ data: #{ size: text.len() } }\n}\n</script>\n",
        )
        .unwrap();
        let app = UzeApplication::new(UzeHome::at(root.join("home")), Vec::new());

        let checked = app.extensions().check(&plugin);
        let errors: Vec<String> = checked[0].errors.iter().map(ToString::to_string).collect();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].starts_with("board.uze:10:16:"), "{errors:?}");
        assert!(
            errors[0].contains("`read_checkout` needs the `read:checkout` grant"),
            "{errors:?}"
        );
        assert_eq!(
            checked[0].errors[0].suggestion.as_deref(),
            Some("add `wants \"read:checkout\"` to extension.kdl")
        );

        let granted = manifest.replace(
            "source \"board\"",
            "wants \"read:checkout\"\nsource \"board\"",
        );
        fs::write(plugin.join("extensions/board/extension.kdl"), granted).unwrap();
        let checked = app.extensions().check(&plugin);
        assert!(checked[0].is_clean(), "{:?}", checked[0].errors);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_harness_is_never_a_program_an_extension_runs() {
        let root = uze_testkit::temp::scratch("extensions-check-harness");
        let manifest = MANIFEST.replace(
            "source \"board\" { fixture \"fixtures/board.json\"; }",
            "wants \"exec:claude\"\nsource \"board\" { run \"claude\" \"-p\"; }",
        );
        let plugin = plugin_with(&root, &manifest, "{}");
        let checked = super::check_one(
            super::identity("greet", "board"),
            &plugin.join("extensions/board"),
            &super::Overlay::new(),
            &["claude", "codex"],
        );
        let errors: Vec<String> = checked.errors.iter().map(ToString::to_string).collect();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].contains("`exec:claude` is never granted"),
            "{errors:?}"
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

    /// The author's recording lands in the linked extension's own
    /// `fixtures/`, and nowhere else.
    #[test]
    fn a_recording_is_written_only_under_a_linked_extensions_fixtures() {
        let world = crate::application::extensions::runner::tests::World::new("record-fixture");
        let extensions = world.app.extensions();
        let events = serde_json::json!([{ "name": "checkout.changed", "synthetic": true }]);
        let written = extensions
            .record_fixture(&world.band(), "fixtures/session.json", &events)
            .unwrap();
        assert_eq!(written, world.band.join("fixtures/session.json"));
        assert_eq!(
            extensions
                .fixture(&world.band, "fixtures/session.json")
                .unwrap(),
            events
        );
        for refused in ["state.uze", "fixtures/../escape.json", "fixtures/notes.txt"] {
            assert!(
                extensions
                    .record_fixture(&world.band(), refused, &events)
                    .is_err(),
                "{refused}"
            );
        }
        assert!(!world.band.join("escape.json").exists());
    }
}
