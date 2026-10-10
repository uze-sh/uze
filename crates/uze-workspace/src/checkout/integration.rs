//! Whether a branch already landed in its target, by commit or by patch, remembered per pair of commits.

use super::*;

/// Whether everything `branch` carries is already in `target`: reachable
/// from it, or there under other commits carrying the same patch.
///
/// Reachability alone is the wrong question wherever the target is written
/// by a forge that rewrites what it integrates. A squash merge replaces a
/// branch's commits with one of its own and a rebase merge gives each of
/// them a new identity, so a branch merged weeks ago still counts commits
/// the target "lacks" — and that answer is what parks its slot, revives its
/// task and keeps its branch, until every new agent is paying for a working
/// tree of its own. Patch identity is what survives both rewrites, and
/// `git cherry` is Git's own answer to it: asked of the branch's commits
/// first, which is free, and then of the single patch a squash would have
/// made of them, which costs one object.
///
/// Fails closed, the way [`is_dirty`] does: a question Git could not answer
/// is answered `false` here, because this predicate is what authorizes
/// `branch -D` and `reset --hard` over an agent's committed work.
///
/// Remembered per pair of commits for the life of the process. Evaluation
/// asks it of every shelved branch on every pass, and the squash question
/// costs up to five Git processes a branch; the answer can only change when
/// one of the two commits does, and resolving them is one process. Only a
/// complete answer is remembered — one that fell closed is asked again.
pub fn is_integrated(root: &Path, target: &str, branch: &str) -> bool {
    let Some((target, branch)) = resolve_commit_pair(root, target, branch) else {
        return false;
    };
    integrated_commits(root, target, branch)
}

/// [`is_integrated`] for a pass that asks it of many branches: the names
/// are resolved from `tips`, read once, instead of by a Git process per
/// question — with the answer remembered, resolving was all a repeated
/// question cost, and a collection asks it of every branch UZE ever cut.
/// A name `tips` does not hold is resolved the way [`is_integrated`] would.
pub fn is_integrated_among(tips: &BranchTips, root: &Path, target: &str, branch: &str) -> bool {
    match (tips.local(target), tips.local(branch)) {
        (Some(target), Some(branch)) => {
            integrated_commits(root, target.to_owned(), branch.to_owned())
        }
        _ => is_integrated(root, target, branch),
    }
}

/// [`commits_ahead`] of a local branch over a commit, for a pass that asks
/// it of every recorded agent: the branch is resolved from `tips` and the
/// count is remembered per pair of commits, since it can only change when
/// one of them does. A branch `tips` does not hold has nothing ahead, which
/// is what asking Git about a missing branch answers too.
pub fn commits_ahead_among(tips: &BranchTips, root: &Path, base: &str, branch: &str) -> usize {
    let Some(tip) = tips.local(branch) else {
        return 0;
    };
    let question = IntegrationQuestion {
        root: root.to_path_buf(),
        target: base.to_owned(),
        branch: tip.to_owned(),
    };
    let mut counts = AHEAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(count) = counts.get(&question) {
        return *count;
    }
    let Some(count) = commits_ahead_checked(root, base, tip) else {
        return 0;
    };
    if counts.len() >= REMEMBERED_INTEGRATIONS {
        counts.clear();
    }
    counts.insert(question, count);
    count
}

static AHEAD: LazyLock<Mutex<HashMap<IntegrationQuestion, usize>>> = LazyLock::new(Mutex::default);

/// Every local branch and remote-tracking ref, and the commit each names,
/// read in one process.
#[derive(Default)]
pub struct BranchTips(HashMap<String, String>);

impl BranchTips {
    pub fn read(root: &Path) -> Self {
        let listing = read(
            root,
            &[
                "for-each-ref",
                "--format=%(objectname) %(refname)",
                "refs/heads/",
                "refs/remotes/",
            ],
        )
        .unwrap_or_default();
        Self(
            listing
                .lines()
                .filter_map(|line| line.split_once(' '))
                .map(|(commit, name)| (name.to_owned(), commit.to_owned()))
                .collect(),
        )
    }

    /// The commit the local branch `name` points at.
    pub fn local(&self, name: &str) -> Option<&str> {
        self.0
            .get(&format!("refs/heads/{name}"))
            .map(String::as_str)
    }

    /// The commit `remote`'s tracking ref for `name` points at.
    pub fn remote(&self, remote: &str, name: &str) -> Option<&str> {
        self.0
            .get(&format!("refs/remotes/{remote}/{name}"))
            .map(String::as_str)
    }

    pub fn contains(&self, branch: &str) -> bool {
        self.local(branch).is_some()
    }
}

pub(super) fn integrated_commits(root: &Path, target: String, branch: String) -> bool {
    let question = IntegrationQuestion {
        root: root.to_path_buf(),
        target,
        branch,
    };
    if let Some(answer) = remembered_integration(&question) {
        return answer;
    }
    match integration_between(root, &question.target, &question.branch) {
        Some(answer) => {
            remember_integration(question, answer);
            answer
        }
        None => false,
    }
}

#[derive(PartialEq, Eq, Hash)]
pub(super) struct IntegrationQuestion {
    pub(super) root: PathBuf,
    pub(super) target: String,
    pub(super) branch: String,
}

/// Enough for every branch of a busy repository across many moves of its
/// target; past it the memory starts over rather than tracking age.
pub(super) const REMEMBERED_INTEGRATIONS: usize = 4096;

