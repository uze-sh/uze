//! Official default marketplace — a snapshot of this repository's own
//! `marketplace.json` + `plugins/**`, compiled into the binary so a fresh
//! install has something to seed without a network fetch or a running
//! registry.
//!
//! The embedded snapshot represents **a marketplace**, not a plugin: this
//! module extracts the whole snapshot and resolves a plugin name against
//! its `marketplace.json` exactly the way a Git or local marketplace root
//! would (`uze_core::acquisition::marketplace`), so adding a plugin to the
//! marketplace never touches this file. `plugins/uze` is not privileged —
//! it is simply the one entry [`DEFAULT_PLUGIN_IDS`] names as installed by
//! default, which is product policy, not a marketplace fact.
//!
//! Every default plugin goes through the exact same lifecycle a normal
//! `uze add` uses (`Plugins::install_materialized`) — Store, Engine,
//! Router and every `IntegrationPort` never learn a plugin's bytes came
//! from the binary rather than disk or Git.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use uze_core::{
    MaterializedPackage, PackageSource, Provenance, ResolvedSource, Result, UzeError,
    acquisition::{self, marketplace},
};

include!(concat!(env!("OUT_DIR"), "/embedded_marketplace.rs"));

/// Ids of the plugins installed by default on a fresh `UZE_HOME`. This is
/// product policy over what the official marketplace *offers*, not the
/// marketplace itself — it names plugins, nothing else. A future
/// marketplace entry the policy doesn't list stays available but
/// uninstalled.
pub const DEFAULT_PLUGIN_IDS: &[&str] = &["uze"];

/// Materializes `plugin_name` from the embedded official marketplace
/// snapshot: extracts the whole snapshot into a fresh scratch directory and
/// resolves the plugin against the snapshot's `marketplace.json` the same
/// way any marketplace root would. `Err(UnknownPackage)` if the snapshot's manifest
/// does not list `plugin_name`.
pub fn materialize(plugin_name: &str) -> Result<MaterializedPackage> {
    let manifest = embedded_manifest()?;
    let provenance = || Provenance {
        requested: PackageSource::Embedded {
            id: plugin_name.to_owned(),
        },
        resolved: ResolvedSource::Embedded {
            id: plugin_name.to_owned(),
        },
    };
    let root = extract_embedded_snapshot()?;
    let mut materialized = MaterializedPackage::owned(root.clone(), provenance());
    let plugin_root = marketplace::resolve_plugin_source(&manifest, plugin_name, &root)?;
    materialized.retarget(plugin_root, provenance());
    Ok(materialized)
}

/// Whether the embedded marketplace currently carries different content for
/// `plugin_name` than what's installed at `stored_root`. Pure read: the
/// fresh comparison copy lives in a scratch directory cleaned up before
/// this returns. Generic over plugin content — compares the resolved
/// directory tree file-for-file, not a fixed list of known filenames — so
/// it needs no per-plugin knowledge either.
pub fn has_update(plugin_name: &str, stored_root: &Path) -> Result<bool> {
    let current = embedded_plugin_files(plugin_name)?;
    let stored = collect_files(stored_root)?;
    Ok(current.len() != stored.len()
        || current
            .iter()
            .any(|(relative, bytes)| stored.get(relative).map(Vec::as_slice) != Some(*bytes)))
}

/// The snapshot's files under `plugin_name`'s manifest entry, keyed by
/// their path relative to the plugin root — read straight from the
/// binary. Extracting the snapshot to disk to answer this was several
/// directory writes per screen, for a comparison that needs no directory.
fn embedded_plugin_files(plugin_name: &str) -> Result<BTreeMap<PathBuf, &'static [u8]>> {
    let manifest = embedded_manifest()?;
    let entry = manifest
        .plugins
        .iter()
        .find(|entry| entry.name == plugin_name)
        .ok_or_else(|| UzeError::UnknownPackage(plugin_name.to_owned()))?;
    let prefix = contained_relative_path(&entry.source)?;
    Ok(EMBEDDED_MARKETPLACE_FILES
        .iter()
        .filter_map(|(relative, bytes)| {
            Path::new(relative)
                .strip_prefix(&prefix)
                .ok()
                .map(|within| (within.to_path_buf(), *bytes))
        })
        .collect())
}

