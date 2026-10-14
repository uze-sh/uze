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
- **PR #154 is not rebased.** Its description and diff are the
  specification of the Claude half; the code is written again on current
  `main`, and everything #154 touched is accounted for in tasks (section 3).

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
experiment). Task 1.10 lists which spec blocks each probe can remove.
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

Floors this change sets: Antigravity 1.3.4 (D3), Claude 2.1.281 (D4), or
2.1.295 if D6's probe says older Claude rejects `onFailure`.

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

### D4. Claude context: rebuild #154's outcome
`ContextDelivery::Native { files, shadowed_by }` — `files` stays, because
Antigravity declares `GEMINI.md` there (`antigravity.rs:276-280`, consumed
in `context.rs:62`). Claude declares `shadowed_by: [CLAUDE.md,
.claude/CLAUDE.md, CLAUDE.local.md]`. `ContextDelivery::Bridge` and
everything only it used (bridge plan/reconcile, `BridgePlan`,
`BridgeStatus`, `ContextMechanism::Bridge`, `ResourceDelivery::Bridged`,
`HarnessContextDelivery` `BRIDGE`/`PROJECTED`) are deleted.

Shadowing is judged at the project root. A file does not shadow when it is
empty, imports `@AGENTS.md` or `@./AGENTS.md`, resolves (symlink) to the
project's own `AGENTS.md`, or holds only an earlier build's
`instruction-bridge` region. The shim drops the `@AGENTS.md` projection and
`CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD` — the vendor doc says a
`--add-dir` with that variable does not load `AGENTS.md`, so keeping it
would be wrong — and activates only when `.agents/` exists, projecting what
of `.agents/` Claude does not read (skills and agents today; `project-rules`
may add rules). Floor: 2.1.281.

`/uze:init` gets #154's whole rework, not just Flow B: Flow B's
precondition is "no AGENTS.md", so the shadowed case needs the
`PARTIALLY_PORTABLE` routing, the `PORTABLE` summary, the MUST NOT lines,
Flow B's title and the plugin description too.

**Migration is the person's agent's job, not reconcile's.** With Claude
reading `AGENTS.md`, the target state is one canonical `AGENTS.md`, and a
`CLAUDE.md` that is either gone or only `@AGENTS.md` plus Claude-only
lines. Getting there means classifying each instruction (portable,
Claude-only, ambiguous), inlining what a `CLAUDE.md` `@path` import pulls
in (no other harness expands that syntax), and, when both files exist,
merging two texts that may disagree. None of that is deterministic, and
`CLAUDE.md` is authored text, which UZE never writes over. So `/uze:init`
does it, with a dry-run and the person's confirmation:
- Flow B (vendor file, no `AGENTS.md`) ends with `CLAUDE.md` reduced to
  the import plus Claude-only lines, no longer "untouched below a bridge".
- A new flow for `ShadowedBy` (`AGENTS.md` exists and `CLAUDE.md` shadows
  it) merges the portable content of both into `AGENTS.md`, each
  disagreement asked, never resolved by picking a side.
- Keeping the one-line `@AGENTS.md` instead of deleting `CLAUDE.md` is the
  default: per the vendor doc it never makes Claude read the file twice,
  and it still delivers on a Claude below 2.1.281 or with
  `instructionFiles: "claude-md"`, which UZE cannot see. Deleting it is
  offered, not done.
- `CLAUDE.local.md` is personal and uncommitted; it is reported, never
  merged into the committed baseline.
- Nested `CLAUDE.md` files in subdirectories are proposed as nested
  `AGENTS.md` files, which Claude, Codex and OpenCode all read.

Alternative considered: `reconcile` migrating automatically (rename when
only `CLAUDE.md` exists, concatenate when both do). Rejected: a rename
carries Claude-only lines and `@` imports to harnesses that misread them,
a concatenation duplicates and contradicts, and both write an authored
file without the person.

**BREAKING** surfaces: `agent context plan|reconcile --format json` lose
`bridges`; `agent context inspect --format json` changes
`HarnessContextDelivery`.

Alternatives kept from #154's review: keeping `Bridge` with no declarer
(rejected, dead code); removing earlier builds' regions (rejected, they
still deliver through the import and are the operator's file).

Probe 1.2: Lab contract leg on the current Claude with no `CLAUDE.md` and
no runtime import; `CLAUDE.local.md` shadowing; and a launch inside a slot
(`.worktrees/<id>`, nested under the primary) to confirm which
`AGENTS.md`/`CLAUDE.md` reaches the model when Claude walks ancestors.

### D5. OpenCode: UZE names the session; following and lookup stay correct
Since ≈2.0.19, `--session <id>` creates the session when it does not
exist. OpenCode moves from `SessionContinuity::Observed` to `Assigned`.

- **Id shape.** `continuity::start` mints with `SessionId::generate()`
  (UUID v4, `continuity.rs:125-131`), whose doc says every naming harness
  demands a UUID. The port gains `new_session_id()`, defaulting to
  `SessionId::generate()`; OpenCode overrides it with the shape
  `packages/schema/src/session-id.ts` requires (`ses` prefix, body read off
  the schema in probe 1.3). The vendor shape stays in the integration; the
  doc on `generate` is corrected.
- **Following.** `refresh()` follows the agent to a newer conversation
  through `observe_session` even for `Assigned` harnesses, and OpenCode has
  no hook dispatch UZE runs to tell it (`opencode/session.rs:14-19`). So
  `observe()` keeps the "newest session since launch in this cwd" reading,
  and only `exists()` changes, to `GET /api/session/<id>`, whose answer is
  independent of the listing's 50-entry page. A session moved by
  `session_move` (D7) is still found by id.

