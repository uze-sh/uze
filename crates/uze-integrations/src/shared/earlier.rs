//! What every integration does only because an earlier release left it
//! in a harness's own files (the `earlier-releases` rule): the marketplace
//! earlier releases registered beside the vendor store, the runtime tree
//! they staged harness content under, and the Skill wrappers generated
//! there. Each is carried out where it always ran — attach, detach and
//! publication — reading what it handles from here.

use std::path::{Path, PathBuf};

use uze_core::{
    Result, UzeError,
    document::earlier::{Earlier, Lasts},
    home::UzeHome,
    integration::{AttachmentReceipt, ManagedArtifact},
};

/// The marketplace earlier releases registered beside the vendor store for
/// packages shipping their own envelope. Read only to retire what they
/// installed through it.
pub(crate) const RETIRED_MARKETPLACE: &str = "uze-local";

/// Every integration's shared entries. Each is retired only when a package
/// it concerns is next delivered or detached, so none is bounded yet.
pub(crate) fn earlier() -> Vec<Earlier> {
    vec![
        Earlier {
            name: "the marketplace earlier releases registered beside the vendor store",
            lasts: Lasts::Unbounded,
            spelled: &[RETIRED_MARKETPLACE],
            site: file!(),
        },
        Earlier {
            name: "harness content earlier releases staged under the runtime tree",
            lasts: Lasts::Unbounded,
            spelled: &[],
            site: file!(),
        },
        Earlier {
            name: "Skill wrappers earlier releases generated under the runtime tree",
            lasts: Lasts::Unbounded,
            spelled: &[],
            site: file!(),
        },
    ]
}

/// Removes what an earlier release generated for one package under the
/// runtime tree, once the receipt naming it goes. Anything outside that
/// tree is a build, and its own receipt's.
pub(crate) fn remove_runtime_leftover(uze_home: &UzeHome, earlier: &Path) -> Result<()> {
    if earlier.starts_with(uze_home.runtime_attachments_dir()) && earlier.is_dir() {
        std::fs::remove_dir_all(earlier).map_err(UzeError::write(earlier))?;
    }
    Ok(())
}

/// Whether a receipt other than `leaving` still installed something through
/// [`RETIRED_MARKETPLACE`] on the same harness.
pub(crate) fn names_retired_marketplace(
    uze_home: &UzeHome,
    leaving: &AttachmentReceipt,
) -> Result<bool> {
    let suffix = format!("@{RETIRED_MARKETPLACE}");
    Ok(uze_core::state::receipts(uze_home, None)?
        .iter()
        .filter(|receipt| receipt.integration == leaving.integration && *receipt != leaving)
        .any(|receipt| {
            matches!(&receipt.artifact, ManagedArtifact::IntegrationOwned { selector, .. }
                if selector.ends_with(&suffix))
        }))
}

/// Where earlier releases staged what one harness read (`skills`,
/// `plugins`, `generated`, `rules`), under the retired runtime
/// attachments tree.
pub(crate) fn attachment_root(uze_home: &UzeHome, vendor: &str) -> PathBuf {
    uze_home.generated_attachments_dir(vendor)
}

/// Removes every directory under a harness's [`attachment_root`] that
/// nothing names any more: what an earlier layout staged there (its
/// marketplaces, rules, hook groups, plugins) stays behind otherwise,
/// since only the delivery that wrote it knew it. A child is kept while a
/// receipt names a path inside it, while `registered` (asked at most once,
/// and only when something would go) answers a root inside it the harness
/// still reads, and when its name starts with `.`, the staging a
/// replacement swaps in. Idempotent: what is left is exactly what is named.
pub(crate) fn retire_unnamed_attachments(
    uze_home: &UzeHome,
    vendor: &str,
    registered: impl FnOnce() -> Vec<PathBuf>,
) -> Result<()> {
    let root = attachment_root(uze_home, vendor);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Ok(());
    };
    let receipts = serde_json::to_string(&uze_core::state::receipts(uze_home, None)?)
        .expect("receipts serialize");
    let mut registered = Some(registered);
    let mut roots: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let child = entry.path();
        let staging = entry.file_name().to_string_lossy().starts_with('.');
        if staging || !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let named = serde_json::to_string(&child.display().to_string()).expect("a path serializes");
        if receipts.contains(named.trim_matches('"')) {
            continue;
        }
        if let Some(ask) = registered.take() {
            roots = ask();
        }
        if roots
            .iter()
            .any(|registered| registered.starts_with(&child))
        {
            continue;
        }
        std::fs::remove_dir_all(&child).map_err(UzeError::write(&child))?;
    }
    Ok(())
}

/// The root an earlier build generated `vendor`'s Skill wrappers under,
/// which an entry detached from that build may still point into.
pub(crate) fn skill_wrapper_root(uze_home: &UzeHome, vendor: &str) -> PathBuf {
    attachment_root(uze_home, vendor).join("skills")
}

