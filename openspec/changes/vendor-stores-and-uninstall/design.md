## Context

See proposal.md for the motivation. State on `b09b1a92` (uze 1.0.0-beta.22).

**How this design got here.**
- Revisions 1–2 put a directory inside each vendor's root.
- Revision 3, decided with the operator, moved what harnesses read to
  `~/.uze/store/<vendor>/`. The reasons:
  - one root that UZE controls;
  - the vendors' territory left untouched;
  - the invariant "every path UZE owns is named in `UzeHome`" holding without
    a new port method;
  - the same layout on all three OSes.

  The cost was accepted on purpose: removing `~/.uze` by hand is
  unsupported.
- Revision 4 folds in two independent reviews of revision 3, one on
  architecture and one on harness feasibility and product. The "Decision
  log" at the end lists every choice made without the operator, with the
  alternatives that were rejected.

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

The code also writes ties outside the harness homes:
- `~/.claude.json`, for user-scope MCP;
- `~/.agents/skills`, written by Codex and also read by OpenCode;
- `~/.gemini/config/{hooks,mcp_config}.json`;
- the workspace links `.worktrees/<id>~uze`.

**What each vendor does with a marketplace source** (sandbox commands,
2026-11-01):

- **Claude 2.1.316**
  - Loads a local marketplace *in place*: *"it loads in place from …"*. The
    copy in `plugins/cache` is not what it loads.
  - Source gone: `✘ failed to load — cache-miss`.
  - A same-name `marketplace add` repoints the marketplace and keeps its
    plugins. `marketplace remove` uninstalls them.
  - `file://` is refused.
- **Codex 0.166.0**
  - Runs from `plugins/cache/<m>/<p>/<v>`, but needs the local marketplace
    root: without it, `plugin list` fails as a whole.
  - A same-name add from another root is refused; `remove` must come first.
    After `remove`, `config.toml` keeps `[plugins."<id>"] enabled = true`,
    and an `add` from the new root lists the plugin as enabled again.
  - `codex plugin add <id>` over an installed plugin rewrites its cache in
    place. A version bump replaces the cache directory.
  - `file://` is refused.
- **Antigravity 1.4.12**
  - `agy plugin install` copies into `config/plugins/<name>` and refuses a
    same-name re-import.

**Code facts the reviews established:**
- Claude and Codex each have **two** UZE marketplaces: `uze-local` (author
  envelopes) and `uze-store` (generated), each with its own root
  (`shared/marketplace.rs` `Origin`).
- Catalogue membership is computed from the package bytes
  (`marketplace.rs` `members()`).
- `Store::prune_ghost_registrations` drops a registry entry whose bytes
  directory is gone, and maintenance then detaches that entry's receipts as
  orphans.
- `ensure_layout` recreates `store/plugins` on every lock and ingest.
- `MutationLock` is a kernel `flock`, which cannot go stale.
- `install.ps1 -Uninstall -Purge` deletes `~/.uze` with no detach.
- Codex keys hook trust by `file:event:group:handler` and by the hashed
  command (`codex/trust.rs`).

## Goals / Non-Goals

**Goals:**
- No harness configuration or discovery file names a `$UZE_HOME` path outside
  that harness's vendor store. A test proves it over the whole fake home, and
  conformance proves it on the real binaries.
- One build per package per harness, which is also the package root.
  `runtime/packages` goes away for harnesses.
- One command removes UZE, either keeping what it delivered working or
  leaving no trace.

**Non-Goals:**
- Changing the delivery precedence (ADR-013) or the vendor commands UZE runs.
- Keeping harnesses intact when `~/.uze` is deleted by hand. That is
  unsupported; `doctor-repair` restores what it can.
- Harness transcripts and session logs, which may quote an expanded
  `${PLUGIN_ROOT}`. They are the vendor's history, not configuration.
- Re-acquiring deleted package bytes. That belongs to `doctor-repair`, which
  depends on this change. This change only guarantees that missing bytes
  never cost a delivery.

## Decisions

### D1. Tiers and layout

