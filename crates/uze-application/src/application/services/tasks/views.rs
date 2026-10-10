//! The read models a person's surfaces are drawn from: an agent, its state, a delivery's report.

use super::*;
use uze_core::path::Canonical as _;

/// A task ended because its agent is gone, and what became of its slot.
/// What one pass of [`Workspace::reconcile_occupancy`] changed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Reconciliation {
    /// The directories whose repository actually gave a slot up, or whose
    /// agents ended, so a caller knows which agents are worth re-reading.
    /// Empty is the ordinary answer.
    pub changed: Vec<PathBuf>,
    pub released: Vec<ReleasedTask>,
    /// The directories whose repository has an agent no pane is in front
    /// of, whose checkout something was still working in: an agent's own
    /// process taking its moment to exit, as often as not. Nothing else
    /// asks again, so the caller does, a little later.
    pub waiting: Vec<PathBuf>,
}

/// The canonical spelling of a directory: the key every record of it is
/// stored under, so two spellings of one root never make two stores.
pub(super) fn canonical(root: &Path) -> PathBuf {
    root.canonical().unwrap_or_else(|_| root.to_path_buf())
}

/// Where one agent is placed: in a checkout of its own, or in the space's
/// own directory beside whoever else is in it. The same two answers
/// `workspace.worktree` gives, asked of a single launch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementKind {
    Isolated,
    InPlace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleasedTask {
    pub id: String,
    pub label: String,
    /// `true` when the task ended holding work — kept on its branch or a
    /// shelf, and listed as unfinished — rather than nothing at all.
    pub unfinished: bool,
}

/// The delivery target against its upstream: commits a pull would bring
/// in and a push would send out.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UpstreamSync {
    pub pull: usize,
    pub push: usize,
}

/// One piece of preserved work, as the list that crosses projects sees it.
///
/// Deliberately thinner than a [`AgentView`]. Readiness, publication and how
/// far a branch is ahead are questions about the project you are *in*, and
/// answering them here would put one Git read per project on the machine
/// behind a keystroke. Everything below comes from the record alone, which
/// is what keeps the list instant however many projects accumulate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreservedWork {
    /// The repository this work belongs to. A list that crosses projects
    /// has to say, or two agents on a branch of the same name in two
    /// projects are one row twice.
    pub project: PathBuf,
    pub id: String,
    pub label: String,
    pub branch: String,
    pub checkout: Option<PathBuf>,
    /// The state the record itself carries — never `Integrated` or
    /// `Closed`, which is what "preserved" means.
    pub state: WorkStateView,
    pub created_at_unix: u64,
}

/// One task as presentation sees it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentView {
    pub id: String,
    pub label: String,
    pub branch: String,
    pub target: String,
    /// The checkout this agent works in: a slot of its own when it has
    /// one, and the project's own root when it does not.
    pub checkout: Option<PathBuf>,
    /// Whether the checkout above was cut for this agent alone.
    ///
    /// The fact, not one of its consequences: an isolated agent is the one
    /// UZE gave a branch and a slot to, and that is what decides whether
    /// delivering it is UZE's to do, which group its row sits in, and
    /// whether it can still be offered isolation. Read the fact and state
    /// the rule at the call site — `branch` is not a proxy for it, because
    /// an agent in the project's root is on one too.
    pub isolated: bool,
    /// The agent whose subagent this is: its checkout was split from that
    /// agent's, and its work joins that agent's branch rather than the
    /// target.
    pub parent: Option<String>,
    pub state: WorkStateView,
    /// What delivering this task does — the project's own say, carried on
    /// the task so a surface offering the delivery can name its ending
    /// instead of showing one verb for three different outcomes.
    pub completion: CompletionBehavior,
    /// Commits the branch has beyond its base — what a delivery would land.
    pub ahead: usize,
    /// The name the branch is published under on the remote, once it is
    /// there. Read from the repository's remote-tracking refs, so a
    /// branch its own agent pushed reads as published exactly like one
    /// UZE pushed.
    pub published_as: Option<String>,
    /// The request open on the forge for the published branch, once there
    /// is one: what turns the delivery button from an errand into a sync.
    pub published_request: Option<u32>,
    /// Which forge `origin` points at, so a surface reporting the request
    /// can use that forge's own word for it — a pull request and a merge
    /// request are one thing under two names, and the remote is what says
    /// which name this project's reader reads. [`Forge::Unknown`] where
    /// it did not say, and then nothing is claimed.
    pub forge: Forge,
    /// Commits the published branch does not carry yet — what a sync would
    /// send. `None` until the branch has been published at all, and
    /// `Some(0)` once the request is level with the branch: work already
    /// handed over is not work waiting to be handed over, however far the
    /// branch still is from the target, which only a merge closes.
    pub unsynced: Option<usize>,
    pub created_at_unix: u64,
}

