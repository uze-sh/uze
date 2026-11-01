## Context

See proposal.md for the motivation. State on `b09b1a92` (uze 1.0.0-beta.22).
Revision 3: the delivery target moved from a directory inside each vendor's
root (revisions 1–2) to `~/.uze/store/<vendor>/`. This was decided with the
operator for four reasons:

- UZE keeps one root it controls.
- The vendors' territory stays untouched.
- The invariant "every path UZE owns is named in `UzeHome`" holds without a
  new port method.
- The layout is the same on all three OSes.

The cost is accepted on purpose: removing `~/.uze` by hand is unsupported.

**Where the ties are.** Three plugins (`flow`, `hook-plugin`, `mcp-plugin`)
were installed for all four harnesses in one Lab sandbox. A grep for
`$UZE_HOME` under the harness homes found 14 files:

| harness | file | names |
|---|---|---|
| Claude | `plugins/known_marketplaces.json` | `runtime/attachments/claude/generated` |
| Claude | `settings.json` | `runtime/attachments/claude/{hooks/exec,<rules engine>}`, `runtime/packages/*` |
| Claude | cache `SKILL.md`, cache `plugin.json` | `runtime/packages/*` |
| Codex | `config.toml` | `[marketplaces.uze-store] source = runtime/attachments/codex/generated` |
| Codex | `hooks.json` | `runtime/attachments/codex/hooks/exec`, `runtime/packages/*` |
| Codex | cache `SKILL.md`, cache `.mcp.json` | `runtime/packages/*` |
| OpenCode | `opencode.json`, `plugins/uze.ts`, `skills/*/SKILL.md` | `runtime/attachments/opencode`, `runtime/packages/*`, `runtime/sessions/opencode` |
| Antigravity | `config/plugins/*/{hooks.json,mcp_config.json}`, `antigravity-cli/skills/*/SKILL.md` | `runtime/attachments/antigravity`, `runtime/packages/*` |

The code also writes ties to places outside the harness homes:

- `~/.claude.json`, for user-scope MCP;
- `~/.agents/skills`;
- `~/.gemini/config/{hooks,mcp_config}.json`;
- the workspace's links `.worktrees/<id>~uze`, which point into
  `runtime/projects`.

**What each vendor does with a marketplace source** (sandbox commands,
2026-11-01):

- **Claude 2.1.316**
  - Loads a local marketplace *in place*: *"it loads in place from …, so
    edits there take effect at the next session start"*. The complete copy in
    `plugins/cache` is not what it reads.
  - Source gone: `✘ failed to load — cache-miss`.
  - A same-name `marketplace add` repoints the marketplace and keeps its
    plugins. `marketplace remove` uninstalls them.
  - `file://` is refused (*"Invalid marketplace source format"*).
- **Codex 0.166.0**
  - Copies the plugin into `plugins/cache/<m>/<p>/<v>`, but needs the local
    marketplace root: without it, `plugin list` fails as a whole.
  - A same-name add from another root is refused; `remove` must come first.
    After `remove`, `config.toml` keeps `[plugins."<id>"] enabled = true`,
    and an `add` from the new root lists the plugin as installed and enabled
    again.
  - `file://` is refused.
- **Antigravity 1.4.12**
  - `agy plugin install` copies into `config/plugins/<name>`.

## Goals / Non-Goals

**Goals:**
- A harness reads nothing under `$UZE_HOME` except its own `store/<vendor>/`.
  A test over the whole fake home and conformance on the real binaries prove
  it.
- One build per package per harness, which is also the package root.
  `runtime/packages` goes away, so UZE keeps fewer copies than today.
- One command removes UZE, with or without what it delivered.

**Non-Goals:**
- Changing the delivery precedence (ADR-013) or the vendor commands UZE
  runs.
- Keeping harnesses intact when `~/.uze` is deleted by hand. That is
  unsupported, and `doctor-repair` restores what it can.
- Workspace launch arguments (`--add-dir`, `-c`, session documents), which
  live as long as one process.

## Decisions

### D1. The tiers: sources, store, records, disposable

