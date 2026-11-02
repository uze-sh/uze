//! What a declared extension keeps between answers (`extension-store`):
//! one JSON object per extension and project, or one for the machine when
//! its manifest wants `store:global`.
//!
//! A record, under `state/`: what an extension remembered (a meter's
//! history, a board's last selection) is observed nowhere else. It is
//! bounded at [`STORE_MAX`] bytes as written, patched key by key in the
//! order answers arrive, and removed with its plugin.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Result, UzeError, home::UzeHome, persistence::write_atomic};

/// The most a store may hold, as written.
pub const STORE_MAX: usize = 1 << 20;
const LOCK_PATIENCE: Duration = Duration::from_secs(2);

/// Whose store, and where: an extension of `plugin@marketplace`, in one
/// project (by its canonical root) or for the machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreOf<'a> {
    pub plugin: &'a str,
    pub id: &'a str,
    pub project: Option<&'a Path>,
}

impl StoreOf<'_> {
    fn project_id(&self) -> Option<String> {
        self.project.map(uze_core::harness_runtime::project_id_for)
    }

    fn path(&self, home: &UzeHome) -> PathBuf {
        home.extension_store_path(self.plugin, self.id, self.project_id().as_deref())
    }

    fn lock_path(&self, home: &UzeHome) -> PathBuf {
        home.extension_store_lock_path(self.plugin, self.id, self.project_id().as_deref())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct StoreRecord {
    schema_version: u32,
    store: Map<String, Value>,
}

/// A record: what an extension kept is observed nowhere else.
impl uze_document::Shaped for StoreRecord {
    const SHAPE: u32 = uze_document::FIRST_SHAPE;
    const KIND: &'static str = "extension store";
}

/// What `of` holds now: an empty object before anything was kept, and
/// for a record this build cannot read.
pub fn read(home: &UzeHome, of: &StoreOf<'_>) -> Map<String, Value> {
    uze_document::read::<StoreRecord>(&of.path(home))
        .ok()
        .and_then(uze_document::Carried::record)
        .map(|record| record.store)
        .unwrap_or_default()
}

/// Replaces each key of `of`'s store that `patch` names, removing those it
/// sets to `null`. Refused, leaving the store as it was, when the result
/// would be larger than [`STORE_MAX`], and when a newer build wrote it. A
/// record this build cannot read is set aside before the patch starts an
/// empty store, never written over.
pub fn patch(
    home: &UzeHome,
    of: &StoreOf<'_>,
    patch: &Map<String, Value>,
) -> Result<Map<String, Value>> {
    let path = of.path(home);
    let lock = of.lock_path(home);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|source| UzeError::Write {
            path: dir.to_path_buf(),
            source,
        })?;
    }
    let _held = crate::file_lock::Held::acquire(&lock, LOCK_PATIENCE)?;
    let mut store = match uze_document::read::<StoreRecord>(&path) {
        Ok(carried) => carried
            .record()
            .map(|record| record.store)
            .unwrap_or_default(),
        Err(reason) if uze_document::may_be_set_aside(&reason) => {
            uze_document::set_aside(&path, <StoreRecord as uze_document::Shaped>::KIND, &reason)?;
            Map::new()
        }
        Err(reason) => return Err(reason.into()),
    };
    for (key, value) in patch {
        if value.is_null() {
            store.remove(key);
        } else {
            store.insert(key.clone(), value.clone());
        }
    }
    let record = StoreRecord {
        schema_version: <StoreRecord as uze_document::Shaped>::SHAPE,
        store,
    };
    let bytes = serde_json::to_vec(&record).expect("a store serializes");
    if bytes.len() > STORE_MAX {
        return Err(UzeError::ExtensionConsent(format!(
            "the store would hold {} KiB, over its {} KiB",
            bytes.len() / 1024,
            STORE_MAX / 1024
        )));
    }
    write_atomic(&path, &bytes)?;
    Ok(record.store)
}

