## Why

An agent's work carries two names, and today both are generated noise. The
task's label is derived from a launch prompt the workspace client never
supplies (`Task::new(None, …)`), so it is always the identifier; the branch
is `agent/<id>` by construction, and the readable name produced at publish
time is derived from that same label — so a pull request opened from an
agent's work is titled `agent/zulqgq`. A reviewer meeting that branch
learns nothing, and the operator reading three agents in a sidebar cannot
tell them apart.

Naming cannot be solved by deriving harder. The only party that knows what
the work is, is the agent doing it — but nothing lets it say so portably,
and a name a harness volunteers is not a name every harness volunteers.
What is missing is a surface an agent can call and a moment it must call
it — and the moment has to be the first one available, because an
instruction an agent reaches while already working is one it weighs against
the work.

A second defect falls out of the same gap: nothing re-reads a task's branch
after it is recorded, so an operator renaming a branch by hand is invisible
to the sidebar — and worse, `commits_ahead` then asks Git about a branch
that no longer exists and answers `0` through its `unwrap_or`, leaving the
task `Running` forever and never offering delivery.

## What Changes

- **`uze agent <noun> <verb>`** — a new command namespace whose audience is
  the agent rather than the person: an ABI, hidden from `uze --help` and
  documented in the projected `AGENTS.md` region, which is where an agent
  reads. First verb: `uze agent work name <type>/<subject>`, with no
  identifier argument: it names only the work of the agent running it
- **`worktrees.branch`** — a new policy in `agents.yaml` declaring the
  project's branch vocabulary: a preset (`conventional`, `gitflow`,
  `flat`, `agent`) or the project's own list of types. Closed either way,
  because a proposed name is validated against it. Undeclared (or `agent`)
  keeps today's behavior: the generated identifier, and naming refused
- **An automatic name never overwrites a chosen one** — the first-commit
  derivation replaces only a branch still in UZE's `agent/` namespace; the
  agent's own naming command renames whenever asked, and the last name
  given stands
- **The checkout's HEAD is the truth** — each evaluation adopts the branch
  the task's checkout is actually on, so a manual rename reaches the
  sidebar, delivery and sync (**BREAKING** for nothing: `task.branch`
  becomes a cache of a Git fact)
- **The moment is the agent's first action** — the projected instruction
  asks for the name before the agent reads a file, plans or edits, and is
  the region's first bullet. A name states an intention, and the intention
  is what an agent holds at its first turn; every later moment competes
  with work already under way, and a moment reached while busy is a moment
  skipped
- **The automatic half is a Git fact** — work that reaches a commit still
  unnamed is named from that commit's subject, on the evaluation pass that
  already runs. This replaces the `PreToolUse` `deny` the design carried:
  it was built, and removed for three reasons kept in design §6
- **A safety net at publish** — a project that declares no vocabulary names
  nothing, so an unnamed branch is still published under a name derived
  from the first commit rather than under the generated identifier

## Capabilities

### New Capabilities
- `agent-work-naming`: how the work an agent does acquires its two names —
  the branch reviewers read and the label the operator reads — who may
  write them, what validates them, and what never overwrites them

### Modified Capabilities
- `worktree-policy`: the requirement `A task's identity is immutable and
  its name is derived` changes on the name half only. Identity, keying and
  atomic state are unchanged; the label stops coming from a launch prompt,
  the branch becomes renameable, and the publish-time readable name
  becomes a fallback rather than the mechanism. `add-portable-worktree-policy`
  was archived carrying that text, so `openspec/specs/worktree-policy`
  already states it and this change carries no delta for it

## Impact

- **Workspace domain** (`crates/uze-workspace`, where these modules moved
  from `uze-core`) — `task` (the label's source, the branch as a mutable
  attribute), `worktree` (the branch vocabulary and its validation, the
  projected instruction), `checkout` (adopting the checkout's HEAD;
  `prune_integrated_branches` stops assuming the `agent/` prefix),
  `landing` (the first-commit derivation and the publish-time fallback)
- **Application** — a naming use case on `Workspace`, and the evaluation
  pass that adopts a renamed branch
- **CLI** — the `agent` namespace, hidden from help; a new leaf command to
  classify in `command_performance.rs`
- **TUI** — the sidebar reads the adopted branch; the task label follows the
  name
- **Docs** — the projected `AGENTS.md` region gains the naming clause;
  `docs/architecture/invariants.md` gains that an automatic name never
  overwrites a chosen one
- **Depends on** — `project-agent-environment` §12: a policy that does not
  reach the projected region is a policy agents never read

## Not in this change

- Resolving the task to name from the identity the agent's launch carried
  (the stamp in its environment, verified against its directory), and the
  `agent-identity` capability that states it: owned by the open change
  `identify-agents-at-launch`, which edits this capability once it lands.
  This change states only that the command names the running agent's own
  work.
- Renaming the branch from the workspace's own rename gesture: it still
  renames the tab label only. A follow-up.
