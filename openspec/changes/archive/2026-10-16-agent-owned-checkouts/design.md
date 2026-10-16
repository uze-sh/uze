## Context

See proposal.md for why. What the investigation established, by
measurement unless marked otherwise:

- **Every harness keys its conversation by directory.** The continuity
  contract run with the checkout moved between the two processes failed on
  all four (Claude Code 2.1.300, Codex 0.162.1, OpenCode 2.0.29,
  Antigravity 1.3.5) and passed on all four when the directory stayed.
  Claude reported the recorded conversation missing in the new directory;
  OpenCode was resumed with the right session id and still started empty;
  Codex and Antigravity were never asked, because UZE observes their
  conversation in the launch's current directory (`continuity::plan`).
- **The pool makes a moved checkout the common case.** `take` hands the most
  recently used free slot to the next agent, and collection removes idle
  slots, so a resumed agent rarely finds its slot (on the operator's
  machine, `s9cxk4` passed from `u8fex6` to `wmp9ak`).
- **`git worktree move` (git 2.43)** keeps uncommitted and ignored files,
  rewrites `gitdir` and the `.git` file, and keeps the admin directory's
  name. It refuses initialized submodules (`-f` does not override) and
  locked worktrees, moves *into* an existing target like `mv`, and fails
  across filesystems. A process working inside keeps running and its
  `getcwd` silently becomes the new path.
- **UZE's checkout record breaks on a move.** `CheckoutRecord.path` is
  absolute and `record::read` requires it to match the checkout
  (`same_place`), so a moved slot reads `Absent` and would be disowned.
- **Cost of a move, this repository:** cold build 85 s, unmoved rebuild
  0.23 s, rebuild after a move 22 s (registry dependencies and other
  workspace crates stay fresh). Without a fix the build after a move fails:
  `crates/uze-application/build.rs` bakes `env!("CARGO_MANIFEST_DIR")` into
  the compiled script.
- **Per-path harness state besides the conversation:** Claude keeps trust,
  permissions, MCP approvals and memory by the repository root; Codex falls
  back to the primary checkout's trust (`codex/trust.rs`); OpenCode shares
  the project across worktrees. Antigravity matches `trustedWorkspaces` by
  exact path and asks once per new folder (inferred from the operator's
  state: 10 per-slot entries).

## Goals / Non-Goals

**Goals:**
- An agent comes back to the same directory every time, so every harness
  continues its conversation without UZE writing into any harness's state.
- Slots and their warm build artifacts stay reusable.

**Non-Goals:**
- Copying or relocating harness transcripts. Rejected: it writes into
  vendor storage, works for one harness, and leaves two copies.
- Making path-bound caches relocatable. The project's `setup` is where they
  are repaired; UZE does not learn ecosystems.
- Seeding Antigravity's folder trust (see Risks).

## Decisions

### An agent's own path is the directory it last held
An agent's own path is `isolation.checkout`, else `last_checkout`, else its
id; a new agent therefore owns `.worktrees/<agent-id>`. Agents placed by an
earlier build keep the generated name they last held, so an upgrade never
moves a slot out from under an agent that would still find it — today's
`resume` already prefers `last_checkout` (`pool.rs:270`), and taking that
away would lose conversations that survive today. `CheckoutId` keeps
equalling the directory's name, which keeps every reader that derives the
checkout from a path (`slot_name`, `slot_holder`, `own_directory`,
`CheckoutId::adopted`) unchanged and keeps two builds on one machine safe:
an older build looks an owner up by directory name and still finds it.
Considered: a stable slot identity plus an agent-named directory as two
fields. Rejected: a second field is a shape change no reader needs.

### Handing a slot to an agent is intent, move, record — under the lock
`take` receives the agent's own path. With the repository's write lock
held: the agent's own directory is used if it is there and the agent's;
otherwise a free slot is chosen, checked, and moved:

1. its record gains `moving_to: <agent path>` (an additive optional field,
   the pattern `pinned` already follows: an older build drops it);
2. `git worktree move` renames the directory;
3. the record's `path` is rewritten and `moving_to` cleared;
4. the checkout is reset onto the task's base, as `reuse` does today.

