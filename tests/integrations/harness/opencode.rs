//! OpenCode invocation-policy conformance (ADR-030): what `autoinvoke`
//! carries, and that OpenCode V2 defines nothing for the person's half.

use crate::policy::*;

#[test]
fn opencode_routes_each_combination_by_what_the_vendor_carries() {
    let combinations = [
        (
            "default",
            default_body("commit"),
            CompatibilityRoute::Native,
        ),
        // `metadata.opencode/autoinvoke: false` withholds the Skill from
        // model discovery, which is the whole of invoke.model=false.
        (
            "user-only",
            user_only_body("review"),
            CompatibilityRoute::Native,
        ),
        // V2 removed `slash` (199aabe9e): `/skills` lists every Skill and a
        // mention (`@id`) expands any, so invoke.user=false is not carried
        // at all. Measured by the Lab's `skill-model-only-is-*`
        // declarations (opencode v2.0.23).
        (
            "model-only",
            model_only_body("legacy"),
            CompatibilityRoute::Degraded,
        ),
        (
            "invalid",
            invalid_body("dead"),
            CompatibilityRoute::Unsupported,
        ),
    ];
    for (label, body, expected) in combinations {
        let (root, home, _package, r) =
            make_policy_package(&format!("opencode-{label}"), "test", &body);
        let opencode = OpenCodeIntegration::new(
            root.join("agents"),
            root.join("config/opencode.json"),
            home.clone(),
        );
        mark_setup(&home, &opencode);
        let plan = opencode.exposure_plan(&r);
        assert_eq!(
            plan.route, expected,
            "OpenCode routes by what it carries ({label})"
        );
        if label == "model-only" {
            assert!(
                plan.evidence.contains("cannot be enforced"),
                "the degradation must be stated, never hidden: {}",
                plan.evidence
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn opencode_default_skill_wrapper_carries_the_qualified_label() {
    let (root, home, package, r) =
        make_policy_package("oc-default-label", "commit", &default_body("commit"));
    let store_bytes = fs::read(package.root.join("skills/commit/SKILL.md")).unwrap();
    let opencode = OpenCodeIntegration::new(
        root.join("agents"),
        root.join("config/opencode.json"),
        home.clone(),
    );
    mark_setup(&home, &opencode);

    let receipt = opencode
        .attach_receipt(&r)
        .unwrap()
        .expect("default Skill attaches on OpenCode");
    let ManagedArtifact::GeneratedTree { path: target, .. } = &receipt.artifact else {
        panic!("expected a materialized skill directory");
    };
    let wrapper = fs::read_to_string(target.join("SKILL.md")).unwrap();
    assert!(
        wrapper.starts_with("---\nname: flow:commit\n"),
        "the OpenCode-visible name stays qualified: {wrapper}"
    );
    assert_eq!(
        fs::read(package.root.join("skills/commit/SKILL.md")).unwrap(),
        store_bytes,
        "the canonical Store bytes stay untouched"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn opencode_user_only_wrapper_carries_autoinvoke_metadata() {
    let (root, home, package, r) =
        make_policy_package("oc-physical", "review", &user_only_body("review"));
    let store_bytes = fs::read(package.root.join("skills/review/SKILL.md")).unwrap();
    let opencode = OpenCodeIntegration::new(
        root.join("agents"),
        root.join("config/opencode.json"),
        home.clone(),
    );
    mark_setup(&home, &opencode);
    let receipt = opencode
        .attach_receipt(&r)
        .unwrap()
        .expect("user-only Skill attaches on OpenCode");
    let ManagedArtifact::GeneratedTree { path: target, .. } = &receipt.artifact else {
        panic!("expected a materialized skill directory");
    };
    let wrapper = fs::read_to_string(target.join("SKILL.md")).unwrap();
    assert!(
        wrapper.contains("metadata:\n  opencode/autoinvoke: false\n"),
        "model=false is translated into OpenCode's own control: {wrapper}"
    );
    assert!(
        !wrapper.contains("slash: false"),
        "user invocation stays enabled for a user-only Skill"
    );
    assert_eq!(
        fs::read(package.root.join("skills/review/SKILL.md")).unwrap(),
        store_bytes,
        "the canonical bytes stay untouched"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn opencode_model_only_wrapper_writes_no_field_v2_does_not_define() {
    let (root, home, _package, r) =
        make_policy_package("oc-model-only", "legacy", &model_only_body("legacy"));
    let opencode = OpenCodeIntegration::new(
        root.join("agents"),
        root.join("config/opencode.json"),
        home.clone(),
    );
    mark_setup(&home, &opencode);
    let receipt = opencode
        .attach_receipt(&r)
        .unwrap()
        .expect("model-only Skill attaches on OpenCode");
    let ManagedArtifact::GeneratedTree { path: target, .. } = &receipt.artifact else {
        panic!("expected a materialized skill directory");
    };
    let wrapper = fs::read_to_string(target.join("SKILL.md")).unwrap();
    assert!(
        !wrapper.contains("slash"),
        "V2 defines no `slash`, so the wrapper carries none: {wrapper}"
    );
    assert!(
        !wrapper.contains("opencode/autoinvoke"),
        "model discovery stays enabled for a model-only Skill"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn user_only_skill_lifecycle_attach_matched_detach_missing_on_opencode() {
    let (root, home, _package, r) =
        make_policy_package("oc-life", "review", &user_only_body("review"));
    let opencode = OpenCodeIntegration::new(
        root.join("agents"),
        root.join("config/opencode.json"),
        home.clone(),
    );
    mark_setup(&home, &opencode);
    let receipt = opencode.attach_receipt(&r).unwrap().expect("attaches");
    assert_eq!(
        opencode.inspect_receipt(&receipt).state,
        AttachmentState::Matched
    );
    assert_eq!(
        opencode.detach_receipt(&receipt).unwrap().state,
        AttachmentState::Missing
    );
    fs::remove_dir_all(root).unwrap();
}
