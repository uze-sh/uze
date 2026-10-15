## Why

An agent resumed into a different checkout loses its conversation, on every
harness. The conformance Lab proved it on all four: Claude Code keeps a
transcript under a directory named after the checkout, OpenCode ties a
session to its directory, and for Codex and Antigravity UZE learns the
conversation by observing the harness in the current directory. The pool
makes a different checkout the common case, not the edge: a slot an agent
leaves is the first one handed to the next agent, and an idle slot is
collected, so an agent resumed later almost never finds its own directory.
Two operator conversations were lost this way in one day.

Every harness keys its conversation by the directory, so the fix belongs to
the one thing UZE decides: where the checkout lives.

## What Changes

- An agent's checkout lives at a path that belongs to it for as long as UZE
  knows it: the directory it last held, or `.worktrees/<agent-id>` for an
  agent that never held one.
- The pool still reuses slots and their warm build artifacts: handing a free
  slot to an agent renames it to the agent's path with `git worktree move`,
  and the checkout record is rewritten in the same locked step.
- Resuming an agent lands in its own directory: kept if it still exists,
  otherwise a free slot is renamed to it, otherwise a checkout is created
  there. Every harness then continues the conversation it left.
- A slot an agent leaves keeps that agent's name until another agent takes
  it, so resuming soon after leaving moves nothing.
- When a slot cannot be renamed (an initialized submodule, a lock, a process
  working inside it), a checkout is created at the agent's path instead;
  nothing is copied.
- The project's `setup` already runs whenever an agent takes a slot; after a
  rename it is where caches that record their own path are repaired, and
  the docs say so.
- `crates/uze-application/build.rs` reads `CARGO_MANIFEST_DIR` at run time,
  so this repository builds after its checkout is renamed (a latent bug for
  any copy or rename of a checkout today).
- The conformance continuity contract gains a scene in which the agent's
  checkout is collected while it is away and the agent is resumed.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `worktree-policy`: an isolated checkout is named after the agent it
  belongs to, and reuse renames a slot to that name.
- `checkout-accounting`: a free slot keeps its last agent's name, trimming
  and collection work on slots whatever their name, and a checkout's record
  carries a move in progress so a crash never disowns it.
- `agent-session-continuity`: a relaunched agent continues its conversation
  even when its checkout was collected or reused while it was away, and
  conformance proves it.

## Impact

- `crates/uze-workspace/src/checkout/` (`pool.rs`: `take`, `reuse`,
  `create`, `resume`; `record.rs`: `moving_to`; `reconcile.rs`;
  `lifecycle.rs`).
- `crates/uze-application/src/application/services/tasks/placement.rs`
  and `services/work.rs` (placement, resume, subagent resume and split pass
  the agent's own path to the pool).
- Journeys that read slot names, and a `07-upgrade` scene.
- `crates/uze-application/build.rs`.
- `conformance/contract/continuity.py` and the four bindings and providers.
- Persisted: one additive optional field on the checkout record
  (`moving_to`); `isolation.checkout` keeps naming the checkout's
  directory.
