## Context

See proposal.md for the motivation. This section covers the state on
`b09b1a92` (uze 1.0.0-beta.22) and the measurements this design rests on.
Revision 2 folds in two independent reviews (architecture; harness
feasibility and uninstall) and one more measurement.

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

The grep looked only at the harness homes. The code also writes ties to
places outside them:

- `~/.claude.json`, for user-scope MCP (`claude/mcp.rs`);
- `~/.agents/skills`, for Codex's loose skills, which OpenCode also reads;
- `~/.gemini/config/hooks.json` and `mcp_config.json`, Antigravity's
  per-capability fallback;
- the workspace's on-disk links `.worktrees/<id>~uze` and `.worktrees/~uze/<agent>`,
  which point into `runtime/projects`.

**What each vendor does with a marketplace source** (sandbox commands,
2026-11-01; Claude Code 2.1.316, Codex 0.166.0, Antigravity 1.4.12):

- **Claude.**
  - `installed_plugins.json` records an `installPath` under
    `~/.claude/plugins/cache/…/1.0.0`, and that copy is complete. Yet
    `claude plugin list` says `Read from: <the marketplace source>`.
  - With the source moved away, the plugin reports `✘ failed to load —
    cache-miss`.
  - `marketplace add` with an existing name repoints the marketplace and keeps
    its plugins. `marketplace remove` uninstalls them.
  - A marketplace placed in `~/.claude/uze/marketplace/…` stays `✔ enabled`
    with all of `$UZE_HOME` moved away.
- **Codex.**
  - It copies the plugin into `~/.codex/plugins/cache/<market>/<plugin>/<version>`.
  - If the local marketplace root is missing, `codex plugin list` fails as a
    whole.
  - `marketplace add` with an existing name from another root is refused:
    *"remove it before adding this source"*.
  - `marketplace remove` empties the installed list, but `config.toml` keeps
    `[plugins."<id>"] enabled = true`. Adding the marketplace back from a root
    under `~/.codex/` lists the plugin as installed and enabled again, and it
    stays so with `$UZE_HOME` moved away.
  - `marketplace add` accepts a local path, `owner/repo`, HTTPS or SSH Git.
    It does not accept `file://`.
- **Antigravity.** `agy plugin install` copies into
  `~/.gemini/config/plugins/<name>`. The only ties are text inside that copy.

So "compile into a throwaway cache, deliver through the official command,
discard" works for Antigravity only. For Claude and Codex the source is read
for as long as the plugin is installed, so what matters is *where* it lives.

## Goals / Non-Goals

**Goals:**
- No file a harness reads, and no configuration entry UZE writes, names
  `$UZE_HOME`. This is proven by a test over the whole fake home and by
  conformance on the real binaries.
- One command removes what UZE delivered, using the same receipts and
  inspection as `remove`.
- Delivery does no more I/O than today when nothing changed. Bytes on disk
  grow by one package-root copy per harness, a cost stated in Risks.

**Non-Goals:**
- Changing the delivery precedence (ADR-013) or the vendor commands UZE runs
  to install.
- Workspace launch arguments (`--add-dir`, `-c`, session documents). They
  live as long as one process. The workspace's on-disk links are not covered
  by this exception; they are handled in D7.
- Honouring `CODEX_HOME` (today `codex.rs` hardcodes `~/.codex`). Each UZE
  directory follows whatever root its integration already resolves.

## Decisions

### D1. One UZE directory per harness, named through the port

`IntegrationPort` gains `owned_directory(&UzeHome) -> PathBuf`. Its answers
are `<claude_home>/uze`, `~/.codex/uze`, `~/.config/opencode/uze` and
`~/.gemini/uze`. Core still names no harness, but it can now enumerate every
UZE-owned path. ADR-052's "every path UZE owns is named in `UzeHome`" becomes
"…in `UzeHome` or in an integration's `owned_directory`", and the layering
test that enforces it is extended to cover both.

```
<harness root>/uze/
  marketplace/<market>/   # marketplace roots UZE registers (Claude, Codex)
  packages/<id>/          # the delivered package root, one per package
  bin/                    # the hook wrapper: exec (POSIX) and exec.ps1 (Windows)
  rules/                  # the rules engine and its sessions/
  sessions/               # OpenCode session documents
  hooks/                  # OpenCode hook-group documents
```

*Rejected: a `delivered/` tier under `$UZE_HOME`.* It is honest about cost,
but `rm -rf ~/.uze` would still break every harness.

*Rejected: the vendor's own directories*, such as `plugins/marketplaces/` or
`config/plugins/`. They are the vendor's bookkeeping, which the vendor may
prune or rewrite. UZE already owns its loose skill directories this way.

