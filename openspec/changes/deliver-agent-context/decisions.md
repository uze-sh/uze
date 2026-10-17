# Decisions taken while the change is open

## M1. Every channel measured (2026-10-17 spike)

**What was measured.** One Lab experiment per harness, run against the
synthetic providers:
- Claude 2.1.301
- Codex 0.163.0
- OpenCode 2.0.31
- Antigravity 1.4.0, signed in

The experiments are `conformance/experiments/<h>/agent-context.py`, with
shared code in `experiments/agent_context_probe.py`. They are spike files,
not yet on this branch. Evidence was written under
`/tmp/harness-conformance/<h>/run1/experiments/<h>-agent-context/`.

**Verdicts.**
- Claude: 17/17
- Codex: 9/9
- Antigravity: 10/10
- OpenCode: 6/7. The failing check is a control that confirms the known
  shared-server limit; it does not affect this channel.

| Harness, channel | Where it lands | Pane | Resume | Changed content | Copies |
|---|---|---|---|---|---|
| Claude `--add-dir` `CLAUDE.md` | User-role `<system-reminder>` part, separate from the prompt; transcript `attachment: instructions` | nothing | re-read | B added as "Instruction files were re-read… each replaces its earlier copy" | old copy replayed, then B |
| Claude `SessionStart` (fallback) | `role: system` entry in `messages`; transcript `hook_additional_context` | nothing | re-fires (`source: resume`) | B added | old copy replayed, then B |
| Codex `SessionStart` via `-c` and session-layer trust | developer item | nothing; no hook review, no hook line | re-fires | B added | old copy replayed, then B |
| OpenCode `session.hook("context")` | `system` | not observed | read per call, by `sessionID` | B only | none |
| Antigravity `PreInvocation` `ephemeralMessage` | user-role text, separate from the prompt; after a tool call, appended in the `functionResponse` output | nothing | per call | B only | none (dropped after each call) |
| Antigravity `PreInvocation` `systemMessage` | user-role text; transcript `SYSTEM_MESSAGE` | nothing | per call | B added | accumulates per call |

**Other findings.**
- **Codex spills over about 10,000 bytes.** A 12,056-byte document was cut
  to about 10,106 bytes: the head and tail were kept, and a "Full hook
  output saved to: /tmp/hook_outputs/…" line was added. The 9,000-byte
  bound in D1 stands.
- **Antigravity.** Hooks fire headless when signed in on 1.4.0, which
  overturns the 1.1.24 finding. `--add-dir` leaves `workspacePaths[0]` on
  the project.
- **OpenCode plugin loading.** A plugin written into
  `~/.config/opencode/plugins/` while the shared service runs is loaded
  without a restart. A plugin named by a later launch's
  `OPENCODE_CONFIG_CONTENT` is never loaded, so the setup-installed plugin
  in D2 is the right shape.
- **OpenCode session ids.** `opencode run --session <id>` creates the
  session when it is absent, which confirms that the id can be chosen
  before launch.

**Consequences for the design.**
1. **No channel is degraded.** All four are declared, and `FirstTurn`
   remains only as the declared fallback.
2. **Claude's `Directory` also accumulates.** On resume it replays the old
   copy and adds the new one, with Claude's own "replaces its earlier
   copy" note. So `accumulates` holds for Claude and Codex.
3. **Antigravity uses `systemMessage`, given once per process.**
   - `ephemeralMessage` is re-sent on every model call. After a tool call
     it lands in the newest `functionResponse`, outside the cached prefix,
     so every call pays for it in full.
   - `systemMessage` stays in history, where it is part of the cached
     prefix, but it accumulates on every call.
   - The handler gives it once per Antigravity process. It drops a marker
     keyed by its parent pid in the agent's context directory, and later
     calls print nothing.
   - This matches Claude's and Codex's one copy per launch.
   - The marker sits in the generated tier and is swept with the
     directory.
4. **Claude's fallback is not needed.** `Directory` passed on its own,
   with or without `AGENTS.md`.

## D1 — Retire the briefing in `continuity::refresh`, not at launch

- status:   pending
- task:     2.4
- commit:
- trigger:  the design names "the evaluation pass that already reads HEAD", but the first-commit naming lives in `landing::naming` and is asked from the application, not from a per-agent pass that has the agent's checkout and record together.
- decided:  `refresh` (the client's per-agent background read-back) reads `git rev-parse HEAD` in the agent's directory while a briefing with a `head_at_switch` is pending, and retires it once HEAD moved. No briefing pending, no Git read.
- rejected: retiring in `plan` at launch (puts a Git spawn on the shim's launch path, which reads only a few `stat`s); a new pass of its own (a second clock for one field).
- blast:    one function; a briefing would then retire only at the next switch.

## D2 — Own the OpenCode plugin by a content header, written by `install`

- status:   pending
- task:     3.4
- commit:
- trigger:  task 3.4 says "delivered by setup with a receipt", but receipts are per package attachment, and this plugin belongs to no package.
- decided:  `install` (run by setup and by every command that installs) writes `plugins/uze-agent-context.ts` when missing or when the file there starts with UZE's header and differs; a file without the header is the operator's and is never written over, and the channel then reports itself unavailable. The plugin names UZE's session directory in its source, not through the environment the shared server does not share.
- rejected: a receipt in the attachments ledger (the ledger's unit is a package's capability, and a synthetic package would appear in every listing); `OPENCODE_CONFIG_CONTENT` (measured: a running service never loads it).
- blast:    one file and one call; reverting leaves OpenCode on `FirstTurn`.

## D3 — Antigravity prints a pre-rendered payload from an installed script

- status:   pending
- task:     3.5
- commit:
- trigger:  the handler must emit `{"injectSteps":[{"systemMessage":…}]}` with the context JSON-escaped, once per launch, and Antigravity hands a hook's line to `cmd /c` on Windows.
- decided:  the launch writes the step as `context.antigravity.json` beside the context and installs a `deliver` script (`sh`, or `.ps1`) that prints it while `delivered-<launch>` is absent; the hook's line is `sealed_script_line(script)`. Old markers are swept by the next launch.
- rejected: an inline shell line (cannot JSON-escape; `cmd /c` mangles quotes); a `uze` subcommand as handler (puts `uze` on the hook's execution path, which ADR-040 keeps off it).
- blast:    one module.

## D4 — A completed switch says, once, that the agent waits for the operator

- status:   pending
- task:     4.1
- commit:
- trigger:  the design says the switch's toast states it, but a switch raised no toast at all.
- decided:  `AgentPlacement.told` carries a sentence the client raises as a `Told` toast when the placement has no warning: "Continued in <harness>. It holds the briefing and waits for your next message." (or that it was pointed at its briefing, on a degraded harness).
- rejected: no toast (an operator used to the automatic "carry on" would read the quiet pane as a stuck launch).
- blast:    one field and one match arm.

## D5 — The briefing contract is the agent-context contract while the briefing is the only section

- status:   pending
- task:     5.1, 5.2
- commit:
- trigger:  task 5.1 asks for `contract/agent_context.py` beside the briefing checks of 5.2, but the briefing is the only section a context holds in this change, so the two would run the same scene twice.
- decided:  `contract/briefing.py` now proves the four outcomes for the briefing: `briefing-reaches-the-model` (beside the operator's first typed turn), `briefing-is-not-shown`, and `briefing-current-on-a-resume` after the record is rewritten between launches. `portable-agent-memory` adds its section's checks to the same scene.
- rejected: a second contract file over the same scene (twice the Lab time for no new claim).
- blast:    the check names change; nothing else reads them.
