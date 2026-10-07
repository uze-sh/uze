//! Every fixture through the wrapper this platform's harnesses run, held
//! to the answer recorded for the POSIX wrapper: the same exit, the same
//! decision document and the same reason, wherever the words are the
//! wrapper's or the handler's. Only the shell's own words differ: how a
//! handler line is spelled, and the code a shell gives a command it cannot
//! find, which are a platform's and no contract.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use uze_core::hook::{CommandHandlerType, CommandHook, HookMatcher, PortableHook};
use uze_core::shell::ShellCommand;

use super::HookTarget;
use super::fixture_set::*;
use super::wrapper::{WRAPPER_RELATIVE_PATH, wrapper_arguments, wrapper_source};

/// The fixture set's handlers, each written for both shells.
fn package(label: &str) -> std::path::PathBuf {
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
        "case \"$HOOK_COMMAND\" in\n  *.env*|*id_rsa*)\n    echo \"blocked: $HOOK_COMMAND (tool=$HOOK_TOOL cwd=$HOOK_CWD)\" >&2\n    exit 3 ;;\nesac\nexit 0",
        "if ($env:HOOK_COMMAND -match '\\.env|id_rsa') {\n  [Console]::Error.WriteLine(\"blocked: $env:HOOK_COMMAND (tool=$env:HOOK_TOOL cwd=$env:HOOK_CWD)\")\n  exit 3\n}\nexit 0\n",
    );
    install(
        "audit",
        "printf '%s\\t%s\\n' \"$HOOK_HARNESS\" \"$HOOK_COMMAND\" >> \"$PLUGIN_ROOT/audit.log\"\nexit 0",
        "Add-Content -LiteralPath \"$env:PLUGIN_ROOT/audit.log\" -Value \"$env:HOOK_HARNESS`t$env:HOOK_COMMAND\"\nexit 0\n",
    );
    install(
        "stall",
        "sh -c 'sleep 30'\nexit 0",
        "powershell.exe -NoProfile -Command 'Start-Sleep -Seconds 30'\nexit 0\n",
    );
    install(
        "refuse",
        "echo \"refused on $HOOK_EVENT from $HOOK_SOURCE\" >&2\nexit 3",
        "[Console]::Error.WriteLine(\"refused on $env:HOOK_EVENT from $env:HOOK_SOURCE\")\nexit 3\n",
    );
    install(
        "rewrite",
        "printf '%s' \"$HOOK_INPUT\" | jq -c 'if has(\"CommandLine\") then .CommandLine = \"echo rewritten\" else .command = \"echo rewritten\" end'",
        "$input = $env:HOOK_INPUT | ConvertFrom-Json\nif ($input.PSObject.Properties['CommandLine']) { $input.CommandLine = 'echo rewritten' } else { $input.command = 'echo rewritten' }\n[Console]::Out.Write(($input | ConvertTo-Json -Compress))\nexit 0\n",
    );
    install(
        "garble",
        "echo 'not an input'",
        "[Console]::Out.Write('not an input')\nexit 0\n",
    );
    root
}

/// A fixture's handler as both shells spell it: a bare name is the
/// package's own `scripts/<name>`, and the other shapes the manifest
/// documents keep theirs.
fn spelled(handler: &str) -> ShellCommand {
    match handler {
        "sh ${PLUGIN_ROOT}/scripts/guard --strict" => {
            ShellCommand::spelled(handler, "& \"${PLUGIN_ROOT}/scripts/guard.ps1\" --strict")
        }
        "scripts/guard" => ShellCommand::spelled(handler, "& ./scripts/guard.ps1"),
        name => ShellCommand::spelled(
            format!("${{PLUGIN_ROOT}}/scripts/{name}"),
            format!("& \"${{PLUGIN_ROOT}}/scripts/{name}.ps1\""),
        ),
    }
}

