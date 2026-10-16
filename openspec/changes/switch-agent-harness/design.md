## Context

Most of what a switch needs is already in the code:

- **Conversations per harness.** `ConversationRecord.harnesses`
  (`crates/uze-workspace/src/conversation.rs`) is one document per agent,
  keyed by integration id. `launched()` touches only its own harness's
  entry, and `claimed_by_others` filters by integration, so two harnesses
  of one agent never see each other's conversation.
- **Relaunch in place.** **To worktree** (`isolate_agent`,
  `src/ui/orchestrator/session/agents.rs`) is already place, open the new
  tab, close the one it replaces (`replacing`). It runs off-frame through
  `spawn_agent_placement` (`src/ui/orchestrator/reads.rs`).
- **A new key per placement.** Every placement mints a fresh launch key
  (`keyed()`, `crates/uze-application/src/application/services/tasks/placement.rs`).
  Once a switch is placed, the old process's claim no longer matches, so
  its `refresh` writes nothing.
- **Identity reaches the shim** through `UZE_AGENT` and `UZE_AGENT_KEY`
  (`crates/uze-terminal/src/launch.rs`). The terminal runtime keeps a tab's
  command and environment and replays them when it restores the tab.
- **The shim plans continuity only for a bare invocation.**
  `continuity::plan` runs only when `original_args.is_empty()`
  (`src/shim.rs`). `LaunchPlan.args` are prepended to the arguments.
- **The integrations already find their transcripts** (Claude
  `~/.claude/projects/<slug>/<id>.jsonl`, Codex rollouts), but read only
  the id and the mtime.
- **`Agent.harness`** is documented as "recorded at launch". That is
  already untrue: `PlacementRequest::Resume` carries no harness, so
  resuming a preserved agent on another harness leaves the old one
  recorded.
- **"Handoff" is taken.** It is a `worktree-policy` completion behaviour,
  so this change calls its document a **briefing**.

The plan was reviewed by two agents, one on architecture and domain and
one on harness mechanics and product. Where they agreed, the decisions
below follow them. The one point where they differed is noted in D8.

## Goals / Non-Goals

**Goals:**
- Continue one agent on another harness without losing its identity,
  checkout, branch, label or any harness's conversation.
- The incoming harness starts knowing the goal and the state of the work.
- Nothing about the switch replays on a restore, and nothing about it
  blocks a launch.
- No vendor name outside `uze-integrations`.

**Non-Goals:**
- Converting one vendor's transcript into another vendor's session
  format. Each harness keeps its own conversation; the briefing is text.
- Anything about usage limits: detecting one, showing it, offering the
  switch because of it (D8).
- Switching a tab whose agent has exited. Resuming from the recent list,
  with a harness picked, already covers that case, and it now records the
  harness it picks (D5).
- Transcript reading for OpenCode and Antigravity. They keep the default
  until someone looks into their stores.

## Decisions

### D1. The first turn is composed by the shim, from a pending entry on the record

The client cannot put the prompt in the tab's command, for two reasons:
the shim would skip continuity, because the invocation would carry
arguments, and the terminal runtime would replay the prompt on every
restore. Instead:

- `switch_harness` writes the briefing, then records a pending briefing
  on the agent's `ConversationRecord`: the target integration and the
  path. It is an additive optional field, so there is no shape bump.
- `continuity::plan`, inside the `conversation::update` lock it already
  takes, gives the pending briefing only when the launching integration
  matches. It clears the entry in the same write that records the launch.
- `LaunchPlan` gains `trailing: Vec<OsString>`. The shim appends it after
  the integration's contributed arguments, because a positional prompt
  has to come last and Codex's `resume <id>` is a subcommand.

A restore launches with the entry already cleared, so it simply resumes.
A launch carrying the operator's own arguments never reaches `plan`, so
it neither gives nor clears the entry. Alternatives considered: a client
env var (replayed on every restore), and typing the prompt into the pane
with `submit_notice` (timing depends on when the harness is ready to take
input). The second stays the fallback only if a harness ever needs one;
all four take a prompt argument today.