impl PreservedWork {
    /// `None` for an agent that has nothing preserved: one working in the
    /// project's own root, which has no branch to hold work on, and one
    /// whose work the target already carries or that never had any.
    ///
    /// Read from the record alone — the state is the one the record
    /// carries, not the one a Git read would draw — because this is what
    /// lets the list answer for a machine without asking a repository
    /// anything.
    pub(super) fn from_agent(project: &Path, agent: &Agent) -> Option<Self> {
        let isolation = agent.isolation()?;
        if matches!(agent.state, WorkState::Integrated | WorkState::Closed) {
            return None;
        }
        Some(Self {
            project: project.to_path_buf(),
            id: agent.id.as_str().to_owned(),
            label: agent.label.clone(),
            branch: isolation.branch.clone(),
            checkout: landing::slot_path(project, isolation),
            state: WorkStateView::from(&agent.state),
            created_at_unix: agent.created_at_unix,
        })
    }
}

impl AgentView {
    /// `None` for an agent working in the project's root: every field
    /// here is about a branch of its own, and it has none.
    pub(super) fn from_agent(
        primary: &Path,
        tips: &checkout::BranchTips,
        agent: &Agent,
        completion: CompletionBehavior,
        target: &str,
        forge: Forge,
    ) -> Option<Self> {
        let Some(task) = agent.isolation() else {
            return Some(Self::in_the_root(primary, agent, completion, target, forge));
        };
        // What the remote holds, not what UZE remembers having sent: a
        // push the agent made is a push, and a view built from UZE's own
        // record of its own deliveries goes on offering to send commits
        // the request already carries.
        //
        // Only where the completion publishes. A branch on the remote is
        // no part of what a merge or a handoff would do, and counting a
        // merge's commits against the remote would report a task as
        // delivered the moment its agent pushed it.
        let published = (completion == CompletionBehavior::Pr)
            .then(|| landing::publication_among(tips, task))
            .flatten();
        let unsynced = published.as_ref().map(|published| {
            checkout::commits_ahead_among(tips, primary, &published.tip, &task.branch)
        });
        Some(Self {
            id: agent.id.as_str().to_owned(),
            label: agent.label.clone(),
            branch: task.branch.clone(),
            target: task.target.clone(),
            checkout: landing::slot_path(primary, task),
            state: drawn_state(primary, &agent.state, task, unsynced),
            completion,
            isolated: true,
            parent: agent
                .parent
                .as_ref()
                .map(|parent| parent.as_str().to_owned()),
            ahead: checkout::commits_ahead_among(tips, primary, &task.base_commit, &task.branch),
            published_as: published.map(|published| published.branch),
            published_request: task.published_request,
            forge,
            unsynced,
            created_at_unix: agent.created_at_unix,
        })
    }

    /// An agent working in the project's own root, beside the operator.
    ///
    /// Every field here is about the checkout it sits in rather than one
    /// cut for it, and every other agent in that checkout reads the same
    /// answers — which is the truth about where they are. What it never
    /// carries is a publication or a base of its own: the branch is the
    /// operator's, UZE did not cut it, and `isolated` says so, because
    /// rebasing and pushing it is theirs to ask for and never UZE's to
    /// offer.
    pub(super) fn in_the_root(
        primary: &Path,
        agent: &Agent,
        completion: CompletionBehavior,
        target: &str,
        forge: Forge,
    ) -> Self {
        let branch = checkout::current_branch(primary).unwrap_or_default();
        Self {
            id: agent.id.as_str().to_owned(),
            label: agent.label.clone(),
            ahead: if branch.is_empty() {
                0
            } else {
                checkout::commits_ahead(primary, target, &branch)
            },
            branch,
            target: target.to_owned(),
            checkout: Some(primary.to_path_buf()),
            state: WorkStateView::from(&agent.state),
            completion,
            isolated: false,
            parent: None,
            published_as: None,
            published_request: None,
            forge,
            unsynced: None,
            created_at_unix: agent.created_at_unix,
        }
    }
}

