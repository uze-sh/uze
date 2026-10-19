use super::*;
use uze_core::integration::{AttachmentReceipt, IntegrationPort};
use uze_core::router::CompatibilityRoute;

fn goldens_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

/// The machine every golden is rendered for.
fn golden_home() -> UzeHome {
    UzeHome::at("/machine/state")
}

fn golden_plugin() -> String {
    let home = golden_home();
    plugin::source(
        &crate::opencode::RULES.engine_path(&home),
        &home.rules_sessions_dir(),
    )
}

#[test]
#[ignore = "regenerates the goldens; run with --ignored after changing a template"]
fn regenerate_goldens() {
    fs::write(goldens_dir().join("rules-engine.sh"), engine::SOURCE).unwrap();
    fs::write(goldens_dir().join("rules-opencode.ts"), golden_plugin()).unwrap();
}

// The rendered templates name the engine's path with `/`, and they ship
// only on Unix, where the engine runs; Windows renders none.
#[cfg(unix)]
#[test]
fn each_template_is_its_golden_byte_for_byte_and_stable_across_renders() {
    assert_eq!(
        fs::read_to_string(goldens_dir().join("rules-engine.sh")).unwrap(),
        engine::SOURCE,
        "the engine drifted from its golden; rerun `regenerate_goldens` if that is intended"
    );
    assert_eq!(
        fs::read_to_string(goldens_dir().join("rules-opencode.ts")).unwrap(),
        golden_plugin(),
        "the OpenCode plugin drifted from its golden"
    );
    assert_eq!(golden_plugin(), golden_plugin());
    assert!(engine::SOURCE.starts_with(engine::HEADER));
    assert!(golden_plugin().starts_with(plugin::HEADER));
}

#[test]
fn nothing_in_a_delivered_template_names_the_packager() {
    for (what, source) in [
        ("engine", engine::SOURCE.to_owned()),
        ("plugin", golden_plugin()),
    ] {
        assert!(
            !source.to_lowercase().contains("uze"),
            "nothing in the delivered {what} may name the packager"
        );
    }
}

// The rendered templates name the engine's path with `/`, and they ship
// only on Unix, where the engine runs; Windows renders none.
#[cfg(unix)]
#[test]
fn the_opencode_plugin_imports_nothing_and_rides_the_two_v2_hooks() {
    let plugin = golden_plugin();
    assert!(
        !plugin.contains("import "),
        "a plugin-directory file may import nothing"
    );
    assert!(plugin.contains("export default {"));
    assert!(plugin.contains("ctx.session.hook(\"context\""));
    assert!(plugin.contains("ctx.tool.hook(\"execute.after\""));
    assert!(plugin.contains("Bun.spawn([\"sh\", ENGINE, SESSIONS]"));
    assert!(
        plugin.contains(
            "const ENGINE = \"/machine/state/runtime/attachments/opencode/rules/engine\";"
        )
    );
    assert!(plugin.contains("const SESSIONS = \"/machine/state/runtime/rules/sessions\";"));
}

/// Every harness's integration, over one isolated machine.
struct Machine {
    root: PathBuf,
    home: UzeHome,
    claude: crate::claude::ClaudeIntegration,
    codex: crate::codex::CodexIntegration,
    opencode: crate::opencode::OpenCodeIntegration,
    antigravity: crate::antigravity::AntigravityIntegration,
}

impl Machine {
    fn new(label: &str) -> Self {
        let root = uze_testkit::temp::scratch(label);
        let home = UzeHome::at(root.join("state"));
        Self {
            claude: crate::claude::ClaudeIntegration::new(root.join("claude"), home.clone()),
            codex: crate::codex::CodexIntegration::new(root.join("agents"), home.clone()),
            opencode: crate::opencode::OpenCodeIntegration::new(
                root.join("agents"),
                root.join("opencode/opencode.json"),
                home.clone(),
            ),
            antigravity: crate::antigravity::AntigravityIntegration::new(
                root.join("agents"),
                home.clone(),
            ),
            root,
            home,
        }
    }

    fn claude_settings(&self) -> PathBuf {
        self.root.join("claude/settings.json")
    }

    // Read only by the attach tests, which run on Unix, where the engine does.
    #[cfg(unix)]
    fn codex_hooks(&self) -> PathBuf {
        self.root.join(".codex/hooks.json")
    }

    // Read only by the attach tests, which run on Unix, where the engine does.
    #[cfg(unix)]
    fn opencode_plugin(&self) -> PathBuf {
        self.root.join("opencode/plugins").join(plugin::FILE_NAME)
    }

