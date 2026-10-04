//! Claude Code's hooks: the events and effects it preserves, the tools it names, and the dialect its command hooks answer in.

use uze_core::hook::{HookEffect, HookEvent, ToolBinding};

use crate::hooks::{
    Decisions, EntryShape, HookRunner, HookTarget, PayloadPaths, UNBOUND, WrapperDialect,
};

/// Claude Code documents `PreToolUse`/`PostToolUse`/`Stop` command hooks
/// with per-group matchers: observations, approvals and denials are
/// expressible. Input rewriting is not yet claimed — a `transform` effect
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
    effects: &[HookEffect::Observe, HookEffect::Allow, HookEffect::Deny],
    tools: TOOLS,
    runner: HookRunner::Wrapper {
        dialect: WrapperDialect {
            payload: PayloadPaths {
                tool: ".tool_name // empty",
                input: ".tool_input // {}",
                cwd: ".cwd // .context.cwd // empty",
            },
            // The event name is echoed back in `hookEventName`, which the
            // harness matches against the event it fired. Every event that
            // can deny is named: `session_start` never gets this far.
            posix: Decisions {
                deny: concat!(
                    "case $HOOK_EVENT in\n",
                    "    pre_tool_use) name=PreToolUse ;;\n",
                    "    post_tool_use) name=PostToolUse ;;\n",
                    "    stop) name=Stop ;;\n",
                    "  esac\n",
                    "  printf '{\"hookSpecificOutput\":{\"hookEventName\":\"%s\",\"permissionDecision\":\"deny\",\"permissionDecisionReason\":%s}}' \"$name\" \"$reason_json\"",
                ),
                allow: ":",
                unfired: &[],
            },
            powershell: Some(Decisions {
                deny: concat!(
                    "$name = @{ pre_tool_use = 'PreToolUse'; post_tool_use = 'PostToolUse'; stop = 'Stop' }[$hookEvent]\n",
                    "  [Console]::Out.Write('{\"hookSpecificOutput\":{\"hookEventName\":\"' + $name + '\",\"permissionDecision\":\"deny\",\"permissionDecisionReason\":' + $reasonJson + '}}')",
                ),
                allow: "",
                unfired: &[],
            }),
            deny_exit: "2",
        },
        // Claude's entries accept `command` + `args`, so the wrapper is
        // started directly with nothing to quote.
        entry: EntryShape::EventExec,
    },
};

const TOOLS: &[ToolBinding] = &[
    ToolBinding {
        alias: "shell",
        native_tool: Some("Bash"),
        also_matches: &[],
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
        native_tool: Some("MultiEdit"),
        also_matches: &["Edit"],
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
        native_tool: Some("Task"),
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
