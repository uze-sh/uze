## Context

The inventory, counted 2026-10-24 (references outside tests): `code` 358
(most in `session/surface.rs`, `session/mouse.rs`, `answers.rs`,
`reads.rs`, `input.rs`), `spec` 134 (`render/sidebar.rs`, `answers.rs`,
`session/surface.rs`, `surfaces.rs`), `architect` 99 (`session/surface.rs`,
`session/mouse.rs`, `surfaces.rs`). Declared extensions are hosted through
`remembered.declared`'s slot map (`src/ui/orchestrator/declared.rs`), whose
engines are `Box<dyn Hosted>`, and run no jobs: the contract's `jobs()` is
consumed nowhere yet. `WorkspaceHost` may appear only inside
`thread::spawn` in `reads.rs`.

## Goals / Non-Goals

**Goals:** the generic path carries every built-in, one at a time, each
step green and revertible; the hardest behaviours (the code editor's
typing, diagram dragging, the timeline section) are proved on it.

**Non-Goals:** any new behaviour; changing how a built-in looks or reads;
the factory over installed declared extensions beyond what hosting the
built-ins needs.

## Decisions

### H1. Jobs first, with no consumer change
The first step makes `jobs()`/`absorb()` real for the slot map: after any
command, hit or open, the host drains the engine's jobs, runs each on a
thread with `WorkspaceHost` and its fallback on a panic, and routes the
answer back to the same slot by id. A test extension proves it; declared
engines keep returning none.

### H2. One extension per step, smallest first
`architect` (one read, one surface), then `spec` (a view and a section, two
reads), then `code` (four reads, a view and the timeline section, the
editor). Each step removes that extension's fields, channels, typed reads
and hit variant in the same commit that hosts it, so the codebase never
holds both paths for one extension.

### H3. Host gestures stay the host's
The navigator's edge, the content scrollbar and text selection are geometry
the host owns today for every surface; the slot path keeps them in the
orchestrator and hands the engine only `ViewHit`s and `Command`s, as the
declared drawer and view already do.

## Risks / Trade-offs

- [The orchestrator is large and these sites interlock] → one extension per
  step, the full lib suite, the architecture suite and the journeys green
  at each; a step that cannot go green is reverted, not patched forward.
- [Typing in the code editor is latency-sensitive] → its commands go
  through the same call they do today, without a job, since they need no
  `Host`.
