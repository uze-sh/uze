//! Support for what an earlier release left is declared, bounded by the
//! release floor, and retired by deleting what the floor passes (the
//! `earlier-releases` rule).

use uze_core::document::earlier::{Earlier, FLOOR, Release};

use crate::layering::{production_sources, repository_root};

const RULE: &str = ".agents/rules/earlier-releases.md";

/// Every entry the workspace declares.
fn every_entry() -> Vec<Earlier> {
    let mut entries = uze_core::earlier::earlier();
    entries.extend(uze_workspace::earlier::earlier());
    entries.extend(uze_terminal::earlier::earlier());
    entries
}

#[test]
fn no_entry_outlives_the_release_floor() {
    let due: Vec<String> = every_entry()
        .into_iter()
        .filter(|entry| entry.lasts.below(FLOOR))
        .map(|entry| format!("  {} ({}): delete this entry", entry.name, entry.site))
        .collect();
    assert!(
        due.is_empty(),
        "\n\nThe release floor is {FLOOR}, and these entries handle only what an \
         older release left. Retiring that support is deleting them and their \
         tests (see {RULE}):\n\n{}\n",
        due.join("\n")
    );
}

#[test]
fn the_floor_is_a_release_at_or_below_this_one() {
    let this = Release::parse(env!("CARGO_PKG_VERSION")).expect("the package version is a release");
    assert!(
        FLOOR <= this,
        "the release floor {FLOOR} is above this build's {this}: a machine cannot \
         come from a release that does not exist yet (see {RULE})"
    );
}

/// A record's ladder reaches the suite only through the `rungs()` of the
/// module that keeps it, which its crate's `earlier()` gathers; a ladder
/// left out is one the floor never sees.
#[test]
fn every_record_lists_its_rungs() {
    let mut missing = Vec::new();
    for (path, source) in production_sources(&repository_root().join("crates")) {
        for record in shaped_records(&source) {
            if !source.contains(&format!("rungs::<{record}>")) {
                let relative = path.strip_prefix(repository_root()).unwrap_or(&path);
                missing.push(format!("  {} ({record})", relative.display()));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "\n\nThese records are `Shaped` but their module lists no rungs for them. \
         Add `uze_document::earlier::rungs::<Record>(file!())` to the module's \
         `rungs()`, gathered by its crate's `earlier()` (see {RULE}):\n\n{}\n",
        missing.join("\n")
    );
}

/// The records `source` implements `Shaped` for, by name; a record named
/// through a macro's `$record` is the macro's to list.
fn shaped_records(source: &str) -> Vec<String> {
    source
        .match_indices("Shaped for ")
        .filter_map(|(at, needle)| {
            let rest = &source[at + needle.len()..];
            let name: String = rest
                .chars()
                .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
        .collect()
}
