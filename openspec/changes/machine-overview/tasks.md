## 1. Core model

- [x] 1.1 Add the harness-neutral types to `uze-core`'s `machine/` concern (found resource with definition files, origin hint, command or path it runs, removal command; session count), re-exported flat
- [x] 1.2 Add `machine_inventory` and `session_count` to `IntegrationPort` with "not known" defaults; prove an integration that declares nothing contributes nothing
- [x] 1.3 Token estimator with a calibration test against known counts (within ~15%); a file over 1 MB measured by size
- [x] 1.4 `uze` ownership from attachment receipts, merging what uze delivered across harnesses, `built-in`; tests for each
- [x] 1.5 Duplicate detection by kind, name and content digest, each naming the others; test
- [x] 1.6 Static health checks (hook command file, local MCP command on `PATH` via `uze-platform`, unreadable or invalid frontmatter, missing plugin directory; a URL server never judged); tests per check
- [x] 1.7 Status and order (broken, duplicate, ok; heaviest first); test

## 2. Collection

- [x] 2.1 Name `cache/machine-overview.json` in `UzeHome`; names, paths, origins, figures and statuses only; collected again when unreadable
- [x] 2.2 Add the snapshot to the cache-is-not-required-for-correctness invariant and its test
- [x] 2.3 `uze-application` read model and staged collection (inventory, tokens and health, sessions), one harness at a time, re-exported for `src/`
- [x] 2.4 Collection id on every answer, stale answers dropped, panic answers "not known", cancel on leaving the route or closing the modal

## 3. Integrations

- [x] 3.1 Claude Code: inventory per design §5 and root-session count by path and file time
- [x] 3.2 Codex: inventory per design §5 and rollout count excluding `parent_thread_id`, reading the first line only
- [x] 3.3 OpenCode: inventory per design §5 and `opencode stats --json` from a directory outside every project, with a timeout
- [x] 3.4 Antigravity: inventory per design §5 incl. built-in skills, and `conversations/*.db` count with the `-wal` time, opening none
- [x] 3.5 Each integration's removal command for what the harness installed, shown only
- [ ] 3.6 Lab checks: each harness's inventory finds a resource placed at each machine-level location; `opencode stats` is machine-wide and excludes subagents; which global instructions file Antigravity reads

## 4. Screen

- [x] 4.1 Overview in Marketplace's shape: header fields, filter, tree folded by kind, panel, foot keys, per the design-system skill's checklist
- [x] 4.2 Harness field narrows every row, group, total and use count; no period field (D12); a test with ten harnesses keeps the columns
- [x] 4.3 `uze` findings group from `actionable_alerts`, drawn only with findings
- [x] 4.4 Drawn at once from the snapshot, in-flight header while collecting, broken reasons, duplicates naming each other, removal guidance (Marketplace link or the harness's command, copy only); click == key tests for every hint
- [x] 4.5 Remove `Feature::Overview` and offer the route in every build

## 5. Performance and privacy guard

- [x] 5.1 Tests that nothing is collected by: an agent launch, the shim, any CLI command, the modal opened on another screen; and that leaving the Overview cancels a collection
- [x] 5.2 A test that the Overview's first frame draws from the snapshot with every integration blocked
- [x] 5.3 A test that no session transcript is opened beyond Codex's first line (a sentinel in a transcript's body never reaches the snapshot)
- [x] 5.5 Extend 5.3 to uses: a sentinel phrase beside a skill's name in every harness's record reaches neither the snapshot nor `cache/machine-usage.json`
- [x] 5.4 A benchmark with a pass/fail budget: a full collection over a machine with 500 resources under 300 ms excluding the `opencode stats` spawn

## 7. Uses

- [x] 7.1 Core types in `machine/usage`: `UsageRecord` (id, stamp), `Resume`, `UsageRead`, `Use` (kind, target, time, invoker, exact); `usage_records` and `read_usage` on `IntegrationPort` with "not known" defaults
- [x] 7.2 Core reading: skip a record whose stamp is unchanged, resume an append-only one at its offset, replace a rewritten one's uses; cancel between records; `cache/machine-usage.json` named in `UzeHome`, in the `cache/` tier, lost only at the cost of a fresh reading; tests for each
- [x] 7.3 Join a use to the inventory (name, MCP server, definition path), daily tallies summed over every record, `uses` and the `unused` status (never where any carrying harness reads unknown); tests
- [x] 7.4 Claude Code reader: `<command-name>`, `agent_mention`, `tool_use` `Skill`/`Agent`/`mcp__`, one use per mention and dispatch; fixture tests from the Lab's shapes
- [x] 7.5 Codex reader: `<skill>`, the `exec` script's `SKILL.md` read judged by its output, `rN/` aliases, `spawn_agent`, `mcp__` as an estimate; fixture tests
- [x] 7.6 OpenCode reader: every session without opening its database, `session export` only for changed ones with a timeout; `skills[]`/`agents[]`, `skill`/`subagent` tools, `<key>_<tool>`; measure the listing and export cost in the Lab first
- [x] 7.7 Antigravity reader: `[Slash Command]`/`<SKILL>`, `read_file`/`view_file`/`run_command` of an absolute `SKILL.md` not in an `ERROR` step, `invoke_subagent`, `call_mcp_tool`, subagents' transcripts; fixture tests
- [x] 7.8 Collection stage 4 (uses) after sessions; the `uses` column (blank for hooks and rules, `?` unknown, `~` estimate), sortable, and `unused` in the status column and the menu's marks
- [x] 7.9 Benchmark: first reading and an incremental one over generated records of realistic size, with a pass/fail budget; check on the real machine without printing any record

## 8. Background sync and archive

- [x] 8.1 Background collection in the workspace (20 s after opening, every 15 minutes), one at a time under a `state/` lock; the Overview draws it, collects only past 15 minutes, says when it synced, and offers `refresh` in its foot; the architecture test names the workspace as the one collector
- [x] 8.2 `usage::Archive` in `state/machine-usage.json`, shaped, per record: a deleted record's uses kept, a relisted one read live, archive written before cache; the header says since when uses count

## 6. Architecture

- [x] 6.1 Update `docs/architecture/core-components.mmd`; run `make artifacts` and `cargo test -p uze-extensions`
- [ ] 6.2 `openspec validate --all --strict`, `make check`
