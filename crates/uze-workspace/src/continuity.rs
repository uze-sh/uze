//! Deciding, per launch, whether an agent resumes or starts.
//!
//! [`conversation`](crate::conversation) is the record; this is what is done
//! with it. Both halves are vendor-neutral: everything specific to a harness
//! arrives through [`IntegrationPort`]'s continuity verbs, so this file
//! names no harness and a fifth one changes nothing here.
//!
//! # Where this runs
//!
//! [`plan`] runs inside the launched process, on the shim — the only place
//! that is reached by *every* relaunch, including the one the terminal
//! runtime performs when it restores a workspace with no client in the room.
//! It therefore does no more than verifying the launch's claim against the
//! record it names — a few `stat`s and one small read — and, on a first
//! launch, one small write. It reads no environment: the shim decides what
//! the launch claims and hands it in.
//!
//! [`refresh`] is the other half, and runs where a background answer already
//! runs: it asks the harness which conversation the agent is actually in and
//! writes that back, so a conversation cleared, forked or switched inside
//! the process is the one that resumes rather than the one it began with.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use crate::{
    briefing,
    conversation::{self, Claim, ConversationOrigin, ConversationRecord, SessionId},
    home::UzeHome,
    integration::{IntegrationPort, ObservationContext, SessionContinuity},
};

/// What a launch should carry, and what could not be done.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LaunchPlan {
    /// Prepended before the caller's own argument list.
    pub args: Vec<OsString>,
    /// Appended after every other argument: the first turn a briefing
    /// asked for, which a harness takes positionally or behind a flag that
    /// has to follow its resume subcommand.
    pub trailing: Vec<OsString>,
    /// Said once, on the shim's existing note channel. Present only when
    /// something was expected to be carried over and could not be.
    pub note: Option<String>,
}

impl LaunchPlan {
    fn nothing() -> Self {
        Self::default()
    }

    fn args(args: Vec<OsString>) -> Self {
        Self {
            args,
            ..Self::default()
        }
    }
}

/// Resume this agent's conversation, or start one and record it.
///
/// `None` of the interesting cases is an error: a claim no record backs, a
/// harness that declares no continuity, a record that no longer resolves —
/// each answers with the launch that would have happened anyway.
/// Continuity is never a precondition for an agent starting.
pub fn plan(home: &UzeHome, claim: Claim<'_>, integration: &dyn IntegrationPort) -> LaunchPlan {
    let continuity = integration.session_continuity();
    // Not an agent UZE launched, or not where its record put it: an
    // ordinary invocation stays ordinary.
    let Some(owner) = conversation::owner_of(home, claim) else {
        tracing::info!(
            agent = claim.id,
            cwd = %claim.cwd.display(),
            "no record backs this launch's claim; launched as an ordinary invocation"
        );
        return LaunchPlan::nothing();
    };
    // A harness that carries no conversation over can still be told what
    // the one before it was doing.
    if continuity == SessionContinuity::Unsupported {
        let mut plan = LaunchPlan::nothing();
        plan.trailing = take_first_turn(home, &owner, integration);
        return plan;
    }
    let mut record = conversation::load(home, &owner.project_root, &owner.agent);
    let id = integration.id();

    // A launch whose read-back never happened — the client was not running,
    // or the agent was relaunched before the refresh came round — would
    // otherwise start a second conversation beside a perfectly good first
    // one. This is the one place the launch path pays for an observation,
    // and it pays for it only here: the entry is pending, so the
    // alternative is losing the conversation, and a harness that answers
    // this question expensively is answering it once per relaunch rather
    // than on a clock.
    resolve_pending(home, &owner, &mut record, claim.cwd, integration);

    if let Some(recorded) = record.get(id).and_then(|entry| entry.conversation.clone()) {
        if integration.session_exists(&recorded, claim.cwd) {
            tracing::info!(
                agent = %owner.agent.as_str(),
                harness = id,
                conversation = %recorded,
                cwd = %claim.cwd.display(),
                "resuming the recorded conversation"
            );
            let mut plan = LaunchPlan::args(integration.resume_session_args(&recorded));
            plan.trailing = take_first_turn(home, &owner, integration);
            return plan;
        }
        tracing::warn!(
            agent = %owner.agent.as_str(),
            harness = id,
            conversation = %recorded,
            cwd = %claim.cwd.display(),
            "the recorded conversation is no longer there; starting a new one"
        );
        let mut plan = start(home, claim.cwd, integration, &owner);
        plan.note = Some(format!(
            "the recorded conversation ({recorded}) is no longer there; starting a new one"
        ));
        return plan;
    }

    tracing::info!(
        agent = %owner.agent.as_str(),
        harness = id,
        cwd = %claim.cwd.display(),
        "no conversation recorded for this agent; starting one"
    );
    start(home, claim.cwd, integration, &owner)
}

