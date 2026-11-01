## Context

See proposal.md for why; the requirements are in `specs/machine-overview/spec.md`.

What exists today and shapes the approach:

- `Route::Overview` is the first route, gated by `Feature::Overview`
  (`crates/uze-core/src/machine/features.rs`), whose comment says what the
  screen should be is undecided. `src/ui/view/overview.rs` draws two
  counts.
- `IntegrationPort` already has optional readers whose default is "not
  known" (`conversation_digest`, `memory_source`, `session_recorded_for`).
  This change follows that pattern.
- The integrations already parse each harness's skills, agents, MCP and
  hook configuration to deliver uze's packages
  (`crates/uze-integrations/src/<harness>/{skills,mcp,hooks,plugin}.rs`).
- `conversation_digest` already reads a harness's own session records for
  the briefing; uses are read from the same records.
- OpenCode's sessions are asked of the harness, never of its database.
  Antigravity's per-conversation databases are never opened. Neither
  stance changes.
- uze never touches foreign hooks, plugins or order (delivery model). A
  read-only Overview keeps that.
- Every background read answers through a channel, a late answer is
  dropped, a panicking read answers "not known", and cache is never
  required for correctness (`docs/architecture/invariants.md`).

Why this scope: what comes from disk and configuration is answerable by all
four harnesses. Uses were measured in the Lab on each harness's latest
release (`conformance/experiments/<harness>/{usage-trace,user-invocation}.py`):
every use of a skill, an agent or an MCP server reaches the harness's own
records, including the ones the person types, which no tool hook sees. The
one gap is Codex's MCP calls, recorded only inside the `exec` script that made
them. Per-session tokens of MCP tools and hooks, hook runs and MCP sign-in
state stay out.

## Goals / Non-Goals

**Goals:**
- The same answers from all four harnesses; a new harness appears by
  implementing its optional methods.
- No cost on any path but the Overview in front: launch, shim, CLI, other
  screens, any frame.
- No hook delivered and no cost on an agent's turn: uses are read after the
  fact, from records the harnesses already write.
- From a record, only which resource was used, by whom and when.

**Non-Goals:**
- Per-session tokens of MCP servers and hooks, hook runs, MCP sign-in state.
- Uses of an instruction file (always on) or of a hook.
- Adopt; executing any removal; starting any MCP server.
- Per-project resources; a CLI report; an exact per-model tokenizer.

## Decisions

### 1. Optional `IntegrationPort` methods; the core owns the rest

