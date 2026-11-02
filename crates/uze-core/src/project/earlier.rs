//! What the project concern does only because an earlier release left it
//! (the `earlier-releases` rule): the move that gathered a project's
//! records into one directory, declared as `scattered_project_records` in
//! the owned map; and the regions earlier releases wrote into a project's
//! `AGENTS.md`, named so what they left behind can be recognised and taken
//! back out.
//!
//! Nothing is written under those regions any more. The workspace's
//! instructions reach an agent through its launch context, the plugin
//! authoring verbs through the `uze:author` Skill, and what a plugin tells
//! every agent of a declaring project through an `always_on` rule: UZE
//! writes nothing into a file the operator authors.

use std::fs;

use uze_document::earlier::{Earlier, Lasts};

use crate::home::UzeHome;

/// This concern's entries: the regions earlier releases wrote into a
/// project's files. Each is taken out by the next install of that project
/// (`regions::remove`), only as often as that runs: unbounded.
pub(crate) fn earlier() -> Vec<Earlier> {
    vec![Earlier {
        name: "regions earlier releases wrote into AGENTS.md and a harness's instructions file",
        lasts: Lasts::Unbounded,
        spelled: &[WORKTREE_POLICY, PLUGIN_AUTHORING, INSTRUCTION_BRIDGE],
        site: file!(),
    }]
}

/// The rung for the release that gathered a project's records into one
/// directory.
///
/// Moving a path *is* a version change, and a version change with no rung
/// is how state gets lost — which is the whole reason this module exists.
/// A machine coming from the previous release has its agents at
/// `state/tasks/<id>.json`, its conversations at
/// `state/conversations/<id>/` and its prompt history at
/// `state/prompt-history/<id>.json`; leaving them there would have this
/// build open on a project with no agents and every checkout orphaned.
///
/// Best-effort and silent, like every other climb: what it carries is
/// UZE's own bookkeeping, and a rename that fails leaves the bytes exactly
/// where they were for the operator to find. Runs once — only when the
/// project directory is being created — and is removable once no machine
/// can still be on the previous layout.
pub(crate) fn carry_across_the_move(home: &UzeHome, id: &str) {
    let scattered = home.scattered_project_records(id);
    let carried = [
        (scattered.agents, home.tasks_path(id)),
        (scattered.conversations, home.conversations_dir(id)),
        (scattered.prompt_history, home.prompt_history_path(id)),
    ];
    for (was, now) in carried {
        if was.exists() && !now.exists() {
            let _ = fs::rename(&was, &now);
        }
    }
    // The lock beside the old store is not carried: it guards a path that
    // no longer exists, and the new store mints its own. Left behind it is
    // a file nothing will ever open again.
    let _ = fs::remove_file(scattered.agents_lock);
    // And the directories the three used to live in, when nothing else is
    // in them. `remove_dir` refuses a directory with anything left, so a
    // project still on the old layout keeps its own.
    for emptied in scattered.directories {
        let _ = fs::remove_dir(emptied);
    }
}

/// The workspace's isolation policy, projected until the launch carried it.
pub const WORKTREE_POLICY: &str = "project:worktree-policy";

/// The plugin authoring verbs, projected until the `uze:author` Skill
/// documented them for every agent.
pub const PLUGIN_AUTHORING: &str = "project:plugin-authoring";

const RETIRED: [&str; 2] = [WORKTREE_POLICY, PLUGIN_AUTHORING];

/// Whether `identity` is a region no release writes any more, written with
/// a digest of its bytes. Each one ended in that digest, so what it held
/// can be told from an edit made since.
pub fn is_retired(identity: &str) -> bool {
    RETIRED.iter().any(|prefix| {
        identity
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// Whether `identity` is a package's instructions region,
/// `package:<plugin>:<market>:instructions`, composed from a plugin's own
/// `AGENTS.md` until plugins spoke through `always_on` rules. Its identity
/// carries no digest and no renderer is left to say what it held, so it is
/// recognised by shape alone and removed structurally.
pub fn is_retired_package(identity: &str) -> bool {
    identity
        .strip_prefix("package:")
        .and_then(|rest| rest.strip_suffix(":instructions"))
        .is_some_and(|middle| {
            middle
                .split_once(':')
                .is_some_and(|(plugin, market)| !plugin.is_empty() && !market.is_empty())
        })
}

/// The region an earlier release wrote into a harness's own instructions
/// file, `CLAUDE.md`, to import `AGENTS.md` while a package contributed to
/// it. It goes with the last package region.
pub const INSTRUCTION_BRIDGE: &str = "instruction-bridge";

/// What [`INSTRUCTION_BRIDGE`] held: the import, and nothing else.
pub const INSTRUCTION_BRIDGE_CONTENT: &str = "@AGENTS.md";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_retired_prefixes_are_retired() {
        assert!(is_retired("project:worktree-policy/0123abcd"));
        assert!(is_retired("project:plugin-authoring/0123abcd"));
        assert!(!is_retired("project:worktree-policy"));
        assert!(!is_retired("package:uze:official:instructions"));
        assert!(!is_retired("instruction-bridge"));
    }

    #[test]
    fn a_package_region_is_recognised_by_its_shape() {
        assert!(is_retired_package("package:greet:tools:instructions"));
        assert!(!is_retired_package("package:greet:instructions"));
        assert!(!is_retired_package("package::tools:instructions"));
        assert!(!is_retired_package("package:greet:tools:rules"));
        assert!(!is_retired_package("project:worktree-policy/0123abcd"));
        assert!(!is_retired_package(INSTRUCTION_BRIDGE));
    }
}
