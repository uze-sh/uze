//! The authoring surface's own contracts: a scaffolded marketplace is born
//! registered and linked, a scaffolded plugin is installable immediately,
//! and the offline check is the same validation an install applies —
//! delivered before one.
//!
//! The scaffold-invariant test lives beside the templates in `uze-core`
//! (`package::authoring::tests`); this suite proves the *orchestration*:
//! what the machine registry and the Store carry once the verbs have run.

use std::{fs, process::Command};
use uze_testkit::{git::isolated_git_in, process::IsolatedHome};

fn uze_bin() -> &'static str {
    env!("CARGO_BIN_EXE_uze")
}

/// Runs `uze` with `UZE_HOME` and `HOME` under `root`, and Git isolated from
/// the ambient configuration — pointing at this world's own `gitconfig`,
/// which carries the identity the scaffold's first commit needs.
fn uze(root: &std::path::Path) -> Command {
    let gitconfig = root.join(".gitconfig");
    if !gitconfig.exists() {
        fs::write(
            &gitconfig,
            "[user]\n\tname = Test\n\temail = t@example.invalid\n",
        )
        .unwrap();
    }
    let mut command = Command::new(uze_bin());
    command
        .env("UZE_HOME", root.join("uze"))
        .isolated_home(root)
        .env("PATH", uze_testkit::process::system_path());
    command
}

/// A scaffolded-and-linked marketplace: created by the CLI, answered about
/// by the registry the machine keeps.
fn create_marketplace(root: &std::path::Path, at: &std::path::Path) -> Command {
    let mut command = uze(root);
    command
        .args(["agent", "market", "create", "tools"])
        .arg("--at")
        .arg(at);
    command
}

#[test]
fn a_scaffolded_marketplace_is_registered_and_linked_in_one_step() {
    let root = uze_testkit::temp::scratch("authoring-cli-market");
    let at = root.join("my-market");
    let created = create_marketplace(&root, &at).output().unwrap();
    assert!(
        created.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    // The bytes are the author's own text, where the agent named.
    assert!(at.join("marketplace.json").is_file());
    assert!(at.join("plugins").is_dir());
    // The identity contract: a Git repository with an initial commit.
    assert!(at.join(".git").exists());
    isolated_git_in(&at, &["rev-parse", "--verify", "HEAD"]);

    // And the machine registry answers for it, linked to the checkout.
    let list = uze(&root)
        .args(["market", "list", "--format", "json"])
        .output()
        .unwrap();
    assert!(list.status.success());
    let registry: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let entry = markets_in(&registry).and_then(|markets| {
        markets
            .iter()
            .find(|market| market["name"] == "tools")
            .cloned()
    });
    let entry = entry.unwrap_or_else(|| panic!("registry does not answer for `tools`: {registry}"));
    assert_eq!(entry["name"], "tools");

    // The refusal: the same name again, from the same directory.
    let again = create_marketplace(&root, &at).output().unwrap();
    assert!(
        !again.status.success(),
        "a marketplace already occupying `--at` is refused"
    );
}

fn markets_in(registry: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    registry
        .get("marketplaces")
        .and_then(|value| {
            if value.is_array() {
                value.as_array().cloned()
            } else {
                None
            }
        })
        .or_else(|| registry.as_array().cloned())
}

#[test]
fn a_scaffolded_plugin_is_installable_before_any_second_commit() {
    let root = uze_testkit::temp::scratch("authoring-cli-plugin");
    let at = root.join("my-market");
    let created = create_marketplace(&root, &at)
        .arg("--description")
        .arg("Test tools")
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "market create failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    let plugin = uze(&root)
        .args([
            "agent", "plugin", "create", "greet", "--market", "tools", "--hook",
        ])
        .output()
        .unwrap();
    assert!(
        plugin.status.success(),
        "plugin create failed: {}",
        String::from_utf8_lossy(&plugin.stderr)
    );
    let plugin_root = at.join("plugins/greet");
    assert!(plugin_root.join("plugin.json").is_file());
    assert!(plugin_root.join("skills/greet/SKILL.md").is_file());
    assert!(plugin_root.join("hooks.json").is_file());
    // One exec-form handler named once: Linux and macOS run `scripts/guard`,
    // Windows its `.ps1` twin, and no pair spells the same thing twice.
    let hooks: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(plugin_root.join("hooks.json")).unwrap()).unwrap();
    let handler = &hooks["hooks"]["PreToolUse"][0]["hooks"][0];
    assert_eq!(handler["command"], "scripts/guard", "{hooks}");
    assert!(handler["args"].is_array(), "{hooks}");
    assert!(plugin_root.join("scripts/guard").is_file());
    assert!(plugin_root.join("scripts/guard.ps1").is_file());
    // The entry that makes it installable, written at scaffold time.
    assert!(
        fs::read_to_string(at.join("marketplace.json"))
            .unwrap()
            .contains("\"greet\"")
    );

    // Check before install — and the check is clean, because the scaffold's
    // own output passes its own validation.
    let check = uze(&root)
        .args(["agent", "plugin", "check"])
        .arg(&plugin_root)
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "the scaffold's own output must pass its check: {}{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(
        String::from_utf8_lossy(&check.stdout).contains("valid Agent Plugins 1.0"),
        "the scaffold is valid under the standard too: {}",
        String::from_utf8_lossy(&check.stdout)
    );

    // And it installs: the linked marketplace reads the working tree, so
    // nothing has been committed since the scaffold's initial commit.
    let install = uze(&root)
        .args(["install", "-m", "greet@tools"])
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "install from the linked marketplace failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );
    assert!(
        root.join("uze/store/plugins/tools/greet/plugin.json")
            .is_file()
            || root.join("uze/store/plugins/greet/plugin.json").is_file(),
        "the Store carries the package"
    );
}

