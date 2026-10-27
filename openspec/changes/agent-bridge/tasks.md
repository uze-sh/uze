## 1. Reporting

- [x] 1.1 `UzeHome::agent_reports_path(id)` (generated tier) and `uze-workspace`'s journal: append bounded (256 lines, 64 KiB), read from an offset (B1)
- [x] 1.2 `uze agent report <event> [--tool --path --call --outcome]`, verified by the launch claim, exit 0 always; classified in `command_performance.rs`; hidden like every `uze agent` verb (parent 14.1)
- [x] 1.3 ADR-056 amended with the commit that creates `uze agent report` (R-A1)

## 2. Delivery per launch

- [x] 2.1 The portable vocabulary grows turn start and approval asked/resolved where a harness offers them (`capability/hook.rs`) (parent 14.2)
- [x] 2.2 Each integration's `runtime_contribution` carries the reporter for what its harness can say, failing open; each names what it cannot (B3)
- [x] 2.3 ADR-040 amended with that commit

## 3. Events and the agents source

- [x] 3.1 The client's one-second watch reads each agent's journal from its offset and normalizes reports into `agent.*` events with ids, times, outcomes, scrubbed tool names and checkout-relative paths; the paint heuristic's turn start and end where no report came (parent 14.3)
- [x] 3.2 `EventName::Agent*` in `uze-contract`, `refresh-on "agent.…"` in the decoder and check under `agent:observe`
- [x] 3.3 `SourceKind::Agents` and `Extension::agents(&Value)`; the engine answers it; the host hands the activity model's agents across open spaces each pass (parent 14.4)

## 4. Actions

- [x] 4.1 `IntegrationPort::skill_invocation(skill, args)` beside `invocation_prefix`, and each integration's native skills (parent 14.5)
- [x] 4.2 `Does::{AgentPrompt { text | skill, args, target }, AgentRestart { target }, FocusAgent { target }}` in the contract, decoder and check; grants `agent:observe`, `agent:prompt`, `agent:prompt:auto`, `agent:restart`
- [x] 4.3 The host writes through `submit_notice` behind `may_submit`, with the strict filter and byte equality with what was shown; refused unless Ready, foreground and no approval pending; never queued (parent 14.6)
- [x] 4.4 The rates and the pause key (B5) (parent 14.7); `agent:prompt:auto`'s record per project and agent, 24 h, revoked by a digest change
- [x] 4.5 `focus-agent` and a global key opening an overlay
- [x] 4.6 ADR-059 amended with `agent.prompt`'s commit; the architecture rule that agent actions from an extension go only through the bridge (parent 14.8, 14.9)

## 5. Gallery and docs

- [x] 5.1 Fixtures: `agents-hud`, `flightdeck`, `chimes`, `jump`, `plan-review`, and `gsd-band`'s next-step button, each with tests
- [x] 5.2 The extensions rule, invariants, diagrams, the `uze:extension` skill
- [ ] 5.3 A journey: an agent's turn refreshes a status row, and a prompt is refused while it works and sent once it is idle
- [ ] 5.4 `make check`, `openspec validate --all --strict`, the hand proof script
