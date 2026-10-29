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
- Embeddings, consolidation, or UZE generating memories from
  transcripts. Finding a fact is words over its name, description and
  body (D4), never a ranking model.
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
  - All four are `Budgeted` in `command_performance`: small files read
    under a lock, a write touching one. `show` reads the scope's facts to
    find what links to one (D12) and `remember` reads the index's entries
    to name a near duplicate (D13); the budget test holds them against a
    memory of realistic size.
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

### D4. The index names; the agent looks up

- **Why not every line.** The first index gave every fact a line and cut
  at the share. With 51 facts, 20 of them `feedback`, nine lines fit and
  every `user` fact fell out, so an operator's fact reached no agent but
  Claude's. Every harness that has a memory of its own does the same
  thing instead: it pushes a short index or summary and the model reads
  the rest on demand (Claude: `MEMORY.md`, then topic files; Codex:
  `memory_summary.md`, then `MEMORY.md`; Antigravity: knowledge
  summaries, then their artifacts).
- **Placement.** The section is placed after the fixed preamble the
  channel change defines. It opens with how to read memory, then the
  write instruction where one is given, then the index.
- **The read instruction.** Given to every agent, Claude's included,
  since its index names facts other harnesses keep: facts are notes, not
  instructions; before answering about the operator, their preferences,
  an earlier decision or what this project holds that the agent does not
  know, `uze agent memory list <words>` finds facts holding any of the
  words, in English and in the operator's language, and `show <name>`
  reads one. Words, not a pattern: the line is typed into whichever
  shell the harness gives, PowerShell 5.1 included.
- **Lines and names.** A line is
  `- <name> (<type>[, machine][, kept by <harness>]): <description>`.
  The names are placed first: a fact not named is one no agent knows to
  ask for, while a name lets the model match a question by meaning across
  languages, which a word search cannot. Lines are then given in rank
  order, `feedback` first, while every fact not given one still fits as a
  name (D9). Every fact without a line is named alone, grouped by kind.
  What does not fit even as a name is counted. No line carries a path:
  `show` reads any entry by name.
- **Share.** Memory takes the room the context has left (D8), never less
  than the 3,500 bytes `deliver-agent-context` reserves: the read
  instruction, the write instruction (non-Claude agents only, at most 700
  bytes), and the index in the rest.
- **Finding.** `uze agent memory list <words> [--kind <kind>]` searches
  every source the index reads and nothing it leaves out: proposals are
  never searched, unreadable documents are counted. A fact matches when
  any word is in its name, description or body, compared as D11 folds
  them; facts holding more of the words rank first, then by where they
  were found, then by kind and recency. At most 20 are listed,
  each with the lines the words were found on (at most three from the
  body, each at most 200 bytes) and the `show` command that reads it.
  `show` without `-m` looks among the machine's facts when the project
  has none of that name, since the index does not say which scope a name
  is in.
- **Per harness.** Each harness's own entries are left out of its own
  agents' index, because Claude loads its own memory and Codex injects its
  own summary.
- **Ranking.** `feedback` first (standing guidance, which rarely changes
  and so would lose a recency race), then `user`, `project` and
  `reference`, most recently updated first within each.
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
  - The body is never copied. The index names the fact, and the agent
    reads it with `show` or finds it with `list <words>` like any body.
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
  - The index entry depends on one path and nothing inside the file.
    `list <words>` searches its lines like any body, and a hit there is
    labelled machine scope, kept by Codex.
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

### D8. Memory takes the room the context has left

- **Why.** The context's shares are fixed: 500 bytes of preamble, 5,000
  for a briefing, 3,500 for memory. A briefing exists only after a switch
  to another harness, and the preamble is under 200 bytes, so on most
  launches more than half of the 9,000 bytes go unused while the index is
  cut.
