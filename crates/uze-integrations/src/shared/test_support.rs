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
/// and every spelling listed twice: a catalog describes the release its
/// snapshot was taken from, and one spelling means one flag.
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
}

#[test]
fn a_flag_named_only_in_a_description_is_not_defined() {
    let help = "Usage: x [--last]\n\nOptions:\n  -c, --continue <id>    resume; see --last\n      --enable <F>\n          like --other\n";

    assert_eq!(
        flags_in_help(help),
        BTreeSet::from(["-c", "--continue", "--enable"])
    );
}
