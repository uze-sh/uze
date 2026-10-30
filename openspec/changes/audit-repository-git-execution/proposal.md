## Why

The workspace treated what a repository's Git configuration and a harness's
session store say as UZE's own, while every agent can write both:

- **WKS-2**: UZE's own Git writes and fetches (placement, delivery, sync,
  publish) ran the hooks, filters and transports the repository's
  configuration names, outside any sandbox and before ADR-055's approval.
  Breaks: *a Git operation UZE starts runs nothing the repository's
  configuration names.*
- **WKS-1**: a session id read back from a harness's store reached the
  relaunch's argv as an option. Breaks: *nothing read back from a harness
  reaches argv as an option.*
- **WKS-3**: the push URL read from Git config reached `git ls-remote` as an
  option. Breaks: *nothing read back from Git reaches argv as an option.*

## What Changes

- Every Git process UZE starts, read or write, runs with no hook (hooks are
  not folded into the approval: they never run in UZE's own operations), no
  file-system monitor, no `ext::` transport and no submodule recursion.
- Where an operation can reach a program the configuration names — a
  filter, a merge driver, a credential helper, an SSH or askpass command, a
  proxy, an upload/receive-pack, a signing program — only the operator's
  own (system, global, environment) configuration counts. What the
  repository's own (local, worktree) configuration names there is
  overridden with the operator's value or an inert one.
- A session id is a validated type: no leading `-`, no whitespace, control
  character or path separator. Each integration also requires its
  harness's real shape before an id read back from the harness is used.
- A value read back from Git and re-passed to Git goes after `--` or
  `--end-of-options`.

## Capabilities

### New Capabilities

### Modified Capabilities
- `worktree-policy`: the workspace's Git operations run nothing the
  repository's configuration names, and values read from Git never reach
  Git as options.
- `agent-session-continuity`: a conversation id from a harness's store is
  validated before it is recorded or reaches a launch.

## Impact

- `crates/uze-git`: one hardening set for every invocation, and the
  configuration-scoped override for operations that can run a program.
- `crates/uze-workspace/src/landing/request.rs`: `ls-remote` end of options.
- `crates/uze-core/src/delivery/session.rs`: `SessionId::parse`.
- `crates/uze-integrations/src/{claude,codex,opencode,antigravity}/session.rs`:
  per-harness id shapes.
- Operators who relied on a repository-local hook (`pre-push`,
  `post-checkout`, a Git LFS hook) inside UZE's own operations, or a
  repository-local `core.sshCommand`, see them no longer run there; the
  approved `workspace.gate`, and global configuration (`includeIf
  "gitdir:"`), are where they belong.
