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
    fn path(&self, home: &UzeHome) -> PathBuf {
        let project = self.project.map(uze_core::harness_runtime::project_id_for);
        home.extension_store_path(self.plugin, self.id, project.as_deref())
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
/// would be larger than [`STORE_MAX`].
pub fn patch(
    home: &UzeHome,
    of: &StoreOf<'_>,
    patch: &Map<String, Value>,
) -> Result<Map<String, Value>> {
    let path = of.path(home);
    let lock = path.with_extension("lock");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|source| UzeError::Write {
            path: dir.to_path_buf(),
            source,
        })?;
    }
    let _held = crate::file_lock::Held::acquire(&lock, LOCK_PATIENCE)?;
    let mut store = read(home, of);
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
}
