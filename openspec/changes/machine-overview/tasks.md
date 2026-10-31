## 1. Core model

- [ ] 1.1 Add the harness-neutral types to `uze-core`'s `machine/` concern (found resource with definition files, origin hint, command or path it runs, removal command; session count), re-exported flat
- [ ] 1.2 Add `machine_inventory` and `session_count` to `IntegrationPort` with "not known" defaults; prove an integration that declares nothing contributes nothing
- [ ] 1.3 Token estimator with a calibration test against known counts (within ~15%); a file over 1 MB measured by size
- [ ] 1.4 `uze` ownership from attachment receipts, merging what uze delivered across harnesses, `built-in`; tests for each
- [ ] 1.5 Duplicate detection by kind, name and content digest, each naming the others; test
- [ ] 1.6 Static health checks (hook command file, local MCP command on `PATH` via `uze-platform`, unreadable or invalid frontmatter, missing plugin directory; a URL server never judged); tests per check
- [ ] 1.7 Status and order (broken, duplicate, ok; heaviest first); test

## 2. Collection

- [ ] 2.1 Name `cache/machine-overview.json` in `UzeHome`; names, paths, origins, figures and statuses only; collected again when unreadable
- [ ] 2.2 Add the snapshot to the cache-is-not-required-for-correctness invariant and its test
- [ ] 2.3 `uze-application` read model and staged collection (inventory, tokens and health, sessions), one harness at a time, re-exported for `src/`
- [ ] 2.4 Collection id on every answer, stale answers dropped, panic answers "not known", cancel on leaving the route or closing the modal

## 3. Integrations

- [ ] 3.1 Claude Code: inventory per design §5 and root-session count by path and file time
- [ ] 3.2 Codex: inventory per design §5 and rollout count excluding `parent_thread_id`, reading the first line only
- [ ] 3.3 OpenCode: inventory per design §5 and `opencode stats --json` from a directory outside every project, with a timeout
- [ ] 3.4 Antigravity: inventory per design §5 incl. built-in skills, and `conversations/*.db` count with the `-wal` time, opening none
- [ ] 3.5 Each integration's removal command for what the harness installed, shown only
- [ ] 3.6 Lab checks: each harness's inventory finds a resource placed at each machine-level location; `opencode stats` is machine-wide and excludes subagents; which global instructions file Antigravity reads

## 4. Screen

- [ ] 4.1 Overview in Marketplace's shape: header fields, filter, tree folded by kind, panel, foot keys, per the design-system skill's checklist
- [ ] 4.2 Harness field narrows every row, group and total; period sets the session count; a test with ten harnesses keeps the columns
- [ ] 4.3 `uze` findings group from `actionable_alerts`, drawn only with findings
- [ ] 4.4 Drawn at once from the snapshot, in-flight header while collecting, broken reasons, duplicates naming each other, removal guidance (Marketplace link or the harness's command, copy only); click == key tests for every hint
- [ ] 4.5 Remove `Feature::Overview` and offer the route in every build

## 5. Performance and privacy guard

- [ ] 5.1 Tests that nothing is collected by: an agent launch, the shim, any CLI command, the modal opened on another screen; and that leaving the Overview cancels a collection
- [ ] 5.2 A test that the Overview's first frame draws from the snapshot with every integration blocked
- [ ] 5.3 A test that no session transcript is opened beyond Codex's first line (a sentinel in a transcript's body never reaches the snapshot)
- [ ] 5.4 A benchmark with a pass/fail budget: a full collection over a machine with 500 resources under 300 ms excluding the `opencode stats` spawn

## 6. Architecture

- [ ] 6.1 Update `docs/architecture/core-components.mmd`; run `make artifacts` and `cargo test -p uze-extensions`
- [ ] 6.2 `openspec validate --all --strict`, `make check`
