use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

use uze_core::{
    integration::{AttachmentReceipt, ManagedArtifact},
    inventory::{Claim, Definition, ResourceKind, Runs, Source},
};
use uze_testkit::temp::TempDir;

use super::{CodexPlaces, inventory, session_counts};

fn places(root: &TempDir) -> CodexPlaces {
    CodexPlaces {
        agents_skills: root.join(".agents/skills"),
        codex_home: root.join(".codex"),
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn skill(dir: &Path, name: &str) {
    write(
        &dir.join(name).join("SKILL.md"),
        &format!("---\nname: {name}\ndescription: {name} things.\n---\nBody.\n"),
    );
}

fn owned(kind: &str, selector: &str, detail: serde_json::Value) -> AttachmentReceipt {
    AttachmentReceipt {
        package_id: "code@uze".into(),
        resource_identity: None,
        integration: "codex".into(),
        artifact: ManagedArtifact::IntegrationOwned {
            kind: kind.into(),
            selector: selector.into(),
            detail: serde_json::from_value(detail).unwrap(),
        },
    }
}

#[test]
fn skills_by_hand_and_codexs_own_are_listed_apart() {
    let root = TempDir::new("codex-inventory-skills");
    skill(&root.join(".agents/skills"), "review");
    skill(&root.join(".codex/skills"), "notes");
    skill(&root.join(".codex/skills/.system"), "imagegen");
    let found = inventory(&places(&root), &[]);
    let names: Vec<_> = found.iter().map(|r| (r.name.as_str(), &r.source)).collect();
    assert_eq!(
        names,
        [
            ("review", &Source::ByHand),
            ("notes", &Source::ByHand),
            ("imagegen", &Source::BuiltIn)
        ]
    );
    assert!(matches!(found[0].definition, Definition::Markdown(_)));
}

#[test]
fn a_listed_plugins_skills_are_named_under_it_and_claimed_by_uzes_receipt() {
    let root = TempDir::new("codex-inventory-plugins");
    write(
        &root.join(".codex/config.toml"),
        "[plugins.\"code@uze-store\"]\nenabled = true\n\n[plugins.\"sites@openai-curated\"]\nenabled = true\n",
    );
    skill(
        &root.join(".codex/plugins/cache/uze-store/code/0.1.0/skills"),
        "polish",
    );
    skill(
        &root.join(".codex/plugins/cache/openai-curated/sites/1.1.2/skills"),
        "publish",
    );
    let receipt = owned(
        "marketplace-plugin-generated",
        "code@uze-store",
        serde_json::json!({}),
    );
    let found = inventory(&places(&root), &[receipt]);
    let polish = found.iter().find(|r| r.name == "code:polish").unwrap();
    assert_eq!(
        polish.source,
        Source::Plugin {
            name: "code".into()
        }
    );
    assert_eq!(
        polish.claim,
        Some(Claim::Owned {
            kind: "marketplace-plugin-generated".into(),
            selector: "code@uze-store".into()
        })
    );
    assert_eq!(
        polish.removal.as_deref(),
        Some("codex plugin remove code@uze-store")
    );
    let publish = found.iter().find(|r| r.name == "sites:publish").unwrap();
    assert_eq!(publish.claim, None);
}

#[test]
fn a_listed_plugin_missing_from_the_cache_is_reported() {
    let root = TempDir::new("codex-inventory-plugin-gone");
    write(
        &root.join(".codex/config.toml"),
        "[plugins.\"gone@market\"]\nenabled = true\n",
    );
    let found = inventory(&places(&root), &[]);
    assert_eq!(
        found[0].problem.as_deref(),
        Some("plugin directory missing")
    );
}

#[test]
fn an_agent_lists_its_description_and_loads_its_instructions() {
    let root = TempDir::new("codex-inventory-agents");
    write(
        &root.join(".codex/agents/reviewer.toml"),
        "name = \"code:reviewer\"\ndescription = \"Reviews code.\"\ndeveloper_instructions = \"You are reviewing code.\"\n",
    );
    let found = inventory(&places(&root), &[]);
    assert_eq!(found[0].kind, ResourceKind::Agent);
    assert_eq!(found[0].name, "code:reviewer");
    let Definition::Listed {
        file,
        per_session,
        when_used,
    } = &found[0].definition
    else {
        panic!("{:?}", found[0].definition);
    };
    assert_eq!(file, &root.join(".codex/agents/reviewer.toml"));
    assert!(per_session.contains("Reviews code."));
    assert_eq!(when_used.as_deref(), Some("You are reviewing code."));
}

#[test]
fn mcp_servers_name_what_they_run_and_how_codex_removes_them() {
    let root = TempDir::new("codex-inventory-mcp");
    write(
        &root.join(".codex/config.toml"),
        "[mcp_servers.github]\ncommand = \"github-mcp\"\nargs = [\"stdio\"]\n\n[mcp_servers.docs]\nurl = \"https://example.invalid/mcp\"\n",
    );
    let found = inventory(&places(&root), &[]);
    let github = found.iter().find(|r| r.name == "github").unwrap();
    assert_eq!(github.runs, Some(Runs::Program("github-mcp".into())));
    assert_eq!(github.claim, Some(Claim::Entry("github".into())));
    assert_eq!(github.removal.as_deref(), Some("codex mcp remove github"));
    let docs = found.iter().find(|r| r.name == "docs").unwrap();
    assert_eq!(
        docs.runs,
        Some(Runs::Url("https://example.invalid/mcp".into()))
    );
}

#[test]
fn hooks_are_named_by_their_program_and_uzes_engine_is_claimed() {
    let root = TempDir::new("codex-inventory-hooks");
    write(
        &root.join(".codex/hooks.json"),
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"'/uze/rules/engine' '/uze/sessions'"}]}],"PostToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":"bash /home/me/bin/format.sh"}]}]}}"#,
    );
    let receipt = owned(
        "rules-engine",
        "SessionStart",
        serde_json::json!({"engine": "/uze/rules/engine"}),
    );
    let found = inventory(&places(&root), &[receipt]);
    let engine = found.iter().find(|r| r.name == "uze-rules").unwrap();
    assert_eq!(
        engine.claim,
        Some(Claim::Owned {
            kind: "rules-engine".into(),
            selector: "SessionStart".into()
        })
    );
    assert_eq!(engine.note.as_deref(), Some("runs on SessionStart"));
    let format = found.iter().find(|r| r.name == "bash").unwrap();
    assert_eq!(format.claim, None);
    assert_eq!(
        format.runs,
        Some(Runs::Shell("bash /home/me/bin/format.sh".into()))
    );
}

