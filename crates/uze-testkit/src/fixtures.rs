//! Canonical fixture resolution.
//!
//! All persistent test inputs live under `tests/_fixtures/` in four kinds
//! (see `tests/_fixtures/README.md`):
//!
//! - `canonical/` — valid UZE-authored package/project inputs, small and
//!   stable; the same directory the product itself would accept.
//! - `foreign/` — vendor-native formats (`.claude-plugin/`, `.codex-plugin/`,
//!   …) that integrations must translate, never treat as canonical.
//! - `scenarios/` — deliberate broken/edge states (malformed lock,
//!   malformed marketplace, …).
//! - `golden/` — the single evolving "North Star" environment used only by
//!   the acceptance suite's [`crate::scenario`]-free golden health test.
//!
//! Tests should never build paths with `env!("CARGO_MANIFEST_DIR")`
//! directly: that resolves to whichever crate compiles the test, and the
//! same fixture must be reachable from every crate.

use std::path::{Path, PathBuf};

use crate::workspace_root;

/// Copies the tree at `source` into `destination`, creating it — how a
/// test stages a fixture somewhere it is free to change.
pub fn copy_tree(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination)
        .unwrap_or_else(|error| panic!("create {}: {error}", destination.display()));
    let entries = std::fs::read_dir(source)
        .unwrap_or_else(|error| panic!("read {}: {error}", source.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("read {}: {error}", source.display()));
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to);
        } else {
            std::fs::copy(&from, &to)
                .unwrap_or_else(|error| panic!("copy {from:?} -> {to:?}: {error}"));
        }
    }
}

/// `tests/_fixtures/`.
///
/// Resolved through `$UZE_TESTKIT_FIXTURES_ROOT` when set (the Lab image
/// copies the canonical tree to a runtime path; compile-time paths do not
/// exist in the container), otherwise by walking up from this crate's
/// manifest directory.
pub fn root() -> PathBuf {
    if let Some(override_dir) = std::env::var_os("UZE_TESTKIT_FIXTURES_ROOT") {
        let root = PathBuf::from(override_dir);
        assert!(
            root.is_dir(),
            "UZE_TESTKIT_FIXTURES_ROOT is set but is not a directory: {}",
            root.display()
        );
        return root;
    }
    workspace_root().join("tests/_fixtures")
}

/// `tests/_fixtures/canonical/<name>`.
pub fn canonical(name: impl AsRef<Path>) -> PathBuf {
    root().join("canonical").join(name)
}

/// `tests/_fixtures/foreign/<vendor>/<name>`.
pub fn foreign(vendor: impl AsRef<Path>, name: impl AsRef<Path>) -> PathBuf {
    root().join("foreign").join(vendor).join(name)
}

/// `tests/_fixtures/scenarios/<name>`.
pub fn scenario(name: impl AsRef<Path>) -> PathBuf {
    root().join("scenarios").join(name)
}

/// `tests/_fixtures/golden/`.
pub fn golden() -> PathBuf {
    root().join("golden")
}

/// The repository's own official plugin (`plugins/uze`) — a canonical
/// package UZE ships and dogfoods, never copied into `tests/_fixtures`.
pub fn official_plugin() -> PathBuf {
    workspace_root().join("plugins/uze")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_root_honors_the_runtime_override() {
        let override_dir = std::env::temp_dir().join(format!(
            "uze-testkit-fixtures-override-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&override_dir).unwrap();
        let mut scope = crate::env::scope();
        scope.set("UZE_TESTKIT_FIXTURES_ROOT", &override_dir);
        assert_eq!(root(), override_dir);
        assert_eq!(canonical("x"), override_dir.join("canonical/x"));
        drop(scope);
        // Without the override the workspace walk is authoritative again.
        assert!(root().ends_with("tests/_fixtures"));
        let _ = std::fs::remove_dir_all(&override_dir);
    }
}

/// One case of the shared `.agents/rules.json` table
/// (`tests/_fixtures/rules/manifest/<name>/`): a checkout's rule files and
/// manifest, and the rule identities every reader of the manifest must
/// answer with, in order. The Rust rule model and the generated engine
/// both read it, so neither can drift from the other alone.
pub struct RulesManifestCase {
    pub name: String,
    /// Relative to the checkout root, sorted.
    pub expected: Vec<String>,
    source: PathBuf,
}

/// The file beside each case's tree that lists what it must answer.
const RULES_MANIFEST_EXPECTED: &str = "expected";

impl RulesManifestCase {
    /// Lays the case out under `root`, the checkout root as its readers
    /// will see it: `{root}` in the manifest becomes that path, escaped as
    /// a JSON string's content, so a case can hold an absolute entry.
    pub fn stage(&self, root: &Path) {
        copy_tree(&self.source, root);
        std::fs::remove_file(root.join(RULES_MANIFEST_EXPECTED))
            .expect("the case lists its answer");
        let manifest = root.join(".agents/rules.json");
        let text = std::fs::read_to_string(&manifest).expect("every case has a manifest");
        let spelled =
            serde_json::to_string(&root.display().to_string()).expect("a path serializes");
        let inside_quotes = &spelled[1..spelled.len() - 1];
        std::fs::write(&manifest, text.replace("{root}", inside_quotes))
            .expect("the manifest is written");
    }
}

/// Every case of the shared `.agents/rules.json` table, by name.
pub fn rules_manifest_cases() -> Vec<RulesManifestCase> {
    let directory = root().join("rules/manifest");
    let mut cases: Vec<RulesManifestCase> = std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .flatten()
        .map(|entry| {
            let source = entry.path();
            let expected = std::fs::read_to_string(source.join(RULES_MANIFEST_EXPECTED))
                .unwrap_or_else(|error| panic!("{}: {error}", source.display()))
                .lines()
                .map(str::to_owned)
                .collect();
            RulesManifestCase {
                name: entry.file_name().to_string_lossy().into_owned(),
                expected,
                source,
            }
        })
        .collect();
    cases.sort_by(|a, b| a.name.cmp(&b.name));
    cases
}
