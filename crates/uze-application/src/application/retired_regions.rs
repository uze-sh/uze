//! The regions earlier releases wrote into a project's `AGENTS.md`, and
//! taking them back out.
//!
//! UZE writes nothing into a file the operator authors any more: the
//! workspace's instructions reach an agent through its launch context, the
//! authoring verbs through the `uze:author` Skill, and what a plugin tells
//! every agent through an `always_on` rule. What is left is the text an
//! earlier release put there, which `uze install` and
//! `uze agent context reconcile` remove once, in a main working tree only.
//! In a linked worktree — a slot — the file belongs to the branch there, and
//! every slot cut from a target that still carries a region would otherwise
//! get the same removal to commit.
//!
//! A `project:` region is removed only while it still holds the bytes it
//! was written with. Its identity ends in a digest of them, so that is
//! answerable with no renderer left to ask; one edited by hand is reported
//! and left alone. A package's region carries no digest, so it goes by its
//! shape alone, as an orphaned one always did. Once no package region is
//! left, the `@AGENTS.md` bridge an earlier release wrote into a harness's
//! own file goes too, and so does a file that held nothing else.
//!
//! This is the one production module that writes `AGENTS.md`, and it only
//! ever takes text out (`a_retired_region_cleanup_is_the_only_writer_of_agents_md`
//! in `tests/architecture/layering.rs`).

use std::path::{Path, PathBuf};

use uze_core::{
    Result, digest,
    integration::{AttachmentState, ContextDelivery},
    project_context::AGENTS_MD_FILE_NAME,
    retired_region::{self, INSTRUCTION_BRIDGE, INSTRUCTION_BRIDGE_CONTENT},
    text_region,
};

use super::{BridgeFate, RetiredBridge, RetiredRegionsPlan, RetiredRegionsStatus, UzeApplication};

/// The files the cleanup reads in one project: its `AGENTS.md`, and the
/// harnesses' own files an earlier release bridged it into.
pub(super) struct Files {
    pub(super) agents_md: PathBuf,
    pub(super) bridges: Vec<PathBuf>,
}

impl UzeApplication {
    /// The files [`plan`] and [`remove`] read at `project_root`. Every
    /// registered harness that declares a bridge file is named, detected
    /// or not: a leftover bridge is in the project whether or not the
    /// harness is still on this machine.
    pub(super) fn retired_region_files(&self, project_root: &Path) -> Files {
        let mut bridges: Vec<PathBuf> = self
            .integrations
            .iter()
            .filter_map(|integration| match integration.context_delivery() {
                ContextDelivery::Bridge { file_name } => Some(project_root.join(file_name)),
                _ => None,
            })
            .collect();
        bridges.sort();
        bridges.dedup();
        Files {
            agents_md: project_root.join(AGENTS_MD_FILE_NAME),
            bridges,
        }
    }
}

/// Whether `project_root` is where the cleanup runs: anywhere but inside a
/// linked worktree, whose `AGENTS.md` belongs to the branch checked out
/// there. Read from the nearest `.git` at or above the project, since a
/// project may sit below its repository's root: a directory is a main
/// working tree, and a file is one too unless it points into a repository's
/// `worktrees/` (a submodule and `--separate-git-dir` point elsewhere). A
/// project in no repository has no slots to protect.
pub(super) fn applies_at(project_root: &Path) -> bool {
    let Some(dot_git) = project_root
        .ancestors()
        .map(|ancestor| ancestor.join(".git"))
        .find(|candidate| candidate.symlink_metadata().is_ok())
    else {
        return true;
    };
    if dot_git.is_dir() {
        return true;
    }
    let pointer = std::fs::read_to_string(&dot_git).unwrap_or_default();
    let written = Path::new(
        pointer
            .trim()
            .strip_prefix("gitdir:")
            .unwrap_or_default()
            .trim(),
    );
    let gitdir = dot_git
        .parent()
        .map_or_else(|| written.to_path_buf(), |dir| dir.join(written));
    // Git's own mark of a linked worktree's administrative directory is its
    // `commondir` file; the `worktrees/` it sits in is read only when the
    // directory is not there to ask.
    if gitdir.is_dir() {
        return !gitdir.join("commondir").is_file();
    }
    !written
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "worktrees")
}

fn is_retired(identity: &str) -> bool {
    retired_region::is_retired(identity) || retired_region::is_retired_package(identity)
}

/// The retired regions in `agents_md`, each once, in file order.
fn present(agents_md: &Path) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    text_region::region_identities_present(agents_md)
        .into_iter()
        .filter(|identity| is_retired(identity) && seen.insert(identity.clone()))
        .collect()
}

