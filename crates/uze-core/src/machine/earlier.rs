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

/// Carries every record this build renamed (a retired owned path naming
/// the builder it became) to its current name: a project's when `project`
/// names one, the machine's otherwise. Renamed only where the current name
/// is free, so nothing is ever overwritten, and the shape is untouched; a
/// rename that fails leaves the bytes where they were. A few `stat`s once
/// carried, so it runs wherever the records' readers and writers start.
pub fn carry_renamed(home: &crate::UzeHome, project: Option<&str>) {
    for retired in crate::UzeHome::owned_paths() {
        let Some(current) = retired.renamed_to.and_then(crate::UzeHome::owned_path) else {
            continue;
        };
        let (was, now) = (retired.path(home), current.path(home));
        let (was, now) = match project {
            Some(id) => {
                let spell = |path: PathBuf| -> Option<PathBuf> {
                    let text = path.to_string_lossy();
                    text.contains("{project}")
                        .then(|| PathBuf::from(text.replace("{project}", id)))
                };
                match (spell(was), spell(now)) {
                    (Some(was), Some(now)) => (was, now),
                    _ => continue,
                }
            }
            None if was.to_string_lossy().contains('{') => continue,
            None => (was, now),
        };
        if was.is_file() && !now.exists() {
            if let Some(parent) = now.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::rename(&was, &now);
        }
    }
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

    #[test]
    fn a_home_with_the_floor_releases_names_comes_out_renamed_and_unchanged() {
        let home = crate::UzeHome::at(uze_testkit::temp::scratch("carry-renamed").join("uze"));
        let seeded = [
            (home.retired_release_path(), home.binary_path(), "release"),
            (
                home.retired_launches_path(),
                home.launch_overrides_path(),
                "launches",
            ),
            (
                home.retired_usage_path(),
                home.machine_usage_path(),
                "usage",
            ),
            (
                home.retired_consent_path(),
                home.extensions_record_path(),
                "consent",
            ),
            (
                home.retired_tasks_path("p1"),
                home.tasks_path("p1"),
                "tasks",
            ),
            (
                home.retired_approvals_path("p1"),
                home.approved_commands_path("p1"),
                "approvals",
            ),
            (
                home.retired_prompts_path("p1"),
                home.prompt_history_path("p1"),
                "prompts",
            ),
        ];
        for (was, _, content) in &seeded {
            std::fs::create_dir_all(was.parent().unwrap()).unwrap();
            std::fs::write(was, content).unwrap();
        }
        let lock = home.retired_tasks_lock_path("p1");
        std::fs::write(&lock, "").unwrap();

        for _ in 0..2 {
            carry_renamed(&home, None);
            carry_renamed(&home, Some("p1"));
        }

        for (was, now, content) in &seeded {
            assert!(!was.exists(), "{}", was.display());
            assert_eq!(
                std::fs::read_to_string(now).unwrap(),
                *content,
                "{}",
                now.display()
            );
        }
        assert!(
            lock.exists(),
            "a lock is not carried: an older build still takes it by name"
        );
    }

    #[test]
    fn a_record_already_at_its_new_name_is_never_overwritten() {
        let home = crate::UzeHome::at(uze_testkit::temp::scratch("carry-no-overwrite").join("uze"));
        std::fs::create_dir_all(home.state_dir()).unwrap();
        std::fs::write(home.retired_release_path(), "older").unwrap();
        std::fs::write(home.binary_path(), "newer").unwrap();

        carry_renamed(&home, None);

        assert_eq!(
            std::fs::read_to_string(home.binary_path()).unwrap(),
            "newer"
        );
        assert_eq!(
            std::fs::read_to_string(home.retired_release_path()).unwrap(),
            "older"
        );
    }
}
