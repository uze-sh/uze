//! Whether a linked checkout is still the one its repository registered.
//!
//! A linked worktree finds its repository through the `.git` file at its
//! root, and that file is the checkout's own: whoever works in it can
//! point it, or a `.git` directory put in its place, at a repository of
//! their making. Git then reads *that* repository's configuration, and a
//! configuration names programs Git runs — a file-system monitor, a hook,
//! a filter — in whatever process asked, which for the workspace is one
//! outside any sandbox the agent runs in.
//!
//! So the administrative directory is found from the primary repository,
//! whose registry (`<common>/worktrees/<admin>/gitdir`) is what `git
//! worktree list` itself reads, and the checkout's `.git` file is only
//! checked against it. A checkout that no longer agrees is not run in.

use std::path::{Path, PathBuf};

use uze_core::path::Canonical as _;

/// The administrative directory the primary repository keeps for
/// `checkout`, when the checkout's own `.git` file still names it.
pub fn admin_dir(primary: &Path, checkout: &Path) -> Result<PathBuf, String> {
    let unanchored = |why: &str| {
        Err(format!(
            "{} is not a checkout of {}: {why}",
            checkout.display(),
            primary.display()
        ))
    };
    // The primary's `.git` is a directory and is the common directory
    // itself (see `worktree::primary_checkout`); taken from the
    // filesystem, so the answer never depends on running Git anywhere.
    let common = primary.join(".git");
    if !common.is_dir() {
        return unanchored("the project has no repository of its own to anchor it");
    }
    let Ok(pointer) = checkout.join(".git").canonical() else {
        return unanchored("it has no `.git`");
    };
    let Some(admin) = registered_admin(&common, &pointer) else {
        return unanchored("the repository does not register it");
    };
    if names(&checkout.join(".git")).as_deref() != Some(admin.as_path()) {
        return unanchored("its `.git` names another repository");
    }
    Ok(admin)
}

/// `Ok` when Git may be run in `checkout` on the workspace's behalf.
pub fn anchored(primary: &Path, checkout: &Path) -> Result<(), String> {
    admin_dir(primary, checkout).map(|_| ())
}

/// The entry of `<common>/worktrees/` whose `gitdir` file names `pointer`.
fn registered_admin(common: &Path, pointer: &Path) -> Option<PathBuf> {
    std::fs::read_dir(common.join("worktrees"))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|admin| {
            std::fs::read_to_string(admin.join("gitdir"))
                .ok()
                .map(|written| resolve(admin, written.trim()))
                .and_then(|named| named.canonical().ok())
                .is_some_and(|named| named == pointer)
        })
        .and_then(|admin| admin.canonical().ok())
}

/// The directory a `.git` *file* names. A `.git` directory, or a link in
/// its place, names nothing: neither is what Git leaves in a checkout it
/// adds.
fn names(dot_git: &Path) -> Option<PathBuf> {
    if !std::fs::symlink_metadata(dot_git).ok()?.is_file() {
        return None;
    }
    let pointer = std::fs::read_to_string(dot_git).ok()?;
    let named = pointer.lines().next()?.strip_prefix("gitdir:")?.trim();
    resolve(dot_git.parent()?, named).canonical().ok()
}

/// `written` as Git means it: relative to `base` unless it is absolute,
/// in this platform's spelling.
fn resolve(base: &Path, written: &str) -> PathBuf {
    base.join(uze_git::native_path(written))
}

/// `Err` when `root` lies in a checkout under the isolation directory whose
/// `.git` no longer names the project's repository. Lexical first, so a
/// path outside every slot costs nothing; a slot with no `.git` at all is
/// not refused, since Git walks up from it to the project's own.
pub fn guard(root: &Path) -> Result<(), String> {
    let Some(checkout) = crate::worktree::isolated_checkout(root) else {
        return Ok(());
    };
    let directory = checkout.directory();
    if std::fs::symlink_metadata(directory.join(".git")).is_err() {
        return Ok(());
    }
    anchored(checkout.primary, &directory)
}
