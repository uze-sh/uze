//! The fixtures every hook wrapper is run against, and the answers
//! recorded for them: shared by the POSIX wrapper's exact golden and the
//! parity every platform's wrapper is held to.

use std::path::PathBuf;

use uze_core::hook::{HookEffect, HookEvent};

use super::HookTarget;

pub(super) const TARGETS: [HookTarget; 3] = [
    crate::claude::HOOKS,
    crate::codex::HOOKS,
    crate::antigravity::HOOKS,
];

pub(super) fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
}

/// A `stop` payload as each harness sends it: no tool at all, which is
/// what the wrapper has to leave the handler seeing.
pub(super) fn stop_payload(target: HookTarget) -> String {
    if target == crate::antigravity::HOOKS {
        serde_json::json!({"workspacePaths": ["/repo"]}).to_string()
    } else {
        serde_json::json!({"cwd": "/repo"}).to_string()
    }
}

/// What Claude Code and Codex hand a `SessionStart` hook: the session's
/// source beside the workspace, and no tool.
pub(super) fn session_payload() -> String {
    serde_json::json!({
        "hook_event_name": "SessionStart",
        "source": "startup",
        "cwd": "/repo",
    })
    .to_string()
}

pub(super) fn payload(target: HookTarget, command: &str) -> String {
    if target == crate::antigravity::HOOKS {
        return serde_json::json!({
            "toolCall": {"name": "run_command", "args": {"CommandLine": command, "Cwd": "/repo"}},
            "workspacePaths": ["/repo"],
        })
        .to_string();
    }
    // Claude and Codex hand a shell call to a hook in the same shape
    // (measured: Codex 0.160 reports `Bash` with `command`, whatever its
    // model was offered).
    serde_json::json!({
            "tool_name": "Bash",
            "tool_input": {"command": command},
            "cwd": "/repo",
    })
    .to_string()
}

/// One fixture the wrapper is run against: a group, the payload it is
/// fired with, and what the handlers do.
pub(super) struct Fixture {
    pub(super) event: HookEvent,
    pub(super) effect: HookEffect,
    pub(super) handlers: &'static [&'static str],
    pub(super) timeout: u16,
    /// The shell command the payload carries. `None` is a `stop`
    /// payload, which carries no tool.
    pub(super) command: Option<&'static str>,
    /// Whether the payload is one the wrapper cannot read. The context
    /// is the whole basis of a decision, so what happens to a payload
    /// that does not parse is part of the contract.
    pub(super) malformed_payload: bool,
    /// Whether the group's entry names a package root that is gone — a
    /// stale entry for a package removed, renamed, or a moved `~/.uze`.
    pub(super) missing_root: bool,
    /// Whether the payload carries more than two megabytes beside what the
    /// handler is handed: the wrapper reads all of it, whatever its size.
    pub(super) large_payload: bool,
    /// Whether the recorded stderr is the wrapper's own words all the
    /// way. A handler that never started is reported with the system
    /// shell's diagnostic appended, and that wording is the platform's,
    /// so only the head of the line is recorded.
    pub(super) wrapper_owns_the_whole_reason: bool,
}

/// A payload no harness would send — truncated mid-object, the shape a
/// crashed writer or a wrong-dialect entry produces.
pub(super) const MALFORMED_PAYLOAD: &str =
    r#"{"tool_name":"Bash","tool_input":{"command":"cat .env""#;

/// Every fixture, in the order the recorded table holds them.
pub(super) fn fixtures() -> Vec<Fixture> {
    let case = |event, effect, handlers, command| Fixture {
        event,
        effect,
        handlers,
        timeout: 10,
        command,
        malformed_payload: false,
        missing_root: false,
        large_payload: false,
        wrapper_owns_the_whole_reason: true,
    };
    let pre = HookEvent::PreToolUse;
    vec![
        case(pre, HookEffect::Deny, &["guard", "audit"], Some("cat .env")),
        case(pre, HookEffect::Deny, &["guard", "audit"], Some("ls -la")),
        // A guard's own denial in an `ask` group asks the person where the
        // harness can; a guard that fails still closes with a denial.
        case(pre, HookEffect::Ask, &["guard"], Some("cat .env")),
        Fixture {
            wrapper_owns_the_whole_reason: false,
            ..case(pre, HookEffect::Ask, &["absent"], Some("ls"))
        },
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
        // Text beyond ASCII reaches the handler and comes back in its
        // reason unchanged.
        case(pre, HookEffect::Deny, &["guard"], Some("cat café/.env")),
        Fixture {
            large_payload: true,
            ..case(pre, HookEffect::Deny, &["guard"], Some("cat .env"))
        },
    ]
}

/// The payload `fixture` is fired with at `target`.
pub(super) fn fixture_payload(target: HookTarget, fixture: &Fixture) -> String {
    if fixture.malformed_payload {
        return MALFORMED_PAYLOAD.to_owned();
    }
    let raw = match fixture.command {
        Some(command) => payload(target, command),
        None if fixture.event == HookEvent::SessionStart => session_payload(),
        None => stop_payload(target),
    };
    if !fixture.large_payload {
        return raw;
    }
    let mut document: serde_json::Value = serde_json::from_str(&raw).unwrap();
    document["transcript"] = serde_json::json!("x".repeat(2_100_000));
    document.to_string()
}

pub(super) fn answers_path() -> PathBuf {
    goldens_dir().join("hooks").join("wrapper-answers.json")
}

/// A reason the system shell contributed the tail of, cut back to the
/// wrapper's own words: `sh` says "not found" on one platform and "No
/// such file or directory" on another, and neither is a contract.
pub(super) fn without_the_shells_own_words(value: serde_json::Value) -> serde_json::Value {
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
