## 1. Core model

- [ ] 1.1 Add the harness-neutral types to `uze-core`'s `machine/` concern (found resource with origin hint, problem and removal command; session source root/child; resolved usage event with completeness), re-exported flat
- [ ] 1.2 Add `machine_inventory`, `session_sources` and `read_usage` to `IntegrationPort` with "not known" defaults; prove an integration that declares nothing contributes nothing
- [ ] 1.3 Token estimator with a calibration test against known counts (within ~15%)
- [ ] 1.4 `uze` ownership from attachment receipts and merging one delivered resource across harnesses; tests for both and for `built-in`
- [ ] 1.5 Status, floors and ordering: unknown never reads unused, a floor never makes unused, broken first, heaviest first

## 2. Reader, index and scan

- [ ] 2.1 Resumable reader in `shared::transcript`: seek, raw-byte lines with a 4 MB cap, non-UTF-8 skipped, resume at the last complete line, substring prefilter; tests for a half-written tail and an oversized line
- [ ] 2.2 Name `cache/machine-usage.json` in `UzeHome`; counts per source, drop a source's counts on shrink or time going back, drop days over 90, whole-snapshot `write_atomic`
- [ ] 2.3 Add `machine-usage.json` to the cache-is-not-required-for-correctness invariant and its test (delete it, the Overview is rebuilt)
- [ ] 2.4 Privacy test: a transcript with an invented skill name, a hook's command and its output; none reaches the index
- [ ] 2.5 Single-pass lock on `cache/machine-usage.lock` named in `UzeHome` (a second client draws from the index), "complete" as read to the size seen at the pass's start, stable resource ids (package or harness, kind, name; never a path or version) with a reinstall test, pass id on every answer, late answers dropped, panic answers "not known", cancel between steps
- [ ] 2.6 `uze-application` read model and staged scan (inventory and tokens, sessions, uses), re-exported for `src/`; "unused" only when every source of the period is complete

## 3. Integrations

- [ ] 3.1 Claude Code: inventory (incl. broken from needs-auth and failed servers), root session sources, usage reader (Skill, slash commands, invoked_skills, Agent with subagent_type, mcp__ by longest server prefix, hook_success matched to configured hooks, as a floor until 3.5 proves silent runs are recorded), MCP names as the net of deferred-tool deltas and injected hook context, both in the uses stage and kept in the index; fixtures from the latest release
- [ ] 3.2 Codex: inventory (shared `~/.agents/skills`, plugins, hooks, MCP, AGENTS.md), root rollouts by `parent_thread_id`, `spawn_agent.agent_type` complete, `<skill>` blocks as floor; fixtures from the latest release
- [ ] 3.3 OpenCode: inventory incl. global AGENTS.md; sessions from the API listing following its cursor, `time.updated`, run outside every project; unknown unless proven machine-wide
- [ ] 3.4 Antigravity: inventory incl. built-in skills, agents, hooks; sessions by `conversations/*.db` with the `-wal` time, opening none
- [ ] 3.5 Lab checks: each harness's inventory finds a resource placed at machine level; the OpenCode listing is machine-wide; whether a silent Claude hook run is recorded; Codex MCP call record shape; which global instructions file Antigravity reads

## 4. Screen

- [ ] 4.1 Overview in Marketplace's shape: header fields, filter, tree folded by kind, panel, foot keys, per the design-system skill's checklist
- [ ] 4.2 Harness and period fields narrow every row, group and total; a test with ten harnesses keeps the columns
- [ ] 4.3 `uze` findings group from `actionable_alerts`, drawn only with findings
- [ ] 4.4 Staged filling with the in-flight header, `≥` floors, "being read" before a complete pass, removal guidance (Marketplace link or the harness's command, copy only); click == key tests for every hint
- [ ] 4.5 Remove `Feature::Overview` and offer the route in every build

## 5. Performance guard

- [ ] 5.1 Tests that no record is read by: an agent launch, the shim, any CLI command, the modal opened on another screen, leaving the Overview mid-pass
- [ ] 5.2 A test that the Overview's first frame draws from the index with every reader blocked
- [ ] 5.3 Benchmarks with pass/fail budgets: a warm pass over 250 sources with one grown under 100 ms; a cold pass over a synthetic 750 MB tree never holding more than the line cap and never stalling a frame

## 6. Architecture

- [ ] 6.1 Update `docs/architecture/core-components.mmd`; run `make artifacts` and `cargo test -p uze-extensions`
- [ ] 6.2 `openspec validate --all --strict`, `make check`