### D2. The first turn is a short user turn that points at a file

The turn is one line: "You are continuing work another agent started in
this checkout. Read `<abs path>` first, then check `git log` and
`git status`, and carry on." It is a user turn, not a system prompt:

- A user turn stays in the vendor's transcript, so a later resume still
  has it.
- Claude's `--append-system-prompt-file` and Codex's
  `-c developer_instructions=…` apply to one process only, and the Codex
  key overrides whatever the operator configured.
- The briefing body is never argv, which `/proc/*/cmdline` exposes.

Per harness, verified against the installed `--help`: Claude
`[--resume <id>] "<line>"`; Codex `["resume" <id>] "<line>"`; OpenCode
`[--session <id>] --prompt "<line>"`; Antigravity
`[--conversation <id>] -i "<line>"`. The port verb is
`first_turn_args(&str) -> Vec<OsString>`, defaulting to none.

### D3. The briefing is generated-tier, one per agent, owner-only

By AGENTS.md's test, deleting the briefing costs at most one briefing that
has not yet been read. Once read, the receiving harness's transcript holds
what it read. The briefing therefore goes under `runtime/`, at a new
`UzeHome::briefing_path(project_id, agent)`, written owner-only the way
`keep_gate_output` writes its output. Each switch overwrites the previous
briefing rather than accumulating them. It is removed with the agent's
conversations in `conversation::forget`. The pending entry from D1 is the
only part that is a record.

This narrows an existing promise. `agent-session-continuity` said UZE
records no conversation content. It now says the *record* holds none, and
that the briefing is the one place UZE writes conversation text. The
delta spec states this openly rather than leaving it implied.

### D4. A dedicated placement, not `resume_task`

`resume_task` acquires a slot when the checkout is missing, and it
reconciles and revives shelved agents. None of that is a switch. Instead,
`Workspace::switch_harness(primary, agent, from, to)`:

- It sets `Agent.harness = to` under `task::locked`, only if the harness
  is still `from` (compare-and-set), and issues the launch key in the
  same write. Without the check, two clients both switching would each
  mint a key; the second key would win, and the first client's process
  would launch as an ordinary invocation with nothing recorded. In a
  separate write, a key that failed to issue would leave the record
  naming a harness no launch was coming for.
- It answers with the agent's current directory (its slot or the root).
- It builds the briefing (D6) and records the pending entry, all on the
  placement thread. Git goes through `uze-git`. The outgoing harness's
  conversation is taken from `continuity::conversation_now`, which
  observes one not read back yet the way `continuity::plan` does, so an
  agent switched moments after it started is still briefed from its
  transcript.

The UI sends `PlacementRequest::Switch { from, agent, was, to }` with
`replacing: Some(tab)`. `absorb_placement` opens the new tab before
closing the old one, so occupancy never sees the slot empty.

### D5. `Agent.harness` means "the harness of the latest launch"

Same type, and old values are still valid, so there is no shape bump; the
doc comment changes. Every placement that chose a harness sets it in the
write that issues its key (`keyed(placed, Some(harness))`), including
`Resume`, which fixes the existing drift. As a result:

- The recent list resumes the current harness.
- The sidebar names the current harness.
- New subagents inherit the current harness.
- Existing children keep theirs, which only affects what they display.

The history of harnesses is the key set of `ConversationRecord.harnesses`,
which is not stored a second time.

### D6. Briefing contents and bounds

A new port verb `conversation_digest(session, cwd, since_unix) ->
Option<ConversationDigest>` returns neutral data from `uze_core::session`,
`ConversationDigest { goal, summary, turns: Vec<Turn>, last_command }`, read in a single pass of the transcript. The
default is `None`. The `uze-workspace` module that writes the briefing
renders it, together with the Git facts, into at most about 12 KB. When
cutting, it cuts the lowest priority first:

1. **Header:** label, branch, base, the source harness and session id,
   and that the outgoing conversation is kept.
2. **Goal:** the first operator turn, up to 1.5 KB.
3. **Summary:** the vendor's latest compaction summary, up to 4 KB.
   Claude: a `user` record with `isCompactSummary`. Codex: the top-level
   `compacted` record.
