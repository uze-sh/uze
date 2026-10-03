//! Canonical Agent capability routes remain explicit across every harness.

use uze_core::{
    capability::Resource,
    capability::{Capability, CapabilityKind},
    home::UzeHome,
    integration::IntegrationPort,
    router::CompatibilityRoute,
    store::PackageId,
};
use uze_integrations::{
    antigravity::AntigravityIntegration, claude::ClaudeIntegration, codex::CodexIntegration,
    opencode::OpenCodeIntegration,
};

fn agent(root: &std::path::Path) -> Resource {
    let package_root = root.join("store/flow");
    let id = PackageId::from_plugin_name("flow", &package_root.join("plugin.json")).unwrap();
    Resource::from_package(
        id,
        package_root.clone(),
        Capability {
            kind: CapabilityKind::Agent,
            path: package_root.join("agents/reviewer.md"),
            payload: b"---\nname: reviewer\n---\nReview.\n".to_vec(),
        },
    )
}

#[test]
fn canonical_agent_routes_natively_for_every_harness() {
    let root = uze_testkit::temp::scratch("agent-routes");
    let home = UzeHome::at(root.join("uze"));
    let resource = agent(&root);
    let claude = ClaudeIntegration::new(root.join("claude"), home.clone());
    let codex = CodexIntegration::new(root.join("agents"), home.clone());
    let opencode = OpenCodeIntegration::new(
        root.join("agents"),
        root.join("config/opencode.json"),
        home.clone(),
    );
    let antigravity = AntigravityIntegration::new(root.join("agents"), home);

    assert_eq!(
        claude.exposure_plan(&resource).route,
        CompatibilityRoute::Native
    );
    assert_eq!(
        opencode.exposure_plan(&resource).route,
        CompatibilityRoute::Native
    );
    assert_eq!(
        antigravity.exposure_plan(&resource).route,
        CompatibilityRoute::Native
    );
    assert_eq!(
        codex.exposure_plan(&resource).route,
        CompatibilityRoute::Native
    );
}

#[test]
fn codex_generates_the_documented_custom_agent_toml_before_exposure() {
    let root = uze_testkit::temp::scratch("codex-agent");
    let home = UzeHome::at(root.join("uze"));
    let codex = CodexIntegration::new(root.join("home/.agents"), home);
    let resource = agent(&root);

    let receipt = codex
        .attach_receipt(&resource)
        .expect("Codex agent attachment succeeds")
        .expect("Codex agent attachment has a receipt");
    let uze_core::integration::ManagedArtifact::GeneratedFile { path, content } = receipt.artifact
    else {
        panic!("Codex native agent is a receipt-owned file");
    };
    // Codex offers a role to the model by its TOML `name`, which is the
    // agent's plugin-qualified label, and runs only a regular file: a
    // linked one is listed and refused.
    assert_eq!(path, root.join("home/.codex/agents/flow:reviewer.toml"));
    assert!(path.is_file() && !path.is_symlink());
    let toml = std::fs::read_to_string(&path).expect("native TOML exists");
    assert_eq!(toml, content);
    assert!(toml.contains("name = \"flow:reviewer\""));
    assert!(toml.contains("description = \"Portable UZE custom agent.\""));
    assert!(toml.contains("developer_instructions = \"Review.\""));
}

#[test]
fn claude_attaches_an_agent_without_treating_its_markdown_as_a_skill_plugin() {
    let root = uze_testkit::temp::scratch("claude-agent");
    let home = UzeHome::at(root.join("uze"));
    let claude = ClaudeIntegration::new(root.join("home/.claude"), home);
    let resource = agent(&root);

    let receipt = claude
        .attach_receipt(&resource)
        .expect("Claude agent attachment succeeds")
        .expect("Claude agent attachment has a receipt");
    let uze_core::integration::ManagedArtifact::GeneratedFile { path, .. } = receipt.artifact
    else {
        panic!("Claude native agent is a receipt-owned file");
    };
    // Outside a plugin Claude names a user agent after its frontmatter
    // `name`, so the definition carries the label there.
    assert_eq!(path, root.join("home/.claude/agents/flow:reviewer.md"));
    assert!(path.is_file() && !path.is_symlink());
    let definition = std::fs::read_to_string(&path).expect("definition exists");
    assert!(definition.starts_with("---\nname: flow:reviewer\n"));
    assert!(definition.ends_with("---\nReview.\n"));
}

