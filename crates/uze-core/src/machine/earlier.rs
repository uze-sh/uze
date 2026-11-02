//! What the machine concern does only because an earlier release left it
//! (the `earlier-releases` rule): a runtime projection's marker under the
//! name earlier releases wrote, and the one step that brings a home to
//! today's names (`carry_all`). An update replaces the binary and restarts
//! nothing, so that step waits until no client of the floor release can
//! still be open.

use std::path::{Path, PathBuf};

use uze_document::earlier::{Earlier, Lasts};

/// The name earlier releases gave a projection's marker.
const PROJECTION_MARKER: &str = "project.json";

/// This concern's entries.
pub(crate) fn earlier() -> Vec<Earlier> {
    vec![
        Earlier {
            name: "runtime projection markers named project.json, read",
            // Read until the projection is next prepared, which an agent
            // launched before the upgrade may never do: unbounded.
            lasts: Lasts::Unbounded,
            spelled: &[PROJECTION_MARKER],
            site: file!(),
        },
        Earlier {
            name: "runtime projection markers named project.json, written for beta.22",
            // Written beside root.json only while a record is uncarried: a
            // beta.22 client, which reads no other name, may still be open.
            lasts: Lasts::Until(uze_document::earlier::Release::beta(1, 0, 0, 22)),
            spelled: &[],
            site: file!(),
        },
        Earlier {
            name: "theme overrides and [memory] harness kept outside today's config.toml shape",
            // Carried when the carry next runs: unbounded.
            lasts: Lasts::Unbounded,
            spelled: &[],
            site: file!(),
        },
    ]
}

