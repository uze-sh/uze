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
//! record it names, reading `agents.yaml` and the checkout's `HEAD` file for
//! the agent's instructions, and writing its context: small reads and
//! writes, and no Git process unless a channel created `.worktrees/` this
//! launch. It reads no environment: the shim decides what
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
    agent_context::{self, Section, SectionAsk},
    briefing, checkout,
    conversation::{self, Claim, ConversationOrigin, ConversationRecord, SessionId},
    declaration,
    home::UzeHome,
    integration::{AgentContext, IntegrationPort, ObservationContext, SessionContinuity},
    worktree::{self, Placement},
};

/// What a launch should carry, and what could not be done.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LaunchPlan {
    /// Prepended before the caller's own argument list: the resume, then
    /// what the agent context's channel adds.
    pub args: Vec<OsString>,
    /// Set on the harness on top of what it inherits: what the agent
    /// context's channel needs, never the context's text.
    pub env: Vec<(OsString, OsString)>,
    /// Appended after every other argument: the first turn a briefing
    /// asked for on a harness with no invisible channel, which it takes
    /// positionally or behind a flag that has to follow its resume
    /// subcommand.
    pub trailing: Vec<OsString>,
    /// Said once, on the shim's existing note channel. Present only when
    /// something was expected to be carried over and could not be; several
    /// such things are all said, in the order they were found.
    pub note: Option<String>,
    /// Whether this launch is one UZE owns: an agent its record backs, in
    /// the directory the record puts it. Only such a launch is changed.
    pub owned: bool,
}

impl LaunchPlan {
    fn nothing() -> Self {
        Self::default()
    }

