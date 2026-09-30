//! Converging one UZE-owned region of a project's `AGENTS.md` toward what
//! its owner wants there. Shared by both owners of the file (the package
//! manager's authoring region, the workspace's policy region), because the
//! safety rules are `text_region`'s and must not be restated per owner.

use std::path::Path;

use uze_core::{context as instruction_context, integration::AttachmentState, text_region};

use super::{ManagedRegionPlan, ManagedRegionStatus};

/// The region an owner wants in the file: its identity and its bytes, or
/// `None` when it wants every region of its own gone.
pub(super) type Desired = Option<(String, String)>;

/// Writes the owner's region, removes the versions it superseded, and says
/// what happened. The caller holds the file's guard.
pub(super) fn converge(
    agents_md: &Path,
    owns: impl Fn(&str) -> bool,
    desired: Desired,
) -> ManagedRegionStatus {
    let wanted: Vec<(String, String)> = desired.into_iter().collect();
    let mut convergence = text_region::converge(agents_md, owns, &wanted);
    let (state, reason) = match convergence.desired.pop() {
        Some(region) => match region.write_failure {
            Some(failure)
                if !matches!(
                    region.inspection.state,
                    AttachmentState::Blocked | AttachmentState::Drifted
                ) =>
            {
                (AttachmentState::Blocked, failure)
            }
            _ => (region.inspection.state, region.inspection.reason),
        },
        None => (
            AttachmentState::Missing,
            "nothing is wanted in this file".to_owned(),
        ),
    };
    ManagedRegionStatus {
        file: agents_md.to_path_buf(),
        state,
        reason,
        removed_superseded: convergence.removed,
        blocked_superseded: convergence.blocked,
    }
}

/// What [`converge`] would do, without writing.
pub(super) fn plan(
    agents_md: &Path,
    owns: impl Fn(&str) -> bool,
    desired: &Desired,
    subject: &str,
) -> ManagedRegionPlan {
    let action = match desired {
        Some((identity, content)) => {
            let state = text_region::inspect(agents_md, identity, content).state;
            plan_action_for_region(true, state, subject)
        }
        None => instruction_context::PlannedAction::NoChange,
    };
    ManagedRegionPlan {
        file: agents_md.to_path_buf(),
        action,
        superseded: text_region::stale_regions(
            agents_md,
            owns,
            desired.iter().map(|(identity, _)| identity.as_str()),
        ),
    }
}

/// Whether the file carries exactly the region the owner wants: present
/// under the current identity and no superseded version beside it.
pub(super) fn in_step(agents_md: &Path, owns: impl Fn(&str) -> bool, desired: &Desired) -> bool {
    let present: Vec<String> = text_region::region_identities_present(agents_md)
        .into_iter()
        .filter(|identity| owns(identity))
        .collect();
    match desired {
        Some((identity, _)) => present == [identity.clone()],
        None => present.is_empty(),
    }
}

/// The action a managed region needs to reach its desired presence. Shared
/// by every region UZE owns in a project's files, because the safety rules
/// are `text_region`'s, not each caller's; `subject` only names the region
/// in a blocked explanation.
pub(super) fn plan_action_for_region(
    needed: bool,
    state: AttachmentState,
    subject: &str,
) -> instruction_context::PlannedAction {
    use instruction_context::PlannedAction;
    match (needed, state) {
        (true, AttachmentState::Matched) | (false, AttachmentState::Missing) => {
            PlannedAction::NoChange
        }
        (true, AttachmentState::Missing) => PlannedAction::Attach,
        (false, AttachmentState::Matched) => PlannedAction::Remove,
        (_, AttachmentState::Drifted) => PlannedAction::Blocked(format!(
            "{subject} content differs from what UZE would write"
        )),
        (_, AttachmentState::Blocked) => {
            PlannedAction::Blocked(format!("{subject} region markers are malformed"))
        }
        (_, AttachmentState::Conflict) => {
            PlannedAction::Blocked(format!("{subject} region ownership is ambiguous"))
        }
    }
}
