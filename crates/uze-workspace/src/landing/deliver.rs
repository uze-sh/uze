//! Delivering a checkout: rebasing it onto the target, running the gate and landing it.

use super::*;

/// Delivers a ready task according to `policy`, under the repository write
/// lock. Updates `task` to say what happened, whatever that was.
pub fn deliver(
    primary: &Path,
    agent: &mut Agent,
    policy: &Policy<'_>,
) -> Result<Delivered, DeliveryFailure> {
    uze_git::locked(primary, uze_git::DEFAULT_WRITE_TIMEOUT, || {
        deliver_locked(primary, agent, policy)
    })
    .map_err(|error| DeliveryFailure::Git(error.to_string()))?
}

pub(super) fn deliver_locked(
    primary: &Path,
    agent: &mut Agent,
    policy: &Policy<'_>,
) -> Result<Delivered, DeliveryFailure> {
    // Split rather than borrowed one at a time: the state and the branch
    // are disjoint fields of one record, and delivery writes both.
    let Agent {
        state, isolation, ..
    } = agent;
    let Some(isolation) = isolation.as_mut() else {
        // Only what UZE cut is UZE's to deliver. An agent in the project's
        // own root is on the operator's branch, and rebasing or pushing it
        // is theirs to ask for.
        return Err(DeliveryFailure::NotReady(Readiness::Running));
    };
    match readiness(primary, isolation) {
        Readiness::Ready { base, .. } => isolation.base_commit = base,
        other => return Err(DeliveryFailure::NotReady(other)),
    }
    let slot =
        slot_path(primary, isolation).ok_or(DeliveryFailure::NotReady(Readiness::Running))?;
    *state = WorkState::Integrating;
    let tip = target_tip(primary, isolation, policy.completion)?;
    // Merged elsewhere — squashed on the forge before the local target
    // heard of it — the work is in the tip under commits of its own, and
    // rebasing would replay it onto itself.
    if checkout::is_integrated(primary, &tip, &isolation.branch) {
        mark_delivered(primary, state, isolation);
        return Err(DeliveryFailure::AlreadyDelivered);
    }
    rebase_in_slot(primary, &slot, state, isolation, &tip)?;
    for step in policy.gate {
        let (passed, output) = run_shell_bounded(&slot, step, GATE_TIMEOUT);
        if !passed {
            *state = WorkState::GateFailed;
            return Err(DeliveryFailure::GateFailed {
                command: step.clone(),
                output,
            });
        }
    }
    match policy.completion {
        CompletionBehavior::Handoff => {
            *state = WorkState::Ready;
            Ok(Delivered::Handoff)
        }
        CompletionBehavior::Merge => {
            let overlap = overlapping_files(primary, &tip, &isolation.branch);
            if !overlap.is_empty() {
                *state = WorkState::Ready;
                return Err(DeliveryFailure::Overlap { files: overlap });
            }
            fast_forward(primary, &isolation.target, &isolation.branch).map_err(|reason| {
                *state = WorkState::Ready;
                DeliveryFailure::Git(format!("fast-forward refused: {reason}"))
            })?;
            *state = WorkState::Integrated;
            Ok(Delivered::Merged {
                target_tip: checkout::tip_of(primary, &isolation.target),
            })
        }
        CompletionBehavior::Pr => {
            let published = publish(primary, isolation)?;
            *state = WorkState::Ready;
            Ok(published)
        }
    }
}

/// Rebases a live task onto the target when the target has moved, under
/// the same rules as delivery. Only for a clean, ready checkout: never
/// under an agent mid-edit. Returns whether anything moved.
///
/// Always the local target, whatever the completion behaviour: this runs
/// whenever a pane goes quiet, and a fetch on that cadence would be paid
/// for on every tick of every task. `pr` reaches the remote's tip twice
/// anyway — where the target is brought in line before an agent is placed
/// on it, and at delivery, which is the one that decides.
/// Takes the state beside the branch rather than the whole agent: this
/// runs inside a pass that is already holding one, and two disjoint
/// fields borrowed apart is what lets it.
pub fn refresh(
    primary: &Path,
    state: &mut WorkState,
    isolation: &mut Isolation,
) -> Result<bool, DeliveryFailure> {
    uze_git::locked(primary, uze_git::DEFAULT_WRITE_TIMEOUT, || {
        let slot =
            slot_path(primary, isolation).ok_or(DeliveryFailure::NotReady(Readiness::Running))?;
        match readiness(primary, isolation) {
            Readiness::Ready { base, .. } => isolation.base_commit = base,
            // Nothing committed yet, but clean: following the target costs
            // the agent nothing.
            Readiness::Running => {}
            other => return Err(DeliveryFailure::NotReady(other)),
        }
        let tip = target_tip(primary, isolation, CompletionBehavior::Handoff)?;
        // The target already holds this work under commits of its own — a
        // squash or rebase merge. Replaying it there conflicts with itself.
        if checkout::is_integrated(primary, &tip, &isolation.branch) {
            return Ok(false);
        }
        rebase_in_slot(primary, &slot, state, isolation, &tip)
    })
    .map_err(|error| DeliveryFailure::Git(error.to_string()))?
}

