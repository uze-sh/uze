## Context

See proposal.md for the motivation. This section covers the state on
`b09b1a92` (uze 1.0.0-beta.22) and the measurements this design rests on.

**Where the ties are.** Three plugins (`flow`, `hook-plugin`, `mcp-plugin`)
were installed for all four harnesses in one Lab sandbox. A grep for
`$UZE_HOME` under the harness homes found 14 files:

| harness | file | names |
|---|---|---|
| Claude | `plugins/known_marketplaces.json` | `runtime/attachments/claude/generated` (marketplace source) |
| Claude | `settings.json` | `runtime/attachments/claude/{hooks/exec,<rules engine>}`, `runtime/packages/*` |
| Claude | cache `skills/*/SKILL.md`, cache `.claude-plugin/plugin.json` | `runtime/packages/*` (`${PLUGIN_ROOT}` resolved by UZE) |
| Codex | `config.toml` | `[marketplaces.uze-store] source = runtime/attachments/codex/generated` |
| Codex | `hooks.json` | `runtime/attachments/codex/hooks/exec`, `runtime/packages/*` |
| Codex | cache `SKILL.md`, cache `.mcp.json` | `runtime/packages/*` |
| OpenCode | `opencode.json`, `plugins/uze.ts`, `skills/*/SKILL.md` | `runtime/attachments/opencode`, `runtime/packages/*`, `runtime/sessions/opencode` |
| Antigravity | `config/plugins/*/hooks.json`, `config/plugins/*/mcp_config.json`, `antigravity-cli/skills/*/SKILL.md` | `runtime/attachments/antigravity`, `runtime/packages/*` |

**What each vendor does with a marketplace source** (sandbox commands,
2026-11-01; Claude Code 2.1.316, Codex 0.166.0, Antigravity 1.4.12):

- **Claude.** `installed_plugins.json` records
  `installPath: ~/.claude/plugins/cache/uze-store/flow/1.0.0`, and that copy is
  complete (`skills/`, `agents/`, `guides/`, manifest). But `claude plugin list`
  says `Read from: <runtime/attachments/claude/generated/flow@uze-lab>`. After
  the source directory is moved away it reports `✘ failed to load — Marketplace
  uze-store failed to load: cache-miss`, and `claude plugin marketplace update`
  cannot refresh it. The `HarnessFact` measured on 2.1.283 still holds.
- **Codex.** It copies the plugin into
  `~/.codex/plugins/cache/<market>/<plugin>/<version>`. With the local
  marketplace root moved away, `codex plugin list` fails as a whole: *"marketplace
  root does not contain a supported manifest"*. `codex plugin marketplace remove`
  empties the installed list. The marketplace was re-added from a copy under
  `~/.codex/` and `runtime/` was then deleted; the plugin stays installed and
  enabled. `marketplace add` accepts a local path, `owner/repo`, HTTPS or SSH
  Git. It does not accept `file://`.
- **Antigravity.** `agy plugin install` copies into
  `~/.gemini/config/plugins/<name>`. The only ties are text inside that copy:
  hook commands, MCP commands and `${PLUGIN_ROOT}`.

So "compile into a throwaway cache, deliver through the official command,
discard" works for Antigravity only. For Claude and Codex the marketplace
source is read for as long as the plugin is installed. What decides the outcome
is *where* that source lives, not whether it exists.

## Goals / Non-Goals

**Goals:**
- Every file a harness reads, and every configuration entry UZE writes, is
  free of `$UZE_HOME`. A test enforces this, and conformance proves it on the
  real binaries.
- One copy fewer per package (`runtime/packages` goes), with no extra work
  per install.
- One command removes what UZE delivered, by the same receipts `remove` uses.

**Non-Goals:**
- Changing the delivery precedence (ADR-013) or any vendor command UZE runs.
- Workspace launch contributions (`--add-dir`, `-c`, session documents):
  they are per process, and the spec names them as the one exception.
- A Windows hook wrapper, and plugin versioning
  (`plugin-freshness-and-linked-marketplaces`).

## Decisions

