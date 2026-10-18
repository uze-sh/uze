## 0. Preconditions

- [x] 0.1 `deliver-agent-context` is implemented and its channels are proven in the Lab

## 1. Store (uze-core)

- [x] 1.1 `UzeHome` paths for machine memory, proposed machine memory and project memory; the architecture map test passes
- [x] 1.2 `memory` module under the project concern: front matter via `split_frontmatter` + `noyalib` serde, rendering of the fixed fields, shape read first, newer shape and unparseable facts reported and never rewritten
- [x] 1.3 Upsert/forget/list/show under the per-scope lock with atomic replace; tests for update-keeps-created, concurrent different facts, same-fact last writer wins whole, unreadable fact left untouched, project-over-machine name collision
- [x] 1.4 Index section: ranking (`feedback`, `user`, then recency), within the 3,500-byte memory share (instruction ≤700 for non-Claude agents, index the rest, Codex entry included), left-out count, each harness's own entries left out of its own agents' index

## 2. Agent surface (application + CLI)

- [x] 2.1 `MemoryEntry` in `uze-core` and `IntegrationPort::memory_source` (default none); `uze-application::MemoryService` with `list`/`show`/`remember`/`forget`/`accept`/`discard` and one read model (entries with origin, scope and writability, proposals, unreadable, harnesses without a channel) consumed by the CLI, status and the workspace
- [x] 2.2 `uze agent memory remember|forget|list|show` with `-m`/`--machine`, body on stdin, scope reported in every outcome, hidden from `uze --help`
- [x] 2.3 Provenance from the launch identity, `operator` without one; a launched agent's `--machine` write lands in `proposed/`
- [x] 2.4 Classify the four leaves in `command_performance.rs`
- [x] 2.5 Memory instruction as an agent-context section for non-Claude harnesses only: the verbs, what is worth remembering, facts are notes not instructions, no secrets; nothing in the projected `AGENTS.md` region
- [x] 2.6 `uze status` lists memory per scope

## 3. Harness memory as a source

- [x] 3.1 Live reader for Claude memory directories at composition: the slug of the primary root and of its checkouts, `type` at the top level or in `metadata`, `metadata.modified`, skip `MEMORY.md`, index lines naming the file path, `claude` provenance, UZE wins a shared name; Claude's files are never written; tests against a fixture directory with both front matter shapes
- [x] 3.2 Codex source: one machine-scope index entry naming `~/.codex/memories/memory_summary.md` and its size when it exists, for non-Codex agents only
- [x] 3.3 Status lists harness-sourced entries with their provenance beside UZE's
- [x] 3.4 Agent-context memory instruction (non-Claude): how an agent asked to distill Codex's memory does it with `remember` (read `~/.codex/memories/`, keep what is durable, choose the scope)

## 4. Workspace

- [x] 4.1 The application's launch path renders the index section from `MemoryService` and hands it to `continuity::plan` as data; `uze-workspace` names no memory type
- [x] 4.2 The proposal question in `src/ui`: a toast that stays until answered, accept moves, discard deletes; proposals read on the existing evaluation pass on the orchestrator's read thread; status lists each proposal's path for acceptance outside the workspace
- [x] 4.3 Status and doctor say memory does not reach a harness whose context delivery is degraded

## 5. Harness memory switch

- [x] 5.1 `memory.harness` in `config.toml` (`keep` default)
- [x] 5.2 With `off`: Codex `-c features.memories=false` at UZE launches only; Claude never turned off; Antigravity reported as unsupported

## 5b. Looking facts up

- [x] 5b.1 The index gives `feedback` a line and names every other fact by kind, names placed before lines, no paths (`render_index`)
- [x] 5b.2 A read instruction for every launched agent, Claude's included (`READ_INSTRUCTION`)
- [x] 5b.3 `uze agent memory list <words> [--kind]`: name, description and body, best first, at most 20, quoted hits and the `show` command; proposals never searched; `--format json`
- [x] 5b.4 `show` without `-m` finds a machine fact the project does not shadow
- [x] 5b.5 Contract: a non-`feedback` fact past the lines reaches the model by name, and the read instruction reaches it

## 6. Conformance and journeys

- [x] 6.1 Contract: a fact remembered before a launch is listed in the model's first request on each harness whose channel is proven
- [x] 6.2 From inside each real harness, under its default sandbox, `remember` lands a fact with provenance, or fails saying why
- [x] 6.3 Journey in `04-workspace`: an agent's `--machine` fact is proposed, the operator accepts it, and a launch in another project carries it; the fact file is checked on disk

## 7. Docs

- [x] 7.1 `crate-layering.mmd`: `memory` under `uze-core`'s project concern; run `make artifacts` and `cargo test -p uze-extensions`
- [x] 7.2 Invariants: memory never in a checkout; an agent's machine fact is never in an index before acceptance; each with its test
- [x] 7.3 `make check` green

## 8. Recall

- [x] 8.1 The corpus test (D14) first, against today's matching: about 30 facts (partly anonymised real ones, fixed dates, no harness source) and about 30 frozen questions with negative cases; the count answered today recorded as the ratchet later tasks may only raise
- [x] 8.2 Room (D8): `agent_context` answers the room sections leave (preamble as written, each section as `compose` emits it) and exposes the heading length; `give_context` hands the closure a request with the primary root, the launching checkout and the room; the memory section's `share` is the room, `MEMORY_SHARE` the floor; `plan()` and the shim updated; tests for no briefing, a full briefing, and the composed context within `BOUND`
- [x] 8.3 Lines in rank order (D9): `render_index` gives lines over the whole ranking while the remaining names fit, descriptions capped at 160 bytes; tests for every kind given a line when it fits, names before lines past the room, and one long description not ending the lines after it
- [x] 8.4 Folding and ranking (D11) in `find`: diacritics and separators, more words in the name then the description first; the accent and name-over-body scenarios; the ratchet rises or holds
- [x] 8.5 Links (D12): resolution by name, stem, then unique folded match; `show` lists both directions with descriptions and unresolved links; `show --format json` with `Shown` serialized; tests for the stem and kind-prefix cases and an ambiguous link
- [x] 8.6 Missing citations (D10): `memory::cited_paths` in core, filled by UZE's listing and each source where the body is read; judged in `gather` against the reader's checkout, 64 in rank order, plain root `.gitignore` entries skipped; the index's closing line reserved inside `render_index`'s bound; `show`, JSON, and the Memory tab's mark through a new theme symbol; tests for a moved file, a fragment, a placeholder, a glob, an ignored directory, a file only an agent's branch has, a machine fact, and the cap's order
- [x] 8.7 Credentials and near duplicates (D13): provider shapes refused in core `remember` naming the shape and line, never the value, fixtures built by concatenation; the assignment caution; near duplicates in `uze-application` over UZE facts, sources and proposals, on the CLI's outcome and JSON; tests for each shape, the documented-variable scenario and the paraphrase
- [x] 8.8 The space overview's drawer scrolls (wheel, keys and scrollbar) the way the management screen's detail drawer does: today `render_drawer` draws its lines as one unscrolled `Paragraph` (`src/ui/orchestrator/render/space_overview.rs`), so a long fact body is cut at the drawer's foot; `TestBackend` test for a body longer than the drawer
- [x] 8.9 Budget: the memory budget test's `World` holds a realistic memory (dozens of facts, citations, links) and measures `show`, `remember`, `list` and the context section; D2's wording matches
- [x] 8.10 `make check` green