/// A manifest `source` as a path inside the snapshot: `./plugins/uze` is
/// `plugins/uze`, and anything that would climb out of the snapshot is
/// refused — the same containment `resolve_plugin_source` holds a
/// marketplace directory to, for a root that is never on disk.
fn contained_relative_path(source: &str) -> Result<PathBuf> {
    let mut contained = PathBuf::new();
    for component in Path::new(source).components() {
        match component {
            std::path::Component::Normal(part) => contained.push(part),
            std::path::Component::CurDir => {}
            _ => {
                return Err(UzeError::UnsafePathReference {
                    path: PathBuf::from(format!(
                        "embedded:{}",
                        uze_core::manifest::BUILT_IN_MARKETPLACE
                    )),
                    reference: source.to_owned(),
                });
            }
        }
    }
    Ok(contained)
}

/// The snapshot's own `marketplace.json`, parsed from the binary.
fn embedded_manifest() -> Result<marketplace::MarketplaceManifest> {
    let bytes = EMBEDDED_MARKETPLACE_FILES
        .iter()
        .find(|(relative, _)| {
            Path::new(relative) == Path::new(uze_core::anchor::MARKETPLACE_MANIFEST_NAME)
        })
        .map(|(_, bytes)| *bytes)
        .ok_or_else(|| {
            UzeError::MissingManifest(PathBuf::from(uze_core::anchor::MARKETPLACE_MANIFEST_NAME))
        })?;
    marketplace::parse_manifest(bytes)
}

/// The official embedded marketplace as it describes itself — a pure,
/// read-only parse of its `marketplace.json`. This is the one place
/// `uze-application` reads `marketplace.json` structure directly; the
/// Application facade turns this into product-facing read models, and
/// nothing below `uze-core::acquisition` ever sees it.
pub struct OfficialCatalog {
    /// Where a reader goes to see this marketplace for themselves
    /// (`owner.url`). `None` when the manifest names no owner: the
    /// embedded snapshot has no source URL of its own to fall back on.
    pub homepage: Option<String>,
    pub plugins: Vec<marketplace::MarketplacePluginEntry>,
    /// What each plugin's own `plugin.json` in the snapshot says about it,
    /// by plugin name.
    pub listings: BTreeMap<String, marketplace::PluginListing>,
}

pub fn entries() -> Result<OfficialCatalog> {
    let manifest = embedded_manifest()?;
    let listings = manifest
        .plugins
        .iter()
        .map(|entry| {
            let bytes = contained_relative_path(&entry.source)
                .ok()
                .and_then(|directory| embedded_file(&directory.join("plugin.json")));
            (entry.name.clone(), marketplace::PluginListing::read(bytes))
        })
        .collect();
    Ok(OfficialCatalog {
        homepage: manifest.owner.and_then(|owner| owner.url),
        plugins: manifest.plugins,
        listings,
    })
}

/// One file of the snapshot, by its path relative to the marketplace root.
fn embedded_file(relative: &Path) -> Option<&'static [u8]> {
    EMBEDDED_MARKETPLACE_FILES
        .iter()
        .find(|(path, _)| Path::new(path) == relative)
        .map(|(_, bytes)| *bytes)
}

/// Extracts the snapshot into a directory only this user can enter, made
/// by this call: what is extracted here is installed as a plugin next, so
/// a directory another account created first under the same name, or a
/// file it placed there, would be bytes UZE installs as its own.
fn extract_embedded_snapshot() -> Result<PathBuf> {
    let scratch = acquisition::scratch_directory()?;
    for (relative, bytes) in EMBEDDED_MARKETPLACE_FILES {
        let destination = scratch.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(UzeError::write(parent))?;
        }
        let mut file =
            uze_platform::fs::private_file(fs::OpenOptions::new().write(true).create_new(true))
                .open(&destination)
                .map_err(UzeError::write(&destination))?;
        std::io::Write::write_all(&mut file, bytes).map_err(UzeError::write(destination))?;
    }
    Ok(scratch)
}