#[test]
fn opencode_receives_only_the_fields_it_reads_and_says_what_it_left() {
    let root = uze_testkit::temp::scratch("opencode-agent");
    let home = UzeHome::at(root.join("uze"));
    let opencode = OpenCodeIntegration::new(
        root.join("home/.agents"),
        root.join("home/.config/opencode/opencode.json"),
        home,
    );
    let package_root = root.join("store/flow");
    let id = PackageId::from_plugin_name("flow", &package_root.join("plugin.json")).unwrap();
    let resource = Resource::from_package(
        id,
        package_root.clone(),
        Capability {
            kind: CapabilityKind::Agent,
            path: package_root.join("agents/review/security.md"),
            payload: b"---\ndescription: Audits\nmodel: haiku\ntools: Read, Grep\n---\nAudit ${PLUGIN_ROOT}/x.\n".to_vec(),
        },
    );

    let plan = opencode.exposure_plan(&resource);
    assert_eq!(plan.route, CompatibilityRoute::Degraded);
    assert!(plan.evidence.contains("model, tools"), "{}", plan.evidence);

    let receipt = opencode
        .attach_receipt(&resource)
        .expect("OpenCode agent attachment succeeds")
        .expect("OpenCode agent attachment has a receipt");
    let uze_core::integration::ManagedArtifact::GeneratedFile { path, .. } = receipt.artifact
    else {
        panic!("OpenCode agent is a receipt-owned file");
    };
    assert_eq!(path.file_name().unwrap(), "flow:review:security.md");
    let definition = std::fs::read_to_string(&path).expect("definition exists");
    assert!(!definition.contains("model:"), "{definition}");
    assert!(!definition.contains("tools:"), "{definition}");
    assert!(definition.contains("description: Audits"));
    // Without `mode: subagent` OpenCode makes the agent a primary one it
    // never offers the model; its name is its file, so none is written.
    assert!(definition.contains("mode: subagent"), "{definition}");
    assert!(!definition.contains("name:"), "{definition}");
    assert!(definition.contains(&format!("Audit {}/x.", package_root.display())));
}

/// An agent that tells each harness its own model, and names a root
/// `model` only Claude spells this way.
const PER_HARNESS: &[u8] = b"---\nname: reviewer\ndescription: Reviews\nmodel: haiku\nharness:\n  claude-code: { model: sonnet, permissionMode: plan }\n  codex: { model: gpt-6-luna, model_reasoning_effort: high, nickname: rev }\n  opencode: { model: anthropic/claude-haiku-4-5, tools: { read: true }, temperature: 0.1 }\n  agy: { model: gemini-3.1-flash-lite-preview }\n---\nReview.\n";

fn agent_with(root: &std::path::Path, payload: &[u8]) -> Resource {
    let mut resource = agent(root);
    resource.capability.payload = payload.to_vec();
    resource
}

fn generated_content(plan: uze_core::exposure::ExposurePlan) -> String {
    match plan.mechanism {
        uze_core::exposure::ExposureMechanism::Managed(
            uze_core::integration::ManagedArtifact::GeneratedFile { content, .. },
        ) => content,
        other => panic!("an agent is a generated file, got {other:?}"),
    }
}

#[test]
fn each_harness_receives_its_own_block_and_never_the_block_itself() {
    let root = uze_testkit::temp::scratch("agent-harness-block");
    let home = UzeHome::at(root.join("uze"));
    let resource = agent_with(&root, PER_HARNESS);

    let claude = generated_content(
        ClaudeIntegration::new(root.join("claude"), home.clone()).exposure_plan(&resource),
    );
    assert!(claude.contains("model: sonnet"), "{claude}");
    assert!(claude.contains("permissionMode: plan"), "{claude}");

    let codex = generated_content(
        CodexIntegration::new(root.join("agents"), home.clone()).exposure_plan(&resource),
    );
    assert!(codex.contains("model = \"gpt-6-luna\""), "{codex}");
    assert!(
        codex.contains("model_reasoning_effort = \"high\""),
        "{codex}"
    );
    assert!(
        !codex.contains("nickname") && !codex.contains("haiku"),
        "Codex refuses a key it does not know: {codex}"
    );

    let opencode = generated_content(
        OpenCodeIntegration::new(
            root.join("agents"),
            root.join("config/opencode.json"),
            home.clone(),
        )
        .exposure_plan(&resource),
    );
    assert!(
        opencode.contains("model: anthropic/claude-haiku-4-5"),
        "{opencode}"
    );
    assert!(opencode.contains("mode: subagent"), "{opencode}");
    assert!(opencode.contains("temperature: 0.1"), "{opencode}");
    assert!(!opencode.contains("model: haiku"), "{opencode}");

    let antigravity = generated_content(
        AntigravityIntegration::new(root.join("agents"), home).exposure_plan(&resource),
    );
    assert!(
        !antigravity.contains("model"),
        "Antigravity drops an agent carrying `model`, so it is left out: {antigravity}"
    );

    for delivered in [&claude, &opencode, &antigravity, &codex] {
        assert!(!delivered.contains("harness"), "{delivered}");
    }
    let _ = std::fs::remove_dir_all(root);
}

