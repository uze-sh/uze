//! OpenCode's hooks: the events and effects its generated plugin preserves, and the tools it names.

use uze_core::hook::{HookEffect, HookEvent, ToolBinding};

use crate::hooks::{HookRunner, HookTarget, UNBOUND};

/// OpenCode V2 has no declarative hook file, so UZE generates an owned,
/// rebuildable plugin (the bridge) that rides the plugin API: the tool
/// hook observes a call and refuses it on a denial (whether or not the
/// tool asks for permission), `permission.evaluate` asks about it (with the
/// input the tool hook kept by call id), and the bus events
/// `session.created` and `session.execution.succeeded` are a new session
/// and the end of a turn (anomalyco/opencode `v2`, 2.0.24). A resumed
/// session announces nothing, so `startup` is the only source. `transform`
/// needs a channel for the handler to answer on, which the exit-code
/// contract does not have.
pub(crate) const HOOKS: HookTarget = HookTarget {
    key: "opencode",
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
    runner: HookRunner::Bridge {
        prompting: PROMPTING,
    },
};

/// The tools that put a call to `permission.evaluate` before they act
/// (`permission.assert` in each tool's own source, anomalyco/opencode `v2`;
/// `read` has none), the one place an `ask` can be answered. Measured by the
/// Lab's `hooks-ask-*` checks on `shell` and its `hooks-deny-unprompted-*`
/// checks on `read`.
const PROMPTING: &[&str] = &["shell", "write", "edit", "grep", "websearch", "subagent"];

/// Measured, never recalled: every native name and field here is the one a
/// Lab census saw a call reach the bridge with (`hook_tools` in
/// `conformance/evidence/tools/opencode.json`, v2.0.23), and
/// `hooks::measured_tests` fails on any that a later census contradicts.
/// V2 renamed the V1 tools this table used to name (`bash`, `web_search`,
/// `task`) and moved the file tools' path to `path`.
const TOOLS: &[ToolBinding] = &[
    ToolBinding {
        alias: "shell",
        native_tool: Some("shell"),
        also_matches: &[],
        fields: &[("command", "command")],
    },
    ToolBinding {
        alias: "file.read",
        native_tool: Some("read"),
        also_matches: &[],
        fields: &[("path", "path")],
    },
    ToolBinding {
        alias: "file.write",
        native_tool: Some("write"),
        also_matches: &[],
        fields: &[("path", "path")],
    },
    ToolBinding {
        alias: "file.edit",
        native_tool: Some("edit"),
        also_matches: &[],
        fields: &[("path", "path")],
    },
    ToolBinding {
        alias: "search.files",
        native_tool: Some("grep"),
        also_matches: &[],
        fields: &[("query", "pattern")],
    },
    ToolBinding {
        alias: "search.web",
        native_tool: Some("websearch"),
        also_matches: &[],
        fields: &[("query", "query")],
    },
    ToolBinding {
        alias: "agent.spawn",
        native_tool: Some("subagent"),
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
