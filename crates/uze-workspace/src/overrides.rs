//! Launch overrides: the flags and variables an operator adds to one
//! agent's launch, which never displace what UZE composes for it.
//!
//! An override is typed as one line ([`split`]), read as entries against
//! the harness's catalog ([`entries`]) and judged one by one ([`check`]):
//! a variable or flag UZE sets or depends on is refused, wherever and
//! however it is spelled, and everything else is passed as typed. The
//! integration declares the catalog and its own variables; the rule every
//! harness shares is this module's. The check runs twice, with two
//! strictnesses: without the launch's plan while the operator types, where
//! a key UZE sets only on some launches is a warning, and with it at the
//! launch, where that key is refused exactly when this launch sets it.
//!
//! What was typed is kept in one owner-only record ([`record`]): each
//! agent's overrides per harness, so a relaunch carries them again, and
//! the sets used lately, so one is a gesture away. A value may be a secret,
//! so nothing here traces one: a span says how many overrides a launch
//! carried, never which.

mod check;
mod line;
pub mod record;

use std::ffi::OsString;

use uze_core::flag_catalog::FlagCatalog;

pub use check::{Entry, Verdict, check, entries};
pub use line::{Unterminated, split};

/// What an agent's recorded overrides add to one launch, once checked
/// against what UZE composes for it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LaunchOverrides {
    /// After what UZE prepends and before the first turn it appends.
    pub args: Vec<OsString>,
    /// On top of the inherited environment, under everything UZE sets.
    pub env: Vec<(OsString, OsString)>,
    /// Each override left out and why, naming flags and variables and never
    /// a value.
    pub left_out: Vec<String>,
    /// What the flags passed give up, for the launch to say.
    pub weakens: Vec<String>,
    /// The overrides passed, flag and value counting as one.
    passed: usize,
}

impl LaunchOverrides {
    /// How many overrides this launch carries: what a span may say of them.
    pub fn len(&self) -> usize {
        self.passed
    }

    pub fn is_empty(&self) -> bool {
        self.args.is_empty() && self.env.is_empty()
    }

    /// Leaves out the flags `composed` now collides with: what UZE adds to
    /// the launch after the plan was made, such as switching the harness's
    /// memory off.
    pub fn leave_out_composed(&mut self, catalog: FlagCatalog, composed: &[OsString]) {
        let flags = compose(&self.arg_words(), catalog, &[], composed);
        self.args = flags.args;
        self.left_out.extend(flags.left_out);
        self.passed = flags.passed + self.env.len();
    }

    fn arg_words(&self) -> Vec<String> {
        self.args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }
}

/// `words`, as recorded, checked against the launch UZE `composed`: what
/// the plan-aware check refuses is left out and named, the rest is passed.
#[tracing::instrument(
    name = "overrides.compose",
    level = "debug",
    skip_all,
    fields(overrides = tracing::field::Empty)
)]
pub fn compose(
    words: &[String],
    catalog: FlagCatalog,
    managed: &[&str],
    composed: &[OsString],
) -> LaunchOverrides {
    let entries = entries(words, catalog);
    tracing::Span::current().record("overrides", entries.len());
    let verdicts = check(&entries, catalog, managed, Some(composed));
    let mut overrides = LaunchOverrides::default();
    for (entry, verdict) in entries.iter().zip(verdicts) {
        match verdict {
            Verdict::Refused(reason) => overrides
                .left_out
                .push(format!("{} left out ({reason})", entry.label())),
            verdict => {
                overrides.passed += 1;
                if let Verdict::Weakens(what) = verdict {
                    overrides
                        .weakens
                        .push(format!("{} gives up {what}", entry.label()));
                }
                match entry {
                    Entry::Variable { name, value } => {
                        overrides.env.push((name.into(), value.into()));
                    }
                    entry => overrides
                        .args
                        .extend(entry.words().into_iter().map(OsString::from)),
                }
            }
        }
    }
    overrides
}

#[cfg(test)]
mod tests {
    use super::*;
    use uze_core::flag_catalog::{
        FlagGrammar,
        FlagOwner::{Keyed, Operator, Uze, Weakens},
        FlagSpec, FlagValue,
    };

    const CATALOG: FlagCatalog = FlagCatalog::new(FlagGrammar::Posix, FLAGS);

    const FLAGS: &[FlagSpec] = &[
        FlagSpec::new(
            &["--model", "-m"],
            FlagValue::Free("model"),
            "model",
            Operator,
        ),
        FlagSpec::new(
            &["--resume"],
            FlagValue::Free("id"),
            "resume",
            Uze("uze continues this agent's conversation itself"),
        ),
        FlagSpec::new(
            &["--yolo"],
            FlagValue::None,
            "yolo",
            Weakens("permission prompts"),
        ),
        FlagSpec::new(
            &["--config", "-c"],
            FlagValue::Free("key=value"),
            "config",
            Keyed {
                owned: &[],
                conditional: &["features.memories"],
            },
        ),
    ];

    fn words(line: &str) -> Vec<String> {
        split(line).unwrap()
    }

    fn os(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn a_launch_carries_flags_as_typed_and_variables_in_its_environment() {
        let overrides = compose(
            &words("DEBUG=1 --model opus -mhaiku --new=x"),
            CATALOG,
            &[],
            &[],
        );

        assert_eq!(
            overrides.args,
            os(&["--model", "opus", "-mhaiku", "--new=x"])
        );
        assert_eq!(overrides.env, vec![("DEBUG".into(), "1".into())]);
        assert!(overrides.left_out.is_empty());
        assert_eq!(overrides.len(), 4);
    }

    #[test]
    fn what_uze_owns_is_left_out_naming_it_and_never_its_value() {
        let overrides = compose(
            &words("UZE_HOME=/secret/path --resume abc123 --model opus stray"),
            CATALOG,
            &[],
            &[],
        );

        assert_eq!(overrides.args, os(&["--model", "opus"]));
        assert!(overrides.env.is_empty());
        assert_eq!(
            overrides.left_out,
            [
                "UZE_HOME left out (UZE_HOME is managed by uze)",
                "--resume left out (uze continues this agent's conversation itself)",
                "a positional argument left out (the harness would read it as a prompt)",
            ]
        );
        let said = overrides.left_out.join(" ");
        assert!(
            !said.contains("/secret/path") && !said.contains("abc123") && !said.contains("stray")
        );
    }

    #[test]
    fn a_flag_that_weakens_safety_is_passed_and_said() {
        let overrides = compose(&words("--yolo"), CATALOG, &[], &[]);

        assert_eq!(overrides.args, os(&["--yolo"]));
        assert_eq!(overrides.weakens, ["--yolo gives up permission prompts"]);
    }

    #[test]
    fn a_key_composed_after_the_plan_is_left_out_then() {
        let mut overrides = compose(
            &words("-c features.memories=true --model opus DEBUG=1"),
            CATALOG,
            &[],
            &[],
        );
        assert_eq!(overrides.args.len(), 4);

        overrides.leave_out_composed(CATALOG, &os(&["-c", "features.memories=false"]));

        assert_eq!(overrides.args, os(&["--model", "opus"]));
        assert_eq!(overrides.env.len(), 1);
        assert_eq!(
            overrides.left_out,
            ["-c left out (uze composes features.memories on this launch)"]
        );
    }
}
