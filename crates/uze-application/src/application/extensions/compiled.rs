//! A declared extension compiled, its fixtures read, and the stamp its
//! files are watched by.
//!
//! Compiling is `uze-core`'s: the only reader of an extension's KDL, behind
//! the bounds that make a third party's document safe to parse. What this
//! adds is everything around it the workspace needs off its drawing
//! thread: a cache of what compiled, keyed by what it was compiled from, so
//! opening the workspace does not parse every extension again; the data a
//! source's fixture holds; and a cheap stamp of the files, so a linked
//! extension is recompiled only when one of them changed.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use uze_contract::{CompiledExtension, Identity};
use uze_core::capability::extension::{self, Diagnostic};
use uze_core::digest::fnv1a64;

use super::InstalledExtension;
use crate::application::services::Extensions;

/// The most a fixture may hold. A fixture is an author's sample of what a
/// source answers, and the engine binds all of it on every change.
pub const FIXTURE_LIMIT: u64 = 1024 * 1024;

/// What a source answered: its data, or why there is none, said as the
/// sentence to show.
pub type SourceData = Result<serde_json::Value, String>;

/// How deep and how wide an extension's directory is walked to digest it.
/// An extension is a manifest, a few views and their fixtures; a directory
/// past these bounds is not digested at all, and compiles every time.
const WALK_DEPTH: usize = 4;
const WALK_FILES: usize = 256;
const WALK_BYTES: u64 = 8 * 1024 * 1024;
/// How deep a linked extension's stamp looks for a save: past the digest's
/// depth, since an extension too deep to cache still reloads on save.
const STAMP_DEPTH: usize = 16;

impl Extensions<'_> {
    /// Compiles `installed`, from the cache when its files are the ones the
    /// cached copy was compiled from. Only a compile that succeeded is kept:
    /// diagnostics are worth reading fresh, since the author is fixing them.
    #[tracing::instrument(name = "extensions.compile", skip_all, fields(extension = %installed.identity))]
    pub fn compile(
        &self,
        installed: &InstalledExtension,
    ) -> Result<CompiledExtension, Vec<Diagnostic>> {
        compile_cached(&self.0.home, installed, extension::compile)
    }

    /// The data each of `compiled`'s sources declares a fixture for, by
    /// source id. A source with no fixture has nothing here.
    pub fn fixtures(
        &self,
        installed: &InstalledExtension,
        compiled: &CompiledExtension,
    ) -> BTreeMap<String, SourceData> {
        compiled
            .manifest
            .sources
            .iter()
            .filter_map(|source| {
                let fixture = source.fixture.as_deref()?;
                Some((source.id.clone(), read_fixture(&installed.dir, fixture)))
            })
            .collect()
    }
}

fn compile_cached(
    home: &uze_core::UzeHome,
    installed: &InstalledExtension,
    compile: impl FnOnce(Identity, &Path) -> Result<CompiledExtension, Vec<Diagnostic>>,
) -> Result<CompiledExtension, Vec<Diagnostic>> {
    let cached_at = files_digest(&installed.identity, &installed.dir)
        .map(|digest| home.compiled_extension_path(&digest));
    if let Some(path) = &cached_at
        && let Ok(bytes) = std::fs::read(path)
        && let Ok(compiled) = serde_json::from_slice::<CompiledExtension>(&bytes)
        && compiled.identity == installed.identity
    {
        return Ok(compiled);
    }
    let compiled = compile(installed.identity.clone(), &installed.dir)?;
    // A cache: one that cannot be written is a compile again next time.
    if let Some(path) = cached_at
        && let Ok(bytes) = serde_json::to_vec(&compiled)
    {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = uze_core::persistence::write_atomic(&path, &bytes);
    }
    Ok(compiled)
}

/// A digest of everything an extension's directory holds, and of the build
/// that compiles it: a newer UZE may compile the same files differently.
/// `None` past the walk's bounds.
fn files_digest(identity: &Identity, dir: &Path) -> Option<String> {
    let mut stream = Vec::new();
    for part in [
        env!("CARGO_PKG_VERSION"),
        &identity.marketplace,
        &identity.plugin,
        &identity.id,
    ] {
        push_field(&mut stream, part.as_bytes());
    }
    let mut total = 0u64;
    for (relative, path) in walk(dir, WALK_DEPTH)? {
        // Read no further than the bound allows: one file of gigabytes is
        // past it after its first eight mebibytes, not after all of it.
        let mut bytes = Vec::new();
        std::fs::File::open(&path)
            .ok()?
            .take(WALK_BYTES - total + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        total += bytes.len() as u64;
        if total > WALK_BYTES {
            return None;
        }
        push_field(&mut stream, relative.as_bytes());
        push_field(&mut stream, &bytes);
    }
    Some(format!("{:016x}", fnv1a64(&stream)))
}

/// A stamp of when each of an extension's files last changed: equal stamps
/// mean nothing was saved in between. Reads metadata only, so a linked
/// extension can be asked several times a second.
pub fn extension_stamp(dir: &Path) -> u64 {
    let mut stream = Vec::new();
    for (relative, path) in walk(dir, STAMP_DEPTH).unwrap_or_default() {
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        let modified = metadata
            .modified()
            .ok()
            .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_nanos());
        push_field(&mut stream, relative.as_bytes());
        push_field(&mut stream, &modified.to_le_bytes());
        push_field(&mut stream, &metadata.len().to_le_bytes());
    }
    fnv1a64(&stream)
}