#[test]
fn a_local_marketplace_is_the_project_itself_and_installs_immediately() {
    let root = uze_testkit::temp::scratch("authoring-cli-local");
    // The project is a real repository with a commit — a marketplace is a
    // Git repository with a commit, and here the project's repo is it.
    let project = git_project(&root, true);
    let created = uze(&root)
        .current_dir(&project)
        .args(["agent", "market", "create", "tools", "--local"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "local create failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    // The manifest sits at the project root — the project is the
    // marketplace — and no second commit was fabricated by the scaffold.
    assert!(project.join("marketplace.json").is_file());
    assert!(project.join("plugins").is_dir());
    assert_eq!(
        isolated_git_in(&project, &["rev-list", "--count", "HEAD"]),
        "1",
        "the scaffold committed nothing; the project's flow owns it"
    );

    // A plugin authored into it is installable straight away.
    let plugin = uze(&root)
        .current_dir(&project)
        .args(["agent", "plugin", "create", "greet", "--market", "tools"])
        .output()
        .unwrap();
    assert!(
        plugin.status.success(),
        "plugin create failed: {}",
        String::from_utf8_lossy(&plugin.stderr)
    );
    assert!(project.join("plugins/greet/plugin.json").is_file());
    assert!(
        fs::read_to_string(project.join("marketplace.json"))
            .unwrap()
            .contains("\"greet\"")
    );

    let check = uze(&root)
        .current_dir(&project)
        .args(["agent", "plugin", "check"])
        .arg(project.join("plugins/greet"))
        .output()
        .unwrap();
    assert!(check.status.success(), "scaffold must pass its own check");

    // The install reads the project's working tree through the link.
    let install = uze(&root)
        .current_dir(&project)
        .args(["install", "-m", "greet@tools"])
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "install from the project marketplace failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );

    // A second marketplace in the same project is refused with the fact.
    let again = uze(&root)
        .current_dir(&project)
        .args(["agent", "market", "create", "other", "--local"])
        .output()
        .unwrap();
    assert!(!again.status.success());
    let stderr = String::from_utf8_lossy(&again.stderr);
    assert!(
        stderr.contains("already"),
        "the refusal says the project is already a marketplace: {stderr}"
    );
}

#[test]
fn a_plugin_that_would_fail_at_install_fails_check_first() {
    let root = uze_testkit::temp::scratch("authoring-cli-check");
    let at = root.join("my-market");
    let _ = create_marketplace(&root, &at).output().unwrap();
    let _ = uze(&root)
        .args(["agent", "plugin", "create", "greet", "--market", "tools"])
        .output()
        .unwrap();
    let plugin_root = at.join("plugins/greet");
    fs::write(
        plugin_root.join("plugin.json"),
        r#"{ "name": "-flag", "description": "x" }"#,
    )
    .unwrap();

    let check = uze(&root)
        .args(["agent", "plugin", "check"])
        .arg(&plugin_root)
        .output()
        .unwrap();
    assert!(
        !check.status.success(),
        "an invalid package name is a finding, not a deliverable"
    );
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(
        stdout.contains("name"),
        "the finding names the invalid name: {stdout}"
    );
    // Nothing was installed: the machine's Store holds no such package.
    let list = uze(&root)
        .args(["status", "-m", "--format", "json"])
        .output()
        .unwrap();
    assert!(list.status.success());
    let machine: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let any_greet = machine["packages"]
        .as_array()
        .expect("the machine read model")
        .iter()
        .any(|package| {
            package["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("greet"))
        });
    assert!(!any_greet, "nothing was installed: {machine}");
}

/// A Git repository at `project`, with a commit when `committed`, under
/// the fixture identity and none of the operator's configuration.
fn git_project(root: &std::path::Path, committed: bool) -> std::path::PathBuf {
    let project = root.join("project");
    let git = |arguments: &[&str]| isolated_git_in(&project, arguments);
    fs::create_dir_all(&project).unwrap();
    git(&["init", "-q", "-b", "main"]);
    if committed {
        fs::write(project.join("README.md"), "# demo\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "first"]);
    }
    project
}

fn registry(root: &std::path::Path) -> String {
    fs::read_to_string(uze_core::UzeHome::at(root.join("uze")).marketplaces_path())
        .unwrap_or_default()
}

/// A clone of the project resolves the marketplace it is: the declaration
/// names the project root relative to itself, never this machine's path.
#[test]
fn a_local_marketplace_is_declared_relative_to_the_project() {
    let root = uze_testkit::temp::scratch("authoring-cli-local-declared");
    let project = git_project(&root, true);
    for arguments in [
        &["agent", "market", "create", "tools", "--local"][..],
        &["agent", "plugin", "create", "greet", "--market", "tools"],
        &["greet@tools"],
    ] {
        let output = uze(&root)
            .current_dir(&project)
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let declared = fs::read_to_string(project.join("agents.yaml")).unwrap();
    assert!(declared.contains("path: ."), "{declared}");
    assert!(
        !declared.contains(&*project.to_string_lossy()),
        "the declaration names no machine path: {declared}"
    );
}

#[test]
fn a_local_marketplace_in_a_project_with_no_commit_writes_nothing() {
    let root = uze_testkit::temp::scratch("authoring-cli-local-unborn");
    let project = git_project(&root, false);

    let created = uze(&root)
        .current_dir(&project)
        .args(["agent", "market", "create", "tools", "--local"])
        .output()
        .unwrap();

    assert!(!created.status.success());
    assert!(
        String::from_utf8_lossy(&created.stderr).contains("with a commit"),
        "the refusal names the missing commit: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(!project.join("marketplace.json").exists());
    assert!(!project.join("plugins").exists());
    assert!(!registry(&root).contains("\"tools\""));
}

#[test]
fn a_registered_name_is_refused_before_the_scaffold() {
    let root = uze_testkit::temp::scratch("authoring-cli-taken-name");
    let first = create_marketplace(&root, &root.join("first"))
        .output()
        .unwrap();
    assert!(first.status.success());

    let second_at = root.join("second");
    let second = create_marketplace(&root, &second_at).output().unwrap();

    assert!(!second.status.success());
    assert!(
        !second_at.exists(),
        "the refusal came before the scaffold wrote or committed anything"
    );
}

/// The registry outlives the directory the command ran from, so it records
/// where the marketplace is, not how it was spelled.
#[test]
fn a_relative_at_is_registered_absolute() {
    let root = uze_testkit::temp::scratch("authoring-cli-relative-at");
    let created = uze(&root)
        .current_dir(&root)
        .args(["agent", "market", "create", "tools", "--at", "rel/mk"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(
        String::from_utf8_lossy(&created.stdout).ends_with('\n'),
        "the answer ends its last line"
    );

    let registry: serde_json::Value = serde_json::from_str(&registry(&root)).unwrap();
    let mut recorded = Vec::new();
    collect_strings(&registry, &mut recorded);
    let paths: Vec<_> = recorded
        .iter()
        .filter(|value| value.ends_with(&uze_testkit::process::native("rel/mk")))
        .collect();
    assert!(!paths.is_empty(), "the marketplace is recorded: {registry}");
    assert!(
        paths
            .iter()
            .all(|path| std::path::Path::new(path).is_absolute()),
        "every recorded path is absolute: {registry}"
    );

    let plugin = uze(&root)
        .args(["agent", "plugin", "create", "greet", "--market", "tools"])
        .output()
        .unwrap();
    assert!(plugin.status.success());
    assert!(String::from_utf8_lossy(&plugin.stdout).ends_with('\n'));
}

fn collect_strings(value: &serde_json::Value, into: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => into.push(text.clone()),
        serde_json::Value::Array(values) => values.iter().for_each(|v| collect_strings(v, into)),
        serde_json::Value::Object(entries) => {
            entries.values().for_each(|v| collect_strings(v, into))
        }
        _ => {}
    }
}

/// The authoring verbs hold the name rule before writing anything, and
/// tell the author the name they meant.
#[test]
fn a_name_outside_the_rule_is_refused_before_the_scaffold() {
    let root = uze_testkit::temp::scratch("authoring-cli-name-rule");
    let at = root.join("named");
    let market = uze(&root)
        .args(["agent", "market", "create", "My Market", "--at"])
        .arg(&at)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&market.stderr);
    assert!(
        !market.status.success(),
        "a bad marketplace name must be refused"
    );
    assert!(stderr.contains("try `my-market`"), "{stderr}");
    assert!(!at.exists(), "the refusal wrote nothing");
    assert!(!registry(&root).contains("My Market"));

    let tools = root.join("tools");
    let created = create_marketplace(&root, &tools).output().unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let plugin = uze(&root)
        .args([
            "agent",
            "plugin",
            "create",
            "My_Plugin",
            "--market",
            "tools",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&plugin.stderr);
    assert!(
        !plugin.status.success(),
        "a bad plugin name must be refused"
    );
    assert!(stderr.contains("try `my-plugin`"), "{stderr}");
    assert!(!tools.join("plugins/My_Plugin").exists());
    assert!(!tools.join("plugins/my-plugin").exists());
    let _ = fs::remove_dir_all(root);
}

/// A plugin whose one agent speaks to each harness through its `harness:`
/// block, written into `root/kit` with the given block.
fn agent_plugin(root: &std::path::Path, frontmatter: &str) -> std::path::PathBuf {
    let plugin = root.join("kit");
    fs::create_dir_all(plugin.join("agents/review")).unwrap();
    fs::write(
        plugin.join("plugin.json"),
        r#"{"name":"kit","description":"harness block fixture"}"#,
    )
    .unwrap();
    fs::write(
        plugin.join("agents/review/security.md"),
        format!("---\n{frontmatter}\n---\nReview the diff for security flaws.\n"),
    )
    .unwrap();
    plugin
}

fn check(root: &std::path::Path, plugin: &std::path::Path) -> (bool, String) {
    let output = uze(root)
        .args(["agent", "plugin", "check"])
        .arg(plugin)
        .output()
        .unwrap();
    (
        output.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

#[test]
fn a_harness_block_every_harness_accepts_checks_clean() {
    let root = uze_testkit::temp::scratch("check-harness-clean");
    let plugin = agent_plugin(
        &root,
        "name: security\ndescription: Reviews the diff\nharness:\n  claude-code: { model: haiku, tools: [Read, Grep] }\n  codex: { model: gpt-6-luna, model_reasoning_effort: high }\n  opencode: { model: anthropic/claude-haiku-4-5, tools: { read: true } }",
    );
    let (clean, text) = check(&root, &plugin);
    assert!(clean, "{text}");
    assert!(!text.contains("Delivered short"), "{text}");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn each_harness_adds_its_own_layer_to_the_check() {
    let root = uze_testkit::temp::scratch("check-harness-layers");
    let plugin = agent_plugin(
        &root,
        "name: security\ndescription: Reviews the diff\nmodel: haiku\nharness:\n  opencode: { model: haiku }\n  codex: { nickname: sec }\n  cursor: { model: x }",
    );
    let (clean, text) = check(&root, &plugin);
    assert!(
        !clean,
        "a value OpenCode drops the agent over fails the check: {text}"
    );
    assert!(
        text.contains("OpenCode: `harness.opencode.model` must be `provider/model`"),
        "{text}"
    );
    assert!(
        text.contains("Codex: `harness.codex.nickname` is not a field this harness reads"),
        "{text}"
    );
    assert!(
        text.contains("`harness.cursor` names no harness this build delivers to"),
        "{text}"
    );
    assert!(
        text.contains("`model` at the root is spelled differently by each harness"),
        "{text}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_block_may_not_redefine_what_every_harness_reads() {
    let root = uze_testkit::temp::scratch("check-harness-identity");
    let plugin = agent_plugin(
        &root,
        "name: security\ndescription: Reviews the diff\nharness:\n  claude-code: { name: other }",
    );
    let (clean, text) = check(&root, &plugin);
    assert!(!clean, "{text}");
    assert!(
        text.contains("`harness.claude-code.name` redefines"),
        "{text}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn check_names_what_keeps_a_plugin_from_agent_plugins_and_still_passes() {
    let root = uze_testkit::temp::scratch("check-agent-plugins");
    let plugin = root.join("legacy");
    fs::create_dir_all(plugin.join("skills/review")).unwrap();
    fs::write(plugin.join("plugin.json"), r#"{"name":"legacy"}"#).unwrap();
    fs::write(
        plugin.join("skills/review/SKILL.md"),
        "---\nname: review\ndescription: Reviews\n---\nbody\n",
    )
    .unwrap();
    fs::write(
        plugin.join("mcp.json"),
        r#"{"mcpServers":{"s":{"command":"server"}}}"#,
    )
    .unwrap();
    let (clean, text) = check(&root, &plugin);
    assert!(clean, "uze's own format stays authoritative: {text}");
    assert!(
        text.contains("not yet a valid Agent Plugins 1.0 plugin"),
        "{text}"
    );
    assert!(text.contains("plugin.json: no `$schema`"), "{text}");
    assert!(text.contains("server `s` has no `type`"), "{text}");
    let _ = fs::remove_dir_all(root);
}

/// A session start has nothing to deny: a group declaring `deny` on it
/// fails the check, which names the group and the one effect it may take.
#[test]
fn a_deny_on_session_start_fails_the_check() {
    let root = uze_testkit::temp::scratch("check-session-start-deny");
    let plugin = root.join("kit");
    fs::create_dir_all(&plugin).unwrap();
    fs::write(plugin.join("plugin.json"), r#"{"name":"kit"}"#).unwrap();
    fs::write(
        plugin.join("hooks.json"),
        r#"{"hooks":{"SessionStart":[{"id":"gate","effect":"deny","hooks":[{"type":"command","command":"${PLUGIN_ROOT}/gate"}]}]}}"#,
    )
    .unwrap();
    let (clean, text) = check(&root, &plugin);
    assert!(!clean, "{text}");
    assert!(
        text.contains("hook `gate` declares effect `deny` on SessionStart")
            && text.contains("use effect `observe`"),
        "{text}"
    );
    let _ = fs::remove_dir_all(root);
}

/// `--extension` scaffolds a manifest, a view and its fixture into the new
/// plugin, and the scaffold passes its own check with nothing to warn of:
/// every field the view names is in the fixture.
#[test]
fn a_scaffolded_extension_passes_its_own_check() {
    let root = uze_testkit::temp::scratch("authoring-cli-extension");
    let at = root.join("my-market");
    assert!(
        create_marketplace(&root, &at)
            .output()
            .unwrap()
            .status
            .success()
    );
    let created = uze(&root)
        .args([
            "agent",
            "plugin",
            "create",
            "greet",
            "--market",
            "tools",
            "--extension",
            "board",
        ])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let extension = at.join("plugins/greet/extensions/board");
    for file in [
        "extension.kdl",
        "board.kdl",
        "fixtures/board.json",
        "board.rhai",
        "tests/board.test.kdl",
        "tests/snapshots/board.view.100x30.dark.snap",
    ] {
        assert!(extension.join(file).is_file(), "{file}");
    }

    let (passed, output) = check(&root, &at.join("plugins/greet"));
    assert!(passed, "{output}");
    assert!(output.contains("extension   board"), "{output}");
    assert!(output.contains("3 tests passed"), "{output}");
    assert!(!output.contains('!'), "nothing to warn of: {output}");
    let _ = fs::remove_dir_all(root);
}

/// `--run` runs the scaffold's script in the engine against the checkout
/// named, and a checkout an extension may not read fails the check naming
/// the source that could not answer.
#[test]
fn the_check_runs_a_script_against_a_checkout_with_run() {
    let root = uze_testkit::temp::scratch("authoring-cli-extension-run");
    let at = root.join("my-market");
    assert!(
        create_marketplace(&root, &at)
            .output()
            .unwrap()
            .status
            .success()
    );
    let created = uze(&root)
        .args(["agent", "plugin", "create", "greet", "--market", "tools"])
        .args(["--extension", "board"])
        .output()
        .unwrap();
    assert!(created.status.success());
    let plugin = at.join("plugins/greet");
    // Outside the system's temporary directory, which is never a checkout
    // an extension reads.
    let checkout = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("authoring-run-{}", std::process::id()));
    fs::create_dir_all(&checkout).unwrap();
    isolated_git_in(&checkout, &["init", "-q"]);
    let run = |checkout: &std::path::Path| {
        let output = uze(&root)
            .args(["agent", "plugin", "check"])
            .arg(&plugin)
            .arg("--run")
            .arg("--checkout")
            .arg(checkout)
            .output()
            .unwrap();
        (
            output.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        )
    };

    let (passed, output) = run(&checkout);
    assert!(passed, "{output}");
    assert!(output.contains("board.rhai answered `live`"), "{output}");
    assert!(output.contains("3 tests passed"), "{output}");

    let (passed, output) = run(&root);
    assert!(!passed, "{output}");
    assert!(output.contains("source `live` did not answer"), "{output}");
    let (passed, _) = check(&root, &plugin);
    assert!(passed, "without --run nothing runs, and nothing fails");
    let _ = fs::remove_dir_all(checkout);
    let _ = fs::remove_dir_all(root);
}

/// A test that no longer holds fails the check at its step's line, saying
/// what was drawn; a changed view fails its snapshot until it is written
/// again with `--update-snapshots`.
#[test]
fn a_failing_extension_test_fails_the_check_until_its_snapshot_is_updated() {
    let root = uze_testkit::temp::scratch("authoring-cli-extension-tests");
    let at = root.join("my-market");
    assert!(
        create_marketplace(&root, &at)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        uze(&root)
            .args([
                "agent",
                "plugin",
                "create",
                "greet",
                "--market",
                "tools",
                "--extension",
                "board"
            ])
            .output()
            .unwrap()
            .status
            .success()
    );
    let plugin = at.join("plugins/greet");
    let fixture = plugin.join("extensions/board/fixtures/board.json");
    let written = fs::read_to_string(&fixture).unwrap();
    fs::write(&fixture, written.replace("This week", "Next week")).unwrap();

    let (passed, output) = check(&root, &plugin);
    assert!(!passed, "{output}");
    assert!(
        output.contains("tests/board.test.kdl:5:5: test \"the board opens on its tasks\": `This week` is not drawn"),
        "{output}"
    );

    let test = plugin.join("extensions/board/tests/board.test.kdl");
    fs::write(
        &test,
        fs::read_to_string(&test)
            .unwrap()
            .replace("\"This week\"", "\"Next week\""),
    )
    .unwrap();
    let (passed, output) = check(&root, &plugin);
    assert!(!passed, "the snapshot still holds the old title: {output}");
    assert!(
        output.contains("the cells differ from tests/snapshots/board.view.100x30.dark.snap"),
        "{output}"
    );

    let updated = uze(&root)
        .args(["agent", "plugin", "check"])
        .arg(&plugin)
        .arg("--update-snapshots")
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&updated.stdout);
    assert!(updated.status.success(), "{said}");
    assert!(
        said.contains("wrote tests/snapshots/board.view.100x30.dark.snap"),
        "{said}"
    );
    let (passed, output) = check(&root, &plugin);
    assert!(passed, "{output}");
    let _ = fs::remove_dir_all(root);
}

/// A key an extension suggests for its action reaches that action in a
/// test the way it does in the workspace: through the extension's own
/// table, beside the host's keys.
#[test]
fn a_tests_press_reaches_an_extension_action_by_its_key() {
    let root = uze_testkit::temp::scratch("authoring-cli-extension-press");
    let at = root.join("my-market");
    assert!(
        create_marketplace(&root, &at)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        uze(&root)
            .args([
                "agent",
                "plugin",
                "create",
                "greet",
                "--market",
                "tools",
                "--extension",
                "board"
            ])
            .output()
            .unwrap()
            .status
            .success()
    );
    let plugin = at.join("plugins/greet");
    let extension = plugin.join("extensions/board");
    let manifest = extension.join("extension.kdl");
    let mut written = fs::read_to_string(&manifest).unwrap();
    written.push_str(
        "action \"reload\" label=\"Reload\" on=\"board\" key=\"ctrl-r\" { refresh \"board\"; }\n",
    );
    fs::write(&manifest, written).unwrap();
    let test = extension.join("tests/board.test.kdl");
    let mut written = fs::read_to_string(&test).unwrap();
    written.push_str(
        "\ntest \"its key reloads the board\" {\n    surface \"board\"\n    press \"ctrl+r\"\n    expect-action \"reload\"\n}\n",
    );
    fs::write(&test, written).unwrap();

    // The footer now names the key, so the snapshot is written again.
    let updated = uze(&root)
        .args(["agent", "plugin", "check"])
        .arg(&plugin)
        .arg("--update-snapshots")
        .output()
        .unwrap();
    assert!(
        updated.status.success(),
        "{}{}",
        String::from_utf8_lossy(&updated.stdout),
        String::from_utf8_lossy(&updated.stderr)
    );
    let snapshot =
        fs::read_to_string(extension.join("tests/snapshots/board.view.100x30.dark.snap")).unwrap();
    assert!(snapshot.contains("ctrl+r reload"), "{snapshot}");
    let (passed, output) = check(&root, &plugin);
    assert!(passed, "{output}");
    assert!(output.contains("4 tests passed"), "{output}");
    let _ = fs::remove_dir_all(root);
}

/// An id a built-in extension answers to is refused, and nothing written.
#[test]
fn a_reserved_extension_id_is_refused_before_the_scaffold() {
    let root = uze_testkit::temp::scratch("authoring-cli-extension-reserved");
    let at = root.join("my-market");
    assert!(
        create_marketplace(&root, &at)
            .output()
            .unwrap()
            .status
            .success()
    );
    let refused = uze(&root)
        .args([
            "agent",
            "plugin",
            "create",
            "greet",
            "--market",
            "tools",
            "--extension",
            "code",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("reserved"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(!at.join("plugins/greet").exists());
    let _ = fs::remove_dir_all(root);
}

/// What would keep an extension from being hosted fails the check, located
/// under its id; a field the view names that its fixture lacks is a
/// warning, placed in the view, and passes.
#[test]
fn check_places_an_extensions_errors_and_what_its_fixture_lacks() {
    let root = uze_testkit::temp::scratch("authoring-cli-extension-check");
    let at = root.join("my-market");
    assert!(
        create_marketplace(&root, &at)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        uze(&root)
            .args([
                "agent",
                "plugin",
                "create",
                "greet",
                "--market",
                "tools",
                "--extension",
                "board",
            ])
            .output()
            .unwrap()
            .status
            .success()
    );
    let plugin = at.join("plugins/greet");
    // What a warning does to the check alone, without the scaffold's tests,
    // which expect the field this renames.
    fs::remove_dir_all(plugin.join("extensions/board/tests")).unwrap();
    let view = plugin.join("extensions/board/board.kdl");
    let written = fs::read_to_string(&view).unwrap();
    fs::write(
        &view,
        written.replace("Text \"{name}\"", "Text \"{label}\""),
    )
    .unwrap();

    let (passed, output) = check(&root, &plugin);
    assert!(passed, "a warning does not fail the check: {output}");
    assert!(output.contains("extension board"), "{output}");
    assert!(
        output.contains("board.kdl:24:9: `label` is not in the fixture fixtures/board.json"),
        "{output}"
    );

    fs::write(
        &view,
        written.replace("action=\"refresh\"", "action=\"refresh\" colour=\"red\""),
    )
    .unwrap();
    let (passed, output) = check(&root, &plugin);
    assert!(!passed, "{output}");
    assert!(
        output.contains("board.kdl:18:") && output.contains("no property `colour`"),
        "{output}"
    );
    assert!(output.contains("check failed (1 problem)"), "{output}");
    let _ = fs::remove_dir_all(root);
}

/// The manifest's grants, sources and actions are checked as the install
/// checks them: a grant a later change brings, a program, a source asked
/// too often and a confirmed action on a bare key each fail the check,
/// placed in `extension.kdl`.
#[test]
fn check_reports_what_a_manifest_may_not_run_yet() {
    let root = uze_testkit::temp::scratch("authoring-cli-extension-grants");
    let at = root.join("my-market");
    assert!(
        create_marketplace(&root, &at)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        uze(&root)
            .args([
                "agent",
                "plugin",
                "create",
                "greet",
                "--market",
                "tools",
                "--extension",
                "board",
            ])
            .output()
            .unwrap()
            .status
            .success()
    );
    let plugin = at.join("plugins/greet");
    let manifest = plugin.join("extensions/board/extension.kdl");
    let written = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        written.replace("every \"30s\"", "every \"2s\"")
            + "wants \"net\" \"write:checkout\"\n"
            + "source \"meter\" { run \"./bin/meter\"; }\n"
            + "action \"wipe\" label=\"Wipe\" on=\"board\" key=\"w\" confirm=\"Wipe it?\" { refresh \"live\"; }\n",
    )
    .unwrap();

    let (passed, output) = check(&root, &plugin);
    assert!(!passed, "{output}");
    for said in [
        "`every` is at least 5s",
        "`net` arrives in a later change (change 8)",
        "`write:checkout` does not exist",
        "the program `bin/meter`",
        "its key needs a modifier, not a bare `w`",
    ] {
        assert!(output.contains(said), "{said}: {output}");
    }
    assert!(output.contains("check failed (5 problems)"), "{output}");
    let _ = fs::remove_dir_all(root);
}

/// The language server exits once the editor says `shutdown` and `exit`,
/// as an editor expects of it, rather than waiting on its own output.
#[test]
fn the_language_server_exits_after_shutdown() {
    use std::io::Write as _;
    let root = uze_testkit::temp::scratch("authoring-lsp-exit");
    let message = |body: serde_json::Value| {
        let body = body.to_string();
        format!("Content-Length: {}\r\n\r\n{body}", body.len())
    };
    let session: String = [
        serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"capabilities": {}}}),
        serde_json::json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "shutdown"}),
        serde_json::json!({"jsonrpc": "2.0", "method": "exit"}),
    ]
    .into_iter()
    .map(message)
    .collect();
    let mut server = uze(&root)
        .args(["agent", "plugin", "lsp"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    server
        .stdin
        .take()
        .unwrap()
        .write_all(session.as_bytes())
        .unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(server.wait_with_output());
    });
    let output = receiver
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("the server exits")
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"hoverProvider\":true"));
    let _ = fs::remove_dir_all(root);
}

/// A snapshot a plugin links elsewhere is never written through, and what
/// a snapshot file says reaches the terminal cleaned. Unix: the link is
/// made with `std::os::unix::fs::symlink`, which Windows grants only to
/// an elevated or developer-mode account.
#[cfg(unix)]
#[test]
fn a_linked_snapshot_is_refused_and_a_held_one_is_said_cleaned() {
    let root = uze_testkit::temp::scratch("authoring-snapshot-link");
    let at = root.join("my-market");
    assert!(
        create_marketplace(&root, &at)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        uze(&root)
            .args([
                "agent",
                "plugin",
                "create",
                "greet",
                "--market",
                "tools",
                "--extension",
                "board"
            ])
            .output()
            .unwrap()
            .status
            .success()
    );
    let plugin = at.join("plugins/greet");
    let snapshot = plugin.join("extensions/board/tests/snapshots/board.view.100x30.dark.snap");

    fs::write(&snapshot, "\u{1b}]52;c;ZXZpbA==\u{7}stolen\n").unwrap();
    let (passed, output) = check(&root, &plugin);
    assert!(!passed, "{output}");
    assert!(
        !output.contains('\u{1b}'),
        "no escape reaches the terminal: {output:?}"
    );
    assert!(output.contains("stolen"), "{output}");

    let outside = root.join("operator-file");
    fs::write(&outside, "mine").unwrap();
    fs::remove_file(&snapshot).unwrap();
    std::os::unix::fs::symlink(&outside, &snapshot).unwrap();
    let updated = uze(&root)
        .args(["agent", "plugin", "check"])
        .arg(&plugin)
        .arg("--update-snapshots")
        .output()
        .unwrap();
    assert!(!updated.status.success());
    assert!(
        String::from_utf8_lossy(&updated.stdout).contains("reached through a link"),
        "{}",
        String::from_utf8_lossy(&updated.stdout)
    );
    assert_eq!(fs::read_to_string(&outside).unwrap(), "mine");
    let _ = fs::remove_dir_all(root);
}

/// An extension's directory name is the plugin's, so the check says it
/// cleaned, whatever escape it was given. Unix: Windows refuses a control
/// character in a file name, so no such directory can exist there.
#[cfg(unix)]
#[test]
fn an_extension_directory_name_reaches_the_terminal_cleaned() {
    let root = uze_testkit::temp::scratch("authoring-hostile-id");
    let plugin = root.join("legacy");
    let extension = plugin.join("extensions/x\u{1b}[2Jy");
    fs::create_dir_all(&extension).unwrap();
    fs::write(plugin.join("plugin.json"), r#"{"name":"legacy"}"#).unwrap();
    fs::write(extension.join("extension.kdl"), "protocol 1\n").unwrap();
    let (passed, output) = check(&root, &plugin);
    assert!(!passed, "{output}");
    assert!(!output.contains('\u{1b}'), "{output:?}");
    assert!(
        output.contains("extension xy"),
        "the sequence is dropped whole: {output}"
    );
    let _ = fs::remove_dir_all(root);
}

/// Every fixture plugin that declares extensions checks clean, its tests
/// and snapshots included: a snapshot a catalog change left behind fails
/// here, not in an author's hands.
#[test]
fn every_fixture_plugins_extensions_check_clean() {
    let root = uze_testkit::temp::scratch("check-fixture-extensions");
    let canonical = uze_testkit::fixtures::canonical("gsd")
        .parent()
        .unwrap()
        .to_path_buf();
    let mut checked = Vec::new();
    for entry in fs::read_dir(&canonical).unwrap() {
        let plugin = entry.unwrap().path();
        if !plugin.join("extensions").is_dir() {
            continue;
        }
        let (clean, text) = check(&root, &plugin);
        assert!(clean, "{}: {text}", plugin.display());
        checked.push(plugin.file_name().unwrap().to_string_lossy().into_owned());
    }
    checked.sort();
    assert!(
        ["bridge", "deck", "gpu", "gsd", "media"]
            .iter()
            .all(|name| checked.iter().any(|checked| checked == name)),
        "{checked:?}"
    );
    let _ = fs::remove_dir_all(root);
}
