# Inventory: 2026-10-05 findings

This file holds the findings `tasks.md` refers to. They were gathered on
2026-10-05 from three sources:

- a `harness-watch` survey of the four vendors;
- a trace of why the Lab missed each break;
- a sweep of the whole Lab.

Paths are relative to the repository root. `conformance/` is abbreviated as
`c/`.

## Baseline

| harness | evidence (22/09) | nightly 05/10 |
|---|---|---|
| Claude Code | 2.1.280 | 2.1.289 (2.1.290 released) |
| Codex | 0.156.0 | 0.160.0 (0.160.1 released) |
| OpenCode V2 | 2.0.14 | 2.0.23 |
| Antigravity | 1.2.8 | 1.2.17 |

All legs passed. Five OpenCode results were ADAPTED.

## Survey: breaks

| # | harness | break | UZE site | source |
|---|---|---|---|---|
| B1 | OpenCode V2 | Tools are `shell`/`command`, `read`/`write`/`edit` with `path`, `websearch`, `subagent`. Bindings use `bash`, `filePath`, `web_search`, `task`, so every matcher-scoped group never fires. | `crates/uze-integrations/src/opencode/hooks.rs:27-70`, bridge `hooks/bridge.rs:263,270` | anomalyco/opencode@v2 `packages/core/src/tool/plugin/*.ts`, cc7827fe0 |
| B2 | Claude Code | The subagent tool is `Agent`. `agent.spawn` is bound to `Task`; `MultiEdit` is gone. | `claude/hooks.rs:85,101-105` | code.claude.com hooks and tools references |
| B3 | Codex | Non-managed hooks run only after a trust review, per hash. UZE reports them Native. | `codex.rs:~103,~223`; Lab bypass `c/harnesses/codex/scenarios.py:415` | learn.chatgpt.com/docs/hooks |
| B4 | Codex | Untrusted projects get no project `AGENTS.md` (0.150.0). | `codex.rs:176-179` | openai/codex#39837 |
| B5 | Codex | A root `plugin.json` with the Agent Plugins `$schema` takes precedence over `.codex-plugin/plugin.json` (0.147.0). On the explicit route, coverage is wrong and MCP may be registered twice. | `shared/marketplace.rs:365`, `codex/plugin.rs:291` | openai/codex#36544, #36796 |
| B6 | OpenCode V2 | `slash` was removed from skills. The `unsupported` reason is false. | `opencode/skills.rs:82`, `c/harnesses/opencode/bindings.py:182` | 199aabe9e, 3ddb0cb1d |
| B7 | OpenCode V2 | The agent fields `tools`, `permission`, `temperature`, `top_p` are legacy. | `opencode.rs:470-479` | `agents.mdx@v2` |
| B8 | Claude Code | PostToolUse and Stop emit `permissionDecision`, where the schema wants `decision: "block"`. Blocking holds only through exit 2. | `claude/hooks.rs:33-41` | hooks reference |

## Survey: rungs and capabilities (group 9)

- **Antigravity:**
  - `~/.gemini/config/plugins.json` in-place registration (1.2.10, 1.2.16);
  - plugin `rules/AGENTS.md`;
  - reinstall replaces the directory exactly (1.1.28);
  - `agy install --skip-path|--skip-aliases`;
  - global `AGENTS.md` paths (1.2.15);
  - a 24 KB per-file cap and a 20k-token rule budget (1.2.7).
- **OpenCode:**
  - `permission.evaluate` `source` plus `effect: deny`;
  - `--session <id>` creates an unknown id (07338c5d4);
  - `opencode mcp add --global`.
- **Codex:**
  - a UZE package is a native Agent Plugin, with `hooks/hooks.json`;
  - `updatedInput`;
  - new events;
  - MCP names with `:@/.` (0.152.0).
- **Claude Code:**
  - plugin CLI `--json` with `errorDetails` (2.1.268);
  - `ask` and `updatedInput`;
  - plugin-bundled hooks;
  - native `AGENTS.md` on all backends (2.1.281). PR #154 owns this.

