## Context

See proposal.md (Why). This change builds on `deliver-agent-context`. Its
composed per-agent context, byte bound, channels and degraded reporting are
taken as given here. This design covers only the store, the verbs, the
acceptance of machine facts and the index section.

What exists that this reuses:

- **Front matter.** `uze-core` already reads YAML front matter with
  `capability::skill::split_frontmatter` and `noyalib`'s serde, for
  agents and harness blocks.
- **Writing.** `UzeHome`'s one way of writing is atomic replace, and the
  cross-process lock lives in `uze-platform`.
- **Launch identity.** The shim exports a launched agent's identity
  (agent key, task, harness) to the harness process, the same identity
  `uze agent work name` uses to know who is asking.
- **Questions to the operator.** The workspace already asks the operator
  questions as toasts that stay until answered.

## Goals / Non-Goals

**Goals:**
- One store, auditable as text, shared by every harness the workspace
  launches.
- A machine-wide fact is never one prompt injection away from every
  project.
- An operator can find, read and delete any fact without being an agent.

**Non-Goals:**
- Writing into, syncing with or redirecting a harness's memory. UZE only
  reads it as a source, and only where it is text with a known place
  (D6).
- Search, embeddings, consolidation, or UZE generating memories from
  transcripts.
- An MCP server.
- Promoting a project fact into `AGENTS.md` for the team. Worth doing
  later: today project memory is the operator's private notes beside the
  team's file.

## Decisions

### D1. A fact is a Markdown document; its tier is record, its handling is authored-like

- **Layout.**
  - Machine scope: `state/memory/<name>.md`.
  - Proposed machine facts: `state/memory/proposed/<name>.md`.
  - Project scope: `state/projects/<id>/memory/<name>.md`.
  - All three are named in `UzeHome`.
- **`<id>` is the project's record id, never a checkout's.**
  - It is `project_id_for` of the primary checkout's canonical root, the
    same directory that already holds the project's `agents.json` and
    conversations.
  - Every worktree under `.worktrees/` and every subagent checkout
    therefore reads and writes one memory.
  - Runtime projections are keyed per checkout and are not used here.
  - Since the id is a hash of the path, moving or renaming the repository
    leaves its memory behind under the old id, the same as every other
    project record today. A second clone of the same repository is
    another project. Re-keying records by something that survives a move
    is a question for all project records, not for memory alone.
- **Front matter.** `shape`, `name`, `description`, `type`, `created`,
  `updated`, and `source` (`{harness, task}` or `operator`).
- **Record, because deleting one costs what nothing else knows.** So it
  carries a shape, read from front matter before anything else. A missing
  `shape` is shape 1.
- **But a person edits it like an authored file.** So the persisted-state
  rules apply this way:
  - A fact whose front matter does not parse is reported and never
    rewritten. There is no set-aside, because the text is the operator's.
  - A fact declaring a newer shape is not taken (the ladder's direction
    rule) and is reported the same way.
  - A shape-1 fact is the only shape; there is no ladder step yet.
- **Reading and writing.** Reading uses `split_frontmatter` plus
  `noyalib`'s serde. Writing renders the fixed fields in a fixed order and
  appends the body untouched. No YAML editing is needed, so the `cst`
  containment stays where it is.
- **No hand-kept index.** The index is computed from front matter when a
  context is composed. Claude's model-maintained `MEMORY.md` drifts, and a
  computed one cannot.
- **Alternatives rejected.**
  - SQLite would be a record the operator cannot read.
  - Protobuf is opaque with nothing to gain at this scale.
  - A flat `key: value` parser of our own would be a second front matter
    reader in a crate that already has one.

### D2. Verbs and provenance

- **The verbs.**
  - `remember <name> --type <t> --description <d> [-m]` takes the body
    from stdin and upserts by name.
  - `forget`, `list` and `show` complete the set.
  - All four are `Budgeted` in `command_performance`: one small file read
    or write under a lock.
- **Provenance.**
  - It comes from the launch identity. Without one it is `operator`.
  - An agent can unset or forge its environment, so provenance is shown as
    evidence and never grants anything. The acceptance in D3 is decided by
    whether the invocation carries a launch identity. Forging "operator"
    means *removing* it, and that is a step an injected agent is unlikely
    to take blind. The risk is residual and is stated.
- **Sandboxes.** The Codex `workspace-write` sandbox may refuse writes
  under `~/.uze/state`. `uze agent work name` already writes UZE state from
  inside agents. Task 5.2 measures whether memory writes hold on each
  harness's default sandbox; where they do not, the verb fails saying so.
  It is never silent.

### D3. An agent's machine fact is a proposal the workspace asks about

- **Why.** A launched agent's `--machine` write lands in `proposed/`, and
  no index reads `proposed/`. The context channel puts agent-written text
  into a position the model weighs more than a user turn. One repository
  that injects an agent must not reach every other project.
