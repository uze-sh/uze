## Why

A declared extension can draw a full-frame `view`, a sidebar `section` and
a `drawer`, fed by a fixture, a script or a confined program. That leaves
out most of what the gallery recreates:

- the one-line meters of `ccstatusline` and `claude-hud`;
- the band `claude-gsd-band` lays over a pane;
- the picker a `jump` needs;
- the chime `peon-ping` plays;
- the settings every one of them has.

Each needs a place to stand, a way to say something outside its surface,
or an answer shaped by its own logic rather than by a view file.

This change is the first half of change 6 of `extension-platform`'s
split (*Proposed split*). It brings everything of change 6 that needs no
word from an agent:

- the `status`, `band`, `overlay` and `notify` slots;
- the `view` output mode (compose);
- `notify` and `sound`;
- preferences;
- events delivered to a source, one at a time or the latest.

The agent bridge is the second half, `extension-agent-bridge`: `uze agent report`,
the `agent.*` events, the `agents` source and the agent actions. Splitting
here keeps one reviewable change about the host's chrome and one about
what crosses into an agent's pane (decisions.md, D1).

## What Changes

- **Slots**:
  - `status`: one row in the sidebar's header, ordered by the operator.
  - `band`: 1 to 6 rows declared in the manifest, along the top or bottom
    of an agent's pane, reserved in the layout so an answer never resizes
    the pane's terminal, with the pane's mouse and selection offset by it.
  - `overlay`: a modal surface over the frame, opened by an `open` action
    and closed by Esc, under the host's attribution strip.
  - `notify`: no surface, only what an answer asks to say.
- **Compose**: a source declares `output "view"`, and its script or
  program answers a tree of catalog nodes.
  - The host validates the tree against the catalog and a node cap,
    cleans every string, and binds each interactive node to a declared
    action with an `item` from the same answer.
  - It ignores a click within 300 ms of a re-layout, and memoizes the
    built view per answer and size.
- **Notify and sound**: an answer's envelope may carry `notify` (a title
  and a body) and `sound` (a cue from a fixed list) under the `notify`
  grant. `uze-platform` gains `notify`: the terminal's OSC 9 or 777 where
  the terminal speaks it, the desktop's own tool where there is one, the
  bell otherwise. Both are rate-limited per extension and attributed.
- **Preferences**: the manifest declares typed fields (`text`, `number`,
  `toggle`, `choice`). The operator edits them on the Extensions screen.
  Views read them as `{pref.<name>}`, and sources receive them in
  `input.preferences` when they name them. They are kept per extension on
  the machine. `secret` waits for change 8.
- **Events delivered**: an event that refreshes a source reaches it as
  `input.event`.
  - `deliver "latest"` (the default) coalesces events in 250 ms.
  - `deliver "each"` runs the source once per event, in order, bounded per
    window, with `dropped` counting the excess.
  - A built-in `events` source answers the last events the extension may
    see.
- Host-animated nodes (`Activity`, `Elapsed`, segmented `Progress`) are
  proven to move with no run behind them.

## Capabilities

### New Capabilities
- `extension-slots`: the `status`, `band`, `overlay` and `notify` places
  an extension may occupy, and the geometry the host keeps for each.
- `extension-compose`: a source answering a tree of catalog nodes.
- `extension-notify`: notifications and sounds an answer asks for.
- `extension-preferences`: typed settings an extension declares and the
  operator edits.

### Modified Capabilities
- None in `openspec/specs/`. Events delivered to a source extend change
  4's `extension-scripts`, which is not archived; the delta is written when
  it is (decisions.md).

## Impact

- `uze-contract`:
  - `Slot::{Status, Band, Overlay}`;
  - the band's placement and rows;
  - `Output::View`, `Envelope::{notify, sound}`, `Grant::Notify`;
  - `PreferenceDecl`, `Deliver`.
- `uze-core`: the decoder and the check for the above.
- `uze-extensions`: the declared engine composes a tree into a `View` and
  receives events.
- `uze-platform`: `notify`.
- `uze-workspace`: the preferences record under `state/`.
- `uze-application`:
  - preferences;
  - `input.event` and `input.preferences`;
  - the `events` source.
- `src/ui`:
  - the sidebar header's status rows and their order;
  - the band in `compute_layout`, the pane's mouse and selection offset;
  - the overlay;
  - notifications from the frame loop;
  - the preferences editor on the Extensions screen.
- Docs: the extensions rule, the invariants, the `uze:extension` skill.
- Fixtures:
  - `gsd-band` gains its band;
  - `gpu-meter` gains a status segment and a preference;
  - a `notes` overlay picker;
  - a composed board.
