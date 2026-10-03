use super::*;
use std::{
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
};
use uze_core::hook::{CommandHandlerType, HookEvent};

const TARGETS: [HookTarget; 3] = [
    HookTarget::Claude,
    HookTarget::Codex,
    HookTarget::Antigravity,
];

fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
}

/// A package whose handlers speak the portable contract: `guard` denies
/// a command touching a secret, `audit` records what got through.
fn package(label: &str) -> PathBuf {
    let root = uze_testkit::temp::scratch(label);
    let scripts = root.join("scripts");
    fs::create_dir_all(&scripts).unwrap();
    write_script(
        &scripts.join("guard"),
        "case \"$HOOK_COMMAND\" in\n  *.env*|*id_rsa*)\n    echo \"blocked: $HOOK_COMMAND (tool=$HOOK_TOOL cwd=$HOOK_CWD)\" >&2\n    exit 3 ;;\nesac\nexit 0",
    );
    write_script(
        &scripts.join("audit"),
        "printf '%s\\t%s\\n' \"$HOOK_HARNESS\" \"$HOOK_COMMAND\" >> \"$PLUGIN_ROOT/audit.log\"\nexit 0",
    );
    // A handler that never answers, and does it through a child of its
    // own — the shape a deadline has to survive: killing the shell that
    // started it leaves the child holding the pipe.
    write_script(&scripts.join("stall"), "sh -c 'sleep 30'\nexit 0");
    write_script(
        &scripts.join("refuse"),
        "echo \"refused on $HOOK_EVENT from $HOOK_SOURCE\" >&2\nexit 3",
    );
    root
}

fn write_script(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A handler's command as the manifest carries it: a bare script name
/// is shorthand for this package's own `scripts/<name>`, and anything
/// else — an `sh` invocation, a flag, a relative path — is taken
/// verbatim, because the ABI says a command is a shell command line.
fn handler_command(spec: &str) -> String {
    if spec.contains(' ') || spec.contains('/') {
        spec.to_owned()
    } else {
        format!("${{PLUGIN_ROOT}}/scripts/{spec}")
    }
}

fn group(effect: HookEffect, handlers: &[&str]) -> PortableHook {
    group_at(HookEvent::PreToolUse, effect, handlers, 10)
}

fn group_at(event: HookEvent, effect: HookEffect, handlers: &[&str], timeout: u16) -> PortableHook {
    PortableHook {
        id: "protect-env".into(),
        event,
        matchers: vec![HookMatcher::Portable("shell".into())],
        handlers: handlers
            .iter()
            .map(|spec| CommandHook {
                handler_type: CommandHandlerType::Command,
                command: handler_command(spec),
                timeout,
            })
            .collect(),
        effect,
        order: 0,
    }
}

struct Answer {
    exit: i32,
    stdout: String,
    stderr: String,
}

/// One execution of the wrapper. The package root and the directory the
/// harness happens to be in are separate because a stale entry is
/// exactly the case where they differ.
struct Run<'a> {
    target: HookTarget,
    /// Where the wrapper itself is written.
    wrapper_root: &'a Path,
    /// The root the group's entry names — the wrapper's first argument.
    package_root: &'a Path,
    /// The harness's own working directory, when it matters.
    cwd: Option<&'a Path>,
    hook: &'a PortableHook,
    payload: &'a str,
    jq: Option<&'a str>,
}

/// Runs the generated wrapper exactly as the harness does: the payload
/// on stdin, the group's own arguments on the command line.
fn run_wrapper(
    target: HookTarget,
    root: &Path,
    hook: &PortableHook,
    payload: &str,
    jq: Option<&str>,
) -> Answer {
    run(Run {
        target,
        wrapper_root: root,
        package_root: root,
        cwd: None,
        hook,
        payload,
        jq,
    })
}

