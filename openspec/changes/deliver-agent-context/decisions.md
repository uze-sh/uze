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
