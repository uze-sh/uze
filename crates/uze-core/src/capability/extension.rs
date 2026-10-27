//! Extensions a plugin declares: finding them in a plugin and compiling
//! their KDL manifest and views into the contract's types.
//!
//! The only place UZE reads an extension's documents. A manifest or a view
//! is a third party's KDL, and the parser recurses without a limit, so it is
//! read here alone, behind the bounds of `extension-platform` D22, and
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
mod loaded;
pub mod locate;
mod manifest;
pub mod program;
mod test_file;
mod view;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use uze_contract::{CompiledExtension, Identity};

pub use bounded::{DEPTH_MAX, FILE_MAX, PARSE_LIMIT};
pub use loaded::{LOAD_BYTES, LOAD_DEPTH, LOAD_FILES, Loaded};
pub use test_file::{TEST_SUFFIX, TESTS_DIR};
pub use view::EACH_MAX;

use document::Report;

/// Text an editor holds for files of an extension, by path relative to its
/// directory, read in place of what is on disk.
pub type Overlay = BTreeMap<String, String>;

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
    compile_overlaid(identity, dir, &Overlay::new())
}

/// [`compile`], reading each file `overlay` holds from it instead of disk.
pub fn compile_overlaid(
    identity: Identity,
    dir: &Path,
    overlay: &Overlay,
) -> Result<CompiledExtension, Vec<Diagnostic>> {
    compile_from(identity, &OnDisk { dir, overlay })
}

/// [`compile`] from files already read into memory, reaching nothing on
/// disk: what compiles is what was digested.
pub fn compile_loaded(
    identity: Identity,
    loaded: &Loaded,
) -> Result<CompiledExtension, Vec<Diagnostic>> {
    compile_from(identity, loaded)
}

/// Where a compile reads an extension's files from.
trait Files {
    /// The text of `file`, behind the same bounds wherever it is held.
    fn text(&self, file: &str) -> Result<String, Diagnostic>;
    /// Whether `file` is one of the extension's files, or why not.
    fn holds(&self, file: &str) -> Result<(), String>;
    /// The first [`program::SHEBANG_MAX`] bytes of `file`, held.
    fn head(&self, file: &str) -> Option<Vec<u8>>;
}

/// The extension's directory, with an editor's unsaved text over it.
struct OnDisk<'a> {
    dir: &'a Path,
    overlay: &'a Overlay,
}

impl Files for OnDisk<'_> {
    fn text(&self, file: &str) -> Result<String, Diagnostic> {
        read_file(self.dir, file, self.overlay)
    }

    fn holds(&self, file: &str) -> Result<(), String> {
        if self.overlay.contains_key(file) {
            return Ok(());
        }
        let path = inside(self.dir, file)?;
        if path.is_file() {
            Ok(())
        } else {
            Err("is not a file".to_owned())
        }
    }

    fn head(&self, file: &str) -> Option<Vec<u8>> {
        if let Some(text) = self.overlay.get(file) {
            return Some(
                text.as_bytes()
                    .iter()
                    .take(program::SHEBANG_MAX)
                    .copied()
                    .collect(),
            );
        }
        let path = inside(self.dir, file).ok()?;
        let mut head = Vec::with_capacity(program::SHEBANG_MAX);
        std::io::Read::read_to_end(
            &mut std::io::Read::take(std::fs::File::open(path).ok()?, program::SHEBANG_MAX as u64),
            &mut head,
        )
        .ok()?;
        Some(head)
    }
}

impl Files for Loaded {
    fn text(&self, file: &str) -> Result<String, Diagnostic> {
        let bytes = self.bytes(file).ok_or_else(|| {
            Diagnostic::whole(file, "does not exist in the extension's directory", None)
        })?;
        bounded::decoded(bytes.to_vec(), file)
    }

    fn holds(&self, file: &str) -> Result<(), String> {
        self.bytes(file)
            .map(|_| ())
            .ok_or_else(|| "does not exist in the extension's directory".to_owned())
    }

    fn head(&self, file: &str) -> Option<Vec<u8>> {
        self.bytes(file)
            .map(|bytes| bytes.iter().take(program::SHEBANG_MAX).copied().collect())
    }
}