## Why the Lab missed them: structural causes

1. **Unmeasured tool vocabulary.** UZE's tables are hand-kept, and the Rust
   tests assert them against themselves (`tests/integrations/hooks.rs:931`).
   Nothing compares them with the tools the harness declares.
2. **One alias, one event.** Only `shell` on PreToolUse is fired. OpenCode
   fires only a stale `native:` MCP name.
3. **No presence proof.** Absence checks, "settled", and allow paths all
   pass when nothing ran.
4. **Declarations never expire.**
   - `versions: ["*"]`.
   - Contracts use `kind="adapt"`, while the gate reads only `"adapted"`
     (`c/gate.py:76`).
   - The evidence baseline is never refreshed.
5. **The Lab answers vendor prompts a user meets.** It is the same class as
   the interactive installer of #172. That lesson became a journeys rule
   only (`journeys/README.md:301-306`).

## Sweep A: prompts and environment the Lab answers

| site | prompt answered | user meets it |
|---|---|---|
| `c/harnesses/claude/fixtures/claude.json` → `scenarios.py:44`, `parity.py:168` | onboarding, trust `/work` | yes |
| claude `scenarios.py:40,110`, `parity.py:165` | API key, custom-key dialog | no (own account) |
| claude `scenarios.py:125,132,136` | trust "Yes", login method | yes |
| claude `bindings.py:89`, `parity.py:174` | `--permission-mode bypassPermissions` | yes |
| codex `fixtures/auth.json`, `scenarios.py:49,53` | API key, login | partly |
| codex `scenarios.py:56,138-140,415` | `[features] hooks=true`, folder trust by Enter, hook-trust bypass | yes |
| codex `bindings.py:94`, `experiments/session_start_probe.py:47-48` | `--skip-git-repo-check`, hook-trust bypass | yes |
| opencode `scenarios.py:45-58` | provider/model pre-seed, `OPENCODE_DISABLE_MODELS_FETCH` | n/a |
| opencode `scenarios.py:76,253,406`, `bindings.py:20,66` | `UZE_HOME=/usr/local/.uze`, image shims, `--standalone`. Phases run against a different UZE home than the one installed into. | no, and the run is wrong |
| opencode `skill_files.py:48` | `opencode run --auto` | yes |
| antigravity `fixtures/settings.json` → `scenarios.py:90` | `trustedWorkspaces`, `permissions.allow: ["mcp(*)"]` | yes |
| antigravity `scenarios.py:91-92` | onboarding state, installation id | yes |
| antigravity `scenarios.py:121-129`, `bindings.py:55-70` | colour scheme, terms, folder trust by keys | yes |
| antigravity `scenarios.py:460-465` | permission prompt, feedback survey | yes |
| antigravity `bindings.py:134` | `--dangerously-skip-permissions` | yes |
| antigravity `bindings.py:130` | Lab-authored `lab-dispatcher` agent | yes (no roster by default) |
| antigravity `provider.py` | `json-hooks-enabled=true`; `fetchAdminControls` and other RPCs answered `{}` | partly |
| `c/shared/common.py:979` vs `c/Dockerfile:63-69` | `seccomp=unconfined`. The read-only root and `cap_drop` that the Dockerfile claims are never applied. | n/a (honesty) |

Only jq, the flags and the Antigravity sign-in are recorded in
`c/DECISIONS.md`.

## Sweep B: checks that pass without the behaviour

