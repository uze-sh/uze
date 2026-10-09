//! Publishing a branch and finding the request it already has.

use super::*;

/// Publishes the branch and says whether the forge already has a request
/// open for it.
///
/// The push is all UZE does here. Opening the request is deliberately not
/// automated: a title and a description are the change's argument, and
/// the agent that wrote the change is the only party holding it — a
/// generated one-liner is a worse request than none. It is also the only
/// half that is not portable, since every forge opens a request its own
/// way, while a push is a push.
pub(super) fn publish(
    primary: &Path,
    isolation: &mut Isolation,
) -> Result<Delivered, DeliveryFailure> {
    let published = publication(primary, isolation);
    let name = published
        .as_ref()
        .map(|published| published.branch.clone())
        .or_else(|| isolation.published_as.clone())
        .unwrap_or_else(|| {
            // A branch outside UZE's namespace was named by somebody, and
            // a chosen name is never replaced by a derived one. A derived
            // name somebody already has on the remote is not taken either.
            if isolation.branch.starts_with(crate::worktree::BRANCH_PREFIX) {
                let derived = readable_branch_name(primary, isolation);
                if derived != isolation.branch && checkout::name_is_taken(primary, &derived) {
                    isolation.branch.clone()
                } else {
                    derived
                }
            } else {
                isolation.branch.clone()
            }
        });
    let refspec = format!("{}:refs/heads/{name}", isolation.branch);
    let tip = checkout::tip_of(primary, &isolation.branch);
    // A branch already on the remote is one a delivery has since rebased,
    // so its history no longer descends from what the remote holds and a
    // plain push is refused. It is overwritten only when the remote still
    // holds exactly the commit UZE pushed there last, and the lease names
    // that commit rather than the remote-tracking ref: a tracking ref says
    // what the remote held at the last fetch, not who put it there, and a
    // colleague's branch of the same name passes that test. Anything else
    // on the remote is pushed to only by fast-forward.
    let lease = isolation
        .published_tip
        .as_ref()
        .filter(|_| published.is_some() && isolation.published_as.as_deref() == Some(&name))
        .map(|pushed| format!("--force-with-lease=refs/heads/{name}:{pushed}"));
    let mut push = vec!["push", "--quiet"];
    push.extend(lease.as_deref());
    push.extend([REMOTE, refspec.as_str()]);
    git(primary, &push).map_err(|reason| {
        DeliveryFailure::Git(if published.is_some() && lease.is_none() {
            format!(
                "`{REMOTE}/{name}` holds commits UZE did not push there, and is only ever \
                 fast-forwarded: {reason}"
            )
        } else {
            reason
        })
    })?;
    isolation.published_tip = (!tip.is_empty()).then_some(tip);
    isolation.published_as = Some(name.clone());
    isolation.forget_request_unless_for(Some(&name));
    if isolation.published_request.is_none() {
        isolation.published_request =
            discover_request(primary, &checkout::tip_of(primary, &isolation.branch));
        isolation.request_branch = isolation.published_request.map(|_| name.clone());
    }
    match isolation.published_request {
        Some(request) => Ok(Delivered::Published {
            branch: name,
            request,
        }),
        None => Ok(Delivered::AwaitingRequest {
            instruction: open_request_message(forge(primary), isolation, &name),
            branch: name,
        }),
    }
}

/// The number of the request open for the published branch, asked of the
/// remote itself.
///
/// Forges publish a request's head as a ref, so `ls-remote` answers this
/// with no CLI, no token beyond the one the push already used, and no
/// knowledge of which forge is on the other end: whichever namespace the
/// remote serves is the one that matches. A request is identified by the
/// commit it points at, since a ref under these namespaces carries a
/// number and nothing else.
///
/// `None` is the ordinary answer the first time — no request exists yet —
/// and stays the answer on a forge that publishes no such refs, where the
/// branch is still pushed and the sync still works, only unnumbered.
pub(super) fn discover_request(primary: &Path, tip: &str) -> Option<u32> {
    if tip.is_empty() {
        return None;
    }
    let listing = crate::git::read(
        primary,
        &[
            "ls-remote",
            REMOTE,
            "refs/pull/*/head",
            "refs/merge-requests/*/head",
        ],
    )
    .ok()?
    .successful()
    .ok()?;
    listing
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(|(sha, _)| *sha == tip)
        .filter_map(|(_, reference)| {
            reference
                .strip_prefix("refs/pull/")
                .or_else(|| reference.strip_prefix("refs/merge-requests/"))?
                .strip_suffix("/head")?
                .parse()
                .ok()
        })
        // Two requests over the same commits is unusual and the newest is
        // the one being worked on; taking the smaller would pin the task
        // to a request somebody already superseded.
        .max()
}

/// The message written into the owning agent's pane when its branch is
/// published and the request is still its to open.
///
/// Names the forge's own word once the remote has said which forge this
/// is, and names both when it has not — the projects that reach a forge
/// this cannot recognize are the reason the second half still exists.
/// Names no tool either way: the agent is in the repository and knows
/// which one this is.
pub(super) fn open_request_message(forge: Forge, isolation: &Isolation, branch: &str) -> String {
    let request = match forge.request_term() {
        Some(term) => format!("a {term}"),
        None => "a pull request — a merge request, on a forge that calls it that —".to_owned(),
    };
    format!(
        "Your branch is published as `{branch}` on `{REMOTE}`, rebased onto `{target}` and past \
         the project's checks. Open {request} from `{branch}` against `{target}`, naming and \
         describing it by this project's own convention. Do not merge it and do not integrate \
         the branch yourself: open the request and end your turn.",
        target = isolation.target,
    )
}

/// Files changed on `branch` since `tip` that the primary checkout has
/// uncommitted changes to — the one case a fast-forward would collide with
/// the operator.
pub(super) fn overlapping_files(primary: &Path, tip: &str, branch: &str) -> Vec<PathBuf> {
    let changed: Vec<String> = crate::git::read(
        primary,
        &["diff", "--name-only", "--end-of-options", tip, branch, "--"],
    )
    .ok()
    .and_then(|output| output.successful().ok())
    .map(|stdout| stdout.lines().map(str::to_owned).collect())
    .unwrap_or_default();
    let dirty: Vec<String> =
        crate::git::read(primary, &["status", "--porcelain", "--untracked-files=no"])
            .ok()
            .and_then(|output| output.successful().ok())
            .map(|stdout| {
                stdout
                    .lines()
                    .filter_map(|line| line.get(3..))
                    .map(|path| path.trim().trim_matches('"').to_owned())
                    .collect()
            })
            .unwrap_or_default();
    changed
        .into_iter()
        .filter(|path| dirty.iter().any(|dirty| dirty == path))
        .map(PathBuf::from)
        .collect()
}

pub(super) fn is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> bool {
    crate::git::read(
        root,
        &["merge-base", "--is-ancestor", "--", ancestor, descendant],
    )
    .is_ok_and(|output| output.is_success())
}

pub(super) fn has_remote(root: &Path) -> bool {
    crate::git::read(root, &["remote", "get-url", REMOTE]).is_ok_and(|output| output.is_success())
}
