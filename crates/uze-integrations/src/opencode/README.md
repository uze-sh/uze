# OpenCode Integration

OpenCode does not consume any external plugin envelope — there is no
`.opencode-plugin/plugin.json` equivalent UZE reads. It decomposes every
package into individual Agent Skill / MCP capability attachments, delivered
through native OpenCode surfaces: its own `~/.config/opencode/skills`
discovery directory, `agents/`, `plugins/` and the global `mcp` object in
`opencode.json`.

## Support

| Capability | Status | Delivery | Evidence |
|---|---|---|---|
| Plugin (package-level) | Unsupported (by design) | — no native envelope exists to consume | CODE_FACT |
| Skills | Supported | Native: a directory of its own in `~/.config/opencode/skills/<label>` (rendered SKILL.md, supporting files copied, never a link: OpenCode's walker does not descend a linked skill root, so its `<skill_files>` came out empty) | EMPIRICAL (OpenCode 2.0.18, Lab `experiments/opencode/study_mechanics`, `experiments/opencode/skill_files`) |
| Skill invocation policy | Native for model=false; Degraded for user=false | Generated wrapper SKILL.md with `metadata.opencode/autoinvoke: false` (model=false). V2 removed `slash` (199aabe9e), so nothing withholds a Skill from the person: `/skills` lists it and a mention expands it (ADR-030) | EMPIRICAL — the Lab's `skill-model-only-is-*` declarations (opencode v2.0.23) |
| MCP | Supported | Adapted — direct write to `opencode.json`'s `mcp.<name>` | TESTED (config-level); no behavioral/CLI-discovery probe recorded for OpenCode in any ADR |
| Instructions/Context | Native (outside this crate) | Reads `AGENTS.md` directly, no bridge needed | DOCUMENTED (ADR-014) |
| Project `.agents/skills` | Native | OpenCode reads the project's `./.agents/skills` itself; UZE writes nothing into the repository | EMPIRICAL (OpenCode 2.0.18, Lab contract `context-project-skill-reaches-model`) |
| Project `.agents/agents` | Unsupported | OpenCode does not read `./.agents/agents`; its project roots are `.opencode/agent(s)`, inside the checkout, where UZE never writes. No launch can hand them over from outside: `OPENCODE_CONFIG_DIR` replaces the user's configuration directory (the user's provider is lost), and the additive `OPENCODE_CONFIG`/`OPENCODE_CONFIG_CONTENT` are read by the process that starts the server. A default launch attaches to the shared background service, which keeps whichever environment started it: one project's agents are then offered in every other project it serves, and a launch finding it already running has its own ignored. Only `--standalone` holds them per launch, and that mode is the person's choice, not UZE's | EMPIRICAL (OpenCode 2.0.18, `experiments/opencode/project-agents`; the contract's `context-project-agent-reaches-model` declares it) |
| Agents | Supported | Generated `~/.config/opencode/agents/<label>.md` (the file name is the agent id) with `description` and `mode: subagent`. A root field is not carried (a Claude-style `model`/`tools` makes OpenCode drop the agent silently) and is reported as Degraded. `harness.opencode` carries V2's own fields (`model`, `color` as `#rrggbb`, `mode`, `permissions` rules (only `ask`/`deny`: an `allow` would grant the agent what the operator did not, so it is left out and reported as Degraded), `steps`, `hidden`, `disabled`) and leaves out V1's (`tools`, `permission`, `temperature`, `top_p`) with the V2 field that says the same thing: one key V2 does not know sends the whole file down its legacy path, where V2 fields are lost and `temperature`/`top_p` go into a request body V2 does not send | EMPIRICAL (OpenCode 2.0.15/2.0.18, Lab probe); V2 dialect from source (`packages/schema/src/config/agent.ts`, `core/src/config/plugin/agent.ts`, `v2` at b78d10cd7) |
| Hooks | Adapted, every portable event and effect but `transform` | One generated plugin at `<config root>/plugins/hooks-<package>.ts`, auto-discovered: the plugin *is* the wrapper (same `HOOK_*`/exit-code contract). Tool hooks observe and deny (`execute.before` renames a refused call to a tool that does not exist, so a tool that asks no permission is refused too); `permission.evaluate` asks, with the input `execute.before` kept by call id; `session.created` is `SessionStart` (`startup` only) and `session.execution.succeeded` is `Stop`, a denied stop continuing the session with synthetic input. The permission prompt does not show an `ask` reason | EMPIRICAL — the Lab's hooks contract (2.0.24); V2 plugin API read at anomalyco/opencode `v2` b78d10cd7 |
| Runtime projection | None | `runtime_contribution`/`supports_runtime_integration` never overridden — inherits passthrough default | CODE_FACT |

## Delivery

```
Store plugin
   │
   ├── Skill → directory ~/.config/opencode/skills/<label>/  (route: Native, once `uze setup` ran)
   │           └── before `uze setup`: Unsupported, naming `uze setup`
   │
   └── MCP   → direct write into opencode.json's `mcp.<name>` object (route: Adaptable)
   ↓
capability receipts (no package-level receipt — there is no native envelope)
```

No marketplace/catalogue step exists for OpenCode: `package_exposure_plan`
is never overridden, so `IntegrationPort`'s default (`None`) always applies
and every resource routes through per-capability `exposure_plan` instead.

## Native package

None. This is a deliberate architectural fact, not a gap: OpenCode has no
documented external plugin manifest UZE could preserve-and-consume the way
Claude's `.claude-plugin/plugin.json` or Codex's `.codex-plugin/plugin.json`
are consumed. Package coverage
(`provided_resource_identities`) is not applicable here.

## Fallbacks

- **Skill and MCP**, pre-`uze setup`: no fallback — `Unsupported`, with a
  rationale naming `uze setup` (ADR-006, ADR-007).

