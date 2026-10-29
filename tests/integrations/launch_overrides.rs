//! What an agent's overrides make of each harness's launch.
//!
//! Asked of every registered harness with its own catalog and its own
//! continuity verbs: the resume it spells, the first turn it takes, and the
//! arguments that switch its memory off. Read off the launch the plan
//! composes, the same one the shim hands the real binary.

use std::ffi::OsString;

use uze_core::{
    UzeHome,
    flag_catalog::{FlagOwner, FlagSpec, FlagValue},
    harness_runtime::HarnessRuntimeContribution,
    integration::{IntegrationPort, SessionContinuity},
    session::SessionId,
};
use uze_integrations::registry::IntegrationRegistry;
use uze_testkit::temp::scratch;
use uze_workspace::{continuity::LaunchPlan, overrides};

const CONTRIBUTION: &str = "--from-the-contribution";
const VARIABLE: (&str, &str) = ("OVERRIDE_PROBE", "1");

fn registry(label: &str) -> IntegrationRegistry {
    let root = scratch(label);
    let home = UzeHome::at(root.join("uze"));
    IntegrationRegistry::isolated(&root, &home)
}

fn os(words: &[&str]) -> Vec<OsString> {
    words.iter().map(OsString::from).collect()
}

/// A flag of the operator's, with a value when it takes one, as an
/// override spells it.
fn an_operator_flag(integration: &dyn IntegrationPort) -> Vec<String> {
    let spec = integration
        .flag_catalog()
        .iter()
        .find(|spec| spec.owner == FlagOwner::Operator)
        .unwrap_or_else(|| panic!("{} offers a flag of the operator's", integration.id()));
    let mut words = vec![spec.name().to_owned()];
    match spec.value {
        FlagValue::None => {}
        FlagValue::Free(_) => words.push("probe-value".to_owned()),
        FlagValue::OneOf(values) => words.push(values[0].to_owned()),
    }
    words
}

/// The plan an owned launch of `integration` makes, carrying `words` as the
/// agent's overrides, checked against what the plan composes.
fn plan(
    integration: &dyn IntegrationPort,
    resume: Vec<OsString>,
    trailing: Vec<OsString>,
    words: &[String],
) -> LaunchPlan {
    let composed: Vec<OsString> = resume.iter().chain(&trailing).cloned().collect();
    let overrides = overrides::compose(
        words,
        integration.flag_catalog(),
        integration.managed_environment(),
        &composed,
    );
    LaunchPlan {
        // What the plan left out is on its note, as `plan_with` puts it.
        note: (!overrides.left_out.is_empty()).then(|| overrides.left_out.join("; ")),
        overrides,
        args: resume,
        trailing,
        owned: true,
        ..LaunchPlan::default()
    }
}

fn contribution() -> HarnessRuntimeContribution {
    HarnessRuntimeContribution {
        extra_args: os(&[CONTRIBUTION]),
        ..HarnessRuntimeContribution::passthrough()
    }
}

fn with_variable(mut words: Vec<String>) -> Vec<String> {
    words.push(format!("{}={}", VARIABLE.0, VARIABLE.1));
    words
}

#[test]
fn a_resumed_launch_takes_its_overrides_after_the_resume_on_every_harness() {
    for integration in registry("overrides-resume").iter() {
        if integration.session_continuity() == SessionContinuity::Unsupported {
            continue;
        }
        let flag = an_operator_flag(integration);
        let resume = integration.resume_session_args(&SessionId::new("recorded"));
        assert!(!resume.is_empty(), "{} spells a resume", integration.id());
        let session = plan(
            integration,
            resume.clone(),
            Vec::new(),
            &with_variable(flag.clone()),
        );
        let mut launch = contribution();

        let note = session.compose(&mut launch, None, integration.flag_catalog());

        let expected: Vec<OsString> = resume
            .into_iter()
            .chain(os(&[CONTRIBUTION]))
            .chain(flag.iter().map(OsString::from))
            .collect();
        assert_eq!(launch.extra_args, expected, "{}", integration.id());
        assert_eq!(
            launch.extra_env.first(),
            Some(&(VARIABLE.0.into(), VARIABLE.1.into())),
            "{}: under everything UZE sets",
            integration.id()
        );
        assert_eq!(note, None, "{}", integration.id());
    }
}