### D2. Marketplace roots live in the UZE directory and are published per package

The marketplace root moves to `<harness>/uze/marketplace/<market>`.
Publication becomes incremental: only a package whose source digest changed
is staged and swapped (`replace_dir`, per package directory), and the
catalogue file is swapped last. Registration still goes through the vendor's
CLI.

- **Claude:** repoint with `marketplace add <same name> <new root>`.
- **Codex:** `marketplace remove` then `marketplace add`, because a
  same-name add is refused. The measured carry-over of `[plugins]` keeps
  plugins enabled.

The old root is deleted only after every marketplace receipt has been
repointed and inspected `MATCHED` at the new root.

*Rejected: a Git marketplace on `file://`.* Codex does not accept it.

*Rejected: a throwaway build cache.* Claude and Codex were measured reading
the source.

### D3. `${PLUGIN_ROOT}` has one answer: `<harness>/uze/packages/<id>`

There is still one resolver (`shared::package_root`) and one meaning, the
whole package. Every route and every file kind resolves to the package
directory in that harness's UZE directory: plugin skills, `settings.json`
hooks, `claude mcp add`, loose skills and agents.

*Rejected: `${CLAUDE_PLUGIN_ROOT}`.* Claude expands it only inside a plugin.
UZE merges Claude hooks into `settings.json`, and MCP and loose skills
bypass the plugin. Expansion in agent files was never measured. One answer
everywhere is simpler than a matrix of placeholders.

*Rejected: the vendor's copy as the root*, as was proposed for Antigravity.
UZE cannot restore a vendor copy, and it would be writing into the vendor's
bookkeeping.

*Rejected: growing the Codex and Antigravity envelopes to the whole
package.* This is no longer needed, because the root is outside the
envelope.

The extension MCP launcher in `uze-application` (`extensions/mcp.rs`) is
UZE's own process, not a harness. It keeps a UZE-only copy under `runtime/`,
which is legitimate because no harness reads it.

### D4. A package root is replaced when its package changes, and checked by a marker

The receipt is `IntegrationOwned { kind: "package-root", detail: {path,
source_digest} }`. This is an existing variant, so `attachments.json` keeps
its shape and older builds still read it. The shared helper in
`uze-integrations/shared` implements it.

- **Inspection.** The root exists, and its `.uze-root` marker file holds the
  receipt's digest. Both are O(1). A missing root or marker is `MISSING` and
  gets re-delivered.
- **Delivery.** When the Store package's digest differs from the receipt's,
  the root is rebuilt whole. Otherwise it is left as it is, including
  anything the package's own hooks wrote into it. That is the rule the
  fidelity spec now states.
- **The Store digest** is cached in `packages.json` at ingest, so delivery
  never walks the Store tree to decide.
- **Detach** removes the directory whole. Loose skill and agent directories
  keep `GeneratedTree`, because an edit there is the operator's.

### D5. Setup's machine artifacts get the receipts they lack

Some setup artifacts already have receipts:

- The rules entries in `settings.json` and `hooks.json` (`SETUP_OWNER`).
- The hook wrapper, owned by `HookConfigEntry.wrapper`. It keeps that owner,
  and the field now names `<harness>/uze/bin/exec`.

These gain receipts: the rules engine file, OpenCode's `uze.ts`, OpenCode's
hook-group documents, and the bundled `uze` plugin's deliveries. `remove`
keeps refusing the bundled plugin; uninstall is its only remover.

Per-session state moves with its reader:

- **Rules sessions** move to `<harness>/uze/rules/sessions`. The engine
  already creates that directory on demand.
- **OpenCode's session documents** move to `<harness>/uze/sessions`. The
  integration now answers both the sweep (`setup.rs`) and the workspace's
  deletion of them (`conversation.rs`), instead of `home.session_context_dir`.

The bundled skills that call `uze` gain one opening line: when `uze` is not
on `PATH`, the skill does not apply. The doctor sweep from
`plugin-freshness-and-linked-marketplaces` recognises references into
`owned_directory()` as UZE's.

### D6. Codex hook trust is part of the migration

Codex keys a trusted hook by its command and its index, so relocating the
wrapper turns every trusted hook into "Modified". `codex exec` then skips
it without a word.

- Migration names the Codex hooks that now need the person's review, in its
  report and in `doctor`.
- A `HarnessFact` records this behaviour, and conformance checks it.
- Fresh installs are unaffected.

### D7. `uze uninstall`

**Before anything.**
- The command skips bootstrap, as `upgrade` does. Otherwise
  `ensure_default_plugins` would re-deliver before the first step.
