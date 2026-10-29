//! The flags an Antigravity CLI launch takes, as offered to an operator's
//! overrides. This integration manages no variable of its own.
//!
//! Described from `help.txt`, the `--help` of agy 1.4.10. `--project`,
//! `--new-project` and `--remote-control` are left out, and so is
//! `--disable-slash-commands`, which acts in print mode only: they pass as
//! unknown.

use uze_core::flag_catalog::{
    FlagOwner::{Operator, Uze, Weakens},
    FlagSpec, FlagValue,
};

use super::session::{FIRST_TURN_FLAG, RESUME_FLAG};

const CONTINUITY: &str = "uze continues this agent's conversation itself";
const INTERACTIVE: &str = "a workspace agent is interactive";

pub(super) const CATALOG: &[FlagSpec] = &[
    FlagSpec::new(
        &["--model"],
        FlagValue::Free("model"),
        "model for the session",
        Operator,
    ),
    FlagSpec::new(
        &["--effort"],
        FlagValue::OneOf(&["low", "medium", "high", "xhigh", "max"]),
        "reasoning effort for the session",
        Operator,
    ),
    FlagSpec::new(
        &["--mode"],
        FlagValue::OneOf(&["accept-edits", "auto", "plan"]),
        "the agent's execution mode",
        Operator,
    ),
    FlagSpec::new(
        &["--add-dir"],
        FlagValue::Free("dir"),
        "another directory in the workspace",
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
        &["--settings"],
        FlagValue::Free("json-or-file"),
        "settings for this session only, never saved",
        Operator,
    ),
    FlagSpec::new(
        &["--sandbox"],
        FlagValue::None,
        "terminal restrictions on",
        Operator,
    ),
    FlagSpec::new(
        &["--log-file"],
        FlagValue::Free("path"),
        "another log file",
        Operator,
    ),
    FlagSpec::new(
        &["--dangerously-skip-permissions"],
        FlagValue::None,
        "approve every tool permission without a prompt",
        Weakens("permission prompts"),
    ),
    FlagSpec::new(
        &[RESUME_FLAG],
        FlagValue::Free("id"),
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
        &["--prompt-interactive", FIRST_TURN_FLAG],
        FlagValue::Free("prompt"),
        "first prompt, then the session",
        Uze("uze gives the agent's first turn itself"),
    ),
    FlagSpec::new(
        &["--print", "-p", "--prompt"],
        FlagValue::None,
        "one prompt, printed, then exit",
        Uze(INTERACTIVE),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::test_support::assert_catalog_in_help;

    #[test]
    fn every_catalogued_spelling_is_in_the_snapshot() {
        assert_catalog_in_help(CATALOG, include_str!("help.txt"));
    }
}
