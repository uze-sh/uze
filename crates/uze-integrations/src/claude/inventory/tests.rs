use std::{fs, path::Path, time::SystemTime};

use uze_core::{
    UzeHome,
    inventory::{Claim, Definition, FoundResource, ResourceKind, Runs, Source},
};

use super::{ClaudeHome, machine_inventory, session_counts};

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn inventory_of(root: &Path) -> Vec<FoundResource> {
    let uze_home = UzeHome::at(root.join("uze"));
    machine_inventory(&ClaudeHome {
        config: &root.join(".claude"),
        command_home: root,
        uze_home: &uze_home,
    })
}

fn named<'a>(found: &'a [FoundResource], name: &str) -> &'a FoundResource {
    found
        .iter()
        .find(|resource| resource.name == name)
        .unwrap_or_else(|| panic!("{name} not in {found:#?}"))
}

#[test]
fn skills_agents_and_the_instructions_file_by_hand() {
    let root = uze_testkit::temp::TempDir::new("claude-inventory-own");
    write(
        &root.join(".claude/skills/review/SKILL.md"),
        "---\nname: review\n---\n",
    );
    write(
        &root.join(".claude/agents/planner.md"),
        "---\nname: planner\n---\n",
    );
    write(&root.join(".claude/CLAUDE.md"), "Answer briefly.\n");
    let found = inventory_of(root.path());
    assert_eq!(named(&found, "review").kind, ResourceKind::Skill);
    assert_eq!(named(&found, "planner").kind, ResourceKind::Agent);
    assert_eq!(
        named(&found, "CLAUDE.md").definition,
        Definition::Whole(root.join(".claude/CLAUDE.md"))
    );
    assert!(
        found
            .iter()
            .all(|resource| resource.source == Source::ByHand)
    );
}

#[test]
fn user_mcp_servers_and_never_a_projects() {
    let root = uze_testkit::temp::TempDir::new("claude-inventory-mcp");
    write(
        &root.join(".claude.json"),
        r#"{"mcpServers":{"github":{"type":"http","url":"https://example.invalid"},"local":{"command":"npx","args":["x"]}},
            "projects":{"/work/app":{"mcpServers":{"only-here":{"command":"x"}}}}}"#,
    );
    let found = inventory_of(root.path());
    let github = named(&found, "github");
    assert_eq!(
        github.runs,
        Some(Runs::Url("https://example.invalid".into()))
    );
    assert_eq!(github.claim, Some(Claim::Entry("github".into())));
    assert_eq!(
        github.removal.as_deref(),
        Some("claude mcp remove --scope user github")
    );
    assert_eq!(
        named(&found, "local").runs,
        Some(Runs::Program("npx".into()))
    );
    assert!(found.iter().all(|resource| resource.name != "only-here"));
}

#[test]
fn hooks_are_named_by_what_they_run_and_uzes_engine_is_claimed() {
    let root = uze_testkit::temp::TempDir::new("claude-inventory-hooks");
    let engine = root.join("uze/runtime/attachments/claude/rules/engine");
    write(
        &root.join(".claude/settings.json"),
        &format!(
            r#"{{"hooks":{{"PostToolUse":[{{"matcher":"Edit","hooks":[{{"type":"command","command":"bash ~/bin/format-on-save.sh"}}]}}],
                "SessionStart":[{{"hooks":[{{"type":"command","command":"{}"}}]}}]}}}}"#,
            engine.display()
        ),
    );
    let found = inventory_of(root.path());
    let format = named(&found, "format-on-save");
    assert_eq!(format.note.as_deref(), Some("on PostToolUse · Edit"));
    assert_eq!(format.claim, None);
    assert_eq!(
        named(&found, "uze-rules").claim,
        Some(Claim::Owned {
            kind: "rules-engine".into(),
            selector: "SessionStart".into()
        })
    );
}

