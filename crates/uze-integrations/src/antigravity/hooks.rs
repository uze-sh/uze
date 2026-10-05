//! Antigravity CLI's hooks: the events and effects it preserves, the tools it names, and the dialect its command hooks answer in.

use uze_core::hook::{HookEffect, HookEvent, ToolBinding};

use crate::hooks::{
    Decisions, EntryShape, HookRunner, HookTarget, PayloadPaths, UNBOUND, WrapperDialect,
};

/// Antigravity CLI's named hooks carry camelCase payloads and native
/// `allow`/`ask`/`deny` decisions. It has no session-start event
/// (1.2.x fires `PreToolUse`, `PostToolUse`, `PreInvocation`,
/// `PostInvocation`, `Stop`); `PreInvocation` fires on every turn, and
/// telling the first from the rest would need per-session state the
/// stateless wrapper does not keep, so `SessionStart` is not claimed.
pub(crate) const HOOKS: HookTarget = HookTarget {
    key: "antigravity",
    events: &[
        HookEvent::PreToolUse,
        HookEvent::PostToolUse,
        HookEvent::Stop,
    ],
    effects: &[
        HookEffect::Observe,
        HookEffect::Allow,
        HookEffect::Ask,
        HookEffect::Deny,
    ],
    tools: TOOLS,
    runner: HookRunner::Wrapper {
        dialect: WrapperDialect {
            payload: PayloadPaths {
                tool: ".toolCall.name // empty",
                input: ".toolCall.args // {}",
                cwd: ".workspacePaths[0] // empty",
            },
            // Only the pre-tool event carries a decision; the others answer
            // with the empty object the vendor's contract requires.
            posix: Decisions {
                deny: "printf '{\"decision\":\"deny\",\"reason\":%s}' \"$reason_json\"",
                allow: "[ \"$HOOK_EVENT\" = pre_tool_use ] || printf '{}'",
                unfired: &[],
            },
            // Measured on 1.2.16 on Windows: the entry is run as
            // `cmd /c "<command>"` (an `args` array is ignored), the payload
            // arrives on stdin as on Linux, and a pre-tool hook allows by
            // writing nothing, `{}` reading as a denial. The entry's line is
            // sealed against `cmd`'s quoting (`sealed_wrapper_command_line`).
            powershell: Some(Decisions {
                deny: "[Console]::Out.Write('{\"decision\":\"deny\",\"reason\":' + $reasonJson + '}')",
                allow: "if ($hookEvent -ne 'pre_tool_use') { [Console]::Out.Write('{}') }",
                unfired: &[],
            }),
            // The decision is the stdout document; a non-zero exit is a
            // failed hook here, not a block.
            deny_exit: "0",
        },
        // The shared `hooks.json` is a map of named hooks, not an array of
        // group entries per event.
        entry: EntryShape::Named,
    },
};

/// Read off the harness's own `parametersJsonSchema` with the Lab's
/// `--discovery` mode: `run_command`/`CommandLine`+`Cwd`,
/// `write_to_file`/`TargetFile`, `view_file`/`AbsolutePath`,
/// `grep_search`/`Query` and `search_web`/`query`.
const TOOLS: &[ToolBinding] = &[
    ToolBinding {
        alias: "shell",
        native_tool: Some("run_command"),
        also_matches: &[],
        fields: &[("command", "CommandLine")],
    },
    ToolBinding {
        alias: "file.read",
        native_tool: Some("view_file"),
        also_matches: &[],
        fields: &[("path", "AbsolutePath")],
    },
    ToolBinding {
        alias: "file.write",
        native_tool: Some("write_to_file"),
        also_matches: &[],
        fields: &[("path", "TargetFile")],
    },
    ToolBinding {
        alias: "file.edit",
        native_tool: Some("replace_file_content"),
        also_matches: &[],
        fields: &[("path", "TargetFile")],
    },
    ToolBinding {
        alias: "search.files",
        native_tool: Some("grep_search"),
        also_matches: &[],
        fields: &[("query", "Query")],
    },
    ToolBinding {
        alias: "search.web",
        native_tool: Some("search_web"),
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
