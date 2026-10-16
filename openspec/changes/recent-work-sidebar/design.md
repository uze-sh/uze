## Context

See proposal.md for why. Today kept work is drawn by the work modal
(`src/ui/orchestrator/work.rs`, `work_list.rs`, `checkouts.rs`,
`session/work_modal.rs`): per project, it merges `PreservedWork` (from
`TasksService::preserved_work`, already machine-wide) with that project's
`CheckoutView`s (a per-project Git read, lazy, off-thread) into `WorkRow`s
grouped by `Standing`. Resume already places the agent in its project's
space and opens one when none is open (`agent-isolation`'s "A resumed
agent returns to its own project's space"). Slot collection
(`collect_slot_garbage_in`) already removes idle free slots, integrated
branches and delivered shelves on every pass, and `worktree-policy`
already reads squash and rebase merges as integrated by patch.

The constraints that shape the approach are the architecture suite's:
`src/` reaches the domain only through `uze-application`; nothing the
client draws waits on a repository (reads are `spawn_*` in
`orchestrator/reads.rs` with an `absorb_*` beside them, answers carry the
question they were asked); chrome comes from `src/ui/widget/` and colour
from `theme`.

## Goals / Non-Goals

**Goals:**
- One surface for kept work, inside the sidebar, agent-first.
- Every action offered moves the row on; nothing offered that the system
  already does on its own.

**Non-Goals:**
- A separate history of integrated work. Recent lists every recorded
  agent, delivered or not, because the conversation and its reasoning are
  what an operator comes back for; a "done" section beside it would split
  one list in two. How long a record lives is the collection's question
  (`task::RECORD_KEPT`), never the list's.
- A filter by project. If scanning by project becomes a need, it is a
  filter, not a column.
- Any change to placement, delivery, joining or collection themselves.

## Decisions

### Recent replaces the tree rather than sitting under it
Both in one column competed for height and attention; a mode shows one
question at a time. The tree's liveness is not lost: toasts and the chime
already announce a finished agent, and the header mark covers the case of
looking at recent when it happens. Considered: a collapsible section under
the tree (the 2b prototype); rejected by the operator as too much at once.

### The row is the agent; the checkout is a fact on it
Rows come from two sources, as today's list does: every `PreservedWork`,
and every subagent checkout that is joinable (a subagent is not a task of
its own). A `CheckoutView` that belongs to neither is not listed. The
checkout contributes only `bytes` and `last_changed` to its agent's row.
Considered: two lists (agents, worktrees) linked by selection, which was
the first prototype; rejected because three of every four worktree rows
repeated an agent row, and the fourth (a free slot) is collected anyway.

### Blocks and reasons are a projection of `WorkStateView`
| state | block | reason said on selection | enter |
|---|---|---|---|
| `Conflicted { files }` | needs you | conflict in N files | resume |
| `GateFailed` | needs you | checks failed | resume |
| `Uncommitted` | needs you | uncommitted changes | resume |
| joinable subagent | needs you | subagent ready to join | join |
| `Ready` | needs you | ready to deliver (+ `i` deliver) | resume |
| `Integrating` | rest | delivering | resume |
| `Published` | rest | with reviewer | resume |
| `Shelved` | rest | shelved | resume |
| `Running` with no pane | rest | no changes yet | resume |

`Ready` sits in the first block because only the operator delivers it,
which is also how `Standing::NeedsYou` already reads a kept task. The
colours are theme tokens (`StateWarning`, `StateDanger`, `StateInfo`,
`StateSuccess`, `TextSecondary`), applied to the reason line only.

### Recency is the checkout's last change, else the record's creation
`PreservedWork` carries `created_at_unix`; `CheckoutView` carries
`last_changed` once its project is read. A row is ordered by the latter
when known and the former until then, so a row may move once when its
read lands. Considered: recording a last-activity timestamp on the agent;
deferred, since the checkout already answers for every row that has one.

### Reads: records at once, Git facts in the background
Opening recent sweeps preserved work (as opening the modal does today) and
asks for the checkouts of every project that has a listed row, each on its
own thread through the existing checkouts read, the answer carrying its
project key. Kept work is swept again when recent opens and after
anything recent did to it — a discard, a forget, a join, a name — and not
on the evaluation clock: nothing outside recent reads it. Answers are kept
under the repository's primary checkout, whichever directory of it was
asked about. The per-project read is the same one the modal used; only
its trigger changes from "the project in front" to "every project with a
row".

### Forget is a new application operation
`discard_task` resolves the repository first and fails when it is gone. A
`forget_task(primary, task)` on the tasks service removes the agent's
record and its conversations under the document lock, runs no Git, and
refuses when the project directory exists (discard is the path then).

### Removed actions and their keys
The toggle is renamed `ToggleRecent` in code and keeps its persisted name,
`toggle-preserved-work`, as the scope keeps `preserved-work`: an
operator's keys.json binds by those names, and `alt+p` stays the key.
`finish-task`, `adopt-checkout`, `clean-up-checkouts`, `join-checkout`,
`resume-task`, `show-space-work`, `next-project` and `previous-project`
are removed; enter covers resuming and joining. `deliver-task` and
`discard-task` stay, scoped to recent. A keymap file naming a removed
action is authored text: it loads, with each binding reported and
ignored, which the keymap loader already did for any unknown name.

### Confirmation follows the product's question rule
Discarding asks with two answers drawn as buttons on the row's own lines,
the keyboard starting on cancel and the arrows walking between them, as
`docs/architecture/invariants.md` requires of every question. The answers
use `widget::button_row`, so recent builds no chrome of its own.

### Code layout
`src/ui/orchestrator/recent.rs` holds the model (rows, blocks, selection,
the pending question); `session/recent.rs` the actions;
`render/recent.rs` draws it with `widget::row`, `widget::hint`,
`widget::button_row` and `theme`. `work.rs`, `work_list.rs`,
`checkouts.rs` and `session/work_modal.rs` are deleted, with the widget
code only the modal drew with (`widget::nav`, `modal::render`,
`screen_header::render`, `Button`'s disabled state, `text::small_digits`)
and the application operations only it called (`finish_task`,
`adopt_checkout`, `remove_checkout`, `clean_up_checkouts`, and their
`uze-workspace` counterparts).

No container, component or dependency changes, so `docs/architecture/`
needs no diagram update.

## Candidate ADRs

- The client stops managing checkouts no agent holds: it narrows what the
  operator can do from the workspace to agents' work, leaving foreign and
  hand-made checkouts to Git.

## Risks / Trade-offs

- [The operator relied on clean up for their own checkouts] → Git's
  `worktree remove` does the same; if it is missed, the place for it is
  `uze doctor`, not the sidebar.
- [Ready work without `finish-task` stays listed when it was merged by a
  path the patch comparison cannot see, such as a merge with edits] → it
  can still be discarded; the patch-based reading is the general answer,
  and a task verifies `Published` work leaves on a squash merge.
- [Reading checkouts of every project with a row costs one Git read per
  project on each open] → reads are off-thread and answered from the last
  answer meanwhile, as the modal already does for one project.
- [A row moves when its project's read lands and recency changes] →
  bounded to one move per open; acceptable against waiting on Git.
