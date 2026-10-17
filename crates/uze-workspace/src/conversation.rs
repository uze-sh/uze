//! The conversation an agent is in, remembered for the agent.
//!
//! # Bound to the agent, never to the directory
//!
//! A slot is reused, and a space's root is shared by every agent in it:
//! the directory an agent stands in was somebody else's yesterday, or is
//! somebody else's right now. A conversation keyed on the directory would
//! therefore hand one agent another's history. Keyed on the agent's
//! identity, a recycled slot simply finds nothing, and a task given its
//! slot back finds exactly what it left.
//!
//! # Advisory, not authoritative
//!
//! Everything here answers one question — should this launch resume, and
//! what — and the answer "no idea" is always safe: the agent starts fresh.
//! So an unreadable, corrupt or unknown-schema document reads as *no
//! record* rather than as an error. The task store refuses a schema it does
//! not know because guessing at a task's state could destroy work; this
//! document only decides whether a conversation is carried over, and
//! refusing to launch over it would trade a lost conversation for a lost
//! agent.
//!
//! # Storage
//!
//! One JSON document per agent, under
//! `UzeHome::state_dir()/conversations/<project id>/<agent id>.json` — the
//! same project key `state/tasks/<project id>.json` uses, so both are
//! outside every checkout by construction. One file per agent rather than a
//! field on the task store, because a launch writes this and launches
//! happen in their own processes: two agents starting at once would
//! otherwise rewrite one document and drop each other's work.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use uze_core::path::Canonical as _;

use serde::{Deserialize, Serialize};

pub use crate::session::SessionId;
use crate::{
    Result,
    harness_runtime::project_id_for,
    home::UzeHome,
    persistence::write_atomic,
    task::{self, AgentId, now_unix},
};

pub const SCHEMA_VERSION: u32 = 3;

/// How the identifier in a record got there, which is what the next launch
/// needs to know: one that UZE named is minted again for a new task, one
/// the harness named is waited for.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationOrigin {
    Assigned,
    Observed,
}

/// One harness's conversation for one agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HarnessConversation {
    /// `None` while a launch is still waiting to be read back — the pending
    /// state is the absence of an answer, not a flag beside it.
    pub conversation: Option<SessionId>,
    pub origin: ConversationOrigin,
    /// The floor a read-back accepts a conversation above, and the token an
    /// answer carries back so a launch that has since been replaced cannot
    /// be overwritten by the previous one's answer.
    pub launched_at_unix: u64,
    pub observed_at_unix: Option<u64>,
    /// What the harness's own records pointed at for this checkout when
    /// this launch started, for a harness that keeps one entry per
    /// directory instead of timestamping its conversations. Reading back
    /// that same value means nothing new was started, so the previous
    /// occupant's conversation can never be adopted.
    pub preceded_by: Option<SessionId>,
    /// When the agent last moved on from this harness to another one. A
    /// resume records no launch, so this is the only mark of when a
    /// harness that comes back stopped following the work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_at_unix: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConversationRecord {
    pub schema_version: u32,
    pub agent: AgentId,
    /// Keyed by integration id, so a fifth harness adds a key rather than a
    /// shape.
    pub harnesses: BTreeMap<String, HarnessConversation>,
    /// The briefing written for the harness the agent was last switched
    /// to: the one conversation text the record holds, because the channel
    /// that gives it on every launch keeps no copy UZE could read back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub briefing: Option<Briefing>,
}

/// What the agent was doing when it was switched, for the harness it was
/// switched to.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Briefing {
    pub integration: String,
    pub text: String,
    /// The checkout's commit when the switch happened. Once the branch has
    /// moved past it the work has moved on, and the briefing retires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_at_switch: Option<String>,
    /// Set once a harness with no invisible channel was pointed at the
    /// briefing by its first turn, which is given only once.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub first_turn_given: bool,
}

