## Why

The workspace launches every agent with exactly the argv and environment
UZE composes: the harness, the launcher in front of it, and what continuity
and the agent context add. An operator who wants one agent on another model,
in plan mode, with an extra directory or with a debug variable has to leave
the workspace and start the harness by hand, which loses everything UZE
gives a managed agent. And the flags an operator reaches for are few and
repeated, so typing them again on every launch is the cost that matters.

## What Changes

- **The new agent's popup takes overrides.** Beside the directory and the
  harness, a launch can carry extra harness flags and extra environment
  variables, chosen in a step of the popup with completion from the
  harness's known flags. The common case stays one question and Enter: no
  override is the default and costs nothing.
- **What UZE owns is never overridden.** An override that would set a
  variable UZE sets or depends on (`UZE_*`, `PATH`, `HOME` and the
  variables an integration reads to deliver and resume), or a flag UZE
  composes for the launch (session, resume, first prompt, agent context,
  configuration keys), is refused as it is typed, with the reason. A flag
  that weakens safety or switches off something UZE delivers is accepted
  and marked.
- **Each harness declares its flag catalog.** An integration names
  every flag its harness's interactive `--help` defines, leaving out only
  those that end the launch or act only in print mode (name, whether it takes a
  value, the values it accepts, a short description, whether it weakens
  safety or switches off delivery) and which of them UZE owns. The catalog
  drives completion and the refusal alike; a flag outside it is still
  accepted as typed.
- **Recent overrides are remembered.** Each flag and variable a launch
  carried is kept per harness, across projects, in a short history the
  popup offers as choices (this project's entries ahead of the rest), so
  one used before is included again in one gesture, one at a time.
- **An agent keeps the overrides it was launched with.** A private
  record holds them, so a respawned pane and a resumed task start the same way; an
  agent continued on another harness drops them, and says so.

- **BREAKING (unreleased surface): the Profiles module is removed
  whole.** The Profiles screen (preference axes for autonomy, sandbox and
  model, written into each harness's global configuration) never left its
  feature flag, and launch overrides answer the same need per launch
  instead of by rewriting `~/.claude/settings.json`, `~/.codex/config.toml`
  and their siblings for every session on the machine. Its domain,
  integration adapters, application service, screen, keys, tests and
  journey go; `state/profiles.json`, which every build has written since
  the overview listed profiles unconditionally, is removed once. Keys an
  operator applied to a harness's configuration stay as they are: no
  receipt says UZE wrote them, so they are the operator's settings now.

## Capabilities

### New Capabilities
- `launch-overrides`: the extra flags and environment an operator gives one
  agent's launch from the workspace — how they are composed with what UZE
  adds, what they may not touch, the per-harness catalog that completes
  and guards them, how they are remembered, and how they survive a
  relaunch.

### Modified Capabilities
- `agent-session-continuity`: `The operator's own invocation is never
  rewritten` gains the case of overrides: they are the operator's, but a
  session or resume flag is not one an override can carry, so continuity is
  never turned off by a launch the workspace composed.
- `agent-harness-switch`: continuing an agent on another harness does not
  carry its overrides over, since flags belong to one harness.
- `cli-performance`: the management read model no longer carries
  profiles.

## Impact

- Removal: `uze-core` `preference` and `profile_state` and their errors and
  path; each integration's `preferences.rs` and `PreferencePort`, the
  registry's preference adapters; `uze-application` `profile.rs`, the
  `Profiles` service, its re-exports and `MachineSnapshot.profiles`;
  `src/ui/view/profiles.rs` and the `Profiles` route, overlays, hits,
  intents and tests; the profile actions and scopes in `uze-keys`;
  `tests/integrations/preferences.rs`; the first-run journey
  `03-a-profile-each-harness-can-read`. `Feature::Profiles` goes, leaving
  `Overview` as the only feature behind the flag.

- `uze-core`: the integration contract gains a flag catalog per harness
  and the set of environment names it manages; no harness named.
- `uze-integrations`: one catalog per harness (Claude Code, Codex,
  OpenCode, Antigravity CLI), kept to each harness's latest release.
- `uze-workspace`: one owner-only record (`state/launch-overrides.json`)
  holding each agent's overrides and the recent entries; the check; `LaunchPlan` carries the
  overrides the shim splices. `agents.json` is not touched.
- `uze-application`: read models for the catalog and the history, a
  validation service, and launches that take overrides.
- `src/ui/orchestrator`: the overrides step of the new agent's popup,
  completion, the history list and the refusal.
- `src/shim.rs`: splices the overrides on an owned launch; the client
  passes them itself when a harness has no shim.
- Conformance: a committed `--help` snapshot per harness backs the
  catalog in a unit test, and the Lab diffs it against the live binary.
