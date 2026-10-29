//! The flags a harness takes on its command line, as its integration
//! declares them: what an operator may add to one agent's launch, and what
//! UZE composes for that launch itself and so may not be overridden.
//!
//! Plain data, read from the integration and never from the binary: the
//! catalog is a fact about a release, proven against its `--help` in the
//! integration's tests and by the Lab, not discovered at runtime. A flag the
//! catalog does not list is still the operator's to pass; the catalog
//! completes and guards, it does not allow.

/// One flag, in every spelling the harness accepts for it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FlagSpec {
    /// Long first, then any short or alias: `["--model", "-m"]`. The first
    /// is the one shown.
    pub spellings: &'static [&'static str],
    pub value: FlagValue,
    /// Whether the harness takes it more than once in one launch.
    pub repeatable: bool,
    /// A short description, lowercase, as a completion row shows it.
    pub summary: &'static str,
    pub owner: FlagOwner,
}

impl FlagSpec {
    /// The spelling a person is shown.
    pub fn name(&self) -> &'static str {
        self.spellings[0]
    }

    /// Whether completion may offer it: everything but what UZE composes.
    pub fn is_offered(&self) -> bool {
        !matches!(self.owner, FlagOwner::Uze(_))
    }
}

/// What follows a flag on the command line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlagValue {
    /// A switch: nothing follows it.
    None,
    /// Any value; the hint names what it is (`model`, `dir`).
    Free(&'static str),
    /// One of a fixed set, the only values checked before launch.
    OneOf(&'static [&'static str]),
}

/// Whose flag it is, which decides whether an override carrying it is
/// passed, marked or refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlagOwner {
    /// The operator's: offered and passed as typed.
    Operator,
    /// The operator's, but it weakens the harness's own safety; passed and
    /// marked, naming what is given up.
    Weakens(&'static str),
    /// The operator's, but it stops something UZE delivers from loading;
    /// passed and marked, naming what stops.
    Degrades(&'static str),
    /// Composed by UZE for the launch (session, resume, first prompt,
    /// context): never offered, refused with the reason.
    Uze(&'static str),
    /// A configuration flag whose value names a key: the flag is the
    /// operator's, a key UZE composes is not. `owned` keys are composed on
    /// every launch; `conditional` ones only when the launch's plan does,
    /// so a check without the plan can only warn about them.
    Keyed {
        owned: &'static [&'static str],
        conditional: &'static [&'static str],
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: FlagSpec = FlagSpec {
        spellings: &["--model", "-m"],
        value: FlagValue::Free("model"),
        repeatable: false,
        summary: "model for the session",
        owner: FlagOwner::Operator,
    };

    #[test]
    fn a_flag_is_shown_by_its_first_spelling() {
        assert_eq!(MODEL.name(), "--model");
    }

    #[test]
    fn only_a_flag_uze_composes_is_withheld_from_completion() {
        let owned = FlagSpec {
            owner: FlagOwner::Uze("uze continues this conversation itself"),
            ..MODEL
        };
        let degrading = FlagSpec {
            owner: FlagOwner::Degrades("delivered hooks"),
            ..MODEL
        };

        assert!(MODEL.is_offered());
        assert!(degrading.is_offered());
        assert!(!owned.is_offered());
    }
}
