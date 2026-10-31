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
- OpenCode's sessions are asked of the harness, never of its database.
  Antigravity's per-conversation databases are never opened. Neither
  stance changes.
- uze never touches foreign hooks, plugins or order (delivery model). A
  read-only Overview keeps that.
- Every background read answers through a channel, a late answer is
  dropped, a panicking read answers "not known", and cache is never
  required for correctness (`docs/architecture/invariants.md`).

Why only this scope: a review of the four harnesses' data showed that what
comes from disk and configuration is answerable by all four. What comes from
session history (per-resource uses, per-session tokens of MCP tools and
hooks, MCP sign-in state) is complete for one harness, partial for one, and
unreadable for two. That is left for a later version.

## Goals / Non-Goals

**Goals:**
- The same answers from all four harnesses; a new harness appears by
  implementing two optional methods.
- No cost on any path but the Overview in front: launch, shim, CLI, other
  screens, any frame.
- No session transcript read at all.

**Non-Goals:**
- Per-resource uses, "unused", per-session tokens of MCP servers and hooks,
  MCP sign-in state.
- Adopt; executing any removal; starting any MCP server.
- Per-project resources; a CLI report; an exact per-model tokenizer.

## Decisions

### 1. Two optional `IntegrationPort` methods; the core owns the rest

| method | answers | default |
|---|---|---|
| `machine_inventory()` | found resources: kind, name, the files that define it, origin hint (harness plugin, built-in, by hand), the command or path it runs (hooks, local MCP), the harness's own removal command when there is one | empty |
| `session_count(since)` | sessions with activity since a time, subagents excluded | `None`, so the count is unknown |

The core owns everything harness-neutral:
- `uze` ownership from receipts;
- merging what uze delivered across harnesses;
- duplicate detection (same kind and name, different content digest);
- token estimation from the files;
- the static health checks;
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
| OpenCode | `opencode stats --days N --json`, run from a directory outside every project with a short timeout | its own `sessions` / `subagents` split |
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

A collection starts only while the Overview is the route in front, after a
short debounce, if the snapshot is older than a minute, or on refresh
(`f5`). It runs on the management worker, one harness at a time:
1. inventory;
2. tokens and health;
3. sessions.

Each answer carries the collection's id and a stale one is dropped. Leaving
the route or closing the modal cancels the collection; a panic answers "not
known".

The result is kept in `cache/machine-overview.json`, named in `UzeHome`,
so the next opening draws at once. Being in `cache/`, it has no shape: if it
cannot be read, it is collected again. It holds only names, kinds, paths of
definition files, origins, figures and statuses. No CLI command, the shim or
a launch calls any of this.

### 7. The screen

The screen has a header with two fields, `‹ every harness ›` and `‹ 30 days ›`;
the period sets only the session count. Below it are three columns:

- a menu of sections: uze's findings while there are any, then every kind,
  always present, each with its count and a `!` mark for what in it is
  broken or duplicated;
- the chosen section's table, with its own filter and the columns
  `name · from · per session · status`;
- the fixed panel.

The keys are in the foot. `✓ ok` is drawn only where a fact on disk was
checked, and `—` where nothing could be. The `uze` section reuses
`actionable_alerts`. `Feature::Overview` is removed when the screen is
complete. The design-system skill governs every detail.

### 8. Diagrams

`docs/architecture/core-components.mmd` gains the machine inventory
component and its relation to `IntegrationPort`.

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
- [No usage data] → the screen cannot say what is unused; it says what
  weighs, what is broken and what is duplicated. Uses are a later version
  that adds a column without changing the layout.
- [Estimated tokens differ from the vendor's count] → always `~`; the panel
  calls them an estimate.
