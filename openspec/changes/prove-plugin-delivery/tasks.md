## 0. Bookkeeping

- [x] 0.1 Mark as moved to this change, with a pointer to it: `native-first-hooks` tasks 6.2–6.4, and the hook-semantics tasks of `extend-conformance-coverage` (1.1–1.3, 5.1). Leave every other task in those changes untouched.
- [x] 0.2 Write (`expected-red.md`) the list of checks that are expected to be red at the end of group 6. That list is the survey's breaks plus the sweep's findings. Every later fix names the check it turns green.

## 1. Measure the vendors

- [x] 1.1 Make each provider extract the declared tools and their input schemas from every request, in that harness's dialect:
  - Anthropic `tools[]`
  - OpenAI Responses `tools[]`
  - chat `tools[].function`
  - Gemini `functionDeclarations`

  A capture that parses zero tools fails the run as a Lab defect.
- [x] 1.2 Write `conformance/evidence/tools/<harness>.json`, keyed by harness version, from a fresh run of each vertical. Commit the four snapshots. Recorded 2026-10-06 from the full run: Claude 2.1.291, Codex 0.160.1, OpenCode 2.0.24, Antigravity 1.2.17; `measured_tests` agrees with all four.
- [x] 1.3 For every tool a harness resolves without declaring it to the model, take a `--discovery` capture and record it with its version. OpenCode's hook payloads (`event.tool`, input field names) come first, since no capture of them was ever taken.
- [x] 1.4 Give each `bindings.py` its own `alias → native tool + fields` expectation, written from the capture, not from `crates/uze-integrations`.
- [x] 1.5 At run start, assert that UZE's binding table and the Lab's expectation are both subsets of the capture and that they agree. Fail with the alias, the native name and the declared tools.
- [x] 1.6 Make the provider refuse to script a tool that is absent from the capture. That fails the run before any check is evaluated.
- [x] 1.7 Replace every hand-kept wire constant with a value from the capture or a recorded discovery: tool and MCP names, argument schemas, call-id shapes, namespaces, model ids. `inventory.md` (Sweep C) lists every one, per provider. Tool and MCP names, namespaces and argument shapes are measured every run: a scripted call naming a tool its request did not declare is refused and fails `vocabulary-scripted-calls-declared`, and a call-id the harness rejects never settles the turn. Responses name the model their request asked for (Claude, Codex); the bare-word skill markers and the catch-all tool results are gone, and Antigravity's unused `TOOL_NAMES` (which still listed `grep_search`) is removed.
- [x] 1.8 Add a deterministic Rust test that reads the committed snapshots and fails when a `ToolBinding` names a tool or field absent from its harness's snapshot. This replaces the self-referential assertions in `tests/integrations/hooks.rs` and `hooks/tests.rs`.

## 2. Results that cannot lie

- [x] 2.1 Collapse `adapt` and `adapted` into one `declared` kind, emitted only through `declare(name, measurement, reason)`. The gate adjudicates it for contracts and vendor scenarios alike.
- [x] 2.2 Move every `bindings.unsupported` declaration into `evidence/expected.json`. Each one gets the measurement that proves it and the versions where it was observed. Make the gate refuse `*`.
- [x] 2.3 On an uncovered version, pass a declaration whose measurement reproduced, write `evidence/expected.next.json` with the version appended, and upload it from CI. Fail a declaration whose measurement did not run as unproven.
- [x] 2.4 Make `check_absence` require `proof`, the same turn's presence result, and record an absence as unproven when its proof failed.
- [x] 2.5 Settle a turn on its final answer and a quiet surface, and rest no verdict on settling alone: a tool that ran is proven by its side effect or its output in the next request, never by a tool result arriving (an error, an unknown tool and a refusal answer with one too). The old waiters that accepted `error`, `denied` or `Sandbox mode` as an end went with the vertical hook phases.
- [x] 2.6 Replace text markers with nonces derived at run time by the handler or the tool. No marker may appear in the prompt, in the scripted arguments or in any earlier request. That includes `plain output` and the bare `commit`, `review` and `init` in `SKILL_MARKERS`.
- [x] 2.7 Add the pytest lint in `conformance/tests/`. It walks every scenario and contract and refuses:
  - a literal verdict;
  - `check_absence` without `proof`;
  - an unknown kind;
  - a marker that appears in a scripted call or prompt;
  - a prompt answered without a decision (see 5.2).