#[test]
fn plugins_bring_their_resources_and_switched_off_ones_bring_nothing() {
    let root = uze_testkit::temp::TempDir::new("claude-inventory-plugins");
    let vercel = root.join(".claude/plugins/cache/official/vercel/1.0.0");
    let code = root.join(".claude/plugins/cache/uze-store/code/0.1.0");
    let off = root.join(".claude/plugins/cache/official/off/1.0.0");
    write(
        &vercel.join("skills/nextjs/SKILL.md"),
        "---\nname: nextjs\n---\n",
    );
    write(
        &vercel.join("agents/deployer.md"),
        "---\nname: deployer\n---\n",
    );
    write(
        &vercel.join(".mcp.json"),
        r#"{"vercel":{"url":"https://example.invalid"}}"#,
    );
    write(
        &code.join("skills/polish/SKILL.md"),
        "---\nname: polish\n---\n",
    );
    write(
        &off.join("skills/hidden/SKILL.md"),
        "---\nname: hidden\n---\n",
    );
    write(
        &root.join(".claude/plugins/installed_plugins.json"),
        &format!(
            r#"{{"version":2,"plugins":{{
                "vercel@official":[{{"scope":"user","installPath":"{}"}}],
                "code@uze-store":[{{"scope":"user","installPath":"{}"}}],
                "off@official":[{{"scope":"user","installPath":"{}"}}],
                "local@official":[{{"scope":"project","installPath":"/work"}}]}}}}"#,
            vercel.display(),
            code.display(),
            off.display()
        ),
    );
    write(
        &root.join(".claude/settings.json"),
        r#"{"enabledPlugins":{"off@official":false}}"#,
    );
    let found = inventory_of(root.path());
    let nextjs = named(&found, "vercel:nextjs");
    assert_eq!(
        nextjs.source,
        Source::Plugin {
            name: "vercel".into()
        }
    );
    assert_eq!(
        nextjs.removal.as_deref(),
        Some("claude plugin uninstall vercel@official")
    );
    assert_eq!(named(&found, "vercel:deployer").kind, ResourceKind::Agent);
    assert_eq!(named(&found, "vercel").kind, ResourceKind::Mcp);
    let polish = named(&found, "code:polish");
    assert_eq!(
        polish.claim,
        Some(Claim::Owned {
            kind: "claude-plugin-generated".into(),
            selector: "code@uze-store".into()
        })
    );
    assert_eq!(polish.removal, None);
    assert!(
        found
            .iter()
            .all(|resource| !resource.name.starts_with("off:"))
    );
    assert!(
        found
            .iter()
            .all(|resource| !resource.name.starts_with("local:"))
    );
}

#[test]
fn a_plugin_whose_directory_is_gone_says_so() {
    let root = uze_testkit::temp::TempDir::new("claude-inventory-gone");
    write(
        &root.join(".claude/plugins/installed_plugins.json"),
        r#"{"plugins":{"gone@official":[{"scope":"user","installPath":"/nowhere/gone"}]}}"#,
    );
    let found = inventory_of(root.path());
    assert_eq!(
        named(&found, "gone").problem.as_deref(),
        Some("plugin directory missing")
    );
}

#[test]
fn nothing_installed_is_an_empty_list_not_an_error() {
    let root = uze_testkit::temp::TempDir::new("claude-inventory-empty");
    assert!(inventory_of(root.path()).is_empty());
}

#[test]
fn sessions_are_root_transcripts_and_never_subagents() {
    let root = uze_testkit::temp::TempDir::new("claude-sessions");
    let projects = root.join("projects");
    write(&projects.join("-work-app/one.jsonl"), "not even json\n");
    write(&projects.join("-work-app/two.jsonl"), "");
    write(&projects.join("-work-app/one/subagents/agent-a.jsonl"), "");
    write(&projects.join("-work-other/three.jsonl"), "");
    let counts = session_counts(&projects, SystemTime::now()).unwrap();
    assert_eq!(counts.days_30, 3);
}

#[test]
fn no_transcripts_directory_is_unknown() {
    let root = uze_testkit::temp::TempDir::new("claude-sessions-none");
    assert!(session_counts(&root.join("projects"), SystemTime::now()).is_none());
}

#[test]
fn a_directory_without_a_skill_file_is_not_a_skill() {
    let root = uze_testkit::temp::TempDir::new("claude-inventory-not-a-skill");
    fs::create_dir_all(root.join(".claude/skills/synced/some-id")).unwrap();
    assert!(inventory_of(root.path()).is_empty());
}
