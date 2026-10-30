## Why

The review of `fix/security-audit` (2026-10-30) left items that are not
structural and not yet decided. Three have a public part, recorded here.
Two are security findings still open, tracked privately and named here by
id only (AGENTS.md, Security findings). The operator asked for the
recommended answer to be recorded as the plan.

## What Changes

- **A client retires only a server that started inside a pane.** Today an
  unconfined client retires any server running under `no_new_privs`,
  together with every agent in it. A person who opened uze in a sandboxed
  terminal (Flatpak, a `NoNewPrivileges` unit) and later in an ordinary one
  loses their running agents. The server will record, when it starts,
  whether it was started from inside a pane, and only such a server is
  retired.
- **Delivery in a Git LFS repository says what it does not do.** UZE's own
  Git runs no hooks, so its delivery push does not upload LFS objects. A
  repository that uses LFS will be told so before the push. Delivery then
  stops with a next step: add the upload to the approved `workspace.gate`,
  or push from a terminal. A remote is never left pointing at objects it
  does not have.
- **The Windows installer's `latest` path is tested.** `install.ps1`
  resolves `latest` through a redirect, and no Pester case covers that path
  without `UZE_VERSION`.
- **Two open findings, tracked privately:** GHSA-w6jq-59jg-h4pg (High) and
  GHSA-97f7-95q3-wcjw (Medium). Each is fixed as its own `audit-<subject>`
  change. That change enters the repository with its fix, never before.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `terminal-runtime`: which server a client may retire.
- `worktree-policy`: delivery in a repository that uses Git LFS.

## Impact

- `crates/uze-terminal/src/runtime/{endpoint,server}.rs`, plus a field in the
  runtime's persisted state or handshake. That is a protocol or format
  change, so design.md covers the version.
- `crates/uze-workspace/src/landing/{deliver,publish}.rs`, and the TUI and
  CLI text for the stop (design-system skill).
- `tests/scripts/installer.Tests.ps1`.
