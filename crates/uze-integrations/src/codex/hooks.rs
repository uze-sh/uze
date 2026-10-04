//! Codex's hooks: the events and effects it preserves, the tools it names, and the dialect its command hooks answer in.

use uze_core::hook::{HookEffect, HookEvent, ToolBinding};

use crate::hooks::{
    Decisions, EntryShape, HookRunner, HookTarget, PayloadPaths, UNBOUND, WrapperDialect,
};

/// Codex mirrors Claude Code's event names in its own `hooks.json` command
/// form: observations, approvals and denials are expressible. Input
/// rewriting is not yet claimed — a `transform` effect therefore degrades
/// instead of silently attaching without its rewrite. `SessionStart` also
/// fires natively, matched on the session's source.
pub(crate) const HOOKS: HookTarget = HookTarget {
    key: "codex",
    events: &[
        HookEvent::PreToolUse,
        HookEvent::PostToolUse,
        HookEvent::Stop,
        HookEvent::SessionStart,
    ],
    effects: &[HookEffect::Observe, HookEffect::Allow, HookEffect::Deny],
    tools: TOOLS,
    runner: HookRunner::Wrapper {
        dialect: WrapperDialect {
            payload: PayloadPaths {
                tool: ".tool_name // empty",
                input: ".tool_input // {}",
                cwd: ".cwd // empty",
            },
            // Stop is the one event whose stdout must parse as JSON even
            // when nothing was decided.
            posix: Decisions {
                deny: "printf '{\"hookSpecificOutput\":{\"permissionDecision\":\"deny\",\"permissionDecisionReason\":%s}}' \"$reason_json\"",
                allow: "[ \"$HOOK_EVENT\" = stop ] && printf '{}'",
                unfired: &[],
            },
            powershell: Some(Decisions {
                deny: "[Console]::Out.Write('{\"hookSpecificOutput\":{\"permissionDecision\":\"deny\",\"permissionDecisionReason\":' + $reasonJson + '}}')",
                allow: "if ($hookEvent -eq 'stop') { [Console]::Out.Write('{}') }",
                // Measured and reported upstream: a Windows shell command runs
                // as `command_execution`, which fires no PreToolUse hook.
                unfired: &[(
                    HookEvent::PreToolUse,
                    "shell",
                    "Codex runs a Windows shell command without firing it \
                     (https://github.com/openai/codex/issues/24453)",
                )],
            }),
            deny_exit: "2",
        },
        // Codex's entries carry a command string only: one quoted shell line.
        entry: EntryShape::EventLine,
    },
};

/// The shell tool is `exec_command` with a `cmd` argument (0.150.1
/// onwards); `Bash` stays in `also_matches` so an older payload still
/// normalizes.
const TOOLS: &[ToolBinding] = &[
    ToolBinding {
        alias: "shell",
        native_tool: Some("exec_command"),
        also_matches: &["Bash"],
        fields: &[("command", "cmd")],
    },
    ToolBinding {
        alias: "file.read",
        native_tool: Some("Read"),
        also_matches: &[],
        fields: &[("path", "file_path")],
    },
    ToolBinding {
        alias: "file.write",
        native_tool: Some("Write"),
        also_matches: &[],
        fields: &[("path", "file_path")],
    },
    ToolBinding {
        alias: "file.edit",
        native_tool: Some("Edit"),
        also_matches: &[],
        fields: &[("path", "file_path")],
    },
    ToolBinding {
        alias: "search.files",
        native_tool: Some("Grep"),
        also_matches: &[],
        fields: &[("query", "pattern")],
    },
    ToolBinding {
        alias: "search.web",
        native_tool: Some("WebSearch"),
        also_matches: &[],
        fields: &[("query", "query")],
    },
    ToolBinding {
        alias: "agent.spawn",
        native_tool: UNBOUND,
        also_matches: &[],
        fields: &[],
    },
    ToolBinding {
        alias: "agent.message",
        native_tool: UNBOUND,
        also_matches: &[],
        fields: &[],
    },
];
