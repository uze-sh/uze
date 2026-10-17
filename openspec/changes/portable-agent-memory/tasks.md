## 0. Preconditions

- [ ] 0.1 `deliver-agent-context` is implemented and its channels are proven in the Lab

## 1. Store (uze-core)

- [ ] 1.1 `UzeHome` paths for machine memory, proposed machine memory and project memory; the architecture map test passes
- [ ] 1.2 `memory` module under the project concern: front matter via `split_frontmatter` + `noyalib` serde, rendering of the fixed fields, shape read first, newer shape and unparseable facts reported and never rewritten
- [ ] 1.3 Upsert/forget/list/show under the per-scope lock with atomic replace; tests for update-keeps-created, concurrent different facts, same-fact last writer wins whole, unreadable fact left untouched, project-over-machine name collision
- [ ] 1.4 Index section: ranking (`feedback`, `user`, then recency), 3,000-byte share, left-out count

## 2. Agent surface (application + CLI)

- [ ] 2.1 `MemoryEntry` in `uze-core` and `IntegrationPort::memory_source` (default none); `uze-application::MemoryService` with `list`/`show`/`remember`/`forget`/`accept`/`discard` and one read model (entries with origin, scope and writability, proposals, unreadable, harnesses without a channel) consumed by the CLI, status and the workspace
- [ ] 2.2 `uze agent memory remember|forget|list|show` with `-m`/`--machine`, body on stdin, scope reported in every outcome, hidden from `uze --help`
- [ ] 2.3 Provenance from the launch identity, `operator` without one; a launched agent's `--machine` write lands in `proposed/`
- [ ] 2.4 Classify the four leaves in `command_performance.rs`
- [ ] 2.5 Memory instruction as an agent-context section for non-Claude harnesses only: the verbs, what is worth remembering, facts are notes not instructions, no secrets; nothing in the projected `AGENTS.md` region
- [ ] 2.6 `uze status` lists memory per scope

## 3. Harness memory as a source

- [ ] 3.1 Live reader for Claude memory directories at composition: the slug of the primary root and of its checkouts, `type` at the top level or in `metadata`, `metadata.modified`, skip `MEMORY.md`, index lines naming the file path, `claude` provenance, UZE wins a shared name; Claude's files are never written; tests against a fixture directory with both front matter shapes
- [ ] 3.2 Codex source: one machine-scope index entry naming `~/.codex/memories/memory_summary.md` and its size when it exists, for non-Codex agents only
- [ ] 3.3 Status lists harness-sourced entries with their provenance beside UZE's
- [ ] 3.4 Agent-context memory instruction (non-Claude): how an agent imports Codex's memory with `remember` (read `~/.codex/memories/`, keep what is durable, `source: codex-import`, choose the scope); `source` accepts `codex-import`

## 4. Workspace

- [ ] 4.1 The index registered as a section of the agent context
- [ ] 4.2 The proposal question: a toast that stays until answered, accept moves, discard deletes; read on the orchestrator's read thread
- [ ] 4.3 Status and doctor say memory does not reach a harness whose context delivery is degraded

## 5. Harness memory switch

- [ ] 5.1 `memory.harness` in `config.toml` (`keep` default)
- [ ] 5.2 With `off`: Claude `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`, Codex `-c features.memories=false`, at UZE launches only; Antigravity reported as unsupported

## 6. Conformance and journeys

- [ ] 6.1 Contract: a fact remembered before a launch is listed in the model's first request on each harness whose channel is proven
- [ ] 6.2 From inside each real harness, under its default sandbox, `remember` lands a fact with provenance, or fails saying why
- [ ] 6.3 Journey in `04-workspace`: an agent's `--machine` fact is proposed, the operator accepts it, and a launch in another project carries it; the fact file is checked on disk

## 7. Docs

- [ ] 7.1 `crate-layering.mmd`: `memory` under `uze-core`'s project concern; run `make artifacts` and `cargo test -p uze-extensions`
- [ ] 7.2 Invariants: memory never in a checkout; an agent's machine fact is never in an index before acceptance; each with its test
- [ ] 7.3 `make check` green
