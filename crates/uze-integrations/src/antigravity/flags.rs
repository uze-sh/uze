//! The flags an Antigravity CLI launch takes, as offered to an operator's
//! overrides. This integration manages no variable of its own.
//!
//! Described from `help.txt`, the `--help` of agy 1.4.10; every flag it
//! defines is catalogued or left out for a reason the tests name, and only
//! flags that act in print mode alone are left out.

use uze_core::flag_catalog::{
    FlagCatalog, FlagGrammar,
    FlagOwner::{Operator, Uze, Weakens},
    FlagSpec, FlagValue,
};

use super::session::{FIRST_TURN_FLAG, RESUME_FLAG};

const CONTINUITY: &str = "uze continues this agent's conversation itself";
const INTERACTIVE: &str = "a workspace agent is interactive";

/// Read as Go's `flag` package reads agy's command line ("Usage of agy:").
pub(super) const CATALOG: FlagCatalog = FlagCatalog::new(FlagGrammar::GoFlag, FLAGS);

const FLAGS: &[FlagSpec] = &[
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
        &["--project"],
        FlagValue::Free("project"),
        "project id or name for the session",
        Operator,
    ),
    FlagSpec::new(
        &["--new-project"],
        FlagValue::None,
        "a new project for the session",
        Operator,
    ),
    FlagSpec::new(
        &["--remote-control"],
        FlagValue::None,
        "a remote connection for the session on startup",
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
    use crate::shared::test_support::{
        LeftOut::{self, PrintOnly},
        assert_catalog_covers_help,
    };

    /// Every flag `help.txt` defines that the catalog does not list. agy's
    /// `--help` defines neither help nor version as flags.
    const LEFT_OUT: &[LeftOut] = &[
        PrintOnly(&["--disable-slash-commands"], "in print mode"),
        PrintOnly(&["--input-format"], "input format for print mode"),
        PrintOnly(&["--output-format"], "output format for print mode"),
        PrintOnly(&["--print-timeout"], "time limit for print mode"),
        PrintOnly(
            &["--json-schema"],
            "structured output, which only print mode's --output-format gives",
        ),
    ];

    #[test]
    fn every_flag_in_the_snapshot_is_catalogued_or_left_out() {
        assert_catalog_covers_help(&CATALOG, LEFT_OUT, include_str!("help.txt"));
    }
}