/// Why a retired region is left where it is, or `None` when it may go: a
/// package region whenever its markers are whole, a `project:` one only
/// while it still holds what it was written with.
fn reason_to_keep(agents_md: &Path, identity: &str) -> Option<String> {
    let malformed = || Some("its markers are malformed, so it is left as it is".to_owned());
    if retired_region::is_retired_package(identity) {
        return match text_region::region_shape(agents_md, identity) {
            text_region::RegionShape::WellFormed => None,
            _ => malformed(),
        };
    }
    let Some(body) = text_region::region_body(agents_md, identity) else {
        return malformed();
    };
    let written = identity.rsplit('/').next().unwrap_or_default();
    (written != digest::short_hex(body.as_bytes()))
        .then(|| "it was edited by hand, so it is left as it is".to_owned())
}

/// What becomes of the bridge in `file` once `AGENTS.md` carries no package
/// region, or `None` when the file holds no bridge.
fn bridge_fate(file: &Path) -> Option<BridgeFate> {
    let inspection = text_region::inspect(file, INSTRUCTION_BRIDGE, INSTRUCTION_BRIDGE_CONTENT);
    match inspection.state {
        AttachmentState::Missing => None,
        AttachmentState::Matched => Some(BridgeFate::Region),
        _ => Some(BridgeFate::Kept(inspection.reason)),
    }
}

/// What [`remove`] would do, without writing. `None` when nothing retired
/// is left in any of `files`.
pub(super) fn plan(files: &Files) -> Option<RetiredRegionsPlan> {
    let (remove, keep) = present(&files.agents_md).into_iter().fold(
        (Vec::new(), Vec::new()),
        |(mut remove, mut keep), identity| {
            match reason_to_keep(&files.agents_md, &identity) {
                Some(reason) => keep.push((identity, reason)),
                None => remove.push(identity),
            }
            (remove, keep)
        },
    );
    let a_package_region_stays = keep
        .iter()
        .any(|(identity, _)| retired_region::is_retired_package(identity));
    let bridges: Vec<RetiredBridge> = if a_package_region_stays {
        Vec::new()
    } else {
        files
            .bridges
            .iter()
            .filter_map(|file| {
                bridge_fate(file).map(|fate| RetiredBridge {
                    file: file.clone(),
                    fate,
                })
            })
            .collect()
    };
    if remove.is_empty() && keep.is_empty() && bridges.is_empty() {
        return None;
    }
    Some(RetiredRegionsPlan {
        file: files.agents_md.clone(),
        remove,
        keep,
        bridges,
    })
}

/// Removes every retired region [`plan`] lets go, and says what it did.
/// The caller holds `AGENTS.md`'s guard.
pub(super) fn remove(files: &Files) -> Result<Option<RetiredRegionsStatus>> {
    let Some(plan) = plan(files) else {
        return Ok(None);
    };
    let mut status = RetiredRegionsStatus {
        file: plan.file.clone(),
        removed: Vec::new(),
        kept: plan.keep,
        bridges: Vec::new(),
    };
    for identity in plan.remove {
        let inspection = text_region::remove_unconditionally(&plan.file, &identity)?;
        match inspection.state {
            AttachmentState::Missing | AttachmentState::Matched => status.removed.push(identity),
            _ => status.kept.push((identity, inspection.reason)),
        }
    }
    for bridge in plan.bridges {
        let fate = match bridge.fate {
            BridgeFate::Region => remove_bridge(&bridge.file)?,
            kept => kept,
        };
        status.bridges.push(RetiredBridge {
            file: bridge.file,
            fate,
        });
    }
    Ok(Some(status))
}