```
~/.uze/
  sources/<id>/                  BYTES      canonical package bytes (today's store/plugins, moved)
  sources/superseded/<id>/       BYTES      a failed restore's only copy (today's store/superseded)
  store/<vendor>/                STORE      what that harness reads
    local/<id>/                  build of an author-enveloped package  (marketplace uze-local)
    generated/<id>/              build of a generated package          (marketplace uze-store)
    .uze/                        hooks/exec(.ps1), rules engine + sessions,
                                 OpenCode hook and session documents
  state/                         RECORD     receipts, registry, tasks, memory
  config.toml keys.json themes/  AUTHORED   the operator's own text
  runtime/  cache/  shims/       GENERATED / REMEMBERED
```

- **Two roots per vendor**, because Claude and Codex keep two marketplaces
  and a root holds one catalogue. Antigravity and OpenCode use only
  `generated/` and `.uze/`. *Rejected: merging the two marketplaces.* It
  renames `<p>@uze-local` and loses Claude's `enabledPlugins` and Codex's
  `[plugins."<id>"]`.
- **The store tier**: deleting it uninstalls that harness's plugins. The
  persisted-state requirement keeps every existing tier (authored included)
  and its rule that content generated for another program never sits beside
  the records.
- `Store`, the Rust type, keeps its name and its invariant (it never writes a
  harness artifact), and now writes `sources/`. Each vendor store is
  written by its integration through `UzeHome::store_dir(vendor)`, in the
  same way `generated_attachments_dir(vendor)` is today.

### D2. A build is the plugin and the package root

Each build is rendered from `sources/<id>` for one harness. It carries the
whole package: manifest, `SKILL.md` with that harness's frontmatter, agents
and MCP in its format, hooks compiled to its dialect, and every supporting
file.

`${PLUGIN_ROOT}` resolves to the build's own directory on every route of
that harness, including `settings.json` hooks, `claude mcp add` and loose
skills and agents.

| vendor | the build is | the vendor's own copy |
|---|---|---|
| Claude | its marketplace entry, read in place | `plugins/cache`, unused |
| Codex | its marketplace entry; must exist | `plugins/cache`, where it runs |
| Antigravity | the install source and the root | `config/plugins/<name>` |
| OpenCode | the root; skills and agents are copied to its discovery dirs | — |

The Codex and Antigravity builds grow to the whole package through
`shared::tree`, as Claude's does. Each package id has one build per vendor,
shared by every scope that installs it. Its receipt is detached when the
last scope referencing the package removes it, and the existing removal
planning already counts those references.

*Rejected: a throwaway build cache* (measured to break Claude and Codex).
*Rejected: a Git marketplace* (`file://` is refused by both).

### D3. Builds are keyed, gated and inspected by content

The package digest is cached in `packages.json` at ingest. Each build
writes a `.uze-build` manifest holding:

- the **build key** `(package digest, UZE build version, dialect version,
  build root, active name)`;
- the digest of every file UZE wrote into the build.

| step | rule |
|---|---|
| publish | stage and swap a build only when its key differs, then swap the catalogue last. A UZE upgrade changes the key, so a stale render never survives one. |
| inspect | `Matched` when every listed file still has its digest; `Drifted` when one was edited; `Missing` when the directory or manifest is gone. A file a hook *added* is not listed, so it is not drift. |
| receipt | `IntegrationOwned { kind: "build" }`, one per (vendor, origin, package) |

**Catalogue membership comes from the registry and the build key, never from
the bytes.** A registry entry whose `sources/` directory is missing becomes
**bytes missing**:

- it stays in its catalogue with the build it has;
- it is never pruned as a ghost and its receipts are never orphaned;
- it is not rebuilt until the bytes return.

`prune_ghost_registrations` applies only to an entry that has neither bytes
nor a build. This is what keeps "deleting the bytes leaves every delivery
working" true.

### D4. Setup's machine artifacts: what changes

These already carry receipts and keep them:
- the rules entries (`SETUP_OWNER`), which name the engine path;
- the wrapper, owned by `HookConfigEntry.wrapper`.

Both paths move to `store/<vendor>/.uze/`.

`uze.ts`, OpenCode's documents and the bundled `uze` plugin's deliveries gain
receipts. `remove` keeps refusing the bundled plugin, and only
`uninstall --purge` detaches it.

Lookups of directories that moved:
- The setup sweep and the workspace's deletion of session documents ask the
  integration for the directory.
- `~/.agents/skills` belongs to the Codex integration. OpenCode reads it
  too, and the copies there resolve into `store/codex`.

