## 1. Jobs

- [ ] 1.1 Run a hosted engine's `jobs()` after it is opened, commanded or hit: each on a thread in `reads.rs` with `WorkspaceHost`, its fallback answered on a panic, the answer routed to the slot's `absorb` by slot id; a test extension in the orchestrator tests proves a job's answer redraws its surface and a panic answers the fallback

## 2. Architect

- [ ] 2.1 `ArchitectExtension: Extension` in `uze-extensions`: its surface, its artifact read as a job, `ArchitectOutcome` onto `Outcome`, its place
- [ ] 2.2 Host it in the slot map; remove `architect`, `architect_root`, `architect_asked`, its channel, `spawn_architect_*`/`absorb_*`, `ExtensionHit::Architect` and `architect_places`; keep `architect_grab` as host geometry
- [ ] 2.3 Every orchestrator and TestBackend test green; the architecture suite green

## 3. Spec

- [ ] 3.1 `SpecExtension: Extension`: the document surface and the summary section, both reads as jobs, its place
- [ ] 3.2 Host it; remove its fields, channels, reads, `ExtensionHit::Spec`/`SpecSummary` and `spec_places`
- [ ] 3.3 Tests green

## 4. Code

- [ ] 4.1 `CodeExtension: Extension`: the surface in its modes and the timeline section, its four reads as jobs, `CodeOutcome::Copy` onto `Outcome`
- [ ] 4.2 Host it; remove its fields, channels, reads, `ExtensionHit::Code`/`CodeTimeline` and `code_places`; typing stays a direct command
- [ ] 4.3 Tests green

## 5. Contract and docs

- [ ] 5.1 `ExtensionHit` as `{ slot, surface, hit }`; the extensions rule and `docs/architecture/invariants.md` updated; `docs/architecture/workspace-components.mmd` redrawn; `make artifacts`
- [ ] 5.2 `make check`, the journeys, `openspec validate --all --strict`