impl ConversationRecord {
    pub fn new(agent: AgentId) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            agent,
            harnesses: BTreeMap::new(),
            briefing: None,
        }
    }

    /// The briefing written for `integration`, if one applies to it.
    pub fn briefing_for(&self, integration: &str) -> Option<&Briefing> {
        self.briefing
            .as_ref()
            .filter(|briefing| briefing.integration == integration)
    }

    /// The briefing for `integration` whose first turn has not been given
    /// yet, marking it given: a harness without an invisible channel is
    /// pointed at it by one launch only.
    pub fn take_first_turn(&mut self, integration: &str) -> Option<String> {
        let briefing = self
            .briefing
            .as_mut()
            .filter(|briefing| briefing.integration == integration && !briefing.first_turn_given)?;
        briefing.first_turn_given = true;
        Some(briefing.text.clone())
    }

    /// Retires the briefing once the checkout's commit is no longer the
    /// one it was switched at. Returns whether it did.
    pub fn retire_briefing_past(&mut self, head: &str) -> bool {
        let moved_on = self
            .briefing
            .as_ref()
            .and_then(|briefing| briefing.head_at_switch.as_deref())
            .is_some_and(|at_switch| at_switch != head);
        if moved_on {
            self.briefing = None;
        }
        moved_on
    }

    pub fn get(&self, integration: &str) -> Option<&HarnessConversation> {
        self.harnesses.get(integration)
    }

    /// Records a launch, replacing whatever the previous one left. An
    /// assigned conversation is known here; an observed one is not, and
    /// stays pending until it is read back.
    pub fn launched(
        &mut self,
        integration: &str,
        origin: ConversationOrigin,
        conversation: Option<SessionId>,
        preceded_by: Option<SessionId>,
    ) {
        self.harnesses.insert(
            integration.to_owned(),
            HarnessConversation {
                conversation,
                origin,
                launched_at_unix: now_unix(),
                observed_at_unix: None,
                preceded_by,
                left_at_unix: None,
            },
        );
    }

    /// Marks that the agent moved on from `integration` now.
    pub fn left(&mut self, integration: &str) {
        if let Some(entry) = self.harnesses.get_mut(integration) {
            entry.left_at_unix = Some(now_unix());
        }
    }

    /// Writes back what was observed for `launched_at_unix`'s launch.
    ///
    /// Returns whether it applied: an answer whose launch this record no
    /// longer names is an answer to a launch that has been replaced, and is
    /// dropped rather than written — the same rule the client applies to a
    /// Git answer that arrives after the viewer moved on.
    pub fn observed(
        &mut self,
        integration: &str,
        launched_at_unix: u64,
        conversation: SessionId,
    ) -> bool {
        let Some(entry) = self.harnesses.get_mut(integration) else {
            return false;
        };
        if entry.launched_at_unix != launched_at_unix {
            return false;
        }
        entry.conversation = Some(conversation);
        entry.observed_at_unix = Some(now_unix());
        true
    }
}

/// The document for one agent of `project_root`.
pub fn store_path(home: &UzeHome, project_root: &Path, agent: &AgentId) -> PathBuf {
    home.conversation_path(&project_id(project_root), agent.as_str())
}

pub(crate) fn project_id(project_root: &Path) -> String {
    let canonical = project_root
        .canonical()
        .unwrap_or_else(|_| project_root.to_path_buf());
    project_id_for(&canonical)
}

/// How long a rewrite waits for another one. Short, because the launch
/// path waits on it and what it guards is one small read and write.
const REWRITE_PATIENCE: Duration = Duration::from_secs(5);

/// Reads `agent`'s record, lets `change` edit it, and writes it back when it
/// changed, as one operation. The launched process ([`crate::continuity::plan`])
/// and the workspace client ([`crate::continuity::refresh`]) both rewrite the
/// same record, and each would otherwise write back the version it read
/// over the one the other just wrote.
pub fn update<T>(
    home: &UzeHome,
    project_root: &Path,
    agent: &AgentId,
    change: impl FnOnce(&mut ConversationRecord) -> T,
) -> Result<T> {
    // Before the lock, whose file sits in the project's directory and would
    // otherwise create it unmarked and readable by anyone.
    crate::record::ensure(home, project_root)?;
    let _held = crate::file_lock::Held::acquire(
        &home.conversations_lock_path(&project_id(project_root)),
        REWRITE_PATIENCE,
    )?;
    let mut record = load(home, project_root, agent);
    let read = record.clone();
    let outcome = change(&mut record);
    if record != read {
        save(home, project_root, &record)?;
    }
    Ok(outcome)
}

