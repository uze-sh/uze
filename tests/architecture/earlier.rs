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

/// Where support for an earlier release may be written: an `earlier`
/// module, the owned map's retired entries, and `uze-document` itself.
fn is_home(relative: &str) -> bool {
    relative.ends_with("/earlier.rs")
        || relative.ends_with("machine/home/owned.rs")
        || relative.starts_with("crates/uze-document/")
}

/// Every production source under `crates/` and `src/`, relative to the
/// repository root, test modules stripped.
fn workspace_sources() -> Vec<(String, String)> {
    let root = repository_root();
    ["crates", "src"]
        .iter()
        .flat_map(|scope| production_sources(&root.join(scope)))
        .filter_map(|(path, source)| {
            let relative = path
                .strip_prefix(&root)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            (!relative.starts_with("crates/uze-testkit/")).then_some((relative, source))
        })
        .collect()
}

/// The identifiers that mark support for an earlier release, in code.
const MARKS: &[&str] = &["legacy_", "retired_", "earlier_"];

/// Files that spell a mark for a reason that is not UZE's own past.
const MARKS_SANCTIONED: &[(&str, &str)] = &[(
    "src/ui/orchestrator/input.rs",
    "the X10 mouse encoding is the terminal's legacy, not an earlier UZE release's",
)];

/// Today's inline support, frozen: exact, so moving a site home fails
/// until its number is lowered, and a new one fails at once.
const MARKS_BUDGET: &[(&str, usize)] = &[
    ("crates/uze-application/src/application.rs", 1),
    ("crates/uze-application/src/application/catalogue.rs", 1),
    ("crates/uze-application/src/application/context.rs", 15),
    (
        "crates/uze-application/src/application/project_environment.rs",
        4,
    ),
    ("crates/uze-application/src/application/read_models.rs", 4),
    (
        "crates/uze-application/src/application/retired_regions.rs",
        6,
    ),
    ("crates/uze-application/src/application/schema.rs", 2),
    ("crates/uze-application/src/application/services.rs", 1),
    (
        "crates/uze-application/src/application/services/retired_regions.rs",
        5,
    ),
    ("crates/uze-application/src/application/setup.rs", 1),
    ("crates/uze-core/src/lib.rs", 1),
    ("crates/uze-core/src/machine/home.rs", 1),
    ("crates/uze-core/src/project.rs", 1),
    ("crates/uze-workspace/src/checkout/git_facts.rs", 2),
    ("crates/uze-workspace/src/lib.rs", 1),
    ("src/cli/status.rs", 6),
    ("src/ui/orchestrator.rs", 2),
    ("src/ui/orchestrator/reads.rs", 2),
    ("src/ui/orchestrator/session/absorb.rs", 4),
    ("src/ui/worker.rs", 1),
];

fn marks_in(source: &str) -> usize {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .map(|line| {
            MARKS
                .iter()
                .map(|mark| {
                    line.match_indices(mark)
                        .filter(|(at, _)| {
                            !line[..*at]
                                .chars()
                                .next_back()
                                .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
                        })
                        .count()
                })
                .sum::<usize>()
        })
        .sum()
}

#[test]
fn support_for_an_earlier_release_is_written_in_its_home() {
    let mut failures = Vec::new();
    for (file, source) in workspace_sources() {
        if is_home(&file)
            || MARKS_SANCTIONED
                .iter()
                .any(|(sanctioned, _)| *sanctioned == file)
        {
            continue;
        }
        let found = marks_in(&source);
        let allowed = MARKS_BUDGET
            .iter()
            .find(|(budgeted, _)| *budgeted == file)
            .map_or(0, |(_, count)| *count);
        if found != allowed {
            failures.push(format!("  {file}: {found} (budget {allowed})"));
        }
    }
    assert!(
        failures.is_empty(),
        "\n\nCode that copes with what an earlier release left is written in the \
         `earlier` module of the crate that owns it, or as an entry of its registry \
         (a ladder rung, a retired owned path), never inline: an inline fallback is \
         one nobody can find when the floor passes it. Move it home and lower its \
         budget; a budget is exact, so a file that went down must say so \
         (see {RULE}):\n\n{}\n",
        failures.join("\n")
    );
}