- **Asking.** The workspace asks in a toast that stays until answered,
  naming the fact, its description and the proposing agent:
  - Accept moves the file to `state/memory/`.
  - Discard deletes it.
- **When nobody answers.** A proposal no client is open to ask about waits.
  Status lists it.
- **Project facts are not gated.** They reach only the project the agent
  was in, which is also where the injecting content lives. The operator
  reads them in status.
- **Alternatives rejected.**
  - Writing machine facts only from the operator's own invocation loses
    the useful case: an agent noticing a preference.
  - Reusing `uze workspace allow` conflates approving commands with
    approving text.
- **Outside the workspace.** `uze status` lists each proposal with its
  path under `state/memory/proposed/`. Moving the file into
  `state/memory/` accepts it, and deleting it discards it. No person-facing
  verb is added.
- **Noticing a new proposal.** The client reads the proposals on the same
  periodic evaluation pass that already re-reads its tasks, on the read
  thread. A proposal is noticed within one pass, with no new watcher.

### D4. The index section and its ranking

- **Placement.** The section is placed after the fixed preamble the
  channel change defines. It holds one line per fact:
  `- <name> (<type>): <description>`, then a line saying how to read a
  body (`uze agent memory show <name>`).
- **Share.** Memory takes the 3,500 bytes `deliver-agent-context`
  reserves:
  - the write instruction (non-Claude agents only), at most 700 bytes;
  - the index, with the Codex summary entry counted inside it, the
    remaining 2,800, or 3,500 for a Claude agent.
- **Per harness.** Each harness's own entries are left out of its own
  agents' index, because Claude loads its own memory and Codex injects its
  own summary.
- **Ranking.** `feedback` and `user` facts first (standing guidance, which
  rarely changes and so would lose a recency race), then `project` and
  `reference`, most recently updated first. Past the share, a count of
  what was left out.
- **Name collisions.** On a collision the project fact wins.

### D5. `memory.harness` turns harness memory off at UZE launches only

- `config.toml` gains `memory.harness = "keep" | "off"`, default `keep`.
  It is authored, and an unknown value reads as the default.
- With `off`, the integration adds its own switch to UZE's launches. Today
  that is Codex: `-c features.memories=false`, which is already embedded
  by UZE's `-c agents=`. This applies to the launch only. Codex's existing
  summary stays readable as a source for other agents.
- Claude is excluded, because its memory is the only way Claude agents
  write (D6).
- OpenCode has nothing to turn off. Antigravity has no documented switch
  and is reported as such.

### D6. Harness memory is read live as a source; UZE's store holds what no harness keeps

- **Why not import or redirect.**
  - A one-shot import copies Claude's facts and lets the two copies
    diverge.
  - Redirecting Claude's `autoMemoryDirectory` into UZE's store for UZE
    launches would split Claude from itself: what an agent learned inside
    the workspace would vanish for a `claude` started outside it.
  - Claude keeps writing where it always does, and UZE reads it.
- **Where it reads.** `~/.claude/projects/<slug>/memory/*.md` on every
  composition. The slug is Claude's encoding of a cwd, with `/` replaced
  by `-`.
  - It reads the slug of the primary checkout's root and the slugs of its
    checkouts under `.worktrees/`.
  - Recent Claude versions keep a worktree's memory under the repository's
    slug. Older ones left per-worktree directories, and about a hundred
    exist on the author's machine.
  - The cost is front matter of a few dozen small files per launch.
- **Reading.**
  - `type` is read from the top level or from `metadata.type`, since both
    shapes exist.
  - `metadata.modified` is read as `updated`.
  - `MEMORY.md` is skipped, because UZE computes its own index.
  - The provenance is `claude`.
  - The body is never copied. The index line names the file's path, and
    the agent reads it on demand like any body.
- **Scope.** Claude's facts are project facts in the index. Its `user`
  facts are kept per project by Claude itself, so they stay there. A fact
  the operator wants everywhere is remembered at machine scope through
  UZE.
- **One write instruction per model.**
  - Claude's system prompt carries its own memory instruction, which
    outweighs anything in `AGENTS.md`.
  - So `uze agent memory` is documented in the agent context, per
    harness, and never in the projected `AGENTS.md` region.
  - Claude agents get the index and no UZE write instruction.
  - Codex, OpenCode and Antigravity agents get both. Codex's own memory
    is written by a background job from finished conversations, not by
    the model, so a Codex agent has no competing write instruction even
    when that memory is on.
- **Accepted limit.**
  - A Claude started outside the workspace does not see what Codex,
    OpenCode or Antigravity agents remembered.
  - Closing that gap means writing into Claude's directory and its
    model-maintained `MEMORY.md`, which is copies again.
  - Memory across harnesses is a workspace feature, and Claude on its own
    keeps all of its own.