## 3. The hooks contract

- [x] 3.1 Add `conformance/contract/hooks.py`, iterating `alias × event × effect` from one matrix that each harness's bindings fill or declare. Register it in `CONTRACTS`.
- [x] 3.2 Rewrite the hook fixtures:
  - one group per alias with an alias-only matcher (no `native:` fallback beside it);
  - per-event groups for PreToolUse, PostToolUse, Stop and SessionStart;
  - per-effect groups for deny, allow, ask and observe;
  - a fail-closed group.
- [x] 3.3 Make every fixture handler write a nonce marker carrying its group, its position and every `HOOK_*` value it received. Make the deny guard decide on the alias's own field (`HOOK_COMMAND`, `HOOK_PATH`) alone, so a wrong field mapping fails instead of denying through `HOOK_INPUT`.
- [x] 3.4 Per harness, script one real tool call per bound alias from the capture. Assert:
  - the handler ran;
  - its `HOOK_TOOL` and portable fields are the expected values;
  - the native decision was relayed (denial reason, block, ask) or the tool executed (allow, observe).
- [x] 3.5 Assert ordering (first deny wins, later handlers do not run) and fail-closed per harness from the markers, not from the absence of side effects.
- [x] 3.6 For Claude PostToolUse and Stop deny, assert the block and also that the harness shows no hook-error notice.
- [x] 3.7 Add a deterministic test that fails when a binding table gains an alias with no matrix row for that harness.

## 4. Rewrite every vacuous check

- [x] 4.1 Turn the contract checks hard-coded `True` with `kind="adapt"` into measured declarations (2.1) or real checks:
  - `skill.py:72,186`
  - `mcp.py:24`
  - `agent.py:64`
  - `context.py:214`
  - `continuity.py:166`
- [x] 4.2 Gate `skill-model-only-is-not-user-invocable` and `-not-invocable` on a real settled turn (`skill.py:131,226`).
- [x] 4.3 Gate `user-only-skill-hidden` (Claude) and `user-only-skill-hidden-from-model` (OpenCode) on a recorded presence. Make the Claude provider record the marker its partner check reads.
- [x] 4.4 Make `model-visible-skill-present` (Claude, `scenarios.py:224`) assert the skill's nonce description, not the substring `commit`.
- [x] 4.5 Replace the OpenCode hook phase:
  - drop the stale direct MCP name;
  - fire the bound aliases through V2's real tools;
  - delete `hooks-{deny,order}-v2-limitation` and the `tool-executed` ADAPTED entries;
  - re-declare only what the capture and measurement prove V2 lacks.
- [x] 4.6 Make `mcp-tool-invoked-via-tui` (Antigravity) and every MCP execution check assert the tool's nonce result, not a final text that is served after any function response.
- [ ] 4.7 Make the TUI checks assert UZE-delivered content (a skill's or server's nonce name and description), not chrome:
  - `plugin-surface-in-tui`
  - `skills-surface-in-tui`
  - `skills-list-opens`
  - `mcp-surface-in-tui`
  - `mcp-server-connected-in-tui`
  - `names_server`
- [x] 4.8 Make `synthetic-credential` (Codex) and `session-start-session-answered` assert something only the subject produces.
- [x] 4.9 Make every `parity-*` check hold only when the native side passed its own presence check, and make the parity MCP probe run, not skip.
- [x] 4.10 Replace the checks on UZE's own output with harness observations, or move them to the deterministic suite where they belong:
  - `policy-sidecar-delivered`
  - `mcp-server-configured`
  - `session-start-entry-delivered`
  - `skill-harness-started-without-the-shim`
- [x] 4.11 Retarget or delete the checks on routes UZE no longer uses:
  - `isolation-declaration-reaches-model`, which uses hand-written bridges;
  - Antigravity `plugin validate` of the plugin `hooks.json`, and `hooks-delivered-hooks-loaded`, so that they measure the route UZE delivers through;
  - `context-agents-md-reaches-model`, so that it proves UZE's reconciled context rather than a hand-written file.
