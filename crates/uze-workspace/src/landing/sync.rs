//! Bringing the target branch up to date with its remote.

use super::*;

/// What bringing the local target in line with the remote's did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TargetSync {
    /// Nothing to sync against: no remote, or the target is not on it yet.
    Unpublished,
    /// The local target already carries everything the remote's does.
    Current,
    /// The local target was moved forward onto the remote's tip.
    FastForwarded { commits: usize },
    /// The remote is ahead and the local target stayed where it was.
    Stalled { behind: usize, reason: String },
}

impl TargetSync {
    /// What the operator has to be told, if anything: agents placed on a
    /// target that could not be brought up to date start behind the work
    /// everyone else is already on, and hear about it as a conflict much
    /// later.
    pub fn concern(&self, target: &str) -> Option<String> {
        match self {
            Self::Stalled { behind, reason } if *behind > 0 => {
                let plural = if *behind == 1 { "" } else { "s" };
                Some(format!(
                    "`{target}` is {behind} commit{plural} behind `{REMOTE}` and could not be \
                     moved: {reason}. New agents start from the local tip."
                ))
            }
            Self::Stalled { reason, .. } => Some(format!(
                "`{REMOTE}` could not be read: {reason}. New agents start from the local tip, \
                 which may be behind."
            )),
            _ => None,
        }
    }
}

/// Brings the local target in line with the remote's, by fast-forward and
/// never by anything else. Run on the workspace's clock rather than by each
/// placement, which branches from whatever this last left.
///
/// An agent is placed on the target's tip, and every judgement made about
/// its work afterwards — what it is ahead of, whether its slot holds
/// anything, what it rebases onto — is made against that same local branch.
/// Left unfetched, the branch drifts a whole day's merges behind the target
/// everyone else is on: the agent starts from history nobody has, and the
/// divergence surfaces as conflicts in a request already opened, which is
/// the most expensive place to learn it.
///
/// Fast-forward only, so this can never lose or reorder an operator's own
/// commits: a target that has commits the remote lacks is left exactly
/// where it is and reported, as is one Git refuses to move because the
/// primary checkout has local modifications in the way.
#[tracing::instrument(name = "landing.sync_target", level = "debug", skip_all, fields(%target))]
pub fn sync_target(primary: &Path, target: &str) -> TargetSync {
    if !has_remote(primary) {
        return TargetSync::Unpublished;
    }
    // The network half without the lock (see `uze_git::fetch_private`): it
    // is seconds long, and every placement and evaluation in the project
    // takes the lock for a prune.
    let fetched = format!("sync/{target}");
    let source = format!("refs/heads/{target}");
    if let Err(reason) = uze_git::fetch_private(primary, REMOTE, &source, &fetched)
        .map_err(|error| error.to_string())
        .and_then(|output| output.successful().map(|_| ()))
    {
        return TargetSync::Stalled { behind: 0, reason };
    }
    uze_git::locked(primary, uze_git::DEFAULT_WRITE_TIMEOUT, || {
        sync_target_locked(primary, target, &fetched)
    })
    .unwrap_or_else(|error| TargetSync::Stalled {
        behind: 0,
        reason: error.to_string(),
    })
}