## Runtime

None. `runtime_contribution` and `supports_runtime_integration` are never
overridden on `OpenCodeIntegration`, so both fall through to the
`IntegrationPort` trait defaults (pure passthrough, `false`). OpenCode gets
the shim/dispatch/bypass machinery Claude uses for free but does nothing
with it today (ADR-014 explicitly anticipates this).

## Lifecycle

| Receipt | Inspect | Detach | Drift-safe |
|---|---|---|---|
| `VendorConfigEntry` (MCP; Skills are `GeneratedTree` receipts, inspected by the directory's `tree_sha256`) | Reads `opencode.json`, checks `mcp.<name>` against the receipt's recorded command/args/transport/cwd/env/enabled | Re-inspects immediately before mutating (ADR-009); removes only the matched key, preserves every other `mcp` entry and top-level config key | Yes — `mcp_inspection_tolerates_unrelated_fields_and_detaches_only_owned_entry` asserts a `foreign` entry and an `unrelated` top-level key both survive detach |

OpenCode is the only integration that writes its vendor config file
**directly** (`attach_mcp_config`/`detach_receipt` parse-and-rewrite JSON)
rather than shelling out to an official CLI verb the way Claude
(`claude mcp add/remove`) and Codex (`codex mcp add`, `codex plugin ...`)
do. `attach_mcp_config` refuses to overwrite a same-named entry it doesn't
already own (`Some(_) => Err(...)` when the existing value differs from
what UZE would write), which is OpenCode's own collision safety net for
this direct-write approach — confirmed by ADR-013's stated consequence
("OpenCode configuration conflicts fail rather than overwrite unrelated
user entries").

**ADR-009 compliance note:** ADR-009 states mutation should prefer a
harness's own CLI/API, with direct file editing "permitted only inside its
integration when no sufficient structured API exists." No ADR in this
repository states outright that OpenCode lacks a `mcp add`-equivalent CLI
verb — the direct-write choice is DOCUMENTED as deliberate and
collision-safe (ADR-013), but the specific justification "no CLI exists"
is not itself sourced anywhere. Treat this as an unverified assumption
carried by the implementation, not a disproven one.

## Provisioning and the `opencode2` name

The V2 installer is OpenCode's standard channel and places `opencode`; the
V2 beta placed only `opencode2`, which it now leaves as a compatibility
script. `provision()`:

1. `resolve_opencode_binary()` looks for `opencode`, then `opencode2`, on
   `PATH` outside the shims, and in the installer's documented directories.
2. Runs `opencode upgrade`, or the official V2 installer when only the beta
   `opencode2` is there (it takes a positional project path, not
   `upgrade`), through the injected `ProcessRunner`, and reports `Verified`
   once the version answers.

UZE creates no alias for either name; its runtime shim is what keeps
`opencode` stable.

## Limitations

- No package-level delivery exists or is planned; every capability is
  decomposed individually.
- `permission.ask` does not fire
  ([anomalyco/opencode#7006](https://github.com/anomalyco/opencode/issues/7006)),
  so an `ask` effect is never claimed here; `tool.execute.before` does not
  cover subagent-issued tool calls
  ([sst/opencode#5894](https://github.com/sst/opencode/issues/5894)), a gap
  the bridge inherits. Both are why this harness's effect set stops at
  `observe`/`allow`.
- MCP attachment writes the vendor config file directly instead of through
  an OpenCode CLI verb — collision-safe by construction, but the "no CLI
  exists" premise behind that choice is undocumented (see Lifecycle above).
- The binary-alias mechanism (`ensure_opencode_alias`) is only exercised in
  tests when `opencode`/`opencode2` genuinely exists on the test machine's
  real `PATH` — see Evidence.
- OpenCode also reads Codex's `~/.agents/skills`. A skill both harnesses
  receive therefore exists twice on disk, and OpenCode resolves the name
  by its own precedence, in which `~/.config/opencode` wins: the copy it
  shows is the one encoded for it.

## Evidence

- Tests: `cargo test -p uze-integrations --lib opencode` — 5 passed, 0
  failed (`opencode::lifecycle_tests` ×2, `opencode::mcp::mcp_tests` ×1,
  `opencode::provision::provision_tests` ×2).
- Skills transparent attachment: EMPIRICAL, OpenCode 1.18.18, real
  behavioral proof-token run via `opencode/deepseek-v4-flash-free`,
  2026-08-20 (ADR-006). No later re-validation recorded against a `v2`
  (`opencode2`-named) install — the alias mechanism this integration's
  `provision.rs` exists for postdates that evidence.
- MCP: no CONFIGURATION/DISCOVERY/BEHAVIORAL conformance run is recorded
  for OpenCode anywhere in this repository's ADRs (unlike Claude/Codex,
  which both have dated real-harness runs in ADR-007). MCP support here is
  TESTED at the unit level only.
- No `opencode`/`opencode2` binary is installed in the environment this
  audit ran in — no live re-validation was possible this pass.

## Next

1. Record a real OpenCode MCP conformance run (configuration/discovery
   tier at minimum), matching what ADR-007 already did for Claude/Codex —
   currently the only harness with zero dated MCP evidence.
2. Confirm or refute whether OpenCode exposes any CLI verb for MCP
   registration; if one exists, ADR-009 would call for using it instead of
   direct JSON writing.
3. Re-verify Skills against an actual `opencode2`-named v2 install — the
   existing EMPIRICAL evidence predates the binary-alias code this
   integration now depends on.
4. `provision_dispatches_install_or_update_consistently_with_detected_state`
   silently skips on a machine with neither `opencode` nor `opencode2` on
   `PATH` (true for CI) — it has never actually asserted `Verified` in that
   environment; a fully mocked alias-resolution seam would close this gap.