    /// Adds `note` to what this launch says, never in place of it: the
    /// operator hears each thing that did not carry over.
    fn add_note(&mut self, note: String) {
        self.note = Some(match self.note.take() {
            Some(earlier) => format!("{earlier}; {note}"),
            None => note,
        });
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
    plan_with(home, claim, integration, &|_| Vec::new())
}

/// [`plan`], with the sections a caller adds to the agent's context beside
/// its briefing, asked with the room the briefing leaves once the launch is
/// known to be UZE's. The workspace composes sections and knows nothing
/// of what they hold.
pub fn plan_with(
    home: &UzeHome,
    claim: Claim<'_>,
    integration: &dyn IntegrationPort,
    sections: &dyn Fn(&SectionAsk<'_>) -> Vec<Section>,
) -> LaunchPlan {
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
    let mut plan = plan_for(home, claim, integration, &owner, sections);
    plan.owned = true;
    plan
}

fn plan_for(
    home: &UzeHome,
    claim: Claim<'_>,
    integration: &dyn IntegrationPort,
    owner: &conversation::Owner,
    sections: &dyn Fn(&SectionAsk<'_>) -> Vec<Section>,
) -> LaunchPlan {
    let continuity = integration.session_continuity();
    // A harness that carries no conversation over can still be told what
    // the one before it was doing.
    if continuity == SessionContinuity::Unsupported {
        let mut plan = LaunchPlan::nothing();
        give_context(
            &mut plan,
            home,
            owner,
            claim.cwd,
            integration,
            None,
            Conversation::New,
            sections,
        );
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
    resolve_pending(home, owner, &mut record, claim.cwd, integration);

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
            give_context(
                &mut plan,
                home,
                owner,
                claim.cwd,
                integration,
                Some(&recorded),
                Conversation::Resumed,
                sections,
            );
            return plan;
        }
        tracing::warn!(
            agent = %owner.agent.as_str(),
            harness = id,
            conversation = %recorded,
            cwd = %claim.cwd.display(),
            "the recorded conversation is no longer there; starting a new one"
        );
        let mut plan = start(home, claim.cwd, integration, owner, sections);
        plan.add_note(format!(
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
    start(home, claim.cwd, integration, owner, sections)
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
    sections: &dyn Fn(&SectionAsk<'_>) -> Vec<Section>,
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
    // carried over next time — never a launch that does not happen.
    let _ = conversation::update(home, &owner.project_root, &owner.agent, |stored| {
        stored.launched(id, origin, session.clone(), preceded_by);
    });
    let mut plan = plan;
    give_context(
        &mut plan,
        home,
        owner,
        cwd,
        integration,
        session.as_ref(),
        Conversation::New,
        sections,
    );
    plan
}

/// Whether a launch starts a conversation or carries one on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Conversation {
    New,
    Resumed,
}

/// Gives this launch the agent's context through the harness's channel, or,
/// on a harness that has none (or whose channel cannot run here), points a
/// new conversation at it with one first turn.
///
/// Never fails the launch: a context that cannot be written or delivered
/// costs this launch its context and says so on the note channel.
#[allow(clippy::too_many_arguments)]
fn give_context(
    plan: &mut LaunchPlan,
    home: &UzeHome,
    owner: &conversation::Owner,
    cwd: &Path,
    integration: &dyn IntegrationPort,
    session: Option<&SessionId>,
    conversation: Conversation,
    extra: &dyn Fn(&SectionAsk<'_>) -> Vec<Section>,
) {
    let id = integration.id();
    let agent = owner.agent.as_str();
    let (instructions, unread) = launch_instructions(home, owner, cwd);
    if let Some(note) = unread {
        tracing::warn!(agent, harness = id, %note, "launch instructions from the default policy");
        plan.add_note(note);
    }
    if integration.context_channel().is_degraded()
        || integration.context_channel_unavailable().is_some()
    {
        plan.trailing = first_turn(home, owner, integration, &instructions, conversation);
        return;
    }
    let record = conversation::load(home, &owner.project_root, &owner.agent);
    let mut sections: Vec<Section> = record
        .briefing_for(id)
        .map(|briefing| Section {
            heading: briefing::HEADING,
            text: briefing.text.clone(),
            share: agent_context::BRIEFING_SHARE,
        })
        .into_iter()
        .collect();
    let room = agent_context::room_after(&instructions, &sections);
    sections.extend(extra(&SectionAsk {
        project_root: &owner.project_root,
        checkout: cwd,
        room,
    }));
    let Some(text) = agent_context::compose(&instructions, &sections) else {
        // A context an earlier launch was given must not outlive what it
        // said: a retired briefing is no longer the agent's to be told.
        let _ = std::fs::remove_file(
            agent_context::dir(home, &owner.project_root, agent).join(agent_context::FILE_NAME),
        );
        integration.withdraw_context(home, session);
        return;
    };
    let opening = agent_context::instructions_part(&instructions);
    let notes = agent_context::notes(&sections);
    let checkout = checkout_root(&owner.project_root, cwd);
    let link = context_link(&checkout, agent);
    let isolation_directory_existed = link
        .ancestors()
        .find(|ancestor| ancestor.ends_with(worktree::WORKTREES_DIRECTORY))
        .is_none_or(Path::exists);
    let delivered =
        agent_context::write(home, &owner.project_root, agent, &text).and_then(|file| {
            let dir = agent_context::dir(home, &owner.project_root, agent);
            let launch = SessionId::generate();
            integration.deliver_context(
                home,
                &AgentContext {
                    dir: &dir,
                    file: &file,
                    text: &text,
                    instructions: &opening,
                    notes: notes.as_deref(),
                    checkout: &checkout,
                    link: &link,
                    launch: launch.as_str(),
                    session,
                },
            )
        });
    // A channel that linked its context into a `.worktrees/` this launch
    // created must not leave it showing in the operator's status. Asked only
    // then: excluding it costs a Git call, and the launch path runs on every
    // relaunch.
    if !isolation_directory_existed && link.symlink_metadata().is_ok() {
        let _ = checkout::exclude_isolation_directory(&owner.project_root);
    }
    match delivered {
        Ok(contribution) => {
            plan.args.extend(contribution.args);
            plan.env.extend(contribution.env);
        }
        Err(error) => {
            tracing::warn!(agent, harness = id, %error, "agent context not delivered");
            plan.add_note(format!("this agent's context was not delivered ({error})"));
        }
    }
}

/// The root of the checkout the agent was launched in, spelled the way the
/// launch's own `cwd` spells it: a channel comparing paths as strings must
/// not meet `/private/tmp/x` beside `/tmp/x`. An isolated checkout is read
/// from `cwd` itself; for the primary, the ancestor of `cwd` that is the
/// project root, falling back to the root as recorded.
fn checkout_root(project_root: &Path, cwd: &Path) -> PathBuf {
    if let Some(isolated) = worktree::isolated_checkout(cwd) {
        return isolated.directory();
    }
    // The platform's rule first, which asks the disk nothing; links are
    // resolved only when no ancestor is spelled as the root is, which is the
    // case of a launch through a link to the tree.
    if let Some(ancestor) = cwd
        .ancestors()
        .find(|ancestor| uze_platform::path::same_path(ancestor, project_root))
    {
        return ancestor.to_path_buf();
    }
    let Ok(recorded) = uze_platform::path::canonical(project_root) else {
        return project_root.to_path_buf();
    };
    cwd.ancestors()
        .find(|ancestor| {
            uze_platform::path::canonical(ancestor)
                .is_ok_and(|resolved| uze_platform::path::same_path(&resolved, &recorded))
        })
        .map_or_else(|| project_root.to_path_buf(), Path::to_path_buf)
}

/// Where a channel may link the agent's context so that its path sorts
/// after the checkout root's: beside an isolated checkout as `<id>~uze`, or
/// under `.worktrees/~uze/` for an agent in the primary. Both sit in the
/// directory UZE already owns. A slot's link sorts after the slot itself and
/// everything inside it, since `/` sorts below `~`; the primary's sorts
/// after the root, which is its prefix, but not after every directory under
/// it, which is why a channel names the root too.
fn context_link(checkout: &Path, agent: &str) -> PathBuf {
    match worktree::isolated_checkout(checkout) {
        Some(isolated) => isolated
            .primary
            .join(worktree::WORKTREES_DIRECTORY)
            .join(format!("{}~uze", isolated.name)),
        None => checkout
            .join(worktree::WORKTREES_DIRECTORY)
            .join("~uze")
            .join(agent),
    }
}

/// The branch the agent's record names, which naming the work keeps in
/// step with the one it renamed.
fn recorded_branch(home: &UzeHome, owner: &conversation::Owner) -> Option<String> {
    let store = crate::task::load(home, &owner.project_root).ok()?;
    let branch = &store.get(&owner.agent)?.isolation()?.branch;
    (!branch.is_empty()).then(|| branch.clone())
}

/// The workspace's instructions for an agent launched at `checkout`, from
/// the policy in force and where the agent stands now. A declaration that
/// cannot be read gives the default policy's, and says so on the launch's
/// note: an agent is never launched without being told how its work is
/// delivered, and never told the default in silence when the project
/// declared otherwise.
fn launch_instructions(
    home: &UzeHome,
    owner: &conversation::Owner,
    cwd: &Path,
) -> (String, Option<String>) {
    let (policy, unread) = declaration::for_instructions(&owner.project_root);
    let note = unread.map(|reason| {
        format!("agents.yaml's workspace section does not read ({reason}); this agent was told the default policy")
    });
    let instructions = match worktree::isolated_checkout(cwd) {
        Some(isolated) => {
            // `/` whatever the platform: what the agent reads must match the
            // `.worktrees/<id>` spelling everything else uses.
            let shown = format!("{}/{}", worktree::WORKTREES_DIRECTORY, isolated.name);
            // Read now rather than taken from the slot's name: naming the
            // work renames the branch, and a resume must say the new one.
            // The record is asked only when the file cannot answer (a
            // reftable repository): naming the work updates both.
            let branch = checkout::branch_on_disk(&isolated.directory())
                .or_else(|| recorded_branch(home, owner))
                .unwrap_or_else(|| format!("{}{}", worktree::BRANCH_PREFIX, isolated.name));
            policy.instructions(&Placement::Isolated {
                checkout: &shown,
                branch: &branch,
            })
        }
        None => policy.instructions(&Placement::InPlace),
    };
    (instructions, note)
}

/// The one visible first turn a harness with no invisible channel gets,
/// pointing at the agent's context written beside it: on a new
/// conversation always, since the instructions reach it no other way, and
/// on a resume only while a briefing waits to be given, once.
fn first_turn(
    home: &UzeHome,
    owner: &conversation::Owner,
    integration: &dyn IntegrationPort,
    instructions: &str,
    conversation: Conversation,
) -> Vec<OsString> {
    let briefing = conversation::update(home, &owner.project_root, &owner.agent, |stored| {
        stored.take_first_turn(integration.id())
    })
    .ok()
    .flatten();
    if briefing.is_none() && conversation == Conversation::Resumed {
        return Vec::new();
    }
    let sections: Vec<Section> = briefing
        .iter()
        .map(|text| Section {
            heading: briefing::HEADING,
            text: text.clone(),
            share: agent_context::BRIEFING_SHARE,
        })
        .collect();
    let Some(text) = agent_context::compose(instructions, &sections) else {
        return Vec::new();
    };
    match agent_context::write(home, &owner.project_root, owner.agent.as_str(), &text) {
        Ok(path) if briefing.is_some() => integration.first_turn_args(&briefing::first_turn(&path)),
        Ok(path) => integration.first_turn_args(&briefing::instructions_turn(&path)),
        Err(_) => Vec::new(),
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
    let Some(owner) = conversation::owner_of(home, claim) else {
        return false;
    };
    let retired = retire_briefing(home, &owner, claim.cwd);
    if integration.session_continuity() == SessionContinuity::Unsupported {
        return retired;
    }
    let record = conversation::load(home, &owner.project_root, &owner.agent);
    let id = integration.id();
    let Some(entry) = record.get(id) else {
        return retired;
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
        return retired;
    };
    if known.as_ref() == Some(&observed) {
        return retired;
    }
    conversation::update(home, &owner.project_root, &owner.agent, |stored| {
        stored.observed(id, launched_at, observed)
    })
    .unwrap_or(false)
        || retired
}

/// Retires the agent's briefing once its checkout's commit moved past the
/// one it was switched at: the work has moved on, and the conversation now
/// holds what the briefing said. Costs one Git read, and only while a
/// briefing is pending.
fn retire_briefing(home: &UzeHome, owner: &conversation::Owner, cwd: &Path) -> bool {
    let record = conversation::load(home, &owner.project_root, &owner.agent);
    if record
        .briefing
        .as_ref()
        .and_then(|briefing| briefing.head_at_switch.as_ref())
        .is_none()
    {
        return false;
    }
    let Some(head) = crate::git::read(cwd, &["rev-parse", "HEAD"])
        .ok()
        .filter(uze_git::Output::is_success)
        .map(|output| output.stdout.trim().to_owned())
    else {
        return false;
    };
    conversation::update(home, &owner.project_root, &owner.agent, |stored| {
        stored.retire_briefing_past(&head)
    })
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        checkout::CheckoutId,
        conversation::SessionId,
        integration::{ContextChannel, ContextContribution, HarnessDetection},
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
        channel: ContextChannel,
        unavailable: Option<String>,
    }

    impl Harness {
        fn new(continuity: SessionContinuity) -> Self {
            Self {
                id: "harness",
                continuity,
                exists: true,
                observed: std::sync::Mutex::new(None),
                recorded_for: None,
                channel: ContextChannel::FirstTurn,
                unavailable: None,
            }
        }

        fn with_channel(continuity: SessionContinuity) -> Self {
            Self {
                channel: ContextChannel::Directory,
                ..Self::new(continuity)
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
        fn context_channel(&self) -> ContextChannel {
            self.channel
        }
        fn context_channel_unavailable(&self) -> Option<String> {
            self.unavailable.clone()
        }
        fn deliver_context(
            &self,
            _home: &UzeHome,
            ctx: &AgentContext,
        ) -> uze_core::Result<ContextContribution> {
            Ok(ContextContribution {
                args: vec![OsString::from("--context"), ctx.file.as_os_str().to_owned()],
                env: vec![(OsString::from("CONTEXT_FLAG"), OsString::from("1"))],
            })
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

    const BRIEFING_TEXT: &str = "what happened";

    /// Records a briefing for `integration` on the managed agent, the way a
    /// switch leaves it.
    fn briefed(home: &UzeHome, primary: &Path, integration: &str) {
        let store = task::load(home, primary).unwrap();
        let agent = store.agents[0].id.clone();
        conversation::update(home, primary, &agent, |stored| {
            stored.briefing = Some(conversation::Briefing {
                integration: integration.to_owned(),
                text: BRIEFING_TEXT.to_owned(),
                head_at_switch: None,
                first_turn_given: false,
            });
        })
        .unwrap();
    }

    fn pending(home: &UzeHome, primary: &Path) -> Option<conversation::Briefing> {
        let store = task::load(home, primary).unwrap();
        conversation::load(home, primary, &store.agents[0].id).briefing
    }

    /// Where the agent's context is written, beside its other files.
    fn context_file(home: &UzeHome, primary: &Path) -> PathBuf {
        let store = task::load(home, primary).unwrap();
        agent_context::dir(home, primary, store.agents[0].id.as_str())
            .join(agent_context::FILE_NAME)
    }

    /// The first turn a degraded harness is given while a briefing waits,
    /// pointing at the context that carries it.
    fn turn_for(home: &UzeHome, primary: &Path) -> Vec<OsString> {
        let path = context_file(home, primary);
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains(BRIEFING_TEXT)
        );
        vec![
            OsString::from("--turn"),
            OsString::from(briefing::first_turn(&path)),
        ]
    }

    /// The first turn a degraded harness is given on a new conversation with
    /// no briefing waiting.
    fn instructions_turn_for(home: &UzeHome, primary: &Path) -> Vec<OsString> {
        let path = context_file(home, primary);
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("Concurrent work isolation")
        );
        vec![
            OsString::from("--turn"),
            OsString::from(briefing::instructions_turn(&path)),
        ]
    }

    fn mentions_the_briefing(plan: &LaunchPlan) -> bool {
        plan.args
            .iter()
            .chain(&plan.trailing)
            .chain(plan.env.iter().map(|(_, value)| value))
            .any(|word| word.to_string_lossy().contains(BRIEFING_TEXT))
    }

    #[test]
    fn a_degraded_harness_is_pointed_at_its_briefing_by_the_first_launch_only() {
        let (home, primary, slot, id) = managed("continuity-briefing-once");
        let harness = Harness::new(SessionContinuity::Assigned);
        briefed(&home, &primary, "harness");

        let first = plan(&home, claim(&id, &slot), &harness);
        assert_eq!(first.trailing, turn_for(&home, &primary));
        assert!(recorded(&home, &primary).is_some());
        assert!(pending(&home, &primary).is_some_and(|b| b.first_turn_given));

        // What a restore does: the same launch again, which resumes and is
        // pointed at nothing.
        let restored = plan(&home, claim(&id, &slot), &harness);
        assert!(restored.args.contains(&OsString::from("--resume")));
        assert!(restored.trailing.is_empty());
    }

    #[test]
    fn a_resumed_degraded_harness_is_given_its_turn_behind_the_resume() {
        let (home, primary, slot, id) = managed("continuity-briefing-resume");
        let harness = Harness::new(SessionContinuity::Assigned);
        plan(&home, claim(&id, &slot), &harness);
        briefed(&home, &primary, "harness");

        let back = plan(&home, claim(&id, &slot), &harness);
        assert!(back.args.contains(&OsString::from("--resume")));
        assert_eq!(back.trailing, turn_for(&home, &primary));
    }

    #[test]
    fn a_briefing_for_another_harness_is_neither_given_nor_changed() {
        let (home, primary, slot, id) = managed("continuity-briefing-other");
        briefed(&home, &primary, "elsewhere");

        for harness in [
            Harness::new(SessionContinuity::Assigned),
            Harness::with_channel(SessionContinuity::Assigned),
        ] {
            let launch = plan(&home, claim(&id, &slot), &harness);
            assert!(!mentions_the_briefing(&launch));
            let context = std::fs::read_to_string(context_file(&home, &primary)).unwrap();
            assert!(!context.contains(BRIEFING_TEXT), "{context}");
        }
        let kept = pending(&home, &primary).unwrap();
        assert_eq!(kept.integration, "elsewhere");
        assert!(!kept.first_turn_given);
    }

    #[test]
    fn a_harness_without_continuity_is_still_given_its_briefing() {
        let (home, primary, slot, id) = managed("continuity-briefing-unsupported");
        let mut harness = Harness::new(SessionContinuity::Unsupported);
        harness.id = "plain";
        briefed(&home, &primary, "plain");

        let launch = plan(&home, claim(&id, &slot), &harness);
        assert!(launch.args.is_empty());
        assert_eq!(launch.trailing, turn_for(&home, &primary));
    }

    #[test]
    fn a_channel_gives_the_briefing_on_every_launch_and_never_in_argv() {
        let (home, primary, slot, id) = managed("continuity-context-every-launch");
        let harness = Harness::with_channel(SessionContinuity::Assigned);
        briefed(&home, &primary, "harness");

        for _ in 0..2 {
            let launch = plan(&home, claim(&id, &slot), &harness);
            assert!(
                launch.trailing.is_empty(),
                "no turn the operator did not type"
            );
            let at = launch
                .args
                .iter()
                .position(|word| word == "--context")
                .expect("the channel's arguments are carried");
            let file = PathBuf::from(&launch.args[at + 1]);
            let context = std::fs::read_to_string(&file).unwrap();
            assert!(context.starts_with(agent_context::INSTRUCTIONS_LEAD));
            assert!(context.contains(agent_context::PREAMBLE));
            assert!(context.contains(BRIEFING_TEXT));
            assert!(!file.starts_with(&slot) && !file.starts_with(&primary));
            assert_eq!(
                launch.env,
                vec![(OsString::from("CONTEXT_FLAG"), OsString::from("1"))]
            );
            assert!(!mentions_the_briefing(&launch));
        }
        assert!(
            pending(&home, &primary).is_some(),
            "kept until the work moves on"
        );
    }

    #[test]
    fn the_sections_are_asked_with_the_launching_checkout_and_the_room_left() {
        let (home, primary, slot, id) = managed("continuity-context-asked");
        let harness = Harness::with_channel(SessionContinuity::Assigned);
        let asked = std::cell::RefCell::new(Vec::new());
        let ask = |ask: &SectionAsk<'_>| {
            asked.borrow_mut().push((
                ask.project_root.to_path_buf(),
                ask.checkout.to_path_buf(),
                ask.room,
            ));
            Vec::new()
        };

        plan_with(&home, claim(&id, &slot), &harness, &ask);
        briefed(&home, &primary, "harness");
        plan_with(&home, claim(&id, &slot), &harness, &ask);

        let asked = asked.into_inner();
        let briefing = Section {
            heading: briefing::HEADING,
            text: BRIEFING_TEXT.to_owned(),
            share: agent_context::BRIEFING_SHARE,
        };
        let owner = conversation::owner_of(&home, claim(&id, &slot)).unwrap();
        let (instructions, _) = launch_instructions(&home, &owner, &slot);
        assert_eq!(
            asked,
            [
                (
                    primary.clone(),
                    slot.clone(),
                    agent_context::room_after(&instructions, &[])
                ),
                (
                    primary,
                    slot,
                    agent_context::room_after(&instructions, &[briefing])
                ),
            ],
            "asked from the agent's own checkout, with what the instructions and the briefing leave"
        );
    }

    #[test]
    fn a_retired_briefing_is_taken_back_from_the_next_launch() {
        let (home, primary, slot, id) = managed("continuity-context-withdrawn");
        let harness = Harness::with_channel(SessionContinuity::Assigned);
        briefed(&home, &primary, "harness");
        let given = plan(&home, claim(&id, &slot), &harness);
        let at = given
            .args
            .iter()
            .position(|word| word == "--context")
            .unwrap();
        let file = PathBuf::from(&given.args[at + 1]);
        assert!(file.is_file());

        let store = task::load(&home, &primary).unwrap();
        conversation::update(&home, &primary, &store.agents[0].id, |stored| {
            stored.briefing = None;
        })
        .unwrap();
        plan(&home, claim(&id, &slot), &harness);
        let context = std::fs::read_to_string(&file).unwrap();
        assert!(
            !context.contains(BRIEFING_TEXT),
            "the last launch's briefing does not outlive it: {context}"
        );
    }

    #[test]
    fn every_launch_is_given_its_instructions() {
        let (home, primary, slot, id) = managed("continuity-context-instructions");
        let launch = plan(
            &home,
            claim(&id, &slot),
            &Harness::with_channel(SessionContinuity::Assigned),
        );
        assert!(launch.args.contains(&OsString::from("--context")));
        let context = std::fs::read_to_string(context_file(&home, &primary)).unwrap();
        assert!(
            context.starts_with(agent_context::INSTRUCTIONS_LEAD),
            "{context}"
        );
        assert!(context.contains("`.worktrees/slot-1`"), "{context}");
        assert!(context.contains(&format!("`agent/{id}`")), "{context}");
        assert!(
            !context.contains(agent_context::PREAMBLE),
            "no notes, no preamble: {context}"
        );
    }

    #[test]
    fn the_policy_in_force_is_the_one_the_manifest_declares() {
        let (home, primary, slot, id) = managed("continuity-context-declared");
        std::fs::write(
            primary.join("agents.yaml"),
            "workspace:\n  delivery: push\n",
        )
        .unwrap();
        plan(
            &home,
            claim(&id, &slot),
            &Harness::with_channel(SessionContinuity::Assigned),
        );
        let context = std::fs::read_to_string(context_file(&home, &primary)).unwrap();
        assert!(
            context.contains(worktree::CompletionBehavior::instruction_clause(Some(
                worktree::CompletionBehavior::Push
            ))),
            "the declared behavior must be the one an agent reads: {context}"
        );
    }

    /// A harness that works in the first of its sorted workspace paths must
    /// be handed a link that sorts after the checkout, in space UZE owns.
    #[test]
    fn the_context_link_sorts_after_the_checkout_inside_the_isolation_directory() {
        let primary = Path::new("/home/me/project");
        let slot = primary.join(".worktrees").join("ab12cd");
        for (cwd, expected) in [
            (slot.clone(), primary.join(".worktrees").join("ab12cd~uze")),
            (
                primary.to_path_buf(),
                primary.join(".worktrees").join("~uze").join("agent-1"),
            ),
        ] {
            let link = context_link(&checkout_root(primary, &cwd), "agent-1");
            assert_eq!(link, expected);
            assert!(
                link.as_os_str() > cwd.as_os_str(),
                "{} must sort after {}",
                link.display(),
                cwd.display()
            );
            assert!(link.starts_with(primary.join(".worktrees")));
        }
    }

    /// A launch below the checkout root, or through a link to it, still
    /// names the root as the launch spells it, and the link sorts after it.
    #[test]
    fn the_checkout_root_is_spelled_as_the_launch_spells_it() {
        let root = uze_testkit::temp::scratch("continuity-checkout-root");
        let primary = root.join("repo");
        std::fs::create_dir_all(primary.join("src")).unwrap();
        let slot = primary.join(".worktrees").join("ab12cd");
        std::fs::create_dir_all(slot.join("src")).unwrap();

        assert_eq!(checkout_root(&primary, &primary.join("src")), primary);
        assert_eq!(checkout_root(&primary, &slot.join("src")), slot);
        let link = context_link(&primary, "agent-1");
        assert!(link.as_os_str() > primary.as_os_str());

        // The same tree reached through a link, made the Unix way: what the
        // launch's cwd says is what a channel compares against, not the
        // resolved path.
        #[cfg(unix)]
        {
            let alias = root.join("alias");
            std::os::unix::fs::symlink(&primary, &alias).unwrap();
            assert_eq!(checkout_root(&primary, &alias.join("src")), alias);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn every_note_a_launch_has_is_said() {
        let mut plan = LaunchPlan::nothing();
        plan.add_note("the default policy".into());
        plan.add_note("the context was not delivered".into());
        assert_eq!(
            plan.note.as_deref(),
            Some("the default policy; the context was not delivered")
        );
    }

    #[test]
    fn a_reftable_stub_head_falls_back_to_the_slot_branch() {
        let (home, primary, slot, id) = managed("continuity-context-reftable");
        let admin = primary.join(".git").join("worktrees").join("slot-1");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::write(slot.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
        std::fs::write(admin.join("HEAD"), "ref: refs/heads/.invalid\n").unwrap();

        plan(
            &home,
            claim(&id, &slot),
            &Harness::with_channel(SessionContinuity::Assigned),
        );
        let context = std::fs::read_to_string(context_file(&home, &primary)).unwrap();
        assert!(!context.contains(".invalid"), "{context}");
        let store = task::load(&home, &primary).unwrap();
        let recorded = store.agents[0].isolation().unwrap().branch.clone();
        assert!(
            context.contains(&format!("`{recorded}`")),
            "the record's branch: {context}"
        );

        // Naming the work updates the record, so a reftable checkout still
        // says the new name.
        let mut store = store;
        let agent = store.agents[0].id.clone();
        store
            .get_mut(&agent)
            .unwrap()
            .take_name("fix/naming".into());
        task::save(&home, &primary, &store).unwrap();
        plan(
            &home,
            claim(&id, &slot),
            &Harness::with_channel(SessionContinuity::Assigned),
        );
        let context = std::fs::read_to_string(context_file(&home, &primary)).unwrap();
        assert!(context.contains("`fix/naming`"), "{context}");
    }

    #[test]
    fn a_declaration_that_does_not_read_is_said_on_the_launch() {
        let (home, primary, slot, id) = managed("continuity-context-unread");
        std::fs::write(
            primary.join("agents.yaml"),
            "workspace:\n  delivery: sideways\n",
        )
        .unwrap();
        let launch = plan(
            &home,
            claim(&id, &slot),
            &Harness::with_channel(SessionContinuity::Assigned),
        );
        let note = launch.note.expect("the default is not given in silence");
        assert!(note.contains("agents.yaml"), "{note}");
        let context = std::fs::read_to_string(context_file(&home, &primary)).unwrap();
        assert!(
            context.contains(worktree::CompletionBehavior::instruction_clause(None)),
            "the agent is still told how its work is delivered: {context}"
        );
    }

    #[test]
    fn the_instructions_name_the_branch_the_checkout_has_now() {
        let (home, primary, slot, id) = managed("continuity-context-branch");
        let admin = primary.join(".git").join("worktrees").join("slot-1");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::write(slot.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
        std::fs::write(admin.join("HEAD"), "ref: refs/heads/fix/naming\n").unwrap();

        plan(
            &home,
            claim(&id, &slot),
            &Harness::with_channel(SessionContinuity::Assigned),
        );
        let context = std::fs::read_to_string(context_file(&home, &primary)).unwrap();
        assert!(context.contains("`fix/naming`"), "{context}");
    }

    #[test]
    fn an_agent_in_the_primary_checkout_is_told_it_is_on_the_operators_branch() {
        let (home, primary, _slot, id) = managed("continuity-context-in-place");
        let owner = conversation::Owner {
            project_root: primary.clone(),
            agent: AgentId::adopted(&id),
            isolated: false,
        };
        let (text, _) = launch_instructions(&home, &owner, &primary);
        assert!(text.contains("operator's own checkout"), "{text}");
        assert!(!text.contains(".worktrees"), "{text}");
        let _ = home;
    }

    #[test]
    fn a_degraded_new_conversation_is_pointed_at_its_instructions_and_a_resume_at_nothing() {
        let (home, primary, slot, id) = managed("continuity-context-degraded");
        let harness = Harness::new(SessionContinuity::Assigned);

        let first = plan(&home, claim(&id, &slot), &harness);
        assert_eq!(first.trailing, instructions_turn_for(&home, &primary));

        let resumed = plan(&home, claim(&id, &slot), &harness);
        assert!(resumed.args.contains(&OsString::from("--resume")));
        assert!(resumed.trailing.is_empty());
    }

    #[test]
    fn an_unavailable_channel_falls_back_to_the_first_turn() {
        let (home, primary, slot, id) = managed("continuity-context-unavailable");
        let mut harness = Harness::with_channel(SessionContinuity::Assigned);
        harness.unavailable = Some("hooks are off".into());
        briefed(&home, &primary, "harness");

        let launch = plan(&home, claim(&id, &slot), &harness);
        assert!(!launch.args.contains(&OsString::from("--context")));
        assert_eq!(launch.trailing, turn_for(&home, &primary));
    }

    #[test]
    fn an_operators_own_invocation_is_given_no_context() {
        let (home, primary, slot, _id) = managed("continuity-context-ordinary");
        briefed(&home, &primary, "harness");
        let launch = plan(
            &home,
            claim("not-an-agent", &slot),
            &Harness::with_channel(SessionContinuity::Assigned),
        );
        assert_eq!(launch, LaunchPlan::default());
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
            LaunchPlan {
                owned: true,
                trailing: instructions_turn_for(&home, &primary),
                ..LaunchPlan::nothing()
            },
            "no argument of its own: the only thing carried is the degraded turn"
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
            LaunchPlan {
                owned: true,
                trailing: instructions_turn_for(&home, &primary),
                ..LaunchPlan::nothing()
            },
            "every launch of it is a new conversation, pointed at its instructions"
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