Codex hook groups are always written **after** the operator's own groups in
each event array. Adding or removing them then never shifts the indices that
key the operator's hook trust. Skills that call `uze` say they do not apply
when it is absent.

### D5. Migration, fenced against older builds

1. **Fence.** The move bumps the shape of both `packages.json`
   (`RawPackageRegistry::SHAPE`) and `attachments.json`, with a ladder rung
   that carries every entry across. An older build then refuses both
   documents by name (persisted-state: a newer record is never taken).
   Without the fence, an older build's `ensure_layout` would recreate
   `store/plugins` and write bytes the newer build never reads, and its
   setup would move the receipts back to `runtime/`.
2. **Move.** Once per package, under the mutation lock: whenever any
   `store/plugins/<id>` exists it is renamed into `sources/<id>`, and the
   same is done for `superseded/`. It is resumable, because each package
   moves on its own.
3. **Re-deliver.** On the next `setup`, `install` or `update`, for each
   harness:
   1. Publish the builds into `store/<vendor>/{local,generated}`.
   2. Repoint each marketplace only when its registered root differs, and
      never while a session of that harness is running:
      - **Claude:** a same-name `marketplace add`.
      - **Codex:** `remove`, then `add`. If the `add` fails, re-add the
        old root (which still exists) and report.
   3. Refresh the vendor caches (measured):
      - **Claude:** nothing, because it reads in place.
      - **Codex:** `codex plugin add <id>` over the installed plugin.
      - **Antigravity:** the existing uninstall-then-install re-delivery.
   4. Retire the old receipts by inspect-before-detach.
   5. Move the wrapper.
4. **Codex trust.** The wrapper path is in the hashed command, so every UZE
   hook reads as "Modified" and Codex holds it back. That includes deny
   guards, which then **stop enforcing until reviewed**. Install, `doctor`
   and the TUI carry a blocking notice naming each hook and the review to
   do, until `codex/trust.rs` reads them as trusted. This is a one-time cost:
   `store/<vendor>` paths do not move again.
5. **Clean-up.** `runtime/attachments/*` and `runtime/packages` are deleted
   once no receipt names them.

The supported way back to an older build is `uze uninstall --purge`
followed by a reinstall.

### D6. `uze uninstall`

**Before anything.** It skips bootstrap, as `upgrade` does. It refuses
inside a workspace pane, and while agents, clients, the terminal runtime with
panes, or a background upgrade are alive.

**Plan.** `UzeApplication::uninstall_plan(purge)` is a read model:
- per harness, the receipts and their inspection, and the registrations;
- the fate of each top-level entry of `$UZE_HOME`;
- records and their size;
- per project, checkouts and `refs/uze/shelf/*` holding unlanded work, and
  links into `$UZE_HOME`;
- the provisioned harnesses;
- the binary's provenance;
- missing vendor CLIs.

When run interactively without either flag, the plan **asks which mode**.
The default mode's report says how to purge later: reinstall, then
`uze uninstall --purge`.

**Execute.** Under the mutation lock, after copying the binary's provenance,
every step idempotent, the plan recomputed on each run:

| step | default (remove the program) | `--purge` (remove every trace) |
|---|---|---|
| 1. per harness, readers first: plugins (`codex plugin remove`, `claude plugin uninstall`, `agy plugin uninstall`, which also drops Codex's `[plugins]` keys), then marketplaces, config entries (UZE's Codex groups last) and `uze.ts`, then `store/<vendor>` | — | yes |
| 2. workspace links into `$UZE_HOME` | yes | yes |
| 3. binary (Windows: undo the recorded `Path` edit, remove on exit) | yes | yes |
| 4. `runtime/`, `cache/`, `shims/` | yes | yes |
| 5. `sources/`, `store/`, `state/`, authored files, then `$UZE_HOME` itself | **kept** | removed, last |

The default mode keeps everything a reinstall needs to adopt, so a
reinstalled UZE starts exactly where it stopped, with no network and no
re-delivery. With a missing vendor CLI, its registrations are listed as
left and never edited by hand. The command is classified `JustifiedSlow`.