```
~/.uze/
  sources/<id>/              BYTES     canonical package bytes (today's store/, renamed)
  store/<vendor>/            STORE     what that harness reads
    <id>/                    the build of <id> for <vendor> = plugin + ${PLUGIN_ROOT}
    .uze/                    hooks/exec(.ps1), rules engine, rules/sessions,
                             OpenCode hook and session documents
  state/                     RECORD    receipts, registry, tasks, memory
  runtime/  cache/           GENERATED / REMEMBERED: deleting them changes no harness
  shims/  config.toml …
```

The new **store** tier is defined by what deleting it costs: it uninstalls
that harness's plugins. `Store` (the Rust type) keeps its name and keeps
its invariant, writing no harness artifact. Its directory becomes
`sources/`. Each vendor store is written by its integration, through a
`UzeHome::store_dir(vendor)` accessor, in the same way
`generated_attachments_dir(vendor)` is today.

*Rejected: a directory inside each vendor's root* (revisions 1–2). It
survives a manual delete of `~/.uze`, but it spreads ownership across five
roots and needs a port method and an ADR exception.

*Rejected: XDG/OS data directories.* The separation is convention on Linux
only. On Windows everything collapses into `%LOCALAPPDATA%`, and it would
take four roots to migrate.

### D2. A build is the plugin and the package root

Each build is rendered from `sources/<id>` for one harness and carries the
whole package:

- manifest;
- `SKILL.md` with that harness's invocation frontmatter;
- agents in that harness's format;
- MCP in that harness's file;
- hooks compiled to that harness's dialect;
- every supporting file.

`${PLUGIN_ROOT}` in a build resolves to the build's own directory. The rule
is the same on every route of that harness, because loose routes (`settings.json`
hooks, `claude mcp add`, loose skills and agents) point into the build too.

| vendor | the build is | the vendor's own copy |
|---|---|---|
| Claude | its marketplace entry; read in place | `plugins/cache`, unused |
| Codex | its marketplace entry; must exist | `plugins/cache`, where it runs |
| Antigravity | the install source and the root | `config/plugins/<name>` |
| OpenCode | the root; skills and agents are copied to its discovery dirs | — |

Codex's and Antigravity's builds grow from skills and MCP to the whole
package, through the mirror Claude's build already uses (`shared::tree`).

*Rejected: building into a throwaway cache and deleting it after install.*
It was measured to break Claude and Codex.

*Rejected: a Git marketplace.* Both refuse `file://`.

### D3. Publication is per package and digest-gated

The package digest is cached in `packages.json` at ingest, with a `serde`
default and no rung. A build carries a `.uze-build` marker holding that
digest. Publication then works per package:

- It stages and swaps (`replace_dir`) only the builds whose digest
  differs, and swaps the catalogue last.
- An unchanged package costs one marker read. Anything a hook wrote into its
  build stays until the package changes.

The build is owned by an `IntegrationOwned { kind: "build" }` receipt. It is
an existing variant, so `attachments.json` keeps its shape. Its inspection
is: the directory exists and its marker matches.

### D4. Setup's machine artifacts get receipts

Some setup artifacts already have receipts and keep them:

- the rules entries in `settings.json` and `hooks.json` (`SETUP_OWNER`);
- the wrapper, owned by `HookConfigEntry.wrapper`, which now names
  `store/<vendor>/.uze/hooks/exec`.

These gain a receipt:

- the rules engine file;
- OpenCode's `uze.ts` and its hook documents;
- the bundled `uze` plugin's deliveries.

`remove` keeps refusing the bundled plugin, and `uninstall --purge` is its
only remover. The setup sweep (`setup.rs`) and the workspace's deletion of
session documents (`uze-workspace/conversation.rs`) ask the integration for
their directory instead of naming it.

The bundled skills that call `uze` open with a line saying they do not apply
when `uze` is not on `PATH`. The `plugin-freshness-and-linked-marketplaces`
doctor sweep needs no change, because the references still point inside
`$UZE_HOME`.

### D5. Migration

1. A ladder step moves `store/` to `sources/` once, under the mutation lock.
   A build that sees `sources/` and no `store/<vendor>` proceeds.
