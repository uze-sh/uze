## Why

`uze install` meant two operations behind one positional: with
`name@marketplace` it declared and installed one plugin, with nothing (or a
project root) it reproduced the project from `agents.yaml` and
`agents.lock`. Half of its flags (`-m`, `--alias`, `--replace`) worked in
one mode only, and `install -m` with no package was a usage error. The
package form only arrived in the flatten pass, as the landing spot for
`uze plugin install`; ADR-016 had already drawn the line the other way:
`uze flow@ai` declares, `git clone && uze install` reproduces.

The shorthand that declares has no name of its own in the root help, and
the verb it pairs with, `uze remove`, has no counterpart: package managers
say `add`/`remove` (cargo, pnpm, bun, uv), and `install` without an
argument is what they keep for reproducing a project.

## What Changes

- **New `uze add <plugin>@<market>`** declares and installs one plugin, with
  `-m`, `--alias`, `--replace`, `--trust` and `--format`. It is the
  canonical name of what `uze <plugin>@<market>` already did; the shorthand
  stays, as the same command, listed only among the root help's examples.
- **`uze install [path]` only reproduces the project.** **BREAKING**: it no
  longer takes a plugin spec, nor `-m`, `--alias` or `--replace`; `uze
  install x@y` is refused with `uze add x@y` as the next step. No alias or
  transition period: beta users move with the release.
- Every caller moves in the same change: tests, journeys, the Conformance
  Lab, demos, `make try`, docs, the site and the official plugin's Skills.
- The stale `uze add <path|git>` requirement in the `plugin` spec is
  removed: direct sources are refused (ADR-019), and `add` now names the
  marketplace operation.

## Capabilities

### Modified Capabilities

- `command-grammar`: `add` is the verb that declares, with the shorthand as
  its spelling without a verb; `install` reproduces and adds nothing.
- `plugin`: the direct-source `uze add` requirement goes; the Git
  marketplace scenario no longer compares to it.

## Impact

`src/cli/{grammar,install,help}.rs`, `src/main.rs`,
`src/command_performance.rs`, `crates/uze-testkit`, `tests/`, `journeys/`,
`conformance/`, `.demo/`, `Makefile`, `web/`, `docs/`, `plugins/`,
`.agents/`.
