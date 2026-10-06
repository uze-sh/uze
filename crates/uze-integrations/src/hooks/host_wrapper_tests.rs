//! The wrapper this platform's harnesses run, run the way they run it: the
//! harness's payload as bytes on stdin, the group's arguments after the
//! interpreter the entry names. The cases are the ones a guard's safety
//! rests on, so they hold for every wrapper template, on its own platform.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use uze_core::hook::{
    CommandHandlerType, CommandHook, HookEffect, HookEvent, HookMatcher, PortableHook,
};
use uze_core::shell::ShellCommand;

use super::wrapper::{WRAPPER_RELATIVE_PATH, wrapper_arguments, wrapper_source};

/// A package with three handlers, each written for both shells: `guard`
/// refuses `rm` and allows the rest, reading the command out of the
/// payload; `broken` fails without deciding.
fn package(label: &str) -> PathBuf {
    let root = uze_testkit::temp::scratch(label);
    let scripts = root.join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    let install = |name: &str, posix: &str, windows: &str| {
        uze_testkit::process::install_executable(
            &scripts.join(name),
            format!("#!/bin/sh\n{posix}\n").as_bytes(),
        );
        std::fs::write(scripts.join(format!("{name}.ps1")), windows).unwrap();
    };
    install(
        "guard",
        r#"case "$HOOK_COMMAND" in rm\ *) echo "no rm here: $HOOK_COMMAND" >&2; exit 3;; esac"#,
        "if ($env:HOOK_COMMAND -like 'rm *') { [Console]::Error.WriteLine(\"no rm here: $env:HOOK_COMMAND\"); exit 3 }\n",
    );
    install("broken", "exit 1", "exit 1\n");
    root
}

fn group(effect: HookEffect, handler: &str) -> PortableHook {
    PortableHook {
        id: "guard".into(),
        event: HookEvent::PreToolUse,
        matchers: vec![HookMatcher::Portable("shell".into())],
        handlers: vec![CommandHook {
            handler_type: CommandHandlerType::Command,
            command: ShellCommand::spelled(
                format!("${{PLUGIN_ROOT}}/scripts/{handler}"),
                format!("& \"${{PLUGIN_ROOT}}/scripts/{handler}.ps1\""),
            ),
            timeout: 10,
        }],
        effect,
        order: 0,
    }
}

struct Answer {
    exit: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Claude Code's wrapper, fired with `payload` as Claude Code fires it.
fn fire(root: &Path, hook: &PortableHook, payload: &[u8]) -> Answer {
    let wrapper = root.join(WRAPPER_RELATIVE_PATH);
    std::fs::create_dir_all(wrapper.parent().unwrap()).unwrap();
    let source = wrapper_source(crate::claude::HOOKS).expect("Claude Code has a wrapper here");
    uze_testkit::process::install_executable(&wrapper, source.as_bytes());
    let (program, mut arguments) = uze_platform::shell::script(&wrapper.display().to_string());
    arguments.extend(wrapper_arguments(hook, root, &hook.handlers));
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the wrapper starts");
    let _ = child.stdin.take().unwrap().write_all(payload);
    let output = child.wait_with_output().unwrap();
    Answer {
        exit: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn shell_payload(command: &str) -> Vec<u8> {
    serde_json::json!({
        "session_id": "s",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": command},
        "cwd": "/repo",
    })
    .to_string()
    .into_bytes()
}

/// A denial in this host harness's dialect: the decision on stdout, with
/// exit 0 (Claude reads stdout only then).
fn denied(answer: &Answer) -> bool {
    answer.exit == Some(0) && answer.stdout.contains(r#""permissionDecision":"deny""#)
}

#[test]
fn a_guard_reads_the_payload_and_denies_with_its_reason() {
    let root = package("host-wrapper-deny");
    let answer = fire(
        &root,
        &group(HookEffect::Deny, "guard"),
        &shell_payload("rm -rf build"),
    );
    assert!(denied(&answer), "{}\n{}", answer.stdout, answer.stderr);
    assert!(
        answer.stdout.contains("no rm here: rm -rf build"),
        "the handler's reason reaches the harness: {}",
        answer.stdout
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_guard_allows_what_it_does_not_refuse() {
    let root = package("host-wrapper-allow");
    let answer = fire(
        &root,
        &group(HookEffect::Deny, "guard"),
        &shell_payload("ls -la"),
    );
    assert_eq!(answer.exit, Some(0), "{}\n{}", answer.stdout, answer.stderr);
    assert!(!answer.stdout.contains("deny"), "{}", answer.stdout);
    let _ = std::fs::remove_dir_all(root);
}

/// A guard that fails, and a payload the wrapper cannot read, deny: the
/// harness reads any other exit as an error that lets the tool through.
#[test]
fn a_guard_that_cannot_decide_denies() {
    let root = package("host-wrapper-closed");
    let broken = fire(
        &root,
        &group(HookEffect::Deny, "broken"),
        &shell_payload("ls"),
    );
    assert!(denied(&broken), "{}\n{}", broken.stdout, broken.stderr);
    let unreadable = fire(
        &root,
        &group(HookEffect::Deny, "guard"),
        br#"{"tool_name":"Bash","tool_input":{"#,
    );
    assert!(
        denied(&unreadable),
        "{}\n{}",
        unreadable.stdout,
        unreadable.stderr
    );
    let _ = std::fs::remove_dir_all(root);
}

/// An observing group's failure is reported and lets the tool through.
#[test]
fn an_observer_that_fails_allows() {
    let root = package("host-wrapper-observe");
    let answer = fire(
        &root,
        &group(HookEffect::Observe, "broken"),
        &shell_payload("ls"),
    );
    assert_eq!(answer.exit, Some(0), "{}\n{}", answer.stdout, answer.stderr);
    let _ = std::fs::remove_dir_all(root);
}
