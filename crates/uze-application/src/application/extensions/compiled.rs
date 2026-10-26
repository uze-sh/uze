//! A declared extension loaded, compiled, its fixtures read, and the
//! stamp its files are watched by.
//!
//! Compiling is `uze-core`'s: the only reader of an extension's KDL, behind
//! the bounds that make a third party's document safe to parse. What this
//! adds is everything around it the workspace needs off its drawing
//! thread: the extension's directory read once into memory and digested,
//! so what compiles, what its fixtures say and what consent is given over
//! are the same bytes; a cache of what compiled, keyed by that digest, so
//! opening the workspace does not parse every extension again; and a cheap
//! stamp of the files, so a linked extension is recompiled only when one of
//! them changed.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use uze_contract::{CompiledExtension, Identity};
use uze_core::capability::extension::{self, Diagnostic, Loaded};
use uze_core::digest::fnv1a64;

use super::InstalledExtension;
use crate::application::services::Extensions;

/// The most a fixture may hold. A fixture is an author's sample of what a
/// source answers, and the engine binds all of it on every change.
pub const FIXTURE_LIMIT: u64 = 1024 * 1024;

/// What a source answered: its data, or why there is none, said as the
/// sentence to show.
pub type SourceData = Result<serde_json::Value, String>;

/// How deep a linked extension's stamp looks for a save: past the load's
/// depth, since an extension too deep to load still says it was saved.
const STAMP_DEPTH: usize = 16;

/// A declared extension as one read of its directory found it.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionLoad {
    pub compiled: CompiledExtension,
    /// `sha256:<hex>` of the files read: what consent is given over.
    pub digest: String,
    /// The data each source's fixture holds, by source id. A source with
    /// no fixture has nothing here.
    pub fixtures: BTreeMap<String, SourceData>,
}

impl Extensions<'_> {
    /// Reads `installed`'s directory once, bounded, and compiles it and its
    /// fixtures from those bytes alone, from the cache when a compile of
    /// the same bytes is kept. Only a compile that succeeded is kept:
    /// diagnostics are worth reading fresh, since the author is fixing
    /// them.
    #[tracing::instrument(name = "extensions.load", skip_all, fields(extension = %installed.identity))]
    pub fn load(&self, installed: &InstalledExtension) -> Result<ExtensionLoad, Vec<Diagnostic>> {
        self.load_with_files(installed).map(|(load, _)| load)
    }

    /// [`Self::load`], keeping the bytes it read: what a script that
    /// answers is taken from, so it is the file the digest covers.
    pub(super) fn load_with_files(
        &self,
        installed: &InstalledExtension,
    ) -> Result<(ExtensionLoad, Loaded), Vec<Diagnostic>> {
        let loaded = Loaded::read(&installed.dir).map_err(|refusal| vec![refusal])?;
        let compiled = compile_cached(
            &self.0.home,
            &installed.identity,
            &loaded,
            extension::compile_loaded,
        )?;
        let fixtures = fixtures_of(&compiled, &loaded);
        let load = ExtensionLoad {
            digest: loaded.digest().to_owned(),
            compiled,
            fixtures,
        };
        Ok((load, loaded))
    }

    /// [`Self::load`]'s compile alone.
    pub fn compile(
        &self,
        installed: &InstalledExtension,
    ) -> Result<CompiledExtension, Vec<Diagnostic>> {
        self.load(installed).map(|load| load.compiled)
    }
}

fn fixtures_of(compiled: &CompiledExtension, loaded: &Loaded) -> BTreeMap<String, SourceData> {
    compiled
        .manifest
        .sources
        .iter()
        .filter_map(|source| {
            let fixture = source.fixture()?;
            Some((source.id.clone(), loaded_fixture(loaded, fixture)))
        })
        .collect()
}

fn compile_cached(
    home: &uze_core::UzeHome,
    identity: &Identity,
    loaded: &Loaded,
    compile: impl FnOnce(Identity, &Loaded) -> Result<CompiledExtension, Vec<Diagnostic>>,
) -> Result<CompiledExtension, Vec<Diagnostic>> {
    let cached_at = home.compiled_extension_path(&cache_key(identity, loaded));
    if let Ok(bytes) = std::fs::read(&cached_at)
        && let Ok(compiled) = serde_json::from_slice::<CompiledExtension>(&bytes)
        && compiled.identity == *identity
    {
        return Ok(compiled);
    }
    let compiled = compile(identity.clone(), loaded)?;
    // A cache: one that cannot be written is a compile again next time.
    if let Ok(bytes) = serde_json::to_vec(&compiled) {
        if let Some(parent) = cached_at.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = uze_core::persistence::write_atomic(&cached_at, &bytes);
    }
    Ok(compiled)
}

/// What a compile is cached by: the bytes, the identity they compile
/// under, and the build that compiles them, since a newer UZE may compile
/// the same files differently.
fn cache_key(identity: &Identity, loaded: &Loaded) -> String {
    let mut stream = Vec::new();
    for part in [
        env!("CARGO_PKG_VERSION"),
        &identity.marketplace,
        &identity.plugin,
        &identity.id,
        loaded.digest(),
    ] {
        push_field(&mut stream, part.as_bytes());
    }
    format!("{:016x}", fnv1a64(&stream))
}

