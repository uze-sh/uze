//! Reading split words as entries against a harness's catalog, and judging
//! each one against what UZE composes for the launch.

use std::ffi::OsString;

use uze_core::flag_catalog::{FlagOwner, FlagSpec, FlagValue};

/// One thing an override adds to a launch, as the words that carry it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Entry {
    /// `NAME=value`, set in the harness's environment.
    Variable { name: String, value: String },
    /// A flag in the spelling typed (`--model`, `-m`), the value it took,
    /// and the words it was given in, which are what reaches the harness.
    Flag {
        name: String,
        value: Option<String>,
        words: Vec<String>,
    },
    /// A word no flag takes, which the harness would read as a prompt.
    Positional(String),
    /// A literal `--`.
    EndOfOptions,
}

impl Entry {
    /// The words this entry is, as typed.
    pub fn words(&self) -> Vec<String> {
        match self {
            Self::Variable { name, value } => vec![format!("{name}={value}")],
            Self::Flag { words, .. } => words.clone(),
            Self::Positional(word) => vec![word.clone()],
            Self::EndOfOptions => vec!["--".to_owned()],
        }
    }

    /// What a note may say this entry is: a variable's name or a flag's
    /// spelling, never a value, which may be a secret.
    pub fn label(&self) -> &str {
        match self {
            Self::Variable { name, .. } | Self::Flag { name, .. } => name,
            Self::Positional(_) => "a positional argument",
            Self::EndOfOptions => "--",
        }
    }
}

/// What an entry gets at a launch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// The operator's, and the catalog knows it.
    Accepted,
    /// A flag the catalog does not list: passed as typed.
    Unknown,
    /// Passed, giving up what it names.
    Weakens(&'static str),
    /// Passed, stopping what it names from loading.
    Degrades(&'static str),
    /// Never passed, for the reason given.
    Refused(String),
}

impl Verdict {
    pub fn is_refused(&self) -> bool {
        matches!(self, Self::Refused(_))
    }
}

/// What every harness's launch carries from UZE, whatever its integration
/// declares: its own variables, where the harness and its shims are found,
/// and the trace the launch belongs to.
const SHARED_VARIABLES: &[&str] = &["PATH", "HOME", "TRACEPARENT", "TRACESTATE"];
const UZE_PREFIX: &str = "UZE_";

const MANAGED: &str = "managed by uze";
const A_PROMPT: &str = "the harness would read it as a prompt";
const ENDS_OPTIONS: &str = "the harness would read what follows as a prompt";

/// `words` as entries. A flag the catalog says takes a value takes the
/// word after it, or the value attached to it (`--model=opus`, `-mopus`);
/// one it does not know takes a value only as `--flag=value`, so the word
/// after it stays a word of its own.
pub fn entries(words: &[String], catalog: &[FlagSpec]) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut rest = words.iter();
    while let Some(word) = rest.next() {
        if word == "--" {
            entries.push(Entry::EndOfOptions);
        } else if let Some((name, value)) = variable(word) {
            entries.push(Entry::Variable {
                name: name.to_owned(),
                value: value.to_owned(),
            });
        } else if word.len() > 1 && word.starts_with('-') {
            let (name, attached) = flag_and_attached(word, catalog);
            let takes_value = spec(catalog, name).is_some_and(|spec| spec.value != FlagValue::None);
            let mut flag_words = vec![word.clone()];
            let value = match attached {
                Some(value) => Some(value.to_owned()),
                None if takes_value => rest
                    .next()
                    .inspect(|next| flag_words.push((*next).clone()))
                    .cloned(),
                None => None,
            };
            entries.push(Entry::Flag {
                name: name.to_owned(),
                value,
                words: flag_words,
            });
        } else {
            entries.push(Entry::Positional(word.clone()));
        }
    }
    entries
}

