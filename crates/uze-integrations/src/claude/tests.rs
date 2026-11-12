//! `ClaudeIntegration` through the port: each method reaches the part of
//! Claude Code it names, on a real integration over an isolated home.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use uze_core::{
    capability::{Capability, CapabilityKind, Resource},
    home::UzeHome,
    integration::{
        AttachmentReceipt, AttachmentState, IntegrationPort, ManagedArtifact, ObservationContext,
        RegistrationRead,
    },
    session::SessionId,
    store::StoredPackage,
    usage::Resume,
};

use super::{ClaudeIntegration, ClaudeMarketplace};
use crate::shared::marketplace::MarketplaceDialect;

const CONVERSATION: &str = "00000000-0000-4000-8000-0000000000aa";

/// A Claude Code configured under `<root>/home/.claude`, so its HOME root
/// (`~/.claude.json`, plugin records) is `<root>/home`.
struct Claude {
    root: PathBuf,
    uze_home: UzeHome,
    integration: ClaudeIntegration,
}

impl Claude {
    fn new(label: &str) -> Self {
        let root = uze_testkit::temp::scratch(label);
        let uze_home = UzeHome::at(root.join("uze"));
        let integration = ClaudeIntegration::new(root.join("home/.claude"), uze_home.clone());
        Self {
            root,
            uze_home,
            integration,
        }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn config(&self) -> PathBuf {
        self.home().join(".claude")
    }

    fn write(&self, path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn package(&self, name: &str) -> StoredPackage {
        let root = self.root.join("store").join(name);
        fs::create_dir_all(&root).unwrap();
        crate::shared::test_support::stored_at(&root, name)
    }

    fn mark_setup(&self) {
        uze_core::state::record(
            &self.uze_home,
            self.integration.id(),
            uze_core::state::IntegrationRecord {
                version: None,
                strategy: "test".to_owned(),
            },
        )
        .unwrap();
    }

    /// A stand-in `claude` first on `PATH`, logging every argument line it
    /// is run with; the returned guard holds the environment.
    // A POSIX shell script, which only Unix runs as a program.
    #[cfg(unix)]
    fn logging_cli(&self) -> (uze_testkit::env::ProcessEnvGuard<'static>, PathBuf) {
        let bin = self.root.join("bin");
        let log = self.root.join("argv.log");
        fs::create_dir_all(&bin).unwrap();
        uze_testkit::process::install_executable(
            &bin.join("claude"),
            format!("#!/bin/sh\necho \"$@\" >> '{}'\n", log.display()).as_bytes(),
        );
        let mut env = uze_testkit::env::scope();
        env.home(self.home())
            .set("PATH", uze_testkit::process::path_with(&[&bin]))
            .remove("CLAUDE_CONFIG_DIR");
        (env, log)
    }
}

impl Drop for Claude {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn logged(log: &Path) -> String {
    fs::read_to_string(log).unwrap_or_default()
}

fn resource(package: &StoredPackage, kind: CapabilityKind, relative: &str, body: &str) -> Resource {
    let path = package.root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, body).unwrap();
    Resource::from_package(
        package.id.clone(),
        package.root.clone(),
        Capability {
            kind,
            path,
            payload: body.as_bytes().to_vec(),
        },
    )
}

fn plugin_receipt(selector: &str) -> AttachmentReceipt {
    AttachmentReceipt {
        package_id: "git".to_owned(),
        resource_identity: None,
        integration: "claude-code".to_owned(),
        artifact: ManagedArtifact::IntegrationOwned {
            kind: ClaudeMarketplace::GENERATED_KIND.to_owned(),
            selector: selector.to_owned(),
            detail: BTreeMap::new(),
        },
    }
}

// ── Conversations ─────────────────────────────────────────────────────

/// The conversation an agent moved into is learned from Claude's own
/// transcripts, and resumed only while its transcript is still there.
#[test]
fn a_conversation_is_observed_and_exists_only_while_its_transcript_does() {
    let claude = Claude::new("claude-port-session");
    let cwd = Path::new("/work/slot");
    let session = SessionId::parse(CONVERSATION).unwrap();
    let transcript =
        super::session::transcript_path(&claude.config().join("projects"), cwd, &session);
    claude.write(&transcript, "{}\n");

    let observed = claude.integration.observe_session(&ObservationContext {
        cwd,
        since_unix: 0,
        preceded_by: None,
        claimed_elsewhere: &[],
    });

    assert_eq!(observed, Some(session.clone()));
    assert!(claude.integration.session_exists(&session, cwd));
    fs::remove_file(&transcript).unwrap();
    assert!(!claude.integration.session_exists(&session, cwd));
}

/// An agent continuing on Claude after another harness is handed its
/// briefing turn as the launch's prompt.
#[test]
fn a_first_turn_reaches_claude_as_the_launchs_prompt() {
    let claude = Claude::new("claude-port-first-turn");
    let words = claude.integration.first_turn_args("read your briefing");
    assert_eq!(
        words.last().map(|word| word.to_string_lossy().into_owned()),
        Some("read your briefing".to_owned())
    );
}

/// What a Claude conversation was about is handed to the harness the
/// agent switches to.
#[test]
fn a_conversation_digest_is_read_from_the_conversations_transcript() {
    let claude = Claude::new("claude-port-digest");
    let cwd = Path::new("/work/slot");
    let session = SessionId::parse(CONVERSATION).unwrap();
    claude.write(
        &super::session::transcript_path(&claude.config().join("projects"), cwd, &session),
        &serde_json::json!({"type": "user", "timestamp": "2026-10-06T00:00:00Z",
            "message": {"role": "user", "content": "fix the login"}})
        .to_string(),
    );

    let digest = claude
        .integration
        .conversation_digest(&session, cwd, None)
        .expect("a digest");

    assert_eq!(digest.goal.as_deref(), Some("fix the login"));
}

// ── Machine overview and usage ────────────────────────────────────────

#[test]
fn the_machine_overview_lists_claudes_skills_and_counts_its_sessions() {
    let claude = Claude::new("claude-port-inventory");
    claude.write(
        &claude.config().join("skills/x/SKILL.md"),
        "---\nname: x\n---\n",
    );
    claude.write(&claude.config().join("projects/-work-app/one.jsonl"), "");

    let found = claude
        .integration
        .machine_inventory()
        .expect("an inventory");
    assert!(
        found.iter().any(|resource| resource.name == "x"),
        "{found:#?}"
    );
    assert_eq!(
        claude
            .integration
            .session_counts()
            .expect("sessions are counted")
            .total,
        1
    );
}

#[test]
fn a_skill_the_model_called_is_read_as_a_use_from_the_transcripts() {
    let claude = Claude::new("claude-port-usage");
    claude.write(
        &claude.config().join("projects/-work-app/session.jsonl"),
        &format!(
            "{}\n",
            serde_json::json!({"type": "assistant", "timestamp": "2026-10-31T10:00:00Z",
                "message": {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t1", "name": "Skill", "input": {"skill": "review"}}]}})
        ),
    );

    let records = claude
        .integration
        .usage_records(std::time::UNIX_EPOCH)
        .expect("Claude keeps usage");
    assert_eq!(records.len(), 1, "{records:?}");
    let read = claude
        .integration
        .read_usage(&records[0], Resume::Start)
        .expect("the record reads");

    assert_eq!(read.uses.len(), 1);
    assert!(read.resume_at.is_some(), "a transcript is appended to");
}

/// Claude's auto memory reaches every agent's index, read in place.
#[test]
fn claudes_memory_is_a_source_for_every_agent() {
    let claude = Claude::new("claude-port-memory");
    let project = claude.root.join("repo");
    fs::create_dir_all(&project).unwrap();
    claude.write(
        &claude
            .config()
            .join("projects")
            .join(super::session::slug(&project))
            .join("memory/reply.md"),
        "---\nname: reply-in-ptbr\ndescription: Reply in pt-BR\ntype: feedback\n---\nbody\n",
    );

    let entries = claude.integration.memory_source(&project);

    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].name, "reply-in-ptbr");
    assert_eq!(entries[0].origin, "claude-code");
}

// ── Naming ────────────────────────────────────────────────────────────

/// `claude mcp add` accepts only letters, digits, `-` and `_`: those are
/// kept, and every other character of a server's name is spelled `-`.
#[test]
fn an_mcp_server_is_named_as_claudes_registry_accepts() {
    let claude = Claude::new("claude-port-mcp-names");
    let root = claude.root.join("store/std");
    fs::create_dir_all(&root).unwrap();
    let id = uze_core::store::PackageId::from_marketplace_plugin(
        "cardinal-rtc",
        "std",
        &root.join("plugin.json"),
    )
    .unwrap();
    let server = Resource::from_package_named(
        id,
        root.clone(),
        Capability {
            kind: CapabilityKind::Mcp,
            path: root.join("mcp.json"),
            payload: Vec::new(),
        },
        "my_srv.v2@edge".to_owned(),
    );

    let names = claude.integration.exposure_name_candidates(&server);

    assert_eq!(
        names,
        ["std-my_srv-v2-edge", "std-cardinal-rtc-my_srv-v2-edge"]
    );
}

/// A namespaced skill is invoked in Claude as `/<plugin>:<skill>`
/// (`docs/capabilities/skill-invocation-policy.md`).
#[test]
fn a_plugins_skill_is_invoked_as_a_slash_command_of_its_label() {
    let claude = Claude::new("claude-port-invocation");
    let package = claude.package("flow");
    let skill = resource(
        &package,
        CapabilityKind::AgentSkill,
        "skills/review/SKILL.md",
        "---\nname: review\ndescription: Review.\n---\n",
    );

    let label = claude.integration.exposure_name_candidates(&skill);

    assert_eq!(
        format!("{}{}", claude.integration.invocation_prefix(), label[0]),
        "/flow:review"
    );
}

/// Diagnosis sweeps the directory a loose skill is actually delivered into.
#[test]
fn the_skill_discovery_root_is_where_a_loose_skill_lands() {
    let claude = Claude::new("claude-port-discovery-root");
    claude.mark_setup();
    let package = claude.package("flow");
    let skill = resource(
        &package,
        CapabilityKind::AgentSkill,
        "skills/review/SKILL.md",
        "---\nname: review\ndescription: Review.\n---\n",
    );
    let uze_core::exposure::ExposureMechanism::Managed(ManagedArtifact::GeneratedTree {
        path, ..
    }) = claude.integration.exposure_plan(&skill).mechanism
    else {
        panic!("a loose skill is a directory of its own");
    };

    assert_eq!(
        path.parent(),
        claude.integration.skill_discovery_root().as_deref()
    );
}

/// The launch's runtime projection is what carries a project's context to
/// Claude, and the integration says so to the read models.
#[test]
fn claude_declares_the_project_context_its_runtime_projection_carries() {
    let claude = Claude::new("claude-port-projection");
    let project = claude.root.join("project");
    fs::create_dir_all(project.join(".agents/skills")).unwrap();
    uze_testkit::git::mark_repository(&project);
    let ctx = uze_core::harness_runtime::RuntimeContext {
        cwd: &project,
        home: &claude.uze_home,
    };

    let contribution = claude.integration.runtime_contribution(&ctx);

    assert!(!contribution.is_passthrough());
    assert!(claude.integration.runtime_projects_project_context());
}

// ── Doctor ────────────────────────────────────────────────────────────

/// Claude names a user agent after its frontmatter `name`, so a delivered
/// agent filed under another label, or under none, would not be shown as
/// the agent UZE delivered.
#[test]
fn a_user_agent_claude_would_name_otherwise_is_unreadable() {
    let claude = Claude::new("claude-port-user-agent");
    let package = claude.package("kit");
    let agent = resource(
        &package,
        CapabilityKind::Agent,
        "agents/reviewer.md",
        "---\nname: reviewer\n---\n",
    );
    let skill = resource(
        &package,
        CapabilityKind::AgentSkill,
        "skills/review/SKILL.md",
        "---\nname: review\n---\n",
    );
    let delivered = claude.config().join("agents/reviewer.md");
    let receipt = AttachmentReceipt {
        package_id: package.id.as_str().to_owned(),
        resource_identity: Some(agent.identity()),
        integration: "claude-code".to_owned(),
        artifact: ManagedArtifact::GeneratedFile {
            path: delivered.clone(),
            content: String::new(),
        },
    };
    let unreadable = |frontmatter: &str, served: &[&Resource]| {
        claude.write(&delivered, &format!("---\n{frontmatter}\n---\nbody\n"));
        claude
            .integration
            .unreadable(&package, &receipt, served)
            .into_iter()
            .map(|finding| finding.capability)
            .collect::<Vec<_>>()
    };

    assert_eq!(
        unreadable("name: reviewer", &[&agent]),
        Vec::<String>::new()
    );
    assert_eq!(unreadable("name: other", &[&agent]), vec![agent.identity()]);
    assert_eq!(
        unreadable("description: no name", &[&agent]),
        vec![agent.identity()]
    );
    assert_eq!(
        unreadable("name: other", &[&skill]),
        Vec::<String>::new(),
        "only an agent is named by its frontmatter"
    );
}

// ── The plugin and its marketplace ────────────────────────────────────

/// A plugin the person switched off in Claude Code is known to be off, so
/// an update leaves it off, which it does through Claude's own CLI.
// The stand-in for Claude's CLI is a POSIX shell script, which only Unix runs as a program.
#[cfg(unix)]
#[test]
fn a_plugin_switched_off_in_claude_is_known_and_switched_off_again() {
    let claude = Claude::new("claude-port-switched-off");
    let (_env, log) = claude.logging_cli();
    let settings = claude.config().join("settings.json");
    let receipt = plugin_receipt("git@uze-store");

    claude.write(&settings, r#"{"enabledPlugins":{"git@uze-store":false}}"#);
    assert!(claude.integration.switched_off(&receipt));
    claude.write(&settings, r#"{"enabledPlugins":{"git@uze-store":true}}"#);
    assert!(!claude.integration.switched_off(&receipt));

    claude.integration.switch_off(&receipt).unwrap();
    assert_eq!(logged(&log).trim(), "plugin disable git@uze-store");
}

/// Repair reads where Claude has UZE's marketplace registered and moves a
/// registration that names another root back to the vendor store.
// The stand-in for Claude's CLI is a POSIX shell script, which only Unix runs as a program.
#[cfg(unix)]
#[test]
fn a_marketplace_registered_elsewhere_is_read_and_pointed_back() {
    let claude = Claude::new("claude-port-registration");
    let (_env, log) = claude.logging_cli();
    claude.write(
        &claude.config().join("plugins/known_marketplaces.json"),
        r#"{"uze-store":{"source":{"source":"directory","path":"/old/root"},"installLocation":"/old/root"}}"#,
    );

    let registrations = claude.integration.registrations(RegistrationRead::Live);

    assert_eq!(registrations.len(), 1, "{registrations:?}");
    let registration = &registrations[0];
    assert_eq!(
        registration.registered.as_deref(),
        Some(Path::new("/old/root"))
    );
    claude
        .integration
        .repoint_registration(registration)
        .unwrap();
    assert!(
        logged(&log)
            .lines()
            .any(|line| line
                == format!("plugin marketplace add {}", registration.expected.display())),
        "{}",
        logged(&log)
    );
}

// ── MCP servers outside a plugin ──────────────────────────────────────

/// An MCP server registered through `claude mcp add` inspects as matched
/// from `~/.claude.json`, and is detached with `claude mcp remove`.
// The stand-in for Claude's CLI is a POSIX shell script, which only Unix runs as a program.
#[cfg(unix)]
#[test]
fn an_mcp_entry_is_inspected_from_claudes_config_and_removed_through_its_cli() {
    let claude = Claude::new("claude-port-mcp");
    let (_env, log) = claude.logging_cli();
    claude.write(
        &claude.home().join(".claude.json"),
        r#"{"mcpServers":{"uze-x":{"command":"tool","args":["a"]}}}"#,
    );
    let receipt = AttachmentReceipt {
        package_id: "kit".to_owned(),
        resource_identity: Some("mcp:kit/uze-x".to_owned()),
        integration: "claude-code".to_owned(),
        artifact: ManagedArtifact::VendorConfigEntry {
            entry_name: "uze-x".to_owned(),
            transport: "stdio".to_owned(),
            command: PathBuf::from("tool"),
            args: vec!["a".to_owned()],
            cwd: None,
            environment: Vec::new(),
            enabled: None,
        },
    };

    assert_eq!(
        claude.integration.inspect_receipt(&receipt).state,
        AttachmentState::Matched
    );
    assert_eq!(
        claude.integration.detach_receipt(&receipt).unwrap().state,
        AttachmentState::Missing
    );
    assert_eq!(logged(&log).trim(), "mcp remove uze-x");
}

// ── The skills-dir shim ───────────────────────────────────────────────

/// Detaches the reference to `shim` in Claude's skills directory, which
/// leaves the shim unreferenced for cleanup to judge.
// A symbolic link, which Windows lets an ordinary account make only in developer mode.
#[cfg(unix)]
fn detach_reference_to(claude: &Claude, shim: &Path) {
    let skills = claude.config().join("skills");
    fs::create_dir_all(&skills).unwrap();
    let reference = skills.join("uze-example");
    std::os::unix::fs::symlink(shim, &reference).unwrap();
    let receipt = AttachmentReceipt {
        package_id: "example".to_owned(),
        resource_identity: Some("skill:example".to_owned()),
        integration: "claude-code".to_owned(),
        artifact: ManagedArtifact::SymlinkReference {
            path: reference,
            target: shim.to_path_buf(),
        },
    };
    assert_eq!(
        claude.integration.detach_receipt(&receipt).unwrap().state,
        AttachmentState::Missing
    );
}

/// Today's delivery copies the `SKILL.md`: a shim carrying it as a
/// regular file beside its plugin manifest is still UZE's to clean up.
// A symbolic link, which Windows lets an ordinary account make only in developer mode.
#[cfg(unix)]
#[test]
fn an_unreferenced_shim_with_a_copied_skill_is_cleaned_up() {
    let claude = Claude::new("claude-port-shim-copied");
    let shim = claude
        .uze_home
        .generated_attachments_dir("claude")
        .join("uze-example");
    claude.write(&shim.join(".claude-plugin/plugin.json"), "{}");
    claude.write(&shim.join("SKILL.md"), "skill");

    detach_reference_to(&claude, &shim);

    assert!(!shim.exists());
}

/// Without the plugin manifest beside its `SKILL.md`, a directory is not
/// proven to be the shim UZE generated, and stays.
// A symbolic link, which Windows lets an ordinary account make only in developer mode.
#[cfg(unix)]
#[test]
fn an_unreferenced_directory_without_the_shims_manifest_stays() {
    let claude = Claude::new("claude-port-shim-unproven");
    let shim = claude
        .uze_home
        .generated_attachments_dir("claude")
        .join("uze-example");
    claude.write(&shim.join("SKILL.md"), "skill");

    detach_reference_to(&claude, &shim);

    assert!(shim.join("SKILL.md").is_file());
}
