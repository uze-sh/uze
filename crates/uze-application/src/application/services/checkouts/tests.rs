//! The operator's checkouts, against real Git.

use super::*;
use crate::UzeApplication;
use crate::application::services::PlacementKind;
use uze_core::UzeHome;
use uze_core::capability::Resource;
use uze_core::exposure::{ExposureMechanism, ExposurePlan};
use uze_core::integration::IntegrationPort;
use uze_core::router::{CompatibilityRoute, HarnessCapabilities};
use uze_testkit::git::Repository;

/// A harness that keeps worktrees of its own, the way an integration
/// declares it.
struct Keeper;

impl IntegrationPort for Keeper {
    fn id(&self) -> &'static str {
        "keeper"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["keeper"]
    }
    fn display_name(&self) -> &'static str {
        "Keeper"
    }
    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::default()
    }
    fn own_worktree_dirs(&self) -> &'static [&'static str] {
        &[".keeper/worktrees"]
    }
    fn exposure_plan(&self, _resource: &Resource) -> ExposurePlan {
        ExposurePlan {
            route: CompatibilityRoute::Adaptable,
            mechanism: ExposureMechanism::Unsupported {
                rationale: "test does not attach".to_owned(),
            },
            evidence: "test".to_owned(),
        }
    }
}

fn application(label: &str) -> UzeApplication {
    UzeApplication::new(
        UzeHome::at(uze_testkit::temp::scratch(label)),
        vec![Box::new(Keeper)],
    )
}

fn nobody() -> Presence {
    Presence::Known(Vec::new())
}

/// A worktree added with Git by hand, on a branch of its own, at `path`.
fn add_by_hand(repository: &Repository, path: &Path, branch: &str) -> PathBuf {
    repository.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        branch,
        &path.to_string_lossy(),
        "HEAD",
    ]);
    path.canonical().unwrap()
}

fn primary(repository: &Repository) -> PathBuf {
    repository.root().canonical().unwrap()
}

fn row<'a>(view: &'a CheckoutsView, path: &Path) -> &'a CheckoutView {
    view.checkouts
        .iter()
        .find(|checkout| same(&checkout.path, path))
        .unwrap_or_else(|| panic!("{} is listed: {view:?}", path.display()))
}

#[test]
fn every_checkout_is_listed_under_its_owner_with_its_facts() {
    let repository = Repository::new("checkouts-listed");
    let root = primary(&repository);
    let app = application("checkouts-listed-home");
    let slot = app
        .workspace()
        .place_new_agent(&root, Some(PlacementKind::Isolated), "keeper", &[])
        .unwrap()
        .cwd;
    let by_hand = add_by_hand(&repository, &root.join(".worktrees/by-hand"), "by-hand");
    let harness = add_by_hand(
        &repository,
        &root.join(".keeper/worktrees/own"),
        "keeper-own",
    );
    let elsewhere = add_by_hand(
        &repository,
        &uze_testkit::temp::scratch("checkouts-listed-elsewhere").join("tree"),
        "elsewhere",
    );
    std::fs::write(by_hand.join("draft.txt"), "unfinished").unwrap();

    let view = app.workspace().checkouts_seen(&root, &nobody()).unwrap();

    assert_eq!(view.checkouts.len(), 4, "{view:?}");
    assert!(matches!(
        row(&view, &slot).owner,
        CheckoutOwner::Agent {
            holder: Some(_),
            live: true,
        }
    ));
    assert!(
        row(&view, &slot).task.is_some(),
        "the slot names the task recorded in it, which kept work is matched by"
    );
    assert_eq!(row(&view, &by_hand).task, None);
    assert_eq!(row(&view, &by_hand).owner, CheckoutOwner::Operator);
    assert!(
        row(&view, &by_hand).adoptable,
        "it sits in the isolation directory"
    );
    assert!(row(&view, &by_hand).dirty);
    assert_eq!(
        row(&view, &by_hand).removal_refusal.as_deref(),
        Some("it holds uncommitted work")
    );
    assert_eq!(
        row(&view, &harness).owner,
        CheckoutOwner::Harness {
            harness: "Keeper".to_owned()
        }
    );
    assert_eq!(row(&view, &elsewhere).owner, CheckoutOwner::Operator);
    assert!(
        !row(&view, &elsewhere).adoptable,
        "only under the isolation directory"
    );
    assert!(row(&view, &elsewhere).in_target);
    assert_eq!(row(&view, &elsewhere).removal_refusal, None);
    assert!(view.checkouts.iter().all(|checkout| checkout.bytes > 0));
    assert_eq!(
        view.total_bytes,
        view.checkouts
            .iter()
            .map(|checkout| checkout.bytes)
            .sum::<u64>()
    );
    assert!(
        by_hand.join("draft.txt").is_file(),
        "reading changes nothing"
    );
}