- **`memory.harness = off` never touches Claude.** Claude's memory is a
  source and the only way Claude agents write.

**The other harnesses** (read from Codex HEAD source and from disk on
2026-10-17):

- **Codex: a live source, as one entry.**
  - When its memory is on, Codex keeps it as Markdown under
    `~/.codex/memories/`:
    - `memory_summary.md`, the consolidated narrative Codex injects at its
      own session start;
    - `MEMORY.md`;
    - `rollout_summaries/*.md`.
  - `memories_1.sqlite` drives the pipeline and also holds per-thread
    `raw_memory` text.
  - All of it is readable, but it is prose consolidated across every
    project on the machine. Codex rewrites the folder on every
    consolidation, and the layout is mid-migration (`memories_v2`).
  - So it is neither parsed into facts nor copied. When
    `memory_summary.md` exists, the index of every non-Codex agent gets
    one machine-scope entry naming it and its size, and the agent reads it
    on demand.
  - Codex agents get no entry, because Codex already injects that summary
    itself.
  - The entry depends on one path and nothing inside the file.
  - Distilling it into UZE facts stays available on request, as an
    ordinary agent task: the agent reads the folder and uses `remember`.
    Its facts carry the agent's own launch provenance, and the
    machine-scope gate (D3) applies. No special provenance or verb is
    added.
- **Antigravity: no source yet.**
  - `conversation_summaries.db` is readable SQLite, but it is a list of
    conversations (title, preview, workspace, status), not memory.
  - `implicit/*.pb` are protobufs of about 400 bytes with no readable
    text.
  - Knowledge items, under `knowledge/`, are what would be memory. None
    exist on the author's machine, and their format is undocumented.
  - Revisit when one can be measured.
- **OpenCode.** It keeps no memory of its own, so there is nothing to
  read.

### D7. One memory service, every consumer; each harness answers as a source

- **`uze-core`.**
  - `memory` holds UZE's store and a vendor-neutral `MemoryEntry`: name,
    type, description, scope, provenance, path, and whether it is
    writable.
  - Every entry has the same shape, whether it is a UZE fact or a
    harness's.
- **`IntegrationPort::memory_source(project_root)`.**
  - Returns the entries a harness keeps for that project and the machine.
    The default is none.
  - It is one more method on the one trait, which AGENTS.md keeps unsplit.
  - Claude's slug reading and Codex's summary path live in
    `uze-integrations` and nowhere else, so core and application stay
    vendor-neutral, as the identity tests require.
  - Entries from a source are always read-only.
- **`uze-application::MemoryService`.** The only surface every consumer
  uses:
  - `list(scope)`: UZE facts and source entries, merged, UZE winning a
    shared name.
  - `show(entry)`: the body, read from either place.
  - `remember` and `forget`: UZE facts only.
  - `accept` and `discard`: proposals.
  - Warnings: unreadable facts, and harnesses without a channel.
  - Its read model is what `uze agent memory`, `uze status` and the
    workspace client's proposal question (`src/ui`, on the orchestrator's
    read thread) consume.
- **Who builds the index section.** `uze-application` depends on
  `uze-workspace`, never the reverse.
  - The application's launch path asks `MemoryService` for the entries
    and hands them to `continuity::plan` as data: a rendered section with
    its share.
  - `uze-workspace` composes sections and knows nothing of memory.
  - The same holds for the briefing, which the workspace already owns.
- **A management screen is a later consumer, not a new API.**
  - A future per-space screen listing the index, opening bodies, and
    accepting proposals or deleting UZE facts reads the same read model.
    That is the reason `src/` reaches the domain only through
    `uze-application`.
  - Its reads go through `orchestrator/reads.rs` like any other disk read
    the client makes.

## Candidate ADRs

- **Agent memory is a UZE record delivered through the launch.** It
  reverses the deferred Memory category and commits UZE to owning operator
  data, in a format, at two scopes.

## Risks / Trade-offs

- **[A poisoned project fact persists in that project.]**
  - Mitigation: status lists every fact with provenance; the agent-context
    memory instruction calls facts notes, not instructions; the machine
    scope is gated (D3).
  - Residual: a project fact reaches later agents in the same project
    until the operator removes it.
- **[An agent strips its launch identity to write machine scope
  directly.]**
  - Residual, stated in the agent-context memory instruction and in the ADR. The gate
    raises the bar; it is not a sandbox.
- **[Two memories disagree while `memory.harness = keep`.]**
  - Accepted as the default, because turning off a vendor feature is the
    operator's call.
  - `status` says the harness's own memory is on.
- **[The index costs bytes on every launch.]**
  - Mitigation: only the index is sent, never bodies, within a fixed
    share.

## Migration Plan

Nothing to migrate: no memory exists before this change. Rollback removes
the section from composition. The facts stay on disk as the operator's
records.
