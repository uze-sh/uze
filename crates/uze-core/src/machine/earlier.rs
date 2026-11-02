//! What the machine concern does only because an earlier release left it
//! (the `earlier-releases` rule): a runtime projection's marker under the
//! name earlier releases wrote, read so a running agent's projection is not
//! swept before its next launch writes the current one.

use std::path::{Path, PathBuf};

use uze_document::earlier::{Earlier, Lasts};

/// The name earlier releases gave a projection's marker.
const PROJECTION_MARKER: &str = "project.json";

/// This concern's entries.
pub(crate) fn earlier() -> Vec<Earlier> {
    vec![Earlier {
        name: "runtime projection markers named project.json",
        // Replaced when the projection is next prepared: unbounded.
        lasts: Lasts::Unbounded,
        spelled: &[],
        site: file!(),
    }]
}

/// The root an earlier release's marker in `project_dir` names.
pub(crate) fn projection_root(project_dir: &Path) -> Option<PathBuf> {
    super::harness_runtime::read_marker(&project_dir.join(PROJECTION_MARKER))
}

/// Removes that marker once the current one is written.
pub(crate) fn remove_projection_marker(project_dir: &Path) {
    let _ = std::fs::remove_file(project_dir.join(PROJECTION_MARKER));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_earlier_marker_still_names_its_root_until_the_projection_is_prepared() {
        let scratch = uze_testkit::temp::scratch("earlier-projection-marker");
        let project = scratch.join("demo");
        std::fs::create_dir_all(&project).unwrap();
        let home = crate::UzeHome::at(scratch.join("uze"));
        let id = crate::harness_runtime::project_id_for(&project);
        let dir = home.runtime_project_dir(&id);
        std::fs::create_dir_all(&dir).unwrap();
        let marker = serde_json::json!({ "root": project }).to_string();
        std::fs::write(dir.join(PROJECTION_MARKER), marker).unwrap();

        assert_eq!(
            crate::harness_runtime::projection_root(&dir),
            Some(project.clone())
        );

        crate::harness_runtime::prepare_projection(&home, "fake-harness", &project).unwrap();
        assert!(!dir.join(PROJECTION_MARKER).exists());
        assert_eq!(crate::harness_runtime::projection_root(&dir), Some(project));
    }
}
