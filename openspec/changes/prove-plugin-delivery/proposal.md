## Why

On 2026-10-05 every conformance leg passed against Claude Code 2.1.289,
Codex 0.160.0, OpenCode 2.0.23 and Antigravity 1.2.17. On the same day, a
survey of the vendors' changelogs found that UZE delivers hooks that never
run on three of the four harnesses:

- **OpenCode:** every hook group with a matcher names V1 tools.
- **Claude Code:** `agent.spawn` is bound to the old `Task` tool name.
- **Codex:** every hook waits for a trust review that UZE never mentions.

Codex also drops project context in projects the user has not trusted, and
may count the wrong manifest for packages delivered on the explicit route.
None of this made the Lab fail. The Lab has looked at the vendors only
through UZE's own beliefs, and in five ways:

1. Tool names come from UZE's own tables. Nothing compares them with the
   tools the harness actually declares.
2. Each harness fires one alias, `shell`, on one event.
3. Checks pass without the behaviour they claim. "Marker absent" holds
   when no hook ran. "Turn settled" accepts an `Unknown tool` error. One
   OpenCode check is `check(True)`.
4. Declared limitations never expire:
   - Registry entries carry `versions: ["*"]`.
   - The contracts declare with `kind="adapt"`, a kind the gate does not
     read.
   - The recorded evidence is never refreshed, so a `VERSION DRIFT` is
     printed every night and never acted on.
5. The Lab answers the vendor prompts a user would meet: folder trust, hook
   trust, feature flags. Interactive installation (#172) was the same
   failure. That lesson was written down for journeys and never for the
   Lab.

Plugin delivery is what UZE sells. A green Lab has to mean the plugin works
on a user's machine, or it protects nothing.

## What Changes

**The Lab measures the vendor, not UZE's beliefs about it.**
- Every run captures the tools the real harness declares. Every native tool
  name and input field a hook binding uses must appear in that capture, or
  the run fails.
- The Lab's provider scripts tool calls only from captured names, never from
  a list kept by hand.

**Every claimed delivery is exercised.**
- Every portable alias and every event × effect that UZE reports as
  delivered to a harness is fired by a real tool call. The handler's
  `HOOK_*` values are observed coming back.
- What a harness does not deliver is a declared, version-pinned limitation.
  It is never silently left out.
- Every delivery route has a fixture: the package's own envelope, the
  generated envelope, and capability by capability. That includes a package
  whose root `plugin.json` carries the Agent Plugins `$schema`.

**No check passes on a turn where nothing happened.**
- A check of absence holds only once a paired check of presence has proven
  that the subject ran.
- "Turn settled" never accepts a tool error as success.
- A hard-coded verdict is refused.
- A lint run in the deterministic suite enforces all three.

**Declarations are measured on every run.**
- Every declared limitation is a registry entry pinned to the harness
  versions where it was observed, with no `*`.
- The contracts and the gate share one kind.
- A declaration is the outcome of a measurement taken in that run, never a
  constant, so a vendor release can neither silently keep one alive nor
  silently retire one.
- On a version the registry does not name yet, a limitation that still
  reproduces passes, and the run publishes the re-pinned registry with its
  evidence.

**The Lab answers no prompt a user would meet.**
- The Lab may answer only a prompt that is outside UZE's scope, such as
  sign-in to the synthetic provider. Each such answer is recorded with its
  reason in `conformance/DECISIONS.md`.
- Each harness has a first-session scene, run the way a user starts after
  `uze install`. It asserts what UZE reports while a vendor prompt is still
  unanswered. It then answers through the harness's own interaction and
  asserts what is delivered afterwards.

**Delivery fixes found by the survey**, each one red in the Lab before it
turns green:
- OpenCode V2 tool vocabulary (`shell`, `path`, `websearch`, `subagent`).
- OpenCode V2 skill and agent dialects: no `slash` field, no V1 agent
  fields.
- Claude `agent.spawn` → `Agent`, and the `MultiEdit` matcher removed.
- Claude PostToolUse/Stop decision JSON in Claude's own schema.
- Codex hook trust and project trust reported by UZE.
- The Codex root `plugin.json` precedence on the explicit route.
- Antigravity's plugin-hook measurement made against the route UZE actually
  uses.
- Every comment and declared reason the survey proved false, corrected.

**Climbing toward native, decided by measurement.** Each item below is first
measured in the now-trustworthy Lab and then adopted or declined with that
evidence:
- Antigravity `plugins.json` in-place registration, and plugin
  `rules/AGENTS.md` for instructions.
- OpenCode `permission.evaluate` for deny/ask.
- Codex hooks shipped inside the plugin, instead of merged into
  `~/.codex/hooks.json`.
- Claude and Codex input transformation (`updatedInput`).
- Claude's plugin CLI `--json` output and `errorDetails` read by inspection.

**A recurring vendor watch.** The `harness-watch` skill
(`.agents/skills/harness-watch/`) is the human loop that re-asks UZE's
beliefs on every vendor release. The Lab is the machine loop.

This change absorbs open work in other changes:
- `native-first-hooks` tasks 6.2–6.4: a Lab case per alias row, OpenCode
  `permission.evaluate`, and Codex `ask`.
- The hook-semantics group of `extend-conformance-coverage`.

Those tasks are marked as moved here.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `local-real-harness-conformance`: adds these requirements:
  - The vocabulary is measured from the real harness.
  - Every claimed delivery is exercised.
  - Checks prove presence.
  - Declarations are version-pinned and measured on every run.
  - The Lab answers no prompt a user would meet, and each harness has a
    first-session scene.
- `portable-hooks`: adds these requirements:
  - A portable alias fires on the harness's current tool of that kind.
  - A hook the harness will not run until the operator acts is reported as
    pending, with the action, and is never reported as delivered.
- `delivery-reporting`: adds a requirement that delivered project context
  the harness will not load in the current project is reported with the
  reason and the operator's action.
- `package-delivery-fidelity`: adds these requirements:
  - The manifest a harness reads is the one UZE computes coverage from.
  - Delivered files use only fields the harness's current dialect defines.

## Impact

- **`conformance/`:**
  - Providers: tool-name capture, and calls scripted from the capture.
  - Fixtures: per alias, per event × effect, per delivery route.
  - Every hook phase and contract (`contract/` gains hooks).
  - `gate.py` and `evidence/expected.json`.
  - `shared/common.py` (check kinds, presence pairing).
  - `DECISIONS.md`, `README.md`, `entrypoint.sh`, and each
    `harnesses/<vendor>/scenarios.py` (prompts answered for the user are
    removed).
- **`crates/uze-integrations`:**
  - The `ToolBinding` tables of `claude`, `opencode`, and possibly `codex`
    and `antigravity`.
  - OpenCode skill and agent dialects.
  - Claude hook decision JSON.
  - Codex plugin coverage and trust inspection.
  - Comments and evidence text the survey proved stale.
- **`crates/uze-core` / `uze-application`:** a "pending operator action"
  state for a delivered capability, and context reachability, carried to
  `uze status`, `uze inspect` and `uze doctor`.
- **Deterministic tests:** the binding tables are checked against the
  recorded captures. Lab lints run in `cargo test` / `pytest`.
- **CI:** a run on a version the registry does not name yet publishes the
  re-pinned registry with its evidence. `conformance-stability.yml` keeps
  the promotion gate.
- **Docs:**
  - `docs/capabilities/portable-hooks.md`
  - `crates/uze-integrations/src/*/README.md`
  - `docs/architecture/invariants.md`, for the new guarded properties
  - The `conformance-debug` skill (no answered prompts, presence pairing)
