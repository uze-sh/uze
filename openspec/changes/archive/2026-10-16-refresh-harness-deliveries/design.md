## Context

See proposal.md for the survey this change absorbs. Two constraints shape
everything:

- **Measure before believing.** Every vendor claim here comes from a
  changelog or a doc page. Each item is first reproduced against the real
  harness (Lab `--sandbox`/`--experiment`, or the Windows Sandbox), with no
  UZE in the probe where the claim is about the vendor alone
  (`conformance-debug`, Step 4). The outcome is written into this file in
  place; an item the probe refutes is removed from the specs and tasks of
  this change, not carried as a hope.
- **Nothing about instruction files.** `AGENTS.md`, `CLAUDE.md`,
  `GEMINI.md` and how each harness resolves them, including PR #154's
  native Claude context, belong to `resolve-instruction-files`.

## Goals / Non-Goals

**Goals:**
- Fix the Antigravity census break before anything else.
- Give UZE a harness version floor, which three of the items below need
  and which does not exist today (`HarnessDetection.version` is only
  displayed).
- Move every delivery the survey found a native mechanism for up the
  ladder, once the Lab has seen it work.
- Leave every gate's evidence recorded on the release the integrations
  claim.

**Non-Goals:**
- Claude Mods, a portable plugin `bin/` capability, OpenCode's
  `permission.assert`/native refusal: each changes a trust class or waits
  on an unreleased version, and gets its own change.
- Forbidding or steering the harnesses' worktree tools (an OpenCode
  worktree strategy delegating to `uze agent work split`). This change
  makes what they create visible and safe.
- Honouring `CODEX_HOME`. The Codex integration resolves every path from
  `command_home/.codex` (`codex.rs:96-120`); honouring the variable for one
  path would be a second rule. Whether to honour it everywhere is its own
  change.

## Decisions

### D1. Probes gate implementation, and their outcome is edited in here
Each decision below names the probe that decides it. Results go into a
`Measured:` line under the decision (version, date, Lab check or
experiment). Task 1.9 lists which spec blocks each probe can remove.
Alternative considered: one exploration change followed by an
implementation change. Rejected: most probes are the same Lab check that
will guard the item, and splitting would record the measurement twice.

### D2. A harness version floor
Each integration declares the oldest version its deliveries are written
for, as a port method returning a version the core compares without
knowing the vendor. `uze doctor` and `uze status` compare it against the
detected version and report a harness below it (spec:
`harness-provisioning`); an operation that fails because of it says so.
No older path is kept for a version below the floor: the floor is the
ladder's bottom rung for harness behaviour, the way a record shape is for
records. An unreadable version is treated as meeting the floor, so a
vendor changing its `--version` output never blocks anyone. Version
comparison lives in core as a plain dotted-number compare; each
integration already parses its own `--version` line.

Floors this change sets: Antigravity 1.3.4 (D3). Claude needs none: D5's
probe found older Claude ignores `onFailure`.

### D3. Antigravity census: one `agy plugin list --json` per operation
1.3.4 returns a top-level array `[{name, enabled, version?, source, path,
skills}]` covering imported and marketplace plugins; `import_manifest.json`
knows only imports and carries no source path (`plugin.rs:158-160`).
Install preflight and detach read the census once per operation from
`agy plugin list --json`, and that one reading answers both "is the name
free" and "is this registration UZE's" (matched by name and staged path).
Two plugins of one name make inspection `Conflict` and detach a no-op,
because `agy plugin uninstall <name>` removes by name and could reach the
foreign one.

The budgeted inspection path (`remove`/`inspect`, `command_performance.rs:47-49`)
keeps reading `import_manifest.json` — its `{"imports": …}` shape and the
`"imports": null` case stay — so no subprocess returns to a budgeted path.
It can still answer Matched for UZE's own import; a marketplace twin is
only seen by the CLI census, which is why detach, not inspection, is where
the refusal lives. What is deleted is only the 1.1.x text reading of
`agy plugin list` and the `No imported plugins.` literal; an unknown CLI
shape reports the state unknown and blocks.

Probe 1.1: install `<name>@antigravity-plugins-official`; record whether it
lands in `import_manifest.json` and in `plugin list --json`, whether an
import of the same name can coexist with it, and which one `agy plugin
uninstall <name>` removes.

