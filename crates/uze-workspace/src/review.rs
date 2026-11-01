//! What the operator wrote reviewing an agent's work, kept for the agent.
//!
//! The review surface writes the operator's comments and viewed marks; the
//! agent reads what was sent to it and answers each comment from its own
//! process (`uze agent review show`, `uze agent review reply`). Both write
//! the same record, so every change is a read-modify-write under the
//! project's review lock.
//!
//! # Keyed on the agent
//!
//! One record per agent, among the project's records: naming the work
//! renames the branch, and unnamed work is named later from its first
//! commit, so a review keyed on the branch would be lost the moment the
//! agent did what it is told to. The record goes when the agent is
//! forgotten.
//!
//! # Read as untrusted
//!
//! The launch key proves which agent answered, not that what it wrote is
//! safe: any process the agent starts inherits the key, and any process of
//! the operator's user can edit the file. So an answer is bounded and its
//! control characters are taken out when it is written, the surface takes
//! them out again when it draws it, and an answer never touches what the
//! operator wrote.

use std::{collections::BTreeMap, path::Path, time::Duration};

use serde::{Deserialize, Serialize};
use uze_core::path::Canonical as _;

use crate::{
    Result, UzeError,
    conversation::{Claim, owner_of},
    harness_runtime::project_id_for,
    home::UzeHome,
    persistence::write_atomic_private,
    task::{self, AgentId},
};

pub const SCHEMA_VERSION: u32 = 1;

/// The most bytes one answer keeps.
pub const ANSWER_MAX: usize = 4 << 10;
/// The most answers one comment keeps.
pub const ANSWERS_MAX: usize = 8;

/// How long a rewrite waits for another one.
const REWRITE_PATIENCE: Duration = Duration::from_secs(5);

/// One agent's review.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReviewRecord {
    #[serde(default = "shape")]
    pub schema_version: u32,
    #[serde(default)]
    pub comments: Vec<Comment>,
    /// Each viewed file, by its path in the checkout, with the fingerprint
    /// of the diff it was viewed on.
    #[serde(default)]
    pub viewed: BTreeMap<String, u64>,
    #[serde(default)]
    pub next_send: u32,
    #[serde(default)]
    pub next_comment: u64,
}