| method | answers | default |
|---|---|---|
| `machine_inventory()` | found resources: kind, name, the files that define it, origin hint (harness plugin, built-in, by hand), the command or path it runs (hooks, local MCP), the harness's own removal command when there is one | empty |
| `session_count(since)` | sessions with activity since a time, subagents excluded | `None`, so the count is unknown |
| `usage_records(since)` | the records of sessions active since a time, each with a stamp (length and modification time, or the harness's own "updated"), from metadata only | `None`, so uses are unknown |
| `read_usage(record, from)` | the uses in one record from a resume point, and where the next reading resumes | `None` |

The core owns everything harness-neutral:
- `uze` ownership from receipts;
- merging what uze delivered across harnesses;
- duplicate detection (same kind and name, different content digest);
- token estimation from the files;
- the static health checks;
- which records to read again, the cursors, the daily tallies, and joining
  a use to the resource it names;
- status and order;
- the snapshot and scheduling.

A third method was rejected: the removal command is a field of a found
resource. `src/` reaches all of this through a `uze-application` read
model, never `uze_core`, as the layering rules require.

### 2. Tokens: a local estimator over the artifact's files

The core estimates tokens from text length, with a per-script ratio
calibrated by a test, and always shows them with `~`. What counts per
session:
- a skill's or agent's frontmatter `name` and `description`;
- an always-on instruction file, whole.

What counts when used is the rest of the definition file. A file over a
cap (1 MB) is measured by its size, without reading it all.

A real BPE tokenizer was rejected: it is a dependency with MBs of tables,
and still not each vendor's tokenizer. The screen ranks resources and needs
only an estimate.

### 3. Static health, in core

The core judges broken from the facts the inventory hands it:
- a hook command whose file is missing;
- a local MCP command not found, resolved on `PATH` through `uze-platform`
  (no `cfg!` at the call site);
- a skill or agent file that cannot be read, or whose frontmatter does not
  parse;
- a plugin directory that is missing.

A URL-reached MCP server is never judged, since nothing on disk can prove
it. Nothing is executed to check.

### 4. Sessions, per harness

| | how sessions are counted | subagents excluded by |
|---|---|---|
| Claude Code | root `projects/*/*.jsonl` by modification time | living under `<session>/subagents/` |
| Codex | rollouts under `sessions/` by modification time | the opening `session_meta` carrying `parent_thread_id` (the first line only) |
| OpenCode | `opencode stats --json`, run from a directory outside every project with a short timeout | its own `sessions` / `subagents` split |
| Antigravity | `conversations/*.db` by the newer of the file's and its `-wal`'s modification time, none opened | not needed: it records no subagent conversations there |

The OpenCode figure is used only if the Lab proves it is machine-wide;
otherwise it is unknown.

### 5. Inventory, per harness

| | skills | agents | mcp | hooks | rules | plugins |
|---|---|---|---|---|---|---|
| Claude Code | `~/.claude/skills`, plugins | `~/.claude/agents`, plugins | `~/.claude.json`, plugins | `settings.json` | `~/.claude/CLAUDE.md` | `plugins/installed_plugins.json` |
| Codex | `~/.agents/skills`, `~/.codex/skills` | `~/.codex/agents` | `config.toml` | `hooks.json` | `~/.codex/AGENTS.md` | `[plugins.*]` |
| OpenCode | `~/.config/opencode/skills` | `…/agents` | `opencode.json` | its plugins | global `AGENTS.md` | `…/plugins` |
| Antigravity | `~/.gemini/…/skills`, `builtin/skills` | `~/.gemini/config/agents` | its MCP config | `hooks.json` | the global instructions file the Lab confirms | its plugins |

### 6. Scheduling and the snapshot

The workspace collects in the background (D13): 20 seconds after it opens
and every 15 minutes from then on, on a thread of its own
(`orchestrator::reads::spawn_machine_sync`), whether or not the modal is
open. An open Overview draws each collection as it lands. The Overview itself
collects only when what was last collected is older than 15 minutes, after a
short debounce, or on refresh (`f5`, or `refresh` in its foot). A collection
holds `state/machine-usage.lock`, waiting at most two seconds (a child
process holds its descriptor between fork and exec): a second workspace
finding it held longer skips its turn. It runs one harness at a time:
1. inventory;
2. tokens and health;
3. sessions;
4. uses.

Each answer carries the collection's id and a stale one is dropped. Leaving
the route or closing the modal cancels the collection; a panic answers "not
known".

The result is kept in `cache/machine-overview.json`, named in `UzeHome`,
so the next opening draws at once. Being in `cache/`, it has no shape: if it
cannot be read, it is collected again. It holds only names, kinds, paths of
definition files, origins, figures and statuses. No CLI command, the shim or
a launch calls any of this.

What a harness's deleted records held is moved, per record, to
`state/machine-usage.json` (`usage::Archive`, D13): a record with a shape,
since it is the only place those uses are left. A record the harness lists
again is taken out of it and read live. The archive is written before the
cache, so a crash in between archives a record twice under one name rather
than losing it.

### 7. The screen

The screen has a header with one field, `‹ every harness ›`, and a summary
that counts the sessions and says how many days of records they span. There is
no period field (D12): every figure covers all the records a harness keeps.
Below it are three columns:

- a menu of sections: uze's findings while there are any, then every kind,
  always present, each with its count and a `!` mark for what in it is
  broken or duplicated;
- the chosen section's table, with its own filter and the columns
  `name · from · per session · uses · status`; `uses` is blank for hooks and
  rules, `?` where unknown, `~` before an estimate;
- the fixed panel.

The keys are in the foot. `✓ ok` is drawn only where a fact on disk was
checked, and `—` where nothing could be. The `uze` section reuses
`actionable_alerts`. `Feature::Overview` is removed when the screen is
complete. The design-system skill governs every detail.

### 8. Diagrams

`docs/architecture/core-components.mmd` gains the machine inventory
component and its relation to `IntegrationPort`.

### 9. Uses, from the records the harnesses keep

Uses are read from each harness's session records, never counted by a hook.
A hook would cost every agent's turn (2–5 ms per use, synchronous: Codex's
`async` lost calls, Antigravity has none), need delivery and upkeep in four
configurations, start empty on the day uze is installed, and miss what the
person types in two harnesses. The records cost nothing on the turn, already
hold the past, and carry the typed uses. Decision D11.