#[test]
fn the_global_instructions_file_is_a_rule_only_when_it_exists() {
    let root = TempDir::new("codex-inventory-rule");
    assert!(inventory(&places(&root), &[]).is_empty());
    write(&root.join(".codex/AGENTS.md"), "Answer briefly.\n");
    let found = inventory(&places(&root), &[]);
    assert_eq!(found[0].kind, ResourceKind::Rule);
    assert!(matches!(found[0].definition, Definition::Whole(_)));
}

fn rollout(root: &TempDir, day: &str, name: &str, first_line: &str, rest: &str) {
    write(
        &root.join(format!(
            ".codex/sessions/2026/10/{day}/rollout-{name}.jsonl"
        )),
        &format!("{first_line}\n{rest}"),
    );
}

#[test]
fn a_subagents_thread_is_not_a_session() {
    let root = TempDir::new("codex-inventory-sessions");
    rollout(
        &root,
        "30",
        "a",
        r#"{"type":"session_meta","payload":{"id":"a","parent_thread_id":null}}"#,
        "",
    );
    rollout(
        &root,
        "30",
        "b",
        r#"{"type":"session_meta","payload":{"id":"b","parent_thread_id":"a"}}"#,
        "",
    );
    rollout(
        &root,
        "31",
        "c",
        r#"{"type":"session_meta","payload":{"id":"c"}}"#,
        "",
    );
    write(&root.join(".codex/sessions/2026/10/31/notes.jsonl"), "{}\n");
    let counts = session_counts(&places(&root));
    assert_eq!(counts.total, 2);
}

#[test]
fn nothing_past_the_first_line_is_read() {
    let root = TempDir::new("codex-inventory-first-line");
    rollout(
        &root,
        "30",
        "a",
        r#"{"type":"session_meta","payload":{"id":"a"}}"#,
        "\u{0}not json at all {{{\n\"parent_thread_id\":\"x\"\n",
    );
    let counts = session_counts(&places(&root));
    assert_eq!(counts.total, 1);
}

#[test]
fn the_oldest_session_says_how_far_back_the_records_reach() {
    let root = TempDir::new("codex-inventory-old");
    rollout(
        &root,
        "01",
        "old",
        r#"{"type":"session_meta","payload":{"id":"old"}}"#,
        "",
    );
    let path = root.join(".codex/sessions/2026/10/01/rollout-old.jsonl");
    let file = fs::File::options().write(true).open(&path).unwrap();
    let forty_days_ago = SystemTime::now() - Duration::from_secs(86_400 * 40);
    file.set_modified(forty_days_ago).unwrap();
    let counts = session_counts(&places(&root));
    assert_eq!(counts.total, 1);
    let oldest = forty_days_ago
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert_eq!(counts.oldest, Some(oldest));
}