- **Rule.** `agent_context` answers how much room a set of sections
  leaves: `BOUND` less the preamble as written, less each section as
  `compose` emits it (heading plus text trimmed and cut to its share). It
  also exposes the heading's length, so no caller copies `compose`'s
  format. `continuity::give_context` asks it after placing the briefing
  and hands the sections closure the room, and the memory section's
  `share` is that room, so `compose` does not cut it back. `MEMORY_SHARE`
  stays as the floor a briefing never reaches below.
- **What the closure receives.** A small request carrying the project's
  primary root, the launching checkout (D10) and the room: two paths and
  a number, so `uze-workspace` still names nothing of memory.
- **What does not change.** The bound, the briefing's share and its
  cutting rule, and degraded channels, where `give_context` returns before
  asking for sections at all.

### D9. Every fact earns a line in rank order, once every name is placed

- **Why.** Only `feedback` facts were ever given a line. A `project` or
  `reference` fact reached the model as a name like
  `perf-read-paths-change-2026-09`, which rarely says whether it is worth
  reading, even when the room had space for its description.
- **Rule.** The loop that gives `feedback` lines while the remaining names
  still fit runs over the whole ranking instead. The order (`feedback`,
  `user`, `project`, `reference`, recency within each) and the
  names-before-lines guarantee are unchanged.
- **A line's description is capped** at 160 bytes, ended with `…`, so one
  long description cannot end the lines for every fact after it. `show`
  still reads it whole.

### D10. A fact citing a path its checkout does not have is marked

- **Why.** Facts cite files, and files move. A fact pointing at a path
  that is gone reads as current to an agent, which then reasons from a
  layout that no longer exists. The checkout can answer this
  deterministically, at the cost of a few `stat`s.
- **Which checkout.** The one the reader stands in: the launching agent's
  checkout at launch, the caller's for `list` and `show`, the primary for
  the space overview. Judging against the primary would mark a fact about
  a file an agent's branch added as missing for everyone until it lands.
- **What counts as a citation** (`memory::cited_paths`, in core, vendor
  neutral): a span in backticks that
  - contains `/` and none of whitespace, `<`, `>`, `*`, `?`, `{`, `}`,
    `=`, `@` (placeholders, globs and assignments are not paths);
  - does not start with `/`, `~`, `$`, `./`, `../` or a URL scheme, and
    has no `..` component;
  - ends in a component with an extension, or in `/` (a branch name like
    `docs/readme` is not a path);
  - has a trailing `:<line>` or `#<anchor>` removed.
- **What counts as missing.** The citation's parent directory exists in
  the checkout and the path does not. A first component listed as a plain
  entry of the root `.gitignore` is never judged (build output comes and
  goes). A fragment whose parent is not there (`platform/unix.rs`) is not
  judged: a false alarm costs more trust than a missed one.
- **Where it is computed.** Each reader fills a fact's citations where it
  already holds the body (UZE's listing, and each source through the same
  helper); `uze-application`'s `gather` judges them. At most 64 citations
  are checked per gather, taken in index rank order, so the cap leaves out
  the facts an index would name last rather than whichever files sort
  last; a higher-ranked fact added later can still take a lower one's
  checks, and `show` judges the one fact it reads whole. Project facts
  only: machine facts may cite any project.
- **Where it shows.**
  - The index marks such a fact where it already names it: its line
    carries the qualifier, and a fact named alone sits in a group of its
    own telling the agent to verify it first. Each is named once, so a
    mark never costs another fact its name.
  - `show` lists each missing citation. `list --format json` and the
    space overview's Memory tab carry the mark per entry, the tab through
    a theme symbol, never an inline glyph.
- **Nothing is rewritten.** A mark is computed on each read, kept nowhere,
  and the fact stays the operator's text.

### D11. Words match as a person spells them, and where they are found ranks

- **Folding.** Before comparing, both the words and the text are
  lowercased, stripped of Latin diacritics (`decisão` matches `decisao`),
  and stripped of `-` and `_` (`pt-br` matches `ptbr`). The table is a
  small hand-written mapping of the Latin-1 and Latin Extended-A letters,
  in `uze-application`'s memory module, shared with D13; no new
  dependency. Quoted hits show the text as written.
