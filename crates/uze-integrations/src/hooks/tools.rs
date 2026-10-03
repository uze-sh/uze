//! Each harness's tool vocabulary, and the matcher a portable hook's tools become in it.

use super::*;

/// Every harness's binding of the portable tool vocabulary: per alias, the
/// native tool it is matched as and the native input field each portable
/// field is read from. This table is the single source the matchers, the
/// generated wrappers and the runtime adapters all read — nothing here is
/// hand-written twice.
///
/// The names come from what each harness declares to the model, captured
/// with the Lab's `--discovery` mode, not from memory: Antigravity's
/// `run_command`/`CommandLine`+`Cwd`, `write_to_file`/`TargetFile`,
/// `view_file`/`AbsolutePath`, `grep_search`/`Query` and `search_web`/
/// `query` are read off its own `parametersJsonSchema`; Codex's shell tool
/// is `exec_command` with a `cmd` argument (0.150.1 onwards — `Bash` stays
/// in `also_matches` so an older payload still normalizes). OpenCode's
/// field names follow its documented tool schema; a `--discovery` capture
/// of that harness has not been taken yet.
pub(crate) fn vocabulary(target: HookTarget) -> HarnessToolVocabulary {
    HarnessToolVocabulary {
        bindings: match target {
            HookTarget::Claude => CLAUDE_TOOLS,
            HookTarget::Codex => CODEX_TOOLS,
            HookTarget::Antigravity => ANTIGRAVITY_TOOLS,
            HookTarget::OpenCode => OPENCODE_TOOLS,
        },
    }
}

/// An alias no harness tool answers to. Kept in every table so the
/// vocabulary is exhaustive by construction: absence of a native name is
/// stated, never left to a missing row.
pub(super) const UNBOUND: Option<&'static str> = None;

pub(super) const CLAUDE_TOOLS: &[ToolBinding] = &[
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

pub(super) const CODEX_TOOLS: &[ToolBinding] = &[
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

pub(super) const ANTIGRAVITY_TOOLS: &[ToolBinding] = &[
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

pub(super) const OPENCODE_TOOLS: &[ToolBinding] = &[
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

/// Every native tool name one matcher intercepts on a target. `native:<name>`
/// passes through unchanged; a portable alias yields every tool this harness
/// binds it to, because a vendor that renames its shell tool keeps answering
/// to the old name for a while and a hook must intercept both. An alias the
/// harness binds to no tool falls back to the alias literal, which matches
/// nothing — an honest no-op rather than a fabricated tool name.
pub(crate) fn tool_names(target: HookTarget, matcher: &HookMatcher) -> Vec<String> {
    match matcher {
        HookMatcher::Native(name) | HookMatcher::Source(name) => vec![name.clone()],
        HookMatcher::Portable(alias) => match vocabulary(target).binding(alias) {
            Some(binding) => {
                let names: Vec<String> = binding
                    .native_tool
                    .into_iter()
                    .chain(binding.also_matches.iter().copied())
                    .map(str::to_owned)
                    .collect();
                if names.is_empty() {
                    vec![alias.clone()]
                } else {
                    names
                }
            }
            None => vec![alias.clone()],
        },
    }
}

/// Translates every matcher of a group for one target; `None` for an
/// unmatch-all group (the entry then omits the matcher key).
pub(crate) fn matcher(target: HookTarget, hook: &PortableHook) -> Option<String> {
    // "Every source" is the portable set, spelled out: a harness reporting
    // a source of its own (a compaction) must not run a handler that was
    // promised `HOOK_SOURCE` is one of these.
    if hook.event == HookEvent::SessionStart && hook.matchers.is_empty() {
        return Some(uze_core::hook::SESSION_SOURCES.join("|"));
    }
    (!hook.matchers.is_empty()).then(|| {
        // Two authored matchers can translate to one native tool (a
        // portable alias plus the `native:` name it already resolves to);
        // the entry names it once.
        let mut names: Vec<String> = Vec::new();
        for entry in &hook.matchers {
            for name in tool_names(target, entry) {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names.join("|")
    })
}

// ============================================================================
// Native entry rendering
// ============================================================================