Probe 1.3: launch twice with one minted id, the second continues the
first; `GET /api/session/<id>` answers for a session outside the first
page; `/new` inside the TUI is followed by the record.

### D6. Claude block-on-failure: one marker per hook dialect, and an upgrade path
`onFailure: "block"` (2.1.295) is emitted on deny/ask/transform groups for
the Claude target only. `handler_entry` is shared by the command-hook
targets, so the dialect gains a field naming its native failure marker, if
any; no vendor branch at the call site.

Delivered entries do not pick it up on their own: inspection compares the
written entry with the receipt's expected entry by equality
(`hooks/event_entry.rs:122-129`), so an entry written before this change
stays Matched forever. An entry whose current rendering differs from its
receipt becomes Stale (as the wrapper already does, `event_entry.rs:236-243`)
and install/setup rewrites it.

The Lab proof is a "wrapper removed" variant of the contract's
`_fail_closed` (`conformance/contract/hooks.py:360`), declared through
`bindings.unsupported` with a reason on targets without a native marker —
not a Claude-only scenario.

Probe 1.4: without UZE, a missing hook command with `onFailure: "block"`
blocks the call; whether 2.1.294 accepts or rejects the key.

### D7. Harness worktree places may live outside the project
`own_worktree_dirs() -> &'static [&'static str]` answers only "relative to
a checkout" (consumed in `checkouts.rs:215`, `accounting.rs:69`). Codex
0.162 creates worktrees under `~/.codex/worktrees` when the model calls
`create_worktree` (default on); OpenCode ≈2.0.27 creates them under
`worktree.directory` via `worktree_create`. The port returns resolved
places of a core type with no vendor in it: `WorktreePlace::InCheckout(rel)`
or `WorktreePlace::Outside(PathBuf)`. The integration resolves an outside
place from its own home (`command_home/.codex/worktrees` for Codex); a
per-OS data directory (OpenCode's default, if probe 1.5 finds one) is
resolved through `uze-platform`'s profile directories, never with a `cfg`
in the integration. Accounting classifies a registered worktree under
either kind as the harness's.

Alternative considered: launching Codex with `--disable worktrees`.
Rejected for now: an explicit feature override triggers Codex's daemon
compatibility prompt (0.162.1), a new prompt between UZE and the operator.

Probe 1.5: in the Lab, have each model create a worktree through its tool;
read where it lands and that the slot record is unaffected.

### D8. Antigravity plugin hooks and agents: measured, then moved or kept
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

Probe 1.6: `--experiment` with a hand-made plugin, reading `loaded N named
hooks from N hooks.json file(s)` and the hook's effect.

### D9. Codex Windows PreToolUse: measured in the Windows Sandbox
Upstream closed #24453 as resolved in 0.159.0, with no release note. If a
shell PreToolUse fires and deny/transform hold on the current Codex, the
`Unfired` entry and its test go; otherwise its `why` names the version
measured.

### D10. Antigravity MCP `url`: measured before it reverses a verified fact
The 1.3.3+ docs make `url` the primary key and `serverUrl` legacy. UZE
records the opposite as verified (`docs/architecture/antigravity-compatibility.md:52`,
ADR-027:40, `generate.rs:147-163`, `antigravity.rs:37`, the Antigravity
README). If a generated plugin with `url` exposes the remote server and
its tool reaches the model on 1.3.4, `url` is kept and `httpUrl` rewritten
to it, and every place above is corrected; otherwise the item leaves the
change and the spec scenario reverts.

Probe 1.9.

### D11. Evidence last
The four gates are re-recorded after the code lands, so the evidence
proves what ships; `FACTS` `measured_on`/`VERSION`, tool snapshots,
`expected.json` and the regenerated harness matrix agree with
`conformance/evidence/<harness>.json`.

## Candidate ADRs

- Claude reads `AGENTS.md` natively; the `CLAUDE.md` bridge is removed —
  revises ADR-014's runtime projection and retires the bridge mechanism.
- A harness version floor, with no path kept below it — a new rule every
  integration follows.
- At archive, amend as measured: ADR-040 (Antigravity hooks in the plugin,
  if D8 confirms), ADR-027 (census, `url`), ADR-047 (OpenCode `Assigned`).

## Risks / Trade-offs

- [A vendor doc overstates the release] → D1: nothing ships on a doc
  alone; the Lab check that proves it is the one that guards it.
- [Floors rise on three harnesses at once] → D2 reports them by name in
  `doctor`/`status`; the floors are stated in the release notes.
- [Shadowing judged at the root only] → a `CLAUDE.md` above the root or a
  user `instructionFiles: "claude-md"` also disables the reader and UZE
  cannot see either; stated in the Claude README, as in #154. Probe 1.2
  covers the nested-slot case.
- [`agent context --format json` changes shape] → `uze agent` is an
  agent-audience ABI; the change is marked breaking.
- [Harness worktrees still escape UZE's slots] → they are visible and
  never removed, and lookup by id survives a moved session; steering them
  into slots is a later decision.
- [Hook entries delivered before this change keep the old shape] → D6
  makes them Stale and rewritten, with an upgrade test.
- [Antigravity inspection still Matched with a marketplace twin present] →
  detach reads the CLI census and refuses; inspection stays on the
  budgeted manifest read on purpose.