`install.sh` and `install.ps1` stop deleting `~/.uze` themselves.
`-Uninstall [-Purge]` runs `uze uninstall --yes [--purge]` when the binary
is present. Otherwise it removes only what the installer placed, and prints
the reinstall-then-purge path.

### D7. Proof

- **Unit:** `vendor_neutral.rs::no_harness_artifact_names_outside_its_store`
  installs the canonical fixtures for the four fake harnesses. It walks every
  configuration and discovery file under the fake home (not transcripts) and
  requires each `$UZE_HOME` path to lie in the matching
  `store/<vendor>`.
- **Unit:** publication rebuilds only a changed key, a hook-added file
  survives an unchanged package, and an edited file is `Drifted`.
- **Unit:** missing bytes are neither pruned nor orphaned. The existing test
  `the_delivered_root_is_a_copy_a_write_cannot_carry_into_the_store`, and
  its invariant, are rewritten for builds.
- **Acceptance:** `tests/acceptance/uninstall.rs` has one test per
  machine-uninstall scenario. `tests/acceptance/migration.rs` covers the
  move, an interrupted move, and an older build refusing the fenced
  documents.
- **Conformance, per vertical:**
  - with `runtime/` and `cache/` moved away, and again with `sources/` moved
    away, plugins list and load, a deny hook denies, and an `always_on` rule
    reaches the session;
  - the same after the default `uze uninstall --yes`;
  - after `--purge`, no configuration or discovery file names `$UZE_HOME`.

## Risks / Trade-offs

- **[`rm -rf ~/.uze` by hand]** → Claude plugins fail to load, Codex's
  `plugin list` fails, and deny hooks block. It is unsupported, and the
  docs, the uninstall report and the installers point to `uze uninstall`.
- **[Codex deny hooks pause after migration until reviewed]** → there is a
  blocking notice everywhere UZE speaks. UZE never answers Codex's review
  for the person (`codex/trust.rs`).
- **[Builds repeat supporting files across harnesses]** → they sit on one
  filesystem, and `std::fs::copy` clones on CoW filesystems. Hardlinking
  identical files between builds (never with `sources/`) is a measured
  optimization left for later.
- **[A hook that edits a package file in place]** → its build reads
  `Drifted`, is kept and is reported. The next package change does not
  overwrite it until the person resolves it. This is rare and visible.

## Decision log (revision 4, taken without the operator)

| # | chosen | rejected, and why |
|---|---|---|
| 1 | Default uninstall **keeps `sources/`** (and authored files); only the program goes | Deleting `sources/` and re-acquiring on reinstall needs the network, makes this change depend on `doctor-repair`, and cannot re-acquire local, linked or embedded packages. |
| 2 | Two roots per vendor (`local/`, `generated/`) | One merged marketplace renames `@uze-local` plugins and loses enablement state. |
| 3 | Fence older builds by bumping both document shapes | "Report as a leftover" is not what the code does. The older build would silently write into the new layout. |
| 4 | Build key includes UZE version, dialect, root and alias | Digest-only keys leave stale renders after a UZE upgrade. |
| 5 | Build inspection by per-file digest; hook-added files ignored | Existence plus marker never yields `Drifted`, so `--purge` would delete an edit. |
| 6 | Membership from the registry and builds; "bytes missing" is a state | Membership from bytes empties every catalogue when `sources/` is missing. |
| 7 | Repoint only when the root differs and no session runs; Codex re-adds the old root on failure | Unconditional repoint briefly uninstalls Codex plugins mid-session. |
| 8 | UZE's Codex hook groups always after the operator's | Removing them earlier in the array shifts the operator's trust keys. |
| 9 | Installers delegate to `uze uninstall` | `install.ps1 -Purge` deleting `~/.uze` is exactly the unsupported path. |
| 10 | Interactive uninstall asks keep-or-purge | A default with no binary left makes `--purge` unreachable without a reinstall. |
| 11 | The ownership test walks configuration and discovery files, not transcripts | Transcripts legitimately quote expanded paths and are the vendor's history. |

## Candidate ADRs

- **The store tier.** What a harness reads is a declared tier under
  `$UZE_HOME`, one directory per vendor, and canonical bytes are `sources/`.
  This amends ADR-052, ADR-040 (wrapper location) and ADR-013 §5 (a derived
  artifact is rebuilt when its build key changes, not on every
  materialization). It binds every future integration.
