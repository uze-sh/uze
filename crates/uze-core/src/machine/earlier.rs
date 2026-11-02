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
    vec![
        Earlier {
            name: "runtime projection markers named project.json",
            // Replaced when the projection is next prepared: unbounded.
            lasts: Lasts::Unbounded,
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

/// Carries every record this build renamed (a retired owned path naming
/// the builder it became) to its current name: a project's when `project`
/// names one, the machine's otherwise. Renamed only where the current name
/// is free, so nothing is ever overwritten, and the shape is untouched; a
/// rename that fails leaves the bytes where they were. A few `stat`s once
/// carried, so it runs wherever the records' readers and writers start.
pub fn carry_renamed(home: &crate::UzeHome, project: Option<&str>) {
    if project.is_none() {
        carry_settings(home);
    }
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

/// Moves `was` to `now` only while nothing is at `now`: a link refuses a
/// taken name where a rename would replace it, so a record another process
/// wrote since the check is never overwritten by the earlier one.
fn move_unless_taken(was: &Path, now: &Path) -> std::io::Result<()> {
    std::fs::hard_link(was, now)?;
    std::fs::remove_file(was)
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
    // a machine still holding the old key pays for the write.
    if matches!(crate::config::get(home, "memory", "harness"), Ok(Some(_)))
        && let Err(error) = crate::config::rename_key(home, "memory", "harness", "native")
    {
        tracing::warn!(%error, "`[memory] harness` was not carried to `native`");
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

        carry_renamed(&home, None);
        carry_renamed(&home, None);

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
            carry_renamed(&home, None);
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
    fn a_config_that_does_not_parse_keeps_the_overrides_where_they_are() {
        let home =
            crate::UzeHome::at(uze_testkit::temp::scratch("carry-settings-broken").join("uze"));
        std::fs::create_dir_all(home.root()).unwrap();
        std::fs::write(home.retired_theme_overrides_path(), r#"{ "colors": {} }"#).unwrap();
        std::fs::write(home.config_path(), "[appearance\n").unwrap();

        carry_renamed(&home, None);

        assert!(home.retired_theme_overrides_path().exists());
        assert_eq!(
            std::fs::read_to_string(home.config_path()).unwrap(),
            "[appearance\n"
        );
    }
}
