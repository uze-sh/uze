## Context

See proposal.md for why. `landing::sync_target`
(`crates/uze-workspace/src/landing/sync.rs`) fetches `origin/<target>` into
`refs/uze/sync/<target>`, copies it to `refs/remotes/origin/<target>`, and
fast-forwards the local target only when nobody has it checked out.
`place_in_slot` (`crates/uze-application/src/application/services/tasks/placement.rs`)
cuts every new agent from `tip_of(primary, target)`, the local branch.
`isolate` cuts from the primary checkout's `HEAD`.

Everything said about an agent's work afterwards names `isolation.target`,
the local branch, and nothing else:

- `checkout::is_integrated` / `is_integrated_among` (`checkout/integration.rs`),
  which every slot, branch and settle question goes through: `holds_work`,
  `slot_state`, `reconcile`, `collect`, `settle_delivered`, `resume_children`,
  the evaluation's handed-on check, the checkouts service;
- `Refs::carries` (`checkout/lifecycle.rs`), whether a shelf is in the target;
- `landing::effective_base` (`landing/request.rs`), what readiness counts
  commits from;
- `landing::target_tip` for `handoff`/`merge` (`landing/deliver.rs`), what
  `refresh` and a local delivery rebase onto, and what `rebase_in_slot`
  records as the new base commit.

An agent cut from a remote tip ahead of the local target carries the
remote's commits; asked against the local branch, every one of these counts
them as the agent's own — a slot with nothing in it is "holding work", an
agent with nothing committed is "ready", and the first refresh records the
local tip as its base.

## Goals / Non-Goals

**Goals:**
- One rule decides the commit a target stands at, and every judgement and
  the New placement read it; no caller special-cases an agent's kind.
- The placement path stays free of network calls: it reads only refs the
  sync already wrote.
- A repository without a remote, or with an unpublished target, behaves
  exactly as today.

**Non-Goals:**
- Changing the sync's clock, its fast-forward rules, or what it fetches.
- Changing `isolate`: "To worktree" still cuts from `HEAD`.
- Delivery under `pr`, which already fetches and rebases onto the remote.
- Readiness of an agent in the primary checkout (`readiness_of_checkout`),
  which is about the operator's branch and has no recorded target.
- Remotes other than `origin`: the sync only ever fetches `origin`.

## Decisions

### D1. The target's reach: the more advanced of its two tips

`checkout::target_reach(root, target, branch)` (and
`BranchTips::target_reach` for a pass that already read every tip) answers
the commit a target stands at, from `refs/heads/<target>` (L) and
`refs/remotes/origin/<target>` (R):

| L, R | answer |
|---|---|
| only one exists | that one |
| equal, or L is an ancestor of R | R |
| R is an ancestor of L | L |
| diverged, and `branch` descends from R but not from L | R |
| diverged otherwise, or no branch asked about | L |

So "the most advanced of the two", and on divergence "the one the agent's
branch was cut from", falling back to the local target — which is what the
placement asks (no branch), since a local target carrying commits the
remote lacks is the operator's own work and is never skipped.

A target name that is neither a local branch nor tracked (`HEAD`, a commit,
a path to a branch that is gone) has no reach and is resolved as Git
resolves it, exactly as today. That keeps `is_integrated(path, "HEAD", …)`
and the delivery's `is_integrated(primary, <commit>, …)` untouched.

**Alternative: judge against `base_commit`.** Simpler, and right for
"commits of its own" (which `effective_base` and the views already do), but
wrong for integration: the agent's work is never in the commit it started
from, so "is it in the target?" needs the target. And `base_commit` moves
on a rebase, on a delivery by patch, and on a rename's adoption; it does not
answer "where is the team".

**Alternative: always the remote when it exists.** Wrong on divergence: a
local target with an unpushed commit of the operator's, and an agent cut
from it, would see that commit as its own.

### D2. Applied inside the integration predicate, not at its callers

