## Why

Reconciling `extension-platform`'s task list with its nine split
changes left 17 tasks that no change delivered and no decision dropped
(`extension-platform/tasks.md`, each marked "open: …"). They are the
platform's own promises: hygiene the sanitizer and catalog owe, the
schema and reference an author reads, the UI state and actions a view
needs to take input, the inspector's authoring tools, the performance
budget's proof, and the journeys the gallery owes. This change closes
them, so the umbrella can be archived whole.

## What Changes

- **Hygiene** (2.1, 2.5, 3.3b, 6.6, 8.2): release notes render through
  `shared::markdown`; a chart given non-numeric data says so; `Clean`
  bounds display cells by `unicode-width`, sanitizes in `Deserialize`,
  and `view::Span` takes only clean text, with an architecture test; the
  check refuses a bare letter on an agent action's key.
- **Schema and reference** (3.5, 19.1, 11.3): the compose wire and the
  manifest as a JSON Schema generated with `schemars` behind a feature,
  written to `docs/extensions/schema.json` with one worked example per
  slot and feed and a drift test; a protocol and catalog reference page
  linked to it.
- **Input in views** (6.5, 22.4, 22.6): `applies-when` gating surfaces and
  sources; host-held `select` and `field`, local `filter`, `with` on
  actions, and a `Form` node delivering `form.<id>`; two-step actions, a
  `run` whose answer fills an `agent-prompt`, on a gesture only.
- **Inspector** (23.0, 23.3, 23.4): runs, events, grants and cost per
  extension, rerun with edited input; on a linked extension, switching a
  source to its fixture, a synthetic event, size and theme changes, fake
  agents, and recording a session's events into a fixture.
- **Budget** (25.6): tests holding D28's binding, memory and storm
  targets, and runs per minute, p95 and memory in the inspector and
  `uze doctor`.
- **Journeys** (12.3, 20.2): a widened update held, enabling refused in an
  agent's pane, and one journey per gallery entry still without one.

## Capabilities

### New Capabilities
- `extension-authoring`: the schema, the reference, and the inspector's
  tools an author uses.

### Modified Capabilities
- None in `openspec/specs/`.

## Impact

`uze-contract` (Clean, schema, Form/select/field), `uze-core` (check),
`uze-extensions` (engine: applies-when, UI state, two-step), `src/ui`
(inspector, release notes, renderer), `uze-application` (metrics),
`docs/extensions/`, journeys, ADRs untouched.