/// Fetches `target` from the remote into its tracking ref and names that
/// ref. An explicit refspec, so the tracking ref moves whatever the
/// remote's configured fetch refspecs say.
pub(super) fn fetch_target(primary: &Path, target: &str) -> Result<String, String> {
    let tracking = format!("refs/remotes/{REMOTE}/{target}");
    let refspec = format!("+refs/heads/{target}:{tracking}");
    git(primary, &["fetch", "--quiet", REMOTE, &refspec])?;
    Ok(tracking)
}

/// The target's tip where the target lives: the remote's after a fetch when
/// delivery publishes a pull request, the local branch otherwise.
pub(super) fn target_tip(
    primary: &Path,
    isolation: &Isolation,
    completion: CompletionBehavior,
) -> Result<String, DeliveryFailure> {
    if completion == CompletionBehavior::Pr {
        if !has_remote(primary) {
            return Err(DeliveryFailure::NoRemote);
        }
        let tracking = fetch_target(primary, &isolation.target).map_err(DeliveryFailure::Git)?;
        let tip = checkout::tip_of(primary, &tracking);
        if tip.is_empty() {
            return Err(DeliveryFailure::Git(format!(
                "`{}` does not exist on `{REMOTE}`",
                isolation.target
            )));
        }
        return Ok(tip);
    }
    let tip = checkout::tip_of(primary, &isolation.target);
    if tip.is_empty() {
        return Err(DeliveryFailure::Git(format!(
            "`{}` has no commit",
            isolation.target
        )));
    }
    Ok(tip)
}

/// Rebases the task's branch onto `tip` in its checkout, and says whether
/// anything moved: a branch already on `tip` only records it as its base.
pub(super) fn rebase_in_slot(
    primary: &Path,
    slot: &Path,
    state: &mut WorkState,
    isolation: &mut Isolation,
    tip: &str,
) -> Result<bool, DeliveryFailure> {
    if tip == isolation.base_commit || is_ancestor(primary, tip, &isolation.branch) {
        isolation.base_commit = tip.to_owned();
        return Ok(false);
    }
    let moved = commits_ahead(primary, &isolation.base_commit, tip);
    // Work delivered by a squash or rebase merge sits below `base_commit`
    // on the branch, and in the target only under commits of its own:
    // replayed, it conflicts with itself. Only what came after it is this
    // task's to move.
    let base = isolation.base_commit.clone();
    let delivered_below = !base.is_empty()
        && is_ancestor(primary, &base, &isolation.branch)
        && !is_ancestor(primary, &base, tip);
    let rebase = if delivered_below {
        vec!["rebase", "--quiet", "--onto", tip, "--", base.as_str()]
    } else {
        vec!["rebase", "--quiet", "--", tip]
    };
    match uze_git::write(slot, &rebase) {
        Ok(output) if output.is_success() => {
            isolation.base_commit = tip.to_owned();
            Ok(true)
        }
        Ok(output) => {
            if let Some(files) = paused_rebase(slot) {
                *state = WorkState::Conflicted {
                    files: files.clone(),
                };
                Err(DeliveryFailure::Conflict {
                    files,
                    target_moved: moved,
                })
            } else {
                *state = WorkState::Ready;
                Err(DeliveryFailure::Git(output.stderr.trim().to_owned()))
            }
        }
        Err(error) => {
            *state = WorkState::Ready;
            Err(DeliveryFailure::Git(error.to_string()))
        }
    }
}