#[test]
fn no_record_reads_an_old_field_through_a_serde_alias() {
    let aliased: Vec<String> = workspace_sources()
        .into_iter()
        .filter(|(_, source)| source.contains("serde(alias"))
        .map(|(file, _)| format!("  {file}"))
        .collect();
    assert!(
        aliased.is_empty(),
        "\n\nA field an earlier shape spelled differently is read by a rung of the \
         record's ladder, not by a serde alias that nothing ever deletes \
         (see {RULE}):\n\n{}\n",
        aliased.join("\n")
    );
}

/// A record's field may default only when it is additive: a field an older
/// build sharing the machine does not write, said so in its doc. Today's
/// unmarked defaults, frozen.
const DEFAULTS_BUDGET: &[(&str, usize)] = &[
    ("crates/uze-core/src/delivery/state.rs", 4),
    ("crates/uze-core/src/machine/usage.rs", 3),
    ("crates/uze-core/src/package/hosts.rs", 2),
    ("crates/uze-core/src/package/store.rs", 2),
    ("crates/uze-terminal/src/runtime/persist.rs", 1),
    ("crates/uze-workspace/src/checkout/record.rs", 2),
    ("crates/uze-workspace/src/client_layout.rs", 5),
    ("crates/uze-workspace/src/conversation.rs", 4),
    ("crates/uze-workspace/src/extensions/consent.rs", 7),
    ("crates/uze-workspace/src/overrides/record.rs", 3),
    ("crates/uze-workspace/src/review.rs", 10),
];

fn unmarked_defaults(source: &str) -> usize {
    let lines: Vec<&str> = source.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains("#[serde(default"))
        .filter(|(index, _)| {
            !lines[..*index]
                .iter()
                .rev()
                .take_while(|line| {
                    let line = line.trim_start();
                    line.starts_with("///") || line.starts_with("#[")
                })
                .any(|line| line.contains("Additive") || line.contains("additive"))
        })
        .count()
}

#[test]
fn a_record_field_defaults_only_when_it_is_additive() {
    let mut failures = Vec::new();
    for (file, source) in workspace_sources() {
        if !source.contains("Shaped for") {
            continue;
        }
        let found = unmarked_defaults(&source);
        let allowed = DEFAULTS_BUDGET
            .iter()
            .find(|(budgeted, _)| *budgeted == file)
            .map_or(0, |(_, count)| *count);
        if found != allowed {
            failures.push(format!("  {file}: {found} (budget {allowed})"));
        }
    }
    assert!(
        failures.is_empty(),
        "\n\nA `#[serde(default)]` on a record's field either marks an additive field \
         — one an older build sharing the machine does not write, said so with \
         `Additive` in its doc — or reads what an earlier shape left out, which is a \
         rung of the record's ladder. Budgets are exact (see {RULE}):\n\n{}\n",
        failures.join("\n")
    );
}

#[test]
fn a_retired_standing_is_read_only_from_the_map() {
    const READERS: &[&str] = &["crates/uze-application/src/application/schema.rs"];
    let elsewhere: Vec<String> = workspace_sources()
        .into_iter()
        .filter(|(file, source)| {
            source.contains("Standing::Retired")
                && !is_home(file)
                && !READERS.contains(&file.as_str())
        })
        .map(|(file, _)| format!("  {file}"))
        .collect();
    assert!(
        elsewhere.is_empty(),
        "\n\nWhether a path is retired is the owned map's to say; read it through \
         `UzeHome::owned_paths()` (see {RULE}):\n\n{}\n",
        elsewhere.join("\n")
    );
}

#[test]
fn a_retired_name_is_spelled_only_by_its_entry() {
    let entries = every_entry();
    let sources = workspace_sources();
    let mut failures = Vec::new();
    for entry in &entries {
        for spelled in entry.spelled {
            let quoted = format!("\"{spelled}\"");
            for (file, source) in &sources {
                if !entry.site.replace('\\', "/").ends_with(file.as_str())
                    && !file.ends_with(entry.site)
                    && source.contains(&quoted)
                {
                    failures.push(format!(
                        "  {quoted} in {file}, declared by {} ({})",
                        entry.name, entry.site
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "\n\nA name only an earlier release wrote is spelled by the entry that \
         handles it and read from there, so retiring the entry retires every use \
         (see {RULE}):\n\n{}\n",
        failures.join("\n")
    );
}
