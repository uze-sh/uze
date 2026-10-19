//! Codex's hooks: the events and effects it preserves, the tools it names, and the dialect its command hooks answer in.

use uze_core::hook::{HookEffect, HookEvent, ToolBinding};

use crate::hooks::{
    Decisions, EntryShape, HookRunner, HookTarget, PayloadPaths, UNBOUND, WrapperDialect,
};
use crate::rules::{RulesDelivery, RulesTarget};

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
    effects: &[
        HookEffect::Observe,
        HookEffect::Allow,
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
                cwd: ".cwd // empty",
                implied_source: None,
            },
            // Stop is the one event whose stdout must parse as JSON even
            // when nothing was decided.
            posix: Decisions {
                deny: "printf '{\"hookSpecificOutput\":{\"permissionDecision\":\"deny\",\"permissionDecisionReason\":%s}}' \"$reason_json\"",
                allow: "[ \"$HOOK_EVENT\" = stop ] && printf '{}'",
                // A rewrite is `updatedInput` beside `allow`, the only decision it
                // goes with, and `hookEventName` is required in this document
                // (codex-rs `hooks/src/schema.rs`, `output_parser.rs`):
                // without it Codex reports the hook failed and runs the call
                // as asked.
                transform: Some(
                    "printf '{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"allow\",\"updatedInput\":%s}}' \"$updated_json\"",
                ),
            },
            powershell: Some(Decisions {
                deny: "[Console]::Out.Write('{\"hookSpecificOutput\":{\"permissionDecision\":\"deny\",\"permissionDecisionReason\":' + $reasonJson + '}}')",
                allow: "if ($hookEvent -eq 'stop') { [Console]::Out.Write('{}') }",
                transform: Some(
                    "[Console]::Out.Write('{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"allow\",\"updatedInput\":' + $updatedJson + '}}')",
                ),
            }),
            deny_exit: "2",
            unwritten_exit: "2",
            failure_marker: None,
        },
        // Codex's entries carry a command string only: one quoted shell line.
        entry: EntryShape::EventLine,
    },
};

/// Codex has no rules of its own; the rules engine answers through its
/// `SessionStart` and `PostToolUse` command hooks in the user-level
/// `hooks.json`, which Codex trusts without the hook trust a project's own
/// file needs. The tools are the ones a hook sees a file reached through
/// since code mode (see [`TOOLS`]), with the outer `exec_command` beside
/// them, in Codex's shell-line entry form.
pub(crate) const RULES: RulesTarget = RulesTarget {
    key: "codex",
    delivery: RulesDelivery::Entries {
        shape: EntryShape::EventLine,
        tools: "Bash|exec_command|shell|apply_patch",
    },
};

/// Why Codex's rules route is adapted, and what it can miss.
pub(crate) const RULES_REASON: &str = "Codex has no rules of its own; the rules engine hands each rule over through a `SessionStart` and a `PostToolUse` hook's `additionalContext`. Codex reads files through its shell, so the path a command touched is recovered from its words: a best effort that can miss a path built from a variable, while a patch names its files exactly";

/// Measured, never recalled: every native name and field here is the one a
/// Lab census saw a call reach a hook with (`hook_tools` in
/// `conformance/evidence/tools/codex.json`, codex-cli 0.160.1), and
/// `hooks::measured_tests` fails on any that a later census contradicts.
///
/// Since code mode, the model calls nested tools through `exec`, and a
/// hook sees them under names of their own: a shell command as `Bash` with
/// `command` (not the `exec_command`/`cmd` the model is offered), a file
/// write or edit as `apply_patch` with the whole patch in `command` and no
/// path — so `file.*` cannot carry the `path` it promises and stays
/// unbound, a guard on patches naming `native:apply_patch` — and a
/// collaboration tool with its namespace run into its name. Code mode
/// offers no tool to read or search files, nor the web, outside the shell.
const TOOLS: &[ToolBinding] = &[
    ToolBinding {
        alias: "shell",
        native_tool: Some("Bash"),
        also_matches: &[],
        fields: &[("command", "command")],
    },
    ToolBinding {
        alias: "file.read",
        native_tool: UNBOUND,
        also_matches: &[],
        fields: &[("path", "file_path")],
    },
    ToolBinding {
        alias: "file.write",
        native_tool: UNBOUND,
        also_matches: &[],
        fields: &[("path", "file_path")],
    },
    ToolBinding {
        alias: "file.edit",
        native_tool: UNBOUND,
        also_matches: &[],
        fields: &[("path", "file_path")],
    },
    ToolBinding {
        alias: "search.files",
        native_tool: UNBOUND,
        also_matches: &[],
        fields: &[("query", "pattern")],
    },
    ToolBinding {
        alias: "search.web",
        native_tool: UNBOUND,
        also_matches: &[],
        fields: &[("query", "query")],
    },
    ToolBinding {
        alias: "agent.spawn",
        native_tool: Some("collaborationspawn_agent"),
        also_matches: &[],
        fields: &[],
    },
    ToolBinding {
        alias: "agent.message",
        native_tool: Some("collaborationsend_message"),
        also_matches: &[],
        fields: &[],
    },
];
