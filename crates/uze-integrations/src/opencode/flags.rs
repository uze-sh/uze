//! The flags an OpenCode launch takes, as offered to an operator's
//! overrides, and the variables this integration reads.
//!
//! Described from `help.txt`, the `--help` of OpenCode 2.1.3. `--standalone`
//! and `--server` are left out: which server holds the agent's session is
//! what the read-back asks, and they pass as unknown.

use uze_core::flag_catalog::{
    FlagOwner::{Operator, Uze, Weakens},
    FlagSpec, FlagValue,
};

use super::session::{FIRST_TURN_FLAG, RESUME_FLAG};

const CONTINUITY: &str = "uze continues this agent's conversation itself";

pub(super) const CATALOG: &[FlagSpec] = &[
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
    use crate::shared::test_support::assert_catalog_in_help;

    #[test]
    fn every_catalogued_spelling_is_in_the_snapshot() {
        assert_catalog_in_help(CATALOG, include_str!("help.txt"));
    }
}