/// Where a harness reads its first turn decides where overrides go: before
/// a turn its own flag carries (`-- <turn>` ends option parsing), after a
/// bare positional one, which a variadic flag before it would swallow.
#[test]
fn a_first_turn_keeps_its_place_beside_the_overrides_on_every_harness() {
    let mut asked = 0;
    for integration in registry("overrides-first-turn").iter() {
        let flag = an_operator_flag(integration);
        let turn = integration.first_turn_args("read your briefing");
        let Some(first) = turn.first() else {
            continue;
        };
        let session = plan(integration, Vec::new(), turn.clone(), &flag);
        let mut launch = contribution();

        session.compose(&mut launch, None, integration.flag_catalog());

        let flag: Vec<OsString> = flag.iter().map(OsString::from).collect();
        let expected: Vec<OsString> = if first.to_string_lossy().starts_with('-') {
            os(&[CONTRIBUTION])
                .into_iter()
                .chain(flag)
                .chain(turn)
                .collect()
        } else {
            os(&[CONTRIBUTION])
                .into_iter()
                .chain(turn)
                .chain(flag)
                .collect()
        };
        assert_eq!(launch.extra_args, expected, "{}", integration.id());
        asked += 1;
    }
    assert!(asked > 0, "some harness takes its first turn as arguments");
}

#[test]
fn a_bare_first_turn_is_never_taken_as_a_variadic_flags_value() {
    let registry = registry("overrides-variadic");
    let codex = registry
        .iter()
        .find(|integration| integration.id() == "codex")
        .expect("Codex is registered");
    let turn = codex.first_turn_args("read your briefing");
    let words: Vec<String> = ["-i", "shot.png"].map(str::to_owned).to_vec();
    let session = plan(codex, Vec::new(), turn.clone(), &words);
    let mut launch = contribution();
    let off = codex.memory_off().expect("Codex turns its memory off");

    session.compose(&mut launch, Some(off.clone()), codex.flag_catalog());

    let expected: Vec<OsString> = os(&[CONTRIBUTION])
        .into_iter()
        .chain(turn)
        .chain(os(&["-i", "shot.png"]))
        .chain(off.args)
        .collect();
    assert_eq!(launch.extra_args, expected);
}

#[test]
fn memory_off_comes_last_and_leaves_out_the_override_it_collides_with() {
    let mut codex = false;
    for integration in registry("overrides-memory-off").iter() {
        let Some(off) = integration.memory_off() else {
            continue;
        };
        let flag = an_operator_flag(integration);
        let resume = integration.resume_session_args(&SessionId::new("recorded"));
        let mut words = flag.clone();
        if integration.id() == "codex" {
            words.extend(["-c".to_owned(), "features.memories=true".to_owned()]);
        }
        let session = plan(integration, resume.clone(), Vec::new(), &words);
        let mut launch = contribution();

        let note = session.compose(&mut launch, Some(off.clone()), integration.flag_catalog());

        let expected: Vec<OsString> = resume
            .into_iter()
            .chain(os(&[CONTRIBUTION]))
            .chain(flag.iter().map(OsString::from))
            .chain(off.args)
            .collect();
        assert_eq!(launch.extra_args, expected, "{}", integration.id());
        if integration.id() == "codex" {
            let note = note.expect("what memory-off left out is said");
            assert!(note.starts_with("-c left out ("), "{note}");
            assert!(!note.contains("features.memories=true"), "{note}");
            codex = true;
        }
    }
    assert!(codex, "Codex turns its memory off per launch");
}

#[test]
fn an_override_of_what_uze_composes_never_reaches_any_harness() {
    for integration in registry("overrides-owned").iter() {
        let owned: Vec<&FlagSpec> = integration
            .flag_catalog()
            .iter()
            .filter(|spec| matches!(spec.owner, FlagOwner::Uze(_)))
            .collect();
        assert!(
            !owned.is_empty(),
            "{} owns some of its flags",
            integration.id()
        );
        for spec in owned {
            for spelling in spec.spellings {
                let mut words = vec![(*spelling).to_owned()];
                if spec.value != FlagValue::None {
                    words.push("elsewhere".to_owned());
                }
                let session = plan(integration, Vec::new(), Vec::new(), &words);
                let mut launch = contribution();

                let note = session.compose(&mut launch, None, integration.flag_catalog());

                assert_eq!(
                    launch.extra_args,
                    os(&[CONTRIBUTION]),
                    "{} {spelling}",
                    integration.id()
                );
                let note = note.unwrap_or_default();
                assert!(
                    note.starts_with(&format!("{spelling} left out (")),
                    "{} {spelling}: {note}",
                    integration.id()
                );
            }
        }
    }
}