/// Removes a generated wrapper directory once nothing in `skills_dir`
/// links to it any more, then prunes the package directory it leaves
/// empty. Only ever touches UZE-owned directories under `managed_root`.
///
/// `is_uze_wrapper` is the harness's own proof that `target` really is the
/// artifact it generated — the one thing that differs between harnesses
/// here, since only Claude's shim carries a plugin manifest beside its
/// `SKILL.md`.
pub(crate) fn cleanup_unused_wrapper(
    target: &Path,
    managed_root: &Path,
    skills_dir: &Path,
    prune_root: &Path,
    is_uze_wrapper: &dyn Fn(&Path) -> bool,
) -> Result<()> {
    // `starts_with` compares components, not resolved paths: a `..` after
    // the root would pass it and point `remove_dir_all` outside.
    let escapes = target
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir));
    if escapes || !target.starts_with(managed_root) || !target.is_dir() {
        return Ok(());
    }
    let referenced = std::fs::read_dir(skills_dir)
        .map_err(UzeError::read(skills_dir))?
        .filter_map(std::result::Result::ok)
        .any(|entry| std::fs::read_link(entry.path()).ok().as_deref() == Some(target));
    if referenced || !is_uze_wrapper(target) {
        return Ok(());
    }
    std::fs::remove_dir_all(target).map_err(UzeError::write(target))?;
    crate::shared::path::prune_empty_package_dir(target, prune_root);
    Ok(())
}

#[cfg(test)]
mod cleanup_unused_wrapper_tests {
    use super::*;

    #[test]
    fn a_target_that_climbs_out_of_the_managed_root_is_never_removed() {
        let root = uze_testkit::temp::scratch("cleanup-escape");
        let managed = root.join("managed");
        let outside = root.join("outside");
        let skills = root.join("skills");
        for dir in [&managed, &outside, &skills] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let climbing = managed.join("..").join("outside");
        assert!(
            climbing.starts_with(&managed),
            "the lexical check alone lets it through"
        );

        cleanup_unused_wrapper(&climbing, &managed, &skills, &managed, &|_| true).unwrap();
        assert!(outside.is_dir());

        let owned = managed.join("wrapper");
        std::fs::create_dir_all(&owned).unwrap();
        cleanup_unused_wrapper(&owned, &managed, &skills, &managed, &|_| true).unwrap();
        assert!(
            !owned.exists(),
            "an unreferenced wrapper inside the root still goes"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod retire_unnamed_attachments_tests {
    use std::fs;

    use uze_core::{
        home::UzeHome,
        integration::{AttachmentReceipt, ManagedArtifact},
    };

    use super::{attachment_root, retire_unnamed_attachments};

    fn naming(home: &UzeHome, package_id: &str, path: &std::path::Path) {
        uze_core::state::record_receipt(
            home,
            AttachmentReceipt {
                package_id: package_id.to_owned(),
                resource_identity: None,
                integration: "opencode".to_owned(),
                artifact: ManagedArtifact::SymlinkReference {
                    path: home.root().join("harness").join(package_id),
                    target: path.to_path_buf(),
                },
            },
        )
        .unwrap();
    }

    /// What an earlier layout staged per harness goes once nothing names
    /// it; what a receipt names, what the harness still registers and a
    /// swap in progress stay, and a second run changes nothing.
    #[test]
    fn only_what_nothing_names_leaves_the_staging_tree() {
        let scratch = uze_testkit::temp::TempDir::new("retire-unnamed");
        let home = UzeHome::at(scratch.path().join("uze"));
        let seed = |vendor: &str, name: &str| {
            let dir = attachment_root(&home, vendor).join(name);
            fs::create_dir_all(dir.join("inner")).unwrap();
            fs::write(dir.join("inner/file"), "x").unwrap();
            dir
        };
        let claude_rules = seed("claude", "rules");
        let codex_rules = seed("codex", "rules");
        let codex_generated = seed("codex", "generated");
        let hook_groups = seed("opencode", "hook-groups");
        let opencode_rules = seed("opencode", "rules");
        let opencode_skills = seed("opencode", "skills");
        let swapping = seed("opencode", ".skills.uze-staging-1-0");
        let plugins = seed("antigravity", "plugins");
        naming(&home, "flow@market", &opencode_skills.join("inner"));
        naming(&home, "kit@market", &opencode_rules.join("inner/file"));

        for _ in 0..2 {
            retire_unnamed_attachments(&home, "claude", Vec::new).unwrap();
            retire_unnamed_attachments(&home, "codex", || vec![codex_generated.clone()]).unwrap();
            retire_unnamed_attachments(&home, "opencode", Vec::new).unwrap();
            retire_unnamed_attachments(&home, "antigravity", Vec::new).unwrap();

            for gone in [&claude_rules, &codex_rules, &hook_groups, &plugins] {
                assert!(!gone.exists(), "{} is named by nothing", gone.display());
            }
            for kept in [
                &codex_generated,
                &opencode_rules,
                &opencode_skills,
                &swapping,
            ] {
                assert!(
                    kept.join("inner/file").is_file(),
                    "{} stays",
                    kept.display()
                );
            }
        }
    }
}
