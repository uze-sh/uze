//! Conversation continuity for OpenCode.
//!
//! `--session <id>` continues a conversation, and creates it under that id
//! when there is none, so UZE names it at launch. Its sessions live in a
//! database UZE will not open; what it does read is the harness's own API —
//! `opencode api GET /api/session[/<id>]` — asked by id whether a
//! conversation is still there, and listed for the one the person started
//! from inside the TUI. Asking the vendor through the interface it
//! publishes is both cheaper to keep working than a private store and the
//! only one it documents.
//!
//! That call spawns the real binary, so it belongs off the launch path: the
//! read-back runs where a background answer already runs, never in the shim.
//!
//! This is also the only channel for this harness. The others tell UZE which
//! conversation they are in through a hook dispatch UZE itself runs; here the
//! generated plugin *is* the runner and spawns the handler directly, so no
//! UZE process ever sees the payload. Nothing is lost — the listing answers
//! the same question — but it is answered on the refresh cadence rather than
//! at the agent's next tool call.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use uze_core::{integration::ObservationContext, session::SessionId};

use crate::shared::process;

pub(super) const RESUME_FLAG: &str = "--session";
pub(super) const FIRST_TURN_FLAG: &str = "--prompt";
const LISTING_ARGS: [&str; 3] = ["api", "GET", "/api/session"];
/// What the API answers, with exit 1, for an id it holds no session under
/// (2.0.29).
const NOT_FOUND: &str = "SessionNotFoundError";
/// The API answers in milliseconds; every comparison here is in seconds.
const MILLISECONDS: u64 = 1_000;

/// A name in the only shape OpenCode's schema requires: a `ses` prefix.
pub(super) fn new_id() -> SessionId {
    conversation_named(format!(
        "ses_{}",
        SessionId::generate().as_str().replace('-', "")
    ))
    .expect("a minted name has OpenCode's shape")
}

/// The conversation `id` names, when it has OpenCode's shape: `ses_`
/// followed by letters and digits, as it lists them and as UZE mints them.
fn conversation_named(id: impl Into<String>) -> Option<SessionId> {
    SessionId::parse(id).ok().filter(|session| {
        session
            .as_str()
            .strip_prefix("ses_")
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric()))
    })
}

/// The operator's first turn. Behind `--prompt`, which the TUI takes beside
/// `--session <id>`.
pub(super) fn first_turn_args(turn: &str) -> Vec<OsString> {
    vec![OsString::from(FIRST_TURN_FLAG), OsString::from(turn)]
}

pub(super) fn resume_args(session: &SessionId) -> Vec<OsString> {
    vec![
        OsString::from(RESUME_FLAG),
        OsString::from(session.as_str()),
    ]
}

/// One conversation as the harness lists it.
struct Listed {
    id: String,
    directory: String,
    created_unix: u64,
}

/// The newest conversation the harness lists for `ctx.cwd` since the launch.
pub(super) fn observe(
    executable: &Path,
    home: &Path,
    ctx: &ObservationContext,
) -> Option<SessionId> {
    newest_of(listing(executable, home)?, ctx)
}

fn newest_of(listed: Vec<Listed>, ctx: &ObservationContext) -> Option<SessionId> {
    listed
        .into_iter()
        .filter(|listed| {
            Path::new(&listed.directory) == ctx.cwd && listed.created_unix >= ctx.since_unix
        })
        .filter_map(|listed| Some((listed.created_unix, conversation_named(listed.id)?)))
        .filter(|(_, session)| ctx.is_unclaimed(session))
        .max_by_key(|(created_unix, _)| *created_unix)
        .map(|(_, session)| session)
}

/// Whether the harness still holds `session`, asked by id: the listing is
/// one page, and a conversation past it is not gone.
pub(super) fn exists(executable: &Path, home: &Path, session: &SessionId) -> bool {
    let path = format!("/api/session/{}", session.as_str());
    // Unreadable is not gone: a harness that cannot answer keeps its
    // conversation, and a resume that then fails is its own error rather
    // than a conversation UZE dropped on a guess.
    process::capture_read_only(executable, home, &["api", "GET", path.as_str()])
        .map_or(true, |output| !names_a_missing_session(&output.stdout))
}

fn names_a_missing_session(payload: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(payload)
        .ok()
        .and_then(|document| document.get("_tag")?.as_str().map(|tag| tag == NOT_FOUND))
        .unwrap_or(false)
}

fn listing(executable: &Path, home: &Path) -> Option<Vec<Listed>> {
    let output = process::capture_read_only(executable, home, &LISTING_ARGS).ok()?;
    if !output.status.success() {
        return None;
    }
    parse_listing(&output.stdout)
}

