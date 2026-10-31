## Context

See proposal.md for why; the requirements are in `specs/machine-overview/spec.md`.

What exists today and shapes the approach:

- `Route::Overview` is the first route, gated by `Feature::Overview`
  (`crates/uze-core/src/machine/features.rs`), whose comment says what the
  screen should be is undecided. `src/ui/view/overview.rs` draws two
  counts.
- `IntegrationPort` already has optional readers of a harness's own records
  whose default is "not known": `conversation_digest` (Claude Code, Codex),
  `memory_source`, `session_recorded_for`. This change follows that
  pattern.
- `shared/transcript.rs::records` streams JSONL from the start of a file,
  through `lines()`, into a `serde_json::Value` per line. There is no
  offset, and invalid UTF-8 ends the read.
- OpenCode's sessions are asked of `opencode api` (`opencode/session.rs`).
  Antigravity's per-conversation databases are never opened
  (`antigravity/session.rs`). Neither stance changes.
- uze never touches foreign hooks, plugins or order (delivery model). A
  read-only Overview keeps that.
- Every background read answers through a channel, a late answer is
  dropped, and a panicking read answers "not known"
  (`docs/architecture/invariants.md`). Cache is never required for
  correctness.
- Observed on a real machine:
  - `~/.claude/projects` is 756 MB, with files up to 83 MB and lines over
    1 MB. Of 248 transcripts, 195 are `<session>/subagents/*.jsonl`.
  - 4 of 9 Codex rollouts are subagent threads
    (`session_meta.payload.parent_thread_id`).

## Goals / Non-Goals

**Goals:**
- One harness-neutral model; a new harness appears by implementing the
  optional methods and nothing else.
- No cost on any path that is not the Overview in front: launch, shim, CLI,
  any other screen, any frame.
- A screen that draws at once and never claims more than it read.

**Non-Goals:**
- Adopt, executing any removal, starting any MCP server.
- OpenCode and Antigravity uses. They report inventory and sessions, and
  their uses stay unknown.
- Per-project resources, a CLI report, an exact per-model tokenizer.

## Decisions

### 1. Three optional `IntegrationPort` methods; the core owns the rest

