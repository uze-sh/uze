//! Fixtures the integrations' own tests share.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use uze_core::capability::{Capability, CapabilityKind, Resource};
use uze_core::store::StoredPackage;

pub(crate) fn temp_root(label: &str) -> PathBuf {
    uze_testkit::temp::scratch(label)
}

/// The package's `commit` Skill.
pub(crate) fn skill_resource(pkg: &StoredPackage) -> Resource {
    let path = pkg.root.join("skills/commit/SKILL.md");
    Resource::from_package(
        pkg.id.clone(),
        pkg.root.clone(),
        Capability {
            kind: CapabilityKind::AgentSkill,
            path,
            payload: Vec::new(),
        },
    )
}

/// The package's MCP server `name`, declared in its `mcp.json`.
pub(crate) fn mcp_resource(pkg: &StoredPackage, name: &str) -> Resource {
    let path = pkg.root.join("mcp.json");
    Resource::from_package_named(
        pkg.id.clone(),
        pkg.root.clone(),
        Capability {
            kind: CapabilityKind::Mcp,
            path,
            payload: Vec::new(),
        },
        name.to_owned(),
    )
}

/// Every path under `root`, directories included.
pub(crate) fn walk(root: &Path) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path.clone());
            }
            out.insert(path);
        }
    }
    out
}

/// Every flag a `--help` text defines, in each spelling it shows: the
/// dash-led words opening an option's own line (`-c, --continue`,
/// `--session, -s string`), up to its value. A flag only mentioned in
/// another's description is not defined by it, which is why the
/// description's deeper indent is not read.
pub(crate) fn flags_in_help(help: &str) -> BTreeSet<&str> {
    const DEFINITION_INDENT: usize = 6;
    help.lines()
        .filter(|line| line.len() - line.trim_start().len() <= DEFINITION_INDENT)
        .flat_map(|line| {
            line.split_whitespace()
                .map(|word| word.trim_end_matches(','))
                .take_while(|word| is_flag(word))
        })
        .collect()
}

/// Every spelling of a flag whose definition shows its value in brackets
/// (`-d, --debug [filter]`): a value it may go without.
pub(crate) fn optional_values_in_help(help: &str) -> BTreeSet<&str> {
    const DEFINITION_INDENT: usize = 6;
    help.lines()
        .filter(|line| line.len() - line.trim_start().len() <= DEFINITION_INDENT)
        .flat_map(|line| {
            let flags: Vec<&str> = line
                .split_whitespace()
                .map(|word| word.trim_end_matches(','))
                .take_while(|word| is_flag(word))
                .collect();
            let optional = line
                .split_whitespace()
                .nth(flags.len())
                .is_some_and(|value| value.starts_with('['));
            if optional { flags } else { Vec::new() }
        })
        .collect()
}

fn is_flag(word: &str) -> bool {
    let name = word.trim_start_matches('-');
    let dashes = word.len() - name.len();
    (dashes == 1 || dashes == 2)
        && name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Fails naming every spelling `catalog` lists that `help` does not define,
/// every spelling listed twice, and a weakening value its flag does not
/// take: a catalog describes the release its snapshot was taken from, and
/// one spelling means one flag.
pub(crate) fn assert_catalog_in_help(catalog: &[uze_core::flag_catalog::FlagSpec], help: &str) {
    let defined = flags_in_help(help);
    let spellings: Vec<&str> = catalog
        .iter()
        .flat_map(|flag| flag.spellings.iter().copied())
        .collect();
    let missing: Vec<&str> = spellings
        .iter()
        .copied()
        .filter(|spelling| !defined.contains(spelling))
        .collect();
    assert!(missing.is_empty(), "not in the snapshot: {missing:?}");
    let unique: BTreeSet<&str> = spellings.iter().copied().collect();
    assert_eq!(unique.len(), spellings.len(), "a spelling is listed twice");
    let optional = optional_values_in_help(help);
    for flag in catalog {
        let shown_optional = flag
            .spellings
            .iter()
            .any(|spelling| optional.contains(spelling));
        assert_eq!(
            matches!(flag.value, uze_core::flag_catalog::FlagValue::Optional(_)),
            shown_optional,
            "{} is catalogued as {:?}, its help shows an optional value: {shown_optional}",
            flag.name(),
            flag.value
        );
        for (value, _) in flag.weakening_values {
            let taken = match (flag.value, flag.owner) {
                (_, uze_core::flag_catalog::FlagOwner::Keyed { .. }) => value.contains('='),
                (uze_core::flag_catalog::FlagValue::OneOf(values), _) => values.contains(value),
                _ => false,
            };
            assert!(
                taken,
                "{} weakens with {value}, which it does not take",
                flag.name()
            );
        }
    }
}

/// A flag a snapshot defines that its catalog leaves out, and why. Only two
/// kinds may be: anything else the interactive `--help` defines is
/// catalogued, a flag that collides with UZE included, as `Uze`.
pub(crate) enum LeftOut {
    /// Help, version, completions, a wizard: it answers and exits, so no
    /// agent is launched.
    EndsTheLaunch(&'static [&'static str]),
    /// It acts only on non-interactive output, which a workspace agent never
    /// is; the reason quotes the help's wording.
    PrintOnly(&'static [&'static str], &'static str),
}