#[test]
fn a_checkout_holding_uncommitted_work_is_not_removed() {
    let repository = Repository::new("checkouts-remove-dirty");
    let root = primary(&repository);
    let app = application("checkouts-remove-dirty-home");
    let path = add_by_hand(&repository, &root.join(".worktrees/dirty"), "dirty");
    std::fs::write(path.join("draft.txt"), "unfinished").unwrap();

    let refused = app
        .workspace()
        .remove_checkout_seen(&root, &path, &nobody())
        .unwrap_err();

    assert_eq!(refused.to_string(), "it holds uncommitted work");
    assert!(path.join("draft.txt").is_file());
}

#[test]
fn a_checkout_holding_commits_no_branch_reaches_is_not_removed() {
    let repository = Repository::new("checkouts-remove-detached");
    let root = primary(&repository);
    let app = application("checkouts-remove-detached-home");
    let path = add_by_hand(&repository, &root.join(".worktrees/detached"), "detached");
    repository.git_in(&path, &["switch", "-q", "--detach"]);
    std::fs::write(path.join("lost.txt"), "only here").unwrap();
    repository.git_in(&path, &["add", "."]);
    repository.git_in(&path, &["commit", "-qm", "only here"]);

    let refused = app
        .workspace()
        .remove_checkout_seen(&root, &path, &nobody())
        .unwrap_err();

    assert_eq!(refused.to_string(), "it holds commits no branch reaches");
    assert!(path.is_dir());
}

#[test]
fn a_checkout_somebody_is_working_in_is_not_removed() {
    let repository = Repository::new("checkouts-remove-in-use");
    let root = primary(&repository);
    let app = application("checkouts-remove-in-use-home");
    let path = add_by_hand(&repository, &root.join(".worktrees/busy"), "busy");

    let refused = app
        .workspace()
        .remove_checkout_seen(&root, &path, &Presence::Known(vec![path.join("src")]))
        .unwrap_err();

    assert_eq!(refused.to_string(), "a process is working inside it");
    assert!(path.is_dir());
}

#[test]
fn removing_a_checkout_keeps_its_branch_and_never_takes_the_primary() {
    let repository = Repository::new("checkouts-remove");
    let root = primary(&repository);
    let app = application("checkouts-remove-home");
    let path = add_by_hand(&repository, &root.join(".worktrees/done"), "done");

    let refused = app
        .workspace()
        .remove_checkout_seen(&root, &root, &nobody())
        .unwrap_err();
    assert_eq!(refused.to_string(), "it is the project's own checkout");

    let removed = app
        .workspace()
        .remove_checkout_seen(&root, &path, &nobody())
        .unwrap();

    assert_eq!(removed.name, ".worktrees/done");
    assert!(removed.bytes > 0);
    assert!(!path.exists());
    assert!(checkout::branch_exists(&root, "done"), "the branch is kept");
}

#[test]
fn a_harness_checkout_is_left_to_its_harness() {
    let repository = Repository::new("checkouts-remove-harness");
    let root = primary(&repository);
    let app = application("checkouts-remove-harness-home");
    let path = add_by_hand(&repository, &root.join(".keeper/worktrees/own"), "own");

    let refused = app
        .workspace()
        .remove_checkout_seen(&root, &path, &nobody())
        .unwrap_err();

    assert_eq!(refused.to_string(), "it is left to Keeper");
    assert!(path.is_dir());
}

