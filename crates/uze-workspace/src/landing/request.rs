//! A published branch and the request opened for it.

use super::*;

/// What the remote holds for a task's branch.
///
/// Publication is a Git fact, exactly like readiness. `git push` writes
/// `refs/remotes/origin/<name>` whoever ran it and from whichever
/// checkout, so a branch its own agent pushed by hand is as published as
/// one UZE pushed, and commits the agent added to an open request are as
/// synced. Reading UZE's record of its own pushes instead made every
/// surface report UZE's history rather than the remote's state — a
/// button offering to send what the remote already had.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Publication {
    /// The name the branch is published under on the remote.
    pub branch: String,
    /// The commit the remote-tracking ref points at.
    pub tip: String,
}

/// How long the remote may go unasked about a request that does not exist
/// yet. The branch is pushed and the agent opens the request moments
/// later, so the answer changes on a human's clock, not a machine's — and
/// this is the only publication question that leaves the machine.
pub(super) const REQUEST_INTERVAL: Duration = Duration::from_secs(60);

/// What the remote holds for this task's branch, or `None` when the
/// branch is not on the remote at all.
///
/// The name UZE published under is tried first and the branch's own name
/// second: an unnamed task leaves under a derived name that nothing but
/// [`Task::published_as`] ties back to it, while a task whose agent
/// pushed on its own is on the remote under the only name it has.
pub fn publication(primary: &Path, isolation: &Isolation) -> Option<Publication> {
    isolation
        .published_as
        .iter()
        .chain(std::iter::once(&isolation.branch))
        .find_map(|branch| {
            let tip = checkout::tip_of(primary, &format!("refs/remotes/{REMOTE}/{branch}"));
            (!tip.is_empty()).then(|| Publication {
                branch: branch.clone(),
                tip,
            })
        })
}

/// [`publication`] read from `tips` rather than by a Git process per
/// branch, for a pass that asks it of every recorded agent.
pub fn publication_among(
    tips: &checkout::BranchTips,
    isolation: &Isolation,
) -> Option<Publication> {
    isolation
        .published_as
        .iter()
        .chain(std::iter::once(&isolation.branch))
        .find_map(|branch| {
            tips.remote(REMOTE, branch).map(|tip| Publication {
                branch: branch.clone(),
                tip: tip.to_owned(),
            })
        })
}

/// What the remote said about a task's request, ready to be written down
/// by [`adopt_request`].
///
/// The two halves are separate because only one of them touches the
/// document: the asking is a `git ls-remote`, the one question in an
/// evaluation that leaves the machine, and the evaluation pass holds
/// every task it is about to write while it runs. Asked inside that
/// lock, one slow remote made every delivery and every placement in the
/// project wait for it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestObservation {
    /// The branch the remote publishes the work under, when it has one.
    pub(super) published: Option<String>,
    /// When the remote was asked — `None` when it was not asked at all.
    pub(super) asked_at_unix: Option<u64>,
    /// The request found for the published branch, when one was.
    pub(super) request: Option<u32>,
}

impl RequestObservation {
    /// Whether answering this took a question to the remote, rather than
    /// the local refs and the clock alone.
    pub fn asked_the_remote(&self) -> bool {
        self.asked_at_unix.is_some()
    }
}

/// Asks the remote whether a request is open for this task's published
/// branch, writing nothing.
///
/// Called on the evaluation pass rather than only from [`publish`],
/// because the request is never UZE's doing: whoever opens it, UZE learns
/// about it only from the forge, and published work would go on reading
/// as published, not in review, for the whole life of the request. Gated
/// three ways so the round
/// trip stays rare — a published branch, no number yet, and at most one
/// question per [`REQUEST_INTERVAL`] — and it stops for good the moment
/// it is answered.
pub fn observe_request(primary: &Path, isolation: &Isolation) -> RequestObservation {
    let published = publication(primary, isolation);
    let branch = published.as_ref().map(|found| found.branch.clone());
    let unasked = RequestObservation {
        published: branch.clone(),
        asked_at_unix: None,
        request: None,
    };
    // A number answers for the branch it was found on, and the task
    // outlives both: an agent that delivered keeps working in the same
    // checkout, often on a new branch, and a number cached for good went on
    // naming the request it had already merged. Such a number is dropped by
    // `adopt_request`, which is what makes this the moment to ask again —
    // and what makes the clock below irrelevant to it, since the answer it
    // records was about another branch.
    let stale = isolation.published_request.is_some() && isolation.request_branch != branch;
    if isolation.published_request.is_some() && !stale {
        return unasked;
    }
    let now = crate::task::now_unix();
    let asked_recently = !stale
        && isolation
            .request_asked_at_unix
            .is_some_and(|asked| now.saturating_sub(asked) < REQUEST_INTERVAL.as_secs());
    if asked_recently {
        return unasked;
    }
    let Some(published) = published else {
        return unasked;
    };
    RequestObservation {
        published: Some(published.branch),
        asked_at_unix: Some(now),
        request: discover_request(primary, &published.tip),
    }
}

/// Whether [`observe_request`] has anything to do for this task, read from
/// `tips` and the record alone: a number for a branch the remote no longer
/// publishes (to forget), or a published branch with no number yet, not
/// asked about within [`REQUEST_INTERVAL`]. A pass asks only these, so a
/// task whose request is known, or that is not on the remote, costs it no
/// process and no write.
pub fn request_due(tips: &checkout::BranchTips, isolation: &Isolation) -> bool {
    let branch = publication_among(tips, isolation).map(|found| found.branch);
    if isolation.published_request.is_some() {
        return isolation.request_branch != branch;
    }
    branch.is_some()
        && !isolation.request_asked_at_unix.is_some_and(|asked| {
            crate::task::now_unix().saturating_sub(asked) < REQUEST_INTERVAL.as_secs()
        })
}

/// Writes down what [`observe_request`] learned. The caller holds the
/// tasks document for this and for nothing else the question needed.
pub fn adopt_request(isolation: &mut Isolation, observed: &RequestObservation) {
    isolation.forget_request_unless_for(observed.published.as_deref());
    if isolation.published_request.is_some() {
        return;
    }
    let Some(asked_at) = observed.asked_at_unix else {
        return;
    };
    isolation.request_asked_at_unix = Some(asked_at);
    isolation.published_request = observed.request;
    isolation.request_branch = observed.request.and_then(|_| observed.published.clone());
}

pub(super) fn effective_base(primary: &Path, isolation: &Isolation) -> String {
    match checkout::target_reach(primary, &isolation.target, Some(&isolation.branch)) {
        Some(reach)
            if reach != isolation.base_commit
                && is_ancestor(primary, &reach, &isolation.branch) =>
        {
            reach
        }
        _ => isolation.base_commit.clone(),
    }
}