Measured: Antigravity 1.3.4, the operator's own signed-in `~/.gemini`,
2026-10-14 (restored afterwards). `agy plugin install
<p>@antigravity-plugins-official` from the shell answers `unknown
marketplace`; `agy --print "/plugin install …"` installs. The plugin is in
`plugin list --json` (`"source": "antigravity-plugins-official"`, `path`
under `~/.gemini/config/plugins/<name>`, the folder an import of the same
name would use) and absent from `import_manifest.json`: the break is real
and the CLI census sees it. Whether an import and a marketplace plugin of
one name can coexist, and which `uninstall` removes, was not measured: the
probe that imports over a marketplace plugin was refused as destructive on
the operator's machine. The vendor's changelog says a marketplace install
"no longer overwrites an existing plugin folder that came from a different
source"; the other direction is UZE's preflight.

Measured with the operator's session (task 3.5): Antigravity 1.3.5,
2026-10-14, a throwaway `HOME` holding a copy of the signed-in token (the
operator's `~/.gemini` untouched, the copy deleted afterwards), `uze` from
this branch, `modern-web-guidance-plugin@antigravity-plugins-official`
against a UZE package of the same name. The Lab cannot run this: the
official marketplace needs a signed-in session and the Internet.
- Marketplace first, then `uze install`: the marketplace plugin's
  directory and listing are unchanged, and UZE records no delivery. The
  install reported success and only `uze doctor` named the skill missing
  (the preflight's "successful no-op"). Now the delivery is refused aloud:
  the holder is named, with `agy plugin uninstall <name>` as the way to
  free it.
- UZE first, then `/plugin install` of the same name: agy answers `Plugin
  "…" is already installed` and changes nothing, so the twin of the
  Conflict scenario cannot be produced through the vendor's own verbs.
- UZE's plugin uninstalled by hand and the marketplace one installed in its
  place, then `uze remove`: UZE drops its receipt and the marketplace
  plugin stays installed, skills intact. Inspection reported UZE's plugin
  as "not imported"; it now says another plugin holds the name.

### D4. OpenCode: UZE names the session; following and lookup stay correct
Since ≈2.0.19, `--session <id>` creates the session when it does not
exist. OpenCode moves from `SessionContinuity::Observed` to `Assigned`.

- **Id shape.** `continuity::start` mints with `SessionId::generate()`
  (UUID v4, `continuity.rs:125-131`), whose doc says every naming harness
  demands a UUID. The port gains `new_session_id()`, defaulting to
  `SessionId::generate()`; OpenCode overrides it with the shape
  `packages/schema/src/session-id.ts` requires (`ses` prefix, body read off
  the schema in probe 1.2). The vendor shape stays in the integration; the
  doc on `generate` is corrected.
- **Following.** `refresh()` follows the agent to a newer conversation
  through `observe_session` even for `Assigned` harnesses, and OpenCode has
  no hook dispatch UZE runs to tell it (`opencode/session.rs:14-19`). So
  `observe()` keeps the "newest session since launch in this cwd" reading,
  and only `exists()` changes, to `GET /api/session/<id>`, whose answer is
  independent of the listing's 50-entry page. A session moved by
  `session_move` (D6) is still found by id.

Probe 1.2: launch twice with one minted id, the second continues the
first; `GET /api/session/<id>` answers for a session outside the first
page; `/new` inside the TUI is followed by the record.

Measured: OpenCode 2.0.29, 2026-10-14, `experiments/opencode/session-assigned`
3/3. `--session ses_uzelab…` (the schema only requires the `ses` prefix)
creates the session under that id; a second run under it adds no session
(6 in the directory, not 7); `GET /api/session/<id>` answers it; the
default listing returns 50. `/new` following is unchanged code
(`observe()` is kept) and was not re-measured.

### D5. Claude block-on-failure: one marker per hook dialect, and an upgrade path
`onFailure: "block"` (2.1.295) is emitted on deny/ask/transform groups for
the Claude target only. `handler_entry` is shared by the command-hook
targets, so the dialect gains a field naming its native failure marker, if
any; no vendor branch at the call site.

Delivered entries pick it up on the next `uze install` or `uze setup`:
every attach renders the entry again and replaces the version its receipt
recorded (`merge_event_entry` with the previous content), so no Stale
state is needed. Until that run, inspection keeps reading the earlier
entry as Matched, which it is.

The Lab proof is a "wrapper removed" variant of the contract's
`_fail_closed` (`conformance/contract/hooks.py:360`), declared through
`bindings.unsupported` with a reason on targets without a native marker —
not a Claude-only scenario.

Probe 1.3: without UZE, a missing hook command with `onFailure: "block"`
blocks the call; whether 2.1.294 accepts or rejects the key.

Measured: 2026-10-14, `experiments/claude/on-failure` (no UZE in the hook).
Claude 2.1.299: a missing hook command and a hook past its timeout, both
with `onFailure: "block"`, blocked the `Bash` call; the same missing
command without the key let it run. Claude 2.1.291 (the previous Lab
image): a denying hook carrying the key still loaded and denied, like the
one without it, so an older Claude ignores the key rather than rejecting
`settings.json`. The marker is emitted unconditionally; no Claude floor.

### D6. Harness worktree places may live outside the project
`own_worktree_dirs() -> &'static [&'static str]` answers only "relative to
a checkout" (consumed in `checkouts.rs:215`, `accounting.rs:69`). Codex
0.162 creates worktrees under `~/.codex/worktrees` when the model calls
`create_worktree` (default on); OpenCode ≈2.0.27 creates them under
`worktree.directory` via `worktree_create`. The port returns resolved
places of a core type with no vendor in it: `WorktreePlace::InCheckout(rel)`
or `WorktreePlace::Outside(PathBuf)`. The integration resolves an outside
place from its own home (`command_home/.codex/worktrees` for Codex); a
per-OS data directory (OpenCode's default, if probe 1.4 finds one) is
resolved through `uze-platform`'s profile directories, never with a `cfg`
in the integration. Accounting classifies a registered worktree under
either kind as the harness's.

Alternative considered: launching Codex with `--disable worktrees`.
Rejected for now: an explicit feature override triggers Codex's daemon
compatibility prompt (0.162.1), a new prompt between UZE and the operator.

Probe 1.4: in the Lab, have each model create a worktree through its tool;
read where it lands and that the slot record is unaffected.

Measured (where, not yet the tool call): Codex 0.162.1 source,
`codex-rs/worktree/src/settings.rs`: `$CODEX_HOME/worktrees`. OpenCode
`packages/core/src/worktree/strategies.ts` (dev, 2026-10-14):
`<global.data>/worktree/<project id>`, and `opencode debug paths` on 2.0.29
reports `data` as `~/.local/share/opencode` — the XDG rule the integration
already follows for `~/.config/opencode`, so no per-OS resolution is
added. Measured with the tool, OpenCode 2.0.29, 2026-10-14
(`experiments/opencode/worktree-tool`): `worktree_create` is a Code Mode
tool, reached through `execute` as `tools["opencode"].worktree_create`, and
the worktree it made landed at
`~/.local/share/opencode/worktree/<project id>/<name>`, inside the declared
place. Codex's `create_worktree` is offered through the TUI's own MCP
server only and was not driven; its place is read from the source.

### D7. Antigravity plugin hooks and agents: measured, then moved or kept
Docs since 1.3.4 say runtime discovery loads `<plugin_dir>/hooks.json`;
1.3.3 added a root `agents.json`. Today hooks go to the shared
`~/.gemini/config/hooks.json` — which the hooks contract already proves on
every run (`scenarios.py:671-673`) — and agents to loose files. What no
check measures is the negative the delivery rests on: that a plugin's own
`hooks.json` is not loaded (`phase_hooks_delivery`, cited in
`scenarios.py:569`, does not exist). The vendor leg gains that probe as a
check. If the plugin's `hooks.json` loads and acts, hooks move into the
generated plugin: existing shared-file receipts are retired before the new
ones attach (the "earlier shape retired first" rule of
`transparent-harness-attachment`), and the `exact coverage` requirement and
the evidence strings feeding the harness matrix (`antigravity.rs:312,509`)
are amended in this change. Agents move likewise through `agents.json` if
the subagent dispatches and reaches the plugin's MCP.

Probe 1.5: `--experiment` with a hand-made plugin, reading `loaded N named
hooks from N hooks.json file(s)` and the hook's effect.

Measured: Antigravity 1.3.5, 2026-10-14, `experiments/antigravity/plugin-hooks`
(no UZE in the plugin). `agy plugin install` reports `hooks: 1 processed`
and `agents: 1 processed`; a headless session logs `loaded 1 named hooks
from 1 hooks.json file(s)`; `/hooks` lists the hook with `source` the
plugin's own `hooks.json`; `plugin list --json` adds `hooks` and `agents`
keys to the plugin's entry. Loading is proven; whether a deny from a
plugin hook gates a TUI call, and whether the subagent dispatches and
reaches the plugin's MCP, are not yet measured.

Measured after the move, 2026-10-14, Antigravity 1.3.5: with hooks in the
generated plugin's `hooks.json` and no shared `hooks.json` on the machine,
the vertical passed every hooks check (57/57); `lifecycle-plugin`, which is
delivered as a generated plugin, has its `keeper` agent offered and
dispatched by label, its body reaching the model
(`experiments/antigravity/plugin-agent-dispatch`). The agent contract's own
`flow` is delivered capability by capability, so it does not cover this
route. Whether a plugin's subagent reaches the plugin's own MCP was not
measured.

### D8. Codex Windows PreToolUse: measured in the Windows Sandbox
Upstream closed #24453 as resolved in 0.159.0, with no release note. If a
shell PreToolUse fires and deny/transform hold on the current Codex, the
`Unfired` entry and its test go; otherwise its `why` names the version
measured.

Measured: Codex 0.162.1, 2026-10-14, Windows Sandbox as an ordinary
account, `uze setup codex` and `uze install` from this branch, real
`codex exec` turns (hook trust recorded by its own hash, since `codex exec`
skips an untrusted hook in silence). A shell call fires PreToolUse as
`Bash` with `command`, for a hand-written hook in Codex's own format and
for UZE's: UZE's `transform` ran the rewritten command, and its `deny`
blocked with the handler's reason once the entry line exited with the
wrapper's code. Before that it did not: Codex runs the line with
PowerShell's `-Command`, which reports 1 for the wrapper's 2, and Codex
lets a call through on 1. `uze_platform::shell::command_line` now ends the
Windows line with `exit $LASTEXITCODE`, which also carries the fail-closed
2 of a handler that failed. Nothing declares an `Unfired` any more, so the
mechanism went with Codex's entry.

### D9. Antigravity MCP `url`: measured before it reverses a verified fact
The 1.3.3+ docs make `url` the primary key and `serverUrl` legacy. UZE
records the opposite as verified (`docs/architecture/antigravity-compatibility.md:52`,
ADR-027:40, `generate.rs:147-163`, `antigravity.rs:37`, the Antigravity
README). If a generated plugin with `url` exposes the remote server and
its tool reaches the model on 1.3.4, `url` is kept and `httpUrl` rewritten
to it, and every place above is corrected; otherwise the item leaves the
change and the spec scenario reverts.

Measured: Antigravity 1.3.5, 2026-10-14, `experiments/antigravity/mcp-url`.
A Streamable HTTP server in its own container on the run's network, in a
plugin installed with `uze install`: with the generated `mcp_config.json`
under `serverUrl` (the build before) and under `url` (this one) alike, agy
initialized it, listed its tools, offered the tool to the model, sent the
`tools/call`, and its result reached the next model request. `url` is
kept; `httpUrl` is renamed to it.

Probe 1.8.

### D10. Evidence last
The four gates are re-recorded after the code lands, so the evidence
proves what ships; `FACTS` `measured_on`/`VERSION`, tool snapshots,
`expected.json` and the regenerated harness matrix agree with
`conformance/evidence/<harness>.json`.

## Candidate ADRs

- A harness version floor, with no path kept below it — a new rule every
  integration follows.
- At archive, amend as measured: ADR-040 (Antigravity hooks in the plugin,
  if D7 confirms), ADR-027 (census, `url`), ADR-047 (OpenCode `Assigned`).

## Risks / Trade-offs

- [A vendor doc overstates the release] → D1: nothing ships on a doc
  alone; the Lab check that proves it is the one that guards it.
- [Floors rise on three harnesses at once] → D2 reports them by name in
  `doctor`/`status`; the floors are stated in the release notes.
- [Harness worktrees still escape UZE's slots] → they are visible and
  never removed, and lookup by id survives a moved session; steering them
  into slots is a later decision.
- [Hook entries delivered before this change keep the old shape] → D5
  makes them Stale and rewritten, with an upgrade test.
- [Antigravity inspection still Matched with a marketplace twin present] →
  detach reads the CLI census and refuses; inspection stays on the
  budgeted manifest read on purpose.