/// Fills in the conversation `integration`'s entry is still waiting to have
/// read back, asking the harness now, and writes it to the stored record.
fn resolve_pending(
    home: &UzeHome,
    owner: &conversation::Owner,
    record: &mut ConversationRecord,
    cwd: &Path,
    integration: &dyn IntegrationPort,
) {
    let id = integration.id();
    let Some(entry) = record.get(id).filter(|entry| entry.conversation.is_none()) else {
        return;
    };
    let claimed = conversation::claimed_by_others(home, &owner.project_root, &owner.agent, id);
    let Some(observed) = integration.observe_session(&ObservationContext {
        cwd,
        since_unix: entry.launched_at_unix,
        preceded_by: entry.preceded_by.as_ref(),
        claimed_elsewhere: &claimed,
    }) else {
        return;
    };
    let launched_at = entry.launched_at_unix;
    // Applied to what is stored now rather than to what was read before
    // the observation: the launch it answers is named by its timestamp, so
    // a launch recorded since simply refuses it.
    if record.observed(id, launched_at, observed.clone()) {
        let _ = conversation::update(home, &owner.project_root, &owner.agent, |stored| {
            stored.observed(id, launched_at, observed)
        });
    }
}

/// The conversation `agent` is in on `integration`, resolving one whose
/// read-back has not happened yet. What a switch reads the outgoing
/// harness's transcript by: an agent switched away moments after it
/// started would otherwise be briefed from Git alone.
pub fn conversation_now(
    home: &UzeHome,
    owner: &conversation::Owner,
    cwd: &Path,
    integration: &dyn IntegrationPort,
) -> Option<SessionId> {
    if integration.session_continuity() == SessionContinuity::Unsupported {
        return None;
    }
    let mut record = conversation::load(home, &owner.project_root, &owner.agent);
    resolve_pending(home, owner, &mut record, cwd, integration);
    record.get(integration.id())?.conversation.clone()
}

/// Records a new launch, replacing whatever conversation the harness had
/// for this agent before.
fn start(
    home: &UzeHome,
    cwd: &Path,
    integration: &dyn IntegrationPort,
    owner: &conversation::Owner,
) -> LaunchPlan {
    let id = integration.id();
    let (plan, origin, session, preceded_by) = match integration.session_continuity() {
        // Named here, so nothing has to be read back and a crash between
        // this line and the harness's first write costs nothing.
        SessionContinuity::Assigned => {
            let session = integration.new_session_id();
            (
                LaunchPlan::args(integration.start_session_args(&session)),
                ConversationOrigin::Assigned,
                Some(session),
                None,
            )
        }
        // The harness names its own; all this launch can do is say when it
        // started and what was already there, so the read-back can tell one
        // from the other.
        SessionContinuity::Observed => (
            LaunchPlan::nothing(),
            ConversationOrigin::Observed,
            None,
            integration.session_recorded_for(cwd),
        ),
        SessionContinuity::Unsupported => return LaunchPlan::nothing(),
    };
    // A record that cannot be written is a conversation that will not be
    // carried over next time — never a launch that does not happen. The
    // briefing is taken in the same write, so a launch either records
    // itself and consumes it, or does neither.
    let briefing = conversation::update(home, &owner.project_root, &owner.agent, |stored| {
        stored.launched(id, origin, session, preceded_by);
        stored.take_briefing(id)
    })
    .ok()
    .flatten();
    LaunchPlan {
        trailing: first_turn_args(integration, briefing),
        ..plan
    }
}

