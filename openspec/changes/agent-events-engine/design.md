## Context

See proposal.md for why. What exists today and shapes the approach:

- **The journal** (`uze-workspace/src/reports.rs`): one bounded file per
  agent, appended under a lock by `uze agent report`, which first proves
  the claim (the agent's launch key, checked against UZE's state). The
  client reads it from a cursor and turns lines into `agent.*` events
  (`src/ui/orchestrator/declared/bridge.rs`). This half is sound and stays.
- **The writers are per harness**: `IntegrationPort::reporter` returns a
  launch contribution. Claude Code: a per-session `--settings` of hooks on
  six events. Codex: `-c notify=…`, turn end only. OpenCode, Antigravity:
  none. `reported_events` says which; the client fills the rest from the
  pane.
- **Portable hooks** (ADR-040) already reach all four harnesses natively:
  `PreToolUse`, `PostToolUse`, `Stop`, `SessionStart`, proven in the Lab.
  The compiled wrapper normalizes each harness's payload into
  `HOOK_EVENT`, `HOOK_TOOL` (portable alias), `HOOK_TOOL_NATIVE`,
  `HOOK_PATH`, `HOOK_COMMAND`, `HOOK_QUERY`, `HOOK_CWD`.
- **The rules engine** (`uze-integrations/src/rules.rs`) is the precedent
  for UZE's own machine-level hooks: delivered by `uze setup` into each
  harness's own configuration, receipt-owned (`IntegrationOwned`,
  `SETUP_OWNER`), with nothing branching on which harness it is.
- **Codex** runs a user hook only once the person trusted it in Codex's own
  review (`codex/trust.rs`); UZE reads that record and never writes it.

## Goals / Non-Goals

**Goals:**
- One delivery for agent reports on every supported harness, with nothing
  in it written for one harness.
- An extension contract whose agent events are true on every harness.
- The workspace's own agent state (sidebar, chime, who is waiting) from the
  same events, not from a second source.

**Non-Goals:**
- What a tool ran (a shell command, a search) reaching extensions. The
  reports carry the tool and the path the workspace needs; the rest waits
  for a use case that justifies exposing it.
- A subagent's name (`agent.spawn` has no portable field today).
- Agents not launched by the workspace.

## Decisions

### D1. UZE's reports are a hooks manifest of its own, delivered like a plugin's

UZE keeps a `hooks.json` of its own (four groups: `SessionStart`,
`PreToolUse` and `PostToolUse` on every tool, `Stop`), compiled by the
portable hooks compiler and attached by `uze setup` through each harness's
existing hook target, as integration-owned receipts beside the rules
engine's.

*Alternatives.* **Per-launch injection**, as Claude has now: only Claude
takes hooks for one session from the command line, so it stays per
harness. **A `sh` engine writing the journal itself**, like the rules
engine answers the harness: it would skip the claim's proof and the
journal's lock and bounds, which `uze agent report` owns. **One script with
the rules engine**: same events, different jobs; one failing must not
silence the other.

### D2. The handler starts no UZE process outside the workspace

Each handler is a shell line that ends at once unless the process carries
the launch of an agent the workspace started (the variable the workspace
already sets for the claim), and only then runs `uze agent report
<event>`. A harness used by hand pays a shell test per hook, not a UZE
start. The handler has a POSIX and a PowerShell spelling, as every
portable handler does.

### D3. `uze agent report` reads the portable fields, not a payload

The report takes the tool from `HOOK_TOOL` / `HOOK_TOOL_NATIVE` and the
path from `HOOK_PATH`, kept only inside the checkout as today. Parsing each
harness's JSON payload (`--hook-input`, `described_by`) goes: the
wrapper already did that per harness. Nothing of `HOOK_COMMAND` or
`HOOK_QUERY` is written (non-goal).

### D4. Reported events and contract events differ on purpose

The journal holds what the hooks give: `session.started`, `tool.started`,
`tool.finished`, `turn.finished`. Extensions get `session.started`,
`turn.started`, `turn.finished`, `waiting`, `waiting.ended`. Tool events
stay inside the workspace: they are how it derives the rest, and exposing
them would promise the per-tool detail this change leaves out.
`approval.asked` and `approval.resolved` leave the contract; an extension
naming one fails to compile with the replacement in the message.

### D5. `waiting` is a tool open while the pane is quiet

