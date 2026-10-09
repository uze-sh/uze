//! Accounting for every checkout Git knows: whose it is, and what state it is in.

use super::*;

/// Every linked worktree Git registers, wherever it is, with the branch it
/// has checked out — the primary checkout left out. Read from `git worktree
/// list`, so a directory that exists but was never registered is none.
pub fn linked_worktrees(primary: &Path) -> Vec<(PathBuf, Option<String>)> {
    let Some(listing) = crate::git::read(primary, &["worktree", "list", "--porcelain"])
        .ok()
        .and_then(|output| output.successful().ok())
    else {
        return Vec::new();
    };
    let mut checkouts = Vec::new();
    let mut current: Option<(PathBuf, Option<String>)> = None;
    // The first entry Git lists is always the main worktree.
    let mut main = true;
    for line in listing.lines().chain(std::iter::once("")) {
        if let Some(path) = line.strip_prefix("worktree ") {
            current = Some((uze_git::native_path(path), None));
        } else if let Some(reference) = line.strip_prefix("branch ")
            && let Some(entry) = current.as_mut()
        {
            entry.1 = Some(
                reference
                    .strip_prefix("refs/heads/")
                    .unwrap_or(reference)
                    .to_owned(),
            );
        } else if line.is_empty()
            && let Some(entry) = current.take()
            && !std::mem::take(&mut main)
            && entry.0.is_dir()
        {
            checkouts.push(entry);
        }
    }
    checkouts.sort();
    checkouts
}

/// Who a worktree of the project belongs to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Owner {
    /// A slot UZE made for an agent.
    Agent,
    /// A checkout UZE made for one of an agent's subagents.
    Subagent { parent: AgentId },
    /// A harness's own isolation, found where its integration says that
    /// harness keeps worktrees.
    Harness { harness: String },
    /// UZE made it, but its record is one this build cannot read.
    Unreadable,
    /// Everyone else's: a person's, or one an agent made by hand.
    Operator,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountedCheckout {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub owner: Owner,
}

/// Every linked worktree of the repository, wherever it is, with its owner.
/// `harness_dirs` pairs a harness with where, relative to any checkout of
/// the project, it keeps worktrees of its own.
pub fn account(primary: &Path, harness_dirs: &[(&str, &str)]) -> Vec<AccountedCheckout> {
    let linked = linked_worktrees(primary);
    let container = primary.join(WORKTREES_DIRECTORY);
    let roots: Vec<&Path> = std::iter::once(primary)
        .chain(linked.iter().map(|(path, _)| path.as_path()))
        .collect();
    linked
        .iter()
        .map(|(path, branch)| {
            let harness = harness_dirs.iter().find(|(_, directory)| {
                roots
                    .iter()
                    .any(|root| uze_platform::path::is_within(path, &root.join(directory)))
            });
            let directly_under = |container: &Path| {
                path.parent()
                    .is_some_and(|parent| uze_platform::path::same_path(parent, container))
            };
            let owner = match (harness, directly_under(&container)) {
                (Some((harness, _)), _) => Owner::Harness {
                    harness: (*harness).to_owned(),
                },
                (None, true) => match record::read(primary, path) {
                    Recorded::Ours(CheckoutRecord {
                        parent: Some(parent),
                        ..
                    }) => Owner::Subagent { parent },
                    Recorded::Ours(_) => Owner::Agent,
                    Recorded::Unreadable => Owner::Unreadable,
                    Recorded::Absent => Owner::Operator,
                },
                (None, false) => Owner::Operator,
            };
            AccountedCheckout {
                path: path.clone(),
                branch: branch.clone(),
                owner,
            }
        })
        .collect()
}

/// The linked worktrees directly under the isolation directory: the only
/// place a slot can be, and so the only place a record is honoured.
pub(super) fn isolated_checkouts(primary: &Path) -> Vec<(PathBuf, Option<String>)> {
    let container = primary.join(WORKTREES_DIRECTORY);
    linked_worktrees(primary)
        .into_iter()
        .filter(|(path, _)| {
            path.parent()
                .is_some_and(|parent| uze_platform::path::same_path(parent, &container))
        })
        .collect()
}