#[test]
fn an_adopted_clean_checkout_is_the_next_agents_slot() {
    let repository = Repository::new("checkouts-adopt");
    let root = primary(&repository);
    let app = application("checkouts-adopt-home");
    let path = add_by_hand(&repository, &root.join(".worktrees/adoptee"), "adoptee");
    let elsewhere = add_by_hand(
        &repository,
        &uze_testkit::temp::scratch("checkouts-adopt-elsewhere").join("tree"),
        "elsewhere",
    );

    let refused = app
        .workspace()
        .adopt_checkout(&root, &elsewhere)
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        "only a checkout directly under .worktrees/ can be a slot"
    );

    let adopted = app.workspace().adopt_checkout(&root, &path).unwrap();
    assert!(adopted.free, "clean and level with the target");
    let view = app.workspace().checkouts_seen(&root, &nobody()).unwrap();
    assert!(matches!(
        row(&view, &path).owner,
        CheckoutOwner::Agent { holder: None, .. }
    ));

    let placed = app
        .workspace()
        .place_new_agent(&root, Some(PlacementKind::Isolated), "keeper", &[])
        .unwrap();
    assert_eq!(placed.cwd.canonical().unwrap(), path);
}

#[test]
fn cleaning_up_removes_only_what_is_done_and_says_why_it_kept_the_rest() {
    let repository = Repository::new("checkouts-clean-up");
    let root = primary(&repository);
    let app = application("checkouts-clean-up-home");
    let slot = app
        .workspace()
        .place_new_agent(&root, Some(PlacementKind::Isolated), "keeper", &[])
        .unwrap()
        .cwd;
    let done: Vec<PathBuf> = ["one", "two", "three"]
        .iter()
        .map(|name| add_by_hand(&repository, &root.join(".worktrees").join(name), name))
        .collect();
    let working = add_by_hand(&repository, &root.join(".worktrees/working"), "working");
    std::fs::write(working.join("draft.txt"), "unfinished").unwrap();
    let ahead = add_by_hand(&repository, &root.join(".worktrees/ahead"), "ahead");
    std::fs::write(ahead.join("feature.txt"), "not landed").unwrap();
    repository.git_in(&ahead, &["add", "."]);
    repository.git_in(&ahead, &["commit", "-qm", "not landed"]);
    let harness = add_by_hand(
        &repository,
        &root.join(".keeper/worktrees/own"),
        "keeper-own",
    );

    let clean_up = app.workspace().clean_up_seen(&root, &nobody());

    let mut removed: Vec<&str> = clean_up.removed.iter().map(|r| r.name.as_str()).collect();
    removed.sort_unstable();
    assert_eq!(
        removed,
        [".worktrees/one", ".worktrees/three", ".worktrees/two"]
    );
    assert!(done.iter().all(|path| !path.exists()));
    assert!(done.iter().all(|path| {
        checkout::branch_exists(&root, &path.file_name().unwrap().to_string_lossy())
    }));
    assert!(clean_up.freed() > 0);
    let kept: Vec<(&str, &str)> = clean_up
        .kept
        .iter()
        .map(|kept| (kept.name.as_str(), kept.reason.as_str()))
        .collect();
    assert!(
        kept.contains(&(".worktrees/working", "it holds uncommitted work")),
        "{kept:?}"
    );
    let not_landed = format!("its work is not in {}", repository.branch());
    assert!(
        kept.contains(&(".worktrees/ahead", not_landed.as_str())),
        "{kept:?}"
    );
    assert_eq!(clean_up.left_to_harness, [".keeper/worktrees/own"]);
    assert!(working.join("draft.txt").is_file());
    assert!(ahead.is_dir());
    assert!(harness.is_dir(), "a harness's own is left to its harness");
    assert!(slot.is_dir(), "UZE's own slots are the pool's, not this");
}
