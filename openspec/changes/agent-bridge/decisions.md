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

## D7 — An agent event reaches every source naming it, whatever checkout is in front

- status:   pending
- task:     3.2
- commit:
- trigger:  A `paths.changed` or `checkout.changed` event asks a source only for the checkout it happened on; an agent is the workspace's, and a HUD is about every agent.
- decided:  `EventName::Agent(AgentEvent)` is delivered through `Extension::agent_events`, apart from the checkout watch, to every source whose `refresh-on` names it, with `input.event` carrying `name`, `agent`, `harness`, `checkout` and the report's detail (`tool`, `path`, `call`, `outcome`). The extension's `events` log keeps them like any other.
- rejected: Filtering by the checkout in front: a band about agent B would not hear B finish while A is in front.
- blast:    `uze-extensions::declared`, `contract::Extension`.

## D8 — An unattended prompt is fixed text, decided in the decoder, aimed at the agent that fired it

- status:   pending
- task:     4.2
- commit:
- trigger:  D16 forbids an unattended prompt from carrying anything an answer, an event or the repository wrote.
- decided:  `agent-prompt … when="agent.<event>"` needs `agent:prompt:auto` and is refused by the decoder when its text or a skill argument holds a `{field}`; the engine queues it on that event, aimed at the agent the event names, and the host takes it through `Extension::requests()`. The parent's "closed set its argument is taken from" is left as `AutoPrompt.values`, empty until an extension needs one.
- rejected: Checking the fixedness at run time: a manifest that can never send its prompt should not install.
- blast:    `uze-contract::{AutoPrompt, PromptText}`, the decoder's `agent_prompt`.

## D9 — The engine resolves an agent action into a request and never asks first; the host shows it

- status:   pending
- task:     4.2
- commit:
- trigger:  The engine's own question (confirm) would show the prompt, and then the host must show it again before typing it (byte equality with what was shown).
- decided:  `Outcome::Agent(AgentRequest { what: Prompt{text} | Skill{skill,args} | Restart | Focus, agent, label })`, with the text cleaned and resolved against the item; `question_for` returns none for it, and the host owns the one confirmation. Until 4.3 lands the host refuses every request with a toast naming it: nothing types into an agent before the bridge's guards exist.
- rejected: Letting `confirm=` stand for the bridge's showing: the engine's question is drawn in the extension's own view, which an extension controls.
- blast:    `contract::{Outcome, AgentRequest, AgentAsk}`, `src/ui/orchestrator/declared/agent.rs`.

## D10 — Journals ride the checkout watch, read only while an enabled extension observes

- status:   pending
- task:     3.1
- commit:
- trigger:  The client already reads every second off the drawing thread (`spawn_checkout_watch`); a second clock and a second thread for the journals would be the same cost twice.
- decided:  `ExtensionWatch.journals` carries each agent's id and offset; the watch thread asks `Workspace::reports_since` (a `trace` span) and answers with the reports and the next offset. Nothing is read when no enabled extension holds `agent:observe`. A first read only sets the offset: what an agent did before the client looked is not an event. A journal halved under its bound is re-read from its start, which can deliver up to half of it again; accepted, since a report moves only what is drawn.
- rejected: A file watcher per journal: another thread per agent for a one-second answer.
- blast:    `src/ui/orchestrator/{reads.rs,declared/{events,bridge}.rs}`.

## D11 — Paint stands in for the turns of an agent that has reported nothing this session

- status:   pending
- task:     3.1
- commit:
- trigger:  OpenCode and Antigravity have no reporter (D3), and a Claude Code or Codex agent launched before this release carries none either.
- decided:  Each watch tick compares each agent's working state (the activity model's) with the last; a change is `agent.turn.started` or `agent.turn.finished` with `detail.source = "paint"`, until the first report from that agent arrives, after which only its reports count. The `agents` source says per agent whether it `reports`.
- rejected: Merging both always: a harness that reports would be told twice per turn.
- blast:    `declared/bridge.rs`.

## D12 — An event carries a scrubbed tool name and only a path inside the agent's checkout

- status:   pending
- task:     3.1
- commit:
- trigger:  A report's fields come from anything running in the agent's pane.
- decided:  The tool keeps `[A-Za-z0-9_:.-]`, 64 at most; a path is kept only when it names something inside the agent's checkout, made relative to it (an absolute path outside, or a relative one climbing out, is dropped). The call id and outcome are carried as the journal bounded them.
- rejected: Passing the journal's fields through: an extension would learn paths outside the checkout it was consented for.
- blast:    `declared/bridge.rs::happened_of`.

