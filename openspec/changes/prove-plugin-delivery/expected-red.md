# Expected red at the end of group 6

Task 6.6 runs every vertical against the hardened Lab. Each claim below must
be red then, and the fix named beside it must turn it green. A red result
that is not listed here, or a listed claim that comes up green, gets
investigated before group 7 starts.

Each claim is filled in with its check name once the check exists.

| claim | harness | source | check | fixed by |
|---|---|---|---|---|
| Every bound native tool and field is declared by the harness | OpenCode | B1 | | 8.1 |
| Every bound native tool and field is declared by the harness | Claude Code (`Task`, `MultiEdit`) | B2 | | 8.4 |
| A `shell`-matched group fires on a real shell call, with `HOOK_COMMAND` | OpenCode | B1 | | 8.1 |
| An `agent.spawn`-matched group fires on subagent dispatch | Claude Code | B2 | | 8.4 |
| `file.*` / `search.*` groups fire with their portable fields | OpenCode | B1 | | 8.1 |
| A PostToolUse/Stop deny blocks without a hook-error notice | Claude Code | B8 | `hooks-post-deny-no-hook-error` | not reproduced on 2.1.290: the PostToolUse denial reaches the model with no error shown (the PreToolUse one does, below) |
| UZE reports hooks pending review before the user trusts them | Codex | B3 | | 7.2 |
| Hooks run after the user trusts them through the interface | Codex | B3 | | (Lab only, 5.4) |
| UZE reports project context not reaching an untrusted project | Codex | B4 | | 7.3 |
| An explicit-route package loads each capability exactly once | Codex | B5 | | 8.6 |
| A delivered skill file carries only fields V2 defines | OpenCode | B6 | | 8.2 |
| A delivered agent file carries only V2 fields | OpenCode | B7 | | 8.3 |
| The phases see the packages installed into the run's UZE home | OpenCode | sweep A | | 5.6 (Lab) |
| An MCP server held behind a permission prompt is reported as pending | Antigravity | sweep A | | 7.4 |

## Found by the measurement, beyond the survey

Each of these was found by the Lab's own capture and census on 2026-10-05,
after the vendor survey.

| claim | harness | evidence | check | fixed by |
|---|---|---|---|---|
| `file.*` and `search.*` are bound to tools the harness offers | Codex | 0.160.1 declares no `Read`/`Write`/`Edit`/`Grep`/`WebSearch` and no `exec_command`: only `functions.exec` (freeform) and `functions.wait` | `vocabulary-*` | 8.x (Codex vocabulary from the census) |
| `search.files` is bound to a tool the harness offers | Antigravity | 1.2.17 declares no `grep_search` | `vocabulary-search.files-*`, Rust `measured_tests` | 8.x |
| `agent.spawn` and `agent.message` fire on the harness's agent tools | Antigravity | 1.2.17 declares `invoke_subagent` and `send_message`; UZE binds neither | `hooks-row-agent.spawn`, `hooks-row-agent.message` | 9.x (capability) |
| An allowed shell call runs under the default permission mode | Claude Code | 2.1.290 defaults to auto mode, whose classifier the provider answered wrongly ("Classifier unavailable") | `hooks-allow-tool-ran`, `hooks-event-post_tool_use` | Lab (provider answers the classifier) |
| A hook's denial is shown as a decision, not as a failed hook | Claude Code | 2.1.290 renders every `PreToolUse` denial as "PreToolUse:Bash hook error: <reason>": the documented `permissionDecision: deny` JSON on exit 0 as much as exit 2, and from a hand-written hook with no UZE in it (`experiments/claude/deny-render`). UZE answers in the JSON dialect, which shows the reason without the handler's path | `hooks-deny-not-reported-as-error` | declared (vendor limit, registered with its measurement); 8.5 keeps the JSON dialect |
| A command guard sees the command it guards | Codex | 0.160.1 reports shell calls to hooks as `Bash` with `{"command"}`; UZE reads `cmd`, so `HOOK_COMMAND` is empty and a deny guard keyed on it lets the denied command run (measured by the census and the deny scene) | `hooks-row-shell`, `hooks-deny-*` | 8.x (Codex vocabulary from the census) |
| A file write or edit reaches its group with the portable alias | Codex | writes and edits reach hooks as `apply_patch` with the patch in `command` and no path field; `HOOK_TOOL` and `HOOK_PATH` arrive empty | `hooks-row-file.write`, `hooks-row-file.edit` | 8.x |
| `agent.spawn` reaches its group with the portable alias | Claude Code | the group fires on `Agent` (the `Task` matcher still matches), but `HOOK_TOOL` arrives empty | `hooks-row-agent.spawn` | 8.4 |
| An `ask` group asks the person, with the reason, and approving runs the call | Antigravity | 1.2.17 shows no prompt for UZE's `ask` decision and never runs the call: `ask` behaves as a silent deny (measured, `hooks > ask`) | `hooks-ask-prompted-with-reason`, `hooks-ask-approved-tool-ran` | 8.x / 9.x (measure the vendor's ask dialect) |
| Preferences reach OpenCode in its current dialect | OpenCode | Not a red: V2 reads `opencode.json`'s `permission` map and maps every action UZE writes (`bash` to `shell`); the legacy note is the agent frontmatter's (8.3). Measured from source, not yet in the Lab | to add (8.11) | 8.11 |
| A person's default agent can dispatch a delivered agent | Antigravity | 1.2.17 offers `invoke_subagent` only to an agent whose definition lists it; the Lab's own `lab-dispatcher` agent hid this until 2026-10-06 | `agent-*-dispatch-delivers-body` (declared) | 9.x (deliver a dispatching agent, or document it) |
| UZE says when a long `AGENTS.md` is only partly read | Codex | 0.160.1 reads the first `project_doc_max_bytes` (32 KiB by default) and drops the rest; UZE reported nothing | `context-long-report-agrees` | fixed (`CodexIntegration::context_unread`, `trust::project_doc_max_bytes`) |
| A delivered agent is reachable from the agent a person starts on | Antigravity | 1.2.17 offers no delivered agent to the default agent, which has no `invoke_subagent`; the Lab's own `lab-dispatcher` hid it, and UZE reported the delivery Native | `agent-*-exposed` (declared), `agents::canonical_agent_routes_natively_except_where_the_default_agent_cannot_reach_it` | fixed (route Degraded, with the reason) |
| An optional tool is judged in the session that scripted it | Claude Code (Lab) | `Grep` read as offered to the person's session because the `parity` experiment's agent lists it in its own definition; the measurement took the run's union | `vocabulary-search.files-scriptable` | fixed (Lab: `vocabulary.evaluate` reads the scripted call's refusal; the snapshot no longer compares an optional tool's presence) |
| Every alias a harness knows is exercised by the rows scene | Codex (Lab) | the `hook-rows` fixture had no `agent.message` group, so `hooks-row-agent.message` recorded nothing and read as Codex not firing | `hooks-row-agent.message`, `RowsFixtureTest` | fixed (fixture) |
| A group whose aliases the harness cannot fire is reported, not delivered | all | UZE delivered `agent.spawn`/`search.files` groups on Antigravity with the alias itself as matcher, which never fires | `hooks::tests::a_group_that_could_never_fire_here_is_reported_not_delivered` | fixed (hooks.rs `unbound_only`) |