/// A stamp of when each of an extension's files last changed: equal stamps
/// mean nothing was saved in between. Reads metadata only, so a linked
/// extension can be asked several times a second.
pub fn extension_stamp(dir: &Path) -> u64 {
    let mut stream = Vec::new();
    for (relative, path) in walk(dir, STAMP_DEPTH) {
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
/// sorted, leaving out directories deeper than `max_depth`. Links are not
/// followed: an extension's files are its own.
fn walk(dir: &Path, max_depth: usize) -> Vec<(String, PathBuf)> {
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
                if depth < max_depth {
                    pending.push((entry.path(), relative, depth + 1));
                }
            } else if kind.is_file() {
                files.push((relative, entry.path()));
            }
        }
    }
    files.sort();
    files
}

/// A fixture's JSON from the loaded files, no larger than
/// [`FIXTURE_LIMIT`].
pub(super) fn loaded_fixture(loaded: &Loaded, fixture: &str) -> SourceData {
    let bytes = loaded
        .bytes(fixture)
        .ok_or_else(|| format!("{fixture}: not found"))?;
    if bytes.len() as u64 > FIXTURE_LIMIT {
        return Err(format!("{fixture}: larger than 1 MiB"));
    }
    parse_fixture(bytes).map_err(|(line, column, why)| format!("{fixture}:{line}:{column}: {why}"))
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
                wants: Vec::new(),
                sources: vec![uze_contract::SourceDecl::new(
                    "board",
                    uze_contract::SourceKind::Fixture("fixtures/board.json".to_owned()),
                )],
                actions: Vec::new(),
            },
            views: BTreeMap::new(),
        }
    }

    fn loaded(installed: &InstalledExtension) -> Loaded {
        Loaded::read(&installed.dir).unwrap()
    }

    #[test]
    fn a_save_is_stamped_beside_a_directory_too_deep_to_walk() {
        let root = uze_testkit::temp::scratch("extensions-stamp-deep");
        let deep = (0..STAMP_DEPTH + 2).fold(root.join("assets"), |path, level| {
            path.join(level.to_string())
        });
        fs::create_dir_all(&deep).unwrap();
        fs::write(root.join("board.kdl"), "Text \"a\"").unwrap();
        let before = extension_stamp(&root);
        fs::write(root.join("board.kdl"), "Text \"a longer view\"").unwrap();
        assert_ne!(extension_stamp(&root), before, "the save is seen");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_compile_is_kept_until_the_files_it_came_from_change() {
        let root = uze_testkit::temp::scratch("extension-compile-cache");
        let home = uze_core::UzeHome::at(root.join("home"));
        let installed = extension_in(&root);
        let compiles = Cell::new(0);
        let compile = |_: Identity, _: &Loaded| {
            compiles.set(compiles.get() + 1);
            Ok(compiled())
        };

        let first = loaded(&installed);
        assert_eq!(
            compile_cached(&home, &identity(), &first, compile),
            Ok(compiled())
        );
        assert_eq!(
            compile_cached(&home, &identity(), &loaded(&installed), compile),
            Ok(compiled())
        );
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
        compile_cached(&home, &identity(), &loaded(&installed), compile).unwrap();
        assert_eq!(compiles.get(), 2, "a saved file compiles again");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn diagnostics_are_never_cached() {
        let root = uze_testkit::temp::scratch("extension-compile-refused");
        let home = uze_core::UzeHome::at(root.join("home"));
        let installed = extension_in(&root);
        let compiles = Cell::new(0);
        let refuse = |_: Identity, _: &Loaded| {
            compiles.set(compiles.get() + 1);
            Err(vec![Diagnostic {
                file: "extension.kdl".to_owned(),
                line: 1,
                column: 1,
                message: "unknown node".to_owned(),
                suggestion: None,
            }])
        };
        let files = loaded(&installed);
        assert!(compile_cached(&home, &identity(), &files, refuse).is_err());
        assert!(compile_cached(&home, &identity(), &files, refuse).is_err());
        assert_eq!(compiles.get(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    /// A fixture is read from the bytes the load holds: changing the file
    /// on disk after the load changes nothing the load answers.
    #[test]
    fn a_fixture_is_read_from_the_loaded_bytes() {
        let root = uze_testkit::temp::scratch("extension-fixture");
        let installed = extension_in(&root);
        fs::write(installed.dir.join("fixtures/broken.json"), "{\n  \"a\": }").unwrap();
        let large = format!("[{}0]", "0,".repeat(FIXTURE_LIMIT as usize / 2));
        fs::write(installed.dir.join("fixtures/large.json"), large).unwrap();
        let files = loaded(&installed);
        fs::write(installed.dir.join("fixtures/board.json"), "{}").unwrap();

        assert_eq!(
            loaded_fixture(&files, "fixtures/board.json"),
            Ok(serde_json::json!({"plans": 19}))
        );
        assert!(loaded_fixture(&files, "../../outside.json").is_err());
        assert!(loaded_fixture(&files, "missing.json").is_err());
        let broken = loaded_fixture(&files, "fixtures/broken.json").unwrap_err();
        assert!(broken.starts_with("fixtures/broken.json:2:"), "{broken}");
        assert_eq!(
            loaded_fixture(&files, "fixtures/large.json"),
            Err("fixtures/large.json: larger than 1 MiB".to_owned())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_load_digests_what_it_compiled() {
        let root = uze_testkit::temp::scratch("extension-load-digest");
        let installed = extension_in(&root);
        let files = loaded(&installed);
        assert!(files.digest().starts_with("sha256:"));
        assert_eq!(
            fixtures_of(&compiled(), &files).get("board"),
            Some(&Ok(serde_json::json!({"plans": 19})))
        );
        fs::write(installed.dir.join("fixtures/board.json"), "{}").unwrap();
        assert_ne!(loaded(&installed).digest(), files.digest());
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
