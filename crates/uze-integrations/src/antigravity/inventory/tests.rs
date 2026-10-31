use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use uze_core::inventory::{
    Claim, Definition, FoundResource, ResourceKind, Runs, SessionCounts, Source,
};

use super::Locations;

struct Gemini {
    _scratch: uze_testkit::temp::TempDir,
    root: PathBuf,
}

impl Gemini {
    fn new(label: &str) -> Self {
        let scratch = uze_testkit::temp::TempDir::new(label);
        let root = scratch.join(".gemini");
        fs::create_dir_all(root.join("antigravity-cli")).unwrap();
        fs::create_dir_all(root.join("config")).unwrap();
        Self {
            _scratch: scratch,
            root,
        }
    }

    fn cli(&self) -> PathBuf {
        self.root.join("antigravity-cli")
    }

    fn config(&self) -> PathBuf {
        self.root.join("config")
    }

    fn write(&self, relative: &str, content: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        path
    }

    fn found(&self) -> Vec<FoundResource> {
        let (cli, config) = (self.cli(), self.config());
        Locations {
            cli_root: &cli,
            config_dir: &config,
            gemini_root: &self.root,
        }
        .inventory()
    }

    fn sessions(&self, now: SystemTime) -> SessionCounts {
        let (cli, config) = (self.cli(), self.config());
        Locations {
            cli_root: &cli,
            config_dir: &config,
            gemini_root: &self.root,
        }
        .session_counts(now)
    }
}

const SKILL: &str = "---\nname: review\ndescription: Review a diff.\n---\nBody.\n";

fn named<'a>(found: &'a [FoundResource], name: &str) -> &'a FoundResource {
    found
        .iter()
        .find(|resource| resource.name == name)
        .unwrap_or_else(|| panic!("{name} not in {found:#?}"))
}

#[test]
fn a_skill_in_the_global_root_is_listed_by_its_directory() {
    let gemini = Gemini::new("agy-inventory-skill");
    let file = gemini.write("antigravity-cli/skills/review/SKILL.md", SKILL);
    let found = gemini.found();
    let skill = named(&found, "review");
    assert_eq!(skill.kind, ResourceKind::Skill);
    assert_eq!(skill.definition, Definition::Markdown(file));
    assert_eq!(skill.source, Source::ByHand);
}

// agy links its skills roots with a Unix symlink; a Windows link needs
// privileges a test process does not have.
#[cfg(unix)]
#[test]
fn a_skill_reachable_through_the_linked_root_is_counted_once() {
    let gemini = Gemini::new("agy-inventory-linked");
    gemini.write("config/skills/review/SKILL.md", SKILL);
    std::os::unix::fs::symlink(gemini.config().join("skills"), gemini.cli().join("skills"))
        .unwrap();
    let found = gemini.found();
    assert_eq!(found.iter().filter(|r| r.name == "review").count(), 1);
}

#[test]
fn a_skill_agy_ships_is_built_in() {
    let gemini = Gemini::new("agy-inventory-builtin");
    gemini.write("antigravity-cli/builtin/skills/plugin/SKILL.md", SKILL);
    assert_eq!(named(&gemini.found(), "plugin").source, Source::BuiltIn);
}

#[test]
fn a_global_agent_is_listed() {
    let gemini = Gemini::new("agy-inventory-agent");
    gemini.write(
        "antigravity-cli/agents/planner.md",
        "---\nname: planner\ndescription: Plans.\n---\n",
    );
    assert_eq!(named(&gemini.found(), "planner").kind, ResourceKind::Agent);
}