/// Every spelling a harness would read as one of its owned flags: a short
/// one inside a cluster of switches (`-zc` as `-z -c`), and a long one
/// written with one dash (`-continue`, `-resume=id`).
fn owned_spellings_in_disguise(spec: &FlagSpec) -> Vec<String> {
    let value = if spec.value == FlagValue::None {
        ""
    } else {
        "=elsewhere"
    };
    spec.spellings
        .iter()
        .filter_map(|spelling| spelling.strip_prefix('-'))
        .map(|name| match name.strip_prefix('-') {
            Some(long) => format!("-{long}{value}"),
            None => format!("-z{name}"),
        })
        .collect()
}

#[test]
fn an_owned_flag_in_disguise_never_reaches_any_harness() {
    let mut asked = 0;
    for integration in registry("overrides-disguised").iter() {
        let catalog = integration.flag_catalog();
        assert!(
            !catalog.iter().any(|spec| spec.spellings.contains(&"-z")),
            "-z stays a letter no harness knows"
        );
        for spec in catalog
            .iter()
            .filter(|spec| matches!(spec.owner, FlagOwner::Uze(_)))
        {
            for word in owned_spellings_in_disguise(spec) {
                let session = plan(
                    integration,
                    Vec::new(),
                    Vec::new(),
                    std::slice::from_ref(&word),
                );
                let mut launch = contribution();

                let note = session.compose(&mut launch, None, catalog);

                assert_eq!(
                    launch.extra_args,
                    os(&[CONTRIBUTION]),
                    "{} {word}",
                    integration.id()
                );
                let label = word.split('=').next().unwrap_or_default();
                assert!(
                    note.is_some_and(|note| note.starts_with(&format!("{label} left out ("))),
                    "{} {word}",
                    integration.id()
                );
                asked += 1;
            }
        }
    }
    assert!(asked > 0, "some harness owns a flag");
}

#[test]
fn a_feature_switched_on_by_name_is_checked_as_the_key_it_sets() {
    let registry = registry("overrides-feature");
    let codex = registry
        .iter()
        .find(|integration| integration.id() == "codex")
        .expect("Codex is registered");
    let catalog = codex.flag_catalog();
    let words: Vec<String> = ["--enable", "memories", "--disable", "memories"]
        .map(str::to_owned)
        .to_vec();
    let entries = overrides::entries(&words, catalog);

    let unplanned = overrides::check(&entries, catalog, codex.managed_environment(), None);
    assert!(
        unplanned
            .iter()
            .all(|verdict| matches!(verdict, overrides::Verdict::Degrades(_))),
        "{unplanned:?}"
    );

    let off = codex.memory_off().expect("Codex turns its memory off");
    let session = plan(codex, Vec::new(), Vec::new(), &words);
    let mut launch = contribution();
    let note = session
        .compose(&mut launch, Some(off.clone()), catalog)
        .expect("what memory-off left out is said");

    let expected: Vec<OsString> = os(&[CONTRIBUTION]).into_iter().chain(off.args).collect();
    assert_eq!(launch.extra_args, expected);
    assert!(note.starts_with("--enable left out ("), "{note}");
    assert!(note.contains("--disable left out ("), "{note}");
}

/// A value that gives up as much as a `--dangerously-…` switch is said
/// as the switch is.
#[test]
fn a_value_that_gives_up_safety_is_said_as_its_switch_is() {
    let registry = registry("overrides-weakening-values");
    for (harness, line) in [
        ("claude-code", "--permission-mode bypassPermissions"),
        ("codex", "--sandbox danger-full-access"),
        ("codex", "-s danger-full-access"),
        ("codex", "--ask-for-approval never"),
        ("codex", "-a never"),
    ] {
        let integration = registry
            .iter()
            .find(|integration| integration.id() == harness)
            .unwrap_or_else(|| panic!("{harness} is registered"));
        let words = overrides::split(line).unwrap();

        let composed = overrides::compose(
            &words,
            integration.flag_catalog(),
            integration.managed_environment(),
            &[],
        );

        assert_eq!(
            composed.args,
            os(&words.iter().map(String::as_str).collect::<Vec<_>>())
        );
        assert_eq!(composed.weakens.len(), 1, "{harness} {line}");
        assert!(
            composed.weakens[0].starts_with(&format!("{} gives up ", words[0])),
            "{harness} {line}: {:?}",
            composed.weakens
        );
    }
}