/// The files a paused rebase stopped on, or `None` when no rebase is
/// paused in `slot`.
pub fn paused_rebase(slot: &Path) -> Option<Vec<PathBuf>> {
    // One question, not one per name. Both states live directly under the
    // checkout's own Git directory — the linked worktree's, for a slot —
    // and asking Git where that is answers for both. This is read three
    // times per agent on every evaluation pass, which is the cadence that
    // makes the difference between one spawn and two worth having.
    let git_dir = uze_git::read(slot, &["rev-parse", "--git-dir"])
        .ok()
        .and_then(|output| output.successful().ok())?;
    let git_dir = Path::new(git_dir.trim());
    let git_dir = if git_dir.is_absolute() {
        git_dir.to_path_buf()
    } else {
        slot.join(git_dir)
    };
    let in_progress = ["rebase-merge", "rebase-apply"]
        .iter()
        .any(|kind| git_dir.join(kind).exists());
    if !in_progress {
        return None;
    }
    let files = uze_git::read(slot, &["diff", "--name-only", "--diff-filter=U"])
        .ok()
        .and_then(|output| output.successful().ok())
        .map(|stdout| stdout.lines().map(PathBuf::from).collect())
        .unwrap_or_default();
    Some(files)
}

/// Ends a task whose work the target already carries, when nothing of the
/// agent's is at stake. Returns whether it was ended.
///
/// A rebase paused in its checkout can only be replaying that work onto
/// itself, and is abandoned: the branch ref does not move until a rebase
/// finishes, so it still names every commit the agent made, and aborting
/// puts the checkout back exactly where the agent left it. A checkout with
/// changes of its own is never touched.
///
/// Only for a task that has something to settle — one parked, or with a
/// rebase paused: a branch with no commits of its own reads as integrated
/// too, and a live agent that has committed nothing yet is not done.
pub fn settle_delivered(primary: &Path, state: &mut WorkState, isolation: &mut Isolation) -> bool {
    if !checkout::branch_exists(primary, &isolation.branch)
        || !checkout::is_integrated(primary, &isolation.target, &isolation.branch)
    {
        return false;
    }
    if let Some(slot) = slot_path(primary, isolation) {
        if paused_rebase(&slot).is_some() && !abort_rebase(primary, &slot) {
            return false;
        }
        if is_dirty(&slot) {
            return false;
        }
    }
    mark_delivered(primary, state, isolation);
    true
}

/// Records a task's work as delivered by patch — a squash or rebase merge
/// the target carries under commits of its own. The branch's tip becomes
/// its base: everything up to it is in the target, so an agent that keeps
/// committing on the same branch is measured, and moved, by what it adds.
pub fn mark_delivered(primary: &Path, state: &mut WorkState, isolation: &mut Isolation) {
    *state = WorkState::Integrated;
    let tip = checkout::tip_of(primary, &isolation.branch);
    if !tip.is_empty() {
        isolation.base_commit = tip;
    }
}

pub(super) fn abort_rebase(primary: &Path, slot: &Path) -> bool {
    uze_git::locked(primary, uze_git::DEFAULT_WRITE_TIMEOUT, || {
        uze_git::write(slot, &["rebase", "--abort"]).is_ok_and(|output| output.is_success())
    })
    .unwrap_or(false)
}

/// The message written into the owning agent's pane when its task cannot be
/// rebased. One line, so a harness's prompt takes it as one submission.
pub fn conflict_message(isolation: &Isolation, files: &[PathBuf], target_moved: usize) -> String {
    format!(
        "Your branch no longer rebases onto {target}: conflicts in {files}. {target} gained \
         {moved} commit{plural} since you started. The rebase is paused in this checkout — \
         resolve the conflicts preserving the intent of your change, run `git rebase \
         --continue`, run the project's checks, and end your turn.",
        target = isolation.target,
        files = join_paths(files),
        moved = target_moved,
        plural = if target_moved == 1 { "" } else { "s" },
    )
}

/// The message written into the owning agent's pane when the gate refused
/// its rebased commits.
pub fn gate_failure_message(isolation: &Isolation, command: &str, output: &str) -> String {
    let tail: String = output
        .lines()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        "The project's checks failed on your branch after it was rebased onto {target}: \
         `{command}`. Fix them on this branch, commit, and end your turn. Last lines: {tail}",
        target = isolation.target,
    )
}
