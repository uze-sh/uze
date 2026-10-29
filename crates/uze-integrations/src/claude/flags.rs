//! The flags a Claude Code launch takes, as offered to an operator's
//! overrides, and the variables this integration sets or reads.
//!
//! Described from `help.txt`, the `--help` of Claude Code 2.1.312; the
//! tests below fail on a spelling the snapshot does not show, and the Lab
//! fails when the snapshot no longer matches the release. Launch modes that
//! take the session somewhere else (`--bg`, `--desktop`, `--cloud`,
//! `--worktree`, `--remote-control`) are left out: they pass as unknown.

use uze_core::flag_catalog::{
    FlagOwner::{Degrades, Operator, Uze, Weakens},
    FlagSpec, FlagValue,
};

use super::{
    runtime::{ADD_DIR_FLAG, RUNTIME_PROJECTION_ENV_VAR, SETTINGS_FLAG},
    session::{RESUME_FLAG, START_FLAG},
};

const CONTINUITY: &str = "uze continues this agent's conversation itself";

pub(super) const CATALOG: &[FlagSpec] = &[
    FlagSpec::new(
        &["--model"],
        FlagValue::Free("model"),
        "model for the session",
        Operator,
    ),
    FlagSpec::new(
        &["--fallback-model"],
        FlagValue::Free("models"),
        "models to fall back to when the default is unavailable",
        Operator,
    ),
    FlagSpec::new(
        &["--effort"],
        FlagValue::OneOf(&["low", "medium", "high", "xhigh", "max"]),
        "effort level for the session",
        Operator,
    ),
    FlagSpec::new(
        &["--permission-mode"],
        FlagValue::OneOf(&[
            "acceptEdits",
            "auto",
            "bypassPermissions",
            "manual",
            "dontAsk",
            "plan",
        ]),
        "permission mode for the session; bypassPermissions skips every prompt",
        Operator,
    )
    .weakens_with(&[("bypassPermissions", "permission prompts")]),
    FlagSpec::new(
        &[ADD_DIR_FLAG],
        FlagValue::Free("dir"),
        "another directory tools may reach",
        Operator,
    )
    .repeatable(),
    FlagSpec::new(
        &["--agent"],
        FlagValue::Free("agent"),
        "agent for the session",
        Operator,
    ),
    FlagSpec::new(
        &["--agents"],
        FlagValue::Free("json"),
        "custom agents, as a JSON object",
        Operator,
    ),
    FlagSpec::new(
        &["--allowed-tools", "--allowedTools"],
        FlagValue::Free("tools"),
        "tools allowed without a prompt",
        Operator,
    )
    .repeatable(),
    FlagSpec::new(
        &["--disallowed-tools", "--disallowedTools"],
        FlagValue::Free("tools"),
        "tools denied",
        Operator,
    )
    .repeatable(),
    FlagSpec::new(
        &["--tools"],
        FlagValue::Free("tools"),
        "the built-in tools available",
        Operator,
    ),
    FlagSpec::new(
        &["--append-system-prompt"],
        FlagValue::Free("prompt"),
        "text appended to the system prompt",
        Operator,
    ),
    FlagSpec::new(
        &["--system-prompt"],
        FlagValue::Free("prompt"),
        "system prompt for the session",
        Operator,
    ),
    FlagSpec::new(
        &["--mcp-config"],
        FlagValue::Free("config"),
        "MCP servers from a JSON file or string",
        Operator,
    )
    .repeatable(),
    FlagSpec::new(
        &["--plugin-dir"],
        FlagValue::Free("path"),
        "a plugin loaded for this session only",
        Operator,
    )
    .repeatable(),
    FlagSpec::new(
        &["--debug", "-d"],
        FlagValue::None,
        "debug mode; --debug=<filter> picks categories",
        Operator,
    ),
    FlagSpec::new(
        &["--debug-file"],
        FlagValue::Free("path"),
        "debug log written to a file",
        Operator,
    ),
    FlagSpec::new(&["--verbose"], FlagValue::None, "verbose output", Operator),
    FlagSpec::new(
        &["--name", "-n"],
        FlagValue::Free("name"),
        "display name for the session",
        Operator,
    ),
    FlagSpec::new(
        &["--chrome"],
        FlagValue::None,
        "Claude in Chrome on",
        Operator,
    ),
    FlagSpec::new(
        &["--no-chrome"],
        FlagValue::None,
        "Claude in Chrome off",
        Operator,
    ),
    FlagSpec::new(
        &["--ide"],
        FlagValue::None,
        "connect to the IDE on startup",
        Operator,
    ),
    FlagSpec::new(
        &["--dangerously-skip-permissions"],
        FlagValue::None,
        "bypass every permission check",
        Weakens("permission prompts"),
    ),
    FlagSpec::new(
        &["--allow-dangerously-skip-permissions"],
        FlagValue::None,
        "let the session switch permission checks off",
        Weakens("permission prompts, once switched off"),
    ),
    FlagSpec::new(
        &["--strict-mcp-config"],
        FlagValue::None,
        "only the MCP servers given by --mcp-config",
        Degrades("the MCP servers uze delivers"),
    ),
    FlagSpec::new(
        &["--bare"],
        FlagValue::None,
        "minimal mode: no hooks, plugins or CLAUDE.md discovery",
        Degrades("the plugins, hooks and project context uze delivers"),
    ),
    FlagSpec::new(
        &["--safe-mode"],
        FlagValue::None,
        "start with every customization off",
        Degrades("the plugins, skills, hooks and MCP servers uze delivers"),
    ),
    FlagSpec::new(
        &["--restricted"],
        FlagValue::None,
        "no command tools, and user, project and local settings ignored",
        Degrades("the plugins and hooks uze delivers through settings"),
    ),
    FlagSpec::new(
        &["--setting-sources"],
        FlagValue::Free("sources"),
        "which setting sources load (user, project, local)",
        Degrades("the plugins and hooks uze delivers through user settings"),
    ),
    FlagSpec::new(
        &["--disable-slash-commands"],
        FlagValue::None,
        "every skill and command off",
        Degrades("the skills uze delivers"),
    ),
    FlagSpec::new(
        &[START_FLAG],
        FlagValue::Free("uuid"),
        "session id",
        Uze(CONTINUITY),
    ),
    FlagSpec::new(
        &[RESUME_FLAG, "-r"],
        FlagValue::Free("session"),
        "resume a conversation",
        Uze(CONTINUITY),
    ),
    FlagSpec::new(
        &["--continue", "-c"],
        FlagValue::None,
        "continue the latest conversation",
        Uze(CONTINUITY),
    ),
    FlagSpec::new(
        &["--fork-session"],
        FlagValue::None,
        "resume into a new session id",
        Uze(CONTINUITY),
    ),
    FlagSpec::new(
        &["--from-pr"],
        FlagValue::Free("pr"),
        "resume a session linked to a PR",
        Uze(CONTINUITY),
    ),
    FlagSpec::new(
        &["--teleport"],
        FlagValue::Free("session"),
        "resume a teleport session",
        Uze(CONTINUITY),
    ),
    FlagSpec::new(
        &["--print", "-p"],
        FlagValue::None,
        "print one response and exit",
        Uze("a workspace agent is interactive"),
    ),
    FlagSpec::new(
        &[SETTINGS_FLAG],
        FlagValue::Free("file-or-json"),
        "additional settings",
        Uze("uze passes the agent's reporter through it"),
    ),
];

/// `CLAUDE_CONFIG_DIR` is read to find where plugins are delivered; the
/// projection variable is set so an added directory's `CLAUDE.md` loads.
pub(super) const MANAGED_ENVIRONMENT: &[&str] = &[RUNTIME_PROJECTION_ENV_VAR, "CLAUDE_CONFIG_DIR"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::test_support::assert_catalog_in_help;

    #[test]
    fn every_catalogued_spelling_is_in_the_snapshot() {
        assert_catalog_in_help(CATALOG, include_str!("help.txt"));
    }
}