### D1. One UZE directory per harness, inside the harness's root

Each integration names exactly one directory of its own:
`<claude_home>/uze/`, `<codex_home>/uze/`, `~/.config/opencode/uze/` and
`~/.gemini/uze/`. Each integration names its own; core names none, so the
vendor-neutrality tests still hold. Its layout is shared:

```
<harness root>/uze/
  marketplace/<market>/      # generated / explicit marketplace roots (Claude, Codex)
  packages/<id>/             # package roots for routes without a vendor copy
  bin/exec                   # the hook wrapper (ADR-040's file, relocated)
  rules/                     # the rules engine and its per-session files
```

Antigravity needs no `marketplace/`, because its install copies.

*Why not a `delivered/` tier under `$UZE_HOME`?* It would be honest about cost,
but `rm -rf ~/.uze`, the most common way to drop a tool, would still break
every harness. *Why not write into the vendor's own directories*, such as
`~/.claude/plugins/marketplaces/`? Those are the vendor's bookkeeping, and the
vendor may prune or rewrite them. A directory nobody else names is the vendor's
"user data" convention, and UZE already owns its loose skill directories the
same way.

### D2. Marketplace sources are compiled directly into the harness's directory

`shared::marketplace` stages each publication beside its destination in
`<harness>/uze/marketplace/<market>` and swaps it in whole, with the same
`replace_dir` stage-and-rename. Then UZE registers or refreshes it through the
vendor's CLI, exactly as now. `runtime/attachments/<h>/` keeps only staging and
is empty between runs.

*Rejected: a Git marketplace on `file://`.* Codex does not accept that scheme.
It would also add a Git repository per harness only to make the vendor clone
what UZE already wrote.

*Rejected: a throwaway build cache plus the official install.* Measured above:
Claude and Codex keep reading the source.

### D3. `${PLUGIN_ROOT}` resolves inside the harness's territory

There is still one resolver (`shared::package_root`), one meaning (the whole
package) and one per-harness answer:

- **Claude:** `${CLAUDE_PLUGIN_ROOT}`, which the vendor expands in skills,
  agents, hooks and MCP (measured, `HarnessFact` "placeholders"). The
  envelope already holds the whole package.
- **Codex:** the plugin's directory in `<codex_home>/uze/marketplace/…`,
  which is the source Codex installs from. The Codex envelope grows from
  `skills/` and `.mcp.json` to the whole package, through the same mirror the
  Claude envelope uses.
- **Antigravity:** `~/.gemini/config/plugins/<name>`, the vendor's own copy.
  The plugin name is UZE's to choose, so the path is known before install.
  The envelope carries the whole package.
- **Loose routes** (no plugin, or OpenCode): `<harness>/uze/packages/<id>`.

A write into the root lands in a directory the harness owns. It is not the
Store, which keeps the reason `deliver-the-whole-plugin` gave for the copy. The
next delivery replaces the root whole when its source digest differs.

### D4. A package root is owned by source digest, not by tree hash

A new receipt variant, `PackageRoot { path, source_digest }`, replaces the
`tree_sha256` inspection for package roots:

- **Inspection** reads only the receipt (O(1)). A root whose recorded digest
  equals the Store package's digest is left alone.
- **Writes into a root** by its own package's hooks are expected. They are
  not drift: a root is never the operator's to edit.
- **Detach** removes the directory whole.

Loose skill and agent directories keep `GeneratedTree`, because an edit there
is the operator's. The new shape climbs the persisted-state ladder: a receipt
naming a `runtime/packages` path is retired by inspect-before-detach and
re-attached in the harness's directory.

### D5. Setup's machine artifacts are deliveries with receipts

The following are attached through the same lifecycle as a package, with
receipts in `attachments.json`:

- the rules engine;
- OpenCode's `uze.ts` and its hook-group documents;
- the wrapper;
- the bundled `uze` plugin.

`remove` keeps refusing the bundled plugin, and uninstall is the one caller
allowed to detach it. The rules engine's per-session files move to
`<harness>/uze/rules/sessions` and OpenCode's to `<harness>/uze/sessions`. Both
are created on demand, so an engine running with `$UZE_HOME` gone keeps
working.

