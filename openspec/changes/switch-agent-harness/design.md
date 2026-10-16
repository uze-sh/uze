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
- Switching automatically when a limit is hit. The limit offers the
  switch, and the operator makes it.
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
  is still `from` (compare-and-set). Without that check, two clients both
  switching would each mint a key; the second key would win, and the
  first client's process would launch as an ordinary invocation with
  nothing recorded.
- It answers with the agent's current directory (`own_directory` or the
  root), plus a fresh key from `keyed()`.
- It builds the briefing (D6) and records the pending entry, all on the
  placement thread. Git goes through `uze-git`.

The UI sends `PlacementRequest::Switch { primary, agent, harness }` with
`replacing: Some(tab)`. `absorb_placement` opens the new tab before
closing the old one, so occupancy never sees the slot empty.

### D5. `Agent.harness` means "the harness of the latest launch"

Same type, and old values are still valid, so there is no shape bump; the
doc comment changes. Every placement sets it, including `Resume`, which
fixes the existing drift. As a result:

- The recent list resumes the current harness.
- The sidebar names the current harness.
- New subagents inherit the current harness.
- Existing children keep theirs, which only affects what they display.

The history of harnesses is the key set of `ConversationRecord.harnesses`,
which is not stored a second time.

### D6. Briefing contents and bounds

A new port verb `conversation_digest(session, cwd) -> Option<Digest>`
returns neutral data, `Digest { goal, summary, turns: Vec<Turn>,
last_command, limit }`, read in a single pass of the transcript. The
default is `None`. The `uze-workspace` module that writes the briefing
renders it, together with the Git facts, into at most about 12 KB. When
cutting, it cuts the lowest priority first:

1. **Header:** label, branch, base, the source harness and session id,
   and why the switch happened (the limit and its reset time, if known).
2. **Goal:** the first operator turn, up to 1.5 KB.
3. **Summary:** the vendor's latest compaction summary, up to 4 KB.
   Claude: a `user` record with `isCompactSummary`. Codex: the top-level
   `compacted` record.
4. **Operator turns:** every turn after the summary, newest kept first,
   each up to 400 B.
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

When the target harness already has an entry in the conversation record,
the digest is limited to turns after that entry's `observed_at_unix` (or
its `launched_at_unix`), and the commits to those after it. The header
says "since you left".

### D8. The limit signal is part of the switch (the one point the reviewers split on)

The architecture review put transcript reading in a second delivery. The
mechanics review wanted limit detection first, because it is the
motivating case. Both come from the same read, so the limit is in this
change:

- `Digest.limit: Option<Limit { resets_at }>`, from Claude's synthetic
  `assistant` record (`error: "rate_limit"`, `quotaLimits.resetsAt`) and
  from Codex's `token_count.rate_limits` (`rate_limit_reached_type`,
  `resets_at`).
- The client asks for it off-frame, on the same refresh pass that already
  reads conversations back, never on render.
- The tab shows a limit mark, a new `theme` symbol and token, and the menu
  lists the switch first.

Splitting delivery is kept in `tasks.md` instead: the Git-only switch
lands and is usable before the transcript work.

### D9. The menu

The menu lists one item per detected harness other than the current one,
"Continue in <display name>", with names taken from `agent_options()`.
It is offered only where `agent_for_tab` resolves, which means the
harness process is alive. Choosing a harness without continuity shows the
`continuity_gap` toast the picker already shows. If the tab is `Working`,
the switch asks for confirmation inline, because relaunching ends the
running turn.

### Diagram

`docs/architecture/agent-lifecycle.mmd` gains a `Running -> switched to
another harness -> briefing written -> Started through its shim` edge. No
container or component is added. `workspace-components.mmd` is checked
for the shim and continuity relationship and updated if it names the
launch plan.

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
- **[Limit read cost]** Reading the transcript tail on each refresh pass
  for every agent costs I/O. Mitigation: read only the file's tail, and
  only while the tab is not `Working`.