- [x] 4.12 Prove Claude MCP tool execution, which is PARTIAL today, or declare it with the measurement. The MCP contract now runs the delivered server's tool (`mcp-tool-executed`, the fixture server's proof read back from the next request); Claude loads it with `ToolSearch` first (deferred, 2.1.290).
- [x] 4.13 Codex MCP execution: measure with `--discovery` how code mode offers a server's tool to the model (a nested tool inside `exec`'s description), then give `CodexBindings.mcp_calls` the call a model makes, so `mcp-tool-executed` runs there too; OpenCode and Antigravity move their own execution phases into the contract the same way. Measured 2026-10-06 (`experiments/codex/mcp-offer`): with `/mcp` showing `uze-conformance: connected (1 tool)`, the user turn's request carries no MCP tool at all, not even the 9 of Codex's own `codex_tui` server, and declares no `tool_search`; suspected Lab cause: the provider's hand-written model metadata (the Codex counterpart of 5.9). Next: find the condition in codex-rs code mode, or record a real account's model metadata as a discovery. Resolved 2026-10-06: the tool was offered all along, as a deferred nested tool (gpt-6.1-sol's bundled metadata: `tool_mode: code_mode_only`, `supports_search_tool: true`, so MCP tools register Deferred and only `ALL_TOOLS`/`tools` carry them; `/mcp` lists a threadless connection set of its own). `CodexBindings.mcp_calls` scripts the `exec` a model writes, the person allows the call on screen, and `mcp-tool-executed` reads the server's proof back. A direct-mode model (`o3`) gets the `mcp__uze_conformance` namespace, with a hand-configured server as the control.

## 5. The Lab answers no prompt a user would meet

- [x] 5.1 Measure in the sandbox, per harness, every prompt a user meets after `uze install`, and where the vendor records the answer:
  - Codex folder trust, hook review per hash, and whether trust gates `exec` and project `.agents/skills`;
  - whether Codex hooks are now on by default;
  - Claude folder trust and permission prompts;
  - Antigravity folder trust, MCP permission and terms;
  - OpenCode permission prompts.
- [x] 5.2 Add the "prompts the Lab answers" section to `conformance/DECISIONS.md`. Admit only prompts outside UZE's scope: synthetic sign-in, the API-key dialog, vendor onboarding unrelated to delivery, auto-update. Each answer in code carries `# decision: <id>`, enforced by the lint (2.7).
- [x] 5.3 Remove every answer to a delivery-affecting prompt (`inventory.md`, Sweep A):
  - Codex `--dangerously-bypass-hook-trust` and `[features] hooks = true`;
  - folder trust answered by keys on all four;
  - Claude `--permission-mode bypassPermissions`;
  - OpenCode `run --auto`;
  - Antigravity `--dangerously-skip-permissions`, and the seeded `trustedWorkspaces` and `permissions.allow`;
  - `--skip-git-repo-check` where a user's project is a repository.
- [x] 5.4 Answer those prompts through each harness's own interface, in a shared helper per binding (for example Codex's hook-review key). Measure the added time per leg, and split legs (`--part`) if a leg exceeds its budget.
- [x] 5.5 Add a first-session scene per harness: `uze install`, then start the harness as a user would. Assert UZE's report while each prompt is unanswered, answer it through the interface, then assert delivery. `contract/first_session.py`: `uze status -m` read before the harness opens and after every prompt is answered on screen, held against the harness's own trust record (Codex `[hooks.state]`); green on all four.
- [x] 5.6 Make the OpenCode phases run against the run's `UZE_HOME` (`scenarios.py:44` vs `:76,253,406`; `bindings.py:20`), so the harness sees what was installed.
- [x] 5.7 Remove the Lab-authored Antigravity `lab-dispatcher` agent (`bindings.py:130`). Prove agent dispatch through what a user's default agent has, or declare the limitation with its measurement.
- [x] 5.8 Make the container apply the isolation `Dockerfile:63-69` claims (read-only root, `cap_drop ALL`), or correct the claim to what `common.py:979` actually applies, with the reason.
- [ ] 5.9 Have the Antigravity provider serve what a real account receives for `fetchAdminControls` and the feature flags, from a recorded discovery, instead of `{}` and a hand-set `json-hooks-enabled`.

## 6. Routes and lifecycle

