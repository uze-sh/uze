## Why

Kept work lives in a modal of its own, the work modal, which lists a
project's checkouts and tasks side by side, grouped into four standings,
with nine actions, most of them about checkouts rather than agents. The
operator thinks in agents and branches: the worktree a task sits in is
UZE's business, and the modal makes them read it anyway. Most of what it
offers is either done automatically already (free slots and integrated
branches are collected on every pass) or concerns checkouts that are not
agents at all. The result is a second surface that is verbose, and that
nobody opens until something is already lost.

## What Changes

- The workspace sidebar gains a **recent** mode: one global list of the
  agents no live pane carries, across every project UZE has recorded,
  replacing the tree while it is open. The tree stays the default mode.
- The list is agent-first: one line per agent, its name and how long ago,
  with no glyph and no state colour. The worktree is an attribute of the
  agent, never a row of its own, and its directory is never named.
- Two blocks: **needs you** (conflict, failed checks, uncommitted changes,
  a subagent ready to join, ready to deliver) on top, then **the rest**
  (with the reviewer, shelved), each ordered by recency.
- Selecting a row expands it: the reason or state in its colour, the
  project (and that resuming opens a space when none is open), the branch,
  how far ahead it is and the disk its checkout holds.
- The sidebar header carries the count of agents needing the operator,
  which is also the way into the mode, and a mark when an agent in the
  tree finishes while the operator is in it.
- Actions follow one rule: **enter** does the state's primary action
  (resume, or join for a subagent), **d** discards, and only ready work
  adds **i** deliver. Discarding asks inline, naming what goes: the
  branch, its commits and the conversation.
- Work whose project is no longer on disk is listed as such and can only
  be forgotten, which removes the record and touches no repository.
- Resuming closes the mode and leaves the operator on the resumed agent in
  the tree, opening a space on its project when none is open.
- **BREAKING** The work modal is removed, with its key binding, its
  per-project checkout listing and the actions only it offered: open
  space, adopt, remove, clean up and mark done. Adoption already happens
  at startup and collection on every pass; the operator's own checkouts
  and a harness's are theirs, as Git already lets them be.

## Capabilities

### New Capabilities
- `recent-work`: the sidebar's recent mode, what it lists, in what order,
  what a selected row says, how the mode is entered and left, and the
  actions each state offers.

### Modified Capabilities
- `checkout-accounting`: the operator no longer gets an in-client view
  of every worktree; adopt, remove and clean up leave the client.
- `agent-isolation`: preserved work names its project on the selected
  row rather than on every row, leaves the list by being resumed, joined,
  discarded or forgotten, and a project gone from disk is listed as such.

## Impact

- `src/ui/orchestrator/`: the sidebar render gains the recent mode;
  `work.rs`, `work_list.rs`, `checkouts.rs` and `session/work_modal.rs`
  are replaced by it; reads move to the existing background-read pattern.
- `crates/uze-keys`: the toggle and its scope keep their persisted
  names and `alt+p`; the modal's own actions (`finish-task`,
  `adopt-checkout`, `clean-up-checkouts`, `join-checkout`, `resume-task`,
  `show-space-work`, `next-project`, `previous-project`) go. A keymap
  naming a removed action must still load.
- `crates/uze-application`: a forget operation for work whose project is
  gone; the checkout-accounting read model loses the callers that only the
  modal had.
- `journeys/suites/06-recovery/07-work-the-closing-did-not-take.yml`,
  `docs/architecture/invariants.md` and the workspace docs page that
  journey proves are updated to the new surface.
