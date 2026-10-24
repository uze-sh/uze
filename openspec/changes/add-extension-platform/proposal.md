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
  remembered place. The view engine is its first user; `code`,
  `architect` and `spec` move onto it after, with no surface changing for
  the operator.
- **A plugin can ship an extension.** `extensions/<id>/extension.kdl`
  becomes a plugin capability. It declares the extension's surfaces, the
  data sources behind them, its actions and their suggested keys, the events
  it reacts to, its preferences, the CLI commands it adds, and the grants it
  needs. It carries no logic.
- **Three tiers behind one manifest.** A surface declares how it is fed, and
  each tier costs more trust and more machine than the one before:
  - *declared*: a stateless answer (JSON or lines) the host builds the
    surface from, through a KDL view;
  - *cells and media*: still stateless, an answer may also be a grid of
    themed cells with clickable regions, an image, or a sprite whose frames
    the host animates from an agent's state;
  - *resident*: a WebAssembly component that keeps state, receives input and
    timers, and answers a catalog tree or cells every frame, reaching nothing
    but the host imports it was granted.
- **Places an extension may occupy.** A full-frame view (with a host
  attribution strip it cannot cover), a sidebar section, a row in the
  sidebar's header, a band of fixed rows along the edge of an agent's pane,
  an overlay (picker, form, dialog) under host chrome, and a notification.
  An extension never draws over a harness's own output.
- **A catalog of primitives in families.** Layout, text, data (list, tree,
  table, details), measure (segmented progress, gauge, pips, bar, sparkline,
  heatmap), status (badge, several marks per row, chips, an activity mark
  and elapsed time the host animates), media, cells, input (button, toggle,
  select, text field, form, fuzzy picker); toasts, notices, notifications and
  sounds are requested from the host, never drawn by a node. A surface is
  either *bound* (a KDL view over a source's data) or *composed* (the answer
  is a tree of catalog nodes). Colour is named only as a semantic role.
- **Views, manifests and tests in KDL v2.** Catalog nodes in PascalCase,
  fixed binding rules with no expressions (references, `each`,
  `when`/`unless`, `show`, templates, lookup tables with locales), UI state
  held by the host, parsed only at check, install, enable and reload
  behind UZE's own bounds, and highlighted in UZE's editor.
- **A component first, a program when it must.** A source or an action is
  answered by a stateless WebAssembly component (the default, confined on
  every platform) or by an argv program (to wrap a CLI, or a quick shell
  script), both on one contract: a JSON input (its slot, size, agent, and
  the event, pipe, store, preferences and fields it names, also as argv or
  `UZE_IN_*` for a shell), a JSON, lines or media answer, optionally in an
  envelope of requests, and the portable hook exit codes, bounded in time
  and output. The host holds all view state; nothing stays resident unless
  the surface is resident.
- **Live development without a mode.** An extension in a linked marketplace
  (the authoring path UZE already has) reloads on save; every extension keeps
  its last good draw on an error and has an inspector, which on a linked
  extension also feeds fixtures and emits synthetic events;
  `uze agent plugin check` runs headless tests with cell snapshots; `uze
  agent plugin lsp` completes manifests, views and tests.
- **A bridge to the agents, across harnesses.** A reporter UZE adds to each
  launch tells the workspace each agent's turns, tool calls and approvals as
  normalized events, which are drawn but never trusted to release a write;
  a read model answers the agents (harness, state, work, checkout). An
  extension may send one named agent a prompt or a skill invocation, which
  its integration spells in its harness's dialect, filtered, shown as sent,
  and refused when the pane is not ready. Unattended, it may send only text
  its manifest fixes, under a per-project, per-agent grant.
- **Richer reasons to refresh.** An interval with a floor, and declared
  paths changing, including paths Git ignores.
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
- **No new command noun.** An extension is a plugin capability: it is
  installed, updated and removed with its plugin, reported by `uze status`
  and `uze inspect`, enabled on the Extensions screen, and checked and
  tested by `uze agent plugin check`; `uze agent plugin create --extension`
  scaffolds one, and a dedicated `uze:extension` Skill, linked from
  `uze:author`, teaches it. Commands an extension adds to the CLI and pipes
  from outside are left out until a gallery entry needs them.
- Rewriting a harness's own transcript, raw terminal output from an
  extension, and native code loaded into UZE's process are **not** part of
  this change, at any tier.

## Capabilities

### New Capabilities
- `extension-platform`: how an extension is declared, discovered, enabled and hosted; the generic host contract; the places it may occupy; the catalog a surface is built from; events, storage and preferences; authoring through linked marketplaces; the reference gallery.
- `extension-program-contract`: the stdin/stdout/exit-code contract a source, action or command program answers, its output modes, limits and failure states.
- `extension-security`: grants, consent and re-consent, execution confinement, output sanitization, attribution of host chrome, and key shadowing.
- `extension-agent-bridge`: the normalized agent events, the agents read model, and the actions an extension may take on an agent, with their grants and safeguards.
- `extension-resident`: the resident tier: a WebAssembly component's lifecycle, its imports as grants, its budgets, and what it may answer.

### Modified Capabilities
- `input-keymap`: extension actions become keymap actions in an extension scope; a key an extension suggests never takes a chord the host has bound; a focused resident surface receives raw keys except the host's escape chord.
- `plugin`: a plugin may carry an extension capability, validated by `uze agent plugin check` and consented to before it runs.

## Impact

- **Crates**: `uze-extensions` (generic contract, declared-extension engine,
  catalog, markdown moved to shared), a new leaf crate `uze-contract` for
  the extension manifest and wire types (no I/O, consumed by `uze-core` and
  `uze-extensions`, the one UZE crate `uze-extensions` may depend on, by an
  amendment to ADR-041), `uze-core` (the bounded KDL decoder, grant record,
  the runner), `uze-platform` (confinement, one implementation per
  platform), `uze-application` (extension read model, runner and view
  compiler services), `uze-integrations` (the reporter in each launch, the
  skill-invocation spelling), a new `uze-wasm` crate (the
  resident tier), `src/ui` (orchestrator, the agents read model,
  extension host, slots, keymap, management Extensions screen),
  `src/cli` (the extension flags of `uze agent plugin create|check`, `uze
  agent plugin lsp`, extensions in `uze status` and `uze inspect`).
- **Persisted state**: one record (consented grants per extension, per
  digest, plus preferences) under `state/`, one store per extension and
  project, and one bounded journal per agent, each declared through
  `uze_document::Shaped` and tiered by what deleting it costs. Secrets go to
  the OS keychain through `uze-platform`.
- **Dependencies**: possibly `landlock` (Linux confinement), `schemars`
  (JSON Schema of the manifest and wire types), a WebAssembly runtime and
  an HTTP client for it,
  each justified per the dependency policy in design.md.
- **Architecture tests**: layering rules updated to admit exactly the new
  leaf crate; new rules for sanitization, for extension execution going
  through one sanctioned runner, and for agent actions going through the
  bridge.
- **Docs**: `docs/architecture/*.mmd` (crate layering, containers),
  `invariants.md`, the plugin authoring docs, the new Skill.
