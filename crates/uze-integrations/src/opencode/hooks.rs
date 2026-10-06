//! OpenCode's hooks: the events and effects its generated plugin preserves, and the tools it names.

use uze_core::hook::{HookEffect, HookEvent, ToolBinding};

use crate::hooks::{HookRunner, HookTarget, UNBOUND};

/// OpenCode's plugin API supplies pre/post tool callbacks that see the
/// tool input but cannot block it; there is no declarative hook file, so
/// UZE generates an owned, rebuildable plugin instead. `Stop` has no
/// OpenCode equivalent and is never claimed, and neither is
/// `SessionStart`: a plugin's event stream (2.0.18) never carries
/// `session.created` for a new session, and nothing in it tells a new
/// session from a continued one (Lab experiment `opencode/session-start`). `deny`/`ask` live only on
/// `permission.evaluate`, which carries the action and its resources
/// rather than the tool input, so they are Unsupported until the Lab
/// proves otherwise. `transform` needs a channel for the handler to
/// answer on, which the exit-code contract does not have.
pub(crate) const HOOKS: HookTarget = HookTarget {
    key: "opencode",
    events: &[HookEvent::PreToolUse, HookEvent::PostToolUse],
    effects: &[HookEffect::Observe, HookEffect::Allow],
    tools: TOOLS,
    runner: HookRunner::Bridge,
};

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
