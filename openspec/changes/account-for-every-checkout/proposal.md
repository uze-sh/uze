## Why

UZE decides which checkouts it may reset, reuse and remove by where they
sit and what they are called, and it sees nothing else. Every worktree
registered directly under `.worktrees/` is taken for a slot; every other
worktree of the repository is invisible. The projected policy meanwhile
tells an agent to give its subagents checkouts with a raw `git worktree
add` — under `.worktrees/<topic>`, inside the pool.

On 2026-09-26 the two met. A subagent's checkout, seconds old and so clean
and level with the target, was adopted at startup, read as a free slot, and
handed to an agent the operator had just launched: `switch -c`, `reset
--hard` and `clean -fd` ran under a writer that was still working in it.
The hotfix on `refactor/hardening` stops the bleeding by name — only a
generated identifier or a legacy `agent-<n>` is recycled, and subagents are
sent a level down — but a name is inference, and the next convention an
agent or a person invents will meet the same rule from the other side.

The fix is to know, not to guess: record who made a checkout when it is
made, account for every worktree of the project whoever made it, treat a
process working inside a checkout as a fact no automatic path may ignore,
and give an agent a verb for its subagents so that nobody has to write the
Git by hand.

## What Changes

- **That UZE made a checkout is recorded where the checkout lives.** A
  checkout UZE makes carries a record in its own Git administrative
  directory, with its path and, for a subagent's checkout, the agent it was
  split from. It survives a lost `state/` and disappears with the worktree.
  Only a recorded checkout is ever reset, reused, collected or pruned
  automatically; who holds it now stays the task store's answer.
- **Adoption by inference ends.** Every pass records, on sight, the slots
  of agents UZE launched and legacy `agent-<n>` checkouts; nothing is
  recorded from its name alone. **BREAKING**: a checkout an earlier build
  adopted by inference is no longer treated as UZE's; it is listed for the
  operator to adopt or remove.
- **UZE's own writes stop parking slots.** A changed lock file, or a
  change to the instruction file confined to UZE's managed regions, is not
  work: a slot holding only that is free. In the repository this change is
  written in, that dirt alone had parked four of fourteen slots.
- **Every worktree is accounted for.** UZE reads every worktree the
  repository registers, wherever it is, and classifies it by owner: an
  agent's slot, a subagent's checkout, a harness's own isolation (named by
  the integration that knows the harness), or the operator's. Foreign
  checkouts are shown with their facts and are never touched on any
  automatic path.
- **A checkout in use is occupied.** A live process whose working
  directory is inside a checkout makes it occupied for every destructive
  decision, UZE's own slots included — generalising today's "a pane is
  inside" check to any process.
- **`uze agent work split|join|list`.** An agent asks for a subagent's
  checkout by topic and gets a warm slot branched from its own HEAD,
  stamped as its child; joins the child's commits back onto its own
  branch without a merge commit; and lists its children. A child is held
  until it is joined or its agent ends; an agent is not delivered while a
  child holds unjoined commits, and one that ends with such a child is
  parked with it.
- **The operator sees and acts on every checkout.** The agent column shows
  a subagent checkout under its parent. A checkouts view lists every
  worktree of the project grouped by owner, and offers only explicit
  actions: open a space there, adopt, remove, and one clean-up gesture for
  every foreign checkout that is clean, unused and already in the target.
- **The projected policy names the verbs.** The raw `git worktree add`
  block and the hotfix's `.worktrees/subagents/` convention are removed
  from the projected region and from the `uze:worktree` Skill.

## Capabilities

### New Capabilities
- `checkout-accounting`: recorded checkout ownership, the classification
  of every worktree of a project, the in-use fact, the agent-audience
  `work split|join|list` verbs, and the operator's checkouts view.

### Modified Capabilities
- None in `openspec/specs/`. The slot, removal, adoption and projection
  requirements this change revises belong to the open change
  `add-portable-worktree-policy` (capability `worktree-policy`, not yet
  archived); they are edited in place there, as that change's own tasks
  record, rather than superseded from here.

## Impact

- `uze-core`: `project/checkout.rs` (record, classification, in-use,
  split and join), `project/worktree.rs` (projected text), `project/task.rs`
  (an additive parent on the agent record), a process-cwd probe under
  `machine/`.
- `uze-integrations`: each integration may declare where its harness keeps
  worktrees of its own; exposed through the registry, never named outside
  the crate.
- `uze-application`: a checkouts read model and the three agent verbs;
  adopt and remove as operator actions.
- `src/`: the `uze agent work` leaves (classified in
  `command_performance.rs`), the agent column's children, and a checkouts
  view whose reads run off the render thread.
- `plugins/uze/skills/worktree/SKILL.md` and the projected worktree-policy
  region.
- `uze-git` carries the new Git calls; no new dependency.