/// What was recorded for `agent`, or an empty record. Never fails: see the
/// module's note on why continuity state is advisory.
/// Best-effort: a conversation nobody can read costs a resumed session,
/// never work, so this answers with an empty record rather than refusing.
///
/// What it must not do — and used to — is discard a record of a shape this
/// build *knows*, in silence. A shape with a rung is carried across; only
/// what has none falls back to empty.
pub fn load(home: &UzeHome, project_root: &Path, agent: &AgentId) -> ConversationRecord {
    let path = store_path(home, project_root, agent);
    uze_document::read::<ConversationRecord>(&path)
        .ok()
        .and_then(uze_document::Carried::record)
        .unwrap_or_else(|| ConversationRecord::new(agent.clone()))
}

/// A record: which harness session this agent was last carrying. The
/// harness has its own sessions, but which one belonged to which agent is
/// only ever here.
impl uze_document::Shaped for ConversationRecord {
    const SHAPE: u32 = SCHEMA_VERSION;
    const KIND: &'static str = "conversation";

    fn ladder() -> uze_document::Ladder {
        &[uze_document::Step {
            from: 2,
            to: 3,
            // Shape 2's briefing named a generated file under `runtime/`,
            // which is not carried across; the conversations are.
            climb: |mut document| {
                if let Some(fields) = document.as_object_mut() {
                    fields.remove("briefing");
                }
                Ok(document)
            },
        }]
    }
}

/// Replaces the document atomically, unless a newer build wrote it: [`load`]
/// answers such a record with an empty one, which must not then be saved
/// over it.
pub fn save(home: &UzeHome, project_root: &Path, record: &ConversationRecord) -> Result<()> {
    crate::record::ensure(home, project_root)?;
    let path = store_path(home, project_root, &record.agent);
    if let Err(error) = uze_document::read::<ConversationRecord>(&path)
        && error.written_by_a_newer_build()
    {
        return Err(error.into());
    }
    let payload =
        serde_json::to_vec_pretty(record).expect("conversation record serialization is infallible");
    write_atomic(&path, &payload)
}

/// The conversations every other agent of `project_root` holds in
/// `integration`. Each is one small read of a directory kept to the
/// project's own agents, and an unreadable record holds nothing: at worst
/// an agent may still adopt that conversation, which is today's behaviour
/// rather than a new failure.
pub(crate) fn claimed_by_others(
    home: &UzeHome,
    project_root: &Path,
    agent: &AgentId,
    integration: &str,
) -> Vec<SessionId> {
    let own = store_path(home, project_root, agent);
    let Some(Ok(entries)) = own.parent().map(fs::read_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| *path != own)
        .filter_map(|path| {
            uze_document::read::<ConversationRecord>(&path)
                .ok()
                .and_then(uze_document::Carried::record)
        })
        .filter_map(|record| record.harnesses.get(integration)?.conversation.clone())
        .collect()
}

/// Forgets an agent's conversations. Best-effort by construction: a record
/// that is already gone is the outcome asked for.
pub fn forget(home: &UzeHome, project_root: &Path, agent: &AgentId) {
    // A channel that finds the context by conversation keeps one document
    // per conversation, outside the agent's own directory.
    for (integration, entry) in load(home, project_root, agent).harnesses {
        if let Some(session) = entry.conversation {
            let _ = fs::remove_file(
                home.session_context_dir(&integration)
                    .join(format!("{}.md", session.as_str())),
            );
        }
    }
    let _ = fs::remove_file(store_path(home, project_root, agent));
    let _ = fs::remove_dir_all(home.agent_context_dir(&project_id(project_root), agent.as_str()));
}

/// The agent a verified claim resolved to, which kind of record it is, and
/// the project root every record for it is keyed on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Owner {
    pub project_root: PathBuf,
    pub agent: AgentId,
    /// Whether the agent works in a checkout of its own. What a caller
    /// asks before offering anything that only means something against a
    /// branch — naming the work, delivering it.
    pub isolated: bool,
}

