## 1. Hygiene

- [x] 1.1 Release notes render through `shared::markdown` (umbrella 2.1)
- [x] 1.2 A chart given non-numeric data draws a notice, tested (2.5)
- [x] 1.3 `Clean`: display cells by `unicode-width`, sanitizing `Deserialize`, `view::Span` built only from clean text, and the architecture test (3.3b, 6.6)
- [x] 1.4 The check refuses a bare-letter key on an agent action (8.2)

## 2. Schema and reference

- [x] 2.1 JSON Schema of the manifest and compose wire via `schemars` behind a feature, `docs/extensions/schema.json`, worked examples per slot and feed, a drift test (3.5, 19.1)
- [x] 2.2 The protocol and catalog reference page, linked to the schema (11.3)

## 3. Input in views

- [x] 3.1 `applies-when` on surfaces and sources (6.5) — D20
- [x] 3.2 Host-held `select` and `field`, local `filter`, `with` on actions, and a `Form` node delivering `form.<id>` (22.4) — D21, D22
- [x] 3.3 Two-step actions: a `run`'s answer filling an `agent-prompt`, gesture only (22.6) — D23

## 4. Inspector and budget

- [x] 4.1 Runs, events, grants and cost in the inspector; rerun with edited input (23.3)
- [x] 4.2 On a linked extension: switch a source to its fixture, a synthetic event, size and theme, fake agents, record a session's events into a fixture (23.0, 23.4)
- [x] 4.3 D28's binding, memory and storm targets tested; runs per minute, p95 and memory in the inspector and `uze doctor` (25.6)

## 5. Journeys and closing

- [x] 5.1 Journeys: a widened update held, enabling refused in an agent's pane (12.3) — D60, D61
- [x] 5.2 A journey per gallery entry still without one: gpu-meter, gsd-next, chimes, jump, plan-review, pets, screen (20.2) — D62–D67
- [x] 5.3 The umbrella's 17 tasks ticked with pointers here; `make check`, `openspec validate --all --strict`
