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

/// The field names follow OpenCode's documented tool schema; a
/// `--discovery` capture of this harness has not been taken yet.
const TOOLS: &[ToolBinding] = &[
    ToolBinding {
        alias: "shell",
        native_tool: Some("bash"),
        also_matches: &[],
        fields: &[("command", "command")],
    },
    ToolBinding {
        alias: "file.read",
        native_tool: Some("read"),
        also_matches: &[],
        fields: &[("path", "filePath")],
    },
    ToolBinding {
        alias: "file.write",
        native_tool: Some("write"),
        also_matches: &[],
        fields: &[("path", "filePath")],
    },
    ToolBinding {
        alias: "file.edit",
        native_tool: Some("edit"),
        also_matches: &[],
        fields: &[("path", "filePath")],
    },
    ToolBinding {
        alias: "search.files",
        native_tool: Some("grep"),
        also_matches: &[],
        fields: &[("query", "pattern")],
    },
    ToolBinding {
        alias: "search.web",
        native_tool: Some("web_search"),
        also_matches: &[],
        fields: &[("query", "query")],
    },
    ToolBinding {
        alias: "agent.spawn",
        native_tool: Some("task"),
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
