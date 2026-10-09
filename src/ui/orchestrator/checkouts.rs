//! Changes to a project's checkouts, as the work modal asks for them and
//! says what they came to, and the few facts about a checkout its rows are
//! drawn with.
//!
//! What is listed is read on a thread (`spawn_checkouts`) because it walks
//! every checkout to measure it, and every change runs on one too
//! (`spawn_checkout_change`); this module only carries the questions and
//! the answers.

use super::*;
use uze_application::{CheckoutOwner, CheckoutView, CheckoutsView, CleanUp, JoinedWork};

/// A change to the checkouts, as it travels to the thread that makes it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CheckoutChange {
    Adopt {
        path: PathBuf,
        name: String,
    },
    Remove {
        path: PathBuf,
        name: String,
    },
    /// An unfinished agent's subagent, joined into that agent.
    Join {
        parent_id: String,
        parent: String,
        topic: String,
    },
    CleanUp,
}

/// What reading the checkouts of `project` answered; `None` outside a
/// repository.
pub(super) struct CheckoutsResolution {
    pub(super) project: PathBuf,
    /// Which read this answers (see `Remembered::checkouts_asked`): only
    /// the project's latest is kept.
    pub(super) asked: u64,
    pub(super) view: Option<CheckoutsView>,
}

pub(super) struct CheckoutChangeResolution {
    pub(super) project: PathBuf,
    pub(super) outcome: CheckoutOutcome,
}

pub(super) enum CheckoutOutcome {
    Adopted {
        name: String,
        answer: std::result::Result<uze_application::AdoptedCheckout, String>,
    },
    Removed {
        name: String,
        answer: std::result::Result<uze_application::RemovedCheckout, String>,
    },
    Joined {
        topic: String,
        parent: String,
        answer: std::result::Result<JoinedWork, String>,
    },
    CleanedUp(CleanUp),
}

/// `812 KB`, `1.4 GB`: the unit that keeps the number short.
pub(super) fn bytes_to_say(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// `4m`, `3h`, `2d`: how long ago the checkout last changed.
pub(super) fn age_to_say(changed: Option<std::time::SystemTime>) -> Option<String> {
    let seconds = changed?.elapsed().ok()?.as_secs();
    Some(match seconds {
        0..60 => "now".to_owned(),
        60..3600 => format!("{}m", seconds / 60),
        3600..86400 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86400),
    })
}

/// Whether clean-up would remove `checkout` as the last read saw it: the
/// operator's own, clean, unused and in the target.
pub(super) fn cleaned_up(checkout: &CheckoutView) -> bool {
    checkout.owner == CheckoutOwner::Operator
        && checkout.removal_refusal.is_none()
        && checkout.in_target
        && !checkout.in_use
        && !checkout.dirty
}

/// The join a subagent's checkout offers: into its agent, once that agent
/// has ended unfinished. A running agent joins its own.
pub(super) fn join_of(checkout: &CheckoutView) -> Option<CheckoutChange> {
    match &checkout.owner {
        CheckoutOwner::Subagent {
            parent,
            parent_id,
            topic: Some(topic),
            joinable: true,
        } => Some(CheckoutChange::Join {
            parent_id: parent_id.clone(),
            parent: parent.clone(),
            topic: topic.clone(),
        }),
        _ => None,
    }
}

/// Why a join is not offered for `checkout`, said when it is asked for.
pub(super) fn join_refusal(checkout: &CheckoutView) -> String {
    match &checkout.owner {
        CheckoutOwner::Subagent { topic: None, .. } => {
            "no subagent holds it any more; remove it instead".to_owned()
        }
        CheckoutOwner::Subagent { parent, .. } => {
            format!("{parent} is still running, and joins its own subagents")
        }
        _ => "only a subagent's checkout joins into its agent".to_owned(),
    }
}

/// What a change came to, as the toast says it: the kind, a title and a
/// detail — both, since a toast worth raising says what it is about.
pub(super) fn describe_change(outcome: &CheckoutOutcome) -> (ToastKind, String, String) {
    match outcome {
        CheckoutOutcome::Adopted {
            name,
            answer: Ok(adopted),
        } => (
            ToastKind::Done,
            "adopted".to_owned(),
            if adopted.free {
                format!("{name} is UZE's now, and free for the next agent")
            } else {
                format!("{name} is UZE's now, and free once its work is kept")
            },
        ),
        CheckoutOutcome::Adopted {
            name,
            answer: Err(reason),
        } => (
            ToastKind::Failed,
            "not adopted".to_owned(),
            format!("{name}: {reason}"),
        ),
        CheckoutOutcome::Removed {
            name,
            answer: Ok(removed),
        } => (
            ToastKind::Done,
            "removed".to_owned(),
            format!(
                "{name}, branch kept · freed {}",
                bytes_to_say(removed.bytes)
            ),
        ),
        CheckoutOutcome::Removed {
            name,
            answer: Err(reason),
        } => (
            ToastKind::Failed,
            "not removed".to_owned(),
            format!("{name}: {reason}"),
        ),
        CheckoutOutcome::Joined {
            topic,
            parent,
            answer: Ok(JoinedWork::Joined { commits }),
        } => (
            ToastKind::Done,
            "joined".to_owned(),
            format!(
                "{topic} into {parent} · {commits} commit{}",
                if *commits == 1 { "" } else { "s" }
            ),
        ),
        CheckoutOutcome::Joined {
            topic,
            answer: Ok(JoinedWork::Conflicted { checkout, paths }),
            ..
        } => (
            ToastKind::Warned,
            "join paused".to_owned(),
            format!(
                "{topic} conflicts on {}; resolve it in {}, continue the rebase, and join again",
                paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                checkout.display()
            ),
        ),
        CheckoutOutcome::Joined {
            topic,
            answer: Err(reason),
            ..
        } => (
            ToastKind::Failed,
            "not joined".to_owned(),
            format!("{topic}: {reason}"),
        ),
        CheckoutOutcome::CleanedUp(clean_up) => describe_clean_up(clean_up),
    }
}

/// How many kept checkouts are named with their reason before the rest
/// are counted: a toast is a line, not a report.
const KEPT_NAMED: usize = 2;

fn describe_clean_up(clean_up: &CleanUp) -> (ToastKind, String, String) {
    let mut parts = Vec::new();
    if !clean_up.removed.is_empty() {
        parts.push(format!(
            "removed {} · freed {}",
            clean_up.removed.len(),
            bytes_to_say(clean_up.freed())
        ));
    }
    for kept in clean_up.kept.iter().take(KEPT_NAMED) {
        parts.push(format!("kept {}: {}", kept.name, kept.reason));
    }
    if clean_up.kept.len() > KEPT_NAMED {
        parts.push(format!("{} more kept", clean_up.kept.len() - KEPT_NAMED));
    }
    if !clean_up.left_to_harness.is_empty() {
        parts.push(format!(
            "{} left to their harness",
            clean_up.left_to_harness.len()
        ));
    }
    if clean_up.removed.is_empty() {
        let detail = if parts.is_empty() {
            "no checkout of yours is clean, unused and in the target".to_owned()
        } else {
            parts.join(" · ")
        };
        (ToastKind::Told, "nothing to clean up".to_owned(), detail)
    } else {
        (ToastKind::Done, "cleaned up".to_owned(), parts.join(" · "))
    }
}
