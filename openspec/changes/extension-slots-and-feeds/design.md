## Context

The platform's design is in `extension-platform/design.md`. This change
carries out the half of its change 6 that needs no agent, citing these
decisions there:

- D4: the envelope's `notify` and `sound`, and the `view` output mode.
- D14: the slots.
- D15: compose.
- D17: delivery.
- D18: preferences.
- D20: the gallery.

It also uses the second review's R-S11 (compose) and R-A12 (`notify` in
`uze-platform`).

What exists after change 5:

- The slot path, `src/ui/orchestrator/declared.rs`, hosts `view`, `section`
  and `drawer` surfaces through `contract::Extension`.
- The declared engine binds compiled views to data and runs actions.
- The runner answers fixtures, scripts and confined programs, applies the
  envelope's `store` and remembers answers for the session.
- The frame loop owns the terminal. `TerminalSession::{ring, emit}` write
  the bell and raw control sequences between frames, as the agent-finished
  chime already does.

## Goals / Non-Goals

**Goals:**

- `gpu-meter` gains a `status` row, its load as a meter in the sidebar's
  header, and a preference for its warning threshold.
- `gsd-band` gains its 6-row `band` over the agent's pane, its segmented
  bar composed by `board.rhai`.
- A `notes` overlay picks an item and copies it.
- An action of the probe asks for a notification and a sound.

**Non-Goals:**

- `uze agent report`, `agent.*` events, the `agents` source,
  `agent.prompt` and `agent.restart`, `focus_agent`, global keys, and the
  `jump`, `flightdeck`, `agents-hud` and `chimes` entries that need them.
  These are `extension-agent-bridge`, the second half of change 6.
- `secret` preferences (change 8).
- Media and the resident tier (change 7).

## Decisions

### S1. Change 6 is two changes

`extension-slots-and-feeds` brings the host's new places and channels.
`extension-agent-bridge` brings everything that crosses into or out of an agent's
pane. The first runs no code an agent can see and changes no integration.
The second amends three ADRs and every integration. Reviewed apart, each
is one question.

### S2. A `status` row is one line in the sidebar's header

The rows are ordered by the operator with Move up and Move down on the
Extensions screen, and the order is kept in the client's remembered
layout.

- A status surface answers for the checkout in front.
- An empty answer frees its row.
- An answer longer than the sidebar is folded (D14), never wrapped.

### S3. A `band` is per pane, about that pane's checkout, and reserved before the pane is laid out

`surface "…" slot="band" rows=2 place="top"|"bottom"` is drawn on every
agent pane whose checkout's project has the extension enabled.

- `compute_layout` takes the band's rows off the pane's area before the
  PTY's size is computed, so an answer never sends a resize.
- Mouse and selection coordinates inside the pane are offset by the band.
- A band with nothing to draw keeps its rows, blank.
- Rows are 1 to 6.

### S4. An `overlay` is opened by an `open` action, sized by the host

The overlay is a centered card at 70% by 70% of the frame, with at least
40 columns and 10 rows. It sits under the host's attribution strip, is
modal and closes with Esc.

An `open` action that names an overlay opens it. The action index offers
"Open <title>" for each overlay of an enabled extension.

### S5. A composed answer is a JSON tree of catalog nodes

```json
{"type": "Stack", "props": {"gap": 1}, "children": [
  {"type": "Text", "args": ["3 open"], "props": {"strong": true}},
  {"type": "Button", "args": ["Archive"], "props": {"action": "archive"},
   "item": {"name": "auth"}}]}
```

- It is validated against the same `NodeSpec`s a view is compiled
  against:
  - the arguments' and properties' kinds;
  - the children a node may hold;
  - the node cap of 2,000, as `bind`.
- It is built into the same `View` a bound view builds.
- A string is cleaned as every answer's is.
- A `Button` or `Toggle` must name an action the manifest declares, and its
  `item` becomes that action's selection.
- A tree that does not validate is a failure drawn on the surface, naming
  the first wrong node by its path (`children[1].props.action`).

### S6. Notifications go out from the frame loop, rate-limited and attributed

- An envelope's `notify {title, body}` and `sound "<cue>"` need the
  `notify` grant.
- Cues are `done`, `attention` and `error`. Every cue is the terminal's
  bell, since the host plays no audio file. D20 records peon-ping's voices
  as lost.
- `uze_platform::notify::send(title, body)` chooses, in order:
  1. the desktop's tool by path: `notify-send`, `osascript`, or Windows'
     PowerShell toast;
  2. OSC 9 when `TERM_PROGRAM` names a terminal known to show it (WezTerm,
     iTerm2, ghostty, kitty);
  3. the bell.

  The workspace writes OSC sequences between frames, as the chime does.
- Each notification's title begins with the extension's attribution.
- The rate is one per extension per 10 s and 30 per hour. The excess is
  dropped and counted in the surface's notice.

### S7. Preferences are a record per extension on the machine

`state/extensions/preferences/<plugin@marketplace>/<id>.json` is a record,
since only the operator's answer says it. The manifest declares the
fields:

```kdl
preferences {
    number "warn-at" label="Warn at load (%)" default=70 min=1 max=100
    toggle "fahrenheit" label="Show °F" default=#false
    choice "unit" "MiB" "GiB" label="Memory unit" default="MiB"
    text "title" label="Title" default="GPU"
}
```

- A view reads `{pref.warn-at}`.
- A source receives `input.preferences` when it declares `preferences`.
- The Extensions drawer lists each field with its value:
  - Enter edits a `text` or `number` in the host's field dialog;
  - Enter flips a `toggle` and cycles a `choice`;
  - a click does what the key does.
- A value outside its declaration reads as the default.

### S8. Events reach a source as `input.event`, latest or each

`refresh-on` gains `deliver "latest" | "each"`.

- **`latest`**: the 250 ms coalescing of change 4 hands the last event.
- **`each`**: one run per event, in order, at most 16 per window. The rest
  are dropped and the next run's `input.event.dropped` counts them.
- **Event shape**:
  - `{name: "paths.changed", paths: [...], at}`;
  - `{name: "checkout.changed", at}`.
- **The `events` source**: `source "log" { events 50 }` answers the last
  50 events of the extension, newest first, kept in memory for the
  session.

## Risks / Trade-offs

- [A band steals rows from every pane of an enabled project] → it is
  declared, at most 6 rows, and listed on the consent screen as "draws N
  rows over each agent's pane".
- [OSC 9 detection by `TERM_PROGRAM` misses terminals] → it falls back to
  the bell, and the operator still hears something.
- [Compose validation costs a walk per answer] → it is memoized per answer
  digest and size, as D15 asks.

## Migration Plan

Nothing persisted changes shape. New records are `preferences/`, and the
status order is in the client layout, which reads an absent order as the
order of discovery.

## Open Questions

None. This change records what it decides in `decisions.md`.
