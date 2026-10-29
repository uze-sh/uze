//! The flags a harness takes on its command line, as its integration
//! declares them: what an operator may add to one agent's launch, and what
//! UZE composes for that launch itself and so may not be overridden.
//!
//! Plain data, read from the integration and never from the binary: the
//! catalog is a fact about a release, proven against its `--help` in the
//! integration's tests and by the Lab, not discovered at runtime. A flag the
//! catalog does not list is still the operator's to pass; the catalog
//! completes and guards, it does not allow.

/// A harness's flags and the grammar its command line is read in: the same
/// word is a different flag to different parsers, so a spelling is only
/// matched the way the harness itself would read it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlagCatalog {
    pub grammar: FlagGrammar,
    pub flags: &'static [FlagSpec],
}

impl FlagCatalog {
    pub const fn new(grammar: FlagGrammar, flags: &'static [FlagSpec]) -> Self {
        Self { grammar, flags }
    }
}

impl std::ops::Deref for FlagCatalog {
    type Target = [FlagSpec];

    fn deref(&self) -> &[FlagSpec] {
        self.flags
    }
}

/// How a harness's parser reads a word that starts with one dash. Named
/// after the parser family, so a catalog says which one its harness uses
/// and the check reads a word as that harness would.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FlagGrammar {
    /// getopt's family (clap, commander): a long flag takes two dashes;
    /// one dash starts a cluster of short switches (`-cd` is `-c -d`), and
    /// a short flag that takes a value takes the rest of its word
    /// (`-mopus`), so `-model` is `-m odel`.
    #[default]
    Posix,
    /// Go's `flag` package: one dash or two name the same flag
    /// (`-model`, `--model=x`), and there are no clusters.
    GoFlag,
    /// Effect's CLI: `-name=value` names a flag, long or short, while a
    /// single-dash word without `=` is a cluster of short flags, none of
    /// which takes the rest of the word.
    EffectCli,
}

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
    /// The configuration table whose key the flag's value names, when the
    /// flag is a configuration key in disguise (`--enable <feature>` sets
    /// `features.<feature>`): that key is then checked as the catalog's
    /// [`FlagOwner::Keyed`] flag checks its own.
    pub sets_key_in: Option<&'static str>,
    /// Values of a [`FlagValue::OneOf`] that give up the harness's own
    /// safety as a [`FlagOwner::Weakens`] switch does, each with what it
    /// gives up: `--sandbox danger-full-access` is no sandbox.
    pub weakening_values: &'static [(&'static str, &'static str)],
}

impl FlagSpec {
    /// A flag given once per launch; [`Self::repeatable`] says otherwise.
    pub const fn new(
        spellings: &'static [&'static str],
        value: FlagValue,
        summary: &'static str,
        owner: FlagOwner,
    ) -> Self {
        Self {
            spellings,
            value,
            repeatable: false,
            summary,
            owner,
            sets_key_in: None,
            weakening_values: &[],
        }
    }

    /// A flag some of whose values give up the harness's own safety, each
    /// with what it gives up.
    pub const fn weakens_with(self, values: &'static [(&'static str, &'static str)]) -> Self {
        Self {
            weakening_values: values,
            ..self
        }
    }

    /// What `value` gives up, when it is one of the values that weaken.
    pub fn what_value_weakens(&self, value: &str) -> Option<&'static str> {
        self.weakening_values
            .iter()
            .find(|(weakening, _)| *weakening == value)
            .map(|(_, what)| *what)
    }

    /// A flag whose value names a key in the configuration table `table`.
    pub const fn sets_key_in(self, table: &'static str) -> Self {
        Self {
            sets_key_in: Some(table),
            ..self
        }
    }

    /// The configuration key `value` sets through this flag, if it sets one.
    pub fn key_set_by(&self, value: &str) -> Option<String> {
        self.sets_key_in.map(|table| format!("{table}.{value}"))
    }

    pub const fn repeatable(self) -> Self {
        Self {
            repeatable: true,
            ..self
        }
    }

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
    /// A value that may be left out (`--debug [filter]`): only an attached
    /// one is its (`--debug=mcp`, `-dmcp`), so the word after it is never
    /// taken. Offered as a switch; the summary names the attached form.
    Optional(&'static str),
}

impl FlagValue {
    /// Whether a value may be attached to the flag, which a short flag in
    /// a cluster then takes as the rest of its word.
    pub fn takes_a_value(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Whether the flag needs a value, and so takes the word after it when
    /// none is attached.
    pub fn needs_a_value(&self) -> bool {
        matches!(self, Self::Free(_) | Self::OneOf(_))
    }
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

    const MODEL: FlagSpec = FlagSpec::new(
        &["--model", "-m"],
        FlagValue::Free("model"),
        "model for the session",
        FlagOwner::Operator,
    );

    #[test]
    fn a_flag_is_given_once_unless_declared_repeatable() {
        let model = MODEL;

        assert!(!model.repeatable);
        assert!(model.repeatable().repeatable);
    }

    #[test]
    fn a_flag_naming_a_key_in_a_table_sets_that_key() {
        let enable = FlagSpec::new(
            &["--enable"],
            FlagValue::Free("feature"),
            "a feature on",
            FlagOwner::Operator,
        );

        assert_eq!(enable.key_set_by("web"), None);
        assert_eq!(
            enable.sets_key_in("features").key_set_by("web").as_deref(),
            Some("features.web")
        );
    }

    #[test]
    fn only_a_value_declared_weakening_gives_anything_up() {
        let sandbox = FlagSpec::new(
            &["--sandbox"],
            FlagValue::OneOf(&["read-only", "off"]),
            "sandbox",
            FlagOwner::Operator,
        )
        .weakens_with(&[("off", "the sandbox")]);

        assert_eq!(sandbox.what_value_weakens("off"), Some("the sandbox"));
        assert_eq!(sandbox.what_value_weakens("read-only"), None);
    }

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