fn compile_from(
    identity: Identity,
    files: &impl Files,
) -> Result<CompiledExtension, Vec<Diagnostic>> {
    let source = files.text(MANIFEST_FILE).map_err(|refusal| vec![refusal])?;
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
        match files.holds(&surface.view) {
            Ok(()) => match compile_view(files, &surface.view) {
                Ok(view) => {
                    views.insert(surface.view.clone(), view);
                }
                Err(diagnostics) => view_diagnostics.extend(diagnostics),
            },
            Err(why) => report.at(*view_at, format!("the view `{}` {why}", surface.view), None),
        }
    }
    for (what, file, at) in decoded.files() {
        if let Err(why) = files.holds(file) {
            report.at(at, format!("the {what} `{file}` {why}"), None);
        } else if what == "program" {
            interpreter_is_granted(&decoded.manifest, files, file, at, &mut report);
        }
    }
    for (sprite, at) in decoded.manifest.sprites.iter().zip(&decoded.sprite_places) {
        if files.holds(&sprite.sheet).is_ok() {
            sheet_holds_its_frames(sprite, files, *at, &mut report);
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

/// A sprite's frames divide its sheet, its columns fit across it, and
/// every state names frames the sheet holds.
fn sheet_holds_its_frames(
    sprite: &uze_contract::SpriteDecl,
    files: &impl Files,
    at: usize,
    report: &mut Report<'_>,
) {
    let Some((width, height)) = files
        .head(&sprite.sheet)
        .as_deref()
        .and_then(manifest::sprite::png_size)
    else {
        report.at(
            at,
            format!("the sprite sheet `{}` is not a PNG", sprite.sheet),
            None,
        );
        return;
    };
    let (frame_width, frame_height) = sprite.frame;
    if width % frame_width != 0 || height % frame_height != 0 {
        report.at(
            at,
            format!(
                "`{}` is {width}x{height}, which {frame_width}x{frame_height} frames do not divide",
                sprite.sheet
            ),
            None,
        );
        return;
    }
    if sprite.columns > width / frame_width {
        report.at(
            at,
            format!(
                "`{}` holds {} columns of frames, not {}",
                sprite.sheet,
                width / frame_width,
                sprite.columns
            ),
            None,
        );
        return;
    }
    let held = sprite.columns * (height / frame_height);
    for state in &sprite.states {
        if let Some(beyond) = state.frames.iter().find(|frame| **frame >= held) {
            report.at(
                at,
                format!(
                    "the state `{}` plays frame {beyond}, and `{}` holds {held}",
                    state.name, sprite.sheet
                ),
                None,
            );
        }
    }
}

/// A packaged script runs under its interpreter's `exec:` grant, so the
/// manifest wants it.
fn interpreter_is_granted(
    manifest: &uze_contract::Manifest,
    files: &impl Files,
    file: &str,
    at: usize,
    report: &mut Report<'_>,
) {
    let Some(head) = files.head(file) else {
        return;
    };
    match program::packaged(&head) {
        Ok(program::Packaged::Script { interpreter, .. }) => {
            let grant = uze_contract::Grant::Exec(interpreter.clone());
            if !manifest.wants(&grant) {
                report.at(
                    at,
                    format!("the program `{file}` is a script run by `{interpreter}`, which needs `{grant}`"),
                    Some(format!("add \"{grant}\" to `wants`")),
                );
            }
        }
        Ok(program::Packaged::Native) => {}
        Err(why) => report.at(at, format!("the program `{file}`: {why}"), None),
    }
}

fn compile_view(
    files: &impl Files,
    file: &str,
) -> Result<uze_contract::CompiledView, Vec<Diagnostic>> {
    let source = files.text(file).map_err(|refusal| vec![refusal])?;
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
pub fn compile_test(
    dir: &Path,
    file: &str,
    overlay: &Overlay,
) -> Result<uze_contract::TestFile, Vec<Diagnostic>> {
    let source = read_file(dir, file, overlay).map_err(|refusal| vec![refusal])?;
    decode_test(file, &source)
}

/// `file` of the extension in `dir`: the text `overlay` holds for it, else
/// the file on disk, refused when it leads outside the directory.
fn read_file(dir: &Path, file: &str, overlay: &Overlay) -> Result<String, Diagnostic> {
    if let Some(text) = overlay.get(file) {
        return bounded::held(text, file);
    }
    let path = inside(dir, file).map_err(|why| Diagnostic::whole(file, why, None))?;
    bounded::read(&path, file)
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