/// Removes every store of the extensions of `plugin@marketplace`, as the
/// plugin leaves the machine.
pub fn forget_plugin(home: &UzeHome, plugin: &str) -> Result<()> {
    let dir = home.extension_plugin_stores_dir(plugin);
    match std::fs::remove_dir_all(&dir) {
        Err(source) if source.kind() != std::io::ErrorKind::NotFound => {
            Err(UzeError::Write { path: dir, source })
        }
        _ => Ok(()),
    }
}

/// The rungs of this module's record's ladder, for the crate's
/// [`earlier`](crate::earlier) registry.
pub(crate) fn rungs() -> Vec<uze_document::earlier::Earlier> {
    uze_document::earlier::rungs::<StoreRecord>(file!())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(project: Option<&Path>) -> StoreOf<'_> {
        StoreOf {
            plugin: "gpu@proving",
            id: "gpu-meter",
            project,
        }
    }

    #[test]
    fn a_store_is_patched_key_by_key_and_kept_per_project() {
        let root = uze_testkit::temp::scratch("extension-store");
        let home = UzeHome::at(root.join("uze"));
        let (a, b) = (root.join("a"), root.join("b"));
        let patch_of = |value: Value| value.as_object().unwrap().clone();
        patch(
            &home,
            &store(Some(&a)),
            &patch_of(serde_json::json!({ "history": [3, 5], "unit": "%" })),
        )
        .unwrap();
        patch(
            &home,
            &store(Some(&a)),
            &patch_of(serde_json::json!({ "unit": null, "last": 5 })),
        )
        .unwrap();
        assert_eq!(
            Value::Object(read(&home, &store(Some(&a)))),
            serde_json::json!({ "history": [3, 5], "last": 5 })
        );
        assert!(
            read(&home, &store(Some(&b))).is_empty(),
            "each project keeps its own"
        );
        assert!(
            read(&home, &store(None)).is_empty(),
            "and the machine's is apart"
        );

        let huge = patch_of(serde_json::json!({ "blob": "x".repeat(STORE_MAX) }));
        assert!(patch(&home, &store(Some(&a)), &huge).is_err());
        assert_eq!(
            read(&home, &store(Some(&a)))["last"],
            5,
            "a refused patch changes nothing"
        );

        forget_plugin(&home, "gpu@proving").unwrap();
        assert!(read(&home, &store(Some(&a))).is_empty());
        forget_plugin(&home, "gpu@proving").unwrap();
    }

    fn last(value: u64) -> Map<String, Value> {
        serde_json::json!({ "last": value })
            .as_object()
            .unwrap()
            .clone()
    }

    #[test]
    fn a_patch_waits_on_the_lock_its_home_names() {
        let root = uze_testkit::temp::scratch("extension-store-lock");
        let home = UzeHome::at(root.join("uze"));
        let lock = home.extension_store_lock_path("gpu@proving", "gpu-meter", None);
        let _held = crate::file_lock::Held::acquire(&lock, LOCK_PATIENCE).unwrap();
        assert!(
            matches!(
                patch(&home, &store(None), &last(5)),
                Err(UzeError::MutationInProgress { .. })
            ),
            "another patch holds the store"
        );
    }

    #[test]
    fn a_newer_builds_store_is_left_as_it_is_and_the_patch_refused() {
        let root = uze_testkit::temp::scratch("extension-store-newer");
        let home = UzeHome::at(root.join("uze"));
        let path = store(None).path(&home);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let newer = br#"{"schema_version": 2, "store": {"history": [1, 2, 3]}}"#;
        std::fs::write(&path, newer).unwrap();

        assert!(patch(&home, &store(None), &last(5)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), newer.to_vec());
    }

    #[test]
    fn an_unreadable_store_is_set_aside_before_a_patch_writes() {
        let root = uze_testkit::temp::scratch("extension-store-unreadable");
        let home = UzeHome::at(root.join("uze"));
        let path = store(None).path(&home);
        let dir = path.parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, b"{ not json").unwrap();

        let patched = patch(&home, &store(None), &last(5)).unwrap();
        assert_eq!(Value::Object(patched), serde_json::json!({ "last": 5 }));
        let kept = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .contains(uze_document::SET_ASIDE_MARK)
            });
        assert!(
            kept,
            "the unreadable bytes were kept aside, not overwritten"
        );
    }
}