2. On the next `setup`, `install` or `update`, for each harness:
   1. Publish the builds into `store/<vendor>`.
   2. Repoint the marketplace. Claude: a same-name `add`. Codex: `remove`,
      then `add`; measured to keep plugins enabled.
   3. Refresh the vendor cache of each repointed plugin. The command per
      vendor is measured in task 1.2.
   4. Retire the old receipts by inspect-before-detach.
   5. Move the wrapper.
   6. Report the Codex hooks that need review again. Codex keys trust by
      the hook's command, so a moved wrapper reads as "Modified".
3. `runtime/attachments/*` and `runtime/packages` are deleted only when no
   receipt names them.

**Rollback:** an older build sees `sources/` and no `store/` of its own
shape, and reports it through persisted-state's upgrade-leftover rule. The
supported way back is `uze uninstall --purge` followed by a reinstall.

### D6. `uze uninstall`

**Before anything:**
- It skips bootstrap, as `upgrade` does, so nothing is re-seeded.
- It refuses inside a workspace pane, and while agents, clients, the
  terminal runtime with panes, or a background upgrade are alive.

**Plan:** `UzeApplication::uninstall_plan(purge)` is a read model:

- per harness, its receipts and their inspection, and the vendor
  registrations;
- the fate of each top-level `$UZE_HOME` entry;
- records and their size;
- per project, checkouts and `refs/uze/shelf/*` holding unlanded work, and
  links into `$UZE_HOME`;
- the provisioned harnesses;
- the binary's provenance;
- missing vendor CLIs.

**Execute:** under the mutation lock, after copying the binary's provenance,
with every step idempotent and the plan recomputed on each run:

| step | default | `--purge` |
|---|---|---|
| 1. per harness: readers first (config entries, `uze.ts`, plugin and marketplace registrations), then `store/<vendor>` | — | yes |
| 2. workspace links into `$UZE_HOME` | yes | yes |
| 3. binary (Windows: undo the recorded `Path` edit, then remove on exit) | yes | yes |
| 4. `sources/`, `runtime/`, `cache/`, `shims/` | yes | yes |
| 5. `store/`, `state/`, the rest of `$UZE_HOME` | kept | removed, last |

Keeping `state/` in the default mode is what lets a reinstalled UZE adopt
`store/` by its receipts, with `sources/` re-acquired from the receipts'
marketplaces. When a vendor CLI is missing, its registrations are listed as
left and never edited by hand. The command is classified `JustifiedSlow`.

### D7. Proof

- **Unit:** `vendor_neutral.rs::no_harness_artifact_names_outside_its_store`
  installs the canonical fixtures for the four fake harnesses. It walks the
  whole fake home except `$UZE_HOME`, and every `$UZE_HOME` path found must
  sit under the matching `store/<vendor>`.
- **Acceptance:** `tests/acceptance/uninstall.rs`, one test per
  machine-uninstall scenario.
- **Conformance, per vertical:**
  - with `sources/`, `runtime/` and `cache/` moved away, plugins list and
    load, a deny hook denies, and an `always_on` rule reaches the session;
  - after the default `uze uninstall`, the same holds;
  - after `uze uninstall --yes --purge`, a grep of `$HOME` finds nothing.

## Risks / Trade-offs

- **[`rm -rf ~/.uze` by hand]** → Claude plugins fail to load, Codex's
  plugin list fails, and deny hooks block. This is accepted and documented as
  unsupported. `uze uninstall` is the exit and `doctor-repair` the recovery.
- **[Builds repeat the same supporting files across harnesses]** → they sit
  on one filesystem, under one root. `std::fs::copy` clones on CoW
  filesystems. Hardlinking identical files between builds (never with
  `sources/`) is a measured optimization left for later.
- **[Codex and Antigravity caches grow to the whole package]** → this is the
  same trade Claude's build already made. `node_modules`-sized trees are
  excluded at ingest.
- **[Uninstall while a harness session is open]** → with `--purge`, Claude
  loses the plugin mid-session. The plan names the running harness processes.

## Candidate ADRs

- **The store tier: what a harness reads is a declared tier under
  `$UZE_HOME`, one directory per vendor, and the canonical bytes are
  `sources/`.** This amends ADR-052 and ADR-040's wrapper location, and binds
  every future integration.
