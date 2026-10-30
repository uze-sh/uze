## Why

"The built-in marketplace is never declared" was written when the
marketplace built into uze held only the bundled `uze` plugin, which
bootstrap installs on every machine, so a project declaring it said
nothing. It now carries plugins bootstrap does not install (`pingpong`,
and the official extensions), and a project that uses one cannot say so:
`uze install focus@uze` in a project writes nothing, and the workspace
offers a project only the extensions it declares. The only way around it
was registering this same repository a second time, under a second name,
which the marketplace's rename to `uze` now refuses.

## What Changes

- A project MAY declare plugins from the built-in marketplace: `agents.yaml`
  lists them under `uze` with no source (`marketplaces: { uze: { plugins:
  [workspace] } }`). A `git:` or `path:` on that name is still refused: the
  marketplace has no source but the binary.
- A bundled plugin (installed by bootstrap on every machine, today `uze`)
  is still never written into a project: it is every project's already.
- `agents.lock` pins the built-in marketplace by the uze version that
  resolved it, and each of its plugins by the digest of its bytes, as any
  plugin is pinned.
- A lock written by another uze version is not a failure: the plugins are
  resolved again from this binary and the lock rewritten, the way a
  marketplace that moved past its pin is an update. A digest that differs
  under the same version still fails, as any pin does.
- The official marketplace is named `uze` (was `uze-official`); a machine
  an earlier release seeded is moved under the new name once.
- **BREAKING** (no released users): the `gsd`, `focus` and `contributions`
  plugins become one plugin, `workspace`, carrying the three extensions; an
  extension's identity follows its plugin (`workspace@uze/focus`).
- `/uze:try`, `make try`, the demos and the playgrounds declare
  `workspace@uze` from the built-in marketplace instead of registering a
  copy of this repository; `make examples` gives way to `make pingpong-mcp`.

## Capabilities

### New Capabilities

### Modified Capabilities
- `agents-manifest`: the built-in marketplace's plugins may be declared;
  only a source on its name, and a bundled plugin, are refused.
- `agents-lock`: a built-in marketplace entry pins the uze version, and an
  entry another version wrote is resolved again rather than refused.
- `marketplace`: the embedded official marketplace is registered as `uze`,
  and an earlier release's `uze-official` records are moved under it.

## Impact

- `uze-core`: `manifest` validation, `project_lock` (`LockedMarketplace`
  gains the built-in form), `BUILT_IN_MARKETPLACE`.
- `uze-application`: `project_environment` (`add`, `plan`, install and the
  lineage check), `setup` (retiring `uze-official`), `bootstrap`.
- Repository: `marketplace.json`, `plugins/workspace`, `plugins/uze`'s
  `try` Skill, `Makefile`, `.demo`, `playground/`, docs.
- Journeys and tests that install the official examples or read the lock.