/// One verdict per entry, in order.
///
/// `managed` is the variables the harness's integration sets or reads.
/// `composed` is what UZE composes for this very launch, when it is known:
/// without it a configuration key UZE sets only on some launches can only
/// be warned about, with it that key is refused exactly when this launch
/// sets it.
pub fn check(
    entries: &[Entry],
    catalog: &[FlagSpec],
    managed: &[&str],
    composed: Option<&[OsString]>,
) -> Vec<Verdict> {
    let composed_keys = composed.map(|args| composed_keys(args, catalog));
    entries
        .iter()
        .map(|entry| match entry {
            Entry::Variable { name, .. } => variable_verdict(name, managed),
            Entry::Flag { name, value, .. } => {
                match (cluster_refusal(name, catalog), spec(catalog, name)) {
                    (Some(refusal), _) => refusal,
                    (None, None) => Verdict::Unknown,
                    (None, Some(spec)) => {
                        flag_verdict(spec, value.as_deref(), catalog, composed_keys.as_deref())
                    }
                }
            }
            Entry::Positional(_) => Verdict::Refused(A_PROMPT.to_owned()),
            Entry::EndOfOptions => Verdict::Refused(ENDS_OPTIONS.to_owned()),
        })
        .collect()
}

fn variable_verdict(name: &str, managed: &[&str]) -> Verdict {
    // Compared without case: Windows reads `Path` as `PATH`.
    let owned = name
        .get(..UZE_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(UZE_PREFIX))
        || SHARED_VARIABLES
            .iter()
            .chain(managed)
            .any(|managed| managed.eq_ignore_ascii_case(name));
    if owned {
        Verdict::Refused(format!("{name} is {MANAGED}"))
    } else {
        Verdict::Accepted
    }
}

fn flag_verdict(
    spec: &FlagSpec,
    value: Option<&str>,
    catalog: &[FlagSpec],
    composed_keys: Option<&[String]>,
) -> Verdict {
    if let FlagOwner::Uze(reason) = spec.owner {
        return Verdict::Refused(reason.to_owned());
    }
    let value = match (spec.value, value) {
        (FlagValue::None, _) => None,
        (FlagValue::Free(hint), None) => return Verdict::Refused(format!("takes a value: {hint}")),
        (FlagValue::OneOf(accepted), value)
            if !value.is_some_and(|value| accepted.contains(&value)) =>
        {
            return Verdict::Refused(format!("takes one of {}", accepted.join(", ")));
        }
        (_, value) => value,
    };
    let own = match spec.owner {
        FlagOwner::Operator => match value.and_then(|value| spec.what_value_weakens(value)) {
            Some(what) => Verdict::Weakens(what),
            None => Verdict::Accepted,
        },
        FlagOwner::Weakens(what) => Verdict::Weakens(what),
        FlagOwner::Degrades(what) => Verdict::Degrades(what),
        FlagOwner::Uze(_) => unreachable!("refused above"),
        FlagOwner::Keyed { owned, conditional } => {
            return keyed_verdict(value.unwrap_or_default(), owned, conditional, composed_keys);
        }
    };
    match spec.key_set_by(value.unwrap_or_default()) {
        Some(key) => key_in_disguise_verdict(&key, own, catalog, composed_keys),
        None => own,
    }
}

/// The verdict of a flag that sets `key` as the catalog's configuration
/// flag would: refused when that flag would refuse the key, warned about
/// when the key or the flag itself warns, and the flag's own otherwise.
fn key_in_disguise_verdict(
    key: &str,
    own: Verdict,
    catalog: &[FlagSpec],
    composed_keys: Option<&[String]>,
) -> Verdict {
    let keyed = catalog.iter().find_map(|spec| match spec.owner {
        FlagOwner::Keyed { owned, conditional } => Some(keyed_verdict(
            &format!("{key}="),
            owned,
            conditional,
            composed_keys,
        )),
        _ => None,
    });
    match (keyed, own) {
        (Some(refused @ Verdict::Refused(_)), _) => refused,
        (Some(warned @ Verdict::Degrades(_)), Verdict::Accepted) => warned,
        (_, own) => own,
    }
}

fn keyed_verdict(
    value: &str,
    owned: &[&str],
    conditional: &[&str],
    composed_keys: Option<&[String]>,
) -> Verdict {
    let Some(key) = config_key(value) else {
        return Verdict::Refused("takes key=value".to_owned());
    };
    if let Some(owned) = owned.iter().find(|owned| collides(&key, owned)) {
        return Verdict::Refused(format!("uze composes {owned} on every launch"));
    }
    match composed_keys {
        None if conditional.iter().any(|maybe| collides(&key, maybe)) => {
            Verdict::Degrades("uze may set this")
        }
        None => Verdict::Accepted,
        Some(composed) => match composed.iter().find(|set| collides(&key, set)) {
            Some(set) => Verdict::Refused(format!("uze composes {set} on this launch")),
            None => Verdict::Accepted,
        },
    }
}

