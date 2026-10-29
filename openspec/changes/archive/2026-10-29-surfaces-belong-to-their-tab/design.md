## Context

The host keeps one engine per extension, holding at most one live view, and
one surface standing in the pane at a time (`open_slot`, `open_root`, and the
tab it stands for). Where a viewer was is an opaque `place`, kept per
checkout and handed back to `resume` after `open`. Code deliberately leaves
its content mode out of `place`: the mode is the door that was used.

## Decision

Option A, extended: a return becomes a kind of arrival, and what a surface
keeps for a return is asked separately from its place.

- `Entry::Return` is entered before `open` when the host brings a surface
  back to its tab. It replaces any arrival left over.
- `hold(surface)` is taken as a surface is put away for a tab that left the
  front, never on a close; it defaults to `place`. After `Entry::Return`,
  `resume` is handed what `hold` took.
- The host keeps `standing: BTreeMap<TabId, Standing>` in the extensions it
  remembers across attaches: owner, surface, root, the held state, and the
  host's own frame around it (navigator scroll, inspector). Opening a surface
  on a tab replaces its record; closing one removes it; a closed tab, an
  extension switched off or gone, and a replaced runtime drop theirs.
- One `settle_pane` follows the session: it puts the live surface away for
  its tab when another comes to the front and, while no overview holds the
  pane, restores the record of the tab in front. It runs after the
  overview's own follow-up, and when the operator dismisses the overview
  over the tab it stood on — never on a frame, where a tab being left from
  under the overview would get its surface back before the move is
  confirmed.
- Records are keyed by agent, not by tab: an agent's tab and the shells
  beside it are one context, which the sidebar marks whichever is in front
  and which a standing surface hides the strip of. Per tab, a surface opened
  from the shell waited there unseen and stood over the shell each time it
  was chosen, beside another kept on the agent's tab. Another tab of the
  same agent coming to the front takes the pane back, as choosing the tab
  in front does.
- A record restores only where the tab in front still answers for the
  agent directory the surface was opened on: the server paints a set-aside
  workspace, whose tab numbers start again, before it says it was set
  aside.

## Alternatives

- **A live view per tab (B).** Every contract method would address a view
  instance, answers would be routed per instance, and memory would grow with
  every tab's highlighted files and diffs, all to keep a stale diff and reads
  in flight that a return re-asks anyway.
- **The host remembers the door each tab used.** Branches on which extension
  it is, which the extension rules forbid.
- **The mode in code's place.** `place` also serves a fresh open through a
  door, and a door that remembered where it last led would stop being one.

## Risks

- An unsaved edit in code is still dropped when its tab leaves the front, as
  before this change; holding it needs the read that a return re-asks not to
  overwrite it, and is left for a change of its own.
- Tab ids restart with a fresh runtime, so records are dropped when the
  workspace is set aside.
