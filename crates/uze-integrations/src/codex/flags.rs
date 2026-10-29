//! The flags a Codex launch takes, as offered to an operator's overrides,
//! and the variables this integration reads.
//!
//! Described from `help.txt` and `resume-help.txt`, the `--help` of
//! codex-cli 0.164.0 and of its `resume`: overrides follow `resume <id>` on
//! a resumed launch, so every flag here must be one `codex resume` takes
//! too. Codex has no session flag to own: resuming is a subcommand, and the
//! check refuses a positional. What UZE composes travels as `-c` keys,
//! which `--help` does not list and the Codex conformance proves instead.
//! Every flag the snapshot defines is catalogued or left out for a reason
//! the tests name; `--worktree` and `--remote`, which take the session out
//! of the agent's checkout or off this machine, are UZE's and refused.

use uze_core::flag_catalog::{
    FlagCatalog, FlagGrammar,
    FlagOwner::{Degrades, Keyed, Operator, Uze, Weakens},
    FlagSpec, FlagValue,
};

/// `--enable <feature>` and `--disable <feature>` are, as `--help` says,
/// `-c features.<feature>=true|false`.
const FEATURES: &str = "features";

const IN_ITS_CHECKOUT: &str =
    "uze starts the agent in its own checkout, where its conversation is found again";
const ON_THIS_MACHINE: &str = "uze finds the agent's conversation among this machine's sessions";

/// Read as clap reads Codex's command line.
pub(super) const CATALOG: FlagCatalog = FlagCatalog::new(FlagGrammar::Posix, FLAGS);

const FLAGS: &[FlagSpec] = &[
    FlagSpec::new(
        &["--config", "-c"],
        FlagValue::Free("key=value"),
        "a config.toml value for this launch",
        Keyed {
            owned: &["agents", "hooks.SessionStart", "hooks.state"],
            conditional: &["notify", "features.memories"],
        },
    )
    // The settings `--sandbox` and `--ask-for-approval` stand for, which
    // give up as much as their weakening values.
    .weakens_with(&[
        ("sandbox_mode=danger-full-access", "the sandbox"),
        ("approval_policy=never", "approval prompts"),
    ])
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
    )
    .weakens_with(&[("danger-full-access", "the sandbox")]),
    FlagSpec::new(
        &["--ask-for-approval", "-a"],
        FlagValue::OneOf(&["on-request", "never"]),
        "when a command asks for approval",
        Operator,
    )
    .weakens_with(&[("never", "approval prompts")]),
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
        Uze(IN_ITS_CHECKOUT),
    ),
    FlagSpec::new(
        &["--worktree"],
        FlagValue::None,
        "the session in a new managed git worktree",
        Uze(IN_ITS_CHECKOUT),
    ),
    FlagSpec::new(
        &["--remote"],
        FlagValue::Free("addr"),
        "the TUI on a remote app server",
        Uze(ON_THIS_MACHINE),
    ),
    FlagSpec::new(
        &["--remote-auth-token-env"],
        FlagValue::Free("env_var"),
        "variable holding the remote app server's token",
        Uze("it takes --remote, and uze finds the agent's conversation on this machine"),
    ),
];

/// Read to find where Codex keeps its configuration, sessions and plugins.
pub(super) const MANAGED_ENVIRONMENT: &[&str] = &["CODEX_HOME"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::test_support::{
        LeftOut::{self, EndsTheLaunch},
        assert_catalog_covers_help, assert_catalog_in_help,
    };

    /// Every flag `help.txt` defines that the catalog does not list.
    const LEFT_OUT: &[LeftOut] = &[
        EndsTheLaunch(&["--help", "-h"]),
        EndsTheLaunch(&["--version", "-V"]),
    ];

    #[test]
    fn every_flag_in_the_snapshot_is_catalogued_or_left_out() {
        assert_catalog_covers_help(&CATALOG, LEFT_OUT, include_str!("help.txt"));
    }

    /// A resumed launch is `codex resume <id> <overrides>`, so a flag only
    /// the fresh launch takes would break every resume.
    #[test]
    fn every_catalogued_spelling_is_taken_by_resume() {
        assert_catalog_in_help(&CATALOG, include_str!("resume-help.txt"));
    }
}
