//! Extensions a plugin declares: finding them in a plugin and compiling
//! their KDL manifest and views into the contract's types.
//!
//! The only place UZE reads an extension's documents. A manifest or a view
//! is a third party's KDL, and the parser recurses without a limit, so it is
//! read here alone, behind the bounds of `add-extension-platform` D22, and
//! never by the workspace client, which draws what this compiled.
//!
//! - [`bounded`]: the size cap, the nesting scan and the timed parse.
//! - [`document`]: places in a file, and the checks every node gets.
//! - [`manifest`]: `extension.kdl` into a [`uze_contract::Manifest`].
//! - [`view`]: a view file into a [`uze_contract::CompiledView`].
//! - [`test_file`]: a `tests/*.test.kdl` into a [`uze_contract::TestFile`].
//! - [`locate`]: what is at a place in a file, for an editor.

mod bounded;
mod document;
pub mod locate;
mod manifest;
mod test_file;
mod view;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use uze_contract::{CompiledExtension, Identity};

pub use bounded::{DEPTH_MAX, FILE_MAX, PARSE_LIMIT};
pub use test_file::{TEST_SUFFIX, TESTS_DIR};
pub use view::EACH_MAX;

use document::Report;

/// Where an extension's manifest sits inside its plugin.
pub const EXTENSIONS_DIR: &str = "extensions";
pub const MANIFEST_FILE: &str = "extension.kdl";

/// Something wrong with an extension's files, said where it is.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    /// Relative to the extension's directory.
    pub file: String,
    /// One-based; `0` when the problem is not at a place in a file.
    pub line: usize,
    pub column: usize,
    pub message: String,
    /// What to write instead, when there is one answer.
    pub suggestion: Option<String>,
}

impl Diagnostic {
    /// A problem with `file` as a whole, at no place in it.
    pub fn whole(file: &str, message: impl Into<String>, suggestion: Option<String>) -> Self {
        Self {
            file: file.to_owned(),
            line: 0,
            column: 0,
            message: message.into(),
            suggestion,
        }
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line > 0 {
            write!(
                f,
                "{}:{}:{}: {}",
                self.file, self.line, self.column, self.message
            )?;
        } else {
            write!(f, "{}: {}", self.file, self.message)?;
        }
        if let Some(suggestion) = &self.suggestion {
            write!(f, " ({suggestion})")?;
        }
        Ok(())
    }
}

/// An extension found in a plugin: the directory name it is declared under
/// and the directory itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoundExtension {
    pub id: String,
    pub dir: PathBuf,
}

/// The extensions `plugin_dir` carries: every `extensions/<id>/` holding an
/// `extension.kdl`, in id order. Reads directory entries only.
pub fn discover(plugin_dir: &Path) -> Vec<FoundExtension> {
    let Ok(entries) = std::fs::read_dir(plugin_dir.join(EXTENSIONS_DIR)) else {
        return Vec::new();
    };
    let mut found: Vec<FoundExtension> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join(MANIFEST_FILE).is_file())
        .map(|entry| FoundExtension {
            id: entry.file_name().to_string_lossy().into_owned(),
            dir: entry.path(),
        })
        .collect();
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

/// Reads, checks and compiles the extension in `dir` under `identity`:
/// its manifest and every view its surfaces name. Every problem found is
/// returned, not only the first.
pub fn compile(identity: Identity, dir: &Path) -> Result<CompiledExtension, Vec<Diagnostic>> {
    let source =
        bounded::read(&dir.join(MANIFEST_FILE), MANIFEST_FILE).map_err(|refusal| vec![refusal])?;
    let mut report = Report::new(MANIFEST_FILE, &source);
    let decoded = bounded::parse(&mut report)
        .and_then(|document| manifest::decode(&document, &identity.id, &mut report));
    let Some(decoded) = decoded else {
        return Err(report.into_diagnostics());
    };
    let mut views = BTreeMap::new();
    let mut view_diagnostics = Vec::new();
    let mut read = BTreeSet::new();
    for (surface, (view_at, _)) in decoded
        .manifest
        .surfaces
        .iter()
        .zip(&decoded.surface_places)
    {
        if !read.insert(surface.view.as_str()) {
            continue;
        }
        match inside(dir, &surface.view) {
            Ok(path) => match compile_view(&path, &surface.view) {
                Ok(view) => {
                    views.insert(surface.view.clone(), view);
                }
                Err(diagnostics) => view_diagnostics.extend(diagnostics),
            },
            Err(why) => report.at(*view_at, format!("the view `{}` {why}", surface.view), None),
        }
    }
    for (source, fixture_at) in decoded.manifest.sources.iter().zip(&decoded.fixture_places) {
        let Some(fixture) = &source.fixture else {
            continue;
        };
        if let Err(why) = inside(dir, fixture).and_then(|path| {
            if path.is_file() {
                Ok(())
            } else {
                Err("is not a file".to_owned())
            }
        }) {
            report.at(*fixture_at, format!("the fixture `{fixture}` {why}"), None);
        }
    }
    let mut diagnostics = report.into_diagnostics();
    diagnostics.extend(view_diagnostics);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    Ok(CompiledExtension {
        identity,
        manifest: decoded.manifest,
        views,
    })
}

fn compile_view(path: &Path, file: &str) -> Result<uze_contract::CompiledView, Vec<Diagnostic>> {
    let source = bounded::read(path, file).map_err(|refusal| vec![refusal])?;
    let mut report = Report::new(file, &source);
    let compiled =
        bounded::parse(&mut report).map(|document| view::compile(&document, file, &mut report));
    match compiled {
        Some(view) if report.diagnostics().is_empty() => Ok(view),
        _ => Err(report.into_diagnostics()),
    }
}

/// The test files of the extension in `dir`, by name relative to it
/// (`tests/board.test.kdl`), in name order.
pub fn test_files(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir.join(TESTS_DIR)) else {
        return Vec::new();
    };
    let mut files: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(TEST_SUFFIX))
        .map(|name| format!("{TESTS_DIR}/{name}"))
        .collect();
    files.sort();
    files
}

/// Reads and decodes one test file of the extension in `dir`, behind the
/// same bounds as a view.
pub fn compile_test(dir: &Path, file: &str) -> Result<uze_contract::TestFile, Vec<Diagnostic>> {
    let path = inside(dir, file).map_err(|why| {
        vec![Diagnostic::whole(
            file,
            format!("the test file {why}"),
            None,
        )]
    })?;
    let source = bounded::read(&path, file).map_err(|refusal| vec![refusal])?;
    decode_test(file, &source)
}

/// Decodes a test file from `source`, as [`compile_test`] does from disk.
pub fn decode_test(file: &str, source: &str) -> Result<uze_contract::TestFile, Vec<Diagnostic>> {
    let mut report = Report::new(file, source);
    let decoded =
        bounded::parse(&mut report).map(|document| test_file::decode(&document, &mut report));
    match decoded {
        Some(tests) if report.diagnostics().is_empty() => Ok(tests),
        _ => Err(report.into_diagnostics()),
    }
}

/// `relative` under `dir`, refused when it does not exist or resolves, by
/// a link, to somewhere outside it.
fn inside(dir: &Path, relative: &str) -> Result<PathBuf, String> {
    let resolved = dir
        .join(relative)
        .canonicalize()
        .map_err(|_| "does not exist in the extension's directory".to_owned())?;
    let root = dir
        .canonicalize()
        .map_err(|error| format!("cannot be found: {error}"))?;
    if resolved.starts_with(&root) {
        Ok(resolved)
    } else {
        Err("leads outside the extension's directory".to_owned())
    }
}

#[cfg(test)]
mod tests;
