//! The operator's checkouts, against real Git.

use super::*;
use crate::UzeApplication;
use crate::application::services::PlacementKind;
use uze_core::UzeHome;
use uze_core::capability::Resource;
use uze_core::exposure::{ExposureMechanism, ExposurePlan};
use uze_core::integration::IntegrationPort;
use uze_core::path::Canonical as _;
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
    fn own_worktree_places(&self) -> Vec<uze_core::integration::WorktreePlace> {
        vec![uze_core::integration::WorktreePlace::InCheckout(
            ".keeper/worktrees",
        )]
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

fn same(left: &Path, right: &Path) -> bool {
    let canonical = |path: &Path| path.canonical().unwrap_or_else(|_| path.to_path_buf());
    canonical(left) == canonical(right)
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

    let view = app.workspace().checkouts(&root).unwrap();

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
    assert!(row(&view, &by_hand).dirty);
    assert_eq!(
        row(&view, &harness).owner,
        CheckoutOwner::Harness {
            harness: "Keeper".to_owned()
        }
    );
    assert_eq!(row(&view, &elsewhere).owner, CheckoutOwner::Operator);
    assert_eq!(row(&view, &elsewhere).ahead, 0, "its work is in the target");
    assert!(view.checkouts.iter().all(|checkout| checkout.bytes > 0));
    assert!(
        by_hand.join("draft.txt").is_file(),
        "reading changes nothing"
    );
}