/// The configuration keys `args` set through a keyed flag.
fn composed_keys(args: &[OsString], catalog: &[FlagSpec]) -> Vec<String> {
    let words: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .take_while(|word| word != "--")
        .collect();
    entries(&words, catalog)
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Flag {
                name,
                value: Some(value),
                ..
            } if spec(catalog, &name)
                .is_some_and(|spec| matches!(spec.owner, FlagOwner::Keyed { .. })) =>
            {
                config_key(&value)
            }
            _ => None,
        })
        .collect()
}

/// The dotted key `key=value` sets, quotes dropped: `"hooks".state` is
/// `hooks.state`.
fn config_key(value: &str) -> Option<String> {
    let (key, _) = value.split_once('=')?;
    let key: String = key
        .split('.')
        .map(|part| part.trim().trim_matches(['"', '\'']))
        .collect::<Vec<_>>()
        .join(".");
    (!key.is_empty()).then_some(key)
}

/// Whether setting `key` sets `other` or part of it: the same key, a table
/// holding it, or a key inside it.
fn collides(key: &str, other: &str) -> bool {
    let within = |inner: &str, outer: &str| {
        inner
            .strip_prefix(outer)
            .is_some_and(|rest| rest.starts_with('.'))
    };
    key == other || within(other, key) || within(key, other)
}

/// `NAME` and `value` of a `NAME=value` whose name a shell would take.
fn variable(word: &str) -> Option<(&str, &str)> {
    let (name, value) = word.split_once('=')?;
    let mut characters = name.chars();
    let first = characters.next()?;
    ((first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_'))
    .then_some((name, value))
}

/// The flag `word` names and the value attached to it, if any: after the
/// `=` of a long flag, or after a short flag the catalog says takes one.
fn flag_and_attached<'w>(word: &'w str, catalog: &[FlagSpec]) -> (&'w str, Option<&'w str>) {
    if word.starts_with("--") {
        return match word.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (word, None),
        };
    }
    if spelled(catalog, word).is_some() {
        return (word, None);
    }
    let (name, attached) = match word.split_once('=') {
        Some((name, value)) => (name, Some(value)),
        None => (word, None),
    };
    if spec(catalog, name).is_some() {
        return (name, attached);
    }
    let Some((at, _)) = word.char_indices().nth(2) else {
        return (word, None);
    };
    let (short, attached) = word.split_at(at);
    match spec(catalog, short) {
        Some(spec) if spec.value != FlagValue::None => (short, Some(attached)),
        _ => (word, None),
    }
}

/// The refusal a single-dash word earns when a harness would read it as
/// several short switches (`-cd` as `-c -d`) and one of them is UZE's or a
/// configuration flag, whose key cannot be checked inside a cluster. The
/// switches are read until one that takes a value, which takes the rest.
fn cluster_refusal(word: &str, catalog: &[FlagSpec]) -> Option<Verdict> {
    let letters = word
        .strip_prefix('-')
        .filter(|letters| !letters.starts_with('-') && letters.chars().count() > 1)?;
    if spelled(catalog, word).is_some() {
        return None;
    }
    for letter in letters.chars() {
        let Some(spec) = spelled(catalog, &format!("-{letter}")) else {
            continue;
        };
        match spec.owner {
            FlagOwner::Uze(reason) => return Some(Verdict::Refused(reason.to_owned())),
            FlagOwner::Keyed { .. } => return Some(Verdict::Refused("takes key=value".to_owned())),
            _ if spec.value != FlagValue::None => return None,
            _ => {}
        }
    }
    None
}

/// The flag `word` names: one of its spellings, or one of its long
/// spellings written with a single dash (`-print` for `--print`), which
/// some harnesses read as the long flag.
fn spec<'c>(catalog: &'c [FlagSpec], word: &str) -> Option<&'c FlagSpec> {
    spelled(catalog, word).or_else(|| {
        word.strip_prefix('-')
            .filter(|name| name.len() > 1 && !name.starts_with('-'))
            .and_then(|name| spelled(catalog, &format!("--{name}")))
    })
}

