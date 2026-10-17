## Context

See proposal.md (Why). The current state:

- **Project context.**
  - Claude gets `AGENTS.md` through a generated `CLAUDE.md` in
    `runtime/projects/<id>/claude-code/`, passed with `--add-dir` and
    `CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1`.
  - That variable is set only when the project has portable context
    (`claude/runtime.rs:50`, `:205`).
  - Codex, OpenCode and Antigravity read `AGENTS.md` natively.
  - Both Codex and Claude carry it inside a **user-role** message: Codex
    as `<user_instructions>`, Claude as a system-reminder in the user
    message. "Invisible" therefore cannot mean "not user-role". It means
    not in the pane, not in the turn the operator typed, and not a user
    message in the harness's conversation view.
- **Briefing.**
  - `switch_harness` renders `briefing.rs` (12 KiB bound) into
    `runtime/projects/<id>/briefings/<agent>.md`.
  - It records `PendingBriefing{integration, path}` on the
    `ConversationRecord`.
  - `continuity::plan` takes that entry once and appends
    `first_turn_args`.
  - `LaunchPlan` carries `args`, `trailing` and `note`, and no
    environment (`continuity.rs:36-47`). The shim merges arguments only
    (`shim.rs:173-178`).
- **Launch-scoped hooks** were measured by `track-touched-repositories`
  (`experiments/<vendor>/launch-hook.py`, 2026-10-10):
  - Claude: `--settings` merges a hook group with the user's own.
  - Codex: `-c 'hooks.<Event>=[…]'` plus `-c 'hooks.state={…trusted_hash…}'`
    gives and trusts a hook for one process. Any `-c` makes the TUI run
    embedded rather than on the shared daemon, so the launch environment
    reaches the handler.
  - Antigravity: `--add-dir <dir>` with `<dir>/.agents/hooks.json` loads
    the hook for that process only. It is gated on the server-side
    `json-hooks-enabled` flag.
  - OpenCode: a per-launch plugin holds only with `--standalone`, because
    the shared server keeps the first launch's plugins and environment.
- **Measured 2026-10-17.** Every channel in D2 reached the model, showed
  nothing in the pane, held on resume and was current there, on all four
  harnesses (decisions.md M1). Claude and Codex keep the earlier copy
  beside the new one on a resume.
- **What each harness offers for context**, as researched before the
  measurement:
  - **Claude:** `CLAUDE.md` content is recomputed per request and not
    stored in the transcript.
  - **Codex:**
    - `SessionStart` fires on `startup` and `resume`.
    - Plain stdout becomes a developer item.
    - A context-only success is not drawn as a TUI history cell
      (`tui/src/history_cell/hook_cell.rs`), but it *is* recorded in the
      conversation (`core/src/hook_runtime.rs:852-867`), so every resume
      adds a copy.
    - Output over about 10,000 bytes (2,500 tokens × 4 bytes) spills to a
      file the model is pointed at.
    - JSON-looking stdout is read as a hook result.
  - **OpenCode v2:**
    - `session.hook("context", e => e.system.push(...))` runs on every
      request.
    - `SessionRequest` carries `sessionID`
      (`packages/plugin/src/promise/session.ts:25-31`).
  - **Antigravity:**
    - A `PreInvocation` hook may return `ephemeralMessage` or
      `injectSteps[].systemMessage`.
    - Whether either is rendered is undocumented.

## Goals / Non-Goals

**Goals:**
- The briefing, and later memory, reach the model the way `AGENTS.md`
  does, on every harness that can, measured rather than assumed.
- No text in argv or environment. No channel acts on the operator's own
  sessions.
- A harness that cannot do it is reported, not silently skipped.

**Non-Goals:**
- Memory itself (`portable-agent-memory`).
- Context for the operator's own invocations.
- Changing what the briefing says, beyond its bound.

## Decisions

### D1. The context is sections, composed in `continuity::plan`, bounded at 9,000 bytes

- **Where it is composed.** In the launch `continuity::plan` already
  owns. That is the place that knows the launch is UZE's, which agent it
  is, and which harness.
- **What it is.** A fixed preamble followed by sections in a fixed order:
  - briefing, here;
  - memory index, later.
- **When nothing applies.** No section means no file and no contribution.
- **Path.** `runtime/projects/<id>/agents/<agent>/context.md`, generated
  and owner-only, rewritten on each launch.
- **Bound.**
  - 9,000 bytes in total, under Codex's 10,000-byte spill.
  - The shares add up to the bound exactly:
    - the preamble, at most 500 bytes;
    - the briefing, 5,000;
    - a 3,500-byte share reserved for memory, which
      `portable-agent-memory` divides between its instruction and its
      index.
  - Each section cuts itself by its own rule. The briefing's `render`
    already drops sections from the end.
  - This makes switches lossier than the 12 KiB bound of today.
