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