fn collect_files(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    let mut out = BTreeMap::new();
    collect_files_into(root, root, &mut out)?;
    Ok(out)
}

fn collect_files_into(
    root: &Path,
    current: &Path,
    out: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<()> {
    let entries = fs::read_dir(current).map_err(UzeError::read(current))?;
    for entry in entries {
        let entry = entry.map_err(UzeError::read(current))?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_into(root, &path, out)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .expect("walked path is under root")
                .to_path_buf();
            let bytes = fs::read(&path).map_err(UzeError::read(&path))?;
            out.insert(relative, bytes);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What is extracted is installed as a plugin next, so it lands in a
    /// directory this call made and only this user can enter, in files
    /// only this user can read.
    // Unix file modes, which Windows does not keep.
    #[cfg(unix)]
    #[test]
    fn the_snapshot_is_extracted_where_only_this_user_reaches() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let root = extract_embedded_snapshot().unwrap();
        assert_eq!(mode(&root), 0o700);
        let (relative, _) = EMBEDDED_MARKETPLACE_FILES[0];
        assert_eq!(mode(&root.join(relative)) & 0o077, 0);
        fs::remove_dir_all(root).unwrap();
    }

    /// The official marketplace is the reference every author is pointed
    /// at, so it is held to its own check with nothing left to say: no
    /// finding, no warning — a describing field drifting back onto its
    /// entry fails here — and a valid Agent Plugins 1.0 plugin.
    #[test]
    fn the_official_marketplace_checks_clean_with_nothing_to_warn() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = uze_core::authoring::check_marketplace(&root).unwrap();

        assert!(report.findings.is_empty(), "{:?}", report.findings);
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert!(
            report
                .agent_plugins
                .as_ref()
                .is_some_and(|standard| standard.conformant),
            "{:?}",
            report.agent_plugins
        );
    }

    /// What the official listing shows is the plugin's own `plugin.json`,
    /// read from the snapshot the binary carries.
    #[test]
    fn the_official_listing_is_read_from_the_embedded_plugin_manifest() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../plugins/uze/plugin.json")).unwrap();
        let listing = &entries().unwrap().listings["uze"];

        assert_eq!(
            listing.description.as_deref(),
            manifest["description"].as_str()
        );
        assert!(!listing.keywords.is_empty());
    }

    #[test]
    fn the_official_uze_plugin_resolves_from_the_embedded_snapshot() {
        let materialized = materialize("uze").unwrap();
        assert!(materialized.root().join("plugin.json").is_file());
        assert!(materialized.root().join("skills/init/SKILL.md").is_file());
        assert!(
            materialized
                .root()
                .join("skills/worktree/SKILL.md")
                .is_file()
        );
        assert!(
            materialized
                .root()
                .join("skills/architect/SKILL.md")
                .is_file()
        );
    }

    #[test]
    fn an_unknown_plugin_name_is_an_error_not_a_silent_empty_result() {
        assert!(materialize("does-not-exist").is_err());
    }

    #[test]
    fn a_fresh_materialization_reports_no_update_against_itself() {
        let materialized = materialize("uze").unwrap();
        assert!(!has_update("uze", materialized.root()).unwrap());
    }

    #[test]
    fn a_stored_copy_with_different_content_reports_an_update() {
        let root = uze_testkit::temp::scratch("bootstrap-drift");
        let materialized = materialize("uze").unwrap();
        uze_testkit::fixtures::copy_tree(materialized.root(), &root);
        fs::write(root.join("plugin.json"), "{}").unwrap();
        assert!(has_update("uze", &root).unwrap());
        fs::remove_dir_all(root).unwrap();
    }
}
