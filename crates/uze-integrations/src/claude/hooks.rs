//! Claude Code's hooks: the events and effects it preserves, the tools it names, and the dialect its command hooks answer in.

use uze_core::hook::{HookEffect, HookEvent, ToolBinding};

use crate::hooks::{
    Decisions, EntryShape, HookRunner, HookTarget, PayloadPaths, UNBOUND, WrapperDialect,
};

/// Claude Code documents `PreToolUse`/`PostToolUse`/`Stop` command hooks
/// with per-group matchers: observations, approvals, denials and asks
/// (`permissionDecision: ask`, which puts the call to the person with the
/// reason) are expressible. Input rewriting is not yet claimed — a `transform` effect
/// therefore degrades instead of silently attaching without its rewrite.
/// `SessionStart` also fires natively, matched on the session's source.
pub(crate) const HOOKS: HookTarget = HookTarget {
    key: "claude",
    events: &[
        HookEvent::PreToolUse,
        HookEvent::PostToolUse,
        HookEvent::Stop,
        HookEvent::SessionStart,
    ],
    effects: &[
        HookEffect::Observe,
        HookEffect::Allow,
        HookEffect::Ask,
        HookEffect::Deny,
        HookEffect::Transform,
    ],
    tools: TOOLS,
    session_sources: uze_core::hook::SESSION_SOURCES,
    runner: HookRunner::Wrapper {
        dialect: &WrapperDialect {
            payload: PayloadPaths {
                tool: ".tool_name // empty",
                input: ".tool_input // {}",
                cwd: ".cwd // .context.cwd // empty",
                implied_source: None,
            },
            // A decision is JSON on stdout with exit 0: Claude reads
            // stdout only then, and renders exit 2 as a failed hook ("hook
            // error: …", measured on 2.1.290) however intentional the
            // denial. Each event has its own shape: `PreToolUse` a
            // `permissionDecision` echoing the event in `hookEventName`,
            // `PostToolUse` and `Stop` a top-level `decision: "block"`.
            // `session_start` never gets this far.
            posix: Decisions {
                deny: concat!(
                    "case $HOOK_EVENT in\n",
                    "    pre_tool_use) printf '{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"%s\",\"permissionDecisionReason\":%s}}' \"$decision\" \"$reason_json\" ;;\n",
                    "    *) printf '{\"decision\":\"block\",\"reason\":%s}' \"$reason_json\" ;;\n",
                    "  esac",
                ),
                allow: ":",
                // A rewrite is `updatedInput` (the whole input) beside `allow`.
                transform: Some(
                    "printf '{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"allow\",\"updatedInput\":%s}}' \"$updated_json\"",
                ),
                unfired: &[],
            },
            powershell: Some(Decisions {
                deny: concat!(
                    "if ($hookEvent -eq 'pre_tool_use') {\n",
                    "    [Console]::Out.Write('{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"' + $decision + '\",\"permissionDecisionReason\":' + $reasonJson + '}}')\n",
                    "  } else {\n",
                    "    [Console]::Out.Write('{\"decision\":\"block\",\"reason\":' + $reasonJson + '}')\n",
                    "  }",
                ),
                allow: "",
                transform: Some(
                    "[Console]::Out.Write('{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"allow\",\"updatedInput\":' + $updatedJson + '}}')",
                ),
                unfired: &[],
            }),
            deny_exit: "0",
        },
        // Claude's entries accept `command` + `args`, so the wrapper is
        // started directly with nothing to quote.
        entry: EntryShape::EventExec,
    },
};

/// Measured, never recalled: every native name and field here is the one a
/// Lab census saw a call reach a hook with (`hook_tools` in
/// `conformance/evidence/tools/claude.json`, 2.1.290), and
/// `hooks::measured_tests` fails on any that a later census contradicts.
const TOOLS: &[ToolBinding] = &[
    // Claude Code's shell tool is `PowerShell` on Windows, with the same
    // `command` field (measured on 2.1.289); `Bash` everywhere else.
    ToolBinding {
        alias: "shell",
        native_tool: Some("Bash"),
        also_matches: &["PowerShell"],
        fields: &[("command", "command")],
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
    // The native build offers `Grep` only on opt-in or to a subagent whose
    // tools name it; its main session searches through `Bash` (2.1.290).
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
        native_tool: Some("Agent"),
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