/// Takes the bridge out of `file`, and the file with it when nothing else
/// was in it: a file UZE created to hold one import is UZE's to take back.
fn remove_bridge(file: &Path) -> Result<BridgeFate> {
    let inspection = text_region::detach(file, INSTRUCTION_BRIDGE, INSTRUCTION_BRIDGE_CONTENT)?;
    if inspection.state != AttachmentState::Missing {
        return Ok(BridgeFate::Kept(inspection.reason));
    }
    if !text_region::has_content_outside_managed_regions(file) {
        std::fs::remove_file(file).map_err(|source| uze_core::UzeError::Write {
            path: file.to_path_buf(),
            source,
        })?;
        return Ok(BridgeFate::File);
    }
    Ok(BridgeFate::Region)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(file: &Path, prefix: &str, body: &str) -> String {
        let identity = format!("{prefix}/{}", digest::short_hex(body.as_bytes()));
        text_region::attach(file, &identity, body).unwrap();
        identity
    }

    /// A project whose `AGENTS.md` holds the operator's text, and whose
    /// `CLAUDE.md` is where an earlier release would have bridged it.
    fn project(label: &str) -> (PathBuf, Files) {
        let dir = uze_testkit::temp::scratch(label);
        std::fs::create_dir_all(&dir).unwrap();
        let files = Files {
            agents_md: dir.join("AGENTS.md"),
            bridges: vec![dir.join("CLAUDE.md")],
        };
        std::fs::write(&files.agents_md, "# Mine\n\nWhat I wrote.\n").unwrap();
        (dir, files)
    }

    fn bridged(files: &Files) -> &Path {
        let claude_md = &files.bridges[0];
        text_region::attach(claude_md, INSTRUCTION_BRIDGE, INSTRUCTION_BRIDGE_CONTENT).unwrap();
        claude_md
    }

    #[test]
    fn both_retired_regions_go_and_the_operators_text_stays_byte_exact() {
        let (dir, files) = project("retired-regions-removed");
        written(
            &files.agents_md,
            retired_region::WORKTREE_POLICY,
            "## Concurrent work isolation\n",
        );
        written(
            &files.agents_md,
            retired_region::PLUGIN_AUTHORING,
            "## Authoring plugins\n",
        );

        let status = remove(&files).unwrap().unwrap();
        assert_eq!(status.removed.len(), 2);
        assert!(status.kept.is_empty());
        assert_eq!(
            std::fs::read_to_string(&files.agents_md).unwrap(),
            "# Mine\n\nWhat I wrote.\n"
        );
        assert!(
            remove(&files).unwrap().is_none(),
            "a second pass finds nothing"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_edited_region_is_kept_and_named() {
        let (dir, files) = project("retired-regions-edited");
        let identity = written(
            &files.agents_md,
            retired_region::WORKTREE_POLICY,
            "## Concurrent work isolation\n",
        );
        let text = std::fs::read_to_string(&files.agents_md).unwrap();
        std::fs::write(
            &files.agents_md,
            text.replace("isolation", "isolation, my way"),
        )
        .unwrap();

        let status = remove(&files).unwrap().unwrap();
        assert!(status.removed.is_empty());
        assert_eq!(status.kept[0].0, identity);
        assert!(status.kept[0].1.contains("edited by hand"));
        assert!(
            std::fs::read_to_string(&files.agents_md)
                .unwrap()
                .contains("my way")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_package_region_goes_by_its_shape_whatever_it_holds() {
        let (dir, files) = project("retired-regions-package");
        let identity = "package:greet:tools:instructions";
        text_region::attach(&files.agents_md, identity, "hello\n").unwrap();
        let text = std::fs::read_to_string(&files.agents_md).unwrap();
        std::fs::write(&files.agents_md, text.replace("hello", "hello, edited")).unwrap();
        text_region::attach(&files.agents_md, "custom:mine", "keep me\n").unwrap();

        let status = remove(&files).unwrap().unwrap();
        assert_eq!(status.removed, vec![identity.to_owned()]);
        let left = std::fs::read_to_string(&files.agents_md).unwrap();
        assert!(!left.contains("hello"), "{left}");
        assert!(left.starts_with("# Mine\n\nWhat I wrote.\n"), "{left}");
        assert!(
            left.contains("keep me"),
            "a region no release of UZE wrote is not retired: {left}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_bridge_only_claude_md_goes_with_the_last_package_region() {
        let (dir, files) = project("retired-regions-bridge-only");
        text_region::attach(&files.agents_md, "package:greet:tools:instructions", "hi\n").unwrap();
        let claude_md = bridged(&files).to_path_buf();

        let planned = plan(&files).unwrap();
        assert_eq!(planned.bridges[0].fate, BridgeFate::Region);
        let status = remove(&files).unwrap().unwrap();
        assert_eq!(status.bridges[0].fate, BridgeFate::File);
        assert!(
            !claude_md.exists(),
            "a CLAUDE.md that held only the bridge goes"
        );
        assert_eq!(
            std::fs::read_to_string(&files.agents_md).unwrap(),
            "# Mine\n\nWhat I wrote.\n"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_operators_lines_in_claude_md_stay_when_the_bridge_goes() {
        let (dir, files) = project("retired-regions-bridge-shared");
        let claude_md = files.bridges[0].clone();
        std::fs::write(&claude_md, "# Claude notes\n\nPrefer small diffs.\n").unwrap();
        bridged(&files);

        let status = remove(&files).unwrap().unwrap();
        assert_eq!(status.bridges[0].fate, BridgeFate::Region);
        assert_eq!(
            std::fs::read_to_string(&claude_md).unwrap(),
            "# Claude notes\n\nPrefer small diffs.\n"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_bridge_stays_while_a_package_region_cannot_be_removed() {
        let (dir, files) = project("retired-regions-bridge-held");
        let identity = "package:greet:tools:instructions";
        std::fs::write(
            &files.agents_md,
            format!(
                "<!-- uze:begin {identity} -->\na\n<!-- uze:end {identity} -->\n\
                 <!-- uze:begin {identity} -->\nb\n<!-- uze:end {identity} -->\n"
            ),
        )
        .unwrap();
        let claude_md = bridged(&files).to_path_buf();

        let status = remove(&files).unwrap().unwrap();
        assert!(status.removed.is_empty());
        assert_eq!(status.kept[0].0, identity);
        assert!(status.bridges.is_empty());
        assert!(claude_md.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_edited_bridge_is_kept_and_named() {
        let (dir, files) = project("retired-regions-bridge-edited");
        let claude_md = bridged(&files).to_path_buf();
        let text = std::fs::read_to_string(&claude_md).unwrap();
        std::fs::write(
            &claude_md,
            text.replace("@AGENTS.md", "@AGENTS.md\n@docs/more.md"),
        )
        .unwrap();

        let status = remove(&files).unwrap().unwrap();
        assert!(matches!(status.bridges[0].fate, BridgeFate::Kept(_)));
        assert!(
            std::fs::read_to_string(&claude_md)
                .unwrap()
                .contains("@docs/more.md")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_project_with_nothing_retired_has_no_plan_and_gets_no_files() {
        let dir = uze_testkit::temp::scratch("retired-regions-none");
        std::fs::create_dir_all(&dir).unwrap();
        let files = Files {
            agents_md: dir.join("AGENTS.md"),
            bridges: vec![dir.join("CLAUDE.md")],
        };
        assert!(plan(&files).is_none());
        assert!(remove(&files).unwrap().is_none());
        assert!(!files.agents_md.exists());
        assert!(!files.bridges[0].exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_linked_worktree_is_never_cleaned() {
        let root = uze_testkit::temp::scratch("retired-regions-where");
        let primary = root.join("repo");
        let slot = primary.join(".worktrees").join("ab12cd");
        std::fs::create_dir_all(primary.join(".git")).unwrap();
        std::fs::create_dir_all(&slot).unwrap();
        std::fs::write(slot.join(".git"), "gitdir: ../../.git/worktrees/ab12cd\n").unwrap();
        assert!(applies_at(&primary));
        assert!(!applies_at(&slot));
        std::fs::create_dir_all(slot.join("svc")).unwrap();
        assert!(
            !applies_at(&slot.join("svc")),
            "a project below a slot's root is the slot's"
        );

        // A project below its repository's root, and a submodule's checkout,
        // are main working trees.
        std::fs::create_dir_all(primary.join("svc")).unwrap();
        assert!(applies_at(&primary.join("svc")));
        let submodule = primary.join("vendor").join("lib");
        std::fs::create_dir_all(&submodule).unwrap();
        std::fs::write(submodule.join(".git"), "gitdir: ../../.git/modules/lib\n").unwrap();
        assert!(applies_at(&submodule));

        // A separate git dir that happens to sit in a `worktrees/` is a main
        // working tree: it has no `commondir`. A real linked worktree's
        // administrative directory does.
        let separate = root.join("worktrees").join("repo.git");
        std::fs::create_dir_all(&separate).unwrap();
        let main = root.join("main");
        std::fs::create_dir_all(&main).unwrap();
        std::fs::write(
            main.join(".git"),
            format!("gitdir: {}\n", separate.display()),
        )
        .unwrap();
        assert!(applies_at(&main));
        let admin = primary.join(".git").join("worktrees").join("real");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::write(admin.join("commondir"), "../..\n").unwrap();
        let linked = root.join("linked");
        std::fs::create_dir_all(&linked).unwrap();
        std::fs::write(
            linked.join(".git"),
            format!("gitdir: {}\n", admin.display()),
        )
        .unwrap();
        assert!(!applies_at(&linked));
        let _ = std::fs::remove_dir_all(root);
    }
}