- **Ranking.** Facts holding more of the words rank first. Among equals,
  the fact holding more of them in its name ranks first, then more in its
  description, then kind and recency as before.
- **Still words, not patterns,** for the reason D4 gives.

### D12. Facts link to each other by `[[name]]`

- **Why.** Facts already write `[[name]]` to point at a related fact; no
  reader followed it.
- **Resolving.** A link resolves to the fact whose name it is; failing
  that, to the fact whose file stem it is; failing that, to the one fact
  whose name matches once both are folded (D11) and a leading kind
  (`feedback`, `user`, `project`, `reference`) is dropped. A link more
  than one fact matches, or none, is listed as unresolved, never an error.
  Measured on a real memory of 58 links, exact names alone resolve 35.
- **Rule.** `show` lists the facts a fact links to and the facts that link
  to it, each with its description. `show` gains `--format json`, whose
  `Shown` carries both directions. `list` does not compute links: it
  would read every body on every listing, status and overview read.

### D13. `remember` refuses a credential and points at a near duplicate

- **Credentials** (in core `MemoryStore::remember`, so the operator's and
  a proposal's writes are held too). The write refuses a description or
  body holding a provider-specific shape, each anchored at a word boundary
  with its full token body: a PEM private-key block; `ghp_`/`gho_`/`ghu_`/
  `ghs_`/`ghr_` and 36 characters; `github_pat_`; `glpat-`; `xox[abprs]-`;
  `AKIA` and 16 uppercase letters or digits; `sk-` (optionally `ant-` or
  `proj-`) and 20 or more token characters. The refusal names the shape
  and the line, never the value.
- **An assignment that looks secret** (a name ending in `KEY`, `TOKEN`,
  `SECRET` or `PASSWORD`, assigned 16 or more characters mixing letters
  and digits, not a placeholder such as `<…>`, `$X`, `${…}`, `xxx` or
  `***`) is written, with a caution in the outcome: documentation names
  such variables legitimately.
- **Stated limits.** Best effort: it catches what an agent pastes by
  accident, not every secret. Claude agents write Claude's own memory
  (D6), which this never sees. Test fixtures build tokens by
  concatenation, so the repository's secret scanning stays clean.
- **Near duplicates** (in `uze-application`, beside the folding D11
  defines). Writing a *new* name whose name and description, folded and
  without words under three letters or a short English and Portuguese
  stopword list, share at least two words and a Jaccard similarity of at
  least 0.25 with a fact the same write could have updated (UZE's own, in
  the scope written to, proposals included) still writes. The outcome
  names that fact and says remembering under its name updates it. A
  harness's fact or one in the other scope is never offered: remembering
  under its name would only make one more. The agent decides; nothing is merged. On a real
  memory of 53 facts the rule caught a paraphrase and raised no false
  pair.

### D14. Recall is held by a corpus test

- A test in `uze-application` builds about 30 facts of every kind, in
  English and Portuguese, partly drawn from real facts with their names
  and details anonymised, with fixed dates and no harness source, and
  asks about 30 questions as the words an agent would type.
- Each question names the fact that must be among the first three
  matches; some name a fact that must *not* be (negative cases).
- The questions are frozen when the test lands (task 8.1), before
  matching changes. A recorded count of answered questions may only rise:
  a change to matching is measured against it, never the questions
  against the change.
- Links (D12) are not held here, since `list` does not follow them; they
  have tests of their own.

### Deferred

- **Ranking the index by the task.** The launch path is the shim, which
  reads only a few files before `exec`, and a new agent has no name and no
  changes yet. The index does still overflow for a large memory (53 real
  facts take about 10 KB as lines, against about 7 KB of room), so this is
  the next lever once the launch can answer what the agent is working on
  without a Git call.
- **Cold facts.** Counting lookups would call a `feedback` fact the agent
  reads every launch, without ever running `show`, unused. A signal that
  lies about the most important facts is worse than none.

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