fn spelled<'c>(catalog: &'c [FlagSpec], spelling: &str) -> Option<&'c FlagSpec> {
    catalog
        .iter()
        .find(|spec| spec.spellings.contains(&spelling))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uze_core::flag_catalog::FlagOwner::{Degrades, Keyed, Operator, Uze, Weakens};

    const CONTINUITY: &str = "uze continues this agent's conversation itself";

    /// A catalog shaped like the real ones, naming no harness.
    const CATALOG: &[FlagSpec] = &[
        FlagSpec::new(
            &["--model", "-m"],
            FlagValue::Free("model"),
            "model",
            Operator,
        ),
        FlagSpec::new(
            &["--permission-mode"],
            FlagValue::OneOf(&["default", "plan", "bypass"]),
            "permission mode",
            Operator,
        )
        .weakens_with(&[("bypass", "permission prompts")]),
        FlagSpec::new(&["--verbose", "-v"], FlagValue::None, "verbose", Operator),
        FlagSpec::new(
            &["--resume", "-r"],
            FlagValue::Free("id"),
            "resume",
            Uze(CONTINUITY),
        ),
        FlagSpec::new(
            &["--continue", "-c"],
            FlagValue::None,
            "continue",
            Uze(CONTINUITY),
        ),
        FlagSpec::new(
            &["--skip-permissions"],
            FlagValue::None,
            "skip",
            Weakens("permission prompts"),
        ),
        FlagSpec::new(
            &["--strict-mcp-config"],
            FlagValue::None,
            "strict",
            Degrades("the MCP servers uze delivers"),
        ),
    ];

    const KEYED: &[FlagSpec] = &[
        FlagSpec::new(
            &["--config", "-c"],
            FlagValue::Free("key=value"),
            "config",
            Keyed {
                owned: &["agents", "hooks.SessionStart", "hooks.state"],
                conditional: &["notify", "features.memories"],
            },
        )
        .repeatable(),
        FlagSpec::new(&["--enable"], FlagValue::Free("feature"), "on", Operator)
            .sets_key_in("features"),
        FlagSpec::new(
            &["--disable"],
            FlagValue::Free("feature"),
            "off",
            Degrades("hooks"),
        )
        .sets_key_in("features"),
        FlagSpec::new(&["--hook"], FlagValue::Free("hook"), "hook", Operator).sets_key_in("hooks"),
    ];

    const MANAGED: &[&str] = &["CLAUDE_CONFIG_DIR"];

    fn words(line: &str) -> Vec<String> {
        crate::overrides::split(line).unwrap()
    }

    fn verdicts(line: &str, catalog: &[FlagSpec]) -> Vec<Verdict> {
        check(&entries(&words(line), catalog), catalog, MANAGED, None)
    }

    fn planned(line: &str, composed: &[&str]) -> Vec<Verdict> {
        let composed: Vec<OsString> = composed.iter().map(OsString::from).collect();
        check(&entries(&words(line), KEYED), KEYED, &[], Some(&composed))
    }

    fn refused(reason: &str) -> Verdict {
        Verdict::Refused(reason.to_owned())
    }

    #[test]
    fn a_line_reads_as_variables_flags_and_their_values() {
        assert_eq!(
            entries(
                &words("DEBUG=1 --model opus --verbose -mhaiku --model=sonnet"),
                CATALOG
            ),
            [
                Entry::Variable {
                    name: "DEBUG".into(),
                    value: "1".into()
                },
                Entry::Flag {
                    name: "--model".into(),
                    value: Some("opus".into()),
                    words: vec!["--model".into(), "opus".into()]
                },
                Entry::Flag {
                    name: "--verbose".into(),
                    value: None,
                    words: vec!["--verbose".into()]
                },
                Entry::Flag {
                    name: "-m".into(),
                    value: Some("haiku".into()),
                    words: vec!["-mhaiku".into()]
                },
                Entry::Flag {
                    name: "--model".into(),
                    value: Some("sonnet".into()),
                    words: vec!["--model=sonnet".into()]
                },
            ]
        );
    }

    #[test]
    fn a_word_that_is_not_a_variable_name_is_not_a_variable() {
        assert_eq!(
            entries(&words("1X=2 A-B=c"), CATALOG),
            [
                Entry::Positional("1X=2".into()),
                Entry::Positional("A-B=c".into())
            ]
        );
    }

    #[test]
    fn the_words_an_entry_was_given_in_are_what_reaches_the_harness() {
        let line = words("DEBUG=1 --model opus -mhaiku --brand-new=x --");
        let rebuilt: Vec<String> = entries(&line, CATALOG)
            .iter()
            .flat_map(Entry::words)
            .collect();
        assert_eq!(rebuilt, line);
    }

    #[test]
    fn overriding_uzes_home_is_refused_naming_it() {
        assert_eq!(
            verdicts("UZE_HOME=/tmp/x", CATALOG),
            [refused("UZE_HOME is managed by uze")]
        );
    }

    #[test]
    fn every_variable_the_launch_carries_from_uze_is_refused() {
        for name in [
            "UZE_AGENT",
            "uze_pane",
            "PATH",
            "Path",
            "HOME",
            "TRACEPARENT",
            "TRACESTATE",
        ] {
            assert!(
                verdicts(&format!("{name}=x"), CATALOG)[0].is_refused(),
                "{name}"
            );
        }
        assert_eq!(
            verdicts("DEBUG=1 UZEFUL=1", CATALOG),
            [Verdict::Accepted, Verdict::Accepted]
        );
    }

    #[test]
    fn a_variable_the_integration_manages_is_refused_with_the_same_reason() {
        assert_eq!(
            verdicts("CLAUDE_CONFIG_DIR=/elsewhere", CATALOG),
            [refused("CLAUDE_CONFIG_DIR is managed by uze")]
        );
    }

    #[test]
    fn a_resume_flag_is_refused_saying_uze_continues_the_conversation() {
        assert_eq!(verdicts("--resume abc", CATALOG), [refused(CONTINUITY)]);
        assert_eq!(verdicts("--resume=abc", CATALOG), [refused(CONTINUITY)]);
    }

    #[test]
    fn the_short_spelling_of_an_owned_flag_is_refused_as_its_long_one_is() {
        assert_eq!(verdicts("-r abc", CATALOG), [refused(CONTINUITY)]);
        assert_eq!(verdicts("-rabc", CATALOG), [refused(CONTINUITY)]);
        assert_eq!(verdicts("-c", CATALOG), [refused(CONTINUITY)]);
    }

    #[test]
    fn a_cluster_of_switches_holding_an_owned_one_is_refused_with_its_reason() {
        assert_eq!(verdicts("-vc", CATALOG), [refused(CONTINUITY)]);
        assert_eq!(verdicts("-cv", CATALOG), [refused(CONTINUITY)]);
        assert_eq!(verdicts("-xc", KEYED), [refused("takes key=value")]);
        assert_eq!(verdicts("-vx", CATALOG), [Verdict::Unknown]);
        // What follows a switch that takes a value is that value.
        assert_eq!(verdicts("-vmc", CATALOG), [Verdict::Unknown]);
    }

    #[test]
    fn a_long_flag_written_with_one_dash_is_read_as_that_flag() {
        assert_eq!(verdicts("-resume abc", CATALOG), [refused(CONTINUITY)]);
        assert_eq!(verdicts("-resume=abc", CATALOG), [refused(CONTINUITY)]);
        assert_eq!(verdicts("-continue", CATALOG), [refused(CONTINUITY)]);
        assert_eq!(
            verdicts("-model", CATALOG),
            [refused("takes a value: model")]
        );
        assert_eq!(
            entries(&words("-model opus"), CATALOG),
            [Entry::Flag {
                name: "-model".into(),
                value: Some("opus".into()),
                words: vec!["-model".into(), "opus".into()]
            }]
        );
        assert_eq!(verdicts("-model=opus", CATALOG), [Verdict::Accepted]);
    }

    #[test]
    fn a_configuration_key_uze_composes_is_refused_and_another_accepted() {
        let owned = refused("uze composes hooks.state on every launch");
        assert_eq!(
            verdicts("-c hooks.state=x", KEYED),
            std::slice::from_ref(&owned)
        );
        assert_eq!(
            verdicts("-chooks.state=x", KEYED),
            std::slice::from_ref(&owned)
        );
        assert_eq!(
            verdicts("--config hooks.state=x", KEYED),
            std::slice::from_ref(&owned)
        );
        assert_eq!(
            verdicts("--config=hooks.state=x", KEYED),
            std::slice::from_ref(&owned)
        );
        assert_eq!(verdicts(r#"-c '"hooks" . state=x'"#, KEYED), [owned]);
        assert_eq!(
            verdicts("-c model=o3 -c hooksfoo=1", KEYED),
            [Verdict::Accepted, Verdict::Accepted]
        );
    }

    #[test]
    fn a_table_holding_an_owned_key_or_a_key_inside_one_is_refused() {
        assert_eq!(
            verdicts("-c hooks={}", KEYED),
            [refused("uze composes hooks.SessionStart on every launch")]
        );
        assert_eq!(
            verdicts("-c agents.max=2", KEYED),
            [refused("uze composes agents on every launch")]
        );
    }

    #[test]
    fn a_keyed_flag_without_a_key_is_refused() {
        assert_eq!(verdicts("-c model", KEYED), [refused("takes key=value")]);
        assert_eq!(verdicts("-c", KEYED), [refused("takes a value: key=value")]);
    }

    #[test]
    fn a_conditional_key_is_a_warning_without_the_plan() {
        assert_eq!(
            verdicts("-c notify=[]", KEYED),
            [Verdict::Degrades("uze may set this")]
        );
        assert_eq!(
            verdicts("-c features={}", KEYED),
            [Verdict::Degrades("uze may set this")]
        );
    }

    #[test]
    fn a_conditional_key_is_refused_only_when_the_launch_sets_it() {
        assert_eq!(
            planned("-c notify=[]", &["-c", "notify=[\"uze\"]"]),
            [refused("uze composes notify on this launch")]
        );
        assert_eq!(
            planned("-c features.memories=true", &["-cfeatures.memories=false"]),
            [refused("uze composes features.memories on this launch")]
        );
        assert_eq!(
            planned("-c notify=[]", &["resume", "abc"]),
            [Verdict::Accepted]
        );
        assert_eq!(
            planned("-c features.web=true", &["-c", "features.memories=false"]),
            [Verdict::Accepted]
        );
    }

    #[test]
    fn a_flag_setting_a_key_by_name_is_judged_as_that_key() {
        assert_eq!(
            verdicts("--hook state", KEYED),
            [refused("uze composes hooks.state on every launch")]
        );
        assert_eq!(
            verdicts("--enable memories --enable web", KEYED),
            [Verdict::Degrades("uze may set this"), Verdict::Accepted]
        );
        assert_eq!(
            verdicts("--disable web", KEYED),
            [Verdict::Degrades("hooks")]
        );
        let composed = ["-c", "features.memories=false"];
        assert_eq!(
            planned(
                "--enable memories --disable=memories --enable web",
                &composed
            ),
            [
                refused("uze composes features.memories on this launch"),
                refused("uze composes features.memories on this launch"),
                Verdict::Accepted
            ]
        );
    }

    #[test]
    fn a_stray_word_is_refused_and_the_flag_before_it_kept() {
        assert_eq!(
            verdicts("--verbose fix-the-bug", CATALOG),
            [Verdict::Accepted, refused(A_PROMPT)]
        );
    }

    #[test]
    fn a_literal_end_of_options_is_refused() {
        assert_eq!(verdicts("--", CATALOG), [refused(ENDS_OPTIONS)]);
    }

    #[test]
    fn an_unknown_flag_passes_marked_and_takes_a_value_only_attached() {
        assert_eq!(verdicts("--brand-new-flag=x", CATALOG), [Verdict::Unknown]);
        assert_eq!(
            verdicts("--brand-new-flag x", CATALOG),
            [Verdict::Unknown, refused(A_PROMPT)]
        );
    }

    #[test]
    fn a_flag_that_weakens_safety_is_passed_and_marked() {
        assert_eq!(
            verdicts("--skip-permissions", CATALOG),
            [Verdict::Weakens("permission prompts")]
        );
    }

    #[test]
    fn a_value_that_weakens_safety_is_passed_and_marked() {
        assert_eq!(
            verdicts("--permission-mode bypass", CATALOG),
            [Verdict::Weakens("permission prompts")]
        );
        assert_eq!(
            verdicts("--permission-mode=plan", CATALOG),
            [Verdict::Accepted]
        );
    }

    #[test]
    fn a_flag_that_drops_what_uze_delivers_is_passed_and_marked() {
        assert_eq!(
            verdicts("--strict-mcp-config", CATALOG),
            [Verdict::Degrades("the MCP servers uze delivers")]
        );
    }

    #[test]
    fn a_value_is_checked_only_against_a_fixed_set() {
        assert_eq!(
            verdicts("--permission-mode plan", CATALOG),
            [Verdict::Accepted]
        );
        assert_eq!(
            verdicts("--permission-mode yolo", CATALOG),
            [refused("takes one of default, plan, bypass")]
        );
        assert_eq!(
            verdicts("--model", CATALOG),
            [refused("takes a value: model")]
        );
        assert_eq!(
            verdicts("--model anything-at-all", CATALOG),
            [Verdict::Accepted]
        );
    }
}
