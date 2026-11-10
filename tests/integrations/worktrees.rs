//! Where each harness keeps worktrees of its own, as the workspace accounts
//! a checkout: one a harness made is shown as that harness's, never as the
//! operator's, so it is neither driven nor removed.

use uze_core::UzeHome;
use uze_integrations::registry::IntegrationRegistry;
use uze_testkit::git::Repository;
use uze_workspace::checkout::{Owner, account};

/// Claude Code's `--worktree` isolation, and the worktree tool it gives
/// the model, put a checkout under `.claude/worktrees` of the checkout it
/// runs in.
#[test]
fn a_worktree_claude_code_made_is_accounted_to_claude_code() {
    let repository = Repository::new("claude-own-worktree");
    let primary = repository.root();
    repository.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "worktree-x",
        ".claude/worktrees/x",
        "HEAD",
    ]);
    let scratch = uze_testkit::temp::scratch("claude-own-worktree-registry");
    let registry = IntegrationRegistry::isolated(&scratch, &UzeHome::at(scratch.join("uze")));
    let places: Vec<_> = registry
        .iter()
        .flat_map(|integration| {
            integration
                .own_worktree_places()
                .into_iter()
                .map(|place| (integration.id(), place))
        })
        .collect();

    let owners: Vec<(Option<String>, Owner)> = account(primary, &places)
        .into_iter()
        .map(|checkout| (checkout.branch, checkout.owner))
        .collect();

    assert_eq!(
        owners,
        [(
            Some("worktree-x".to_owned()),
            Owner::Harness {
                harness: "claude-code".to_owned()
            }
        )]
    );
    let _ = std::fs::remove_dir_all(scratch);
}
