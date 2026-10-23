> **Status: frozen (2026-09-27), revised 2026-10-23.** Two reviews
> (architecture fit, security) found blockers that prevent implementation as
> written. They are recorded in design.md under *Unresolved review findings*,
> alongside the proposed split and the product decisions still owed. The
> 2026-10-23 revision widens the plan from a survey of what the four
> harnesses and adjacent tools let plugin authors build (design.md,
> *Evidence*); it resolves none of the earlier findings by itself. Do not
> apply this change until those are resolved and the plan is refined.

## Why

The workspace client's extensions (`code`, `architect`, `spec`) can only be
added by editing UZE and rebuilding it: each one is wired into the
orchestrator by hand, through its own fields, its own outcome enum, its own
async pair and its own `ExtensionHit` variant. Nobody outside this repository
can put a surface in front of an operator, even when the surface is a list
over a CLI they already have (an `openspec` board, a CI panel, an issue
list).

The harnesses show what operators build when they are let in, and what it
costs when the door has no lock:

- Claude Code's mods draw panes, bands above the prompt, cell grids at 60fps,
  images and games, and drive the session; OpenCode's TUI plugins draw into
  fixed regions and open dialogs and pages. Both run plugin code with the
  operator's full rights and no grant model.
- Codex and Antigravity let nothing draw in their terminal UI. Their most
  creative projects live outside it: overlays fed by hook files, tmux
  wrappers, web panels, clients of a JSON protocol.
- The most popular projects in every ecosystem answer one question, "what
  are my agents doing" (a status HUD with 28k stars that parses transcripts
  by hand), which UZE answers natively because it sits above every harness.

The seams that would let a third party in were drawn on purpose and have
held: an extension describes a `View` and the host draws it (ADR-041), and
whatever it needs from the machine it asks the `Host` for, which grants it
narrowly (ADR-048). What is missing is everything past those seams: a
generic place in the host, places on screen an extension may occupy, a
vocabulary rich enough that authors do not leave for a web page, a way to
declare an extension without Rust, contracts a program in any language can
answer, a bridge to the agents, and a trust model for code UZE runs on the
operator's machine. That last one gates the rest, because a loading
mechanism chosen without a capability model cannot be given one afterwards
(ADR-041).

## What Changes

- **The host knows extensions generically.** One contract every extension
  implements, a registry that constructs them instead of listing metadata,
  one routing path for keys, pointer hits, outcomes, async requests and
  remembered place. `code` and `architect` move onto it first, as the proof
  the contract is sufficient; no surface changes for the operator.
- **A plugin can ship an extension.** `extensions/<id>/extension.yaml`
  becomes a plugin capability. It declares the extension's surfaces, the
  data sources behind them, its actions and their suggested keys, the events
  it reacts to, its preferences, the CLI commands it adds, and the grants it
  needs. It carries no logic.
- **Three tiers behind one manifest.** A surface declares how it is fed, and
  each tier costs more trust and more machine than the one before:
  - *declared*: stateless programs answer JSON and the host builds the
    surface from the catalog;
  - *cells and media*: still stateless, a program may also answer a grid of
    themed cells with clickable regions, an image, or a sprite whose frames
    the host animates from an agent's state;
  - *resident*: a WebAssembly component that keeps state, receives input and
    timers, and answers a catalog tree or cells every frame, reaching nothing
    but the host imports it was granted.
- **Places an extension may occupy.** A full-frame view, a sidebar section, a
  segment of the status bar, a band along the edge of an agent's pane, an
  overlay (picker, form, dialog) under host chrome, and a notification. An
  extension never draws over a harness's own output.
- **A catalog of primitives in families.** Layout, text, data (list, tree,
  table, details), measure (segmented progress, gauge, pips, bar, sparkline,
  heatmap), status (badge, several marks per row, chips, an activity mark
  and elapsed time the host animates), media, cells, input (button, toggle,
  select, text field, form, fuzzy picker) and feedback (toast, notice,
  confirmation, notification, sound). A surface is either *bound* (manifest
  templates over a source's data) or *composed* (a program answers a tree of
  catalog nodes). Colour is named only as a semantic role.
- **Programs in any language answer one contract.** A source, an action or
  a command is an argv the host runs with JSON on stdin, JSON (or lines) on
  stdout and the portable hook exit codes (0 answers, 3 refuses with a
  reason, anything else fails), bounded in time and output. The host holds
  all view state; programs are stateless and short-lived, so a declared
  extension costs no resident process.
