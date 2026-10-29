## Context

The platform's design lives in `extension-platform/design.md`; this
change carries out its change 4 (*Proposed split*) and cites its
decisions by number rather than restating them: D2 (the contract as
built), D3 (`Host::answer`), D4 (the session cache), D5 (consent), D7
(`Clean`), D8 (the keymap's second table), D17 (reasons to refresh), D19
(the `rhai` dependency), D27 (scripts first) and D28 (the budget). The
group-0 decisions of 2026-10-26 are final there.

What exists: `capability::extension::discover` and the bounded KDL
decoder in `uze-core`; `uze_contract::{Manifest, Identity, Clean,
RESERVED_IDS}`; `Extensions::{installed, check, disabled, set_enabled}` in
`uze-application`; the declared engine in `uze-extensions/src/declared.rs`
hosted on the slot path (`src/ui/orchestrator/declared.rs`,
`answers.rs::schedule_engine_jobs`).

## Goals / Non-Goals

**Goals:** a plugin from a marketplace nobody links, installed, consented
to, enabled in one project, drawing a `view` and a `section` from a Rhai
script under `read:checkout`, with an action that refreshes and one that
confirms, driven by key and pointer; `gsd-band`'s board from `board.rhai`
refreshed by `paths.changed` and `every`.

**Non-Goals:** programs, `exec:` and OS confinement (change 5); the store
(5); preferences, compose, the `overlay`/`status`/`band`/`notify` slots and
the agent bridge (6); media and the resident tier (7); `net`, `fetch`,
secrets and `edit:checkout` (8).

## Decisions

### M1. Order of the work
Manifest and check, then the consent record, then the keymap, then
`uze-script` and `Host::answer`, then sources and actions, then events,
then the operator's surfaces, then docs. Each step lands green on its own;
nothing third-party runs until the consent record exists.

### M2. The consent record is the workspace domain's
`state/extensions.json` (shape 1, `uze_document::Shaped`) sits beside the
existing switch (`uze_workspace::extensions`), which it absorbs: a
built-in keeps its machine-wide switch, a declared extension is keyed by
`Identity` and carries `accepted_grants`, `accepted_digest`,
`accepted_at`, `enabled_in` (project roots as ADR-055 names them) and the
fields change 5 fills (`exec_paths`, `enforced_at_accept`). The old
`disabled` list is read as shape 0 and carried forward.

### M3. The digest is of what was loaded
`uze-application` reads the extension's directory once into memory,
bounded (file count and bytes), digests it (SHA-256 over sorted relative
paths and contents) and compiles from those bytes. A linked marketplace's
save produces a new digest that is accepted without asking only while the
grant set is unchanged (D5, D25).

### M4. Only the runner runs
`uze-extensions` gains `Host::answer(Question) -> Answer`; the question
carries the identity, a declaration id and the input. The runner in
`uze-application` looks the declaration up in the compile the operator
consented to, checks the checkout (D6's definition), and calls
`uze-script`. A fixture is answered the same way, so the engine has one
path for data.

### M5. `uze-script` is a leaf
It depends on `uze-contract` and `uze-platform` only, names `rhai` alone
(a layering test), and exposes `Script::compile(bytes) -> Result<Script>`
and `Script::answer(&Script, input, &Grants, &dyn Reach) -> Envelope`,
where `Reach` is the host's narrow interface for the granted functions
(`read_checkout`, `list_checkout`, `clock`). The engine is built raw per
D19 with the limits of D28.

### M6. Rate and staleness are the host's
`every` has a 5-second floor and runs only while a surface showing the
source is visible; `paths.changed`/`checkout.changed` come from the
one-second watch and coalesce in 250 ms; an answer to a question that is
no longer the latest is dropped.

## Risks / Trade-offs

- [Rhai bounds values, not memory] → the operation and size limits bound
  it indirectly; the per-extension measure of D28 is the backstop.
- [Enabling per project adds a prompt per project] → the same cost ADR-055
  already asks for a project's commands; fixtures keep authoring prompt-free.
- [The old switch file changes shape] → read as shape 0 and carried
  forward, never rewritten by an older build.

## Migration Plan

The switch file is read as shape 0; built-ins keep their state. A declared
extension that only declares fixtures runs nothing and keeps drawing as it
does today, with no consent asked; one that declares a script asks for
consent and enabling in the project before its first run, linked or not.