- [ ] 6.1 Add fixture packages per route: the package's own envelope per harness; one with `.codex-plugin/plugin.json` beside a root `plugin.json` carrying the Agent Plugins `$schema`; and the generated envelope. Assert which capabilities each harness loaded and that each was loaded once.
- [x] 6.2 Exercise update: change a package's skill, agent and hook. Assert from the harness's listing and filesystem that the change reached it, and that a hook's trust prompt reappears where the harness asks again. `contract/lifecycle.py` update scene: the updated skill and agent reach the model and the old ones do not, on all four (Antigravity's agent half declared, its default agent being offered no agent).
- [x] 6.3 Exercise removal: assert from the harness's listing and filesystem that nothing UZE delivered is still loaded, and that foreign entries survived. `contract/lifecycle.py` removal scene: nothing offered, no residue where the harness loads from, the untouched plugin still offered; green on all four.
- [x] 6.4 Exercise context limits: an `AGENTS.md` over each harness's measured limit, and a project with no `.agents`. `contract/context.py` long `AGENTS.md`: what UZE says agrees with what the harness read; found Codex's 32 KiB `project_doc_max_bytes`, now reported.
- [ ] 6.5 Exercise agent restrictions that UZE carries (`tools:` where delivered) from the harness's behaviour.
- [x] 6.6 Run all four verticals. The red set must equal the list from 0.2. Investigate every difference before group 7, using the `conformance-debug` skill's loops. 2026-10-06, image 1b55d75e: Claude 2.1.291 131 pass, Codex 0.160.1 104, OpenCode 2.0.24 89, Antigravity 1.2.17 99 + vendor leg 28, no red left; every red in `expected-red.md` is fixed or a registered, measured declaration.

## 7. Delivered, held back by the harness

- [x] 7.1 Add the held-back report: an `IntegrationPort` method returning `HeldBack { capability, action }` notes beside the receipt (the receipt stays `Matched`, D5). Render it through `uze-application`'s read models in the install report, `uze status`, `uze inspect` and `uze doctor`, with human and JSON parity.
- [x] 7.2 Codex: read the vendor's hook trust record (measured in 5.1) and report each UZE hook whose current content has no recorded trust as pending review, with the action. Do this read-only. An unreadable record reports "trust unknown", never delivered.
- [x] 7.3 Add context reachability per harness to the integration's context delivery: Codex project trust; a setting turning the context file off (Claude's `agents-md` plugin disabled, `instructionFiles`); the measured size caps (Antigravity 24 KB). Render it in `uze status`.
- [x] 7.4 Report MCP permission prompts that hold a delivered server back, where a harness has them (Antigravity `mcp(*)`), the same way. Measured on 1.2.17 with the Lab's `mcp(*)` seed removed: the server is not held back; agy asks before each call, like any tool, and the person answers it in place (`mcp-tool-invoked-via-tui`), so there is nothing waiting to report.
- [ ] 7.5 Make the first-session scenes (5.5) assert 7.2–7.4 against the harness's own state, and add deterministic tests for the rendering.

## 8. Delivery fixes (each turns a check from 0.2 green)

- [x] 8.1 OpenCode V2 hook vocabulary: `shell`/`command`, `read`/`write`/`edit` with `path`, `websearch`, `subagent`, from the capture. Correct the binding's "never captured" comment to the capture it now cites.
- [x] 8.2 OpenCode V2 skills: stop writing `slash`. Make the route reason say that `invoke.user=false` is not enforced on V2, and rewrite `bindings.py:182` from the measurement.
- [x] 8.3 OpenCode V2 agents: replace the V1 dialect (`tools`, `permission`, `temperature`, `top_p`) with V2's (`permissions`, `steps`, `hidden`, `disabled`, `request`, `system`). Name a legacy field as not carried.
- [x] 8.4 Claude: bind `agent.spawn` to `Agent`, keep `Task` only while the capture declares it, and drop `MultiEdit` from `file.edit`.
- [x] 8.5 Claude: emit `decision`/`reason` for PostToolUse and Stop, and `permissionDecision` only for PreToolUse. 3.6 proves it. Measured on 2.1.290: Claude renders every PreToolUse denial as a hook error whatever the dialect (`experiments/claude/deny-render`), so `hooks-deny-not-reported-as-error` is a registered declaration; the JSON dialect stays, since it shows the reason without the handler's path.
- [x] 8.6 Codex: answer "which manifest Codex reads" in one function (D6), and compute explicit-route coverage and the mirror from it. 6.1 proves no double MCP registration.
- [x] 8.7 Codex: stop documenting `[features] hooks` as required if 5.1 measured hooks on by default (`codex/README.md:28`).
- [x] 8.8 Antigravity: correct the stale rationale the survey disproved:
  - "install merges, stale files survive" (since 1.1.28);
  - `--skip-path`/`--skip-aliases` unavailable;
  - the context file list (`AGENTS.md` global paths, 1.2.15).
