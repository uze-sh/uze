//! Antigravity CLI's hooks: the events and effects it preserves, the tools it names, and the dialect its command hooks answer in.

use uze_core::hook::{HookEffect, HookEvent, ToolBinding};

use crate::hooks::{
    Decisions, EntryShape, HookRunner, HookTarget, PayloadPaths, UNBOUND, WrapperDialect,
};

/// Antigravity CLI's named hooks carry camelCase payloads and native
/// `allow`/`ask`/`deny` decisions. Beside the documented events it reads a
/// `SessionStart` key the docs do not list (the binary's
/// `CallSessionStartHook`, measured on 1.2.17): flat like `Stop`, run once
/// for a new conversation at its first model call, and never for one
/// resumed with `--continue`, so its only source is `startup`. Being
/// undocumented, the Lab measures it every run (`hooks > events`).
pub(crate) const HOOKS: HookTarget = HookTarget {
    key: "antigravity",
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
    session_sources: &["startup"],
    runner: HookRunner::Wrapper {
        dialect: &WrapperDialect {
            payload: PayloadPaths {
                tool: ".toolCall.name // empty",
                input: ".toolCall.args // {}",
                cwd: ".workspacePaths[0] // empty",
                // Its session-start hook (undocumented, measured on 1.2.17)
                // fires for a new conversation only and carries no source.
                implied_source: Some("startup"),
            },
            // Only the pre-tool event carries a decision; the others answer
            // with the empty object the vendor's contract requires. A
            // handler's denial in an `ask` group is answered `ask`, which
            // the harness puts to the person; any other closing is `deny`.
            posix: Decisions {
                deny: "printf '{\"decision\":\"%s\",\"reason\":%s}' \"$decision\" \"$reason_json\"",
                allow: "[ \"$HOOK_EVENT\" = pre_tool_use ] || printf '{}'",
                // A rewrite is `overwrite` (the tool call's whole args) beside
                // `allow`: the hook result's field the docs do not list
                // (`PreToolHookResult.overwrite`, 1.3.0), measured by the Lab.
                transform: Some(
                    "printf '{\"decision\":\"allow\",\"overwrite\":%s}' \"$updated_json\"",
                ),
                unfired: &[],
            },
            // Measured on 1.2.16 on Windows: the entry is run as
            // `cmd /c "<command>"` (an `args` array is ignored), the payload
            // arrives on stdin as on Linux, and a pre-tool hook allows by
            // writing nothing, `{}` reading as a denial. The entry's line is
            // sealed against `cmd`'s quoting (`sealed_wrapper_command_line`).
            powershell: Some(Decisions {
                deny: "[Console]::Out.Write('{\"decision\":\"' + $decision + '\",\"reason\":' + $reasonJson + '}')",
                allow: "if ($hookEvent -ne 'pre_tool_use') { [Console]::Out.Write('{}') }",
                transform: Some(
                    "[Console]::Out.Write('{\"decision\":\"allow\",\"overwrite\":' + $updatedJson + '}')",
                ),
                unfired: &[],
            }),
            // The decision is the stdout document; a non-zero exit is a
            // failed hook here, not a block.
            deny_exit: "0",
            // No status blocks here: any non-zero exit is a failed hook and
            // the call runs. The document is written by a shell builtin with a
            // reason that cannot fail to encode, so this is the last resort
            // of a closed stdout, and says so in the hook log.
            unwritten_exit: "2",
        },
        // The shared `hooks.json` is a map of named hooks, not an array of
        // group entries per event.
        entry: EntryShape::Named,
    },
};

/// Measured, never recalled: every native name and field here is the one a
/// Lab census saw a call reach a hook with (`hook_tools` in
/// `conformance/evidence/tools/antigravity.json`, 1.3.3), and
/// `hooks::measured_tests` fails on any that a later census contradicts.
/// 1.3.3 offers no `grep_search`, so `search.files` stays unbound.
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
        native_tool: UNBOUND,
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
        native_tool: Some("invoke_subagent"),
        also_matches: &[],
        fields: &[],
    },
    ToolBinding {
        alias: "agent.message",
        native_tool: Some("send_message"),
        also_matches: &[],
        fields: &[],
    },
];
