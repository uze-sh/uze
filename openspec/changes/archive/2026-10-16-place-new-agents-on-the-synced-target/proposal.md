## Why

A new agent is cut from the local target, and the target sync never moves a
local target the operator has checked out — which is where the operator
usually stands. The sync still fetches the remote's target every three
minutes, so the machine knows where the team is, and then places every new
agent somewhere else: on this repository an agent was born seven commits
behind after a day of merges. Delivery rebases onto the remote's tip, so the
divergence surfaces as a rebase conflict at the most expensive moment there
is.

## What Changes

- **A new agent is cut from the most advanced tip of its target this machine
  already has.** When the remote-tracking ref the last sync wrote descends
  from the local target, the new agent's branch is cut from it; when the
  local target carries commits the remote lacks, there is no remote, or the
  target is unpublished, it is cut from the local target as today. The
  placement still asks the remote nothing. The recorded base names the
  declared target, and the base commit records the commit actually used.
- **An agent moved into a checkout of its own keeps today's rule.** "To
  worktree" and "To worktree, no changes" cut from the primary checkout's
  `HEAD`, because they continue the operator's own work.
- **Every judgement about an agent's work asks the same target.** Whether a
  branch is integrated, whether a slot or a branch still holds work, whether a
  shelf is carried, what an agent is ahead of, and what a refresh rebases
  onto, are asked against the most advanced of the local target and its
  remote-tracking ref — on divergence, the one the agent's branch was cut
  from. One rule, in the integration predicate, for every kind of agent.
- **The sync says something only when a placement falls back.** A target the
  operator stands on, one Git refuses to move, or one with no local branch no
  longer starts new agents behind, so nothing is said about it. A local
  target carrying commits the remote lacks is reported as such, and the toast
  says the local target is behind instead of that new agents start behind.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `worktree-policy`: "An agent is placed on the target as the last sync left
  it" (the name `fast-agent-placement` gives it) cuts a new agent from the
  most advanced tip the last sync left, and states when the sync speaks; a new
  requirement says what an agent's work is judged against.

## Impact

- `uze-workspace`: `checkout::integration` gains the one rule (`target_reach`)
  and applies it in `is_integrated`/`is_integrated_among`; `Refs::carries`,
  `landing::effective_base`, `landing::target_tip` (non-`pr`) read it;
  `landing::TargetSync` gains `Diverged`, and `concern` speaks only for it.
- `uze-application`: `place_in_slot` births from `target_reach`; `isolate`
  unchanged.
- TUI: the target-sync toast's title.
- Archives after `fast-agent-placement`, whose renamed requirement this
  modifies.
- No network call is added anywhere; a judgement on a branch-named target
  costs one more `for-each-ref` when it is not asked from a pass's
  `BranchTips`.
