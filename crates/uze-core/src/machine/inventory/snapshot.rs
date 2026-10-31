//! The last Overview collected, so the next opening draws at once.
//!
//! Remembered, not recorded: it lives in `cache/`, carries no shape, and is
//! collected again whenever it cannot be read. It holds names, kinds, the
//! paths of definition files, origins, figures and statuses — nothing a
//! session said.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::{Result, home::UzeHome, persistence::write_atomic};

use super::model::MachineOverview;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    collected_at: SystemTime,
    overview: MachineOverview,
}

/// The last Overview collected and when, if one can be read.
pub fn load_snapshot(home: &UzeHome) -> Option<(MachineOverview, SystemTime)> {
    let bytes = std::fs::read(home.machine_overview_cache_path()).ok()?;
    let snapshot: Snapshot = serde_json::from_slice(&bytes).ok()?;
    Some((snapshot.overview, snapshot.collected_at))
}

pub fn save_snapshot(
    home: &UzeHome,
    overview: &MachineOverview,
    collected_at: SystemTime,
) -> Result<()> {
    let snapshot = Snapshot {
        collected_at,
        overview: overview.clone(),
    };
    let path = home.machine_overview_cache_path();
    let payload = serde_json::to_vec(&snapshot).map_err(|source| crate::UzeError::Json {
        path: path.clone(),
        source,
    })?;
    write_atomic(&path, &payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unreadable_snapshot_is_simply_absent() {
        let root = uze_testkit::temp::TempDir::new("machine-snapshot");
        let home = UzeHome::at(root.path());
        assert!(load_snapshot(&home).is_none());
        std::fs::create_dir_all(home.cache_dir()).unwrap();
        std::fs::write(home.machine_overview_cache_path(), b"{not json").unwrap();
        assert!(load_snapshot(&home).is_none());
    }

    #[test]
    fn a_snapshot_reads_back_as_it_was_saved() {
        let root = uze_testkit::temp::TempDir::new("machine-snapshot");
        let home = UzeHome::at(root.path());
        let overview = MachineOverview::default();
        let now = SystemTime::now();
        save_snapshot(&home, &overview, now).unwrap();
        let (read, at) = load_snapshot(&home).unwrap();
        assert_eq!(read, overview);
        assert_eq!(at, now);
    }
}
