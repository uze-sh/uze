//! The move that gave the package bytes a directory of their own.
//!
//! Package bytes lived in `store/plugins` and `store/superseded`. They now
//! live in `sources/`, so `store/` can be what the harnesses read, one
//! vendor store each. This is the rung for that release, run under the
//! mutation lock by every mutation until no machine is on the old layout.
//!
//! **The fence comes first.** The package registry, the receipt ledger and
//! the marketplace registry are written in the shape this build writes
//! *before* any byte moves, so an older build that runs afterwards refuses
//! them by name instead of reading a registry whose bytes are gone, pruning
//! every entry as a ghost, detaching its receipts as orphans, or rewriting
//! the marketplaces without the pins this build records there.
//!
//! **Then each package moves on its own.** A rename per package and per
//! superseded revision, so an interruption leaves some moved and some not,
//! and the next run carries the rest. A marketplace already on the new side
//! is merged into, package by package; a package already on the new side
//! is left where it is, since nothing is overwritten and two revisions are
//! never mixed.
//!
//! **Then the digests the registry lacks are recorded**, read from the
//! moved bytes. A ladder step cannot do it: it is a function over the
//! document and never sees the disk.

use std::fs;
use std::path::Path;

use crate::{Result, UzeError, UzeHome, UzeStore, state};

/// Carries the package bytes of a machine on the old layout into
/// `sources/`. A no-op, after one `stat` per old directory, on a machine
/// that never had them or has already moved.
///
/// Called with the mutation lock held.
pub(crate) fn carry_into_sources(home: &UzeHome) -> Result<()> {
    let layout = home.pre_sources_layout();
    if !layout.iter().any(|(old, _)| old.exists()) {
        return Ok(());
    }
    let store = UzeStore::new(home.clone());
    store.rewrite_registry_with_digests()?;
    state::rewrite_ledger(home)?;
    state::rewrite_marketplaces(home)?;
    for (old, new) in &layout {
        // `plugins/<market>/<plugin>` and `superseded/<package>`: how deep a
        // whole package sits, which is what moves in one rename.
        let package_depth = if *new == home.plugins_dir() { 2 } else { 1 };
        carry_entries(old, new, package_depth)?;
    }
    store.rewrite_registry_with_digests()
}

