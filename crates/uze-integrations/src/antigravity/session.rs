//! Conversation continuity for Antigravity CLI.
//!
//! It names its own conversation and continues one with
//! `--conversation <id>`. The identifier is read back from the index the
//! harness maintains for its own `--continue`: a plain
//! `cache/last_conversations.json` mapping a working directory to the last
//! conversation held there. The conversations themselves are one database
//! per conversation, and UZE opens none of them — the index is the whole
//! read.
//!
//! That index keeps one entry per directory and timestamps nothing, so the
//! launch time cannot be the guard here. What is recorded instead is the
//! value the index already held for this checkout when the launch started:
//! reading that same value back means nothing new was started, and the
//! conversation still named there belongs to whoever ran before.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use uze_core::{integration::ObservationContext, session::SessionId};

pub(super) const RESUME_FLAG: &str = "--conversation";
pub(super) const FIRST_TURN_FLAG: &str = "-i";

/// The operator's first turn. Behind `-i` (`--prompt-interactive`): the
/// plain prompt flag runs once and exits, and an agent is a session.
pub(super) fn first_turn_args(turn: &str) -> Vec<OsString> {
    vec![OsString::from(FIRST_TURN_FLAG), OsString::from(turn)]
}

pub(super) fn resume_args(session: &SessionId) -> Vec<OsString> {
    vec![
        OsString::from(RESUME_FLAG),
        OsString::from(session.as_str()),
    ]
}

pub(super) fn index_path(cli_root: &Path) -> PathBuf {
    cli_root.join("cache").join("last_conversations.json")
}

/// What the index already names for `cwd`.
pub(super) fn recorded_for(cli_root: &Path, cwd: &Path) -> Option<SessionId> {
    index(cli_root)?
        .get(cwd.to_string_lossy().as_ref())
        // Antigravity names every conversation with a UUID, and the index
        // is a plain file anything can write.
        .and_then(|id| SessionId::parse(id.clone()).ok())
        .filter(SessionId::is_uuid)
}

/// The conversation the index names for `ctx.cwd`, once it is one this
/// launch could have started.
pub(super) fn observe(cli_root: &Path, ctx: &ObservationContext) -> Option<SessionId> {
    let current = recorded_for(cli_root, ctx.cwd)?;
    // Unchanged since the launch: the harness has not written a
    // conversation of its own yet, and what is there belongs to the
    // previous occupant of this checkout.
    if ctx.preceded_by == Some(&current) {
        return None;
    }
    // One entry per directory: another agent's conversation here is the
    // only answer this harness has, and it is not this agent's.
    ctx.is_unclaimed(&current).then_some(current)
}

/// Whether a conversation's own store is still there. The store is named by
/// the identifier, so this never opens one.
pub(super) fn exists(cli_root: &Path, session: &SessionId) -> bool {
    cli_root
        .join("conversations")
        .join(format!("{}.db", session.as_str()))
        .is_file()
}

fn index(cli_root: &Path) -> Option<BTreeMap<String, String>> {
    let bytes = fs::read(index_path(cli_root)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli_root(label: &str) -> PathBuf {
        uze_testkit::temp::scratch(label).join("antigravity-cli")
    }

    fn write_index(root: &Path, entries: &[(&str, &str)]) {
        let map: BTreeMap<&str, &str> = entries.iter().copied().collect();
        let path = index_path(root);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, serde_json::to_vec(&map).unwrap()).unwrap();
    }

    #[test]
    fn resuming_names_the_conversation() {
        assert_eq!(
            resume_args(&SessionId::parse("c-1").unwrap()),
            vec![OsString::from("--conversation"), OsString::from("c-1")]
        );
    }

    #[test]
    fn the_conversation_the_index_names_for_the_checkout_is_the_one_observed() {
        let root = cli_root("agy-session-observe");
        write_index(
            &root,
            &[
                ("/work/slot", "f85c1dbb-27bc-438b-9191-b007994e8f65"),
                ("/work/other", "f85c1dbb-27bc-438b-9191-b007994e8f63"),
            ],
        );

        assert_eq!(
            observe(
                &root,
                &ObservationContext {
                    cwd: Path::new("/work/slot"),
                    since_unix: 0,
                    preceded_by: Some(
                        &SessionId::parse("f85c1dbb-27bc-438b-9191-b007994e8f64").unwrap()
                    ),
                    claimed_elsewhere: &[],
                },
            ),
            Some(SessionId::parse("f85c1dbb-27bc-438b-9191-b007994e8f65").unwrap())
        );
    }

    /// The recycled-slot case this harness needs a value guard for: the
    /// index still names the previous task's conversation because this
    /// launch has not written one yet.
    #[test]
    fn an_index_entry_unchanged_since_the_launch_is_not_this_task_s_conversation() {
        let root = cli_root("agy-session-previous");
        write_index(
            &root,
            &[("/work/slot", "f85c1dbb-27bc-438b-9191-b007994e8f64")],
        );

        assert_eq!(
            observe(
                &root,
                &ObservationContext {
                    cwd: Path::new("/work/slot"),
                    since_unix: 0,
                    preceded_by: Some(
                        &SessionId::parse("f85c1dbb-27bc-438b-9191-b007994e8f64").unwrap()
                    ),
                    claimed_elsewhere: &[],
                },
            ),
            None
        );
    }

    #[test]
    fn what_the_index_held_at_launch_is_readable_on_its_own() {
        let root = cli_root("agy-session-recorded");
        write_index(
            &root,
            &[("/work/slot", "f85c1dbb-27bc-438b-9191-b007994e8f62")],
        );

        assert_eq!(
            recorded_for(&root, Path::new("/work/slot")),
            Some(SessionId::parse("f85c1dbb-27bc-438b-9191-b007994e8f62").unwrap())
        );
        assert_eq!(recorded_for(&root, Path::new("/work/unknown")), None);
    }

    /// The index is a plain file anything can write: an entry that is not
    /// one of the harness's own identifiers is no conversation at all.
    #[test]
    fn an_index_entry_not_named_by_the_harness_is_not_a_conversation() {
        let root = cli_root("agy-session-shape");
        write_index(
            &root,
            &[("/work/slot", "--yolo"), ("/work/other", "../../escape")],
        );

        assert_eq!(recorded_for(&root, Path::new("/work/slot")), None);
        assert_eq!(recorded_for(&root, Path::new("/work/other")), None);
    }

    #[test]
    fn a_conversation_exists_only_while_its_own_store_does() {
        let root = cli_root("agy-session-exists");
        fs::create_dir_all(root.join("conversations")).unwrap();
        fs::write(root.join("conversations").join("kept.db"), b"").unwrap();

        assert!(exists(&root, &SessionId::parse("kept").unwrap()));
        assert!(!exists(&root, &SessionId::parse("deleted").unwrap()));
    }

    #[test]
    fn a_first_turn_is_an_interactive_prompt_beside_the_conversation() {
        let session = SessionId::parse("conv-1").unwrap();
        let line: Vec<OsString> = resume_args(&session)
            .into_iter()
            .chain(first_turn_args("carry on"))
            .collect();
        assert_eq!(
            line,
            ["--conversation", "conv-1", "-i", "carry on"].map(OsString::from)
        );
    }
}