/// The state a task reads as once what the remote holds is folded in.
///
/// `Ready` alone answers "the branch holds commits its base lacks", which
/// stops being the interesting question the moment the branch is on the
/// remote: from then on what every surface needs to say is whether
/// anything is still waiting to be handed over. Only where the completion
/// publishes — `published` is `None` everywhere else, and the record's own
/// state stands.
pub(super) fn drawn_state(
    primary: &Path,
    state: &WorkState,
    task: &Isolation,
    unsynced: Option<usize>,
) -> WorkStateView {
    // Parked says only that nobody is there. A rebase paused in the
    // checkout is what the operator will find, and "uncommitted changes"
    // sent them looking for edits that were really conflict markers.
    if *state == WorkState::Shelved
        && let Some(files) =
            landing::slot_path(primary, task).and_then(|slot| landing::paused_rebase(&slot))
    {
        return WorkStateView::Conflicted { files };
    }
    publication_state(state, unsynced)
}

pub(super) fn publication_state(state: &WorkState, unsynced: Option<usize>) -> WorkStateView {
    let view = WorkStateView::from(state);
    if view == WorkStateView::Ready && unsynced == Some(0) {
        return WorkStateView::Published;
    }
    view
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkStateView {
    Running,
    Uncommitted,
    Ready,
    /// The branch is on the remote and carries nothing the remote lacks:
    /// the work is with whoever reviews it, not with the operator.
    ///
    /// A view of `Ready`, not a record of its own — `Ready` is still what
    /// the branch holds, and pressing deliver still syncs it as the target
    /// moves. It is a state here because "there is work to hand over" and
    /// "the work is handed over" are the two things every surface has to
    /// tell apart, and a surface that reads only `Ready` cannot: the
    /// sidebar went on marking a task deliverable for the whole life of an
    /// open request.
    Published,
    Integrating,
    Conflicted {
        files: Vec<PathBuf>,
    },
    GateFailed,
    Integrated,
    Shelved,
    /// The agent is gone and its branch held nothing to deliver.
    Closed,
}

impl WorkStateView {
    /// Why delivery is refused, for a state where it is — `None` for the
    /// states delivery may be offered for. `Published` is one: the request
    /// is level with the branch, but the target moves, and a re-sync is how
    /// the branch follows it.
    ///
    /// Beside the predicate rather than beside whoever shows the answer:
    /// "not yet" and "already done" are the same refusal to a caller that
    /// only sees a boolean, and a second surface asking the same question
    /// would otherwise write its own second version of these words.
    ///
    /// A few words each: these are read in the header's own row, beside
    /// the button that was just pressed, where the state's mark and the
    /// tab already carry everything the sentence would repeat.
    pub fn undeliverable_reason(&self) -> Option<&'static str> {
        match self {
            Self::Ready | Self::Published | Self::GateFailed => None,
            Self::Running => Some("nothing committed"),
            Self::Uncommitted => Some("uncommitted changes"),
            Self::Conflicted { .. } => Some("rebase paused"),
            Self::Integrating => Some("already delivering"),
            Self::Integrated => Some("already delivered"),
            Self::Closed => Some("branch holds nothing"),
            Self::Shelved => Some("unfinished — resume it first"),
        }
    }
}

impl From<&WorkState> for WorkStateView {
    fn from(state: &WorkState) -> Self {
        match state {
            WorkState::Running => Self::Running,
            WorkState::Uncommitted => Self::Uncommitted,
            WorkState::Ready => Self::Ready,
            WorkState::Integrating => Self::Integrating,
            WorkState::Conflicted { files } => Self::Conflicted {
                files: files.clone(),
            },
            WorkState::GateFailed => Self::GateFailed,
            WorkState::Integrated => Self::Integrated,
            WorkState::Shelved => Self::Shelved,
            WorkState::Closed => Self::Closed,
        }
    }
}