`is_integrated(root, target, branch)` and `is_integrated_among(tips, …)`
resolve `target` through D1 before asking. Every caller listed in Context
passes `isolation.target` (or a target name) and gets the rule without
being touched; the four the plan named — placement's `resume_children`,
`settle_delivered`'s two questions, `effective_base`, and
`commits_ahead_among`/`is_integrated` in `integration.rs` — all reach it
through here or through D3.

`commits_ahead_among` takes a commit, not a target name, and is always
asked from `base_commit` or a published tip; it is correct as soon as
`base_commit` is (D4) and is not changed.

### D3. The three readers that are not the predicate

- `Refs::carries(primary, shelf, target, branch)` resolves the target
  through `tips.target_reach`, with the shelf's agent's branch for the
  divergence case.
- `landing::effective_base` uses the reach instead of the local tip: the
  reach when it is an ancestor of the branch, `base_commit` otherwise. An
  agent cut from L while R is ahead (the "To worktree" case) has R not in
  its history, so its base stays L and only its own commits count.
- `landing::target_tip` for `handoff` and `merge` answers the reach.
  `refresh` and a local delivery then rebase onto it: an agent cut from R is
  already on it and records R as its base instead of rewinding to L; an
  agent cut from L is moved onto R, which is what the sync would have done
  to its target anyway, and a `merge` delivery still fast-forwards the local
  target, now including what the remote had — a fast-forward, on an
  explicit operator action, through Git's own `--ff-only`.

### D4. The New placement reads the reach; the record does not change

`place_in_slot` cuts from `target_reach(primary, target, None)`.
`Base::Ref(target)` keeps naming the `agents.yaml` target, and
`base_commit` records the commit used, so no record changes shape and no
ladder rung is needed. `isolate` is untouched.

### D5. The sync speaks only when a placement falls back

`TargetSync` gains `Diverged { behind }` for "the local target carries
commits the remote lacks", the only case in which D1 makes a New placement
start behind the remote. `concern` answers only for it. `Stalled` (checked
out, Git refusing the move, no local branch) no longer has a concern: the
placement is cut from R regardless. A fetch that failed is not reported
either: the placement uses the last fetched tip, and a delivery fetches for
itself and reports its own failure. The toast's title becomes "The local
target is behind its remote".

### D6. Ancestry is remembered per pair of commits

D1 asks `merge-base --is-ancestor` of two commits, at most twice, and the
evaluation asks it of every recorded agent on every pass. The answer of an
ancestry question between two commits never changes, so it is remembered
the way integration answers are (keyed by root and both commits, bounded,
cleared when full); only a complete answer is remembered. With no tracking
ref, or with L equal to R (the common case after a sync that could move the
target), no Git process is spent.

### D7. A released slot detached on the remote's tip is free

A slot is released detached where its agent left it, and a detached `HEAD`
carrying commits no branch reaches pins the slot, since reusing it would
lose them. An agent cut from the remote's tip that committed nothing leaves
its slot detached on commits only `refs/remotes/origin/*` reaches, and its
branch is pruned as integrated: asked against `--branches` alone, the slot
was pinned forever. `holds_unbranched_commits` excludes remote-tracking
refs too — a commit the remote has is not lost by reusing the checkout.

## Risks / Trade-offs

- [A judgement on a branch-named target, outside a pass's `BranchTips`,
  costs one more `for-each-ref`] → it reads two exact refs; passes keep
  asking through `is_integrated_among`, which reads nothing new.
- [A remote that rewinds the target (force-push) makes R an ancestor of
  nothing] → divergence falls back to L, as today.
- [Work merged on the forge now reads integrated before the operator
  pulls] → intended: the request flow already treats the remote as where
  work lands; a `merge` project integrates locally, where L is the more
  advanced tip and the answer does not change.
- [A refresh can now rebase a "To worktree" agent onto the remote's tip] →
  the same as the next sync that could move the target would cause; it is
  only run on a clean, ready checkout, and a conflict leaves the rebase
  paused for the agent, as every refresh does.
- [`fast-agent-placement` is open and renames the same requirement] → this
  change modifies it under the new name and archives after it.
