use super::*;
use std::{os::unix::fs::PermissionsExt, process::Command};
use uze_core::hook::{CommandHandlerType, HookEvent};

fn bun_available() -> bool {
    Command::new("bun")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Loads the plugin generated for `hook` under `root` and runs `calls`
/// against it, answering with what it reported on the console.
fn drive(root: &Path, hook: &PortableHook, calls: &str) -> Vec<String> {
    fs::write(
        root.join("hooks-demo.ts"),
        opencode_bridge(&[hook], root, "demo"),
    )
    .unwrap();
    fs::write(
        root.join("drive.ts"),
        format!(
            r#"import plugin from "./hooks-demo.ts";
const hooks = {{}};
await plugin.setup({{ tool: {{ hook: async (name, fn) => {{ hooks[name] = fn; }} }} }});
const errors = [];
console.error = (...parts) => errors.push(parts.join(" "));
{calls}
console.log(JSON.stringify(errors));
"#
        ),
    )
    .unwrap();
    let output = Command::new("bun")
        .arg("run")
        .arg("drive.ts")
        .current_dir(root)
        .output()
        .expect("bun runs the driver");
    assert!(
        output.status.success(),
        "the plugin must load and run: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap()
}

fn observing(command: &str, timeout: u16) -> PortableHook {
    PortableHook {
        id: "observe".into(),
        event: HookEvent::PreToolUse,
        matchers: Vec::new(),
        handlers: vec![CommandHook {
            handler_type: CommandHandlerType::Command,
            command: command.to_owned(),
            timeout,
        }],
        effect: HookEffect::Observe,
        order: 0,
    }
}

/// Stopping the handler's shell does not close a pipe something it
/// started still holds; waiting for that pipe to close is waiting for
/// the grandchild, however long it lives.
#[test]
fn a_handler_whose_child_holds_stderr_is_still_stopped_at_its_deadline() {
    if !bun_available() {
        eprintln!("bun is not installed; the OpenCode plugin runtime check is skipped");
        return;
    }
    let root = uze_testkit::temp::scratch("opencode-runtime-deadline");
    fs::create_dir_all(&root).unwrap();
    let started = std::time::Instant::now();
    let reported = drive(
        &root,
        &observing("sleep 8 & sleep 8", 1),
        r#"await hooks["execute.before"]({ tool: "bash", input: { command: "ls" } });"#,
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(6),
        "the deadline bounds the answer, not only the shell: {:?}",
        started.elapsed()
    );
    assert!(
        reported
            .iter()
            .any(|line| line.contains("handler timed out after 1s")),
        "{reported:?}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_reason_the_plugin_reports_is_bounded() {
    if !bun_available() {
        eprintln!("bun is not installed; the OpenCode plugin runtime check is skipped");
        return;
    }
    let root = uze_testkit::temp::scratch("opencode-runtime-bound");
    fs::create_dir_all(&root).unwrap();
    let reported = drive(
        &root,
        &observing("head -c 200000 /dev/zero | tr '\\0' x >&2; exit 1", 10),
        r#"await hooks["execute.before"]({ tool: "bash", input: { command: "ls" } });"#,
    );
    let reason = reported
        .iter()
        .find(|line| line.contains("handler failed (exit 1)"))
        .unwrap_or_else(|| panic!("{reported:?}"));
    let stderr = reason.rsplit(" — ").next().unwrap();
    assert_eq!(
        stderr,
        "x".repeat(HANDLER_REASON_LIMIT),
        "only the first {HANDLER_REASON_LIMIT} bytes of stderr become the reason"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_plugin_runs_the_handlers_on_the_harnesss_own_runtime() {
    if !bun_available() {
        eprintln!("bun is not installed; the OpenCode plugin runtime check is skipped");
        return;
    }
    let root = uze_testkit::temp::scratch("opencode-runtime");
    let scripts = root.join("scripts");
    fs::create_dir_all(&scripts).unwrap();
    for (name, body) in [
        (
            "guard",
            "case \"$HOOK_COMMAND\" in\n  *.env*)\n    echo \"blocked: $HOOK_COMMAND\" >&2\n    exit 3 ;;\nesac\nexit 0",
        ),
        (
            "audit",
            "printf '%s|%s|%s\\n' \"$HOOK_HARNESS\" \"$HOOK_TOOL\" \"$HOOK_COMMAND\" \
                 >> \"$PLUGIN_ROOT/audit.log\"\nexit 0",
        ),
    ] {
        let path = scripts.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let hook = PortableHook {
        id: "protect-env".into(),
        event: HookEvent::PreToolUse,
        matchers: vec![HookMatcher::Portable("shell".into())],
        handlers: ["guard", "audit"]
            .into_iter()
            .map(|name| CommandHook {
                handler_type: CommandHandlerType::Command,
                command: format!("${{PLUGIN_ROOT}}/scripts/{name}"),
                timeout: 10,
            })
            .collect(),
        effect: HookEffect::Observe,
        order: 0,
    };
    let reported = drive(
        &root,
        &hook,
        r#"await hooks["execute.before"]({ tool: "bash", input: { command: "cat .env" } });
await hooks["execute.before"]({ tool: "bash", input: { command: "ls -la" } });
await hooks["execute.before"]({ tool: "read", input: { filePath: "/x" } });"#,
    );
    assert!(
        reported
            .iter()
            .any(|line| line.contains("blocked: cat .env")),
        "the denial reason is reported: {reported:?}"
    );
    assert_eq!(
        fs::read_to_string(root.join("audit.log")).unwrap(),
        "opencode|shell|ls -la\n",
        "the second handler ran only for the allowed call, with the portable context"
    );
    let _ = fs::remove_dir_all(root);
}