4. **Operator turns:** every turn after the summary, newest kept first,
   each up to 400 B. The one already shown as the goal is recognized by
   its text, not its position: a digest floored at a time keeps only
   later turns.
5. **Assistant text:** the last three messages, up to 3 KB.
6. **Last command:** the last command run and whether it failed (command
   line only).
7. **Git:** commit subjects since the base, `diff --stat` and dirty files.
   Git answers "which files", so the briefing only points at them.

Parsing rules live only in `uze-integrations`:

- **Claude:** operator turns are `user` records with string content. Skip
  `tool_result`, `isSidechain` and `isMeta`. From assistant records, take
  `text` blocks only.
- **Codex:** read `event_msg/item_completed` items of type `UserMessage`,
  `AgentMessage` and `CommandExecution`. Never read `response_item`
  messages, which include the injected `AGENTS.md`.

Any parse failure yields `None`, and the briefing falls back to the Git
facts alone.

### D7. Switching back sends a delta

A switch marks the outgoing harness's entry with `left_at_unix`, an
additive field. When the target harness has a conversation and that mark,
the digest is limited to turns after it, and the commits to those after
it; the header says "since you left". Neither `launched_at_unix` nor
`observed_at_unix` can stand in: a resume records no launch, so after a
full cycle both are older than the work the returning harness missed.

### D8. Usage limits are out of this change

The mechanics review found both harnesses record a usage limit in their
transcripts (Claude's synthetic `rate_limit` record with
`quotaLimits.resetsAt`, Codex's `token_count.rate_limits`), and the first
implementation showed it on the tab and listed the switch first. The
operator took it out: limits are a subject of their own, with more to
decide than this change should. The digest reads neither; Claude's
`isApiErrorMessage` records are only skipped, since nobody said them.

### D9. The menu

The menu has one "Continue in…" entry, `Action::SwitchHarness`, that
opens a submenu of the set-up harnesses other than the current one, from
the client's `AgentIdentity` list (no detection on the input path). A group
rather than an entry per harness keeps the menu one length however many
harnesses a machine has, and is the first of the action groups the menu
can grow (decisions.md D1). `Alt+.` (`Action::OpenMenu`, the key the code
surface opens a row's actions with, under the modifier the pane leaves
free) opens the selected agent's menu; Up and Down move, Right or Enter
open a group, Left or Escape close it, Enter takes an entry. The pointer
opens a group by hovering or clicking its row.
It is offered only where `agent_for_tab` resolves, which means the
harness process is alive. Choosing a harness without continuity shows the
`continuity_gap` toast the picker already shows. If the tab is `Working`,
the switch asks first in a dialog over the whole client, the way the
approval of a project's commands is asked, because relaunching ends the
running turn.

### Diagram

`docs/architecture/agent-lifecycle.mmd` gains a `Running -> continued in
another harness -> briefing written -> Started through its shim` edge.
`workspace-components.mmd` gains the `briefing` component and its three
relationships. No container is added.

## Candidate ADRs

- **The briefing is the one place UZE writes conversation text.** This
  narrows a privacy promise of `agent-session-continuity`, and a later
  feature would cite it as precedent.
- **Reading vendor transcripts for content, not just identity.** It ties
  UZE to private, unversioned formats. The Lab is the only guard.

## Risks / Trade-offs

- **[Private transcript formats drift]** Neither the Claude JSONL nor the
  Codex rollout is an API, and Codex already changed from `user_message`
  events to `item_completed`. Mitigation: failure is always `None`, which
  falls back to Git, and a Lab contract check replays a recorded
  transcript fixture per harness so drift shows up as a red vertical.
- **[Old process in the overlap window]** Between placement and the old
  tab closing, the old process's key is already rejected, so
  `uze agent work name` fails there and a late clear or fork is not
  recorded. The window is a fraction of a second, and the conversation
  entry it would have written belongs to the harness being left.
- **[A briefing the agent ignores]** The first turn asks the agent to
  read the file; nothing can make it. Git is in the checkout regardless.