/// Each field length-prefixed, so no arrangement of names and contents
/// reads as another.
fn push_field(stream: &mut Vec<u8>, field: &[u8]) {
    stream.extend_from_slice(&(field.len() as u64).to_le_bytes());
    stream.extend_from_slice(field);
}

/// The files under `dir`, by their path relative to it in `/` spelling,
/// sorted. Links are not followed: an extension's files are its own.
/// `None` past [`WALK_FILES`] files or a directory deeper than
/// `max_depth`: a digest that left files out would serve a stale compile.
fn walk(dir: &Path, max_depth: usize) -> Option<Vec<(String, PathBuf)>> {
    let mut files = Vec::new();
    let mut pending = vec![(dir.to_path_buf(), String::new(), 0usize)];
    while let Some((directory, prefix, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if kind.is_dir() {
                if depth >= max_depth {
                    return None;
                }
                pending.push((entry.path(), relative, depth + 1));
            } else if kind.is_file() {
                files.push((relative, entry.path()));
                if files.len() > WALK_FILES {
                    return None;
                }
            }
        }
    }
    files.sort();
    Some(files)
}

/// A fixture's JSON, read from inside the extension's directory and no
/// further than [`FIXTURE_LIMIT`].
pub(super) fn read_fixture(dir: &Path, fixture: &str) -> SourceData {
    let bytes = fixture_bytes(dir, fixture)?;
    parse_fixture(&bytes).map_err(|(line, column, why)| format!("{fixture}:{line}:{column}: {why}"))
}

/// A fixture's JSON, or where it stops reading and why.
pub(super) fn parse_fixture(bytes: &[u8]) -> Result<serde_json::Value, (usize, usize, String)> {
    serde_json::from_slice(bytes).map_err(|error| (error.line(), error.column(), error.to_string()))
}