## D13 — The `agents` source is every agent across the open spaces, handed each pass

- status:   pending
- task:     3.3
- commit:
- trigger:  The parent's 14.4: "the host hands the activity model's agents across open spaces each pass".
- decided:  Each agent is `{id, harness, label, space, checkout, state: working|done|ready, reports}`; the engine answers its `agents` sources only when the value changed, so an unchanged workspace asks nothing again.
- rejected: Per checkout: a HUD is about all of them.
- blast:    `declared/bridge.rs::agents_value`, `Extension::agents`.

## D14 — A skill is spelled from the harness's invocation prefix, carried on `AgentIdentity`

- status:   pending
- task:     4.1
- commit:
- trigger:  Task 4.1 asks for `IntegrationPort::skill_invocation(skill, args)`; the TUI cannot ask an integration (it names none), and building the application on the frame to ask is a read on the drawing thread.
- decided:  No new port method: `invocation_prefix` (`/`, `$`, `@`) already is each harness's spelling, and its doc now says the workspace types with it. `AgentIdentity.skill_prefix` carries it, and `uze_application::skill_invocation(prefix, skill, args)` composes `<prefix><skill> <args…>`.
- rejected: A port method returning `String` per harness: four identical bodies today.
- blast:    `AgentIdentity`, `IntegrationPort::invocation_prefix`'s doc.

## D15 — A prompt is shown in a sealing dialog and typed by the session, guarded twice

- status:   pending
- task:     4.3
- commit:
- trigger:  B5: shown first (text, agent, extension), typed through `submit_notice` only into a ready agent whose harness is the pane's foreground and with no approval pending; refused, never queued.
- decided:  `ask_of_agent(owner, request)` refuses at once what can never go (paused, no agent, a control character or line break, a leading `! / # @ $` outside a skill's spelling) and otherwise opens `AgentWriteDialog`, sealing the client as the extension offer does. Allowing hands a `Write` to the session, which on the same pump checks the pane's harness (`agent_for_tab`), a pending approval (from `approval.asked` until the agent's next event), `may_submit` and the rates, then types it or drops it with an attributed notice. A request with no `agent` aims at the agent in front. A second request while a dialog is open is refused. `$` joins D16's list: it is Codex's command character.
- rejected: Typing from the model: only the session holds the pane's stream.
- blast:    `src/ui/orchestrator/declared/agent.rs`, the session's pump, keys, mouse and render.

## D16 — An unattended prompt's 24-hour allowance lives in the client, not on disk

- status:   pending
- task:     4.4
- commit:
- trigger:  The spec: `agent:prompt:auto` "accepted per project and agent for 24 hours and revoked by any change of the extension's files".
- decided:  The first unattended prompt from an extension to an agent opens the dialog in its unattended form; allowing it lets that extension prompt that agent on its own for 24 hours of this client's life. Not persisted: an agent id is per launch, so a record would outlive what it names, and a recompile replaces the engine anyway. A client restart asks again.
- rejected: A record in `state/extensions.json` keyed by agent id: generated ids, written every day, for an allowance that a restart asking again costs one dialog.
- blast:    `Writes::unattended_until`.

## D17 — The rates, and F8 as the pause key

- status:   pending
- task:     4.4
- commit:
- trigger:  B5's numbers, and a key that works over any pane.
- decided:  Per extension and agent: one write per turn (turns counted from `agent.turn.started`, reported or painted) and one per 60 s; per agent: 3 a minute, 20 an hour, 10 s between unattended writes. `Action::PauseAgentWrites` on F8 (Workspace scope, in the index) toggles the pause, resets every count and drops whatever was allowed and not yet typed.
- rejected: An `alt+` letter: an agent's prompt reads those.
- blast:    `uze-keys`, `declared/agent.rs::Rates`.

## D18 — `agent-restart` is refused until restarting a conversation is a workspace operation

- status:   pending
- task:     4.2
- commit:
- trigger:  Nothing in the workspace starts an agent's harness again on a new conversation; switching harness is the nearest, and it carries the conversation.
- decided:  The manifest accepts `agent-restart` under `agent:restart`, and the host refuses it with a notice saying it is not available yet. `focus-agent` selects the agent's tab.
- rejected: Killing the pane and launching anew from the extension path: a lifecycle operation built for one caller.
- blast:    `declared/agent.rs::ask_of_agent`.