| method | answers | default |
|---|---|---|
| `machine_inventory()` | found resources: kind, name, location, origin hint (harness plugin, built-in, account, by hand), text listed every session, text loaded when used, a problem the harness reports, and the harness's own removal command when there is one | empty |
| `session_sources(since)` | root session records (never subagents') with last-activity time, plus the child records that hold uses | `None`, so sessions are unknown |
| `read_usage(source, resume_at, budget, inventory)` | `(events, resume_at, reached_end)`; each event is already resolved to an inventory resource id, harness, day and completeness (`Complete` or `Floor`); unresolved names are dropped inside the integration | `None`, so uses are unknown |

The core owns, independent of harness:
- `uze` ownership from receipts;
- merging across harnesses;
- token estimation;
- status, order, floors, period and totals;
- the index;
- scheduling.

The removal command is a field of a found resource, not a fourth method.
`src/` reaches all of it through a `uze-application` read model, never
`uze_core` (layering).

### 2. Tokens: a local estimator over what the harness lists

The core estimates tokens from text length, with a per-script ratio
calibrated by a test, and always shows them with `~`. For each resource,
the integration supplies the text its harness lists:
- a skill's or agent's frontmatter name and description;
- an always-on file whole;
- an MCP server's tool names, as the harness lists them. Claude Code defers
  MCP tools, and its latest transcript's `deferred_tools_delta.addedNames`
  carries them, so they are read from there; otherwise the figure is
  unknown;
- for a hook, the context its last runs injected
  (`hook_additional_context`), otherwise zero.

Schemas are never fetched, because that would start servers. A real BPE
tokenizer was rejected: it is a dependency with MBs of tables, and still
not each vendor's tokenizer. The ranking needs only an estimate.

### 3. What each harness gives, in this change

**Claude Code**
- Inventory:
  - skills and agents under `~/.claude`;
  - plugins from `plugins/installed_plugins.json`;
  - `settings.json` hooks;
  - MCP from `~/.claude.json` and plugins;
  - `CLAUDE.md`.
- Broken: `mcp-needs-auth-cache.json` and `failedMcpServers`.
- Sessions: `projects/*/*.jsonl` roots only.
- Uses, all `Complete`, read from roots and `subagents/`:
  - skills: `Skill` tool uses, `<command-name>` slash invocations and `invoked_skills` attachments, deduplicated per turn;
  - agents: `Agent` uses with a `subagent_type`;
  - MCP: `mcp__<server>__<tool>`, resolved by longest normalized server prefix;
  - hooks: `hook_success` matched to a configured hook by event and command, stored by inventory id.

**Codex**
- Inventory:
  - skills in `~/.agents/skills` and `~/.codex/skills`;
  - agents;
  - `[plugins.*]`;
  - `hooks.json`;
  - `config.toml` MCP;
  - `~/.codex/AGENTS.md`.
- Sessions: rollouts without `parent_thread_id`.
- Uses:
  - agents: `spawn_agent.agent_type`, `Complete`;
  - skills: injected `<skill><name>` blocks, `Floor` (explicit invocations only);
  - MCP and hooks: unknown until a Lab fixture proves their record shape.

**OpenCode**
- Inventory: skills, agents, plugins, `opencode.json` MCP, the global `AGENTS.md`.
- Sessions: the API listing, following its cursor, reading `time.updated`, run from a directory outside every project. Unknown unless the Lab proves the listing is machine-wide.
- Uses: unknown.

**Antigravity**
- Inventory:
  - skills, `built-in` skills, agents, hooks, plugins, MCP under `~/.gemini`;
  - the global instructions file, wherever the Lab shows agy reads it.
- Sessions: `conversations/*.db` counted by the newest modification time of the file or its `-wal`, none opened.
- Uses: unknown.

A rule's uses are its harness's sessions.

### 4. Scheduling

A pass starts only while the Overview is the route in front, after a short
debounce, if the index is older than a minute, or on refresh (`f5`). It is
cancelled when another route is chosen or the modal closes. The steps are:

1. inventory and tokens;
2. sessions;
3. uses.

Uses are read one source at a time with a byte budget per step, on one
low-priority thread, checking the cancel flag between steps. Each answer
carries the pass id and a stale one is dropped. A panic answers "not
known". Only one pass runs at a time. A second TUI client finds the lock
taken and draws from the index. No CLI command, the shim or a launch calls
any of this.

### 5. The reader

`shared::transcript` gains a resumable reader:
- it seeks to a byte offset;
- it reads raw bytes up to the next newline, skipping a line longer than a
  cap (4 MB) without holding it;
- it treats a non-UTF-8 line as one skipped line;
- it reports the end of the last complete line as the resume point, since
  a half-written tail is left for the next pass;
- it filters by substring (`"tool_use"`, `command-name`, `hook_`,
  `invoked_skills`, `spawn_agent`, `<skill>`) before parsing a line.

Memory stays bounded by the cap whatever the file size.

### 6. The index

`UzeHome` names `cache/machine-usage.json`. For each source it holds:
- path, size, modification time, root or child;
- the resume point;
- complete;
- counts as `day → (resource id, completeness) → n`.

Counts are kept per source, so a source that shrank or whose time went back
drops its own counts and is read again from zero. Days older than 90 are
dropped. The file is written as a whole snapshot with
`persistence::write_atomic`. Being in `cache/` it has no shape: unreadable
means rebuild. "Unused" requires every source of the period to be
`complete`. Only inventory ids reach it.

### 7. The screen

The screen is Marketplace's shape:
- header fields `‹ every harness ›` and `‹ 30 days ›`;
- the filter;
- one tree folded by kind;
- the fixed panel;
- keys in the foot.

Floors read `≥ n`. The `uze` group reuses `actionable_alerts`.
`Feature::Overview` is removed when complete. The design-system skill
governs every detail.

### 8. Diagrams

`docs/architecture/core-components.mmd` gains the machine inventory
component and its relation to `IntegrationPort`.

## Candidate ADRs

- **The machine overview reads harness records but never writes them**: it
  sets the line between observing and managing foreign resources, which a
  later adopt or remove would have to cross deliberately.

## Risks / Trade-offs

- [A harness release changes its record format] → Lab fixtures from the
  latest release per reader; `harness-watch` flags drift. Until a reader is
  fixed, that harness reads unknown and the others are unaffected.
- [A cold pass over hundreds of MB] → it is bounded, cancellable,
  low-priority and resumable, and has a benchmark budget (task 5.3). Uses
  read as being read, and never as unused, until it finishes.
- [Hook runs may be recorded only for some hooks] → the Lab proves whether
  a silent run is recorded. If not, hook uses are a floor.
- [Estimated tokens differ from the vendor's count] → always `~`, and the
  panel calls them an estimate.