/// A message for the pane of the agent that owns `task`, running in
/// `checkout`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentNotice {
    pub task: String,
    pub checkout: PathBuf,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Evaluation {
    pub tasks: Vec<AgentView>,
    pub notices: Vec<AgentNotice>,
    /// Why the repository's recorded tasks could not be read, when they
    /// could not be.
    ///
    /// An empty `tasks` says "this repository has no tasks", and a store
    /// that failed to open says something entirely different — every agent
    /// loses its branch, its mark and its delivery button, and the surface
    /// that swallowed the error has no way to say why. `place_new_agent`
    /// already reported this and was the only thing that did, so the
    /// condition surfaced as a single truncated line the one time somebody
    /// happened to add an agent.
    pub unreadable: Option<String>,
    /// The document could not be read at all and was set aside, so the
    /// tasks above were adopted afresh from the checkouts Git registers
    /// rather than read from what UZE had recorded.
    ///
    /// Said once, and never in place of the work: an old schema, a hand
    /// edit or a corrupt file used to stop every agent in the project
    /// from being placed, which is a worse answer than starting the
    /// record again from what is on disk.
    pub recovered: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeliveryOutcome {
    Handoff,
    Merged,
    /// The branch was pushed and the forge already has this request
    /// open for it: from here delivery is a sync, and Git alone.
    Published {
        branch: String,
        request: u32,
    },
    /// The branch was pushed and has no request yet, so the owning agent
    /// was handed the words to open one. Carries an [`AgentNotice`] like
    /// the two failures do, because it reaches the agent the same way: a
    /// submission into its pane.
    AwaitingRequest(AgentNotice),
    /// Nothing was written; the reason names why.
    Refused(String),
    /// Nothing was written: the project's gate waits for the operator's
    /// approval, and these are the commands to approve.
    AwaitingApproval(crate::application::services::CommandsAwaitingApproval),
    /// The target is untouched and the owning agent has been told what to do.
    ReturnedToAgent(AgentNotice),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryReport {
    pub task: AgentView,
    pub outcome: DeliveryOutcome,
    /// What the delivery could not do, none of which undoes the outcome —
    /// the same channel [`AgentPlacement`] carries. The one entry today is
    /// the delivery that landed and could not be written down: the branch
    /// is pushed and the request is open, and the next evaluation still
    /// reads the task as deliverable, so the operator has to know before
    /// pressing it again.
    pub warnings: Vec<String>,
}

/// What writing the policy is about to do to the project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyWriteConsequence {
    /// The file does not exist yet, so declaring adds a tracked file to
    /// the repository rather than editing one.
    pub creates_manifest: bool,
    pub manifest: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryPolicyView {
    pub completion: &'static str,
    pub target: Option<String>,
    pub gate: Vec<String>,
}

/// What bringing a project's target in line with its remote found worth
/// saying: why it stayed behind, if it did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetSyncReport {
    pub project: PathBuf,
    pub concern: Option<String>,
}

/// Where an agent starts, and the record its launch carries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentPlacement {
    pub cwd: PathBuf,
    /// The repository the agent belongs to, which is a different question
    /// from where it will run: an isolated agent's `cwd` is a checkout
    /// under the project, and a client deciding *which space* to open the
    /// tab in needs the project rather than the checkout.
    pub project: PathBuf,
    pub placement: Placement,
    /// What preparing the checkout could not do — a missing link target, a
    /// failed setup — none of which stops the launch.
    pub warnings: Vec<String>,
    /// The project's commands, when the checkout was placed without them
    /// because the operator has not approved them yet.
    pub awaiting_approval: Option<crate::application::services::CommandsAwaitingApproval>,
    /// The agent's row as it stands the instant it exists, so a client can
    /// draw it before an evaluation has run. `None` only where the record
    /// could not be read back — never a reason to draw the agent as
    /// something else.
    pub view: Option<AgentView>,
    /// The secret this launch of the agent carries in its environment,
    /// beside its identity, issued by the placement and recorded only as a
    /// digest. Empty until the placement is complete.
    pub launch_key: String,
}

/// What an agent was placed as. There is no third case: a placement that
/// cannot do what was asked answers with an error and starts nothing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Placement {
    /// The agent runs in a checkout of its own, on the task's branch.
    Isolated {
        task: AgentId,
        checkout: checkout::CheckoutId,
        branch: String,
        reused: bool,
    },
    /// The agent runs in the space's own directory, on whatever branch it
    /// is on, sharing that tree with whoever else is in it.
    InPlace { id: AgentId },
}

impl Placement {
    /// The identity the launch carries, whichever kind of record it is.
    pub fn agent(&self) -> &AgentId {
        match self {
            Self::Isolated { task, .. } => task,
            Self::InPlace { id } => id,
        }
    }
}