/// A fixture's bytes, read from inside the extension's directory and no
/// further than [`FIXTURE_LIMIT`].
pub(super) fn fixture_bytes(dir: &Path, fixture: &str) -> Result<Vec<u8>, String> {
    let relative = Path::new(fixture);
    if !relative
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!("{fixture}: not a path inside the extension"));
    }
    let path = dir.join(relative);
    let inside = match (path.canonicalize(), dir.canonicalize()) {
        (Ok(file), Ok(root)) => file.starts_with(root),
        _ => return Err(format!("{fixture}: not found")),
    };
    if !inside {
        return Err(format!("{fixture}: not a path inside the extension"));
    }
    let file = std::fs::File::open(&path).map_err(|error| format!("{fixture}: {error}"))?;
    let mut bytes = Vec::new();
    file.take(FIXTURE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{fixture}: {error}"))?;
    if bytes.len() as u64 > FIXTURE_LIMIT {
        return Err(format!("{fixture}: larger than 1 MiB"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs;

    fn identity() -> Identity {
        Identity {
            marketplace: "market".to_owned(),
            plugin: "board".to_owned(),
            id: "board".to_owned(),
        }
    }

    fn extension_in(root: &Path) -> InstalledExtension {
        let dir = root.join("extensions/board");
        fs::create_dir_all(dir.join("fixtures")).unwrap();
        fs::write(dir.join("extension.kdl"), "protocol 1\n").unwrap();
        fs::write(dir.join("fixtures/board.json"), r#"{"plans": 19}"#).unwrap();
        InstalledExtension {
            identity: identity(),
            dir,
            linked: false,
        }
    }

    fn compiled() -> CompiledExtension {
        CompiledExtension {
            identity: identity(),
            manifest: uze_contract::Manifest {
                protocol: 1,
                id: "board".to_owned(),
                name: "Board".to_owned(),
                description: String::new(),
                surfaces: Vec::new(),
                sources: vec![uze_contract::SourceDecl {
                    id: "board".to_owned(),
                    fixture: Some("fixtures/board.json".to_owned()),
                }],
            },
            views: BTreeMap::new(),
        }
    }

    #[test]
    fn a_directory_past_the_walks_bounds_is_not_digested() {
        let root = uze_testkit::temp::scratch("extensions-digest-bounds");
        let identity = Identity {
            marketplace: "m".into(),
            plugin: "p".into(),
            id: "x".into(),
        };
        fs::create_dir_all(root.join("v/a/b/c/d")).unwrap();
        fs::write(root.join("v/a/b/c/d/board.kdl"), "Text \"a\"").unwrap();
        assert_eq!(
            files_digest(&identity, &root),
            None,
            "too deep to see every file"
        );

        let flat = root.join("flat");
        fs::create_dir_all(&flat).unwrap();
        fs::write(flat.join("board.kdl"), "Text \"a\"").unwrap();
        assert!(files_digest(&identity, &flat).is_some());
        let blob = fs::File::create(flat.join("blob.bin")).unwrap();
        blob.set_len(WALK_BYTES + 1).unwrap();
        assert_eq!(files_digest(&identity, &flat), None, "too large");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_compile_is_kept_until_the_files_it_came_from_change() {
        let root = uze_testkit::temp::scratch("extension-compile-cache");
        let home = uze_core::UzeHome::at(root.join("home"));
        let installed = extension_in(&root);
        let compiles = Cell::new(0);
        let compile = |_: Identity, _: &Path| {
            compiles.set(compiles.get() + 1);
            Ok(compiled())
        };

        assert_eq!(compile_cached(&home, &installed, compile), Ok(compiled()));
        assert_eq!(compile_cached(&home, &installed, compile), Ok(compiled()));
        assert_eq!(compiles.get(), 1, "the second open reads the cache");
        assert!(
            home.compiled_extensions_dir().is_dir(),
            "kept in the generated tier"
        );

        fs::write(
            installed.dir.join("extension.kdl"),
            "protocol 1\n// saved\n",
        )
        .unwrap();
        compile_cached(&home, &installed, compile).unwrap();
        assert_eq!(compiles.get(), 2, "a saved file compiles again");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn diagnostics_are_never_cached() {
        let root = uze_testkit::temp::scratch("extension-compile-refused");
        let home = uze_core::UzeHome::at(root.join("home"));
        let installed = extension_in(&root);
        let compiles = Cell::new(0);
        let refuse = |_: Identity, _: &Path| {
            compiles.set(compiles.get() + 1);
            Err(vec![Diagnostic {
                file: "extension.kdl".to_owned(),
                line: 1,
                column: 1,
                message: "unknown node".to_owned(),
                suggestion: None,
            }])
        };
        assert!(compile_cached(&home, &installed, refuse).is_err());
        assert!(compile_cached(&home, &installed, refuse).is_err());
        assert_eq!(compiles.get(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_fixture_is_read_as_json_from_inside_the_extension_only() {
        let root = uze_testkit::temp::scratch("extension-fixture");
        let installed = extension_in(&root);
        assert_eq!(
            read_fixture(&installed.dir, "fixtures/board.json"),
            Ok(serde_json::json!({"plans": 19}))
        );
        fs::write(root.join("outside.json"), "{}").unwrap();
        assert!(read_fixture(&installed.dir, "../../outside.json").is_err());
        assert!(read_fixture(&installed.dir, "missing.json").is_err());

        fs::write(installed.dir.join("fixtures/broken.json"), "{\n  \"a\": }").unwrap();
        let broken = read_fixture(&installed.dir, "fixtures/broken.json").unwrap_err();
        assert!(broken.starts_with("fixtures/broken.json:2:"), "{broken}");

        let large = format!("[{}0]", "0,".repeat(FIXTURE_LIMIT as usize / 2));
        fs::write(installed.dir.join("fixtures/large.json"), large).unwrap();
        assert_eq!(
            read_fixture(&installed.dir, "fixtures/large.json"),
            Err("fixtures/large.json: larger than 1 MiB".to_owned())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_stamp_moves_when_a_file_is_saved() {
        let root = uze_testkit::temp::scratch("extension-stamp");
        let installed = extension_in(&root);
        let before = extension_stamp(&installed.dir);
        assert_eq!(before, extension_stamp(&installed.dir), "nothing saved");
        fs::write(
            installed.dir.join("fixtures/board.json"),
            r#"{"plans": 200}"#,
        )
        .unwrap();
        assert_ne!(before, extension_stamp(&installed.dir));
        fs::remove_dir_all(root).unwrap();
    }
}