- **A bridge to the agents, across harnesses.** UZE reports each agent's
  turns, tool calls and approval requests as normalized events from a hook
  of its own delivered to every harness, and offers a read model of the
  agents (harness, state, work, checkout). An extension may send an agent a
  prompt or a skill invocation, which each integration spells in its own
  harness's dialect, and may interrupt or restart its conversation, under
  grants of their own; an unattended prompt needs a stronger one.
- **Richer reasons to refresh, and a way in from outside.** An interval with
  a floor, declared paths changing, and pipes: `uze ext pipe` and other
  extensions push data to a surface, waking it if it is closed, without
  making it resident.
- **Memory and preferences.** A small per-extension store, and typed
  preferences the operator edits on the Extensions screen.
- **A security model for extension code.** Grants declared in the manifest
  and consented to at enable time, recorded against the package digest,
  re-asked when an update widens them; argv-only execution with no shell;
  programs resolved inside the package or through a named grant; a scrubbed
  environment; OS confinement where the platform offers it, and
  confinement by construction for the resident tier on every platform; every
  string an extension produces sanitized before it reaches the terminal;
  host chrome always attributed and never drawable by an extension;
  extension keys never shadow the host's.
- **A gallery that proves it.** Ten reference extensions, each recreating a
  plugin operators already use on a harness, are the acceptance suite; a
  primitive joins the catalog when a gallery entry needs it.
- **Authoring tools and a skill.** `uze agent plugin create --extension`,
  `uze agent extension check <path>` (schema, bindings, and a real dry run
  of every source), `uze ext <extension> <command>` for declared commands,
  and a dedicated `uze:extension` Skill, linked from `uze:author`.
- Rewriting a harness's own transcript, raw terminal output from an
  extension, and native code loaded into UZE's process are **not** part of
  this change, at any tier.

## Capabilities

### New Capabilities
- `extension-platform`: how an extension is declared, discovered, enabled and hosted; the generic host contract; the places it may occupy; the catalog a surface is built from; events, pipes, storage, preferences and CLI commands; the reference gallery.
- `extension-program-contract`: the stdin/stdout/exit-code contract a source, action or command program answers, its output modes, limits and failure states.
- `extension-security`: grants, consent and re-consent, execution confinement, output sanitization, attribution of host chrome, and key shadowing.
- `extension-agent-bridge`: the normalized agent events, the agents read model, and the actions an extension may take on an agent, with their grants and safeguards.
- `extension-resident`: the resident tier: a WebAssembly component's lifecycle, its imports as grants, its budgets, and what it may answer.

### Modified Capabilities
- `input-keymap`: extension actions become keymap actions in an extension scope; a key an extension suggests never takes a chord the host has bound; a focused resident surface receives raw keys except the host's escape chord.
- `plugin`: a plugin may carry an extension capability, validated by `uze agent plugin check` and consented to before it runs.

## Impact

- **Crates**: `uze-extensions` (generic contract, declared-extension engine,
  catalog, markdown moved to shared), a new leaf crate for the extension
  manifest and wire types (no I/O, consumed by `uze-core` and
  `uze-extensions`), `uze-core` (capability parsing, grant record, sandboxed
  execution, the resident runtime), `uze-application` (extension read model,
  runner service, agents read model), `uze-integrations` (the reporting hook
  per harness, the skill-invocation dialect), `src/ui` (orchestrator,
  extension host, slots, keymap, management Extensions screen),
  `src/main.rs` (`uze ext`, `uze agent extension check`).
- **Persisted state**: one record (consented grants per extension, per
  digest, plus preferences) under `state/`, and one per-extension store,
  each declared through `uze_document::Shaped` and tiered by what deleting
  it costs.
- **Dependencies**: possibly `landlock` (Linux confinement), `schemars`
  (JSON Schema of the manifest and wire types) and a WebAssembly runtime,
  each justified per the dependency policy in design.md.
- **Architecture tests**: layering rules updated to admit exactly the new
  leaf crate; new rules for sanitization, for extension execution going
  through one sanctioned runner, and for agent actions going through the
  bridge.
- **Docs**: `docs/architecture/*.mmd` (crate layering, containers),
  `invariants.md`, the plugin authoring docs, the new Skill.
