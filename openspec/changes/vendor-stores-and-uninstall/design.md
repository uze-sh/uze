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
- Revision 4 folds in two independent reviews of revision 3, and revision 5 flattens the store with the operator after three more measurements, one on
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
  - `codex plugin remove <id>` deletes `[plugins."<id>"]` and the cache.
    `marketplace remove` then deletes `[marketplaces.<name>]`, which leaves
    `config.toml` clean.
  - `file://` is refused.
- **Antigravity 1.4.12**
  - `agy plugin install` copies into `config/plugins/<name>`; installing
    again under the same name replaces that copy ("replaced existing
    installation", `experiments/antigravity/source-tie.py`).

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

### D1. Tiers and layout: a vendor store is one marketplace of builds

```
~/.uze/
  sources/<id>/                  BYTES      canonical package bytes (today's store/plugins, moved)
  sources/superseded/<id>/       BYTES      a failed restore's only copy (today's store/superseded)
  store/<vendor>/                STORE      the one marketplace UZE registers with that harness
    <marketplace manifest>       .claude-plugin/marketplace.json, .agents/plugins/marketplace.json
    <id>/                        the build of <id> for <vendor>: plugin, ${PLUGIN_ROOT}, hooks/uze-exec
    uze/                         the bundled plugin's build, which also carries the rules engine
  state/                         RECORD     receipts, registry, tasks, memory
  config.toml keys.json themes/  AUTHORED   the operator's own text
  runtime/  cache/  shims/       GENERATED / REMEMBERED
```

**The store holds only builds**, and a build is everything one package
needs in one harness. Nothing structural sits beside them.

- **One marketplace per vendor, named `uze-store`.** Today Claude and Codex
  carry two: `uze-local` for author envelopes and `uze-store` for generated
  ones. The separation guarded against a generated envelope overriding an
  author's, but ADR-013's precedence already decides that **per package
  and per harness**. A package that ships an author envelope for that
  harness is built from it; otherwise UZE generates one. The fixture
  `route-explicit` is explicit on Codex and generated on Claude. The origin
  is therefore a field of the build's receipt (`origin: explicit |
  generated`), not a directory. A plugin-name collision between two
  packages is resolved when the name is chosen (ADR-029), exactly as two
  generated plugins are today.

  *Measured 2026-11-01:* one Claude marketplace and one Codex marketplace
  each held an author-enveloped plugin beside a generated one, and both
  listed, installed and loaded.
- **The store tier**: deleting it uninstalls that harness's plugins.
  Nothing disposable lives in it (see D4). The persisted-state requirement
  keeps every existing tier, authored included, and its rule that content
  generated for another program never sits beside the records.
- `Store`, the Rust type, keeps its name and its invariant (it never writes a
  harness artifact), and now writes `sources/`. Each vendor store is
  written by its integration through `UzeHome::store_dir(vendor)`, in the
  same way `generated_attachments_dir(vendor)` is today.

*Rejected:*
- *Two roots per vendor (`local/`, `generated/`).* Revision 4 kept them to
  avoid the rename. The rename was then measured to be clean (D5), and the
  directories encoded something that is per-harness state.
- *A nested `.uze/` for machine material.* Every item in it has a more
  natural home (D4).

### D2. A build is the plugin and the package root

Each build is rendered from `sources/<id>` for one harness. It carries the
whole package:

- the manifest, either the author's or a generated one;
- `SKILL.md` with that harness's frontmatter;
- agents and MCP in that harness's format;
- hooks compiled to its dialect;
- **the hook wrapper** at the reserved path `hooks/uze-exec` (and `.ps1`);
- every supporting file.

`${PLUGIN_ROOT}` resolves to the build's own directory on every route of
that harness, including `settings.json` hooks, `claude mcp add` and loose
skills and agents.

| vendor | the build is | the vendor's own copy |
|---|---|---|
| Claude | its marketplace entry, read in place | `plugins/cache`, unused |
| Codex | its marketplace entry; must exist | `plugins/cache`, where it runs |
| Antigravity | the install source and the root | `config/plugins/<name>` |
| OpenCode | the root; skills and agents are copied to its discovery dirs, and the build holds the package's hook-group document | — |

The Codex and Antigravity builds grow to the whole package through
`shared::tree`, as Claude's does. Packages are machine-wide, with one
registry entry each, so each package has one build per vendor. Its receipt
is detached with the package's other receipts when the package is removed
(`take_off`). Nothing needs reference counting.

*Rejected: a throwaway build cache* (measured to break Claude and Codex).
*Rejected: a Git marketplace* (`file://` is refused by both).

### D3. Builds are keyed, gated and inspected by content

The package digest is cached in `packages.json` at ingest. Each build is
owned by one receipt, `IntegrationOwned { kind: "build" }`, whose `detail`
holds:

- the **build key** `(package digest, UZE build version, dialect version,
  build root, active name)`;
- `origin: explicit | generated`;
- `source: pinned | linked` (D3b);
- the digest of every file UZE wrote into the build.

The build directory therefore holds the plugin, plus the hook wrapper at the
reserved path `hooks/uze-exec` (and `hooks/uze-exec.ps1`) when the package has
hooks. Ingest refuses a package that ships a file at a reserved path, so an
author's own `hooks/` directory never collides with it. No bookkeeping file
of UZE's appears in a vendor's cache or in its `plugin validate`. The
receipt is already the source of truth for what UZE wrote. If the records
are lost, the builds become unknown to UZE, which is what persisted-state
says losing a record costs.

The build receipt carries a resource identity of its own (`build`), so it is
never mistaken for the receipt of the vendor install that `attach` checks
before installing a plugin. `attach.rs` filters receipts by kind.

*Rejected: a `.uze-build` file inside each build.* It duplicates the receipt
and leaks a UZE file into every vendor copy.

| step | rule |
|---|---|
| publish | stage and swap a build only when its key differs, then swap the catalogue last. A UZE upgrade changes the key, so a stale render never survives one. |
| inspect | `Matched` when every file the receipt lists still has its digest; `Drifted` when one was edited; `Missing` when the directory is gone. A file a hook *added* is not listed, so it is not drift. |
| receipt | one per (vendor, package), identity `build` |

**Catalogue membership comes from the registry and the build receipts,
never from the bytes.** A registry entry whose `sources/` directory is
missing becomes **bytes missing**:

- it stays in the catalogue with the build it has, and its origin is read
  from that build's receipt;
- it is never pruned as a ghost and its receipts are never orphaned;
- it is not rebuilt until the bytes return.

`prune_ghost_registrations` applies only to an entry that has neither bytes
nor a build.

### D3b. A linked source: same identity, another source

`uze market link` (from `plugin-freshness-and-linked-marketplaces`) points a
marketplace at a checkout the operator develops. That checkout lives
**outside `$UZE_HOME`**, in the operator's own directory.

- **What that change already does, and keeps doing.** It ingests the
  checkout into `sources/<id>` whenever the checkout's tree changes, and
  ingest enforces the containment rules: Git-ignored files are dropped, and
  a link that escapes the package is refused. That change rejected reading
  the checkout in place, so that delivery keeps one path, and this design
  respects that.
- **What this change adds:**
  - **Same identity.** The build stays `store/<vendor>/<id>`, with the
    same selector, enablement and receipts. Its receipt records
    `source: linked`. An edit changes the digest, so the build key changes
    and the build is rendered again in place.
  - **Unlinking restores the original build.** The link record also
    keeps the commit the package was pinned to before linking, because
    linked ingest records `Local { checkout }` as its provenance. After
    `uze market unlink`, the next resolution re-ingests that commit from the
    marketplace's cached partial clone. That resolution is whatever runs
    next: install, update or maintenance. This matches
    `plugin-freshness-and-linked-marketplaces`: unlink restores resolution
    from the registered source and leaves the bytes alone until something
    asks for them. The build is then rendered again as `source: pinned`.
    This works offline when the clone holds that commit's blobs;
    otherwise it is `NeedsNetwork` for `doctor-repair`.
  - **The pin never moves from linked work.** `agents.lock` is not written
    from a linked source, as that change already guarantees.

*Rejected: keeping a separate copy of the linked tree inside `$UZE_HOME`.*
It is a second snapshot beside `sources/`, and the build is already one.

### D4. Machine material finds its natural home

`store/<vendor>/.uze/` is gone. Each item moves where it belongs:

| item | before | after |
|---|---|---|
| hook wrapper | one shared copy per harness | inside every build that has hooks, at the reserved `hooks/uze-exec`. The entry names its own build's wrapper, and the wrapper leaves with the package. This was ADR-040's original intent. `HookConfigEntry.wrapper` then names a file the build receipt already owns, and its "removed once no entry needs it" logic goes. |
| rules engine | a shared file per harness | a real file of the bundled `uze` package (`plugins/uze`), rendered into its build like any package file. The `SETUP_OWNER` entries name it there. No package is special-cased: `plugins/uze` stays "not privileged" (`bootstrap.rs`), and it simply ships the engine. |
| rule-engine sessions | `runtime/rules/sessions` | **unchanged**: a scratch directory under `runtime/` that its reader recreates on demand (`rules/engine.rs`). Deleting it changes nothing a harness does, so it is the declared exception to "a harness reads only its store". |
| OpenCode hook groups | `runtime/attachments/opencode/hook-groups` | inside each package's build. `uze.ts` reads `store/opencode/*/`. |
| OpenCode session documents | `runtime/sessions/opencode`, baked into `uze.ts` | **unchanged**: the same declared on-demand scratch exception. The launch environment cannot carry it, because one shared OpenCode server serves every launch and keeps the environment of whichever launch started it (`opencode/agent_context.rs`). |

`uze.ts` and the bundled `uze` plugin's deliveries gain receipts. `remove`
keeps refusing the bundled plugin, and only `uninstall --purge` detaches
it. `~/.agents/skills` belongs to the Codex integration. OpenCode reads it
too, and the copies there resolve into `store/codex`.

Codex hook groups are always written **after** the operator's own groups in
each event array. Adding or removing them then never shifts the trust keys of
groups the operator placed first. Two shifts remain, both already true
today:

- removing one UZE package's groups shifts the UZE groups after them;
- at purge, removing UZE's groups shifts operator groups added after them.

Both are reported. Through `codex/trust.rs`, the report names every hook
whose review Codex will ask for again. Skills that call `uze` say they do
not apply when it is absent.

### D5. Migration, fenced against older builds

1. **Fence first.** Under the mutation lock, `packages.json` and
   `attachments.json` are both written at their new shape
   (`RawPackageRegistry::SHAPE` and the ledger shape are bumped, and a ladder
   rung carries every entry across). Only then does anything move. An older
   build then refuses both documents by name before it can prune or detach
   anything (persisted-state: a newer record is never taken). The opposite
   order would let an older build read a shape-1 registry with no bytes,
   prune every entry as a ghost and detach its receipts.
2. **Move and backfill.** Once per package, still under the lock:
   whenever any `store/plugins/<id>` exists, it is renamed into
   `sources/<id>`, and the same is done for `superseded/`. Its digest is
   computed from the moved bytes and written into the registry. A ladder rung
   is a pure function and cannot read disk, so the digest is backfilled
   here. The step is resumable, because each package moves on its own.
   An older build's `ensure_layout` may still leave an empty `store/plugins`
   before it refuses. The new build removes it when it is empty.
3. **Re-deliver.** On the next `setup`, `install` or `update`, for each
   harness:
   1. Publish the builds into `store/<vendor>`.
   2. Retire `uze-local` with three explicit steps:
      - **`uze-store` receipts are matched by marketplace name.**
        `receipt_serves` keys on the name, not the root, so the root
        change is handled by the repoint (step 3) and never by an
        uninstall and reinstall.
      - **`<p>@uze-local` is retired.** Each matched receipt is detached
        through the vendor CLI and `<p>@uze-store` is attached. Enablement
        is not carried, because UZE is in beta and keeps no legacy paths. A
        plugin the operator had disabled comes back enabled once, and the
        changelog says so. On Codex, a disabled plugin inspects as
        `Drifted`: it is **left and reported**, never installed beside
        itself.
      - **The `uze-local` marketplace is removed** once no receipt names it.
        `detach_package` never removes a marketplace by itself.

      Each vendor keeps an empty `cache/uze-local` of its own, which is
      vendor bookkeeping UZE leaves alone.
   3. Repoint each marketplace only when its registered root differs,
      which is once per machine (the move off `runtime/`). No session of
      the harness is waited for: nothing in UZE sees a harness process it
      did not start, and the repoint is safe without it. The risk is
      accepted (D13):
      - **Claude:** a same-name `marketplace add`, which keeps its plugins
        installed (measured).
      - **Codex:** `remove`, then `add`. Its plugins stay enabled across
        it (measured); a session running through that brief window sees
        them missing until the `add` lands. If the `add` fails, re-add the
        old root (which still exists) and report.
   4. Refresh the vendor caches (measured):
      - **Claude:** nothing, because it reads in place.
      - **Codex:** `codex plugin add <id>` over the installed plugin.
      - **Antigravity:** `agy plugin install` again, which replaces its copy
        (measured).
   5. Retire the old receipts by inspect-before-detach.
   6. The wrappers now sit in the builds.
4. **Codex trust.** The wrapper path is in the hashed command, so every UZE
   hook reads as "Modified" and Codex holds it back. That includes deny
   guards, which then **stop enforcing until reviewed**. Install, `doctor`
   and the TUI carry a blocking notice naming each hook and the review to
   do, until `codex/trust.rs` reads them as trusted. The wrapper move is a
   one-time cost, because `store/<vendor>` paths do not move again. Index
   shifts on later removals are the pre-existing cost described in D4.
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
- **[An older workspace left open across a background upgrade]** → after
  the move it refuses the fenced documents and shows errors until restarted.
  It writes nothing and loses nothing, and its agents keep running, because
  the terminal runtime's state is untouched. Its restart notice already
  exists. Accepted for the beta.
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

## Decision log

Choices taken without the operator, and those agreed in conversation, all for the operator to confirm.

| # | chosen | rejected, and why |
|---|---|---|
| 1 | Default uninstall **keeps `sources/`** (and authored files); only the program goes | Deleting `sources/` and re-acquiring on reinstall needs the network, makes this change depend on `doctor-repair`, and cannot re-acquire local, linked or embedded packages. |
| 2 | One marketplace per vendor (`uze-store`), with `origin` as build state (revision 5, with the operator) | Two roots (`local/`, `generated/`) encoded a per-harness fact as a directory. The rename it avoided was measured to be clean, carrying the disabled state on Claude. |
| 3 | Fence older builds by bumping both document shapes | "Report as a leftover" is not what the code does. The older build would silently write into the new layout. |
| 4 | Build key includes UZE version, dialect, root and alias | Digest-only keys leave stale renders after a UZE upgrade. |
| 5 | Build inspection by per-file digest; hook-added files ignored | Existence plus marker never yields `Drifted`, so `--purge` would delete an edit. |
| 6 | Membership from the registry and builds; "bytes missing" is a state | Membership from bytes empties every catalogue when `sources/` is missing. |
| 7 | Repoint only when the root differs, once per machine, with no session guard; Codex re-adds the old root on failure | Unconditional repoint briefly uninstalls Codex plugins mid-session. A session guard needs a census of harness processes UZE does not have; Claude's same-name add keeps plugins and Codex's window is brief (D13). |
| 8 | UZE's Codex hook groups always after the operator's | Removing them earlier in the array shifts the operator's trust keys. |
| 9 | Installers delegate to `uze uninstall` | `install.ps1 -Purge` deleting `~/.uze` is exactly the unsupported path. |
| 10 | Interactive uninstall asks keep-or-purge | A default with no binary left makes `--purge` unreachable without a reinstall. |
| 11 | The ownership test walks configuration and discovery files, not transcripts | Transcripts legitimately quote expanded paths and are the vendor's history. |
| 12 | Write both fenced documents, then move the bytes, then backfill digests during the move | Moving first lets an older build prune and detach. A ladder rung cannot compute digests from disk. |
| 13 | No reference counting; one build per package per vendor | Packages are machine-wide with one registry entry. A scope model does not exist and is not needed. |
| 19 | No nested `.uze/` in the store. The wrapper sits in each build at a reserved path, the rules engine is a real file of `plugins/uze`, and session scratch stays in `runtime/` as a declared on-demand exception. | A nested `.uze/` mixed scratch into the store tier, and a shared wrapper needed "last entry" bookkeeping. `$TMPDIR` sessions break `UzeHome`'s map and `$UID` is unset in `sh`. The launch environment cannot reach OpenCode's shared server. |
| 20 | `uze-local` is retired explicitly: match `uze-store` by name, detach each `@uze-local` receipt, then remove the marketplace. Enablement is not carried, and a disabled Codex plugin is left and reported. | The generic earlier-shape rule removes no marketplace, would uninstall every `uze-store` plugin on the root change, and would install a disabled Codex plugin twice. |
| 21 | The build's key, origin, source and file digests live in its receipt; the build holds only the plugin | A `.uze-build` file duplicates the receipt and leaks into every vendor copy. |
| 22 | A linked source keeps the build's identity; unlinking re-ingests the pinned commit from the cached clone | Reading the checkout in place forks delivery, which `plugin-freshness` rejected for containment. A separate linked snapshot in `$UZE_HOME` is redundant with the build. |
| 23 | No guard for an older UZE still running after a background upgrade. It refuses the fenced documents by name, shows errors and writes nothing, and its existing restart notice resolves it. | Making the new build wait and keep working on the old layout until every older process restarts needs code that reads both layouts, a legacy path the beta does not carry. |

## Candidate ADRs

- **The store tier.** What a harness reads is a declared tier under
  `$UZE_HOME`, one directory per vendor, and canonical bytes are `sources/`.
  This amends ADR-052, ADR-040 (wrapper location) and ADR-013 §5 (a derived
  artifact is rebuilt when its build key changes, not on every
  materialization). It binds every future integration.
