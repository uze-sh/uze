<!-- Decisions taken while implementing, one entry each, judged at archive. -->

## D1 — The journal is a JSON-lines file per agent in the runtime tier, read by offset

- status:   pending
- task:     1.1
- commit:
- trigger:  D16 asks for "a bounded per-agent journal named in `UzeHome`" without a tier, a format or a bound.
- decided:  `runtime/agents/<id>/reports.jsonl` (generated: a report is a claim about now, and losing it costs nothing), one `Report { event, at, tool?, path?, call?, outcome? }` per line, fields cut to 256 characters with control characters removed. Past 256 lines or 64 KiB the newest half is kept (rewritten atomically). The client reads from the byte offset it stopped at, only whole lines, and from the start when the file is shorter than its offset.
- rejected: A record under `state/`: nothing about it is the operator's answer, and it grows by the turn.
- blast:    `uze-workspace::reports`, `UzeHome::agent_reports_path`.

## D2 — `uze agent report` exits 0 whatever happens, saying on stderr why it took nothing

- status:   pending
- task:     1.2
- commit:
- trigger:  The spec says the reporter never blocks a turn, and every `uze agent` verb refuses a process that is not an agent UZE launched.
- decided:  `uze agent report <event> [--tool --path --call --outcome]` verifies the launch claim (`conversation::owner_of`, as `uze agent work name` does) and appends; a refusal (no key, unknown event, unwritable journal) is one line on stderr and exit 0. It is budgeted like the agent surface, measured in `the_agent_surface_meets_the_budget`.
- rejected: Exit 1 on refusal: a hook that fails makes some harnesses show an error to the person on every tool call.
- blast:    `src/cli/agent.rs`, `src/command_performance.rs`.

## D3 — The reporter is `IntegrationPort::reporter`, asked by the agent's launch plan

- status:   pending
- task:     2.2
- commit:
- trigger:  D16 delivers the reporter "through `runtime_contribution`", but that contribution is asked by every shim-mediated launch and knows no agent; the reporter belongs to an agent UZE launched.
- decided:  A new `IntegrationPort::reporter(&ReporterContext { uze, dir })` (default `None`) and `reported_events()`, asked by `continuity`'s launch plan right after the agent's context, with this binary's path. Claude Code writes `reporter-settings.json` in the agent's context directory and names it with `--settings`: hooks for SessionStart, UserPromptSubmit, Stop, Pre/PostToolUse (matcher `*`) and Notification, each running `'<uze>' agent report <event> --hook-input` (5 s timeout). Codex gets `-c notify=["<uze>","agent","report","turn.finished"]`, which Codex calls with the turn's JSON as its last argument. OpenCode and Antigravity report nothing yet; their turns come from the paint heuristic.
- rejected: `runtime_contribution`: it is asked for launches no agent owns, and would need the agent's identity it does not have.
- blast:    `uze-core::integration`, `uze-workspace::continuity`, the Claude and Codex integrations.

## D4 — `--hook-input` and a trailing payload read the tool, path and call from a harness's own JSON

- status:   pending
- task:     2.2
- commit:
- trigger:  Claude hands a hook its event as JSON on stdin; Codex hands `notify` its JSON as the last argument; neither sets the tool's name in an argument UZE could template.
- decided:  `uze agent report <event> [payload] [--hook-input]` reads up to 64 KiB of JSON from stdin with `--hook-input`, or from `payload`, and takes `tool_name`|`tool`, `tool_input.file_path`|`tool_input.path`|`path`, and `tool_use_id`|`call_id` when the flags do not say them. The keys are the hooks' own field names, not a harness's: `src/` still names no harness.
- rejected: One subcommand per harness's hook shape.
- blast:    `src/cli/{grammar,agent}.rs`.

## D5 — The portable hook vocabulary does not grow in this change

- status:   pending
- task:     2.1
- commit:
- trigger:  The parent's 14.2 grows the portable vocabulary with turn start and approval asked/resolved "where a harness offers them".
- decided:  Not done: the reporter is not a portable hook (D3), so nothing here needs the vocabulary to grow, and a plugin's hooks gaining events is a change of its own with conformance behind it. Approval resolved is not reported by any harness wired here.
- rejected: Growing it unused: a vocabulary entry with no consumer and no conformance check.
- blast:    None; the task stays recorded as deferred.

## D6 — The reporter's real-harness proof is the conformance lab's, not yet run

- status:   pending
- task:     2.2
- commit:
- trigger:  The Claude hooks file and Codex's `-c notify=` are built and unit-tested here, but this machine runs neither harness for real in a test.
- decided:  Unit tests cover the generated settings (quoting, matchers, the commands) and the CLI's refusal path; whether Claude merges a `--settings` file's hooks with the operator's and Codex honours `-c notify=` on this version is left to the conformance lab (`conformance/`), recorded as unproven until then.
- rejected: Claiming them proven from the docs.
- blast:    A harness that ignores the flag reports nothing; turns still come from the paint heuristic.