The bundled skills that call `uze` gain one opening line: when `uze` is not
on `PATH`, the skill does not apply.

### D6. `uze uninstall` is plan, then execute, over receipts

`UzeApplication::uninstall_plan()` returns a read model with these parts:

- per harness, the receipts and their inspection;
- the setup artifacts;
- the records directory and its size;
- the live agents;
- checkouts with unlanded work;
- the provisioned harnesses;
- how the binary was installed.

`execute` then runs these steps in order:

1. Refuse if agents are live.
2. Stop the terminal runtime (`ending-processes` rule).
3. Detach every `MATCHED` receipt through its integration: vendor uninstall,
   `mcp remove`, marketplace remove, then directory removal.
4. Remove each harness's `uze/` directory if it is now empty of anything
   unowned.
5. Remove `runtime/`, `cache/` and `store/`, plus `state/` under `--purge`.
6. Remove the binary when the installer's receipt names it.

Each step is idempotent, and the plan is recomputed on every run.

The command is classified `JustifiedSlow`, because it runs vendor CLIs.

### D7. Proof

- **Unit:** `vendor_neutral.rs::no_harness_artifact_names_uze_home` installs
  the canonical fixtures for all four fake harnesses and walks every receipt's
  artifact and every configuration entry for the `$UZE_HOME` prefix.
- **Acceptance:** `tests/acceptance/uninstall.rs` covers install,
  then uninstall, then no UZE directory and no reference left. It also covers
  an edited skill that is kept, and a second run that is clean.
- **Conformance:** a contract check per vertical moves `$UZE_HOME` away, then
  checks that the plugin lists and loads, a deny hook denies, and a rules
  `always_on` reaches the session.

## Risks / Trade-offs

- **[A vendor starts scanning `<root>/uze/`]** → the conformance check
  "harness ignores the directory" (spec) fails first. The name is one
  constant per integration.
- **[Codex's whole-package envelope is larger in its cache]** → this is the
  same trade Claude already made, and `node_modules`-sized trees are excluded
  at ingest.
- **[The Antigravity plugin path depends on the vendor's naming]** →
  `HarnessFact` records it with the version it was measured on, and
  conformance asserts it.
- **[Uninstall runs while a harness session is open]** → Claude reads the
  marketplace live, so an open session loses the plugin mid-session. The
  plan names the harnesses that have running processes. This is the same
  behaviour as `remove`.
- **[Migration re-delivers every package once]** → this cost is paid on the
  first `install`, `update` or `setup` after upgrade, and that run is
  reported.

## Migration Plan

1. Add the ladder rung for receipts that name `runtime/packages` or
   `runtime/attachments`.
2. Run `setup` / `install` / `update`. For each package and harness, retire
   the old artifact (matched → detached) and attach in the new location.
   Re-register each marketplace with its new source through the vendor CLI,
   never by removing it first. Measured on Claude 2.1.316: `marketplace add`
   with the same name repoints it and keeps the installed plugins ("now points
   at dir:…/.claude/uze/marketplace/uze-store"), while `marketplace remove`
   uninstalls them. Once repointed, the plugin stays `✔ enabled` with the
   whole of `$UZE_HOME` moved away. Measured on Codex 0.166.0: after
   `marketplace remove` the plugin disappears from `plugin list`, and an
   `add` of the same name from `<codex_home>/` lists it as installed and
   enabled again, because `config.toml` kept `[plugins."flow@uze-store"]`.
3. Afterwards, `runtime/attachments/*` and `runtime/packages` are
   staging-only and are deleted.

Rollback: an older build reads the new receipt shape as one from a newer
build and refuses it, per persisted-state. The operator reinstalls with the
older build after `uze uninstall`.

## Candidate ADRs

- **What a harness reads lives in the harness's territory**: this amends
  ADR-052 (the generated tier is never read by a harness) and ADR-040 (where
  the wrapper lives). It binds every future integration.