/// Moves every entry `depth` levels under `old` that `new` lacks, merging
/// the directories above it, then removes each directory the move emptied;
/// an older build that ran before the fence existed may also have left an
/// empty one behind.
fn carry_entries(old: &Path, new: &Path, depth: usize) -> Result<()> {
    let Ok(entries) = fs::read_dir(old) else {
        return Ok(());
    };
    fs::create_dir_all(new).map_err(UzeError::write(new))?;
    for entry in entries.flatten() {
        let source = entry.path();
        let destination = new.join(entry.file_name());
        if !destination.exists() {
            fs::rename(&source, &destination).map_err(UzeError::write(&destination))?;
        } else if depth > 1 && source.is_dir() && destination.is_dir() {
            carry_entries(&source, &destination, depth - 1)?;
        }
    }
    let _ = fs::remove_dir(old);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(label: &str) -> UzeHome {
        let root = std::env::temp_dir().join(format!(
            "uze-sources-move-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        UzeHome::at(root)
    }

    fn old_package(home: &UzeHome, market: &str, plugin: &str) {
        let dir = home.store_dir().join("plugins").join(market).join(plugin);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("plugin.json"), format!(r#"{{"name":"{plugin}"}}"#)).unwrap();
    }

    /// A machine as the previous release left it: a package installed, its
    /// bytes under `store/plugins`, and a shape-1 registry with no digest.
    fn installed_on_the_old_layout(home: &UzeHome) {
        let source = home.root().join("source");
        fs::create_dir_all(source.join("skills/locate")).unwrap();
        fs::write(source.join("plugin.json"), r#"{"name":"flow"}"#).unwrap();
        fs::write(
            source.join("skills/locate/SKILL.md"),
            "---\nname: locate\ndescription: d\n---\nbody\n",
        )
        .unwrap();
        let store = UzeStore::new(home.clone());
        store
            .ingest(
                &crate::acquisition::acquire(&crate::PackageSource::local(&source)).unwrap(),
                "ai",
                None,
            )
            .unwrap();
        fs::create_dir_all(home.store_dir()).unwrap();
        fs::rename(home.plugins_dir(), home.store_dir().join("plugins")).unwrap();
        let mut registry: serde_json::Value =
            serde_json::from_slice(&fs::read(home.registry_path()).unwrap()).unwrap();
        registry.as_object_mut().unwrap().remove("schema_version");
        for entry in registry["packages"].as_object_mut().unwrap().values_mut() {
            entry.as_object_mut().unwrap().remove("digest");
        }
        fs::write(home.registry_path(), serde_json::to_vec(&registry).unwrap()).unwrap();
    }

    #[test]
    fn a_machine_with_nothing_to_move_is_left_alone() {
        let home = home("nothing");
        carry_into_sources(&home).unwrap();
        assert!(!home.sources_dir().exists());
        assert!(!home.registry_path().exists());
    }

    #[test]
    fn the_bytes_move_and_nothing_is_overwritten() {
        let home = home("moves");
        old_package(&home, "ai", "flow");
        old_package(&home, "ai", "kit");
        let superseded = home.store_dir().join("superseded").join("flow@ai");
        fs::create_dir_all(&superseded).unwrap();
        // Something already on the new side, as an interrupted earlier run
        // would leave it: kept, never overwritten.
        let already = home.plugins_dir().join("other");
        fs::create_dir_all(&already).unwrap();
        fs::write(already.join("keep"), "kept").unwrap();

        carry_into_sources(&home).unwrap();

        assert!(home.plugins_dir().join("ai/flow/plugin.json").is_file());
        assert!(home.plugins_dir().join("ai/kit/plugin.json").is_file());
        assert!(home.sources_dir().join("superseded/flow@ai").is_dir());
        assert_eq!(fs::read_to_string(already.join("keep")).unwrap(), "kept");
        assert!(!home.store_dir().join("plugins").exists());
        assert!(!home.store_dir().join("superseded").exists());
        fs::remove_dir_all(home.root()).unwrap();
    }

    #[test]
    fn a_marketplace_on_both_sides_is_merged_package_by_package() {
        let home = home("merge");
        old_package(&home, "ai", "flow");
        old_package(&home, "ai", "kit");
        // The same marketplace already on the new side, holding one of the
        // two packages in another revision.
        let kept = home.plugins_dir().join("ai/kit");
        fs::create_dir_all(&kept).unwrap();
        fs::write(kept.join("plugin.json"), "new revision").unwrap();

        carry_into_sources(&home).unwrap();

        assert!(home.plugins_dir().join("ai/flow/plugin.json").is_file());
        assert_eq!(
            fs::read_to_string(kept.join("plugin.json")).unwrap(),
            "new revision"
        );
        assert!(
            !home.store_dir().join("plugins/ai/flow").exists(),
            "the package the new side lacked was stranded"
        );
        // Never overwritten, and never mixed into the kept revision.
        assert!(
            home.store_dir()
                .join("plugins/ai/kit/plugin.json")
                .is_file()
        );
        fs::remove_dir_all(home.root()).unwrap();
    }

    #[test]
    fn a_fence_with_nothing_to_change_leaves_the_registry_untouched() {
        let home = home("untouched");
        installed_on_the_old_layout(&home);
        carry_into_sources(&home).unwrap();
        // Left behind on the old side: the move runs again on the next
        // mutation, and finds the registry already as it would write it.
        old_package(&home, "ai", "flow");
        let registry: serde_json::Value =
            serde_json::from_slice(&fs::read(home.registry_path()).unwrap()).unwrap();
        let compact = serde_json::to_vec(&registry).unwrap();
        fs::write(home.registry_path(), &compact).unwrap();

        carry_into_sources(&home).unwrap();

        assert_eq!(fs::read(home.registry_path()).unwrap(), compact);
        fs::remove_dir_all(home.root()).unwrap();
    }

    #[test]
    fn the_fence_is_written_and_every_moved_package_gets_its_digest() {
        let home = home("fence");
        installed_on_the_old_layout(&home);

        carry_into_sources(&home).unwrap();

        let written: serde_json::Value =
            serde_json::from_slice(&fs::read(home.registry_path()).unwrap()).unwrap();
        assert_eq!(written["schema_version"], 2, "{written}");
        assert!(
            written["packages"]["flow@ai"]["digest"].is_string(),
            "{written}"
        );
        assert!(home.plugins_dir().join("ai/flow/plugin.json").is_file());
        let id = crate::PackageId::from_marketplace_plugin(
            "ai",
            "flow",
            &home.plugins_dir().join("ai/flow/plugin.json"),
        )
        .unwrap();
        let store = UzeStore::new(home.clone());
        assert_eq!(
            store.recorded_digest(&id).unwrap(),
            Some(store.digest(&store.package(&id).unwrap()).unwrap())
        );
        fs::remove_dir_all(home.root()).unwrap();
    }

    #[test]
    fn the_receipt_ledger_is_fenced_even_when_no_receipt_changes() {
        let home = home("ledger-fence");
        old_package(&home, "ai", "flow");
        fs::create_dir_all(home.state_dir()).unwrap();
        fs::write(
            home.attachments_path(),
            r#"{"schema_version":3,"receipts":[]}"#,
        )
        .unwrap();

        carry_into_sources(&home).unwrap();

        let written: serde_json::Value =
            serde_json::from_slice(&fs::read(home.attachments_path()).unwrap()).unwrap();
        assert_eq!(written["schema_version"], 4, "{written}");
        fs::remove_dir_all(home.root()).unwrap();
    }

    #[test]
    fn the_marketplace_registry_is_fenced() {
        let home = home("marketplaces-fence");
        old_package(&home, "ai", "flow");
        fs::create_dir_all(home.state_dir()).unwrap();
        fs::write(
            home.marketplaces_path(),
            r#"{"marketplaces":{"ai":{"source":{"LOCAL":{"path":"/m"}}}}}"#,
        )
        .unwrap();

        carry_into_sources(&home).unwrap();

        let written: serde_json::Value =
            serde_json::from_slice(&fs::read(home.marketplaces_path()).unwrap()).unwrap();
        assert_eq!(written["schema_version"], 2, "{written}");
        assert_eq!(
            written["marketplaces"]["ai"]["source"]["LOCAL"]["path"], "/m",
            "{written}"
        );
        fs::remove_dir_all(home.root()).unwrap();
    }

    #[test]
    fn an_empty_directory_an_older_build_recreated_is_removed() {
        let home = home("empty");
        fs::create_dir_all(home.store_dir().join("plugins")).unwrap();
        carry_into_sources(&home).unwrap();
        assert!(!home.store_dir().join("plugins").exists());
        fs::remove_dir_all(home.root()).unwrap();
    }
}