pub(super) static INTEGRATIONS: LazyLock<Mutex<HashMap<IntegrationQuestion, bool>>> =
    LazyLock::new(Mutex::default);

pub(super) fn remembered_integration(question: &IntegrationQuestion) -> Option<bool> {
    let integrations = INTEGRATIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    integrations.get(question).copied()
}

pub(super) fn remember_integration(question: IntegrationQuestion, answer: bool) {
    let mut integrations = INTEGRATIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if integrations.len() >= REMEMBERED_INTEGRATIONS {
        integrations.clear();
    }
    integrations.insert(question, answer);
}

/// Both names as the commits they point at, in one process, or `None` when
/// either does not name a commit.
pub(super) fn resolve_commit_pair(
    root: &Path,
    target: &str,
    branch: &str,
) -> Option<(String, String)> {
    let listing = read(
        root,
        &[
            "rev-parse",
            &format!("{target}^{{commit}}"),
            &format!("{branch}^{{commit}}"),
            "--",
        ],
    )?;
    let mut commits = listing.lines().filter(|line| *line != "--");
    let (Some(target), Some(branch), None) = (commits.next(), commits.next(), commits.next())
    else {
        return None;
    };
    Some((target.to_owned(), branch.to_owned()))
}

/// Whether `branch` is in `target`, or `None` when Git could not answer a
/// question a "no" would rest on. A "yes" from either patch question stands
/// on its own: the other failing cannot take it back.
pub(super) fn integration_between(root: &Path, target: &str, branch: &str) -> Option<bool> {
    if commits_ahead_checked(root, target, branch)? == 0 {
        return Some(true);
    }
    let patch = patch_is_in(root, target, branch);
    if patch == Some(true) {
        return Some(true);
    }
    if squashed_patch_is_in(root, target, branch)? {
        return Some(true);
    }
    patch
}

/// Whether every commit of `branch` outside `target` has an equivalent
/// there — what a rebase merge, and a fast-forward of a single commit,
/// leave behind.
pub(super) fn patch_is_in(root: &Path, target: &str, branch: &str) -> Option<bool> {
    read(root, &["cherry", "--end-of-options", target, branch])
        .map(|listing| every_commit_is_there(&listing))
}

/// `git cherry` marks a commit `-` when the target already has its patch.
/// Nothing listed is not an answer: the question was asked of commits, and
/// there were none to read.
pub(super) fn every_commit_is_there(listing: &str) -> bool {
    let mut commits = listing.lines().peekable();
    commits.peek().is_some() && commits.all(|commit| commit.starts_with('-'))
}

/// Whether the one patch a squash would have made of `branch` is already in
/// `target`. The probe commit is written rather than described because
/// patch identity is Git's to compute: its tree is the branch's, its parent
/// the merge base, so it carries exactly what the branch adds and nothing
/// of how it was written.
///
/// Its dates are pinned: evaluation asks this of every shelved task on every
/// pass, and a probe dated by the clock was a new object each time — loose
/// objects piling up in the operator's repository until Git collected them.
pub(super) fn squashed_patch_is_in(root: &Path, target: &str, branch: &str) -> Option<bool> {
    let base = read(root, &["merge-base", "--", target, branch])?;
    let tree = read(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &format!("{branch}^{{tree}}"),
        ],
    )?;
    let probe = crate::git::write_with_env(
        root,
        &[
            "-c",
            "user.name=UZE",
            "-c",
            "user.email=uze@invalid",
            "commit-tree",
            &tree,
            "-p",
            &base,
            "-m",
            "squash probe",
        ],
        &[
            ("GIT_AUTHOR_DATE", "@0 +0000"),
            ("GIT_COMMITTER_DATE", "@0 +0000"),
        ],
    )
    .ok()
    .and_then(|output| output.successful().ok())
    .map(|stdout| stdout.trim().to_owned())?;
    patch_is_in(root, target, &probe)
}

/// A read whose failure is simply no answer.
pub(super) fn read(root: &Path, args: &[&str]) -> Option<String> {
    crate::git::read(root, args)
        .ok()?
        .successful()
        .ok()
        .map(|stdout| stdout.trim().to_owned())
}

pub fn branch_exists(root: &Path, branch: &str) -> bool {
    crate::git::read(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok_and(|output| output.is_success())
}

/// Whether `branch` is a name somebody already has: a local branch, or one
/// a remote is known to carry. A name taken only on a remote is a
/// colleague's branch the next fetch brings in, and work published under
/// it would land on top of theirs.
pub fn name_is_taken(root: &Path, branch: &str) -> bool {
    if branch_exists(root, branch) {
        return true;
    }
    let Some(remotes) = read(root, &["remote"]) else {
        return false;
    };
    let candidates: Vec<String> = remotes
        .lines()
        .map(|remote| format!("refs/remotes/{remote}/{branch}"))
        .collect();
    if candidates.is_empty() {
        return false;
    }
    let mut args = vec!["for-each-ref", "--count=1", "--format=%(refname)"];
    args.extend(candidates.iter().map(String::as_str));
    read(root, &args).is_some_and(|found| !found.is_empty())
}

pub(super) fn agent_branches(root: &Path) -> Vec<String> {
    crate::git::read(
        root,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            &format!("refs/heads/{BRANCH_PREFIX}"),
        ],
    )
    .ok()
    .and_then(|output| output.successful().ok())
    .map(|stdout| stdout.lines().map(str::to_owned).collect())
    .unwrap_or_default()
}