fn shape() -> u32 {
    SCHEMA_VERSION
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Comment {
    pub id: u64,
    pub path: String,
    #[serde(default)]
    pub rows: Vec<Row>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub excerpt: String,
    pub text: String,
    pub status: CommentStatus,
    /// The send it went out in and its number there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent: Option<(u32, u32)>,
    /// The agent it was sent to, who alone may answer it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentStatus {
    #[default]
    Draft,
    Sent,
    Answered,
    Resolved,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Row {
    pub kind: RowKind,
    pub line: u32,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    #[default]
    Context,
    Added,
    Removed,
}

/// A record: what an operator wrote about an agent's work is known nowhere
/// else.
impl uze_document::Shaped for ReviewRecord {
    const SHAPE: u32 = SCHEMA_VERSION;
    const KIND: &'static str = "review";
}

/// Where `agent`'s review of `project_root` is kept.
pub fn store_path(home: &UzeHome, project_root: &Path, agent: &AgentId) -> std::path::PathBuf {
    home.review_path(&project_id_for(project_root), agent.as_str())
}

/// The review kept for `agent`, or an empty one. A record that cannot be
/// read is an error, never an empty review: writing an empty one over it
/// would lose what the operator wrote.
pub fn load(home: &UzeHome, project_root: &Path, agent: &AgentId) -> Result<ReviewRecord> {
    let path = store_path(home, project_root, agent);
    Ok(uze_document::read::<ReviewRecord>(&path)?.or_default())
}

/// Reads `agent`'s review, lets `change` edit it, and writes it back when
/// it changed, as one operation under the project's review lock.
pub fn update<T>(
    home: &UzeHome,
    project_root: &Path,
    agent: &AgentId,
    change: impl FnOnce(&mut ReviewRecord) -> T,
) -> Result<(ReviewRecord, T)> {
    crate::record::ensure(home, project_root)?;
    let _held = crate::file_lock::Held::acquire(
        &home.review_lock_path(&project_id_for(project_root)),
        REWRITE_PATIENCE,
    )?;
    let mut record = load(home, project_root, agent)?;
    let read = record.clone();
    let outcome = change(&mut record);
    if record != read {
        record.schema_version = SCHEMA_VERSION;
        let path = store_path(home, project_root, agent);
        let payload =
            serde_json::to_vec_pretty(&record).expect("review record serialization is infallible");
        write_atomic_private(&path, &payload)?;
    }
    Ok((record, outcome))
}

/// Forgets an agent's review. Best-effort: a record already gone is the
/// outcome asked for.
pub fn forget(home: &UzeHome, project_root: &Path, agent: &AgentId) {
    let _ = std::fs::remove_file(store_path(home, project_root, agent));
}

/// The project and agent whose checkout `checkout` is: the agent the
/// nearest project's records place there.
pub fn agent_of_checkout(home: &UzeHome, checkout: &Path) -> Option<(std::path::PathBuf, AgentId)> {
    let checkout = checkout
        .canonical()
        .unwrap_or_else(|_| checkout.to_path_buf());
    checkout.ancestors().find_map(|root| {
        if !task::store_path(home, root).exists() {
            return None;
        }
        let store = task::load(home, root).ok()?;
        store
            .agents
            .iter()
            .rev()
            .find(|agent| {
                agent
                    .own_directory(root)
                    .is_some_and(|own| own.canonical().unwrap_or(own) == checkout)
            })
            .map(|agent| (root.to_path_buf(), agent.id.clone()))
    })
}

/// The review of the agent working in `checkout`, for the review surface.
pub fn of_checkout(home: &UzeHome, checkout: &Path) -> Result<ReviewRecord> {
    let (project, agent) = agent_of_checkout(home, checkout).ok_or_else(no_agent)?;
    load(home, &project, &agent)
}

/// [`update`], for the agent working in `checkout`.
pub fn update_checkout(
    home: &UzeHome,
    checkout: &Path,
    change: impl FnOnce(&mut ReviewRecord),
) -> Result<ReviewRecord> {
    let (project, agent) = agent_of_checkout(home, checkout).ok_or_else(no_agent)?;
    update(home, &project, &agent, change).map(|(record, ())| record)
}

fn no_agent() -> UzeError {
    UzeError::AgentWork("no agent UZE launched works in this checkout".to_owned())
}

/// What `uze agent review show` prints for the agent making `claim`: each
/// comment sent to it and not yet answered, in full.
pub fn show(home: &UzeHome, claim: Claim<'_>) -> Result<String> {
    let owner = owner_of(home, claim).ok_or_else(not_launched)?;
    let record = load(home, &owner.project_root, &owner.agent)?;
    let waiting: Vec<&Comment> = record
        .comments
        .iter()
        .filter(|comment| {
            comment.status == CommentStatus::Sent
                && comment.agent.as_deref() == Some(owner.agent.as_str())
        })
        .collect();
    if waiting.is_empty() {
        return Ok("No review comment waits for you.\n".to_owned());
    }
    let mut out = format!(
        "{} review comment{} wait{} for you. Address each, then answer it with \
         `uze agent review reply <send>/<n> \"<answer>\"`.\n",
        waiting.len(),
        if waiting.len() == 1 { "" } else { "s" },
        if waiting.len() == 1 { "s" } else { "" },
    );
    for comment in waiting {
        let (send, n) = comment.sent.unwrap_or_default();
        let lines: Vec<u32> = comment.rows.iter().map(|row| row.line).collect();
        let at = match (lines.iter().min(), lines.iter().max()) {
            (Some(from), Some(to)) if from == to => format!(":{from}"),
            (Some(from), Some(to)) => format!(":{from}-{to}"),
            _ => String::new(),
        };
        out.push_str(&format!("\n{send}/{n}  {}{at}\n", comment.path));
        for row in &comment.rows {
            let sign = match row.kind {
                RowKind::Added => '+',
                RowKind::Removed => '-',
                RowKind::Context => ' ',
            };
            out.push_str(&format!("  > {sign}{}\n", row.text));
        }
        if !comment.excerpt.is_empty() {
            out.push_str(&format!("  on \"{}\"\n", comment.excerpt));
        }
        for line in comment.text.lines() {
            out.push_str(&format!("  {line}\n"));
        }
    }
    Ok(out)
}

/// Attaches `answer` to comment `reference` (`<send>/<n>`), sent to the
/// agent making `claim`. Writes the answer and its status, nothing else.
pub fn reply(home: &UzeHome, claim: Claim<'_>, reference: &str, answer: &str) -> Result<()> {
    let owner = owner_of(home, claim).ok_or_else(not_launched)?;
    let wanted = parse_reference(reference)
        .ok_or_else(|| UzeError::AgentWork(format!("`{reference}` is not <send>/<n>")))?;
    let answer = bounded(answer);
    if answer.trim().is_empty() {
        return Err(UzeError::AgentWork("the answer is empty".to_owned()));
    }
    let agent = owner.agent.clone();
    let (_, found) = update(home, &owner.project_root, &owner.agent, |record| {
        let comment = record.comments.iter_mut().find(|comment| {
            comment.sent == Some(wanted)
                && comment.agent.as_deref() == Some(agent.as_str())
                && matches!(
                    comment.status,
                    CommentStatus::Sent | CommentStatus::Answered
                )
        })?;
        if comment.answers.len() >= ANSWERS_MAX {
            return Some(false);
        }
        comment.answers.push(answer.clone());
        comment.status = CommentStatus::Answered;
        Some(true)
    })?;
    match found {
        Some(true) => Ok(()),
        Some(false) => Err(UzeError::AgentWork(format!(
            "comment {reference} already holds {ANSWERS_MAX} answers"
        ))),
        None => Err(UzeError::AgentWork(format!(
            "no comment {reference} was sent to you"
        ))),
    }
}

fn not_launched() -> UzeError {
    UzeError::AgentWork("this process is not an agent UZE launched".to_owned())
}

fn parse_reference(reference: &str) -> Option<(u32, u32)> {
    let (send, n) = reference.trim().split_once('/')?;
    Some((send.parse().ok()?, n.parse().ok()?))
}

/// `answer` with its control characters taken out, cut at [`ANSWER_MAX`]
/// bytes on a character boundary, saying so where it was cut.
fn bounded(answer: &str) -> String {
    let clean: String = answer
        .chars()
        .filter(|character| *character == '\n' || !character.is_control())
        .collect();
    if clean.len() <= ANSWER_MAX {
        return clean;
    }
    let mut cut = ANSWER_MAX;
    while !clean.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{} … (cut)", &clean[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Agent, AgentStore};

    /// A project with two agents working in its root, each issued a key:
    /// the home, the root, and each agent's id and key.
    fn two_agents(
        label: &str,
    ) -> (
        UzeHome,
        std::path::PathBuf,
        (AgentId, String),
        (AgentId, String),
    ) {
        let home = UzeHome::at(uze_testkit::temp::scratch(label));
        let root = uze_testkit::temp::scratch(&format!("{label}-project"))
            .canonical()
            .unwrap();
        let mut store = AgentStore::default();
        let mut agents = Vec::new();
        for _ in 0..2 {
            let mut agent = Agent::in_the_root("claude-code");
            let key = agent.issue_launch_key().unwrap();
            agents.push((agent.id.clone(), key));
            store.upsert(agent);
        }
        task::save(&home, &root, &store).unwrap();
        let second = agents.pop().unwrap();
        let first = agents.pop().unwrap();
        (home, root, first, second)
    }

    fn claim<'a>(agent: &'a (AgentId, String), cwd: &'a Path) -> Claim<'a> {
        Claim {
            id: agent.0.as_str(),
            key: &agent.1,
            cwd,
        }
    }

    /// The operator drafted one comment and sent it to `to`, as the
    /// review surface does.
    fn sent_to(home: &UzeHome, root: &Path, to: &AgentId) {
        update(home, root, to, |record| {
            record.comments.push(Comment {
                id: 1,
                path: "naming.rs".to_owned(),
                rows: vec![Row {
                    kind: RowKind::Added,
                    line: 53,
                    text: "fn apply() {".to_owned(),
                }],
                text: "Refuse a taken name.".to_owned(),
                status: CommentStatus::Sent,
                sent: Some((1, 1)),
                agent: Some(to.as_str().to_owned()),
                ..Comment::default()
            });
        })
        .unwrap();
    }

    #[test]
    fn the_agent_reads_what_was_sent_to_it_and_answers_it_in_place() {
        let (home, root, first, _) = two_agents("review-answered");
        sent_to(&home, &root, &first.0);
        let shown = show(&home, claim(&first, &root)).unwrap();
        assert!(shown.contains("1/1  naming.rs:53"), "{shown}");
        assert!(shown.contains("> +fn apply() {"), "{shown}");
        assert!(shown.contains("Refuse a taken name."), "{shown}");

        reply(&home, claim(&first, &root), "1/1", "apply refuses it now").unwrap();
        let record = load(&home, &root, &first.0).unwrap();
        let comment = &record.comments[0];
        assert_eq!(comment.status, CommentStatus::Answered);
        assert_eq!(comment.answers, ["apply refuses it now"]);
        assert_eq!(
            comment.text, "Refuse a taken name.",
            "the operator's words stand"
        );
        assert!(
            show(&home, claim(&first, &root))
                .unwrap()
                .contains("No review comment")
        );
    }

    #[test]
    fn a_reply_from_outside_an_agent_or_to_another_agents_comment_attaches_nothing() {
        let (home, root, first, second) = two_agents("review-refused");
        sent_to(&home, &root, &first.0);
        let stranger = (first.0.clone(), "not-the-key".to_owned());
        assert!(reply(&home, claim(&stranger, &root), "1/1", "done").is_err());
        assert!(reply(&home, claim(&second, &root), "1/1", "done").is_err());
        assert!(reply(&home, claim(&first, &root), "1/9", "done").is_err());
        let record = load(&home, &root, &first.0).unwrap();
        assert!(record.comments[0].answers.is_empty());
        assert_eq!(record.comments[0].status, CommentStatus::Sent);
    }

    #[test]
    fn a_record_that_cannot_be_read_is_reported_and_never_written_over() {
        let (home, root, first, _) = two_agents("review-unreadable");
        let path = store_path(&home, &root, &first.0);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let newer = br#"{"schema_version": 99, "comments": "not this shape"}"#;
        std::fs::write(&path, newer).unwrap();
        assert!(load(&home, &root, &first.0).is_err());
        assert!(update(&home, &root, &first.0, |record| record.next_send = 7).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), newer, "left as it was");
    }

    #[test]
    fn a_review_follows_its_agent_however_the_branch_is_named() {
        let (home, root, first, _) = two_agents("review-renamed");
        sent_to(&home, &root, &first.0);
        let path = store_path(&home, &root, &first.0);
        assert!(path.ends_with(format!("reviews/{}.json", first.0.as_str())));
        forget(&home, &root, &first.0);
        assert!(load(&home, &root, &first.0).unwrap().comments.is_empty());
    }

    #[test]
    fn an_answer_loses_its_escapes_and_is_cut_at_its_bound() {
        assert_eq!(bounded("ok\u{1b}[2J done"), "ok[2J done");
        let long = "é".repeat(ANSWER_MAX);
        let cut = bounded(&long);
        assert!(cut.len() <= ANSWER_MAX + "… (cut)".len() + 1);
        assert!(cut.ends_with("(cut)"));
    }

    #[test]
    fn a_reference_is_a_send_and_a_number() {
        assert_eq!(parse_reference("3/1"), Some((3, 1)));
        assert_eq!(parse_reference("3"), None);
        assert_eq!(parse_reference("a/1"), None);
    }
}