#[test]
fn a_plugins_skills_and_agents_carry_its_namespace_and_its_claim() {
    let gemini = Gemini::new("agy-inventory-plugin");
    gemini.write("config/plugins/code/plugin.json", r#"{"name":"code"}"#);
    gemini.write(
        "config/plugins/code/agents.json",
        r#"{"entries":[{"path":"agents"}]}"#,
    );
    gemini.write("config/plugins/code/skills/polish/SKILL.md", SKILL);
    gemini.write(
        "config/plugins/code/agents/code_reviewer.md",
        "---\nname: code_reviewer\ndescription: Reviews.\n---\n",
    );
    let found = gemini.found();
    let claim = Some(Claim::Owned {
        kind: "antigravity-plugin-generated".into(),
        selector: "code".into(),
    });
    for name in ["code:polish", "code:reviewer"] {
        let resource = named(&found, name);
        assert_eq!(
            resource.source,
            Source::Plugin {
                name: "code".into()
            }
        );
        assert_eq!(resource.claim, claim);
        assert_eq!(
            resource.removal.as_deref(),
            Some("agy plugin uninstall code")
        );
    }
}

#[test]
fn mcp_servers_are_entries_with_what_they_run() {
    let gemini = Gemini::new("agy-inventory-mcp");
    let config = gemini.write(
        "config/mcp_config.json",
        r#"{"mcpServers":{"local":{"command":"/bin/server","args":["--serve"]},"remote":{"serverUrl":"https://example.invalid/mcp"}}}"#,
    );
    let found = gemini.found();
    let local = named(&found, "local");
    assert_eq!(local.definition, Definition::Entry(config));
    assert_eq!(local.runs, Some(Runs::Program("/bin/server".into())));
    assert_eq!(local.claim, Some(Claim::Entry("local".into())));
    assert_eq!(local.removal.as_deref(), Some("agy mcp remove local"));
    assert_eq!(
        named(&found, "remote").runs,
        Some(Runs::Url("https://example.invalid/mcp".into()))
    );
}

#[test]
fn an_empty_mcp_config_lists_nothing() {
    let gemini = Gemini::new("agy-inventory-mcp-empty");
    gemini.write("config/mcp_config.json", "");
    assert!(gemini.found().iter().all(|r| r.kind != ResourceKind::Mcp));
}

#[test]
fn named_hooks_run_their_first_command_in_either_form() {
    let gemini = Gemini::new("agy-inventory-hooks");
    gemini.write(
        "config/hooks.json",
        r#"{
            "guard": {"PreToolUse": [{"matcher": "run_command", "hooks": [{"type": "command", "command": "bash /nowhere/guard.sh"}]}]},
            "done": {"Stop": [{"type": "command", "command": "notify-send done"}]}
        }"#,
    );
    let found = gemini.found();
    let guard = named(&found, "guard");
    assert_eq!(guard.kind, ResourceKind::Hook);
    assert_eq!(
        guard.runs,
        Some(Runs::Shell("bash /nowhere/guard.sh".into()))
    );
    assert_eq!(guard.claim, Some(Claim::Entry("guard".into())));
    assert_eq!(guard.note.as_deref(), Some("on PreToolUse"));
    assert_eq!(
        named(&found, "done").runs,
        Some(Runs::Shell("notify-send done".into()))
    );
}

#[test]
fn the_global_rules_file_is_read_whole_when_it_exists() {
    let gemini = Gemini::new("agy-inventory-rules");
    assert!(gemini.found().iter().all(|r| r.kind != ResourceKind::Rule));
    let file = gemini.write("GEMINI.md", "Answer briefly.\n");
    assert_eq!(
        named(&gemini.found(), "GEMINI.md").definition,
        Definition::Whole(file)
    );
}

fn age(path: &Path, seconds_ago: u64, now: SystemTime) {
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(now - Duration::from_secs(seconds_ago))
        .unwrap();
}

#[test]
fn sessions_are_the_conversation_databases_and_never_their_logs() {
    let gemini = Gemini::new("agy-inventory-sessions");
    let now = SystemTime::now();
    let day = 86_400;
    let recent = gemini.write("antigravity-cli/conversations/a.db", "");
    gemini.write("antigravity-cli/conversations/a.db-wal", "");
    gemini.write("antigravity-cli/conversations/a.db-shm", "");
    let old = gemini.write("antigravity-cli/conversations/b.db", "");
    age(&recent, 2 * day, now);
    age(&old, 40 * day, now);
    let counts = gemini.sessions(now);
    assert_eq!((counts.days_7, counts.days_30, counts.days_90), (1, 1, 2));
}

#[test]
fn a_write_ahead_log_newer_than_its_database_moves_the_activity() {
    let gemini = Gemini::new("agy-inventory-wal");
    let now = SystemTime::now();
    let database = gemini.write("antigravity-cli/conversations/a.db", "");
    let log = gemini.write("antigravity-cli/conversations/a.db-wal", "");
    age(&database, 40 * 86_400, now);
    age(&log, 86_400, now);
    assert_eq!(gemini.sessions(now).days_7, 1);
}