pub(super) fn slot_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(super) fn slot_state(
    primary: &Path,
    tips: &BranchTips,
    path: &Path,
    branch: Option<&str>,
    id: &CheckoutId,
    store: &AgentStore,
    presence: &Presence,
) -> SlotState {
    let owner = store.slot_owner(id);
    let isolation = owner.and_then(Agent::isolation);
    let somebody_inside = presence.inside(path);
    if let Some(owner) = owner
        && (is_live(&owner.state) || somebody_inside)
    {
        return SlotState::Occupied {
            task: owner.id.clone(),
        };
    }
    // Somebody at work in a directory no agent claims is still somebody
    // at work there; only the operator moves it on.
    if somebody_inside {
        return SlotState::Parked;
    }
    // An agent that ended with a child still holding work keeps its own
    // checkout: the child's work reaches the target only through it.
    let keeps_a_child = owner.is_some_and(|owner| {
        store.agents.iter().any(|child| {
            child.parent.as_ref() == Some(&owner.id) && child.state == WorkState::Parked
        })
    });
    if keeps_a_child {
        return SlotState::Parked;
    }
    // Commits before the working tree, though either parks the slot: the
    // integration answer is remembered and a status is not, and a pool is
    // mostly slots parked for their commits, so asking this first is what
    // keeps placing an agent from reading every working tree it owns.
    let declared_done = owner.is_some_and(|owner| owner.state == WorkState::Integrated);
    let target = isolation.map(|isolation| isolation.target.as_str());
    let holds_commits = match (branch, target) {
        (Some(branch), Some(target)) => !is_integrated_among(tips, primary, target, branch),
        (Some(branch), None) => !is_integrated(primary, "HEAD", branch),
        (None, _) => holds_unbranched_commits(path),
    };
    if (holds_commits && !declared_done) || holds_uncommitted_work(path) {
        SlotState::Parked
    } else {
        SlotState::Free
    }
}

/// Whether a detached `HEAD` in `path` carries commits no branch reaches.
/// Nothing but this checkout points at them, so reusing or removing it is
/// what would lose them. A question Git could not answer is taken as yes,
/// as [`is_integrated`] takes it.
pub fn holds_unbranched_commits(path: &Path) -> bool {
    crate::git::read(
        path,
        &["rev-list", "--max-count=1", "HEAD", "--not", "--branches"],
    )
    .ok()
    .and_then(|output| output.successful().ok())
    .is_none_or(|unbranched| !unbranched.trim().is_empty())
}

pub(super) fn modified_at(path: &Path) -> SystemTime {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Excludes the isolation directory through `.git/info/exclude`, which
/// belongs to the local repository and never to the operator's tree: the
/// primary's status stays exactly what the operator left, and `git add -A`
/// there never sweeps a slot in as an embedded repository. Idempotent.
pub fn exclude_isolation_directory(primary: &Path) -> Result<(), AcquireError> {
    let common = uze_git::repository::common_dir(primary).map_err(AcquireError::Git)?;
    let exclude = common.join("info").join("exclude");
    let entry = format!("/{WORKTREES_DIRECTORY}/");
    // Bytes, not text: the file is the operator's, and a line this build
    // cannot decode is still one it must hand back exactly as it was.
    let current = match fs::read(&exclude) {
        Ok(current) => current,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return Err(AcquireError::Git(format!(
                "could not read {}: {error}",
                exclude.display()
            )));
        }
    };
    if String::from_utf8_lossy(&current).lines().any(|line| {
        let line = line.trim();
        line == entry || line == format!("{WORKTREES_DIRECTORY}/") || line == WORKTREES_DIRECTORY
    }) {
        return Ok(());
    }
    if let Some(parent) = exclude.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AcquireError::Git(format!("could not create {}: {error}", parent.display()))
        })?;
    }
    let mut next = current;
    if !next.is_empty() && !next.ends_with(b"\n") {
        next.push(b'\n');
    }
    next.extend_from_slice(entry.as_bytes());
    next.push(b'\n');
    fs::write(&exclude, next).map_err(|error| {
        AcquireError::Git(format!("could not update {}: {error}", exclude.display()))
    })
}
