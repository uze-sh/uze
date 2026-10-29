## 0. Preconditions

- [x] 0.1 Archive `switch-agent-harness` (all tasks done, #258), so `agent-harness-switch` and its `agent-session-continuity` delta exist in `openspec/specs/`

## 1. Measure every channel first (Lab experiments, no Rust)

Measured on 2026-10-17 (decisions.md M1). The experiment files live in the `agent-context-lab` subagent checkout and are joined with this branch before group 5.


- [x] 1.1 Synthetic providers record where a marker lands per dialect (Anthropic `system` / user block; OpenAI `instructions` / developer / user input item; Gemini `systemInstruction` / `contents`) as evidence beside the existing substring flags
- [x] 1.2 `experiments/claude/agent-context`: a marker in a `CLAUDE.md` under a second `--add-dir`, with the env flag set; four outcomes, also in a project with no `AGENTS.md`; repeat with the `--settings` `SessionStart` fallback
- [x] 1.3 `experiments/codex/agent-context`: launch-scoped `SessionStart` via `-c` plus session-layer trust, handler printing an absolute path; four outcomes, copies after two resumes, size in bytes against the spill limit, and the daemon variant without `-c` as a control
- [x] 1.4 `experiments/opencode/agent-context`: a standalone plugin's `session.hook("context")` reading a file keyed by `sessionID`; four outcomes against the shared server, hot-load of a plugin written while it runs, and a `/new` session getting nothing
- [x] 1.5 `experiments/antigravity/agent-context`: `--add-dir` with `.agents/hooks.json` `PreInvocation`, once with `ephemeralMessage` and once with `injectSteps`; four outcomes, rendering in the TUI, persistence across `--conversation`, and the `workspacePaths[0]` ordering
- [x] 1.6 Record each harness's measured channel, or its declared limitation, in `decisions.md`, and adjust D2 before group 3 starts

## 2. Context and briefing (uze-core + uze-workspace)

- [x] 2.1 `UzeHome` paths for the agent context directory and the OpenCode session index; the architecture map test passes
- [x] 2.2 Compose the context in `continuity::plan`: preamble (≤500 bytes), sections with shares summing to the 9,000-byte bound (briefing 5,000, memory 3,500 reserved), nothing when no section applies; tests for the bound with multi-byte text and the non-JSON opening
- [x] 2.3 Briefing into the record: `Briefing{integration, text, head_at_switch}`, the continuity record's shape step and ladder rung, `render` bounded at 5,000 bytes
- [x] 2.4 Retire the briefing on the evaluation pass once HEAD moves past `head_at_switch`, and on the next switch; tests for both and for a launch on another harness
- [x] 2.5 `LaunchPlan.env`; the shim sets it, and never composes or gives a context on a launch it does not own

## 3. Channels (uze-integrations), per what group 1 measured

- [x] 3.1 `IntegrationPort::context_channel()` with `Directory`, `LaunchHook`, `SessionPlugin`, `FirstTurn` (default) and an `accumulates` flag
- [x] 3.2 Claude `Directory`: per-agent `CLAUDE.md` with the context inline, `--add-dir`, env flag always set
- [x] 3.3 Codex `LaunchHook`: `-c` hook and session-layer trust for the exact handler, through `uze-platform`'s shell; shared with `track-touched-repositories` if it landed first
- [x] 3.4 OpenCode `SessionPlugin`: `plugins/uze-context.ts` delivered by setup with a receipt; the shim writes the context under the assigned session id in the integration-parameterised `UzeHome` directory; `conversation::forget` removes it
- [x] 3.5 Antigravity `LaunchHook`: per-launch `hooks.json` with a nonce, conditional handler (`sh` and PowerShell) printing once per launch, `accumulates`; signed-in as the `json-hooks-enabled` proxy, `FirstTurn` for the briefing when not signed in
- [x] 3.6 Status and doctor report a degraded or unavailable channel per harness, with the reason

## 4. Switch

- [x] 4.1 `switch_harness` adds no first turn on non-`FirstTurn` channels, and its toast says the agent waits for the operator's next turn
- [x] 4.2 Remove the `runtime/.../briefings/` path and its sweep

## 5. Conformance and journeys

- [x] 5.1 `conformance/contract/agent_context.py`: the four outcomes, bindings per harness, declared limitations where measured
- [x] 5.2 Move `briefing-first-turn-*` onto the contract as `briefing-reaches-the-model`, and keep `briefing-first-turn` for `FirstTurn` harnesses
- [x] 5.3 A journey in `04-workspace`: switch an agent; the incoming pane holds no turn the operator did not type, and the record on disk holds the briefing; after a commit, a relaunch's context file has no briefing

## 6. Docs

- [x] 6.1 Update `agent-lifecycle.mmd` and `workspace-components.mmd`; run `make artifacts` and `cargo test -p uze-extensions`
- [x] 6.2 Invariants with their tests: no context text in argv or environment; context only for launches UZE owns
- [x] 6.3 `make check` green
