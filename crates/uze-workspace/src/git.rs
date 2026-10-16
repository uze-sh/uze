//! Git as the workspace runs it: through `uze-git`, and never in a slot
//! whose `.git` no longer names the project's repository.
//!
//! The check sits here rather than at each caller because every question
//! the workspace asks of a checkout — is it dirty, which branch, is a
//! rebase paused — runs Git in it, and whichever caller forgot the check
//! would be the one an agent steered. Refused, a call answers the way a
//! Git that could not run does, which every caller already treats as the
//! cautious answer: dirty, holding work, on no branch. Past the check Git
//! is handed the administrative directory that was checked, and never reads
//! the slot's `.git` again: one repointed between the check and Git's start
//! would otherwise be the repository Git ran.

use std::path::Path;

use crate::checkout::anchor_of;

pub(crate) fn read(root: &Path, args: &[&str]) -> Result<uze_git::Output, String> {
    match anchor_of(root)? {
        Some(anchor) => uze_git::read_pinned(root, anchor.pinned(), args),
        None => uze_git::read(root, args),
    }
    .map_err(|error| error.to_string())
}

pub(crate) fn write(root: &Path, args: &[&str]) -> Result<uze_git::Output, String> {
    match anchor_of(root)? {
        Some(anchor) => uze_git::write_pinned(root, anchor.pinned(), args),
        None => uze_git::write(root, args),
    }
    .map_err(|error| error.to_string())
}

pub(crate) fn write_with_stdin(
    root: &Path,
    args: &[&str],
    input: &str,
) -> Result<uze_git::Output, String> {
    match anchor_of(root)? {
        Some(anchor) => uze_git::write_pinned_with_stdin(root, anchor.pinned(), args, input),
        None => uze_git::write_with_stdin(root, args, input),
    }
    .map_err(|error| error.to_string())
}

pub(crate) fn write_with_env(
    root: &Path,
    args: &[&str],
    env: &[(&str, &str)],
) -> Result<uze_git::Output, String> {
    match anchor_of(root)? {
        Some(anchor) => uze_git::write_pinned_with_env(root, anchor.pinned(), args, env),
        None => uze_git::write_with_env(root, args, env),
    }
    .map_err(|error| error.to_string())
}