- [x] 8.9 Sweep every integration's FACTS/README/evidence text for claims the captures contradict, and correct each with its source.
- [x] 8.10 OpenCode preferences: measure what V2 reads for autonomy and sandbox. Measured from source (`v2` at b78d10cd7): `config/normalize.ts` still reads `opencode.json`'s top-level `permission` map and turns it into V2 rules (`bash` becomes `shell`, `write`/`patch` become `edit`), and V2's own `permissions.mdx` documents `permission` as the configuration; every action UZE writes (`edit`, `bash`, `webfetch`, `websearch`, `external_directory`) is one V2 evaluates. The legacy note belongs to an agent's frontmatter (8.3), not to the configuration, so the preferences are left as they are.
- [ ] 8.11 A Lab scene that a preference changes what OpenCode asks: a profile set through the workspace's profile editor (the only place a person applies one) denies the shell, and a scripted shell call is refused before it runs.

## 9. Climb toward native, measured first

- [ ] 9.1 Antigravity: experiment with a receipt-owned `~/.gemini/config/plugins.json` entry pointing at UZE's generated directory. If skills, MCP and hooks load in place, replace the staging copy and import-manifest dependency. Otherwise record the measurement.
- [ ] 9.2 Antigravity: measure plugin `rules/AGENTS.md`. If it reaches the model, deliver `Instruction` through the generated plugin and retire its `unsupported`.
- [ ] 9.3 Antigravity: re-measure plugin-bundled `hooks.json` on the current release. If they fire, move hooks into the generated plugin.
- [ ] 9.4 OpenCode: rerun `experiments/opencode/permission-evaluate` on the current release. If `resources` or `source.id` carries the call's input, deliver deny and ask natively and retire the declarations.
- [ ] 9.5 Codex: measure a UZE package as a native Agent Plugin (root `plugin.json` plus `hooks/hooks.json`). If it holds, ship hooks inside the generated plugin instead of merging into `~/.codex/hooks.json`, and decide the explicit/generated route precedence.
- [ ] 9.6 Claude and Codex: measure `updatedInput`. Where it is native, claim `transform` under the existing contract, with Lab rows.
- [ ] 9.7 Claude: read `plugin install|uninstall --json` and surface `errorDetails` from `plugin list --json` in inspection as drifted or blocked.
- [ ] 9.8 Each adoption or decline in 9.1–9.7 records its measurement in `conformance/DECISIONS.md`.

## 10. Documentation and the loops

- [x] 10.1 Update `conformance/README.md` and the `conformance-debug` skill: measured vocabulary, nonce markers, presence proofs, measured declarations, no answered prompts.
- [x] 10.2 Update `docs/capabilities/portable-hooks.md` (the alias matrix per harness, from the captures) and each `crates/uze-integrations/src/*/README.md`.
- [x] 10.3 Add to `docs/architecture/invariants.md`:
  - "bindings are a subset of the measured vocabulary" (1.8);
  - "every bound alias has a Lab row" (3.7);
  - "no constant verdicts or unanswered-prompt bypasses" (2.7);

  each with its test.
- [x] 10.4 Fold what this change learned into `.agents/skills/harness-watch/SKILL.md`: the tool snapshots as the baseline to diff against a release.
- [x] 10.5 Run `make check`, `openspec validate --all --strict` and the Lab's pytest suite. `make check` green, `openspec validate --all --strict` 50/50, the Lab's unit tests and ruff green.

## 11. Proof

- [ ] 11.1 Three consecutive clean nightlies of all four verticals with the full red list from 0.2 turned green and no `*` in the registry.
- [ ] 11.2 Run the `harness-watch` survey again against the captures. Every finding is either covered by a check or recorded as watch.
- [ ] 11.3 Validate by hand on the operator's machine, per harness: install a hook package, see the pending state where the harness asks, answer it in the harness, and see the hooks fire.