- It refuses when run inside a workspace pane.
- It refuses while UZE agents, clients, the terminal runtime with panes, or
  a background `upgrade` are alive. Each refusal names what is running.

**Plan.** `UzeApplication::uninstall_plan()` is a read model. It lists:
- per harness, the receipts and their inspection, plus files the vendor CLI
  wrote that name UZE (marketplace registrations, `~/.claude.json`);
- setup artifacts, each top-level entry of `$UZE_HOME` with its fate, and
  the records directory and its size;
- per project, checkouts and `refs/uze/shelf/*` holding unlanded work, and
  links pointing into `$UZE_HOME`;
- provisioned harnesses, how the binary was installed, and whether a vendor
  CLI is missing.

**Execute.** Run under the mutation lock. It takes a copy of the binary's
provenance first, then runs these steps in order, each idempotent, with the
plan recomputed on every run:

1. Per harness, detach readers before what they read:
   - first configuration entries, `uze.ts` and plugin and marketplace
     registrations;
   - then the UZE directory's material.

   This keeps OpenCode's bridge from failing closed on a half-removed state.
   When a vendor CLI is missing, its registrations are listed as left and
   never edited by hand.
2. Remove workspace links that point into `$UZE_HOME`. The
   `.git/info/exclude` line, checkouts and refs are left and reported.
3. Remove the binary when the installer placed it, using the
   `install.json` copy:
   - Linux and macOS: unlink.
   - Windows: undo the recorded user `Path` edit, then schedule removal of
     the running `uze.exe` on exit.

   Otherwise print the command for the way it was installed.
4. Without `--purge`: clear `packages.json` and `attachments.json`, so no
   registry outlives its Store, and remove everything except `state/`'s
   records.

   With `--purge`: remove `$UZE_HOME` last.

The command is classified `JustifiedSlow`.

### D8. Proof

- **Unit:** `vendor_neutral.rs::no_harness_artifact_names_uze_home` installs
  the canonical fixtures for all four fake harnesses. It then walks the
  **whole fake home** except `$UZE_HOME` for the prefix. That covers files
  written by vendor CLIs that no receipt names.
- **Acceptance:** `tests/acceptance/uninstall.rs` covers one case per
  uninstall scenario in the spec.
- **Conformance, per vertical:**
  - with `$UZE_HOME` moved away, plugins list and load, a deny hook denies,
    and an `always_on` rule reaches the session;
  - after `uze uninstall --yes --purge`, a grep of `$HOME` finds nothing;
  - decoy `SKILL.md`, agent and `hooks.json` planted in `<root>/uze/` never
    load.

## Risks / Trade-offs

- **[Bytes: one package root per harness, up to four, versus one shared
  `runtime/packages` today]** → mitigations:
  - `std::fs::copy` clones on CoW filesystems;
  - copying is gated by digest, so an unchanged package costs no I/O;
  - `node_modules`-sized trees are excluded at ingest.

  This is the price of each harness standing alone.
- **[A vendor starts scanning `<root>/uze/`]** → the decoy conformance check
  fails first.
- **[Vendor caches still hold the old text after migration]** (cached
  `SKILL.md` and `.mcp.json` naming `runtime/packages`) → migration
  reinstalls each repointed plugin through the vendor (`plugin update` or
  uninstall-install). Task 1.2 measures which command refreshes each cache.
- **[Uninstall while a harness session is open]** → Claude reads the
  marketplace live, so the open session loses the plugin. The plan names
  the running harness processes. This is the same behaviour as `remove`.
- **[A drifted old artifact keeps naming `$UZE_HOME`]** → it is listed by
  migration and by uninstall. The spec names it as the one allowed residue.

## Migration Plan

1. No `attachments.json` shape change: package roots use `IntegrationOwned`.
   `packages.json` gains an optional cached digest, with a `serde` default
   and no rung.
2. Run `setup`, `install` or `update`. For each harness:
   1. Publish to the new marketplace root.
   2. Repoint the marketplace (D2).
   3. Refresh the vendor cache of each plugin.
   4. Retire old receipts by inspect-before-detach.
   5. Relocate the wrapper, then report the Codex hooks that need review
      (D6).
3. Delete `runtime/attachments/*` and `runtime/packages` only when no
   receipt names them. A harness that could not be reached keeps its old
   source until the next run.

Rollback: an older build reads the new receipts but resolves nothing under
`<harness>/uze`. It re-delivers to its own locations on its next install,
which is the same "two builds on one machine" behaviour as today.

## Candidate ADRs

- **What a harness reads lives in the harness's territory**: this amends
  ADR-052 (no tier under `$UZE_HOME` is read by a harness, and integrations
  name their owned directory) and ADR-040 (where the wrapper lives). It binds
  every future integration.