/// Brings the home to today's names, once nothing of the floor release can
/// be running on it: the settings, every record of the machine and of every
/// project, and what is left of those names afterwards. Run when a client
/// starts, which a restart into a new release is, or by a command while no
/// workspace is running; never beside a client that may still read the
/// earlier names (`UzeHome`'s builders follow a record until then).
pub fn carry_all(home: &crate::UzeHome) {
    carry_settings(home);
    carry_renamed(home, None);
    let projects: Vec<String> = std::fs::read_dir(home.projects_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    for project in &projects {
        carry_renamed(home, Some(project));
    }
    remove_earlier_locks(home, &projects);
    remove_rebuilt_copies(home);
    carry_projection_markers(home);
}

/// Whether a record is still under the name the floor release gave it, so
/// a client of that release may yet be open and using what it left.
pub fn holds_uncarried_records(home: &crate::UzeHome) -> bool {
    let projects: Vec<String> = std::fs::read_dir(home.projects_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    crate::UzeHome::owned_paths().iter().any(|retired| {
        let Some(current) = retired.renamed_to.and_then(crate::UzeHome::owned_path) else {
            return false;
        };
        let (was, now) = (retired.path(home), current.path(home));
        if !was.to_string_lossy().contains("{project}") {
            return was.is_file() && !now.exists();
        }
        projects
            .iter()
            .any(|id| spelled(&was, id).is_file() && !spelled(&now, id).exists())
    })
}

fn spelled(path: &Path, project: &str) -> PathBuf {
    PathBuf::from(path.to_string_lossy().replace("{project}", project))
}

/// Removes the locks of the earlier names: nothing takes them once the
/// records they guarded have moved. A failure leaves an empty file that
/// `doctor` accounts for.
fn remove_earlier_locks(home: &crate::UzeHome, projects: &[String]) {
    for retired in crate::UzeHome::owned_paths().iter().filter(|owned| {
        owned.standing == crate::home::Standing::Retired && owned.form == crate::home::Form::Lock
    }) {
        let path = retired.path(home);
        let text = path.to_string_lossy();
        if text.contains("{entry}") {
            let Some(directory) = path.parent() else {
                continue;
            };
            for lock in std::fs::read_dir(directory).into_iter().flatten().flatten() {
                if lock.path().extension().is_some_and(|ext| ext == "lock") {
                    let _ = std::fs::remove_file(lock.path());
                }
            }
            let _ = std::fs::remove_dir(directory);
        } else if text.contains("{project}") {
            for project in projects {
                let _ = std::fs::remove_file(spelled(&path, project));
            }
        } else {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Removes what is only a remembered answer when it is left beside the
/// record it moved to: the release check asks again within the hour.
fn remove_rebuilt_copies(home: &crate::UzeHome) {
    if home.names_only().release_path().exists() {
        let _ = std::fs::remove_file(home.retired_release_path());
    }
}

/// Moves each projection's marker to today's name. A projection is keyed
/// on the project's own root, so an existing marker says the same thing.
fn carry_projection_markers(home: &crate::UzeHome) {
    for project in std::fs::read_dir(home.runtime_projects_dir())
        .into_iter()
        .flatten()
        .flatten()
    {
        let earlier = project.path().join(PROJECTION_MARKER);
        if !earlier.is_file() {
            continue;
        }
        let current = project
            .path()
            .join(super::harness_runtime::PROJECTION_MARKER);
        if current.exists() || move_unless_taken(&earlier, &current).is_err() {
            let _ = std::fs::remove_file(&earlier);
        }
    }
}

/// Carries the records this build renamed (a retired owned path naming the
/// builder it became) to their current names: a project's when `project`
/// names one, the machine's otherwise. Renamed only where the current name
/// is free, so nothing is ever overwritten, and the shape is untouched; a
/// rename that fails leaves the bytes where they were.
fn carry_renamed(home: &crate::UzeHome, project: Option<&str>) {
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
            if let Err(error) = move_unless_taken(&was, &now) {
                tracing::debug!(was = %was.display(), %error, "a record was not carried to its current name");
            }
        }
    }
}

/// Removes the files directly in `dir` that one of the owned map's retired
/// entries `names` composes and `also` accepts: the map's own pattern, so
/// a collector never spells an earlier release's names a second time.
pub fn remove_retired_files(
    home: &crate::UzeHome,
    dir: &Path,
    names: &[&str],
    also: impl Fn(&Path) -> bool,
) -> Vec<PathBuf> {
    let retired: Vec<_> = names
        .iter()
        .filter_map(|name| crate::UzeHome::owned_path(name))
        .collect();
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && also(path))
        .filter(|path| {
            let relative: Vec<String> = path
                .strip_prefix(home.root())
                .unwrap_or(path)
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            retired.iter().any(|owned| owned.matches(home, &relative))
        })
        .filter(|path| std::fs::remove_file(path).is_ok())
        .collect()
}

/// Moves `was` to `now` only while nothing is at `now`: a link refuses a
/// taken name where a rename would replace it, so a record another process
/// wrote since the check is never overwritten by the earlier one.
fn move_unless_taken(was: &Path, now: &Path) -> std::io::Result<()> {
    move_with(was, now, |was, now| std::fs::hard_link(was, now))
}

/// The same, linking with `link`. A filesystem that cannot link (FAT, some
/// network shares) is moved by a rename once `now` is seen free again: the
/// narrower guard, but a record that is never carried is worse.
fn move_with(
    was: &Path,
    now: &Path,
    link: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    match link(was, now) {
        Ok(()) => std::fs::remove_file(was),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists || now.exists() => {
            Err(error)
        }
        Err(_) => std::fs::rename(was, now),
    }
}

/// Carries what an earlier release kept outside `config.toml` into it: the
/// overrides of the active theme (`theme-overrides.json`) into
/// `[appearance.colors|symbols|syntax]` and its glyph set into
/// `[appearance] glyphs`, and `[memory] harness` to
/// `[memory] native`. A key the operator already has is never replaced; a `config.toml` that does not parse stops the carry, and the
/// earlier file stays where it is to be carried once it is fixed.
fn carry_settings(home: &crate::UzeHome) {
    let overrides = home.retired_theme_overrides_path();
    if let Ok(text) = std::fs::read_to_string(&overrides) {
        match serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&text) {
            Ok(layer) => {
                let glyphs = layer
                    .get("glyphs")
                    .and_then(|set| set.as_str())
                    .map(str::to_owned);
                let tables = layer
                    .into_iter()
                    .filter(|(field, _)| ["colors", "symbols", "syntax"].contains(&field.as_str()))
                    .collect();
                let carried = crate::config::add_tables(home, "appearance", &tables).and_then(
                    |_| match glyphs {
                        Some(set) if crate::appearance::glyphs(home)?.is_none() => {
                            crate::appearance::set_glyphs(home, &set)
                        }
                        _ => Ok(()),
                    },
                );
                match carried {
                    Ok(_) => {
                        let _ = std::fs::remove_file(&overrides);
                    }
                    Err(error) => {
                        tracing::warn!(%error, "the theme overrides were not carried into config.toml")
                    }
                }
            }
            Err(error) => {
                tracing::warn!(path = %overrides.display(), %error, "the theme overrides are not JSON; left where they are")
            }
        }
    }
    // Read before any lock is taken: every process passes here, and only
    // a machine holding the old key alone pays for the write.
    if matches!(crate::config::get(home, "memory", "harness"), Ok(Some(_)))
        && matches!(crate::config::get(home, "memory", "native"), Ok(None))
        && let Err(error) = crate::config::rename_key(home, "memory", "harness", "native")
    {
        tracing::warn!(%error, "`[memory] harness` was not carried to `native`");
    }
}

/// `[memory] native`, or the key it was until the settings are carried.
pub fn memory_native_setting(home: &crate::UzeHome) -> Option<String> {
    crate::config::get(home, "memory", "native")
        .ok()
        .flatten()
        .or_else(|| crate::config::get(home, "memory", "harness").ok().flatten())
}

/// The root an earlier release's marker in `project_dir` names.
pub(crate) fn projection_root(project_dir: &Path) -> Option<PathBuf> {
    super::harness_runtime::read_marker(&project_dir.join(PROJECTION_MARKER))
}

/// Writes that marker beside the current one while any record is still
/// under the floor release's names (a checkout's projection has an id of
/// its own, so this asks of the machine): a client of that release may
/// be open, reads only this name, and sweeps a projection it cannot find a
/// root for while its agents are still in it.
pub(crate) fn write_projection_marker(
    home: &crate::UzeHome,
    project_dir: &Path,
    marker: &[u8],
) -> crate::Result<()> {
    if !holds_uncarried_records(home) {
        return Ok(());
    }
    let path = project_dir.join(PROJECTION_MARKER);
    if std::fs::read(&path).is_ok_and(|current| current == marker) {
        return Ok(());
    }
    crate::persistence::write_atomic_unsynced(&path, marker)
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
        assert_eq!(crate::harness_runtime::projection_root(&dir), Some(project));
    }

    #[test]
    fn a_projection_names_its_root_to_the_floor_release_only_until_the_home_is_carried() {
        let scratch = uze_testkit::temp::scratch("earlier-projection-marker-written");
        let project = scratch.join("demo");
        std::fs::create_dir_all(&project).unwrap();
        let home = crate::UzeHome::at(scratch.join("uze"));
        std::fs::create_dir_all(home.state_dir()).unwrap();
        std::fs::write(home.retired_release_path(), "{}").unwrap();

        crate::harness_runtime::prepare_projection(&home, "fake-harness", &project).unwrap();

        let dir = home.runtime_project_dir(&crate::harness_runtime::project_id_for(&project));
        assert_eq!(
            projection_root(&dir),
            crate::harness_runtime::projection_root(&dir),
            "a client of the floor release, still open, finds the projection live"
        );
        assert!(projection_root(&dir).is_some());

        carry_all(&home);

        assert!(!dir.join(PROJECTION_MARKER).exists());
        assert!(crate::harness_runtime::projection_root(&dir).is_some());
        crate::harness_runtime::prepare_projection(&home, "fake-harness", &project).unwrap();
        assert!(
            !dir.join(PROJECTION_MARKER).exists(),
            "nothing reads that name any more"
        );
    }

    #[test]
    fn until_it_is_carried_every_build_uses_a_record_where_it_is_and_its_lock() {
        let home = crate::UzeHome::at(uze_testkit::temp::scratch("where-it-is").join("uze"));
        let (today, today_lock) = (home.tasks_path("p1"), home.tasks_lock_path("p1"));
        let was = home.retired_tasks_path("p1");
        std::fs::create_dir_all(was.parent().unwrap()).unwrap();
        std::fs::write(&was, "{}").unwrap();

        assert_eq!(home.tasks_path("p1"), was);
        assert_eq!(
            home.tasks_lock_path("p1"),
            home.retired_tasks_lock_path("p1")
        );
        assert!(holds_uncarried_records(&home));

        carry_all(&home);

        assert_eq!(home.tasks_path("p1"), today);
        assert_eq!(home.tasks_lock_path("p1"), today_lock);
        assert!(today.is_file() && !was.exists());
        assert!(!holds_uncarried_records(&home));
    }

    #[test]
    fn a_carried_home_keeps_nothing_of_the_earlier_names() {
        let home = crate::UzeHome::at(uze_testkit::temp::scratch("carry-no-trash").join("uze"));
        let project = home.project_dir("p1");
        std::fs::create_dir_all(&project).unwrap();
        let mirror_lock = home.retired_mirror_lock_path("market");
        std::fs::create_dir_all(mirror_lock.parent().unwrap()).unwrap();
        let earlier = [
            home.retired_release_path(),
            home.retired_release_lock_path(),
            home.retired_launches_path(),
            home.retired_launches_lock_path(),
            home.retired_usage_path(),
            home.retired_usage_lock_path(),
            home.retired_consent_path(),
            home.retired_consent_lock_path(),
            home.retired_fetch_lock_path(),
            home.retired_tasks_path("p1"),
            home.retired_tasks_lock_path("p1"),
            home.retired_approvals_path("p1"),
            home.retired_prompts_path("p1"),
            home.retired_prompts_lock_path("p1"),
            mirror_lock.clone(),
        ];
        for path in &earlier {
            std::fs::write(path, "{}").unwrap();
        }
        // A remembered answer left beside its current record goes too.
        let names = home.names_only();

        carry_all(&home);
        std::fs::write(home.retired_release_path(), "{}").unwrap();
        carry_all(&home);

        for path in &earlier {
            assert!(!path.exists(), "{}", path.display());
        }
        assert!(!mirror_lock.parent().unwrap().exists());
        assert!(names.release_path().is_file());
    }

    #[test]
    fn a_home_with_the_floor_releases_names_comes_out_renamed_and_unchanged() {
        let home = crate::UzeHome::at(uze_testkit::temp::scratch("carry-renamed").join("uze"));
        let seeded = [
            (home.retired_release_path(), home.release_path(), "release"),
            (
                home.retired_launches_path(),
                home.launches_path(),
                "launches",
            ),
            (home.retired_usage_path(), home.usage_path(), "usage"),
            (home.retired_consent_path(), home.consent_path(), "consent"),
            (
                home.retired_tasks_path("p1"),
                home.tasks_path("p1"),
                "tasks",
            ),
            (
                home.retired_approvals_path("p1"),
                home.approvals_path("p1"),
                "approvals",
            ),
            (
                home.retired_prompts_path("p1"),
                home.prompts_path("p1"),
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
    fn a_move_never_replaces_what_took_the_name_after_the_check() {
        let scratch = uze_testkit::temp::scratch("carry-move-unless-taken");
        std::fs::create_dir_all(&scratch).unwrap();
        let (was, now) = (scratch.join("was.json"), scratch.join("now.json"));
        std::fs::write(&was, "older").unwrap();
        std::fs::write(&now, "newer").unwrap();

        assert!(move_unless_taken(&was, &now).is_err());

        assert_eq!(std::fs::read_to_string(&now).unwrap(), "newer");
        assert_eq!(std::fs::read_to_string(&was).unwrap(), "older");
    }

    #[test]
    fn a_filesystem_that_cannot_link_still_carries_a_record() {
        let scratch = uze_testkit::temp::scratch("carry-move-no-links");
        std::fs::create_dir_all(&scratch).unwrap();
        let (was, now) = (scratch.join("was.json"), scratch.join("now.json"));
        std::fs::write(&was, "older").unwrap();
        let unsupported = |_: &Path, _: &Path| Err(std::io::ErrorKind::Unsupported.into());

        move_with(&was, &now, unsupported).unwrap();

        assert_eq!(std::fs::read_to_string(&now).unwrap(), "older");
        assert!(!was.exists());
    }

    #[test]
    fn only_the_logs_earlier_releases_kept_loose_are_removed() {
        let home = crate::UzeHome::at(uze_testkit::temp::scratch("loose-logs").join("uze"));
        std::fs::create_dir_all(home.logs_dir()).unwrap();
        let loose = [
            home.loose_gate_log_path("a1"),
            home.loose_setup_log_path("codex"),
        ];
        let kept = home.logs_dir().join("uze.log");
        for path in loose.iter().chain([&kept]) {
            std::fs::write(path, "x").unwrap();
        }

        let mut removed = remove_retired_files(
            &home,
            &home.logs_dir(),
            &["loose_gate_log_path", "loose_setup_log_path"],
            |_| true,
        );
        removed.sort();

        assert_eq!(removed, loose.to_vec());
        assert!(kept.exists());
    }

    #[test]
    fn a_record_already_at_its_new_name_is_never_overwritten() {
        let home = crate::UzeHome::at(uze_testkit::temp::scratch("carry-no-overwrite").join("uze"));
        std::fs::create_dir_all(home.state_dir()).unwrap();
        let current = home.release_path();
        std::fs::write(home.retired_release_path(), "older").unwrap();
        std::fs::write(&current, "newer").unwrap();

        carry_renamed(&home, None);

        assert_eq!(std::fs::read_to_string(&current).unwrap(), "newer");
        assert_eq!(
            std::fs::read_to_string(home.retired_release_path()).unwrap(),
            "older"
        );
    }

    #[test]
    fn the_theme_overrides_and_the_memory_key_are_carried_into_config_toml() {
        let home = crate::UzeHome::at(uze_testkit::temp::scratch("carry-settings").join("uze"));
        std::fs::create_dir_all(home.root()).unwrap();
        std::fs::write(
            home.retired_theme_overrides_path(),
            r##"{ "symbols": { "status.idle": "@", "mark.native": { "glyph": "N", "width": 2 } },
                 "colors": { "accent": "#bd93f9", "state.success": "#50fa7b" } }"##,
        )
        .unwrap();
        std::fs::write(
            home.config_path(),
            "# mine\n[memory]\nharness = \"off\" # keep it quiet\n\n[appearance.colors]\naccent = \"#000000\"\n",
        )
        .unwrap();

        carry_all(&home);
        carry_all(&home);

        assert!(!home.retired_theme_overrides_path().exists());
        let text = std::fs::read_to_string(home.config_path()).unwrap();
        assert!(
            text.contains("# mine") && text.contains("# keep it quiet"),
            "{text}"
        );
        assert_eq!(
            crate::config::get(&home, "memory", "native")
                .unwrap()
                .as_deref(),
            Some("off")
        );
        assert_eq!(
            crate::config::get(&home, "memory", "harness").unwrap(),
            None
        );
        let layer = crate::appearance::overrides(&home).unwrap().unwrap();
        assert_eq!(layer["symbols"]["status.idle"], "@");
        assert_eq!(layer["symbols"]["mark.native"]["width"], 2);
        assert_eq!(
            layer["colors"]["accent"], "#000000",
            "the operator's own table is kept"
        );
    }

    #[test]
    fn the_glyph_set_an_override_chose_is_carried_unless_one_is_chosen() {
        let carried = |label: &str, config: &str| {
            let home = crate::UzeHome::at(uze_testkit::temp::scratch(label).join("uze"));
            std::fs::create_dir_all(home.root()).unwrap();
            std::fs::write(
                home.retired_theme_overrides_path(),
                r#"{ "glyphs": "ascii", "symbols": { "status.idle": "@" } }"#,
            )
            .unwrap();
            std::fs::write(home.config_path(), config).unwrap();
            carry_all(&home);
            assert!(!home.retired_theme_overrides_path().exists());
            crate::appearance::glyphs(&home).unwrap()
        };

        assert_eq!(carried("carry-glyphs", "").as_deref(), Some("ascii"));
        assert_eq!(
            carried("carry-glyphs-chosen", "[appearance]\nglyphs = \"nerd\"\n").as_deref(),
            Some("nerd"),
            "the operator's own choice is kept"
        );
    }

    #[test]
    fn a_memory_key_already_carried_takes_no_lock_on_the_next_run() {
        let home = crate::UzeHome::at(uze_testkit::temp::scratch("carry-memory-both").join("uze"));
        std::fs::create_dir_all(home.state_dir()).unwrap();
        let config = "[memory]\nharness = \"off\"\nnative = \"keep\"\n";
        std::fs::write(home.config_path(), config).unwrap();

        carry_all(&home);

        assert_eq!(std::fs::read_to_string(home.config_path()).unwrap(), config);
        assert!(
            !home.config_lock_path().exists(),
            "only a machine with the old key alone pays for the lock"
        );
    }

    #[test]
    fn a_config_that_does_not_parse_keeps_the_overrides_where_they_are() {
        let home =
            crate::UzeHome::at(uze_testkit::temp::scratch("carry-settings-broken").join("uze"));
        std::fs::create_dir_all(home.root()).unwrap();
        std::fs::write(home.retired_theme_overrides_path(), r#"{ "colors": {} }"#).unwrap();
        std::fs::write(home.config_path(), "[appearance\n").unwrap();

        carry_all(&home);

        assert!(home.retired_theme_overrides_path().exists());
        assert_eq!(
            std::fs::read_to_string(home.config_path()).unwrap(),
            "[appearance\n"
        );
    }
}