An agent is waiting when its last `tool.started` has no `tool.finished`
and its pane has painted nothing for a short threshold (about 3 s, set by
the Lab's measurements). It ends at that tool's `tool.finished` or the
turn's end. This covers a permission prompt on every harness that runs
`PreToolUse` before asking, which the Lab must confirm per harness; a long
tool that keeps printing is not waiting.

### D6. `turn.started` is the first sign after a turn's end

No portable hook marks a prompt submitted. The first report after a
`turn.finished` (or after `session.started`), or the pane starting to work,
whichever comes first, starts the turn, once.

### D7. The Lab decides what enters the contract

The first task measures, per harness, in a real session: the four hooks
fire for a workspace-launched agent; `PreToolUse` fires before a
permission prompt; `HOOK_PATH` is set for an edit. An event a harness
cannot give is either derived for all (D5, D6) or stays out, and the
contract check joins `conformance/contract/` so a harness release that
drops one turns the Lab red.

### D8. The per-harness reporters go from the port

`IntegrationPort::reporter` and `reported_events`, Claude's
`runtime::reporter`, Codex's `notify` contribution and the
`ReporterContext` they share are removed; the client stops asking which
events a harness reports, since it is the same set for all. The pane
fallback stays for an agent whose report has not arrived (Codex before the
person trusts UZE's hooks, a harness whose hooks are unsupported on a
platform).

### D9. radar is one bar item

Its drawer of events goes. The item says one agent waiting first, then how
many work and the longest turn, then idle; a click runs `focus-agent` on
the waiting agent, else the longest-working one. The script keeps who works
and who waits from `turn.*` and `waiting*`.

## Candidate ADRs

- **UZE's own agent reports are a portable hooks manifest**: it fixes where
  agent observability comes from for every future harness, and moves a
  vertical's obligation from "write a reporter" to "deliver portable
  hooks", which is costly to move back.

## Risks / Trade-offs

- [Codex holds UZE's hooks until the person trusts them] → `uze setup` and
  `uze doctor` say so with the review to open; until then that agent's
  turns come from its pane, as today.
- [UZE's entries live in the operator's hook configuration] → receipts as
  the rules engine's, inspected before any change, removed by UZE's setup
  removal, never touching the operator's own entries.
- [A UZE start per tool call in a workspace agent] → the same cost Claude
  agents pay today; measured in the Lab run and reported if over the
  per-command budget.
- [`waiting`'s threshold misjudges a quiet tool] → it says "waiting" for a
  silent long tool at worst; the threshold comes from measurement, and the
  event ends the moment the tool does.
- [Extensions naming removed events] → only this repository's examples and
  gallery do today; each moves in this change.

## Migration Plan

`uze setup` (and the setup a new release runs) delivers UZE's hooks; the
per-session `--settings` and `notify` stop being passed at the next launch.
An existing journal keeps its old lines; the client ignores events it no
longer turns into contract events. Rollback is the previous release, whose
launch reporters work without the new hooks; the receipts let it remove
them.

## Open Questions

- The exact `waiting` threshold, from the Lab's measurements.

## Review before resuming

An independent review against the code (2026-10-28) found, most severe
first. Each needs answering before tasks are written.

1. **Identity.** The hook wrapper runs handlers in the package root
   (`hooks/wrapper/posix.rs`, `cd "$PLUGIN_ROOT"`), so a claim made with
   the process's directory is refused: the report must claim with
   `HOOK_CWD`. OpenCode serves every launch from one background server
   that keeps the first launch's environment (`opencode/agent_context.rs`):
   identity there must come from the session id, as agent context does. A
   harness the agent starts inside its pane inherits the launch variables;
   its `Stop` would read as the parent's turn end. Whether the variables
   reach Codex's and Antigravity's hook processes is unmeasured.
2. **Coverage per field and per turn.** The Lab proves the four events
   for a package's hooks, not under a workspace launch. Codex binds no
   `file.*` alias, so no path there; `HOOK_QUERY` is complete only on
   OpenCode. `SessionStart` does not fire on resume for OpenCode and
   Antigravity; `Stop` misses interrupts (Claude) and failed executions
   (OpenCode): the pane must stay co-authority for a turn's end, with a
   timeout. Tool inputs over the wrapper's bound are not exported.
3. **Delivery.** Antigravity has no rules-engine target; its hooks live
   in a generated plugin. Codex asks the person to trust UZE's hooks in
   hand-run sessions too. The gate runs after the wrapper's `sh`/`jq`
   start, in every session on the machine: measure that cost on Linux
   and Windows.
4. **ADR-040.** Keeping `uze agent report` on the hook path reverses the
   ADR's amendment; the claim proves little since any process in the
   pane holds the key. Preferred: the handler appends one line to a
   private per-agent spool, the client validates, bounds and rotates it.
5. **`waiting`.** No portable call id exists: count open tools per agent
   instead of pairing calls. Measure that `PreToolUse` precedes the native
   permission prompt on all four, and calibrate "pane quiet". Journals
   are read today only while an observing extension exists.
6. **Activity detail (the audit amendment).** Forgeable and floodable by
   the agent: call it activity, not audit. Keep command text host-only in
   a first version; journal at 0600; mask by allow-list. If it ever
   reaches extensions, a separate grant that refuses network, exec, MCP,
   prompt and global-store grants.
7. **Housekeeping.** Journeys 19 and 24 use the removed events; the
   `extension-platform` change's bridge spec requires them; fixtures
   (`bridge/chimes`, `flightdeck`), `uze-script` bridge fixtures, the
   extension reference and the author skill name them.
