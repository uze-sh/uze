//! The flags an OpenCode launch takes, as offered to an operator's
//! overrides, and the variables this integration reads.
//!
//! Described from `help.txt`, the `--help` of OpenCode 2.1.3; every flag it
//! defines is catalogued or left out for a reason the tests name. Which
//! server holds the agent's session is what the read-back asks of the
//! background service, so `--server` is UZE's, and `--standalone` is marked
//! for the conversation uze may not find again.

use uze_core::flag_catalog::{
    FlagCatalog, FlagGrammar,
    FlagOwner::{Degrades, Operator, Uze, Weakens},
    FlagSpec, FlagValue,
};

use super::session::{FIRST_TURN_FLAG, RESUME_FLAG};

const CONTINUITY: &str = "uze continues this agent's conversation itself";

/// Read as Effect's CLI reads OpenCode's command line.
pub(super) const CATALOG: FlagCatalog = FlagCatalog::new(FlagGrammar::EffectCli, FLAGS);

const FLAGS: &[FlagSpec] = &[
    FlagSpec::new(
        &["--log-level"],
        FlagValue::OneOf(&[
            "all", "trace", "debug", "info", "warn", "warning", "error", "fatal", "none",
        ]),
        "the minimum log level",
        Operator,
    ),
    FlagSpec::new(
        &["--print-logs"],
        FlagValue::None,
        "logs printed to stderr",
        Operator,
    ),
    FlagSpec::new(
        &["--auto"],
        FlagValue::None,
        "approve every permission not explicitly denied",
        Weakens("permission prompts"),
    ),
    FlagSpec::new(
        &["--standalone"],
        FlagValue::None,
        "a private server instead of the background service",
        Degrades("uze continuing this conversation, which it asks the background service for"),
    ),
    FlagSpec::new(
        &["--server"],
        FlagValue::Free("url"),
        "a server URL instead of the background service",
        Uze("uze finds the agent's conversation on the background service"),
    ),
    FlagSpec::new(
        &[RESUME_FLAG, "-s"],
        FlagValue::Free("session"),
        "session to continue",
        Uze(CONTINUITY),
    ),
    FlagSpec::new(
        &["--continue", "-c"],
        FlagValue::None,
        "continue the last session",
        Uze(CONTINUITY),
    ),
    FlagSpec::new(
        &[FIRST_TURN_FLAG],
        FlagValue::Free("prompt"),
        "first prompt",
        Uze("uze gives the agent's first turn itself"),
    ),
];

/// Read to find where OpenCode keeps its configuration and its data.
pub(super) const MANAGED_ENVIRONMENT: &[&str] = &["XDG_CONFIG_HOME", "XDG_DATA_HOME"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::test_support::{
        LeftOut::{self, EndsTheLaunch},
        assert_catalog_covers_help,
    };

    /// Every flag `help.txt` defines that the catalog does not list.
    const LEFT_OUT: &[LeftOut] = &[
        EndsTheLaunch(&["--help", "-h"]),
        EndsTheLaunch(&["--version", "-v"]),
        EndsTheLaunch(&["--wizard"]),
        EndsTheLaunch(&["--completions"]),
    ];

    #[test]
    fn every_flag_in_the_snapshot_is_catalogued_or_left_out() {
        assert_catalog_covers_help(&CATALOG, LEFT_OUT, include_str!("help.txt"));
    }
}