/// The first turn of the briefing pending for this harness, consumed so no
/// later launch — a restore, a relaunch — is pointed at it again.
fn take_first_turn(
    home: &UzeHome,
    owner: &conversation::Owner,
    integration: &dyn IntegrationPort,
) -> Vec<OsString> {
    let briefing = conversation::update(home, &owner.project_root, &owner.agent, |stored| {
        stored.take_briefing(integration.id())
    })
    .ok()
    .flatten();
    first_turn_args(integration, briefing)
}

/// A briefing swept since it was written points at nothing, and a first
/// turn naming a missing file would send the agent looking for it.
fn first_turn_args(integration: &dyn IntegrationPort, briefing: Option<PathBuf>) -> Vec<OsString> {
    match briefing {
        Some(path) if path.is_file() => integration.first_turn_args(&briefing::first_turn(&path)),
        _ => Vec::new(),
    }
}

/// Writes back the conversation the agent is actually in.
///
/// Answers whether the record changed, so a caller can avoid a write it does
/// not need. Deliberately silent about everything else: a harness with
/// nothing new to report, a task that has moved on, an unreadable record —
/// all of them mean "nothing to do", and none of them is worth interrupting
/// anybody over.
pub fn refresh(home: &UzeHome, claim: Claim<'_>, integration: &dyn IntegrationPort) -> bool {
    if integration.session_continuity() == SessionContinuity::Unsupported {
        return false;
    }
    let Some(owner) = conversation::owner_of(home, claim) else {
        return false;
    };
    let record = conversation::load(home, &owner.project_root, &owner.agent);
    let id = integration.id();
    let Some(entry) = record.get(id) else {
        return false;
    };
    let launched_at = entry.launched_at_unix;
    let known = entry.conversation.clone();
    let claimed = conversation::claimed_by_others(home, &owner.project_root, &owner.agent, id);
    let observed = integration.observe_session(&ObservationContext {
        cwd: claim.cwd,
        since_unix: launched_at,
        preceded_by: entry.preceded_by.as_ref(),
        claimed_elsewhere: &claimed,
    });
    let Some(observed) = observed else {
        return false;
    };
    if known.as_ref() == Some(&observed) {
        return false;
    }
    conversation::update(home, &owner.project_root, &owner.agent, |stored| {
        stored.observed(id, launched_at, observed)
    })
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        checkout::CheckoutId,
        conversation::SessionId,
        integration::HarnessDetection,
        task::{self, Agent, AgentId, AgentStore, Base},
    };
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use uze_core::{Resource, exposure::ExposurePlan, router::HarnessCapabilities};

    /// A harness whose every continuity answer the test decides.
    struct Harness {
        id: &'static str,
        continuity: SessionContinuity,
        exists: bool,
        observed: std::sync::Mutex<Option<SessionId>>,
        recorded_for: Option<SessionId>,
    }

    impl Harness {
        fn new(continuity: SessionContinuity) -> Self {
            Self {
                id: "harness",
                continuity,
                exists: true,
                observed: std::sync::Mutex::new(None),
                recorded_for: None,
            }
        }
    }

    impl IntegrationPort for Harness {
        fn id(&self) -> &'static str {
            self.id
        }
        fn capabilities(&self) -> HarnessCapabilities {
            HarnessCapabilities::default()
        }
        fn exposure_plan(&self, _resource: &Resource) -> ExposurePlan {
            panic!("not used")
        }
        fn detect(&self) -> HarnessDetection {
            HarnessDetection::default()
        }
        fn session_continuity(&self) -> SessionContinuity {
            self.continuity
        }
        fn start_session_args(&self, session: &SessionId) -> Vec<OsString> {
            vec![OsString::from("--start"), OsString::from(session.as_str())]
        }
        fn resume_session_args(&self, session: &SessionId) -> Vec<OsString> {
            vec![OsString::from("--resume"), OsString::from(session.as_str())]
        }
        fn session_recorded_for(&self, _cwd: &Path) -> Option<SessionId> {
            self.recorded_for.clone()
        }
        fn observe_session(&self, ctx: &ObservationContext) -> Option<SessionId> {
            self.observed
                .lock()
                .unwrap()
                .clone()
                .filter(|session| ctx.is_unclaimed(session))
        }
        fn session_exists(&self, _session: &SessionId, _cwd: &Path) -> bool {
            self.exists
        }
        fn first_turn_args(&self, turn: &str) -> Vec<OsString> {
            vec![OsString::from("--turn"), OsString::from(turn)]
        }
    }

    /// A repository with one task in one slot: the home, the primary, the
    /// slot's path and the task's identifier — what a launch there claims.
    fn managed(label: &str) -> (UzeHome, PathBuf, PathBuf, String) {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = uze_testkit::temp::scratch(&format!("{label}-{unique}"));
        let home = UzeHome::at(root.join("uze"));
        let primary = root.join("repo");
        let slot = primary.join(".worktrees").join("slot-1");
        std::fs::create_dir_all(&slot).unwrap();

        let mut task = Agent::isolated(
            "claude",
            None,
            Base::Ref("main".into()),
            String::new(),
            "main".into(),
        );
        task.isolation_mut().unwrap().checkout = Some(CheckoutId::adopted("slot-1"));
        launched_with_test_key(&mut task);
        let id = task.id.as_str().to_owned();
        let mut store = AgentStore::default();
        store.upsert(task);
        task::save(&home, &primary, &store).unwrap();
        (home, primary, slot, id)
    }

    /// The key every agent these tests record was issued.
    const TEST_KEY: &str = "the-launch-key";

    fn launched_with_test_key(agent: &mut Agent) {
        agent.launch_key = Some(uze_core::digest::secret_sha256(TEST_KEY));
    }

    fn claim<'a>(id: &'a str, cwd: &'a Path) -> Claim<'a> {
        Claim {
            id,
            key: TEST_KEY,
            cwd,
        }
    }

    fn recorded(home: &UzeHome, primary: &Path) -> Option<SessionId> {
        let store = task::load(home, primary).unwrap();
        let task = store.agents.first()?;
        conversation::load(home, primary, &task.id)
            .get("harness")?
            .conversation
            .clone()
    }

    /// Writes a briefing for `integration` and records it as pending on
    /// the managed agent, the way a switch leaves it.
    fn briefed(home: &UzeHome, primary: &Path, integration: &str) -> PathBuf {
        let store = task::load(home, primary).unwrap();
        let agent = store.agents[0].id.clone();
        let path = home.briefing_path("test", agent.as_str());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "what happened").unwrap();
        conversation::update(home, primary, &agent, |stored| {
            stored.briefing = Some(conversation::PendingBriefing {
                integration: integration.to_owned(),
                path: path.clone(),
            });
        })
        .unwrap();
        path
    }

    fn pending(home: &UzeHome, primary: &Path) -> Option<conversation::PendingBriefing> {
        let store = task::load(home, primary).unwrap();
        conversation::load(home, primary, &store.agents[0].id).briefing
    }

    fn turn_for(briefing: &Path) -> Vec<OsString> {
        vec![
            OsString::from("--turn"),
            OsString::from(briefing::first_turn(briefing)),
        ]
    }

    #[test]
    fn a_pending_briefing_is_given_to_the_first_launch_and_to_no_other() {
        let (home, primary, slot, id) = managed("continuity-briefing-once");
        let harness = Harness::new(SessionContinuity::Assigned);
        let briefing = briefed(&home, &primary, "harness");

        let first = plan(&home, claim(&id, &slot), &harness);
        assert_eq!(first.trailing, turn_for(&briefing));
        assert!(recorded(&home, &primary).is_some());
        assert_eq!(pending(&home, &primary), None);

        // What a restore does: the same launch again, which resumes and is
        // pointed at nothing.
        let restored = plan(&home, claim(&id, &slot), &harness);
        assert!(restored.args.contains(&OsString::from("--resume")));
        assert!(restored.trailing.is_empty());
    }

    #[test]
    fn a_resumed_harness_is_given_its_briefing_behind_the_resume() {
        let (home, primary, slot, id) = managed("continuity-briefing-resume");
        let harness = Harness::new(SessionContinuity::Assigned);
        plan(&home, claim(&id, &slot), &harness);
        let briefing = briefed(&home, &primary, "harness");

        let back = plan(&home, claim(&id, &slot), &harness);
        assert!(back.args.contains(&OsString::from("--resume")));
        assert_eq!(back.trailing, turn_for(&briefing));
        assert_eq!(pending(&home, &primary), None);
    }

    #[test]
    fn a_briefing_for_another_harness_stays_pending() {
        let (home, primary, slot, id) = managed("continuity-briefing-other");
        briefed(&home, &primary, "elsewhere");

        let launch = plan(
            &home,
            claim(&id, &slot),
            &Harness::new(SessionContinuity::Assigned),
        );
        assert!(launch.trailing.is_empty());
        assert_eq!(
            pending(&home, &primary).map(|pending| pending.integration),
            Some("elsewhere".to_owned())
        );
    }

    #[test]
    fn a_harness_without_continuity_is_still_given_its_briefing() {
        let (home, primary, slot, id) = managed("continuity-briefing-unsupported");
        let mut harness = Harness::new(SessionContinuity::Unsupported);
        harness.id = "plain";
        let briefing = briefed(&home, &primary, "plain");

        let launch = plan(&home, claim(&id, &slot), &harness);
        assert!(launch.args.is_empty());
        assert_eq!(launch.trailing, turn_for(&briefing));
    }

    #[test]
    fn a_briefing_swept_before_its_launch_points_at_nothing() {
        let (home, primary, slot, id) = managed("continuity-briefing-swept");
        let briefing = briefed(&home, &primary, "harness");
        std::fs::remove_file(&briefing).unwrap();

        let launch = plan(
            &home,
            claim(&id, &slot),
            &Harness::new(SessionContinuity::Assigned),
        );
        assert!(launch.trailing.is_empty());
        assert_eq!(pending(&home, &primary), None);
    }

    #[test]
    fn a_first_launch_of_an_assigning_harness_names_and_records_the_conversation() {
        let (home, primary, slot, id) = managed("continuity-assign");
        let plan = plan(
            &home,
            claim(&id, &slot),
            &Harness::new(SessionContinuity::Assigned),
        );

        let session = recorded(&home, &primary).expect("the conversation was recorded");
        assert_eq!(
            plan.args,
            vec![OsString::from("--start"), OsString::from(session.as_str())]
        );
        assert_eq!(plan.note, None);
    }

    #[test]
    fn a_second_launch_resumes_what_the_first_recorded() {
        let (home, primary, slot, id) = managed("continuity-resume");
        let harness = Harness::new(SessionContinuity::Assigned);
        plan(&home, claim(&id, &slot), &harness);
        let session = recorded(&home, &primary).unwrap();

        let plan = plan(&home, claim(&id, &slot), &harness);
        assert_eq!(
            plan.args,
            vec![OsString::from("--resume"), OsString::from(session.as_str())]
        );
    }

    #[test]
    fn a_first_launch_of_an_observing_harness_carries_no_argument_but_is_recorded() {
        let (home, primary, slot, id) = managed("continuity-observe");
        let mut harness = Harness::new(SessionContinuity::Observed);
        harness.recorded_for = Some(SessionId::new("previous-tenant"));

        assert_eq!(
            plan(&home, claim(&id, &slot), &harness),
            LaunchPlan::nothing()
        );

        let store = task::load(&home, &primary).unwrap();
        let entry = conversation::load(&home, &primary, &store.agents[0].id)
            .get("harness")
            .cloned()
            .expect("the launch was recorded");
        assert_eq!(entry.conversation, None);
        assert_eq!(entry.origin, ConversationOrigin::Observed);
        assert_eq!(entry.preceded_by, Some(SessionId::new("previous-tenant")));
    }

    /// Without this, a harness that names its own conversation would start
    /// a second one every time it was relaunched with no client watching —
    /// which is exactly the relaunch that matters.
    #[test]
    fn a_launch_resolves_a_read_back_nobody_else_did() {
        let (home, primary, slot, id) = managed("continuity-pending");
        let harness = Harness::new(SessionContinuity::Observed);
        plan(&home, claim(&id, &slot), &harness);
        assert_eq!(recorded(&home, &primary), None, "still pending");

        *harness.observed.lock().unwrap() = Some(SessionId::new("named-by-the-harness"));
        let plan = plan(&home, claim(&id, &slot), &harness);

        assert_eq!(
            plan.args,
            vec![
                OsString::from("--resume"),
                OsString::from("named-by-the-harness")
            ]
        );
        assert_eq!(
            recorded(&home, &primary),
            Some(SessionId::new("named-by-the-harness"))
        );
    }

    #[test]
    fn a_directory_no_task_owns_is_left_exactly_as_it_was() {
        let (home, _primary, _slot, id) = managed("continuity-unmanaged");
        let elsewhere = uze_testkit::temp::scratch("continuity-elsewhere");
        assert_eq!(
            plan(
                &home,
                claim(&id, &elsewhere),
                &Harness::new(SessionContinuity::Assigned)
            ),
            LaunchPlan::nothing()
        );
    }

    #[test]
    fn an_identifier_no_record_names_is_left_exactly_as_it_was() {
        let (home, _primary, slot, _id) = managed("continuity-unknown-id");
        assert_eq!(
            plan(
                &home,
                claim("nobody-recorded-this", &slot),
                &Harness::new(SessionContinuity::Assigned)
            ),
            LaunchPlan::nothing()
        );
    }

    /// Two records over one directory are told apart by the identifier
    /// alone: each launch resumes its own conversation and never the other's.
    #[test]
    fn two_agents_in_one_directory_keep_their_own_conversations() {
        let (home, primary, slot, first) = managed("continuity-shared-directory");
        let mut second = Agent::isolated(
            "claude",
            None,
            Base::Ref("main".into()),
            String::new(),
            "main".into(),
        );
        second.isolation_mut().unwrap().checkout = Some(CheckoutId::adopted("slot-1"));
        launched_with_test_key(&mut second);
        let second_id = second.id.as_str().to_owned();
        let mut store = task::load(&home, &primary).unwrap();
        store.upsert(second);
        task::save(&home, &primary, &store).unwrap();
        let harness = Harness::new(SessionContinuity::Assigned);

        let started_first = plan(&home, claim(&first, &slot), &harness).args;
        let started_second = plan(&home, claim(&second_id, &slot), &harness).args;
        assert_ne!(
            started_first, started_second,
            "each launch names its own conversation"
        );

        let resumed_first = plan(&home, claim(&first, &slot), &harness).args;
        assert_eq!(
            resumed_first[1], started_first[1],
            "the first resumes its own"
        );
        let resumed_second = plan(&home, claim(&second_id, &slot), &harness).args;
        assert_eq!(
            resumed_second[1], started_second[1],
            "the second resumes its own"
        );
    }

    #[test]
    fn a_harness_that_declares_no_continuity_contributes_nothing_and_records_nothing() {
        let (home, primary, slot, id) = managed("continuity-unsupported");
        assert_eq!(
            plan(
                &home,
                claim(&id, &slot),
                &Harness::new(SessionContinuity::Unsupported)
            ),
            LaunchPlan::nothing()
        );
        assert_eq!(recorded(&home, &primary), None);
    }

    #[test]
    fn a_conversation_the_harness_no_longer_holds_starts_a_new_one_and_says_so() {
        let (home, primary, slot, id) = managed("continuity-vanished");
        let mut harness = Harness::new(SessionContinuity::Assigned);
        plan(&home, claim(&id, &slot), &harness);
        let gone = recorded(&home, &primary).unwrap();

        harness.exists = false;
        let plan = plan(&home, claim(&id, &slot), &harness);

        let replacement = recorded(&home, &primary).unwrap();
        assert_ne!(replacement, gone);
        assert_eq!(
            plan.args,
            vec![
                OsString::from("--start"),
                OsString::from(replacement.as_str())
            ]
        );
        assert!(plan.note.is_some_and(|note| note.contains(gone.as_str())));
    }

    #[test]
    fn unreadable_state_still_launches_the_agent() {
        let (home, primary, slot, id) = managed("continuity-unreadable");
        let store = task::load(&home, &primary).unwrap();
        let path = conversation::store_path(&home, &primary, &store.agents[0].id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not json at all").unwrap();

        let plan = plan(
            &home,
            claim(&id, &slot),
            &Harness::new(SessionContinuity::Assigned),
        );
        assert!(!plan.args.is_empty(), "the agent still starts");
    }

    #[test]
    fn a_read_back_records_where_the_agent_actually_is() {
        let (home, primary, slot, id) = managed("continuity-refresh");
        let harness = Harness::new(SessionContinuity::Observed);
        plan(&home, claim(&id, &slot), &harness);

        *harness.observed.lock().unwrap() = Some(SessionId::new("started-by-the-harness"));
        assert!(refresh(&home, claim(&id, &slot), &harness));
        assert_eq!(
            recorded(&home, &primary),
            Some(SessionId::new("started-by-the-harness"))
        );

        // Nothing new to say is not a write.
        assert!(!refresh(&home, claim(&id, &slot), &harness));
    }

    /// The clear/fork case: the agent left the conversation it started in,
    /// and what resumes has to be where the work actually went.
    #[test]
    fn a_conversation_the_agent_moved_to_replaces_the_one_it_started_in() {
        let (home, primary, slot, id) = managed("continuity-moved");
        let harness = Harness::new(SessionContinuity::Assigned);
        plan(&home, claim(&id, &slot), &harness);
        let started_in = recorded(&home, &primary).unwrap();

        *harness.observed.lock().unwrap() = Some(SessionId::new("moved-to"));
        assert!(refresh(&home, claim(&id, &slot), &harness));

        let plan = plan(&home, claim(&id, &slot), &harness);
        assert_eq!(
            plan.args,
            vec![OsString::from("--resume"), OsString::from("moved-to")]
        );
        assert_ne!(recorded(&home, &primary), Some(started_in));
    }

    /// Agents sharing a directory each see the others' conversations as
    /// the newest one there. Adopting it is how several agents came back
    /// from a restart as one.
    #[test]
    fn a_conversation_another_agent_holds_is_never_taken_over() {
        let (home, primary, slot, id) = managed("continuity-claimed-elsewhere");
        let harness = Harness::new(SessionContinuity::Assigned);
        plan(&home, claim(&id, &slot), &harness);
        let own = recorded(&home, &primary).unwrap();

        let mut neighbour = conversation::ConversationRecord::new(AgentId::generate());
        neighbour.launched(
            "harness",
            ConversationOrigin::Assigned,
            Some(SessionId::new("neighbour")),
            None,
        );
        conversation::save(&home, &primary, &neighbour).unwrap();

        *harness.observed.lock().unwrap() = Some(SessionId::new("neighbour"));
        assert!(!refresh(&home, claim(&id, &slot), &harness));
        assert_eq!(recorded(&home, &primary), Some(own));
    }
}