| check | why |
|---|---|
| `skill.py:72,186`, `mcp.py:24`, `agent.py:64`, `context.py:214`, `continuity.py:166` | `True` with `kind="adapt"`, counted as plain passes |
| `skill-model-only-is-not-user-invocable` / `-not-invocable` (`skill.py:131,226`) | `settled=True` hard-coded |
| Claude `user-only-skill-hidden` (`scenarios.py:229`); OpenCode `user-only-skill-hidden-from-model` (`:224`) | not gated on presence; Claude's `Review code` is never recorded |
| Claude `model-visible-skill-present` (`scenarios.py:224`) | matches the substring `commit` |
| `hooks-allow-tool-executed`, Claude (`:414,478`) and Antigravity (`:750,791`) | `plain output` is in the scripted command |
| `hooks-allow-marker-absent-*`, all four | no proof that the hook ran |
| `hooks-*-marker-absent-*`, deny/order | gated on "settled" only |
| `hooks-*-turn-settled` / `-turn-requested` (Codex `:488` `bool(struct)`) | any tool result settles; Codex accepts `error`, `denied`, `Sandbox mode` |
| OpenCode `hooks-deny-marker-absent-blocked by protect-env` | asserts that the denial is absent |
| OpenCode `hooks-{deny,order}-v2-limitation`, `hooks-*-tool-executed` | `check(True, "adapted")`; stale MCP name (`scenarios.py:365`) |
| Antigravity `mcp-tool-invoked-via-tui` (`:316`) | final text served after any function response |
| `plugin-surface-in-tui`, `skills-surface-in-tui`, `skills-list-opens`, `mcp-surface-in-tui` | chrome only |
| OpenCode `mcp-server-connected-in-tui` (`:177`), `names_server` | any `Connected` row or key hint |
| Codex `synthetic-credential` | same text as the prompt check |
| all `parity-*` | pass when both sides are broken; the MCP probe is skipped |
| `session-start-session-answered` | any output line |
| `policy-sidecar-delivered`, `mcp-server-configured`, `session-start-entry-delivered`, `skill-harness-started-without-the-shim` | UZE's own output |
| `isolation-declaration-reaches-model` (`isolation.py:62-63`), Antigravity plugin `hooks.json` validate, `hooks-delivered-hooks-loaded`, `context-agents-md-reaches-model` | a route UZE does not use |

The deny guard also decides on `$HOOK_COMMAND$HOOK_INPUT`, and the fixtures
pair `shell` with a `native:exec_command` fallback. Both hide
translation bugs.

## Sweep C: wire constants with no committed capture

- **Claude:**
  - `MCP_TOOL` (`provider.py:34`);
  - model `claude-opus-5`;
  - fixed `toolu_1`;
  - catch-all `{"success":true}`.
- **Codex:**
  - `TOOL_NAME="Bash"` (`:82`);
  - the `collaboration` namespace, `spawn_agent`/`wait_agent`;
  - models;
  - the `### Available skills` heading;
  - `fc_uze_N`;
  - `exec_command`/`cmd`;
  - catch-all `{"ok":true}`.
- **OpenCode:**
  - `MCP_NAMESPACE` (`provider.py:52`);
  - the `execute` shape;
  - `subagent`/`agent`;
  - `skill`/`id`;
  - hook `serverName`/`toolName`/`arguments`.
- **Antigravity:**
  - `FC_ARGS` ServerName (`:113`);
  - `RUN_COMMAND_ARGS`;
  - `TOOL_NAMES`;
  - `Subagents[].TypeName`;
  - model ids.
- **All four:** `SKILL_MARKERS` holds the bare words `commit`, `review`,
  `init`.

## Surfaces with no real-harness check

- **Hooks:**
  - aliases `file.read`, `file.write`, `file.edit`, `search.files`,
    `search.web`;
  - PostToolUse and Stop;
  - the effects `ask`, `allow`, `transform`, `observe`;
  - fail-closed (experiment only);
  - SessionStart on OpenCode and Antigravity;
  - the command-field mapping;
  - any OpenCode hook firing.
- **MCP:** Claude MCP execution (PARTIAL).
- **Agents:** the agent `tools:` restriction.
- **Routes and lifecycle:**
  - explicit-route plugins;
  - removal residue;
  - update and re-projection;
  - machine vs project scope.
- **Context:** `AGENTS.md` size limits, and a project with no `.agents`.
