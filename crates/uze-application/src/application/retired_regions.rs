//! The regions earlier releases wrote into a project's `AGENTS.md`, and
//! taking them back out.
//!
//! UZE writes nothing into a file the operator authors any more: the
//! workspace's instructions reach an agent through its launch context, and
//! the authoring verbs through the `uze:author` Skill. What is left is the
//! text an earlier release put there, which `uze install` and
//! `uze agent context reconcile` remove once, in a main working tree only.
//! In a linked worktree — a slot — the file belongs to the branch there, and every slot cut
//! from a target that still carries a region would otherwise get the same
//! removal to commit.
//!
//! A region is removed only while it still holds the bytes it was written
//! with. Its identity ends in a digest of them, so that is answerable with
//! no renderer left to ask; one edited by hand is reported and left alone.

use std::path::Path;

use uze_core::{
    Result, context as instruction_context, digest, integration::AttachmentState, retired_region,
    text_region,
};

use super::{RetiredRegionsPlan, RetiredRegionsStatus};

/// Whether `project_root` is where the cleanup runs: a main working tree,
/// whose `.git` is a directory. A linked worktree's is a file, and its
/// `AGENTS.md` belongs to the branch checked out there.
pub(super) fn applies_at(project_root: &Path) -> bool {
    project_root.join(".git").is_dir()
}

/// The retired regions in `agents_md`, each once, in file order.
pub(super) fn present(agents_md: &Path) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    text_region::region_identities_present(agents_md)
        .into_iter()
        .filter(|identity| retired_region::is_retired(identity) && seen.insert(identity.clone()))
        .collect()
}

/// Why a retired region is left where it is, or `None` when it still holds
/// what it was written with and may go.
fn reason_to_keep(agents_md: &Path, identity: &str) -> Option<String> {
    let Some(body) = text_region::region_body(agents_md, identity) else {
        return Some("its markers are malformed, so it is left as it is".to_owned());
    };
    let written = identity.rsplit('/').next().unwrap_or_default();
    (written != digest::short_hex(body.as_bytes()))
        .then(|| "it was edited by hand, so it is left as it is".to_owned())
}

/// What [`remove`] would do, without writing. `None` when there is nothing
/// retired in the file.
pub(super) fn plan(agents_md: &Path) -> Option<RetiredRegionsPlan> {
    let present = present(agents_md);
    if present.is_empty() {
        return None;
    }
    let (remove, keep) = present.into_iter().fold(
        (Vec::new(), Vec::new()),
        |(mut remove, mut keep), identity| {
            match reason_to_keep(agents_md, &identity) {
                Some(reason) => keep.push((identity, reason)),
                None => remove.push(identity),
            }
            (remove, keep)
        },
    );
    Some(RetiredRegionsPlan {
        file: agents_md.to_path_buf(),
        remove,
        keep,
    })
}

/// Removes every retired region that still holds what it was written with,
/// and says what it did. The caller holds the file's guard.
pub(super) fn remove(agents_md: &Path) -> Result<Option<RetiredRegionsStatus>> {
    let Some(plan) = plan(agents_md) else {
        return Ok(None);
    };
    let mut status = RetiredRegionsStatus {
        file: agents_md.to_path_buf(),
        removed: Vec::new(),
        kept: plan.keep,
    };
    for identity in plan.remove {
        let inspection = text_region::remove_unconditionally(agents_md, &identity)?;
        match inspection.state {
            AttachmentState::Missing | AttachmentState::Matched => status.removed.push(identity),
            _ => status.kept.push((identity, inspection.reason)),
        }
    }
    Ok(Some(status))
}

/// The action a managed region needs to reach its desired presence. Shared
/// by every region UZE still owns in a project's files — the bridge, and
/// package contributions until they retire — because the safety rules are
/// `text_region`'s, not each caller's; `subject` only names the region in a
/// blocked explanation.
pub(super) fn plan_action_for_region(
    needed: bool,
    state: AttachmentState,
    subject: &str,
) -> instruction_context::PlannedAction {
    use instruction_context::PlannedAction;
    match (needed, state) {
        (true, AttachmentState::Matched) | (false, AttachmentState::Missing) => {
            PlannedAction::NoChange
        }
        (true, AttachmentState::Missing) => PlannedAction::Attach,
        (false, AttachmentState::Matched) => PlannedAction::Remove,
        (_, AttachmentState::Drifted) => PlannedAction::Blocked(format!(
            "{subject} content differs from what UZE would write"
        )),
        (_, AttachmentState::Blocked) => {
            PlannedAction::Blocked(format!("{subject} region markers are malformed"))
        }
        (_, AttachmentState::Conflict) => {
            PlannedAction::Blocked(format!("{subject} region ownership is ambiguous"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn written(file: &Path, prefix: &str, body: &str) -> String {
        let identity = format!("{prefix}/{}", digest::short_hex(body.as_bytes()));
        text_region::attach(file, &identity, body).unwrap();
        identity
    }

    fn agents_md(label: &str) -> (PathBuf, PathBuf) {
        let dir = uze_testkit::temp::scratch(label);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("AGENTS.md");
        std::fs::write(&file, "# Mine\n\nWhat I wrote.\n").unwrap();
        (dir, file)
    }

    #[test]
    fn both_retired_regions_go_and_the_operators_text_stays_byte_exact() {
        let (dir, file) = agents_md("retired-regions-removed");
        written(
            &file,
            retired_region::WORKTREE_POLICY,
            "## Concurrent work isolation\n",
        );
        written(
            &file,
            retired_region::PLUGIN_AUTHORING,
            "## Authoring plugins\n",
        );

        let status = remove(&file).unwrap().unwrap();
        assert_eq!(status.removed.len(), 2);
        assert!(status.kept.is_empty());
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "# Mine\n\nWhat I wrote.\n"
        );
        assert!(
            remove(&file).unwrap().is_none(),
            "a second pass finds nothing"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_edited_region_is_kept_and_named() {
        let (dir, file) = agents_md("retired-regions-edited");
        let identity = written(
            &file,
            retired_region::WORKTREE_POLICY,
            "## Concurrent work isolation\n",
        );
        let text = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, text.replace("isolation", "isolation, my way")).unwrap();

        let status = remove(&file).unwrap().unwrap();
        assert!(status.removed.is_empty());
        assert_eq!(status.kept[0].0, identity);
        assert!(status.kept[0].1.contains("edited by hand"));
        assert!(std::fs::read_to_string(&file).unwrap().contains("my way"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_region_another_owner_keeps_is_not_retired() {
        let (dir, file) = agents_md("retired-regions-foreign");
        text_region::attach(&file, "package:greet:tools:instructions", "hello\n").unwrap();
        assert!(plan(&file).is_none());
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
        let _ = std::fs::remove_dir_all(root);
    }
}