impl LeftOut {
    fn spellings(&self) -> &'static [&'static str] {
        match self {
            Self::EndsTheLaunch(spellings) | Self::PrintOnly(spellings, _) => spellings,
        }
    }
}

/// Fails unless every flag `help` defines is either in `catalog` or
/// `left_out`, and nothing in either is a flag `help` does not define: a
/// release's interactive flags are catalogued whole, so a flag that appears
/// in a refreshed snapshot fails here until it is classified.
pub(crate) fn assert_catalog_covers_help(
    catalog: &[uze_core::flag_catalog::FlagSpec],
    left_out: &[LeftOut],
    help: &str,
) {
    assert_catalog_in_help(catalog, help);
    for flag in left_out {
        if let LeftOut::PrintOnly(spellings, why) = flag {
            assert!(
                !why.is_empty(),
                "{spellings:?} is left out without a reason"
            );
        }
    }
    let defined = flags_in_help(help);
    let catalogued: BTreeSet<&str> = catalog
        .iter()
        .flat_map(|flag| flag.spellings.iter().copied())
        .collect();
    let excluded: BTreeSet<&str> = left_out
        .iter()
        .flat_map(|flag| flag.spellings().iter().copied())
        .collect();
    let both: Vec<&str> = catalogued.intersection(&excluded).copied().collect();
    assert!(both.is_empty(), "catalogued and left out: {both:?}");
    let stale: Vec<&str> = excluded.difference(&defined).copied().collect();
    assert!(
        stale.is_empty(),
        "left out but not in the snapshot: {stale:?}"
    );
    let unclassified: Vec<&str> = defined
        .difference(&catalogued)
        .copied()
        .filter(|flag| !excluded.contains(flag))
        .collect();
    assert!(
        unclassified.is_empty(),
        "in the snapshot but neither catalogued nor left out: {unclassified:?}"
    );
}

#[test]
fn a_catalog_covers_its_help_only_when_every_flag_is_classified() {
    use uze_core::flag_catalog::{FlagOwner, FlagSpec, FlagValue};
    const HELP: &str =
        "Options:\n  -h, --help    help\n  --model <m>   model\n  --json        print only\n";
    const CATALOG: &[FlagSpec] = &[FlagSpec::new(
        &["--model"],
        FlagValue::Free("model"),
        "model",
        FlagOwner::Operator,
    )];
    let covered = [
        LeftOut::EndsTheLaunch(&["--help", "-h"]),
        LeftOut::PrintOnly(&["--json"], "print only"),
    ];

    assert_catalog_covers_help(CATALOG, &covered, HELP);
    let unclassified = std::panic::catch_unwind(|| {
        assert_catalog_covers_help(CATALOG, &[LeftOut::EndsTheLaunch(&["--help", "-h"])], HELP);
    });
    assert!(
        unclassified.is_err(),
        "--json is neither catalogued nor left out"
    );
}

#[test]
fn a_flag_named_only_in_a_description_is_not_defined() {
    let help = "Usage: x [--last]\n\nOptions:\n  -c, --continue <id>    resume; see --last\n      --enable <F>\n          like --other\n";

    assert_eq!(
        flags_in_help(help),
        BTreeSet::from(["-c", "--continue", "--enable"])
    );
}

/// The fixture both readers of a `--help` are held to: this suite's
/// `flags_in_help` and the Lab's `defined_flags`
/// (`conformance/tests/test_flags.py`), so the rule is written twice but
/// cannot read two ways.
#[test]
fn the_shared_help_fixture_defines_exactly_its_expected_flags() {
    let expected: BTreeSet<&str> = include_str!("help_fixture/flags.txt")
        .lines()
        .filter(|line| !line.is_empty())
        .collect();

    assert_eq!(
        flags_in_help(include_str!("help_fixture/help.txt")),
        expected
    );
}

#[test]
fn a_value_in_brackets_is_one_a_flag_may_go_without() {
    let help = "Options:\n  -d, --debug [filter]   debug\n  --model <m>   model [default]\n  --bare   bare\n";

    assert_eq!(
        optional_values_in_help(help),
        BTreeSet::from(["-d", "--debug"])
    );
}