fn parse_listing(payload: &[u8]) -> Option<Vec<Listed>> {
    let document: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let entries = document.get("data")?.as_array()?;
    Some(
        entries
            .iter()
            .filter_map(|entry| {
                Some(Listed {
                    id: entry
                        .get("id")
                        .and_then(serde_json::Value::as_str)?
                        .to_owned(),
                    directory: entry
                        .get("location")?
                        .get("directory")
                        .and_then(serde_json::Value::as_str)?
                        .to_owned(),
                    created_unix: entry
                        .get("time")?
                        .get("created")
                        .and_then(serde_json::Value::as_u64)?
                        / MILLISECONDS,
                })
            })
            .collect(),
    )
}

/// The real binary behind the harness, for the API call. `None` when it is
/// not installed, which reads as "nothing to observe".
pub(super) fn executable(shims_dir: &Path) -> Option<PathBuf> {
    uze_core::harness_runtime::resolve_real_executable(&["opencode", "opencode2"], shims_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(entries: &[(&str, &str, u64)]) -> Vec<u8> {
        let data: Vec<serde_json::Value> = entries
            .iter()
            .map(|(id, directory, created)| {
                serde_json::json!({
                    "id": id,
                    "location": {"directory": directory},
                    "time": {"created": created * MILLISECONDS},
                })
            })
            .collect();
        serde_json::to_vec(&serde_json::json!({"data": data})).unwrap()
    }

    fn observed(entries: &[(&str, &str, u64)], cwd: &str, since: u64) -> Option<SessionId> {
        newest_of(
            parse_listing(&payload(entries)).unwrap(),
            &ObservationContext {
                cwd: Path::new(cwd),
                since_unix: since,
                preceded_by: None,
                claimed_elsewhere: &[],
            },
        )
    }

    /// What the harness's API lists is the harness's, and a listing can
    /// be answered by whatever is first on the path: only an identifier of
    /// OpenCode's own shape is a conversation.
    #[test]
    fn only_an_identifier_of_opencode_s_shape_is_a_conversation() {
        assert!(conversation_named("ses_3b8fZq0aB1").is_some());
        assert!(conversation_named(new_id().as_str()).is_some());
        for refused in ["--yolo", "ses_", "ses_../x", "c-1", "SES_1"] {
            assert_eq!(conversation_named(refused), None, "{refused}");
        }
    }

    #[test]
    fn a_listed_id_of_another_shape_is_never_observed() {
        let entries = [
            ("SES_1", "/work/slot", 400),
            ("c-1", "/work/slot", 300),
            ("ses_../x", "/work/slot", 200),
            ("ses_kept", "/work/slot", 100),
        ];
        assert_eq!(
            observed(&entries, "/work/slot", 50),
            Some(SessionId::parse("ses_kept").unwrap())
        );
    }

    #[test]
    fn resuming_names_the_session() {
        assert_eq!(
            resume_args(&SessionId::parse("ses_1").unwrap()),
            vec![OsString::from("--session"), OsString::from("ses_1")]
        );
    }

    #[test]
    fn the_newest_conversation_for_the_checkout_is_the_one_observed() {
        let entries = [
            ("ses_old", "/work/slot", 100),
            ("ses_new", "/work/slot", 200),
            ("ses_other", "/work/other", 300),
        ];
        assert_eq!(
            observed(&entries, "/work/slot", 50),
            Some(SessionId::parse("ses_new").unwrap())
        );
    }

    #[test]
    fn a_conversation_from_before_the_launch_is_never_adopted() {
        let entries = [("ses_previous", "/work/slot", 100)];
        assert_eq!(observed(&entries, "/work/slot", 500), None);
    }

    #[test]
    fn a_minted_name_has_the_shape_opencode_accepts() {
        let id = new_id();
        assert!(id.as_str().starts_with("ses_"), "{id}");
        assert!(!id.as_str().contains('-'), "{id}");
        assert_ne!(new_id(), id);
    }

    #[test]
    fn only_the_harness_saying_so_makes_a_session_missing() {
        assert!(names_a_missing_session(
            br#"{"_tag":"SessionNotFoundError","sessionID":"ses_x","message":"Session not found: ses_x"}"#
        ));
        assert!(!names_a_missing_session(
            br#"{"data":{"id":"ses_x","location":{"directory":"/work"}}}"#
        ));
        assert!(!names_a_missing_session(b"HTTP 502 Bad Gateway"));
        assert!(!names_a_missing_session(b""));
    }

    #[test]
    fn an_answer_that_is_not_a_listing_is_no_listing_at_all() {
        assert!(parse_listing(b"<!doctype html>").is_none());
    }

    #[test]
    fn a_first_turn_rides_on_its_prompt_flag_beside_the_session() {
        let session = SessionId::parse("ses_1").unwrap();
        let line: Vec<OsString> = resume_args(&session)
            .into_iter()
            .chain(first_turn_args("carry on"))
            .collect();
        assert_eq!(
            line,
            ["--session", "ses_1", "--prompt", "carry on"].map(OsString::from)
        );
    }
}