Reconciliation accepts a record whose `path` differs from where it is found
only when it equals that record's `moving_to`, and runs `git worktree
repair` before any `git worktree prune` (`reconcile.rs:178`,
`pool.rs:340`), because Git renames the directory before it rewrites
`gitdir` and a prune in that window would delete the admin directory and
the record with it. A copied checkout carries no `moving_to` naming its
location, so "a copied checkout does not inherit the record" still holds.

### Which slot is taken, and which is not
Free slots are preferred in this order: one whose last agent will not come
back to it (delivered, discarded, or closed with no recent conversation),
one with a generated name nobody owns, then the most recently used, as
today. This keeps the slot an agent just left for that agent where the pool
allows. A slot is not taken when Git would refuse (an initialized
submodule, a lock), when its target path exists, or when any process works
in it — `Presence::anyone_inside`, every process the table holds, not
`inside`, which ignores processes with no terminal on purpose
(`process_cwd.rs`) and would let a Codex app-server daemon follow the
rename into another agent's path. Both answers come from one read of the
table, made under the lock by the observer `take` is handed, as `release`
makes its own before a reset: what the caller read before waiting for the
lock may be stale by the time a slot moves. A slot set
aside this way does not count against the cap for the `create` that
replaces it, so the operator is never told every slot is in use when one
is merely unmovable.

### When the agent's path is taken
An older build reuses slots without renaming, so another agent can stand at
an agent's path; a crash can leave a stray directory there. Then the agent
is placed at a generated name and the toast says its conversation could not
be carried. `CheckoutId::generate` stays for this, and for `create`'s
fallback; its doc at `pool.rs:44-47` is reworded.

### Resume is placement at the agent's path
`resume_task` and `resume_children` ask the pool for the agent's own path:
kept if present, otherwise a free slot moved to it, otherwise created.
`split_work` (`work.rs:112`) places a subagent at its own path the same
way. The `fresh` start added for closed agents stays. `continuity::plan`
does not change: the directory now matches.

### A restore with no client stays where it was
The terminal runtime respawns a tab in its saved directory without asking
UZE to place a checkout (`state.rs`, `pane.rs`). This change does not add
placement to that path; when the saved directory was collected the tab
opens as it does today. The continuity requirement is scoped to the
operator's resume accordingly.

### The build script reads its directory at run time
`build.rs` uses `std::env::var("CARGO_MANIFEST_DIR")`, as it already does
for `OUT_DIR`. Independent of the rest; its own commit.

### Conformance proves the path, not the move
The continuity contract gains a scene that removes the task's checkout
between the two processes and adds it again at the same path, on the same
branch, as placement now does. Its scaffolding is a `between` step on the
relaunch and a pair of markers per scene; an earlier variant that moved the
checkout to a different name proved the defect on all four harnesses and is
not kept.

No container, component or dependency changes, so `docs/architecture/`
needs no diagram update.

## Candidate ADRs

- An agent owns its checkout's path: the pool reuses slot contents but
  never makes an agent change directory, because harnesses key their
  conversations by it.

## Risks / Trade-offs

- [Caches that record absolute paths break when a slot changes hands:
  Python virtualenvs, direnv allow lists, CMake build trees] → `setup` runs
  after every take (it already does) and the docs name it as the place to
  repair them; cargo rebuilds only what its build scripts recorded.
- [Antigravity asks to trust each new agent's folder, where today it asks
  once per slot] → accepted and documented; seeding `trustedWorkspaces`
  would be UZE writing harness state, which this change refuses elsewhere.
  Revisit if it proves a real cost.
- [On Windows a rename fails on open handles] → the precondition checks a
  process inside first; a failed move falls back to creating a checkout.
- [More directories in each harness's per-path stores (one per agent
  rather than per slot)] → bounded by agents the operator keeps; Claude
  already holds one transcript directory per slot today.
- [The pool's choice of slot no longer guarantees the warmest one ends up
  under an active agent's name] → the move keeps whatever was warm in it;
  the measured cost is a partial rebuild.
- [The spare-slot requirement said "at most a declared number, two", and
  the pool keeps only an idle age (`pool.rs`)] → the code is right: `spare`
  was retired with the shelf (#223) and declaring it is refused by name. The
  requirement this change modifies is rewritten to the idle-age rule.
