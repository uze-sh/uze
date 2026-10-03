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
        .unwrap_or_else(|| readable_branch_name(primary, isolation));
    let refspec = format!("{}:refs/heads/{name}", isolation.branch);
    // A branch already on the remote is one a delivery has since rebased,
    // so its history no longer descends from what the remote holds and a
    // plain push is refused. Whether it is there is asked of Git rather
    // than remembered: an agent that pushed the branch itself left UZE no
    // record to remember, and the refusal landed on the operator as a
    // failed delivery. `--force-with-lease` reads the same
    // remote-tracking ref this did, so the two agree on what is being
    // overwritten.
    let push = if published.is_some() {
        vec![
            "push",
            "--quiet",
            "--force-with-lease",
            REMOTE,
            refspec.as_str(),
        ]
    } else {
        vec!["push", "--quiet", REMOTE, refspec.as_str()]
    };
    git(primary, &push).map_err(DeliveryFailure::Git)?;
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
    let listing = uze_git::read(
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
    let changed: Vec<String> = uze_git::read(primary, &["diff", "--name-only", tip, branch, "--"])
        .ok()
        .and_then(|output| output.successful().ok())
        .map(|stdout| stdout.lines().map(str::to_owned).collect())
        .unwrap_or_default();
    let dirty: Vec<String> =
        uze_git::read(primary, &["status", "--porcelain", "--untracked-files=no"])
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
    uze_git::read(
        root,
        &["merge-base", "--is-ancestor", "--", ancestor, descendant],
    )
    .is_ok_and(|output| output.is_success())
}

pub(super) fn has_remote(root: &Path) -> bool {
    uze_git::read(root, &["remote", "get-url", REMOTE]).is_ok_and(|output| output.is_success())
}