pub(super) fn sync_target_locked(primary: &Path, target: &str, fetched: &str) -> TargetSync {
    let tracking = format!("refs/remotes/{REMOTE}/{target}");
    let private = format!("{}{fetched}", uze_git::PRIVATE_REFS);
    if let Err(reason) = git(primary, &["update-ref", &tracking, &private]) {
        return TargetSync::Stalled { behind: 0, reason };
    }
    if checkout::tip_of(primary, &tracking).is_empty() {
        return TargetSync::Unpublished;
    }
    // "Nothing behind" and "the question could not be asked" are different
    // answers, and the second is what a checkout that never fetched the
    // declared target gives — reported as current, it read as healthy while
    // every agent placed afterwards had no branch to start from.
    let Some(behind) = checkout::commits_ahead_checked(primary, target, &tracking) else {
        return TargetSync::Stalled {
            behind: 0,
            reason: format!("`{target}` does not exist in this checkout"),
        };
    };
    if behind == 0 {
        return TargetSync::Current;
    }
    let local_tip = checkout::tip_of(primary, target);
    if !is_ancestor(primary, &local_tip, &tracking) {
        return TargetSync::Stalled {
            behind,
            reason: format!("it carries commits `{REMOTE}` does not"),
        };
    }
    // Moving the target where the operator stands on it would change files
    // under their editor on a clock, with no gesture of theirs behind it.
    // Their checkout already says what a pull would bring; the sync only
    // moves a target nobody has checked out there.
    if checkout::current_branch(primary).as_deref() == Some(target) {
        return TargetSync::Stalled {
            behind,
            reason: "it is checked out in the primary checkout, where a pull moves it".to_owned(),
        };
    }
    match fast_forward(primary, target, &tracking) {
        Ok(()) => TargetSync::FastForwarded { commits: behind },
        Err(reason) => TargetSync::Stalled { behind, reason },
    }
}

/// Moves the local target onto `source` — the remote's tracking ref when a
/// sync brings the target in line, the task's branch when a delivery lands
/// it. Through the working tree when the operator is standing on the target
/// — Git's own fast-forward, which refuses rather than overwrite anything
/// uncommitted in the way — and by moving the ref when they are not.
///
/// Which of the two it is has to be asked, because `git merge` advances
/// `HEAD` and not the named target: run against a detached `HEAD` it
/// succeeds while the target never moves, and run on any other branch that
/// is an ancestor of `source` it fast-forwards *that* branch instead.
pub(super) fn fast_forward(primary: &Path, target: &str, source: &str) -> Result<(), String> {
    if checkout::current_branch(primary).as_deref() == Some(target) {
        git(primary, &["merge", "--quiet", "--ff-only", "--", source])?;
    } else {
        move_ref_forward(primary, target, source)?;
    }
    // `git merge` advances whatever HEAD is, so the one thing that matters —
    // that the target now names the source's commit — is read back rather
    // than assumed.
    let moved = checkout::tip_of(primary, target);
    let expected = checkout::tip_of(primary, source);
    if moved.is_empty() || moved != expected {
        return Err(format!(
            "`{target}` did not move onto `{source}`; it still points at \
             {landed}",
            landed = if moved.is_empty() {
                "nothing".to_owned()
            } else {
                moved
            }
        ));
    }
    Ok(())
}

/// Moves `refs/heads/<target>` onto `source` as a compare-and-swap: only
/// from the commit that was read here, only to a descendant of it, and
/// never under another checkout that has the branch out. The write lock is
/// UZE's alone, so an operator's `git commit` on the target in another
/// checkout, or a fetch, can land between the read and the write; Git then
/// refuses the swap rather than drop what landed.
fn move_ref_forward(primary: &Path, target: &str, source: &str) -> Result<(), String> {
    let from = checkout::tip_of(primary, target);
    let onto = checkout::tip_of(primary, source);
    if from.is_empty() || onto.is_empty() {
        return Err(format!("`{target}` or `{source}` names no commit"));
    }
    if !is_ancestor(primary, &from, &onto) {
        return Err(format!(
            "`{source}` does not descend from `{target}`; only a fast-forward moves it"
        ));
    }
    let reference = format!("refs/heads/{target}");
    if let Some((holder, _)) = checkout::linked_worktrees(primary)
        .into_iter()
        .find(|(_, branch)| branch.as_deref() == Some(target))
    {
        return Err(format!(
            "`{target}` is checked out in {}, which a moved ref would leave behind",
            holder.display()
        ));
    }
    git(
        primary,
        &[
            "update-ref",
            "-m",
            &format!("uze: fast-forward {target} onto {source}"),
            &reference,
            &onto,
            &from,
        ],
    )
    .map(|_| ())
}
