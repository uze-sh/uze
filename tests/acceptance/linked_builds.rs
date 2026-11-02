//! Acceptance: a marketplace linked to a checkout keeps each build's
//! identity, and unlinking puts back the build of the commit pinned before
//! the link, from the marketplace's cached clone rather than its head.

use std::path::Path;

use uze_testkit::fixtures;
use uze_testkit::temp::TestEnvironment;

use crate::util::{install_fake_harnesses, uze_bin};

/// What Claude Code's build of `store-tie@test` records: where its bytes
/// came from (`pinned` or `linked`) and the build directory.
fn claude_build(env: &TestEnvironment) -> (String, String) {
    let ledger: serde_json::Value = serde_json::from_slice(
        &std::fs::read(env.uze_home.join("state/attachments.json")).unwrap(),
    )
    .unwrap();
    let receipt = ledger["receipts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|receipt| {
            receipt["package_id"] == "store-tie@test"
                && receipt["integration"] == "claude-code"
                && receipt["resource_identity"] == "build"
        })
        .expect("Claude Code holds store-tie's build")
        .clone();
    let detail = &receipt["artifact"]["INTEGRATION_OWNED"];
    (
        detail["source"].as_str().unwrap().to_owned(),
        detail["root"].as_str().unwrap().to_owned(),
    )
}

fn guide(build: &str) -> String {
    std::fs::read_to_string(Path::new(build).join("guides/locate.md")).unwrap()
}

#[test]
fn a_linked_build_is_rendered_from_the_checkout_and_unlinking_puts_the_pinned_one_back() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    env.run(uze_bin(), &["setup"]);
    let (market, add) = uze_testkit::marketplace::marketplace_add_args(
        env.root(),
        &fixtures::canonical("store-tie"),
    );
    env.run_ok(
        uze_bin(),
        &market.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    env.run_ok(
        uze_bin(),
        &add.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let checkout = env.root().join("market");
    let (source, build) = claude_build(&env);
    assert_eq!(source, "pinned");
    let pinned = guide(&build);

    env.run_ok(
        uze_bin(),
        &["market", "link", "test", &checkout.to_string_lossy()],
    );
    std::fs::write(
        checkout.join("plugins/store-tie/guides/locate.md"),
        "# Locate\n\nEdited in the checkout, never committed.\n",
    )
    .unwrap();
    env.run_ok(uze_bin(), &["update", "-m"]);

    let (source, linked_build) = claude_build(&env);
    assert_eq!(source, "linked", "the build says it came from the checkout");
    assert_eq!(linked_build, build, "the build kept its place");
    assert!(guide(&build).contains("Edited in the checkout"));

    env.run_ok(uze_bin(), &["market", "unlink", "test"]);
    assert_eq!(
        claude_build(&env).0,
        "linked",
        "unlinking leaves the bytes alone until something asks for them"
    );

    // The edit is committed since: what comes back is the commit pinned
    // before the link, not the marketplace's head.
    uze_testkit::git::commit_everything_in(&checkout);
    env.run_ok(uze_bin(), &["update", "-m"]);

    let (source, restored_build) = claude_build(&env);
    assert_eq!(source, "pinned", "the build is the pinned commit's again");
    assert_eq!(restored_build, build);
    assert_eq!(guide(&build), pinned);
}