/// What a process says about itself: the identifier its launch carried,
/// the secret it was issued, and the directory it stands in. All three are
/// verified together — the identifier says *which* agent, the key proves
/// the process was started by that agent's launch, the directory says
/// *where* it should be — and a claim the record contradicts is no claim
/// at all. The identifier and the directory are the caller's own to
/// choose; the key is the one part another agent does not hold.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Claim<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub cwd: &'a Path,
}

/// The project whose task store the directory falls under: the nearest
/// ancestor one is kept for. The same walk [`owner_of`] makes, for a claim
/// it did not recognize.
pub fn project_of(home: &UzeHome, cwd: &Path) -> Option<PathBuf> {
    let cwd = cwd.canonical().unwrap_or_else(|_| cwd.to_path_buf());
    cwd.ancestors()
        .find(|root| task::store_path(home, root).exists())
        .map(Path::to_path_buf)
}

/// Whose agent is this process?
///
/// Ancestors of the claimed directory, nearest first, are asked whether a
/// store keyed on them names the identifier; the first that does answers,
/// provided the key is the one that agent's latest launch was issued and
/// the record's own directory contains the claimed one. A few `stat`s and
/// one small read: no subprocess, nothing that scales with the Store,
/// because a harness launch waits on this. `None` for a claim no record
/// backs — an identifier nobody recorded, a key that launch was not given,
/// a directory the record does not allow — which is what keeps an ordinary
/// invocation ordinary, and keeps a process that sets another agent's
/// identifier and stands in its checkout from being taken for it.
pub fn owner_of(home: &UzeHome, claim: Claim<'_>) -> Option<Owner> {
    let cwd = claim
        .cwd
        .canonical()
        .unwrap_or_else(|_| claim.cwd.to_path_buf());
    cwd.ancestors().find_map(|root| {
        if !task::store_path(home, root).exists() {
            return None;
        }
        let store = task::load(home, root).ok()?;
        let record = store.agent(claim.id)?;
        if !record.admits_launch(claim.key) {
            return None;
        }
        let own = record.own_directory(root)?;
        let own = own.canonical().unwrap_or(own);
        cwd.starts_with(&own).then(|| Owner {
            project_root: root.to_path_buf(),
            agent: record.id.clone(),
            isolated: record.is_isolated(),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Agent, AgentStore, Base};

    fn home(label: &str) -> UzeHome {
        UzeHome::at(uze_testkit::temp::scratch(label))
    }

    /// A project root of its own per test, so two tests never key the same
    /// document.
    fn project(label: &str) -> PathBuf {
        let root = uze_testkit::temp::scratch(label).join("repo");
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn task_named(checkout: &str) -> Agent {
        let mut agent = Agent::isolated(
            "claude",
            None,
            Base::Ref("main".into()),
            String::new(),
            "main".into(),
        );
        agent.isolation_mut().unwrap().checkout =
            Some(crate::checkout::CheckoutId::adopted(checkout));
        agent
    }

    #[test]
    fn a_record_written_before_briefings_reads_with_none_pending() {
        let record: ConversationRecord =
            serde_json::from_slice(br#"{"schema_version":2,"agent":"x","harnesses":{}}"#).unwrap();
        assert_eq!(record.briefing, None);
        assert!(
            !serde_json::to_string(&record).unwrap().contains("briefing"),
            "nothing pending writes nothing"
        );
    }

    fn briefing(integration: &str, head: Option<&str>) -> Briefing {
        Briefing {
            integration: integration.into(),
            text: "the work so far".into(),
            head_at_switch: head.map(str::to_owned),
            first_turn_given: false,
        }
    }

    #[test]
    fn a_briefing_applies_only_to_the_harness_it_was_written_for() {
        let mut record = ConversationRecord::new(AgentId::adopted("x"));
        record.briefing = Some(briefing("codex", None));
        assert!(record.briefing_for("claude").is_none());
        assert_eq!(
            record.briefing_for("codex").map(|b| b.text.as_str()),
            Some("the work so far")
        );
        // Given on every launch: reading it leaves it where it is.
        assert!(record.briefing_for("codex").is_some());
    }

    #[test]
    fn a_first_turn_is_given_once() {
        let mut record = ConversationRecord::new(AgentId::adopted("x"));
        record.briefing = Some(briefing("codex", None));
        assert_eq!(record.take_first_turn("claude"), None);
        assert_eq!(
            record.take_first_turn("codex").as_deref(),
            Some("the work so far")
        );
        assert_eq!(record.take_first_turn("codex"), None);
        assert!(record.briefing_for("codex").is_some());
    }

    #[test]
    fn a_briefing_retires_once_the_checkout_moved_past_its_switch() {
        let mut record = ConversationRecord::new(AgentId::adopted("x"));
        record.briefing = Some(briefing("codex", Some("abc")));
        assert!(!record.retire_briefing_past("abc"));
        assert!(record.briefing.is_some());
        assert!(record.retire_briefing_past("def"));
        assert!(record.briefing.is_none());
    }

    #[test]
    fn a_shape_two_record_is_carried_across_without_its_briefing_path() {
        let home = home("conversation-shape-two");
        let root = project("conversation-shape-two-project");
        let agent = AgentId::generate();
        let path = store_path(&home, &root, &agent);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            serde_json::json!({
                "schema_version": 2,
                "agent": agent.as_str(),
                "harnesses": {"claude": {
                    "conversation": "c-1", "origin": "assigned",
                    "launched_at_unix": 1, "observed_at_unix": null, "preceded_by": null
                }},
                "briefing": {"integration": "codex", "path": "/runtime/b.md"}
            })
            .to_string(),
        )
        .unwrap();
        let record = load(&home, &root, &agent);
        assert_eq!(record.schema_version, SCHEMA_VERSION);
        assert!(record.briefing.is_none());
        assert_eq!(
            record
                .get("claude")
                .and_then(|e| e.conversation.as_ref())
                .map(|c| c.as_str()),
            Some("c-1")
        );
    }

    #[test]
    fn a_record_round_trips_through_the_document() {
        let home = home("conversation-round-trip");
        let root = project("conversation-round-trip-project");
        let task = AgentId::generate();

        let mut record = ConversationRecord::new(task.clone());
        record.launched(
            "claude-code",
            ConversationOrigin::Assigned,
            Some(SessionId::new("c-1")),
            None,
        );
        save(&home, &root, &record).unwrap();

        let read = load(&home, &root, &task);
        assert_eq!(
            read.get("claude-code").and_then(|e| e.conversation.clone()),
            Some(SessionId::new("c-1"))
        );
    }

    /// The property the lock exists for: the launched process and the
    /// client rewriting one agent's record at once each keep what the
    /// other wrote.
    #[test]
    fn concurrent_rewrites_of_one_record_lose_nothing() {
        let home = home("conversation-concurrent");
        let root = project("conversation-concurrent-project");
        let task = AgentId::generate();
        let writers = 8;

        std::thread::scope(|scope| {
            for writer in 0..writers {
                let (home, root, task) = (&home, &root, &task);
                scope.spawn(move || {
                    update(home, root, task, |record| {
                        record.launched(
                            &format!("harness-{writer}"),
                            ConversationOrigin::Assigned,
                            Some(SessionId::new(format!("c-{writer}"))),
                            None,
                        );
                    })
                    .unwrap();
                });
            }
        });

        assert_eq!(load(&home, &root, &task).harnesses.len(), writers);
    }

    #[test]
    fn an_unreadable_document_reads_as_no_record_rather_than_as_an_error() {
        let home = home("conversation-unreadable");
        let root = project("conversation-unreadable-project");
        let task = AgentId::generate();
        let path = store_path(&home, &root, &task);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"{ not json").unwrap();

        assert!(load(&home, &root, &task).harnesses.is_empty());
    }

    #[test]
    fn a_document_from_a_schema_this_build_does_not_know_is_ignored_not_refused() {
        let home = home("conversation-unknown-schema");
        let root = project("conversation-unknown-schema-project");
        let task = AgentId::generate();
        let path = store_path(&home, &root, &task);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            br#"{"schema_version":99,"agent":"x","harnesses":{}}"#,
        )
        .unwrap();

        assert!(load(&home, &root, &task).harnesses.is_empty());
    }

    #[test]
    fn a_newer_builds_record_is_never_saved_over() {
        let home = home("conversation-newer-schema");
        let root = project("conversation-newer-schema-project");
        let task = AgentId::generate();
        let path = store_path(&home, &root, &task);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let newer = br#"{"schema_version":99,"agent":"x","harnesses":{}}"#;
        fs::write(&path, newer).unwrap();

        let mut record = load(&home, &root, &task);
        record.launched("claude-code", ConversationOrigin::Assigned, None, None);

        assert!(save(&home, &root, &record).is_err());
        assert_eq!(fs::read(&path).unwrap(), newer);
    }

    #[test]
    fn each_harness_keeps_its_own_conversation() {
        let mut record = ConversationRecord::new(AgentId::generate());
        record.launched(
            "claude-code",
            ConversationOrigin::Assigned,
            Some(SessionId::new("c-1")),
            None,
        );
        record.launched("codex", ConversationOrigin::Observed, None, None);

        assert_eq!(
            record.get("claude-code").unwrap().conversation,
            Some(SessionId::new("c-1"))
        );
        assert_eq!(record.get("codex").unwrap().conversation, None);
    }

    #[test]
    fn an_answer_to_a_replaced_launch_is_dropped() {
        let mut record = ConversationRecord::new(AgentId::generate());
        record.launched("codex", ConversationOrigin::Observed, None, None);
        let stale = record.get("codex").unwrap().launched_at_unix;

        // The pane was relaunched, so the entry names a different launch.
        record.launched("codex", ConversationOrigin::Observed, None, None);
        record.harnesses.get_mut("codex").unwrap().launched_at_unix = stale + 1;

        assert!(!record.observed("codex", stale, SessionId::new("late")));
        assert_eq!(record.get("codex").unwrap().conversation, None);
        assert!(record.observed("codex", stale + 1, SessionId::new("current")));
        assert_eq!(
            record.get("codex").unwrap().conversation,
            Some(SessionId::new("current"))
        );
    }

    #[test]
    fn a_claim_no_record_backs_has_no_owner() {
        let home = home("conversation-no-owner");
        let primary = project("conversation-no-owner-project");
        let slot = primary.join(".worktrees").join("slot-1");
        fs::create_dir_all(&slot).unwrap();
        let mut store = AgentStore::default();
        let mut task = task_named("slot-1");
        let key = task.issue_launch_key().unwrap();
        let id = task.id.as_str().to_owned();
        let mut sibling = task_named("slot-2");
        let sibling_key = sibling.issue_launch_key().unwrap();
        store.upsert(task);
        store.upsert(sibling);
        task::save(&home, &primary, &store).unwrap();
        let claim = |id, key, cwd| owner_of(&home, Claim { id, key, cwd });

        assert!(claim(&id, &key, &slot).is_some(), "the agent itself");
        assert_eq!(
            claim("unrecorded", &key, &slot),
            None,
            "an identifier nobody recorded"
        );
        assert_eq!(
            claim(&id, &key, &primary),
            None,
            "the operator's own checkout is not the task's directory"
        );
        let elsewhere = project("conversation-no-owner-elsewhere");
        assert_eq!(
            claim(&id, &key, &elsewhere),
            None,
            "a directory outside the project"
        );
        // The identifier and the directory are both the caller's to set;
        // the key is what its launch alone was given.
        assert_eq!(claim(&id, &sibling_key, &slot), None, "another agent's key");
        assert_eq!(claim(&id, "", &slot), None, "no key at all");
    }

    #[test]
    fn a_verified_claim_resolves_to_its_own_record_wherever_inside_its_directory() {
        let home = home("conversation-owner");
        let primary = project("conversation-owner-project");
        let slot = primary.join(".worktrees").join("slot-1");
        let nested = slot.join("src").join("deep");
        fs::create_dir_all(&nested).unwrap();

        let mut store = AgentStore::default();
        let mut previous = task_named("slot-1");
        let mut current = task_named("slot-1");
        let previous_key = previous.issue_launch_key().unwrap();
        let current_key = current.issue_launch_key().unwrap();
        let previous_id = previous.id.clone();
        let current_id = current.id.clone();
        store.upsert(previous);
        store.upsert(current);
        task::save(&home, &primary, &store).unwrap();

        let owner = owner_of(
            &home,
            Claim {
                id: current_id.as_str(),
                key: &current_key,
                cwd: &nested,
            },
        )
        .expect("the claim is backed by its record");
        assert_eq!(owner.agent, current_id);
        assert!(owner.isolated);
        assert_eq!(owner.project_root, primary.canonical().unwrap());
        // Two records over one slot are told apart by the identifier alone.
        let previous_owner = owner_of(
            &home,
            Claim {
                id: previous_id.as_str(),
                key: &previous_key,
                cwd: &slot,
            },
        )
        .expect("the slot's earlier agent still resolves by its own id");
        assert_eq!(previous_owner.agent, previous_id);
    }

    /// An agent placed before launches were issued a key has a record with
    /// none and a tab that relaunches it with none: it is still that agent,
    /// and comes back to its conversation. A record that was issued a key
    /// is no less strict for it.
    #[test]
    fn a_record_issued_no_key_admits_the_launch_that_names_it() {
        let home = home("conversation-before-keys");
        let primary = project("conversation-before-keys-project");
        let slot = primary.join(".worktrees").join("slot-1");
        fs::create_dir_all(&slot).unwrap();
        let mut store = AgentStore::default();
        let before_keys = task_named("slot-1");
        let mut keyed = task_named("slot-1");
        let key = keyed.issue_launch_key().unwrap();
        let before_keys_id = before_keys.id.clone();
        let keyed_id = keyed.id.clone();
        store.upsert(before_keys);
        store.upsert(keyed);
        task::save(&home, &primary, &store).unwrap();

        let owner = |id: &AgentId, key: &str| {
            owner_of(
                &home,
                Claim {
                    id: id.as_str(),
                    key,
                    cwd: &slot,
                },
            )
            .map(|owner| owner.agent)
        };
        assert_eq!(
            owner(&before_keys_id, ""),
            Some(before_keys_id.clone()),
            "a launch carrying no key names the record that was issued none"
        );
        assert_eq!(
            owner(&before_keys_id, "anything"),
            Some(before_keys_id),
            "whatever it carries, since nothing was issued to compare with"
        );
        assert_eq!(
            owner(&keyed_id, ""),
            None,
            "a record issued a key still refuses a launch carrying none"
        );
        assert_eq!(owner(&keyed_id, &key), Some(keyed_id));
    }

    #[test]
    fn a_claim_in_the_root_resolves_there_and_says_it_is_not_isolated() {
        let home = home("conversation-in-the-root-owner");
        let root = project("conversation-in-the-root-owner-project")
            .canonical()
            .unwrap();
        let nested = root.join("src");
        fs::create_dir_all(&nested).unwrap();
        let mut agent = Agent::in_the_root("claude-code");
        let key = agent.issue_launch_key().unwrap();
        let id = agent.id.clone();
        let mut store = AgentStore::default();
        store.upsert(agent);
        task::save(&home, &root, &store).unwrap();

        let owner = owner_of(
            &home,
            Claim {
                id: id.as_str(),
                key: &key,
                cwd: &nested,
            },
        )
        .expect("the claim is backed by its record");
        assert_eq!(owner.agent, id);
        assert!(!owner.isolated);
        assert_eq!(owner.project_root, root);
    }

    #[test]
    fn a_recycled_slots_new_task_finds_nothing_the_previous_one_left() {
        let home = home("conversation-recycled");
        let root = project("conversation-recycled-project");

        let previous = AgentId::generate();
        let mut record = ConversationRecord::new(previous.clone());
        record.launched(
            "claude-code",
            ConversationOrigin::Assigned,
            Some(SessionId::new("old")),
            None,
        );
        save(&home, &root, &record).unwrap();

        let recycled = AgentId::generate();
        assert!(load(&home, &root, &recycled).harnesses.is_empty());
        // And the previous task's own record is untouched by that.
        assert!(load(&home, &root, &previous).get("claude-code").is_some());
    }

    #[test]
    fn forgetting_a_task_takes_its_record_with_it() {
        let home = home("conversation-forget");
        let root = project("conversation-forget-project");
        let task = AgentId::generate();
        let mut record = ConversationRecord::new(task.clone());
        record.launched("codex", ConversationOrigin::Observed, None, None);
        save(&home, &root, &record).unwrap();

        forget(&home, &root, &task);

        assert!(!store_path(&home, &root, &task).exists());
        assert!(load(&home, &root, &task).harnesses.is_empty());
    }
}
