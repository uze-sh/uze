## 0. Preconditions

- [ ] 0.1 Archive `switch-agent-harness` (all tasks done, #258), so `agent-harness-switch` and its `agent-session-continuity` delta exist in `openspec/specs/`

## 1. Measure every channel first (Lab experiments, no Rust)

- [ ] 1.1 Synthetic providers record where a marker lands per dialect (Anthropic `system` / user block; OpenAI `instructions` / developer / user input item; Gemini `systemInstruction` / `contents`) as evidence beside the existing substring flags
- [ ] 1.2 `experiments/claude/agent-context`: a marker in a `CLAUDE.md` under a second `--add-dir`, with the env flag set; four outcomes, also in a project with no `AGENTS.md`; repeat with the `--settings` `SessionStart` fallback
- [ ] 1.3 `experiments/codex/agent-context`: launch-scoped `SessionStart` via `-c` plus session-layer trust, handler printing an absolute path; four outcomes, copies after two resumes, size in bytes against the spill limit, and the daemon variant without `-c` as a control
- [ ] 1.4 `experiments/opencode/agent-context`: a standalone plugin's `session.hook("context")` reading a file keyed by `sessionID`; four outcomes against the shared server, hot-load of a plugin written while it runs, and a `/new` session getting nothing
- [ ] 1.5 `experiments/antigravity/agent-context`: `--add-dir` with `.agents/hooks.json` `PreInvocation`, once with `ephemeralMessage` and once with `injectSteps`; four outcomes, rendering in the TUI, persistence across `--conversation`, and the `workspacePaths[0]` ordering
- [ ] 1.6 Record each harness's measured channel, or its declared limitation, in `decisions.md`, and adjust D2 before group 3 starts

## 2. Context and briefing (uze-core + uze-workspace)

- [ ] 2.1 `UzeHome` paths for the agent context directory and the OpenCode session index; the architecture map test passes
- [ ] 2.2 Compose the context in `continuity::plan`: preamble, sections, 9,000-byte bound, nothing when no section applies; tests for the bound with multi-byte text and the non-JSON opening
- [ ] 2.3 Briefing into the record: `Briefing{integration, text, head_at_switch}`, the continuity record's shape step and ladder rung, `render` bounded at 6,000 bytes
- [ ] 2.4 Retire the briefing on the evaluation pass once HEAD moves past `head_at_switch`, and on the next switch; tests for both and for a launch on another harness
- [ ] 2.5 `LaunchPlan.env`; the shim sets it, and never composes or gives a context on a launch it does not own

## 3. Channels (uze-integrations), per what group 1 measured

- [ ] 3.1 `IntegrationPort::context_channel()` with `Directory`, `LaunchHook`, `SessionPlugin`, `FirstTurn` (default) and an `accumulates` flag
- [ ] 3.2 Claude `Directory`: per-agent `CLAUDE.md` with the context inline, `--add-dir`, env flag always set
- [ ] 3.3 Codex `LaunchHook`: `-c` hook and session-layer trust for the exact handler, through `uze-platform`'s shell; shared with `track-touched-repositories` if it landed first
- [ ] 3.4 OpenCode `SessionPlugin`: `plugins/uze-context.ts` delivered by setup with a receipt; the shim places the context under the assigned session id
- [ ] 3.5 Antigravity: `LaunchHook` if 1.5 proved it, otherwise `FirstTurn` with the limitation declared
- [ ] 3.6 Status and doctor report a degraded or unavailable channel per harness, with the reason

## 4. Switch

- [ ] 4.1 `switch_harness` adds no first turn on non-`FirstTurn` channels, and its toast says the agent waits for the operator's next turn
- [ ] 4.2 Remove the `runtime/.../briefings/` path and its sweep

## 5. Conformance and journeys

- [ ] 5.1 `conformance/contract/agent_context.py`: the four outcomes, bindings per harness, declared limitations where measured
- [ ] 5.2 Move `briefing-first-turn-*` onto the contract as `briefing-reaches-the-model`, and keep `briefing-first-turn` for `FirstTurn` harnesses
- [ ] 5.3 A journey in `04-workspace`: switch an agent; the incoming pane holds no turn the operator did not type, and the record on disk holds the briefing; after a commit, a relaunch's context file has no briefing

## 6. Docs

- [ ] 6.1 Update `agent-lifecycle.mmd` and `workspace-components.mmd`; run `make artifacts` and `cargo test -p uze-extensions`
- [ ] 6.2 Invariants with their tests: no context text in argv or environment; context only for launches UZE owns
- [ ] 6.3 `make check` green
