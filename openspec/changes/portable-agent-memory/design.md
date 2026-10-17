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
- Syncing with a harness's memory, or importing from any store but
  Claude's. Codex's is private SQLite and off by default, and
  Antigravity's is protobuf. Claude's is the only one that is readable
  text, and it is imported once, on request (D6).
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

### D4. The index section and its ranking

- **Placement.** The section is placed after the fixed preamble the
  channel change defines. It holds one line per fact:
  `- <name> (<type>): <description>`, then a line saying how to read a
  body (`uze agent memory show <name>`).
- **Share.** 3,000 bytes of the context's bound.
- **Ranking.** `feedback` and `user` facts first (standing guidance, which
  rarely changes and so would lose a recency race), then `project` and
  `reference`, most recently updated first. Past the share, a count of
  what was left out.
- **Name collisions.** On a collision the project fact wins.

### D5. `memory.harness` turns harness memory off at UZE launches only

- `config.toml` gains `memory.harness = "keep" | "off"`, default `keep`.
  It is authored, and an unknown value reads as the default.
- With `off`, the integration adds its own switch to UZE's launches:
  - Claude: `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`.
  - Codex: `-c features.memories=false`. Codex is already embedded by
    UZE's `-c agents=`.
- OpenCode has nothing to turn off. Antigravity has no documented switch
  and is reported as such.

### D6. Importing Claude's memory once, on request

- **Why import.** Claude's memory is already one Markdown file per fact
  with `name`, `description` and a type from the same four, so the mapping
  is close to the identity. Without it the feature starts empty for anyone
  who already uses Claude, and `memory.harness = off` would cut them off
  from what they have.
- **Where it reads.** `~/.claude/projects/<slug>/memory/*.md`. The slug is
  Claude's encoding of a cwd, with `/` replaced by `-`.
  - The import reads the slug of the primary checkout's root and the slugs
    of its checkouts under `.worktrees/`.
  - Recent Claude versions keep a worktree's memory under the repository's
    slug. Older ones left per-worktree directories, and about a hundred
    exist on the author's machine.
- **Reading.**
  - `type` is read from the top level or from `metadata.type`, since both
    shapes exist.
  - Claude's `metadata.modified` becomes `updated` when present.
  - `MEMORY.md` is skipped, because UZE computes its own index.
  - Bodies are kept verbatim, `[[links]]` included.
- **Where facts land.**
  - Every fact lands at project scope with `source: claude-import`.
  - `user` and `feedback` facts are also offered as proposed machine
    facts, which the operator accepts or discards through D3's question.
    The import cannot tell which of them hold only for that project.
- **Re-running.** It upserts by name and skips a fact whose `updated` is
  later than its import date. That date is recorded in the fact as
  `imported`, so an edit made in UZE is never overwritten.
- **No sync.** Two memories written in parallel would diverge, and UZE
  would carry Claude's format changes forever. Status counts the Claude
  facts not yet in UZE, so the operator knows when to ask for the import
  again.
- **Who runs it.** It is an agent verb, not a person's, keeping memory's
  one audience: the operator asks their agent to import.

**The other harnesses** (read from Codex HEAD source on 2026-10-17, and
from disk):

- **Codex.**
  - Its consolidated memory is Markdown under `~/.codex/memories/`:
    - `memory_summary.md`, the narrative injected at session start;
    - `MEMORY.md`;
    - `rollout_summaries/*.md`;
    - `skills/`.
  - The SQLite database only drives the pipeline.
  - The text is prose consolidated across every project on the machine,
    not one fact per file, and its layout is mid-migration
    (`memories_v2`, `consolidation_v2.md`). A deterministic parser would
    split prose badly and break at the next layout.
  - So the import is **agent-assisted**. The projected region tells an
    agent asked to import Codex's memory to read that folder, keep what is
    durable, and write each fact with `remember`, using `source:
    codex-import` and choosing the scope.
  - The machine-scope gate (D3) applies unchanged.
  - No verb and no parser are added.
- **Antigravity.** Knowledge items under
  `~/.gemini/antigravity-cli/knowledge/` and `implicit/*.pb` are
  undocumented and opaque, and none exist on the author's machine to
  measure. Not imported. Revisit when the format is documented.
- **OpenCode.** Keeps no memory of its own, so there is nothing to
  import.

## Candidate ADRs

- **Agent memory is a UZE record delivered through the launch.** It
  reverses the deferred Memory category and commits UZE to owning operator
  data, in a format, at two scopes.

## Risks / Trade-offs

- **[A poisoned project fact persists in that project.]**
  - Mitigation: status lists every fact with provenance; the projected
    region calls facts notes, not instructions; the machine scope is gated
    (D3).
  - Residual: a project fact reaches later agents in the same project
    until the operator removes it.
- **[An agent strips its launch identity to write machine scope
  directly.]**
  - Residual, stated in the projected region and in the ADR. The gate
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
