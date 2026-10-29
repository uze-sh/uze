//! The flags a Codex launch takes, as offered to an operator's overrides,
//! and the variables this integration reads.
//!
//! Described from `help.txt` and `resume-help.txt`, the `--help` of
//! codex-cli 0.164.0 and of its `resume`: overrides follow `resume <id>` on
//! a resumed launch, so every flag here must be one `codex resume` takes
//! too. Codex has no session flag to own: resuming is a subcommand, and the
//! check refuses a positional. What UZE composes travels as `-c` keys,
//! which `--help` does not list and the Codex conformance proves instead.
//! `--worktree` and `--remote` are left out: they pass as unknown.

use uze_core::flag_catalog::{
    FlagOwner::{Degrades, Keyed, Operator, Uze, Weakens},
    FlagSpec, FlagValue,
};

/// `--enable <feature>` and `--disable <feature>` are, as `--help` says,
/// `-c features.<feature>=true|false`.
const FEATURES: &str = "features";

pub(super) const CATALOG: &[FlagSpec] = &[
    FlagSpec::new(
        &["--config", "-c"],
        FlagValue::Free("key=value"),
        "a config.toml value for this launch",
        Keyed {
            owned: &["agents", "hooks.SessionStart", "hooks.state"],
            conditional: &["notify", "features.memories"],
        },
    )
    .repeatable(),
    FlagSpec::new(
        &["--model", "-m"],
        FlagValue::Free("model"),
        "model the agent uses",
        Operator,
    ),
    FlagSpec::new(
        &["--profile", "-p"],
        FlagValue::Free("profile"),
        "a <name>.config.toml layered on the user config",
        Operator,
    ),
    FlagSpec::new(
        &["--sandbox", "-s"],
        FlagValue::OneOf(&["read-only", "workspace-write", "danger-full-access"]),
        "sandbox for model-run commands; danger-full-access runs them unsandboxed",
        Operator,
    ),
    FlagSpec::new(
        &["--ask-for-approval", "-a"],
        FlagValue::OneOf(&["on-request", "never"]),
        "when a command asks for approval",
        Operator,
    ),
    FlagSpec::new(
        &["--approve-for-me"],
        FlagValue::None,
        "approvals reviewed automatically, in the workspace-write sandbox",
        Operator,
    ),
    FlagSpec::new(
        &["--add-dir"],
        FlagValue::Free("dir"),
        "another writable directory",
        Operator,
    )
    .repeatable(),
    FlagSpec::new(
        &["--image", "-i"],
        FlagValue::Free("file"),
        "an image attached to the first prompt",
        Operator,
    )
    .repeatable(),
    FlagSpec::new(
        &["--enable"],
        FlagValue::Free("feature"),
        "a feature on",
        Operator,
    )
    .sets_key_in(FEATURES)
    .repeatable(),
    FlagSpec::new(&["--search"], FlagValue::None, "live web search", Operator),
    FlagSpec::new(
        &["--oss"],
        FlagValue::None,
        "the open-source provider",
        Operator,
    ),
    FlagSpec::new(
        &["--local-provider"],
        FlagValue::OneOf(&["lmstudio", "ollama"]),
        "which local provider --oss uses",
        Operator,
    ),
    FlagSpec::new(
        &["--strict-config"],
        FlagValue::None,
        "fail on config.toml fields this version does not know",
        Operator,
    ),
    FlagSpec::new(
        &["--no-alt-screen"],
        FlagValue::None,
        "inline mode, keeping the terminal's scrollback",
        Operator,
    ),
    FlagSpec::new(
        &["--no-daemon"],
        FlagValue::None,
        "without the shared background server",
        Operator,
    ),
    FlagSpec::new(
        &["--disable"],
        FlagValue::Free("feature"),
        "a feature off",
        Degrades("what uze delivers through that feature, such as hooks or plugins"),
    )
    .sets_key_in(FEATURES)
    .repeatable(),
    FlagSpec::new(
        &["--dangerously-bypass-approvals-and-sandbox"],
        FlagValue::None,
        "no approval prompts and no sandbox",
        Weakens("approval prompts and the sandbox"),
    ),
    FlagSpec::new(
        &["--dangerously-bypass-hook-trust"],
        FlagValue::None,
        "run hooks without their persisted trust",
        Weakens("hook trust"),
    ),
    FlagSpec::new(
        &["--cd", "-C"],
        FlagValue::Free("dir"),
        "another working root",
        Uze("uze starts the agent in its own checkout, where its conversation is found again"),
    ),
];

/// Read to find where Codex keeps its configuration, sessions and plugins.
pub(super) const MANAGED_ENVIRONMENT: &[&str] = &["CODEX_HOME"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::test_support::assert_catalog_in_help;

    #[test]
    fn every_catalogued_spelling_is_in_the_snapshot() {
        assert_catalog_in_help(CATALOG, include_str!("help.txt"));
    }

    /// A resumed launch is `codex resume <id> <overrides>`, so a flag only
    /// the fresh launch takes would break every resume.
    #[test]
    fn every_catalogued_spelling_is_taken_by_resume() {
        assert_catalog_in_help(CATALOG, include_str!("resume-help.txt"));
    }
}
