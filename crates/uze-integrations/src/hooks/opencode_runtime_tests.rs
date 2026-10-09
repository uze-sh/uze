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
        opencode_bridge(crate::opencode::HOOKS, &[hook], root, "demo"),
    )
    .unwrap();
    fs::write(
        root.join("drive.ts"),
        format!(
            r#"import plugin from "./hooks-demo.ts";
const hooks = {{}};
await plugin.setup({{
  tool: {{ hook: async (name, fn) => {{ hooks[name] = fn; }} }},
  permission: {{ hook: async (name, fn) => {{ hooks[`permission.${{name}}`] = fn; }} }},
  event: {{ subscribe: () => ({{ [Symbol.asyncIterator]: () => ({{ next: () => new Promise(() => {{}}) }}) }}) }},
  session: {{ synthetic: async () => {{}} }},
}});
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
            command: command.into(),
            args: None,
            interpreter: None,
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
        r#"await hooks["execute.before"]({ tool: "shell", input: { command: "ls" } });"#,
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
        r#"await hooks["execute.before"]({ tool: "shell", input: { command: "ls" } });"#,
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
                command: format!("${{PLUGIN_ROOT}}/scripts/{name}").into(),
                args: None,
                interpreter: None,
                timeout: 10,
            })
            .collect(),
        effect: HookEffect::Observe,
        order: 0,
    };
    let reported = drive(
        &root,
        &hook,
        r#"await hooks["execute.before"]({ tool: "shell", input: { command: "cat .env" } });
await hooks["execute.before"]({ tool: "shell", input: { command: "ls -la" } });
await hooks["execute.before"]({ tool: "read", input: { path: "/x" } });"#,
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

/// A transform group's rewrite is what the tool runs: `execute.before`'s
/// input is replaced, as OpenCode's own input repair does it; an answer
/// that is not an input refuses the call at the permission check.
#[test]
fn a_rewrite_replaces_the_input_the_tool_runs() {
    if !bun_available() {
        eprintln!("bun is not installed; the OpenCode plugin runtime check is skipped");
        return;
    }
    let root = uze_testkit::temp::scratch("opencode-runtime-transform");
    fs::create_dir_all(&root).unwrap();
    let rewriting = |command: &str| PortableHook {
        effect: HookEffect::Transform,
        ..observing(command, 10)
    };
    let rewritten = drive(
        &root,
        &rewriting(r#"printf '{"command":"echo rewritten"}'"#),
        r#"const event = { id: "call-1", tool: "shell", input: { command: "cat .env" } };
await hooks["execute.before"](event);
console.error(`input ${JSON.stringify(event.input)}`);"#,
    );
    assert!(
        rewritten
            .iter()
            .any(|line| line == r#"input {"command":"echo rewritten"}"#),
        "{rewritten:?}"
    );
    let refused = drive(
        &root,
        &rewriting("echo not-an-input"),
        r#"const event = { id: "call-2", tool: "shell", input: { command: "ls" } };
await hooks["execute.before"](event);
const check = { source: { type: "tool", id: "call-2" }, effect: "allow" };
await hooks["permission.evaluate"](check);
console.error(`decision ${check.effect}: ${check.message}`);"#,
    );
    assert!(
        refused
            .iter()
            .any(|line| line.starts_with("decision deny: handler did not write a JSON object")),
        "{refused:?}"
    );
    let _ = fs::remove_dir_all(root);
}

/// The exec form on OpenCode: a JavaScript guard runs in the harness's
/// own runtime, and a Python one is spawned from its words with no shell
/// between; both receive their words intact.
#[test]
fn exec_form_handlers_run_from_their_words_and_javascript_in_the_harnesss_runtime() {
    if !bun_available() {
        eprintln!("bun is not installed; the OpenCode plugin runtime check is skipped");
        return;
    }
    let root = uze_testkit::temp::scratch("opencode-runtime-exec").join("with space");
    fs::create_dir_all(root.join("hooks")).unwrap();
    fs::write(
        root.join("hooks").join("guard.js"),
        "const fs = require('fs');\n\
         fs.writeFileSync(process.env.PLUGIN_ROOT + '/js-argv.txt', process.argv.slice(2).join('\\n'));\n\
         if ((process.env.HOOK_COMMAND || '').includes('.env')) { process.stderr.write('blocked by js'); process.exit(3); }\n",
    )
    .unwrap();
    fs::write(
        root.join("hooks").join("audit.py"),
        "import os, sys\n\
         open(os.path.join(os.environ['PLUGIN_ROOT'], 'py-argv.txt'), 'w').write('\\n'.join(sys.argv[1:]))\n",
    )
    .unwrap();
    let word = r#"it's "$HOME""#;
    let exec = |script: &str| CommandHook {
        handler_type: CommandHandlerType::Command,
        command: script.into(),
        args: Some(vec![word.into()]),
        interpreter: None,
        timeout: 10,
    };
    let hook = PortableHook {
        id: "exec".into(),
        event: HookEvent::PreToolUse,
        matchers: vec![HookMatcher::Portable("shell".into())],
        handlers: vec![exec("hooks/audit.py"), exec("hooks/guard.js")],
        effect: HookEffect::Observe,
        order: 0,
    };
    let reported = drive(
        &root,
        &hook,
        r#"await hooks["execute.before"]({ tool: "shell", input: { command: "cat .env" } });"#,
    );
    assert!(
        reported.iter().any(|line| line.contains("blocked by js")),
        "the JavaScript guard ran and denied: {reported:?}"
    );
    for file in ["js-argv.txt", "py-argv.txt"] {
        assert_eq!(
            fs::read_to_string(root.join(file)).unwrap(),
            word,
            "{file}: the word arrived as written"
        );
    }
    let _ = fs::remove_dir_all(root.parent().unwrap());
}

fn deciding(effect: HookEffect, command: &str) -> PortableHook {
    PortableHook {
        effect,
        ..observing(command, 10)
    }
}

/// `read` asks OpenCode for no permission, so a denial kept for the
/// permission check never met it: the call is refused where its input is
/// seen, by the name OpenCode looks the tool up by.
#[test]
fn a_denial_refuses_a_tool_that_never_asks_for_permission() {
    if !bun_available() {
        eprintln!("bun is not installed; the OpenCode plugin runtime check is skipped");
        return;
    }
    let root = uze_testkit::temp::scratch("opencode-runtime-refuse");
    fs::create_dir_all(&root).unwrap();
    let guard = deciding(
        HookEffect::Deny,
        "case \"$HOOK_PATH\" in *.env*) echo \"blocked: $HOOK_PATH\" >&2; exit 3 ;; esac",
    );
    let reported = drive(
        &root,
        &guard,
        r#"const denied = { id: "call-1", tool: "read", input: { path: "/repo/.env" } };
await hooks["execute.before"](denied);
console.error(`denied ${denied.tool} ${JSON.stringify(denied.input ?? null)}`);
const allowed = { id: "call-2", tool: "read", input: { path: "/repo/README.md" } };
await hooks["execute.before"](allowed);
console.error(`allowed ${allowed.tool} ${JSON.stringify(allowed.input)}`);"#,
    );
    assert!(
        reported
            .iter()
            .any(|line| line == "denied read (refused by a hook: blocked: /repo/.env) null"),
        "{reported:?}"
    );
    assert!(
        reported
            .iter()
            .any(|line| line == r#"allowed read {"path":"/repo/README.md"}"#),
        "{reported:?}"
    );

    let refused = drive(
        &root,
        &deciding(HookEffect::Transform, "echo not-an-input"),
        r#"const event = { id: "call-3", tool: "read", input: { path: "/x" } };
await hooks["execute.before"](event);
console.error(`tool ${event.tool}`);"#,
    );
    assert!(
        refused.iter().any(|line| line
            .starts_with("tool read (refused by a hook: handler did not write a JSON object")),
        "a rewrite that did not happen refuses the call: {refused:?}"
    );
    let _ = fs::remove_dir_all(root);
}

/// The contract's bound on a `HOOK_*` value holds here as in the wrapper:
/// past it a deny group closes rather than starting a handler it cannot
/// hand the context to.
#[test]
fn an_input_too_large_for_the_environment_closes_a_deny_group() {
    if !bun_available() {
        eprintln!("bun is not installed; the OpenCode plugin runtime check is skipped");
        return;
    }
    let root = uze_testkit::temp::scratch("opencode-runtime-oversized");
    fs::create_dir_all(&root).unwrap();
    let reported = drive(
        &root,
        &deciding(HookEffect::Deny, "exit 0"),
        &r#"const event = { id: "call-1", tool: "shell", input: { command: "ls #" + "x".repeat(LIMIT) } };
await hooks["execute.before"](event);
console.error(`tool ${event.tool}`);"#
            .replace("LIMIT", &HOOK_VALUE_LIMIT.to_string()),
    );
    assert!(
        reported
            .iter()
            .any(|line| line.contains("refused by a hook") && line.contains("larger than")),
        "{reported:?}"
    );
    let _ = fs::remove_dir_all(root);
}

/// The deadline ends what the handler started, not only the shell it was
/// started from.
#[test]
fn a_handler_s_children_do_not_outlive_its_deadline() {
    if !bun_available() {
        eprintln!("bun is not installed; the OpenCode plugin runtime check is skipped");
        return;
    }
    let root = uze_testkit::temp::scratch("opencode-runtime-group");
    fs::create_dir_all(&root).unwrap();
    let pid = root.join("child.pid");
    drive(
        &root,
        &observing(
            &format!("sleep 30 & echo $! > '{}'; wait", pid.display()),
            1,
        ),
        r#"await hooks["execute.before"]({ tool: "shell", input: { command: "ls" } });"#,
    );
    let child = fs::read_to_string(&pid).unwrap();
    let running = || {
        let alive = Command::new("ps")
            .args(["-p", child.trim(), "-o", "stat="])
            .output()
            .expect("ps reports whether the child is still running");
        let state = String::from_utf8_lossy(&alive.stdout).trim().to_owned();
        !state.is_empty() && !state.starts_with('Z')
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while running() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        !running(),
        "the handler's child was stopped with it: pid {child}"
    );
    let _ = fs::remove_dir_all(root);
}

/// A package root with a space in it is one word to the handler's shell.
#[test]
fn a_package_root_with_a_space_reaches_the_handler_as_one_word() {
    if !bun_available() {
        eprintln!("bun is not installed; the OpenCode plugin runtime check is skipped");
        return;
    }
    let root = uze_testkit::temp::scratch("opencode-runtime-space").join("with space");
    let scripts = root.join("scripts");
    fs::create_dir_all(&scripts).unwrap();
    let guard = scripts.join("guard");
    fs::write(&guard, "#!/bin/sh\necho \"blocked by guard\" >&2\nexit 3\n").unwrap();
    fs::set_permissions(&guard, fs::Permissions::from_mode(0o755)).unwrap();
    let reported = drive(
        &root,
        &deciding(HookEffect::Deny, "${PLUGIN_ROOT}/scripts/guard"),
        r#"const event = { id: "call-1", tool: "shell", input: { command: "ls" } };
await hooks["execute.before"](event);
console.error(`tool ${event.tool}`);"#,
    );
    assert!(
        reported
            .iter()
            .any(|line| line == "tool shell (refused by a hook: blocked by guard)"),
        "{reported:?}"
    );
    let _ = fs::remove_dir_all(root.parent().unwrap());
}

/// An `ask` is answered by OpenCode's permission prompt, which only a tool
/// that asks for permission puts to the person: about any other tool the
/// call is refused, never run unasked. A tool that asks still asks.
#[test]
fn an_ask_about_a_tool_with_no_permission_prompt_refuses_the_call() {
    if !bun_available() {
        eprintln!("bun is not installed; the OpenCode plugin runtime check is skipped");
        return;
    }
    let root = uze_testkit::temp::scratch("opencode-runtime-ask");
    fs::create_dir_all(&root).unwrap();
    let reported = drive(
        &root,
        &deciding(HookEffect::Ask, "echo 'confirm this' >&2; exit 3"),
        r#"const read = { id: "call-1", tool: "read", input: { path: "/repo/.env" } };
await hooks["execute.before"](read);
console.error(`read ${read.tool}`);
const shell = { id: "call-2", tool: "shell", input: { command: "ls" } };
await hooks["execute.before"](shell);
const check = { source: { type: "tool", id: "call-2" }, effect: "allow" };
await hooks["permission.evaluate"](check);
console.error(`shell ${shell.tool} ${check.effect}: ${check.message}`);"#,
    );
    assert!(
        reported.iter().any(|line| line.starts_with(
            "read read (refused by a hook: confirm this (OpenCode offers no permission prompt for `read`"
        )),
        "{reported:?}"
    );
    assert!(
        reported
            .iter()
            .any(|line| line == "shell shell ask: confirm this"),
        "{reported:?}"
    );
    let _ = fs::remove_dir_all(root);
}
