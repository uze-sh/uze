## Why

UZE delivers in each harness's own format, yet what it delivers still depends
on `$UZE_HOME`. On a fresh Lab machine (Claude Code 2.1.316, Codex 0.166.0,
OpenCode 2.2.5, Antigravity 1.4.12, uze 1.0.0-beta.22) with three plugins
installed, 14 files under the four harness homes name a path inside
`~/.uze/runtime`. Each is a marketplace source, a `${PLUGIN_ROOT}`, a hook
wrapper, the rules engine or a session file. Two of these ties are measured to
break a harness when `~/.uze` goes away:

- Claude reads a directory-marketplace plugin from its source even though it
  keeps a complete copy in `~/.claude/plugins/cache`. With the source gone,
  `claude plugin list` reports `failed to load: cache-miss`.
- Codex refuses to load its whole plugin list once a configured local
  marketplace root is missing. Removing the marketplace registration
  uninstalls its plugins.

Other ties are deny hooks that fail closed on a missing wrapper, rules entries
pointing at a missing engine, and MCP commands and skill text naming
`runtime/packages`. These contradict persisted-state's own tier rule. That
rule says the generated tier costs nothing to delete, but every harness reads
it.

The other half of the problem is that there is no way out. `remove` undoes one
package at a time. Nothing undoes `setup`: the rules engine, `uze.ts`, the
shims and the bundled `uze` plugin's copies remain until someone finds and
deletes them by hand.

## What Changes

- **Deliveries live in the harness's territory.**
  - Each harness gets exactly one UZE-owned directory inside its own root, for
    example `~/.claude/uze/` and `~/.codex/uze/`. Its marketplace source, the
    package roots its plugins name, the hook wrapper and the rules engine all
    live there. Everything in it is owned through receipts.
  - No file a harness reads and no harness configuration entry names a path
    under `$UZE_HOME`. A test over every receipt holds this.
- **The official mechanism is unchanged.** `claude plugin install`,
  `codex plugin install`, `agy plugin install`, `claude mcp add` and the rest
  stay as they are. What moves is the directory those commands are pointed
  at.
- **`runtime/packages` is removed.** `${PLUGIN_ROOT}` keeps its one meaning,
  the whole delivered package, but resolves to the package directory inside
  the harness's own UZE directory, or to the vendor's own copy where the
  vendor keeps one. Codex and Antigravity envelopes carry the whole package,
  as Claude's already does.
- **`runtime/` becomes truly generated.** It holds only build staging and
  per-process material (shims, launch context). Deleting it changes nothing a
  harness does.
- **Machine-level deliveries become ordinary deliveries.** The rules engine,
  OpenCode's `uze.ts` and its hook groups, and the bundled `uze` plugin are
  attached with receipts through the same path as any package. Skills that
  call `uze` say they do not apply when it is absent.
- **New `uze uninstall`.** A machine-scope command that shows a plan and, once
  confirmed, does the following:
  - Detaches every receipt that still matches what UZE wrote. Edited or
    conflicting ones are left in place and listed.
  - Refuses while agents UZE launched are alive, and stops the terminal
    runtime.
  - Keeps the operator's records unless `--purge` is given.
  - Never touches a project or unlanded work.
  - Removes `$UZE_HOME`, and the binary when UZE's own installer placed it.
- **BREAKING (on-disk only):** existing installs are re-delivered to the new
  locations on the first `install`/`update`/`setup` after upgrade. Receipts
  in the old shape climb the persisted-state ladder, and the old artifacts
  are retired by the existing inspect-before-detach rule.

## Capabilities

### New Capabilities
- `machine-uninstall`: undoing everything UZE did to a machine. That covers
  the plan, the receipt-driven detach, what is kept and why, the
  refusal while agents live, and the removal of UZE itself.

### Modified Capabilities
- `transparent-harness-attachment`: a harness reads nothing under
  `$UZE_HOME`. Each harness has one UZE-owned directory inside its own root.
  A marketplace UZE registers has its source there.
- `package-delivery-fidelity`: `${PLUGIN_ROOT}` resolves to the package
  inside the harness's own territory, never to a UZE-home copy.
- `portable-hooks`: a delivered hook keeps working after `$UZE_HOME` is
  deleted, not only after the binary is removed.
- `persisted-state`: deleting the generated tier changes nothing a harness
  does.

## Impact

- `crates/uze-integrations`:
  - each vertical's marketplace, envelope, skills, MCP, hooks and rules modules
    name the harness-side directory;
  - `shared/marketplace.rs` and `shared/tree.rs` stage into it;
  - Codex and Antigravity envelopes mirror the whole package.
- `crates/uze-core`:
  - `machine/home.rs` loses `delivered_packages_dir` and narrows
    `runtime_attachments_dir` to staging;
  - `delivery/delivered_root.rs` resolves per harness;
  - a new receipt shape, plus its ladder rung.
- `crates/uze-application`: an uninstall service (plan, then execute); setup's
  machine artifacts are attached as receipts; the bundled plugin can be
  detached by uninstall alone.
- `src/cli/`: the `uninstall` verb, its report and its performance class.
- Tests: `tests/integrations/vendor_neutral.rs` gains the UZE-home
  reference test over every receipt; `tests/acceptance/` gains an uninstall
  journey; existing projection tests that pin `runtime/packages` change.
- Conformance:
  - every vertical asserts that the harness still loads plugins, hooks and
    rules with `$UZE_HOME` moved away;
  - an uninstall check leaves no file citing it.
- Docs: `docs/capabilities/portable-hooks.md`, `invariants.md`,
  `docs/architecture/install-pipeline.mmd` and `attachment-lifecycle.mmd`.
