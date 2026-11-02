//! What the project concern does only because an earlier release left it
//! (the `earlier-releases` rule): the move that gathered a project's
//! records into one directory, declared as `scattered_project_records` in
//! the owned map.

use std::fs;

use crate::home::UzeHome;

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
