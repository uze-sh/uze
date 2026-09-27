> **Status: frozen (2026-09-27).** Two reviews (architecture fit, security)
> found blockers that prevent implementation as written. They are recorded in
> design.md under *Unresolved review findings*, alongside the proposed split
> into five changes and the product decisions still owed. Do not apply this
> change until those are resolved and the plan is refined.

## Why

The workspace client's extensions (`code`, `architect`) can only be added by
editing UZE and rebuilding it: each one is wired into the orchestrator by
hand, through its own fields, its own outcome enum, its own async pair and
its own `ExtensionHit` variant. Nobody outside this repository can put a
surface in front of an operator, even when the surface is a list over a CLI
they already have (an `openspec` board, a CI panel, an issue list).

The seams that would let a third party in were drawn on purpose and have
held: an extension describes a `View` and the host draws it (ADR-041), and
whatever it needs from the machine it asks the `Host` for, which grants it
narrowly (ADR-048). What is missing is everything past those seams: a
generic place in the host, a way to declare an extension without Rust, a
contract a program in any language can answer, and a trust model for code
UZE runs on the operator's machine. That last one gates the rest, because a
loading mechanism chosen without a capability model cannot be given one
afterwards (ADR-041).

## What Changes

- **The host knows extensions generically.** One contract every extension
  implements, a registry that constructs them instead of listing metadata,
  one routing path for keys, pointer hits, outcomes, async requests and
  remembered place. `code` and `architect` move onto it first, as the proof
  the contract is sufficient; no surface changes for the operator.
- **A plugin can ship an extension.** `extensions/<id>/extension.yaml`
  becomes a plugin capability. It declares the extension's surfaces (a
  full-frame view or a sidebar section), the data sources behind them, its
  actions and their suggested keys, the lifecycle events it reacts to, the
  CLI commands it adds, and the grants it needs. It carries no logic.
- **Programs in any language answer one contract.** A source, an action or
  a command is an argv the host runs with JSON on stdin, JSON on stdout and
  the portable hook exit codes (0 answers, 3 refuses with a reason, anything
  else fails), bounded in time and output. The host holds all view state;
  programs are stateless and short-lived, so an installed extension costs no
  resident process.
- **A richer view catalog.** The host renders, from declarations, a
  vocabulary broad enough to build real tools without drawing: grouped and
  tree lists, markdown, highlighted code, tables, key/value details,
  progress, badges, bar and sparkline charts, empty and error states,
  confirmations, menus, toasts. Markdown rendering moves from the `code`
  extension into the host so every surface shares one.
- **A security model for extension code.** Grants declared in the manifest
  and consented to at install, recorded against the package digest, re-asked
  when an update widens them; argv-only execution with no shell; programs
  resolved inside the package or through a named grant; a scrubbed
  environment; OS confinement where the platform offers it; every string an
  extension produces sanitized before it reaches the terminal; host chrome
  (confirmations, toasts, consent) always attributed and never drawable by an
  extension; extension keys never shadow the host's.
- **Authoring tools and a skill.** `uze agent plugin create --extension`,
  `uze agent extension check <path>` (schema, bindings, and a real dry run
  of every source), `uze ext <extension> <command>` for declared commands,
  and a dedicated `uze:extension` Skill, linked from `uze:author`.
- The long-lived interactive protocol (free drawing, editors) is **not** in
  this change; `kind: interactive` is reserved and refused with a reason.

## Capabilities

### New Capabilities
- `extension-platform`: how an extension is declared, discovered, enabled and hosted; the generic host contract; the view catalog a declaration may use; lifecycle events and CLI commands an extension may add.
- `extension-program-contract`: the stdin/stdout/exit-code contract a source, action or command program answers, its limits and failure states.
- `extension-security`: grants, consent and re-consent, execution confinement, output sanitization, attribution of host chrome, and key shadowing.

### Modified Capabilities
- `input-keymap`: extension actions become keymap actions in an extension scope; a key an extension suggests never takes a chord the host has bound.
- `plugin`: a plugin may carry an extension capability, validated by `uze agent plugin check` and consented to at install.

## Impact

- **Crates**: `uze-extensions` (generic contract, declared-extension engine,
  catalog, markdown moved to shared), a new leaf crate for the extension
  manifest and wire types (no I/O, consumed by `uze-core` and
  `uze-extensions`), `uze-core` (capability parsing, grant record, sandboxed
  execution), `uze-application` (extension read model and runner service),
  `src/ui` (orchestrator, extension host, keymap, management Extensions
  screen), `src/main.rs` (`uze ext`, `uze agent extension check`).
- **Persisted state**: one new record (consented grants per extension, per
  digest) under `state/`, declared through `uze_document::Shaped`; one
  remembered cache (last source answers) under `cache/`.
- **Dependencies**: possibly `landlock` (Linux confinement) and `schemars`
  (JSON Schema of the manifest and wire types), each justified per the
  dependency policy in design.md.
- **Architecture tests**: layering rules updated to admit exactly the new
  leaf crate; new rules for sanitization and for extension execution going
  through one sanctioned runner.
- **Docs**: `docs/architecture/*.mmd` (crate layering, containers),
  `invariants.md`, the plugin authoring docs, the new Skill.
