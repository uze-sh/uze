## 1. Measure before building

- [ ] 1.1 `conformance/experiments/antigravity/nested-instructions.py`: whether a post-tool hook in UZE's `hooks.json` can inject context (`injectSteps`) the model reads, and whether `workspacePaths[0]` is the launch directory or the git root when launched in `sub/`. Record the outcome in design D3.
- [ ] 1.2 For Claude Code and OpenCode, measure whether a nested `AGENTS.md` arrives on edit, on write of a new file, and on a shell read (`cat sub/notes.txt`), not only on the file-read tool. Record divergences in design Risks.
- [ ] 1.3 Measure whether a `sub/CLAUDE.md` below Claude's launch directory stops `sub/AGENTS.md` from being injected; hand the result to `resolve-instruction-files`.
- [ ] 1.4 Re-measure the proposal's matrix on the harness versions current at implementation time and update it.

## 2. Declaration and read model

- [ ] 2.1 `NestedInstructions { Native, RulesEngine, Unsupported { reason } }` and `IntegrationPort::nested_instructions()` in `uze-core`, defaulting to `Unsupported`.
- [ ] 2.2 Declare it per integration (Claude Code and OpenCode `Native`, Codex `RulesEngine`, Antigravity per 1.1), each with a `HarnessFact` in `facts()` whose `proven_by` names the Lab check from 5.1.
- [ ] 2.3 `nested_instructions` in `HarnessContextSupport` and `AgentContextStatus`, taking the shadowed state when the `AGENTS.md` row is shadowed. Unit tests per declaration.

## 3. Engine

- [ ] 3.1 Second engine argument from each harness's `RulesTarget` (`--nested` or nothing); every hook entry and receipt rewritten by setup. Record it as an extension of ADR-040 D8 in decisions.md.
- [ ] 3.2 Nested walk on `PostToolUse`, before the rules gate: from each touched path's directory up to the payload `cwd` (excluded), outermost first, recorded per session like a glob rule.
- [ ] 3.3 Rewrite the "a checkout without rules costs one directory check" invariant in `docs/architecture/invariants.md` and `a_checkout_without_rules_or_outside_git_pays_nothing` to the new early exit.
- [ ] 3.4 Engine tests in `rules/engine_tests.rs`: first read delivers, second does not, compact start delivers again, two levels outermost first, nothing above the launch directory, nothing without `--nested`, delivered with no `.agents/rules`, checkout unchanged.
- [ ] 3.5 If 1.1 allows it: Antigravity output mode, `deliver_rules_engine`/`remove_rules_engine`/`doctor` attaching it for nested files only, `invariants.md` and `antigravity_is_left_to_its_own_rules` updated, `docs/architecture/core-components.mmd` edge, then `make artifacts` and `cargo test -p uze-extensions`.

## 4. Report

- [ ] 4.1 Nested-instructions row in the harness drawer (`src/ui/view/harnesses.rs`), the agent support popup and `uze status`, with the existing `context_row` words. Read the `design-system` skill before and run its checklist after.
- [ ] 4.2 Field in `uze agent context inspect`, including `--format json` (BREAKING shape change, stated in the PR).

## 5. Proof and docs

- [ ] 5.1 Lab contract check in `conformance/contract/context.py` with a marker in `shared/markers.py`: launched at the checkout root, the agent reads a file under `sub/`, the request after the read carries the nonce from `sub/AGENTS.md`, the working tree is unchanged. A harness declaring `Unsupported` uses `bindings.unsupported` with its reason.
- [ ] 5.2 Lab check that a `Native` harness receives the nested file exactly once with the engine attached.
- [ ] 5.3 Product journey for the nested row in `uze status` (`journey-author` skill), after the operator validated by hand.
- [ ] 5.4 Each integration's README support table (with the size and subagent notes from design), `docs/capabilities/context-manager.md` and the site's harness pages.
- [ ] 5.5 `make check`, `openspec validate --all --strict`, and the Lab verticals for the four harnesses.