/// `flow` installed into a Store of its own, carrying one agent at
/// `relative` with `definition`, and that agent as the Engine finds it.
fn stored_agent(
    root: &std::path::Path,
    relative: &str,
    definition: &str,
) -> (uze_core::store::StoredPackage, Resource) {
    let package_root = root.join("flow");
    let agent = package_root.join(relative);
    std::fs::create_dir_all(agent.parent().unwrap()).unwrap();
    std::fs::write(package_root.join("plugin.json"), r#"{"name":"flow"}"#).unwrap();
    std::fs::write(&agent, definition).unwrap();
    let store = uze_core::UzeStore::new(UzeHome::at(root.join("uze")));
    let package = store
        .ingest(
            &uze_core::acquisition::acquire(&uze_core::PackageSource::local(&package_root))
                .unwrap(),
            "local",
            None,
        )
        .unwrap();
    let resource = uze_core::engine::package_resources(&package)
        .unwrap()
        .into_iter()
        .find(|resource| resource.capability.kind == CapabilityKind::Agent)
        .expect("the Engine finds the agent");
    (package, resource)
}

/// Inside the plugin Claude namespaces the agent itself, and outside it
/// the file carries the label: either way a session dispatches it as
/// `flow:<agent>`, never by the bare name the author wrote.
#[test]
fn claude_exposes_an_agent_only_under_its_plugin_qualified_label() {
    for (relative, label, bare) in [
        ("agents/reviewer.md", "flow:reviewer", "reviewer"),
        (
            "agents/review/security.md",
            "flow:review:security",
            "security",
        ),
    ] {
        let root = uze_testkit::temp::scratch("claude-agent-label");
        let (package, resource) = stored_agent(
            &root,
            relative,
            &format!("---\nname: {bare}\ndescription: Reviews\n---\nReview.\n"),
        );
        let claude = ClaudeIntegration::new(root.join("claude"), UzeHome::at(root.join("uze")));

        assert_eq!(
            claude
                .packaged_exposure_name(&package, &resource)
                .as_deref(),
            Some(label)
        );
        assert_eq!(claude.exposure_name_candidates(&resource), vec![label]);
        let plan = claude.exposure_plan(&resource);
        let uze_core::exposure::ExposureMechanism::Managed(
            uze_core::integration::ManagedArtifact::GeneratedFile { path, content },
        ) = &plan.mechanism
        else {
            panic!("a loose Claude agent is a generated file: {plan:?}");
        };
        assert_eq!(path.file_stem().unwrap(), label);
        assert!(
            content.starts_with(&format!("---\nname: {label}\n")),
            "{content}"
        );
        assert!(!content.contains(&format!("name: {bare}\n")), "{content}");
        let _ = std::fs::remove_dir_all(root);
    }
}

/// Claude loads a plugin's agents without four fields a user agent
/// honours; an agent declaring any of them is delivered Degraded, with
/// the ones it loses named, and one declaring none is delivered whole.
#[test]
fn claude_names_the_fields_an_agent_loses_inside_the_plugin() {
    let root = uze_testkit::temp::scratch("claude-packaged-shortfall");
    let (package, resource) = stored_agent(
        &root,
        "agents/reviewer.md",
        "---\nname: reviewer\ndescription: Reviews\nmodel: haiku\ninitialPrompt: Begin\npermissionMode: plan\nmcpServers: [docs]\n---\nReview.\n",
    );
    let claude = ClaudeIntegration::new(root.join("claude"), UzeHome::at(root.join("uze")));

    assert_eq!(
        claude.packaged_shortfall(&package, &resource),
        Some((
            CompatibilityRoute::Degraded,
            "Claude Code ignores these fields on an agent a plugin delivers: permissionMode, \
             mcpServers, initialPrompt."
                .to_owned()
        ))
    );

    let mut whole = resource.clone();
    whole.capability.payload =
        b"---\nname: reviewer\ndescription: Reviews\nmodel: haiku\n---\nReview.\n".to_vec();
    assert_eq!(claude.packaged_shortfall(&package, &whole), None);

    let mut skill = resource;
    skill.capability.kind = CapabilityKind::AgentSkill;
    assert_eq!(
        claude.packaged_shortfall(&package, &skill),
        None,
        "only an agent loses fields inside the plugin"
    );
    let _ = std::fs::remove_dir_all(root);
}