fn run(execution: Run<'_>) -> Answer {
    let Run {
        target,
        wrapper_root,
        package_root,
        cwd,
        hook,
        payload,
        jq,
    } = execution;
    let wrapper = wrapper_root.join("hooks").join("exec");
    materialize_wrapper(&wrapper, &wrapper_source(target).unwrap()).unwrap();
    // Through its interpreter, as its shebang names it, rather than by
    // `execve` of a file this process just wrote: a sibling test forking
    // while the write descriptor was open made the kernel answer ETXTBSY.
    let mut command = Command::new("/bin/sh");
    command
        .arg(&wrapper)
        .args(wrapper_arguments(hook, package_root, &hook.handlers));
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    if let Some(jq) = jq {
        command.env("HOOK_JQ", jq);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("cannot start the generated wrapper: {error}"));
    use std::io::Write;
    // A wrapper that denies before reading stdin (a missing dependency)
    // closes the pipe first; that is an answer, not a test failure.
    let _ = child.stdin.take().unwrap().write_all(payload.as_bytes());
    let output = child.wait_with_output().unwrap();
    Answer {
        exit: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// A `stop` payload as each harness sends it: no tool at all, which is
/// what the wrapper has to leave the handler seeing.
fn stop_payload(target: HookTarget) -> String {
    match target {
        HookTarget::Antigravity => serde_json::json!({"workspacePaths": ["/repo"]}).to_string(),
        _ => serde_json::json!({"cwd": "/repo"}).to_string(),
    }
}

/// What Claude Code and Codex hand a `SessionStart` hook: the session's
/// source beside the workspace, and no tool.
fn session_payload() -> String {
    serde_json::json!({
        "hook_event_name": "SessionStart",
        "source": "startup",
        "cwd": "/repo",
    })
    .to_string()
}

fn payload(target: HookTarget, command: &str) -> String {
    match target {
        HookTarget::Antigravity => serde_json::json!({
            "toolCall": {"name": "run_command", "args": {"CommandLine": command, "Cwd": "/repo"}},
            "workspacePaths": ["/repo"],
        })
        .to_string(),
        _ => serde_json::json!({
            "tool_name": if target == HookTarget::Codex { "exec_command" } else { "Bash" },
            "tool_input": if target == HookTarget::Codex {
                serde_json::json!({"cmd": command})
            } else {
                serde_json::json!({"command": command})
            },
            "cwd": "/repo",
        })
        .to_string(),
    }
}

/// What a denial exits with, per harness. Claude and Codex document
/// exit 2 as the block signal; Antigravity reads the decision from
/// stdout and logs any non-zero exit as a *failed* hook, so a denial
/// there exits 0 (measured on 1.1.24).
fn block_exit(target: HookTarget) -> i32 {
    if target == HookTarget::Antigravity {
        0
    } else {
        2
    }
}

#[test]
#[ignore = "regenerates the goldens; run with --ignored after changing the template"]
fn regenerate_goldens() {
    for target in TARGETS {
        fs::create_dir_all(goldens_dir()).unwrap();
        fs::write(
            goldens_dir().join(format!("hooks-exec-{target}.sh")),
            wrapper_source(target).unwrap(),
        )
        .unwrap();
    }
}

/// POSIX runs a trap only once the foreground command returns, so a
/// trap that merely cleaned up would return into the loop and start
/// the next handler for a harness that already gave up on the hook.
#[test]
fn a_terminated_wrapper_runs_no_further_handler() {
    let root = package("wrapper-terminated");
    // The handler says it started before it stalls, so the signal lands
    // inside it — with the wrapper's trap already set — rather than
    // whenever a fixed wait happened to end.
    write_script(
        &root.join("scripts").join("announce-and-stall"),
        "touch \"$PLUGIN_ROOT/started\"\nsh -c 'sleep 30'\nexit 0",
    );
    let hook = group_at(
        HookEvent::PreToolUse,
        HookEffect::Observe,
        &["announce-and-stall", "audit"],
        2,
    );
    let wrapper = root.join("hooks").join("exec");
    materialize_wrapper(&wrapper, &wrapper_source(HookTarget::Claude).unwrap()).unwrap();
    let mut child = Command::new("/bin/sh")
        .arg(&wrapper)
        .args(wrapper_arguments(&hook, &root, &hook.handlers))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| panic!("cannot start the generated wrapper: {error}"));
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin
            .write_all(payload(HookTarget::Claude, "ls").as_bytes())
            .unwrap();
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !root.join("started").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the first handler never started"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let signalled = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(signalled.success());
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(143), "{status:?}");
    assert!(
        !root.join("audit.log").exists(),
        "the handler after the one the signal interrupted never ran"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_wrapper_is_one_byte_identical_file_per_harness() {
    for target in TARGETS {
        let source = wrapper_source(target).expect("every command-hook harness has a wrapper");
        assert_eq!(
            source,
            wrapper_source(target).unwrap(),
            "{target}'s wrapper must be deterministic"
        );
        let golden = goldens_dir().join(format!("hooks-exec-{target}.sh"));
        assert_eq!(
            fs::read_to_string(&golden).unwrap_or_default(),
            source,
            "{} is out of date; regenerate it from wrapper_source",
            golden.display()
        );
        assert!(
            !source.to_lowercase().contains("uze"),
            "nothing in a delivered artifact may name the packager"
        );
    }
}

#[test]
fn a_denial_is_relayed_in_each_harnesss_own_dialect() {
    for target in TARGETS {
        let root = package(&format!("wrapper-deny-{target}"));
        let hook = group(HookEffect::Deny, &["guard", "audit"]);
        let answer = run_wrapper(target, &root, &hook, &payload(target, "cat .env"), None);
        assert_eq!(
            answer.exit,
            block_exit(target),
            "{target}: a denial uses this harness's block signal"
        );
        assert!(
            answer.stderr.contains("blocked: cat .env"),
            "{target}: the reason reaches stderr"
        );
        let document: serde_json::Value = serde_json::from_str(answer.stdout.trim()).unwrap();
        let (decision, reason) = if target == HookTarget::Antigravity {
            (&document["decision"], &document["reason"])
        } else {
            (
                &document["hookSpecificOutput"]["permissionDecision"],
                &document["hookSpecificOutput"]["permissionDecisionReason"],
            )
        };
        assert_eq!(*decision, "deny");
        assert!(reason.as_str().unwrap().contains("blocked: cat .env"));
        assert!(
            reason.as_str().unwrap().contains("tool=shell"),
            "{target}: the handler read the portable alias, not a native name"
        );
        assert!(
            !root.join("audit.log").exists(),
            "{target}: the denial stopped the second handler"
        );
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn an_allowance_lets_the_next_handler_run() {
    for target in TARGETS {
        let root = package(&format!("wrapper-allow-{target}"));
        let hook = group(HookEffect::Deny, &["guard", "audit"]);
        let answer = run_wrapper(target, &root, &hook, &payload(target, "ls -la"), None);
        assert_eq!(answer.exit, 0, "{target}: nothing was denied");
        assert_eq!(
            fs::read_to_string(root.join("audit.log")).unwrap(),
            format!("{target}\tls -la\n"),
            "{target}: the second handler ran and read the portable command"
        );
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn a_handler_that_cannot_run_follows_the_groups_effect() {
    for target in TARGETS {
        let root = package(&format!("wrapper-fail-{target}"));
        let closed = group(HookEffect::Deny, &["absent"]);
        let answer = run_wrapper(target, &root, &closed, &payload(target, "ls"), None);
        assert_eq!(
            answer.exit,
            block_exit(target),
            "{target}: a deny group fails closed"
        );
        assert!(answer.stderr.contains("handler failed"));

        let open = group(HookEffect::Observe, &["absent"]);
        let answer = run_wrapper(target, &root, &open, &payload(target, "ls"), None);
        assert_eq!(answer.exit, 0, "{target}: an observe group fails open");
        assert!(answer.stderr.contains("handler failed"));
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn a_missing_wrapper_dependency_follows_the_groups_effect() {
    for target in TARGETS {
        let root = package(&format!("wrapper-jq-{target}"));
        let closed = group(HookEffect::Deny, &["guard"]);
        let answer = run_wrapper(
            target,
            &root,
            &closed,
            &payload(target, "ls"),
            Some("/nonexistent/jq"),
        );
        assert_eq!(
            answer.exit,
            block_exit(target),
            "{target}: a deny group denies without jq"
        );
        assert!(answer.stderr.contains("jq is not installed"));

        let open = group(HookEffect::Observe, &["guard"]);
        let answer = run_wrapper(
            target,
            &root,
            &open,
            &payload(target, "ls"),
            Some("/nonexistent/jq"),
        );
        assert_eq!(answer.exit, 0, "{target}: an observe group proceeds");
        assert!(answer.stderr.contains("jq is not installed"));
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn a_native_tool_the_vocabulary_does_not_bind_carries_raw_input_only() {
    let root = uze_testkit::temp::scratch("wrapper-native");
    let scripts = root.join("scripts");
    fs::create_dir_all(&scripts).unwrap();
    write_script(
        &scripts.join("probe"),
        "printf '%s|%s|%s' \"$HOOK_TOOL\" \"$HOOK_TOOL_NATIVE\" \"$HOOK_INPUT\" \
             > \"$PLUGIN_ROOT/seen.txt\"\nexit 0",
    );
    let hook = group(HookEffect::Observe, &["probe"]);
    let payload = serde_json::json!({
        "tool_name": "SomeVendorOnlyTool",
        "tool_input": {"anything": "x"},
    })
    .to_string();
    let answer = run_wrapper(HookTarget::Claude, &root, &hook, &payload, None);
    assert_eq!(answer.exit, 0);
    assert_eq!(
        fs::read_to_string(root.join("seen.txt")).unwrap(),
        r#"|SomeVendorOnlyTool|{"anything":"x"}"#
    );
    let _ = fs::remove_dir_all(root);
}

/// The author's `timeout` is the bound the handler actually gets. It is
/// the only bound there is: the native entry's group timeout is the
/// *harness's* backstop, which a harness is free to ignore, and a
/// hanging `deny` handler would otherwise stall every matching tool
/// call for as long as the handler felt like taking.
#[test]
fn a_handler_is_stopped_at_the_deadline_its_author_declared() {
    for target in TARGETS {
        let root = package(&format!("wrapper-timeout-{target}"));
        let closed = group_at(HookEvent::PreToolUse, HookEffect::Deny, &["stall"], 1);
        let started = std::time::Instant::now();
        let answer = run_wrapper(target, &root, &closed, &payload(target, "ls"), None);
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(20),
            "{target}: the 1s bound decided when to stop waiting, not the handler: {elapsed:?}"
        );
        assert_eq!(
            answer.exit,
            block_exit(target),
            "{target}: a deny group whose handler timed out blocks"
        );
        assert!(
            answer.stderr.contains("timed out after 1s"),
            "{target}: the reason names the author's own bound: {}",
            answer.stderr
        );

        let open = group_at(HookEvent::PreToolUse, HookEffect::Observe, &["stall"], 1);
        let answer = run_wrapper(target, &root, &open, &payload(target, "ls"), None);
        assert_eq!(answer.exit, 0, "{target}: an observe group proceeds");
        assert!(answer.stderr.contains("timed out after 1s"));
        let _ = fs::remove_dir_all(root);
    }
}

/// A payload the wrapper cannot read leaves every `HOOK_*` variable
/// empty, and a guard written the documented way (`case "$HOOK_COMMAND"
/// in *"rm -rf"*)`) then sees nothing and allows. The context is the
/// whole basis of the decision, so an unreadable payload is a failure
/// like any other and the group's effect decides.
#[test]
fn a_payload_that_does_not_parse_follows_the_groups_effect() {
    for target in TARGETS {
        let root = package(&format!("wrapper-payload-{target}"));
        let truncated = r#"{"tool_name":"Bash","tool_input":{"command":"cat .env""#;
        let closed = group(HookEffect::Deny, &["guard"]);
        let answer = run_wrapper(target, &root, &closed, truncated, None);
        assert_eq!(
            answer.exit,
            block_exit(target),
            "{target}: a deny group blocks a payload it cannot read"
        );
        assert!(
            answer.stderr.contains("the harness payload is not JSON"),
            "{target}: the reason names what went wrong: {}",
            answer.stderr
        );

        let open = group(HookEffect::Observe, &["guard"]);
        let answer = run_wrapper(target, &root, &open, truncated, None);
        assert_eq!(answer.exit, 0, "{target}: an observe group proceeds");
        assert!(answer.stderr.contains("the harness payload is not JSON"));
        let _ = fs::remove_dir_all(root);
    }
}

/// A handler is a command line relative to the package root — the
/// documented shape, and two recorded fixtures use it. A root that is
/// gone must therefore stop the hook, not leave the handlers running
/// from wherever the harness happened to be: that directory is the
/// user's own checkout, whose content ADR-041 keeps off UZE's
/// execution path.
#[test]
fn a_package_root_that_is_gone_never_runs_the_checkouts_own_script() {
    for target in TARGETS {
        let root = package(&format!("wrapper-root-{target}"));
        let checkout = uze_testkit::temp::scratch(&format!("wrapper-checkout-{target}"));
        fs::create_dir_all(checkout.join("scripts")).unwrap();
        write_script(
            &checkout.join("scripts").join("guard"),
            "touch \"$PWD/ran-the-projects-script\"\nexit 0",
        );
        let gone = root.join("gone");
        let closed = group(HookEffect::Deny, &["scripts/guard"]);
        let answer = run(Run {
            target,
            wrapper_root: &root,
            package_root: &gone,
            cwd: Some(&checkout),
            hook: &closed,
            payload: &payload(target, "ls"),
            jq: None,
        });
        assert_eq!(
            answer.exit,
            block_exit(target),
            "{target}: a deny group whose package is gone blocks"
        );
        assert!(
            answer.stderr.contains("the package root is gone"),
            "{target}: the reason names the missing root: {}",
            answer.stderr
        );
        assert!(
            !checkout.join("ran-the-projects-script").exists(),
            "{target}: the checkout's same-named script must never run"
        );
        let _ = fs::remove_dir_all(checkout);
        let _ = fs::remove_dir_all(root);
    }
}

/// ADR-033's fail-closed set is deny, ask **and** transform: a rewrite
/// that did not happen must not let the original input through as if it
/// had. `transform` degrades rather than being dropped, so the wrapper
/// is the only thing that can hold this.
#[test]
fn a_transform_group_fails_closed_like_a_deny() {
    for target in TARGETS {
        let root = package(&format!("wrapper-transform-{target}"));
        let hook = group(HookEffect::Transform, &["absent"]);
        let answer = run_wrapper(target, &root, &hook, &payload(target, "ls"), None);
        assert_eq!(
            answer.exit,
            block_exit(target),
            "{target}: a handler that cannot run denies for a transform group"
        );
        let _ = fs::remove_dir_all(root);
    }
}

/// "Bounded output" is part of the ABI and the wrapper is the only
/// place left that can hold it: without the bound, a handler that
/// writes megabytes to stderr hands the harness a decision document
/// that big to parse.
#[test]
fn the_reason_a_harness_is_handed_is_bounded() {
    for target in TARGETS {
        let root = package(&format!("wrapper-reason-{target}"));
        write_script(
            &root.join("scripts").join("loud"),
            "head -c 1000000 /dev/zero | tr '\\0' 'x' >&2\nexit 3",
        );
        let hook = group(HookEffect::Deny, &["loud"]);
        let answer = run_wrapper(target, &root, &hook, &payload(target, "ls"), None);
        assert_eq!(
            answer.exit,
            block_exit(target),
            "{target}: the denial stands"
        );
        assert!(
            answer.stdout.len() < HANDLER_REASON_LIMIT * 2,
            "{target}: the decision document carries a bounded reason, not {} bytes",
            answer.stdout.len()
        );
        let _ = fs::remove_dir_all(root);
    }
}

/// The deadline's second pass is the one that matters: a handler (or a
/// child of one) that ignores `TERM` is what `KILL` is for, and the
/// parent cancelling the watchdog the moment the handler dies must not
/// cancel that pass with it. One leaked process per timed-out tool call
/// is what this costs when it is wrong.
#[test]
fn a_handler_that_ignores_term_does_not_outlive_its_deadline() {
    let target = HookTarget::Claude;
    let root = package("wrapper-escalation");
    // `trap '' TERM` is SIG_IGN, which survives the `exec`: the
    // grandchild is a `sleep` that cannot be TERMed, only killed.
    write_script(
        &root.join("scripts").join("stubborn"),
        "trap '' TERM\nsh -c 'trap \"\" TERM; exec sleep 30' &\n\
             printf '%s' \"$!\" > \"$PLUGIN_ROOT/grandchild.pid\"\nsleep 30",
    );
    let hook = group_at(HookEvent::PreToolUse, HookEffect::Deny, &["stubborn"], 1);
    let answer = run_wrapper(target, &root, &hook, &payload(target, "ls"), None);
    assert_eq!(answer.exit, block_exit(target), "the deadline blocks");
    let grandchild = fs::read_to_string(root.join("grandchild.pid")).unwrap();
    // The KILL pass runs on the deadline's own clock and the orphan is
    // reaped by init after it, so the answer is awaited, not assumed.
    let running = || {
        let alive = Command::new("ps")
            .args(["-p", grandchild.trim(), "-o", "pid="])
            .output()
            .expect("ps reports whether the grandchild is still running");
        !String::from_utf8_lossy(&alive.stdout).trim().is_empty()
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while running() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        !running(),
        "a grandchild that ignored TERM is killed, not left behind: pid {grandchild}"
    );
    let _ = fs::remove_dir_all(root);
}

/// A `stop` payload carries no tool at all, and the wrapper has to
/// leave the handler seeing that rather than a stale or invented one.
#[test]
fn a_stop_payload_leaves_the_handler_without_a_tool() {
    for target in TARGETS {
        let root = package(&format!("wrapper-stop-{target}"));
        write_script(
            &root.join("scripts").join("probe"),
            "printf '%s|%s|%s' \"$HOOK_EVENT\" \"$HOOK_TOOL\" \"$HOOK_TOOL_NATIVE\" \
                 > \"$PLUGIN_ROOT/seen.txt\"\nexit 0",
        );
        let hook = group_at(HookEvent::Stop, HookEffect::Observe, &["probe"], 10);
        let answer = run_wrapper(target, &root, &hook, &stop_payload(target), None);
        assert_eq!(answer.exit, 0, "{target}: a stop observation proceeds");
        assert_eq!(
            fs::read_to_string(root.join("seen.txt")).unwrap(),
            "stop||",
            "{target}: no tool is invented for a payload that carries none"
        );
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn a_session_start_hands_the_handler_its_source_and_no_tool() {
    for target in [HookTarget::Claude, HookTarget::Codex] {
        let root = package(&format!("wrapper-session-{target}"));
        write_script(
            &root.join("scripts").join("probe"),
            "printf '%s|%s|%s|%s' \"$HOOK_EVENT\" \"$HOOK_SOURCE\" \"$HOOK_TOOL\" \"$HOOK_CWD\" \
                 > \"$PLUGIN_ROOT/seen.txt\"\nexit 0",
        );
        let hook = group_at(HookEvent::SessionStart, HookEffect::Observe, &["probe"], 10);
        let answer = run_wrapper(target, &root, &hook, &session_payload(), None);
        assert_eq!(answer.exit, 0, "{target}: the session opens");
        assert!(
            answer.stdout.trim().is_empty(),
            "{target}: nothing to decide"
        );
        assert_eq!(
            fs::read_to_string(root.join("seen.txt")).unwrap(),
            "session_start|startup||/repo",
        );
        let _ = fs::remove_dir_all(root);
    }
}

/// A handler answering with the denial code on a session start has
/// nothing to deny: the reason is reported and the session opens, with
/// no decision document a harness could read as one.
#[test]
fn a_denial_on_session_start_is_only_reported() {
    for target in [HookTarget::Claude, HookTarget::Codex] {
        let root = package(&format!("wrapper-session-deny-{target}"));
        let hook = group_at(
            HookEvent::SessionStart,
            HookEffect::Observe,
            &["refuse"],
            10,
        );
        let answer = run_wrapper(target, &root, &hook, &session_payload(), None);
        assert_eq!(answer.exit, 0, "{target}: the session opens");
        assert!(answer.stdout.trim().is_empty(), "{target}: no decision");
        assert!(
            answer
                .stderr
                .contains("refused on session_start from startup"),
            "{target}: the reason is reported: {}",
            answer.stderr
        );
        let _ = fs::remove_dir_all(root);
    }
}

// ========================================================================
// The recorded answers
// ========================================================================

/// One fixture the wrapper is run against: a group, the payload it is
/// fired with, and what the handlers do.
struct Fixture {
    event: HookEvent,
    effect: HookEffect,
    handlers: &'static [&'static str],
    timeout: u16,
    /// The shell command the payload carries. `None` is a `stop`
    /// payload, which carries no tool.
    command: Option<&'static str>,
    /// Whether the payload is one the wrapper cannot read. The context
    /// is the whole basis of a decision, so what happens to a payload
    /// that does not parse is part of the contract.
    malformed_payload: bool,
    /// Whether the group's entry names a package root that is gone — a
    /// stale entry for a package removed, renamed, or a moved `~/.uze`.
    missing_root: bool,
    /// Whether the recorded stderr is the wrapper's own words all the
    /// way. A handler that never started is reported with the system
    /// shell's diagnostic appended, and that wording is the platform's,
    /// so only the head of the line is recorded.
    wrapper_owns_the_whole_reason: bool,
}

/// A payload no harness would send — truncated mid-object, the shape a
/// crashed writer or a wrong-dialect entry produces.
const MALFORMED_PAYLOAD: &str = r#"{"tool_name":"Bash","tool_input":{"command":"cat .env""#;

/// Every fixture, in the order the recorded table holds them.
fn fixtures() -> Vec<Fixture> {
    let case = |event, effect, handlers, command| Fixture {
        event,
        effect,
        handlers,
        timeout: 10,
        command,
        malformed_payload: false,
        missing_root: false,
        wrapper_owns_the_whole_reason: true,
    };
    let pre = HookEvent::PreToolUse;
    vec![
        case(pre, HookEffect::Deny, &["guard", "audit"], Some("cat .env")),
        case(pre, HookEffect::Deny, &["guard", "audit"], Some("ls -la")),
        Fixture {
            wrapper_owns_the_whole_reason: false,
            ..case(pre, HookEffect::Deny, &["absent"], Some("ls"))
        },
        Fixture {
            wrapper_owns_the_whole_reason: false,
            ..case(pre, HookEffect::Observe, &["absent"], Some("ls"))
        },
        // A command line, not an executable path: the shapes the
        // manifest documents and a bare-argv runner cannot start.
        case(
            pre,
            HookEffect::Deny,
            &["sh ${PLUGIN_ROOT}/scripts/guard --strict", "audit"],
            Some("cat .env"),
        ),
        case(
            pre,
            HookEffect::Deny,
            &["sh ${PLUGIN_ROOT}/scripts/guard --strict", "audit"],
            Some("ls -la"),
        ),
        case(pre, HookEffect::Deny, &["scripts/guard"], Some("cat .env")),
        case(
            pre,
            HookEffect::Deny,
            &["scripts/guard", "audit"],
            Some("ls -la"),
        ),
        case(
            HookEvent::PostToolUse,
            HookEffect::Observe,
            &["audit"],
            Some("ls"),
        ),
        case(HookEvent::Stop, HookEffect::Observe, &["audit"], None),
        // A session start decides nothing, whatever its handler answers.
        case(
            HookEvent::SessionStart,
            HookEffect::Observe,
            &["refuse"],
            None,
        ),
        Fixture {
            wrapper_owns_the_whole_reason: false,
            ..case(
                HookEvent::SessionStart,
                HookEffect::Observe,
                &["absent"],
                None,
            )
        },
        // A handler that never answers is a handler failure like any
        // other: the deadline is its author's, and the group's effect
        // decides what that means.
        Fixture {
            timeout: 1,
            ..case(pre, HookEffect::Deny, &["stall"], Some("ls"))
        },
        Fixture {
            timeout: 1,
            ..case(pre, HookEffect::Observe, &["stall"], Some("ls"))
        },
        // The three shapes the wrapper has to answer for without ever
        // reaching the author's handlers: a payload it cannot read, a
        // package root that is gone, and an effect whose rewrite never
        // happened. Each follows the group's effect, so each is
        // recorded both ways round.
        Fixture {
            malformed_payload: true,
            ..case(pre, HookEffect::Deny, &["guard"], Some("cat .env"))
        },
        Fixture {
            malformed_payload: true,
            ..case(pre, HookEffect::Observe, &["guard"], Some("cat .env"))
        },
        Fixture {
            missing_root: true,
            ..case(pre, HookEffect::Deny, &["scripts/guard"], Some("ls"))
        },
        Fixture {
            missing_root: true,
            ..case(pre, HookEffect::Observe, &["scripts/guard"], Some("ls"))
        },
        Fixture {
            wrapper_owns_the_whole_reason: false,
            ..case(pre, HookEffect::Transform, &["absent"], Some("ls"))
        },
    ]
}

fn answers_path() -> PathBuf {
    goldens_dir().join("hooks").join("wrapper-answers.json")
}

/// Runs one fixture through one harness's wrapper and records the
/// answer, with the throwaway package root written back as the
/// placeholder an author would have typed.
fn recorded_answer(target: HookTarget, index: usize, fixture: &Fixture) -> serde_json::Value {
    let root = package(&format!("recorded-{target}-{index}"));
    let hook = group_at(
        fixture.event,
        fixture.effect,
        fixture.handlers,
        fixture.timeout,
    );
    let raw = if fixture.malformed_payload {
        MALFORMED_PAYLOAD.to_owned()
    } else {
        match fixture.command {
            Some(command) => payload(target, command),
            None if fixture.event == HookEvent::SessionStart => session_payload(),
            None => stop_payload(target),
        }
    };
    let package_root = if fixture.missing_root {
        root.join("gone")
    } else {
        root.clone()
    };
    let answer = run(Run {
        target,
        wrapper_root: &root,
        package_root: &package_root,
        cwd: None,
        hook: &hook,
        payload: &raw,
        jq: None,
    });
    let portable = |text: &str| {
        text.trim()
            .replace(&root.display().to_string(), "${PLUGIN_ROOT}")
    };
    let stdout = portable(&answer.stdout);
    let mut case = serde_json::Map::new();
    case.insert("harness".to_owned(), serde_json::json!(target.key()));
    case.insert(
        "event".to_owned(),
        serde_json::json!(fixture.event.abi_name()),
    );
    case.insert(
        "effect".to_owned(),
        serde_json::json!(fixture.effect.abi_name()),
    );
    case.insert(
        "handlers".to_owned(),
        serde_json::json!(fixture.handlers.to_vec()),
    );
    case.insert("command".to_owned(), serde_json::json!(fixture.command));
    if fixture.malformed_payload {
        case.insert("payload".to_owned(), serde_json::json!("malformed"));
    }
    if fixture.missing_root {
        case.insert("package_root".to_owned(), serde_json::json!("missing"));
    }
    case.insert("exit".to_owned(), serde_json::json!(answer.exit));
    let document = if stdout.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&stdout).expect("the wrapper answers with one JSON document")
    };
    case.insert(
        "stdout".to_owned(),
        if fixture.wrapper_owns_the_whole_reason {
            document
        } else {
            without_the_shells_own_words(document)
        },
    );
    let stderr = portable(&answer.stderr);
    if fixture.wrapper_owns_the_whole_reason {
        case.insert("stderr".to_owned(), serde_json::json!(stderr));
    } else {
        case.insert(
            "stderr_prefix".to_owned(),
            serde_json::json!(stderr.split(" — ").next().unwrap_or_default()),
        );
    }
    let _ = fs::remove_dir_all(root);
    serde_json::Value::Object(case)
}

/// A reason the system shell contributed the tail of, cut back to the
/// wrapper's own words: `sh` says "not found" on one platform and "No
/// such file or directory" on another, and neither is a contract.
fn without_the_shells_own_words(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => serde_json::Value::String(
            text.split(" \u{2014} ")
                .next()
                .unwrap_or_default()
                .to_owned(),
        ),
        serde_json::Value::Object(fields) => serde_json::Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key, without_the_shells_own_words(value)))
                .collect(),
        ),
        other => other,
    }
}

fn recorded_answers() -> Vec<serde_json::Value> {
    let fixtures = fixtures();
    let mut answers = Vec::new();
    for target in TARGETS {
        for (index, fixture) in fixtures.iter().enumerate() {
            // An event this harness never fires has no answer to record.
            if !target.capabilities().events.contains(&fixture.event) {
                continue;
            }
            answers.push(recorded_answer(target, index, fixture));
        }
    }
    answers
}

#[test]
#[ignore = "rewrites the recorded answers; run with --ignored and review the diff"]
fn regenerate_wrapper_answers() {
    let table = serde_json::json!({
        "about": "What the generated hooks/exec wrapper answers for every \
                  fixture, per harness: the native decision document, the \
                  exit status and the reason on stderr. Regenerate with \
                  `cargo test -p uze-integrations regenerate_wrapper_answers \
                  -- --ignored`; every changed line is a changed contract \
                  and belongs in the review.",
        "answers": recorded_answers(),
    });
    fs::create_dir_all(answers_path().parent().unwrap()).unwrap();
    fs::write(
        answers_path(),
        format!("{}\n", serde_json::to_string_pretty(&table).unwrap()),
    )
    .unwrap();
}

/// The wrapper is the only implementation of the contract, so what it
/// answers is the contract — recorded once, per harness, per fixture.
///
/// The recorded values were taken from the reference runtime this file
/// used to be tested against (`uze hook-exec`, removed 2026-09-12): each
/// pre-tool fixture below was proven to answer identically on both
/// routes before the runtime was deleted. A golden that changes is a
/// changed contract, and the diff is where that gets reviewed — never a
/// regeneration folded into an unrelated change.
#[test]
fn the_wrapper_answers_every_fixture_as_recorded() {
    let table: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(answers_path()).unwrap_or_default())
            .expect("the recorded answers are readable");
    let expected = table["answers"]
        .as_array()
        .expect("the recorded answers are a list");
    let actual = recorded_answers();
    assert_eq!(
        expected.len(),
        actual.len(),
        "the fixture set changed; regenerate {}",
        answers_path().display()
    );
    for (recorded, answered) in expected.iter().zip(actual) {
        assert_eq!(
            *recorded, answered,
            "{}/{} answered differently than recorded",
            recorded["harness"], recorded["event"]
        );
    }
}
