Audited commit: 2ae15624 · Date: 2026-11-05 · Found by: `mutation-audit`, while triaging a mutation audit of `uze-workspace`, then reading the code · Advisory: GHSA-2m3v-7xj9-hx9g

## WKS-1 - A commit on a project's target can make UZE create a declared link, and directories, outside the agent's checkout

- **Severity**: Medium. The operator's linked file, typically secrets, gets a
  new name in a directory the committer chooses, anywhere the operator can
  write; a location something serves, syncs, uploads or mounts exposes it,
  and a writer there writes the operator's primary file.
- **Status**: CONFIRMED by a test in a scratch repository, on Unix.
- **Threat actor**: someone who can land a commit on the project's target
  branch.
- **Preconditions**: the project declares a `workspace.link` entry such as
  `config/.env`, ignored and present in the operator's primary, where
  `config/` is a real directory there. `workspace.link` is not behind the
  workspace approval.
- **Location**: `crates/uze-workspace/src/checkout/pool.rs`, `materialize`
  (lines 597-616); `crates/uze-workspace/src/checkout/lifecycle.rs`,
  `carry_untracked`, for the same pattern on the copy of untracked changes.
- **Attack**: the committer turns `config` into a symlink to a directory of
  their choice, absolute or relative. On the operator's next agent placement,
  which cuts the slot from the remote tip, UZE creates a symlink to the
  operator's primary `config/.env` in that directory, and creates directories
  there. Nothing is overwritten: an existing destination is skipped.
- **Evidence**: after placement, a link to the primary's `config/.env` and a
  new directory existed outside the slot, with no warning:
  `made outside the checkout: [".../elsewhere/sub", ".../elsewhere/.env"]`.
- **Root cause**: `materialize` checks the link's name (no absolute path, no
  `..`) and asks `git check-ignore` in the primary, but never checks where the
  destination resolves inside the slot, whose tree is the target's and not the
  primary's. `exists()` and `create_dir_all` follow symlinks. `carry_untracked`
  makes and writes the same way, guarded only by the slot being cut from the
  primary's own `HEAD`.

## Defenses that held

- A link's name is refused when it is absolute or climbs out with `..`.
- A link to a file the primary does not ignore is refused when the policy is read.
- An existing destination is skipped, so nothing was overwritten.
- Windows checks out a committed symlink as a file by default (not tested).
