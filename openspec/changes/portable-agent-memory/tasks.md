## 0. Preconditions

- [ ] 0.1 `deliver-agent-context` is implemented and its channels are proven in the Lab

## 1. Store (uze-core)

- [ ] 1.1 `UzeHome` paths for machine memory, proposed machine memory and project memory; the architecture map test passes
- [ ] 1.2 `memory` module under the project concern: front matter via `split_frontmatter` + `noyalib` serde, rendering of the fixed fields, shape read first, newer shape and unparseable facts reported and never rewritten
- [ ] 1.3 Upsert/forget/list/show under the per-scope lock with atomic replace; tests for update-keeps-created, concurrent different facts, same-fact last writer wins whole, unreadable fact left untouched, project-over-machine name collision
- [ ] 1.4 Index section: ranking (`feedback`, `user`, then recency), 3,000-byte share, left-out count

## 2. Agent surface (application + CLI)

- [ ] 2.1 `uze-application` memory service and status read model (directories, facts with type and provenance, proposals, unreadable)
- [ ] 2.2 `uze agent memory remember|forget|list|show` with `-m`/`--machine`, body on stdin, scope reported in every outcome, hidden from `uze --help`
- [ ] 2.3 Provenance from the launch identity, `operator` without one; a launched agent's `--machine` write lands in `proposed/`
- [ ] 2.4 Classify the four leaves in `command_performance.rs`
- [ ] 2.5 Projected `AGENTS.md` region: the verbs, what is worth remembering, facts are notes not instructions, no secrets
- [ ] 2.6 `uze status` lists memory per scope

## 3. Claude import

- [ ] 3.1 Reader for Claude memory directories: the slug of the primary root and of its checkouts, `type` at the top level or in `metadata`, `metadata.modified`, skip `MEMORY.md`
- [ ] 3.2 `uze agent memory import claude`: project facts with `claude-import` provenance and an `imported` date, `user`/`feedback` also proposed at machine scope, upsert by name, skip facts edited since import, Claude files read-only; tests against a fixture directory with both front matter shapes
- [ ] 3.3 Status counts the Claude facts not imported
- [ ] 3.4 Projected region: how an agent imports Codex's memory with `remember` (read `~/.codex/memories/`, keep what is durable, `source: codex-import`, choose the scope); `source` accepts `codex-import`

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