/// What the wrapper answered for one fixture, in the recorded shape.
fn answered(target: HookTarget, index: usize, fixture: &Fixture) -> serde_json::Value {
    let root = package(&format!("parity-{target}-{index}"));
    let hook = PortableHook {
        id: "protect-env".into(),
        event: fixture.event,
        matchers: vec![HookMatcher::Portable("shell".into())],
        handlers: fixture
            .handlers
            .iter()
            .map(|handler| CommandHook {
                handler_type: CommandHandlerType::Command,
                command: spelled(handler),
                args: None,
                interpreter: None,
                timeout: fixture.timeout,
            })
            .collect(),
        effect: fixture.effect,
        order: 0,
    };
    let payload = fixture_payload(target, fixture);
    let package_root = if fixture.missing_root {
        root.join("gone")
    } else {
        root.clone()
    };
    let wrapper = root.join(WRAPPER_RELATIVE_PATH);
    std::fs::create_dir_all(wrapper.parent().unwrap()).unwrap();
    uze_testkit::process::install_executable(
        &wrapper,
        wrapper_source(target).expect("a wrapper here").as_bytes(),
    );
    let (program, mut arguments) = uze_platform::shell::script(&wrapper.display().to_string());
    arguments.extend(wrapper_arguments(&hook, &package_root, &hook.handlers));
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(payload.as_bytes());
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let document = if stdout.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&stdout).expect("one JSON document")
    };
    let answer = serde_json::json!({
        "exit": output.status.code(),
        "stdout": document,
        "said": String::from_utf8_lossy(&output.stderr).trim(),
    });
    let answer = portable(answer, &root);
    let _ = std::fs::remove_dir_all(&root);
    answer
}

/// Every string in `value` with the throwaway root written back as the
/// placeholder, in the spelling the POSIX wrapper's answers were recorded
/// in.
fn portable(value: serde_json::Value, root: &Path) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => {
            let root = root.display().to_string();
            serde_json::Value::String(
                text.replace(&root, "${PLUGIN_ROOT}")
                    .replace(&root.replace('\\', "/"), "${PLUGIN_ROOT}")
                    .replace("${PLUGIN_ROOT}\\", "${PLUGIN_ROOT}/"),
            )
        }
        serde_json::Value::Object(fields) => serde_json::Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key, portable(value, root)))
                .collect(),
        ),
        other => other,
    }
}

/// A reason with the shell's own words taken out: which line a handler
/// was, and what code the shell gave a command it could not find.
fn decided(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => {
            serde_json::Value::String(if text.starts_with("handler failed (exit ") {
                "handler failed".to_owned()
            } else if let Some(reason) = [
                "handler did not write a JSON object",
                "handler wrote more than",
            ]
            .into_iter()
            .find(|reason| text.starts_with(reason))
            {
                reason.to_owned()
            } else if text.starts_with("handler timed out after ") {
                text.split(": ").next().unwrap_or_default().to_owned()
            } else {
                text
            })
        }
        serde_json::Value::Object(fields) => serde_json::Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key, decided(value)))
                .collect(),
        ),
        other => other,
    }
}

#[test]
fn every_fixture_is_answered_as_recorded_on_this_platform() {
    let table: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(answers_path()).unwrap()).unwrap();
    let recorded = table["answers"].as_array().unwrap();
    let fixtures = fixtures();
    let mut index_of = std::collections::BTreeMap::<&str, usize>::new();
    for target in TARGETS
        .into_iter()
        .filter(|target| wrapper_source(*target).is_some())
    {
        for (index, fixture) in fixtures.iter().enumerate() {
            if !target.capabilities().events.contains(&fixture.event) {
                continue;
            }
            let position = index_of.entry(target.key()).or_insert(0);
            let expected = recorded
                .iter()
                .filter(|answer| answer["harness"] == target.key())
                .nth(*position)
                .expect("a recorded answer per fixture");
            *position += 1;
            let said = expected
                .get("stderr")
                .or_else(|| expected.get("stderr_prefix"))
                .cloned()
                .unwrap_or_default();
            let mut answer = answered(target, index, fixture);
            if !fixture.wrapper_owns_the_whole_reason {
                let text = answer["said"].as_str().unwrap_or_default();
                answer["said"] =
                    serde_json::json!(text.split(" \u{2014} ").next().unwrap_or_default());
                answer["stdout"] = without_the_shells_own_words(answer["stdout"].take());
            }
            assert_eq!(
                decided(answer),
                decided(serde_json::json!({
                    "exit": expected["exit"],
                    "stdout": expected["stdout"],
                    "said": said,
                })),
                "{} {}/{} {:?} {:?}",
                target.key(),
                expected["event"],
                expected["effect"],
                fixture.handlers,
                fixture.command
            );
        }
    }
}
