## Why

"The built-in marketplace is never declared" was written when the
marketplace built into uze held only the bundled `uze` plugin, which
bootstrap installs on every machine, so a project declaring it said
nothing. It now carries plugins bootstrap does not install (`pingpong`,
and the official extensions), and a project that uses one cannot say so:
`uze add focus@uze` in a project writes nothing, and the workspace
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
- `agents.lock` records, at its top, the uze version its built-in plugins
  were resolved from, and each of them by the digest of its bytes; the
  built-in marketplace has no entry under `marketplaces:`.
- Built-in plugins follow the binary that runs: bytes another uze build
  left, on the machine or in the lock, are replaced by this binary's and
  reported as updated, never refused, since the binary is the only source.
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
- `agents-lock`: built-in plugins are pinned by the uze version and their
  bytes, follow the binary, and have no marketplace entry.
- `marketplace`: the embedded official marketplace is registered as `uze`,
  and an earlier release's `uze-official` records are moved under it.

## Impact

- `uze-core`: `manifest` validation and `BUNDLED_PLUGINS`, `project_lock`
  (a top-level `uze:`), `BUILT_IN_MARKETPLACE`.
- `uze-application`: `project_environment` (`add`, `plan`, install and the
  lineage check), `setup` (retiring `uze-official`), `bootstrap`.
- Repository: `marketplace.json`, `plugins/workspace`, `plugins/uze`'s
  `try` Skill, `Makefile`, `.demo`, `playground/`, docs.
- Journeys and tests that install the official examples or read the lock.