- **Wording.** It is written as facts ("This agent was moved from Claude
  to Codex. Its goal was …"), not imperatives. Claude's hook docs warn
  that imperative injected text trips its injection defences, and facts
  stay true when given again.
- **Never opening with `{` or `[`.** That is enforced by the preamble,
  because Codex reads JSON-looking stdout as a hook result.
- **Frozen for the life of the launch.** The context is composed once,
  and no channel refreshes it mid-session, even one that runs per request
  (OpenCode's `context` hook reads a file nothing rewrites until the next
  launch).
  - A context that stays the same through a conversation is part of the
    cached prompt prefix every vendor bills at a fraction of the price.
    One that changes per turn is paid in full each time.
  - The cost is that a fact recorded mid-session reaches other agents at
    their next launch. An agent that needs the current state asks for it.
  - Sections carry indexes, never bodies. A body is fetched on demand, so
    the fixed per-launch cost stays at the index.

### D2. Each integration declares a channel; the shim carries a path, never text

`IntegrationPort::context_channel()` returns one of the variants below. The
default is `FirstTurn`.

- **`Directory`: Claude.**
  - The integration writes a `CLAUDE.md` into the agent's context
    directory, holding the context inline.
  - It is inline, not an `@import`, because an import outside the working
    directory raises Claude's one-time approval dialog.
  - The plan adds `--add-dir <agent dir>` and always sets
    `CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1`, whether or not the
    project has portable context.
  - Re-read on every launch: a resume gets the current one. Claude also
    records each copy in its transcript and replays it (decisions.md M1),
    so the channel declares `accumulates`.
  - Fallback if the Lab disagrees: a `SessionStart` hook given through
    `--settings`. Its docs say it re-fires on `resume`; it is capped at
    10,000 characters.
- **`LaunchHook`: Codex and Antigravity.**
  - A hook given for this process only, whose handler prints the
    composed file named **by absolute path in the hook's command**.
    Nothing is read from the environment, so a nested harness the agent
    starts inherits nothing.
  - The handler runs in the platform's shell: `sh -c` (`cat`), or
    PowerShell (`Get-Content -Raw`), through `uze-platform`.
  - Codex: `-c 'hooks.SessionStart=[{hooks=[{type="command",command=…}]}]'`
    plus `-c 'hooks.state={"/<session-flags>/config.toml:session_start:0:0"={trusted_hash=…}}'`.
    The hash is `codex/trust.rs::identity_hash` of that exact handler.
  - Codex keeps each launch's copy, so the channel declares
    `accumulates`.
  - Claude's `Directory` measured the same: on a resume the old copy is
    replayed and the new one is added, with Claude's own "replaces its
    earlier copy" note. So it declares `accumulates` too.
  - Antigravity: `--add-dir <agent dir>` with
    `<agent dir>/.agents/hooks.json`, a `PreInvocation` handler returning
    `injectSteps[].systemMessage` once per launch.
    - The shim writes a fresh `hooks.json` for every launch, naming a
      launch nonce. The handler prints the context only while
      `<agent dir>/delivered-<nonce>` is absent, then creates it, so
      later model calls in that process print nothing.
    - A nonce, not a pid, so a reused pid can never suppress a delivery.
      Composition sweeps old markers.
    - The handler is conditional, so it has a `sh` and a PowerShell
      template, chosen through `uze-platform`.
    - One copy per launch stays in history, so the channel declares
      `accumulates`, like Claude and Codex.
    - `ephemeralMessage` was measured and rejected: it is re-sent on every
      model call, outside the cached prefix (decisions.md M1).
- **`SessionPlugin`: OpenCode.**
  - A UZE-owned plugin, `plugins/uze-context.ts`, delivered by `uze setup`
    and receipt-owned like every other attachment.
  - Its `session.hook("context")` reads
    `<sessionID>.md` in a machine-wide directory. One shared server
    serves every project, so the directory is not per project.
  - The directory is named in `UzeHome` parameterised by the
    integration's id, as `runtime_projection_dir(integration, …)` is, so
    `uze-core` names no vendor.
  - The shim rewrites the file on every launch of that session.
    `conversation::forget` removes it with the agent's conversations.
  - The shim places the agent's composed context there under the session
    id it assigned. OpenCode's continuity is `Assigned`, so the id is
    known before launch.
  - No file means nothing pushed, so the operator's own sessions,
    subagent child sessions and `/new` sessions get nothing.
  - No environment is involved, so the shared server is no obstacle.
- **`FirstTurn`.**
  - Today's pointed-at first turn, for the briefing only, given once.
  - Declared degraded.
  - Never used for memory.

Precedence follows the project's: `Directory` and `SessionPlugin` are
native; `LaunchHook` is generated native; `FirstTurn` is safe adaptation.

### D3. Codex: UZE trusts its own launch hook, for that process only

- `codex/trust.rs` says UZE reads Codex's trust record and never writes
  it.
- The session-layer `hooks.state` writes nothing: it trusts one fixed
  handler for one process, and the person's `config.toml` is untouched.
  The operator installed UZE, and this is UZE's own handler.
- `track-touched-repositories` reaches the same recommendation for its
  file-trail hook. Both changes rest on one decision, recorded once.
- `--dangerously-bypass-hook-trust` is rejected, because it trusts every
  hook of the invocation, the person's untrusted ones included.

### D4. The briefing lives in the continuity record until the work moves on

- **Why it becomes a record.**
  - In `switch-agent-harness` D3 the briefing was generated tier, because
    the receiving transcript held it once read.
  - Here it is given again on every launch on its harness until the
    agent commits, and `SessionPlugin` keeps no copy in the conversation
    at all.
  - Deleting `runtime/` would lose it while the record still points at
    it. So it is a record now.
- **What the entry holds.** The pending entry becomes
  `Briefing{integration, text, head_at_switch}`, holding the rendered
  text, at most 5,000 bytes.
- **Shape.** The continuity record takes a shape step. Its one ladder
  rung drops an older build's `{integration, path}` entry: the generated
  file it names is not carried across, the same as today's floor for a
  generated file.
- **When it retires.**
  - On the evaluation pass that already reads HEAD (work naming from the
    first commit), once HEAD has moved past `head_at_switch` on the
    agent's branch.
  - Or when a later switch replaces it.
- **The promise it narrows.** `agent-session-continuity` says the record
  holds no conversation text. It now holds the briefing and nothing else,
  which the delta states openly.

### D5. The switch launches with no turn

- On every channel except `FirstTurn`, `switch_harness` no longer adds a
  first turn.
- The operator chose the switch and is at the keyboard. An auto-submitted
  "carry on" was the visible prompt, and it started work nobody asked
  for.
- The operator's next turn, even "continue", is answered by a model that
  already holds the briefing.

### D6. The shim carries the plan's environment

- `LaunchPlan` gains `env: Vec<(OsString, OsString)>`, which the shim
  sets on the harness.
- Today only Claude's `CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1`
  needs it. A nested `claude` the agent starts inherits the flag but not
  the `--add-dir`, so it loads nothing extra. The shim does not clear it.
- Channels avoid environment by design (D2), so a nested launch inherits
  no context path.

### D7. The Lab proves four outcomes, and records where the context landed

- **Contract.** `conformance/contract/agent_context.py`, vendor-neutral:
  1. *reaches the model*: a marker appears in the first request.
  2. *not shown*: absent from the pane, from the content item holding the
     typed prompt, and from user messages in the harness's own
     conversation view.
  3. *holds on resume*: the first request after a resume in a new process
     carries it.
  4. *current on resume*: a section changed between launches is the
     newest copy the model receives.
- **Where the marker lands.** Providers additionally record *where* it
  lands (Anthropic `system` vs a user content block; OpenAI `instructions`
  vs developer vs user input items; Gemini `systemInstruction` vs
  `contents`). That is evidence for the run and not a pass criterion,
  since `AGENTS.md` itself lands in user-role parts on two harnesses.
- **Briefing checks.** The switch's `briefing-first-turn-*` checks become
  `briefing-reaches-the-model` on this contract, plus
  `briefing-first-turn` for a harness declared `FirstTurn`.

## Candidate ADRs

- **An agent context channel per harness.** It adds a port declaration
  and a second invisible delivery path beside `AGENTS.md`, and every
  future harness must answer it.
- **UZE trusts its own launch-scoped Codex hooks.** This is shared with
  `track-touched-repositories`. It is an exception to `codex/trust.rs`'s
  "never answers the review".

## Risks / Trade-offs

- **[Claude and Codex keep a copy per launch (measured).]** A long-lived
  agent resumed often carries several copies until it compacts.
  - Mitigation: the briefing retires at the first commit, and the
    contract asserts that the newest copy is the current one.
- **[Antigravity hooks depend on the server-side `json-hooks-enabled`
  flag, which is on only when signed in.]**
  - The flag is server-side and invisible before launch. UZE treats
    "signed in" (the OAuth token the integration already detects) as the
    flag's proxy, which the Lab measured as on.
  - When not signed in, status reports the channel unavailable and the
    switch falls back to `FirstTurn` for the briefing, so a switch never
    loses it silently.
  - `workspacePaths[0]` stays on the project (measured).
- **[OpenCode v2's `session.hook` is new API.]**
  - No version pin exists today.
  - The channel's Lab vertical runs nightly and is what catches a break.
- **[The visible turn is gone, so an operator expecting the agent to
  resume work on its own will see it wait.]** Intended (D5), and stated
  in the switch's toast.

## Migration Plan

- Archive `switch-agent-harness` first, so its specs exist for these
  deltas.
- The continuity record's shape step carries older records across,
  dropping a pending path entry.
- Rollback: declare every channel `FirstTurn`, which is today's
  behaviour, with the briefing text served from the record.

## Diagram updates

- `docs/architecture/agent-lifecycle.mmd`: the switch edge's briefing
  becomes "kept in the record, given through the agent context until the
  first commit".
- `docs/architecture/workspace-components.mmd`: an `agent context`
  component composed by `continuity` and fed by `briefing`, read by the
  shim.
