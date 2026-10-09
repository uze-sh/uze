//! A subagent's checkout, split from an agent and joined back into it.
//!
//! A child is a slot like any other, recorded as its agent's child, whose
//! target is its agent's branch: "has this child's work reached its agent"
//! is then the same question every slot is freed by, asked of a different
//! branch.

use std::path::{Path, PathBuf};

use super::{AcquireError, commits_ahead, git, tip_of};

/// What joining a child produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JoinOutcome {
    /// The child's commits are on the agent's branch. `split_at` is where
    /// the child now starts, when its commits had to be replayed.
    Joined {
        commits: usize,
        split_at: Option<String>,
    },
    /// Replaying the child's commits stopped on these files; the replay is
    /// paused in the child's checkout.
    Conflicted { paths: Vec<PathBuf> },
}

/// Whether a rebase or a merge is under way in `root`: a checkout in the
/// middle of either has no commit to split from.
pub fn operation_in_progress(root: &Path) -> bool {
    let Ok(git_dir) = uze_git::repository::git_dir(root) else {
        return true;
    };
    ["rebase-merge", "rebase-apply", "MERGE_HEAD"]
        .iter()
        .any(|state| git_dir.join(state).exists())
}

/// Brings the commits `child_branch` made after `split_at` onto the branch
/// checked out in `parent`, with no merge commit: fast-forward when the
/// agent has not moved since, otherwise a replay onto its current commit in
/// the child's checkout, then the fast-forward. A merge commit would be
/// dropped by the next rebase of the agent's branch, and with it any
/// conflict resolved inside it; a replay from the split point never brings
/// back the agent's own commits from before a rebase.
pub fn join(
    primary: &Path,
    parent: &Path,
    child: &Path,
    child_branch: &str,
    split_at: &str,
) -> Result<JoinOutcome, AcquireError> {
    // Read back from a record in the child's administrative directory, so
    // it is held to the one shape UZE writes there before Git sees it.
    if !is_object_id(split_at) {
        return Err(AcquireError::Git(format!(
            "the recorded split point `{split_at}` is not a commit id"
        )));
    }
    for checkout in [parent, child] {
        super::anchored(primary, checkout).map_err(AcquireError::Git)?;
    }
    uze_git::locked(primary, uze_git::DEFAULT_WRITE_TIMEOUT, || {
        let head = tip_of(parent, "HEAD");
        let commits = commits_ahead(parent, &head, child_branch);
        if commits == 0 {
            return Ok(JoinOutcome::Joined {
                commits,
                split_at: None,
            });
        }
        let replayed = if is_ancestor(parent, &head, child_branch) {
            None
        } else {
            if git(
                child,
                &[
                    "rebase",
                    "--quiet",
                    "--onto",
                    &head,
                    "--end-of-options",
                    split_at,
                    child_branch,
                ],
            )
            .is_err()
            {
                return match crate::landing::paused_rebase(child) {
                    Some(paths) => Ok(JoinOutcome::Conflicted { paths }),
                    None => Err(AcquireError::Git(
                        "the child's commits could not be replayed onto the agent's".to_owned(),
                    )),
                };
            }
            Some(head)
        };
        git(
            parent,
            &[
                "merge",
                "--quiet",
                "--ff-only",
                "--end-of-options",
                child_branch,
            ],
        )?;
        Ok(JoinOutcome::Joined {
            commits,
            split_at: replayed,
        })
    })
    .map_err(|error| AcquireError::Git(error.to_string()))?
}

fn is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> bool {
    crate::git::read(
        root,
        &["merge-base", "--is-ancestor", "--", ancestor, descendant],
    )
    .is_ok_and(|output| output.is_success())
}

/// A full object id, in either hash Git names objects by: what `rev-parse`
/// prints and the only thing a split point is ever recorded as.
fn is_object_id(candidate: &str) -> bool {
    matches!(candidate.len(), 40 | 64) && candidate.bytes().all(|byte| byte.is_ascii_hexdigit())
}