The integration only parses; the core owns the rest:

- **Records.** `usage_records(since)` lists, from metadata, the records of
  every session the harness keeps, subagents' included.
- **Reading.** The core reads a record again only when its stamp changed, and
  an append-only one from the byte offset where it stopped (`Resume::At`);
  one rewritten as a whole is read from the start (`Resume::Start`) and its
  earlier uses are replaced. Lines are filtered by a plain substring before any
  is parsed, and the collection's cancel is checked between records.
- **A use** is a kind, a target (a name, an MCP server, or the path of a
  definition file the core joins to the inventory), a time, who invoked it
  (person or model) and whether it is exact or inferred.
- **Kept.** `cache/machine-usage.json` holds, per record, its stamp, its
  resume point and its uses as daily counts per target. Losing it costs one
  fresh reading.

| | records | a use, typed by the person | a use, chosen by the model | resume |
|---|---|---|---|---|
| Claude Code | `projects/**/*.jsonl` | `<command-name>/<name>` (skill, plugin skill, MCP prompt); `agent_mention` | `tool_use` `Skill` / `Agent` / `mcp__<server>__<tool>` (a plugin's as `mcp__plugin_<plugin>_<server>__`) | offset |
| Codex | `sessions/**/rollout-*.jsonl` | `<skill><name>…<path>` | the `exec` script's read of `…/<dir>/SKILL.md` when its output shows it succeeded; `spawn_agent` `agent_type`; `mcp__<server>__<tool>` in the script, as an estimate | offset |
| OpenCode | every session, asked of `opencode api session.list` | `skills[]` / `agents[]` of a user message | tool `skill` `id`, `subagent` `agent`, `<key>_<tool>` matched against the configured keys | start (`session export`) |
| Antigravity | `brain/*/.system_generated/logs/transcript.jsonl` | `[Slash Command]` with `<SKILL>…(<name>)` | `read_file`/`view_file` of an absolute `SKILL.md` path, or a `run_command` reading it, not an `ERROR` step; `invoke_subagent` `TypeName`; `call_mcp_tool` `ServerName` | offset |

One use counts once: within a message, a mention and the call it led to are
one use, and an agent counts by its dispatch, never by its start and stop
events as well. Codex's `rN/` aliases are resolved against its skill roots;
one that cannot be resolved does not count.

## Candidate ADRs

- **The machine overview reads harness files but never writes them**: it
  sets the line between observing and managing foreign resources, which a
  later adopt or remove would have to cross deliberately.

## Risks / Trade-offs

- [A harness release moves a config file] → each inventory reader has a Lab
  check against the latest release. Until a reader is fixed, that harness
  reads unknown and the others are unaffected.
- [`opencode stats` costs a process spawn] → it runs only with the Overview
  in front, off every frame, with a timeout, at most once per collection.
- [A harness release changes its record's shape] → that harness's uses read
  unknown, never zero, so nothing turns `unused` by mistake; each reader has a
  Lab experiment against the latest release.
- [The first reading is large: 562 MB of Claude records in 30 days on the
  author's machine, about 2.4 s cold] → off every frame, per harness, after
  the inventory has drawn, cancellable; the next readings take only what was
  appended.
- [Codex's MCP uses are inferred] → shown with `~`.
- [`opencode session export` is a process per session] → only sessions whose
  stamp changed are exported, with a timeout.
- [Estimated tokens differ from the vendor's count] → always `~`; the panel
  calls them an estimate.
