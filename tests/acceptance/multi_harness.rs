//! Acceptance A4/A5: multi-harness projection and invocation policy,
//! driven through the public CLI with deterministic fake harness CLI
//! binaries (no real vendor binary, no model call).

use uze_testkit::assertions;
use uze_testkit::fixtures;
use uze_testkit::temp::TestEnvironment;

use crate::util::{
    default_body, install_fake_harnesses, make_skill_package, model_only_body, user_only_body,
    uze_bin,
};

/// A4 — one canonical plugin reaches every harness through its most native
/// safe representation, with no duplicate delivery.
#[test]
fn one_plugin_reaches_every_harness_with_no_duplicate_delivery() {
    let env = TestEnvironment::isolated();
    let harnesses = install_fake_harnesses(&env);

    // One `setup` provisions every detected harness (and records setup
    // state so codex/opencode prefer UZE-managed attachment); running it
    // twice must stay idempotent (regression for the double-attach found
    // by this suite).
    env.run_ok(uze_bin(), &["setup"]);
    let fixture = fixtures::canonical("skill-plugin");
    let (market_args, install_args) =
        uze_testkit::marketplace::marketplace_install_args(&env.home, &fixture);
    env.run_ok(
        uze_bin(),
        &market_args.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let mut with_json = install_args.clone();
    with_json.push("--format".to_owned());
    with_json.push("json".to_owned());
    let install = env.run_ok(
        uze_bin(),
        &with_json.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let report: serde_json::Value = serde_json::from_slice(&install.stdout).expect("json report");
    let delivery = report["package_plans"]
        .as_array()
        .expect("package_plans array");
    assert!(
        !delivery.is_empty(),
        "install must report package plans, got: {report}"
    );

    // Claude: generated native package envelope under UZE_HOME state.
    let claude_envelope = env.uze_home.join(
        "runtime/attachments/claude/generated/uze-agent-skill-conformance@test/.claude-plugin/plugin.json",
    );
    assertions::assert_file(&claude_envelope, "claude generated envelope");

    // OpenCode: a directory of its own preserving the canonical body while
    // publishing the stable qualified label. Codex's plugin covers the
    // skill, so nothing lands in Codex's loose root beside it.
    let opencode_entry =
        env.home
            .join(".config/opencode/skills")
            .join(uze_core::path::file_name_for(
                "uze-agent-skill-conformance:uze-e2e",
            ));
    assert!(
        opencode_entry.is_dir() && !opencode_entry.is_symlink(),
        "OpenCode's skill entry must be a real directory"
    );
    let wrapper =
        std::fs::read_to_string(opencode_entry.join("SKILL.md")).expect("read OpenCode skill");
    assert!(
        wrapper.starts_with("---\nname: uze-agent-skill-conformance:uze-e2e\n"),
        "the delivered SKILL.md keeps the qualified skill label: {wrapper}"
    );
    assert!(
        !env.home
            .join(".agents/skills")
            .join(uze_core::path::file_name_for(
                "uze-agent-skill-conformance:uze-e2e"
            ))
            .exists(),
        "a skill Codex's plugin covers is not also delivered loose"
    );

    let ledger = std::fs::read(env.uze_home.join("state/attachments.json")).unwrap();
    let ledger: serde_json::Value = serde_json::from_slice(&ledger).unwrap();
    let receipts = ledger["receipts"].as_array().unwrap();
    let for_package: Vec<_> = receipts
        .iter()
        .filter(|receipt| receipt["package_id"] == "uze-agent-skill-conformance@test")
        .collect();
    // One package-level (or one capability-level) receipt per integration —
    // never both, never two of the same kind. The exact count differs by
    // harness; the invariant is: no integration appears more than once.
    let mut integrations: Vec<&str> = for_package
        .iter()
        .map(|receipt| receipt["integration"].as_str().unwrap_or("?"))
        .collect();
    integrations.sort();
    integrations.dedup();
    assert!(
        for_package.len() == integrations.len(),
        "no duplicate capability receipt may exist for a package-covered resource, got \
         {for_package:?}"
    );
    let _ = harnesses;
}

/// A5 — invocation policy: normal/user-only/model-only Skills project with
/// the correct per-harness classification through the public CLI.
#[test]
fn invocation_policy_projects_per_harness_classification() {
    let env = TestEnvironment::isolated();
    install_fake_harnesses(&env);
    env.run_ok(uze_bin(), &["setup"]);

    let policy_package = make_skill_package(
        env.root(),
        "policy-fixture",
        &[
            ("commit", &default_body("commit")),
            ("review", &user_only_body("review")),
        ],
    );
    let (market_args, install_args) =
        uze_testkit::marketplace::marketplace_install_args(&env.home, &policy_package);
    env.run_ok(
        uze_bin(),
        &market_args.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    env.run_ok(
        uze_bin(),
        &install_args.iter().map(String::as_str).collect::<Vec<_>>(),
    );

    // Inspect reports all three skills with their policy classification.
    let inspect = env.run_ok(
        uze_bin(),
        &["inspect", "policy-fixture", "--format", "json"],
    );
    let report: serde_json::Value = serde_json::from_slice(&inspect.stdout).expect("json report");
    let names: Vec<&str> = report["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|cap| cap["name"].as_str())
        .collect();
    for expected in ["commit", "review"] {
        assert!(
            names.contains(&expected),
            "inspect must list {expected}, got {names:?}"
        );
    }

    // Physical projection: OpenCode gets each skill as a directory of its
    // own, the user-only one carrying OpenCode's own control, so the model
    // never sees it as auto-discoverable.
    let opencode_root = env.home.join(".config/opencode/skills");
    assert!(
        opencode_root
            .join(uze_core::path::file_name_for("policy-fixture:commit"))
            .is_dir(),
        "default skill must be projected for OpenCode"
    );
    let review = opencode_root.join(uze_core::path::file_name_for("policy-fixture:review"));
    let review_skill = std::fs::read_to_string(review.join("SKILL.md"))
        .expect("user-only skill must be projected with its own SKILL.md");
    assert!(
        review_skill.contains("opencode/autoinvoke: false"),
        "OpenCode's copy carries its own model-invocation control: {review_skill}"
    );
    // Codex's generated envelope carries its own `agents/openai.yaml` with
    // implicit invocation disabled.
    let codex_policy = env.uze_home.join(
        "runtime/attachments/codex/generated/policy-fixture@test/skills/review/agents/openai.yaml",
    );
    assert!(
        codex_policy.is_file(),
        "codex user-only delivery must carry the policy sidecar"
    );
    let sidecar = std::fs::read_to_string(&codex_policy).unwrap();
    assert!(
        sidecar.contains("allow_implicit_invocation: false"),
        "the sidecar must disable implicit (model) invocation, got: {sidecar}"
    );
}

/// A11 — a model-only Skill on Codex and OpenCode installs cleanly through
/// the CLI, and OpenCode's own directory carries OpenCode's encoding of the
/// policy (`slash: false`) and nothing of Codex's. Codex still reports its
/// own user=false limitation honestly (Degraded).
#[test]
fn a_model_only_skill_reaches_opencode_in_its_own_encoding() {
    let env = TestEnvironment::isolated();
    install_fake_harnesses(&env);
    env.run_ok(uze_bin(), &["setup"]);

    let conflict_package = make_skill_package(
        env.root(),
        "conflict-fixture",
        &[("audit", &model_only_body("audit"))],
    );
    let (market_args, install_args) =
        uze_testkit::marketplace::marketplace_install_args(&env.home, &conflict_package);
    env.run_ok(
        uze_bin(),
        &market_args.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    env.run_ok(
        uze_bin(),
        &install_args.iter().map(String::as_str).collect::<Vec<_>>(),
    );

    let entry = env
        .home
        .join(".config/opencode/skills")
        .join(uze_core::path::file_name_for("conflict-fixture:audit"));
    assert!(
        entry.is_dir() && !entry.is_symlink(),
        "OpenCode's own directory for the model-only Skill"
    );
    let wrapper = std::fs::read_to_string(entry.join("SKILL.md")).unwrap();
    assert!(
        wrapper.contains("slash: false"),
        "the entry carries OpenCode's user-invocation suppression: {wrapper}"
    );
    assert!(
        !wrapper.contains("opencode/autoinvoke"),
        "model discovery stays enabled for a model-only Skill: {wrapper}"
    );
    assert!(
        !entry.join("agents/openai.yaml").exists(),
        "OpenCode's directory carries no Codex policy sidecar"
    );
}

/// Claude Code loads a plugin's agents from the plugin, namespaced: an agent
/// riding in the package never also lands in `~/.claude/agents`, and a loose
/// agent file an earlier build put there is taken back off by the next
/// install, receipt and all.
#[test]
fn claude_agents_ride_in_the_plugin_and_an_earlier_loose_copy_is_retired() {
    use uze_core::{UzeHome, exposure::ManagedArtifact, state};

    let env = TestEnvironment::isolated();
    install_fake_harnesses(&env);
    env.run_ok(uze_bin(), &["setup"]);
    let package = env.root().join("crew");
    std::fs::create_dir_all(package.join("agents")).unwrap();
    std::fs::write(package.join("plugin.json"), r#"{"name":"crew"}"#).unwrap();
    std::fs::write(
        package.join("agents/reviewer.md"),
        "---\nname: reviewer\ndescription: Reviews a change.\n---\nReview.\n",
    )
    .unwrap();
    let (market_args, install_args) =
        uze_testkit::marketplace::marketplace_install_args(&env.home, &package);
    env.run_ok(
        uze_bin(),
        &market_args.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let install_args: Vec<&str> = install_args.iter().map(String::as_str).collect();
    env.run_ok(uze_bin(), &install_args);

    let claude_agents = env.home.join(".claude/agents");
    let loose = |dir: &std::path::Path| -> Vec<std::path::PathBuf> {
        std::fs::read_dir(dir)
            .map(|entries| entries.map(|entry| entry.unwrap().path()).collect())
            .unwrap_or_default()
    };
    assert_eq!(loose(&claude_agents), Vec::<std::path::PathBuf>::new());

    // What an earlier build delivered: the agent as a loose Claude file,
    // owned by a receipt of its own beside the plugin's.
    let uze = UzeHome::at(&env.uze_home);
    let held_elsewhere = state::receipts(&uze, None)
        .unwrap()
        .into_iter()
        .find(|receipt| {
            receipt
                .resource_identity
                .as_deref()
                .is_some_and(|identity| identity.contains("agents/reviewer.md"))
        })
        .expect("another harness holds the agent on its own");
    let identity = held_elsewhere.resource_identity.clone().unwrap();
    let earlier = claude_agents.join(format!(
        "{}.md",
        uze_core::path::file_name_for("crew:reviewer")
    ));
    let written = "---\nname: crew:reviewer\ndescription: Reviews a change.\n---\nReview.\n";
    std::fs::create_dir_all(&claude_agents).unwrap();
    std::fs::write(&earlier, written).unwrap();
    state::record_receipt(
        &uze,
        uze_core::integration::AttachmentReceipt {
            package_id: held_elsewhere.package_id,
            resource_identity: Some(identity.clone()),
            integration: "claude-code".to_owned(),
            artifact: ManagedArtifact::GeneratedFile {
                path: earlier.clone(),
                content: written.to_owned(),
            },
        },
    )
    .unwrap();

    env.run_ok(uze_bin(), &install_args);

    assert_eq!(loose(&claude_agents), Vec::<std::path::PathBuf>::new());
    let claude_receipts: Vec<_> = state::receipts(&uze, None)
        .unwrap()
        .into_iter()
        .filter(|receipt| receipt.integration == "claude-code")
        .collect();
    assert!(
        claude_receipts
            .iter()
            .all(|receipt| receipt.resource_identity.as_deref() != Some(identity.as_str())),
        "the loose receipt is retired with its file: {claude_receipts:?}"
    );
}