    // Read only by the attach tests, which run on Unix, where the engine does.
    #[cfg(unix)]
    fn json(path: &Path) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    /// Attaches the engine through `integration` and records each artifact
    /// the way setup does.
    fn attach(&self, integration: &dyn IntegrationPort) -> Vec<AttachmentReceipt> {
        integration
            .attach_rules_engine()
            .unwrap()
            .into_iter()
            .map(|artifact| {
                let ManagedArtifact::IntegrationOwned { selector, .. } = &artifact else {
                    unreachable!("the engine's artifacts are integration-owned");
                };
                let receipt = AttachmentReceipt {
                    package_id: uze_core::state::SETUP_OWNER.to_owned(),
                    resource_identity: Some(format!("rules:{selector}")),
                    integration: integration.id().to_owned(),
                    artifact,
                };
                uze_core::state::record_receipt(&self.home, receipt.clone()).unwrap();
                receipt
            })
            .collect()
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

// Read only by the attach tests, which run on Unix, where the engine does.
#[cfg(unix)]
fn foreign_claude_settings() -> serde_json::Value {
    serde_json::json!({
        "model": "opus",
        "hooks": {
            "PostToolUse": [
                {"matcher": "Write", "hooks": [{"type": "command", "command": "my-formatter"}]}
            ]
        }
    })
}

// The engine is a POSIX `sh` script: a PowerShell platform is delivered
// nothing, which `a_powershell_platform_is_delivered_no_engine` holds.
#[cfg(unix)]
#[test]
fn claude_receives_exec_form_entries_beside_its_foreign_hooks() {
    let machine = Machine::new("rules-claude-attach");
    fs::create_dir_all(machine.claude_settings().parent().unwrap()).unwrap();
    fs::write(
        machine.claude_settings(),
        serde_json::to_string_pretty(&foreign_claude_settings()).unwrap(),
    )
    .unwrap();

    let receipts = machine.attach(&machine.claude);
    assert_eq!(receipts.len(), 2);
    let engine = crate::claude::RULES.engine_path(&machine.home);
    let sessions = machine.home.rules_sessions_dir();
    let settings = Machine::json(&machine.claude_settings());
    assert_eq!(settings["model"], "opus", "foreign keys are kept");
    assert_eq!(
        settings["hooks"]["SessionStart"],
        serde_json::json!([{
            "hooks": [{
                "type": "command",
                "command": engine.display().to_string(),
                "args": [sessions.display().to_string()],
                "timeout": 10,
            }]
        }]),
        "no matcher: every start, a compaction and a clear included"
    );
    let post = settings["hooks"]["PostToolUse"].as_array().unwrap();
    assert_eq!(post.len(), 2);
    assert_eq!(
        post[0],
        foreign_claude_settings()["hooks"]["PostToolUse"][0]
    );
    assert_eq!(post[1]["matcher"], "Read|Edit|MultiEdit|Write|Bash");
    assert_eq!(post[1]["hooks"][0]["command"], engine.display().to_string());
    assert!(engine.is_absolute() && sessions.is_absolute());

    assert_eq!(fs::read_to_string(&engine).unwrap(), engine::SOURCE);
    assert!(uze_platform::executable::is_marked_runnable(&engine));
    for receipt in &receipts {
        assert_eq!(
            machine.claude.inspect_receipt(receipt).state,
            AttachmentState::Matched
        );
    }

    machine.attach(&machine.claude);
    let again = Machine::json(&machine.claude_settings());
    assert_eq!(again, settings, "attaching twice changes nothing");
    assert_eq!(
        uze_core::state::receipts(&machine.home, Some(uze_core::state::SETUP_OWNER))
            .unwrap()
            .len(),
        2,
        "one receipt per entry, however often setup runs"
    );
}

// The engine is a POSIX `sh` script: a PowerShell platform is delivered
// nothing, which `a_powershell_platform_is_delivered_no_engine` holds.
#[cfg(unix)]
#[test]
fn codex_receives_shell_line_entries_in_its_user_hooks_file() {
    let machine = Machine::new("rules-codex-attach");
    let receipts = machine.attach(&machine.codex);
    let engine = crate::codex::RULES.engine_path(&machine.home);
    let hooks = Machine::json(&machine.codex_hooks());
    let line = uze_platform::shell::command_line(
        &engine.display().to_string(),
        &[machine.home.rules_sessions_dir().display().to_string()],
    );
    assert_eq!(
        hooks["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        line
    );
    assert!(hooks["hooks"]["SessionStart"][0].get("matcher").is_none());
    assert_eq!(
        hooks["hooks"]["PostToolUse"][0]["matcher"],
        "Bash|exec_command|shell|apply_patch"
    );
    assert!(
        hooks["hooks"]["PostToolUse"][0]["hooks"][0]
            .get("args")
            .is_none()
    );
    for receipt in &receipts {
        assert_eq!(
            machine.codex.inspect_receipt(receipt).state,
            AttachmentState::Matched
        );
    }
}

// The engine is a POSIX `sh` script: a PowerShell platform is delivered
// nothing, which `a_powershell_platform_is_delivered_no_engine` holds.
#[cfg(unix)]
#[test]
fn opencode_receives_a_generated_plugin_and_no_config_entry() {
    let machine = Machine::new("rules-opencode-attach");
    let plugins = machine.opencode_plugin().parent().unwrap().to_path_buf();
    fs::create_dir_all(&plugins).unwrap();
    fs::write(plugins.join("mine.ts"), "export default {};\n").unwrap();

    let receipts = machine.attach(&machine.opencode);
    assert_eq!(receipts.len(), 1);
    let engine = crate::opencode::RULES.engine_path(&machine.home);
    assert_eq!(
        fs::read_to_string(machine.opencode_plugin()).unwrap(),
        plugin::source(&engine, &machine.home.rules_sessions_dir())
    );
    assert!(
        !machine.root.join("opencode/opencode.json").exists(),
        "the plugin directory is auto-discovered: no `plugin` entry in opencode.json"
    );
    assert_eq!(
        machine.opencode.inspect_receipt(&receipts[0]).state,
        AttachmentState::Matched
    );

    let detached = machine.opencode.detach_receipt(&receipts[0]).unwrap();
    assert_eq!(detached.state, AttachmentState::Missing);
    assert!(!machine.opencode_plugin().exists() && !engine.exists());
    assert_eq!(
        fs::read_to_string(plugins.join("mine.ts")).unwrap(),
        "export default {};\n",
        "the operator's own plugin is untouched"
    );
}

// The engine is a POSIX `sh` script: a PowerShell platform is delivered
// nothing, which `a_powershell_platform_is_delivered_no_engine` holds.
#[cfg(unix)]
#[test]
fn an_operators_plugin_of_the_same_name_is_never_written_over() {
    let machine = Machine::new("rules-opencode-foreign");
    fs::create_dir_all(machine.opencode_plugin().parent().unwrap()).unwrap();
    fs::write(machine.opencode_plugin(), "// mine\n").unwrap();
    let refused = machine.opencode.attach_rules_engine().unwrap_err();
    assert!(
        matches!(refused, UzeError::ManagedEntryConflict(_)),
        "{refused}"
    );
    assert_eq!(
        fs::read_to_string(machine.opencode_plugin()).unwrap(),
        "// mine\n"
    );
}

#[test]
fn antigravity_is_left_to_its_own_rules() {
    let machine = Machine::new("rules-antigravity");
    assert_eq!(
        machine.antigravity.rules_route().route,
        CompatibilityRoute::Native
    );
    assert!(machine.attach(&machine.antigravity).is_empty());
    assert!(
        !machine.home.runtime_dir().exists() && !machine.root.join("agents").exists(),
        "nothing is written for Antigravity"
    );
}

#[test]
fn every_harness_reports_its_route_and_why() {
    let machine = Machine::new("rules-routes");
    let claude = machine.claude.rules_route();
    let codex = machine.codex.rules_route();
    let opencode = machine.opencode.rules_route();
    if deliverable_here() {
        for route in [&claude, &codex, &opencode] {
            assert_eq!(
                route.route,
                CompatibilityRoute::Adaptable,
                "{}",
                route.reason
            );
            assert!(route.needs_engine());
        }
        assert!(
            claude.reason.contains("`.claude/rules/`"),
            "{}",
            claude.reason
        );
        assert!(
            claude.reason.contains("declines to create"),
            "{}",
            claude.reason
        );
        assert!(codex.reason.contains("best effort"), "{}", codex.reason);
    } else {
        for route in [&claude, &codex, &opencode] {
            assert_eq!(route.route, CompatibilityRoute::Unsupported);
            assert_eq!(route.reason, NO_ENGINE_TEMPLATE);
        }
    }
    assert!(!machine.antigravity.rules_route().needs_engine());
}

// The engine is a POSIX `sh` script: a PowerShell platform is delivered
// nothing, which `a_powershell_platform_is_delivered_no_engine` holds.
#[cfg(unix)]
#[test]
fn removal_takes_only_the_engines_entries_and_refuses_an_edited_one() {
    let machine = Machine::new("rules-claude-detach");
    fs::create_dir_all(machine.claude_settings().parent().unwrap()).unwrap();
    fs::write(
        machine.claude_settings(),
        serde_json::to_string(&foreign_claude_settings()).unwrap(),
    )
    .unwrap();
    let receipts = machine.attach(&machine.claude);
    let engine = crate::claude::RULES.engine_path(&machine.home);
    let (start, post) = (&receipts[0], &receipts[1]);

    // The operator edits the engine's tool entry.
    let mut settings = Machine::json(&machine.claude_settings());
    settings["hooks"]["PostToolUse"][1]["matcher"] = serde_json::json!("Read");
    fs::write(machine.claude_settings(), settings.to_string()).unwrap();
    let refused = machine.claude.detach_receipt(post).unwrap();
    assert_eq!(
        refused.state,
        AttachmentState::Drifted,
        "{}",
        refused.reason
    );
    assert_eq!(
        Machine::json(&machine.claude_settings()),
        settings,
        "drift blocks removal: nothing is touched"
    );

    let detached = machine.claude.detach_receipt(start).unwrap();
    assert_eq!(detached.state, AttachmentState::Missing);
    assert!(
        engine.exists(),
        "the engine stays while an entry still starts it"
    );

    // Back as UZE wrote it, the entry goes, and the engine with it.
    let mut settings = Machine::json(&machine.claude_settings());
    settings["hooks"]["PostToolUse"][1]["matcher"] =
        serde_json::json!("Read|Edit|MultiEdit|Write|Bash");
    fs::write(machine.claude_settings(), settings.to_string()).unwrap();
    assert_eq!(
        machine.claude.detach_receipt(post).unwrap().state,
        AttachmentState::Missing
    );
    assert!(!engine.exists());
    assert_eq!(
        Machine::json(&machine.claude_settings()),
        foreign_claude_settings(),
        "only the engine's entries were ever removed"
    );
}

// The executable bit an unrunnable engine loses is a Unix file mode.
#[cfg(unix)]
#[test]
fn a_foreign_or_unrunnable_engine_is_drift() {
    use std::os::unix::fs::PermissionsExt;
    let machine = Machine::new("rules-engine-drift");
    let receipts = machine.attach(&machine.codex);
    let engine = crate::codex::RULES.engine_path(&machine.home);

    fs::set_permissions(&engine, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        machine.codex.inspect_receipt(&receipts[0]).state,
        AttachmentState::Drifted
    );

    fs::write(&engine, "#!/bin/sh\necho mine\n").unwrap();
    fs::set_permissions(&engine, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        machine.codex.inspect_receipt(&receipts[0]).state,
        AttachmentState::Drifted
    );
    assert_eq!(
        machine.codex.detach_receipt(&receipts[0]).unwrap().state,
        AttachmentState::Drifted
    );
    assert_eq!(
        fs::read_to_string(&engine).unwrap(),
        "#!/bin/sh\necho mine\n"
    );

    // One an earlier build wrote is still UZE's own, and setup rewrites it.
    fs::write(&engine, format!("{}; an older build\n", engine::HEADER)).unwrap();
    assert_eq!(
        machine.codex.inspect_receipt(&receipts[0]).state,
        AttachmentState::Matched
    );
    machine.attach(&machine.codex);
    assert_eq!(fs::read_to_string(&engine).unwrap(), engine::SOURCE);
}

// The engine is a POSIX `sh` script: a PowerShell platform is delivered
// nothing, which `a_powershell_platform_is_delivered_no_engine` holds.
#[cfg(unix)]
#[test]
fn an_entry_an_earlier_build_wrote_is_replaced_not_doubled() {
    let machine = Machine::new("rules-codex-upgrade");
    let receipts = machine.attach(&machine.codex);
    // An earlier build wrote a shorter timeout, and its receipt says so.
    let mut hooks = Machine::json(&machine.codex_hooks());
    hooks["hooks"]["SessionStart"][0]["hooks"][0]["timeout"] = serde_json::json!(5);
    fs::write(machine.codex_hooks(), hooks.to_string()).unwrap();
    let mut older = receipts[0].clone();
    if let ManagedArtifact::IntegrationOwned { detail, .. } = &mut older.artifact {
        detail.insert(
            "expected".to_owned(),
            serde_json::Value::String(hooks["hooks"]["SessionStart"][0].to_string()),
        );
    }
    uze_core::state::record_receipt(&machine.home, older).unwrap();

    machine.attach(&machine.codex);
    let start = Machine::json(&machine.codex_hooks())["hooks"]["SessionStart"].clone();
    assert_eq!(start.as_array().unwrap().len(), 1, "{start}");
    assert_eq!(start[0]["hooks"][0]["timeout"], 10);
}

// The Windows half of the gate above: where the engine cannot run, attaching
// writes nothing and reports nothing delivered.
#[cfg(windows)]
#[test]
fn a_powershell_platform_is_delivered_no_engine() {
    let machine = Machine::new("rules-windows-nothing");
    let integrations: [&dyn IntegrationPort; 3] =
        [&machine.claude, &machine.codex, &machine.opencode];
    for integration in integrations {
        assert!(machine.attach(integration).is_empty());
    }
    assert!(!machine.claude_settings().exists());
}
