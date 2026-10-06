# Portable Hooks

The full contract, for plugin authors and contributors. The user-facing
summary is the site's
[Capabilities](../../web/content/docs/concepts/delivery.mdx) page; change
both together.

One authored declaration, one handler contract, four harnesses (ADR-033,
ADR-040). A package ships a root `hooks.json` plus plain scripts; UZE
compiles that, at install time, into each harness's own hook form plus a
small wrapper vendored inside the delivered artifact. **Nothing UZE puts on
the execution path is UZE**: a delivered hook keeps working after the `uze`
binary is removed.

## Canonical manifest

`hooks.json` at the package root:

```json
{
  "hooks": {
    "PreToolUse": [{
      "id": "protect-env",
      "matcher": "shell|file.write|native:Write",
      "effect": "deny",
      "hooks": [{ "type": "command", "command": "${PLUGIN_ROOT}/scripts/check", "timeout": 10 }]
    }]
  }
}
```

- **Events**: the semantic events are `PreToolUse`, `PostToolUse`, `Stop`
  and `SessionStart`. No other event is canonical.
- **Matcher**: `|`-separated portable tool aliases or an explicit
  `native:<tool>` escape hatch. Omitting the matcher matches every tool.
  A `SessionStart` group matches on how the session began instead:
  `startup`, `resume`, `clear`, or several joined by `|`. Omitting it
  matches all three.
- **Effect**: `observe` (default), `allow`, `ask`, `deny`, or `transform`.
  `transform` is only valid on `PreToolUse`, and rewrites the call before
  it runs (see [Rewriting a call](#rewriting-a-call)). `SessionStart` takes
  `observe` only: a session has nothing to allow or deny, and a manifest
  declaring any other effect there is refused at `uze agent plugin check`
  and at install, naming the group.
- **Handlers**: only `type: command`. `timeout` is seconds, bounded to
  1..300, default 30, and it is the handler's real deadline: the wrapper
  runs each handler under it and stops one that exceeds it (see
  [Handler contract](#handler-contract)). A *group* is bounded too: the sum
  of its handlers' deadlines (plus the second between `TERM` and `KILL`, and
  one to render the answer) may not exceed 300s either, because that sum is
  what the harness's own backstop has to outlast — a manifest whose group
  can outlive the backstop is refused, naming the sum. `command` may use the
  `${PLUGIN_ROOT}` placeholder; UZE resolves it at generation time and also
  exports `PLUGIN_ROOT`.
- **`id`** is optional; absent ids are derived deterministically from
  event and group order (`pre_tool_use-0`, `stop-1`, …).

Malformed, duplicate, or unsafe declarations are rejected before any
attachment; nothing is projected silently.

> **Commands are shell command lines.** A handler's `command` is executed
> as a user would type it — `${PLUGIN_ROOT}/scripts/check` therefore
> requires the script to be executable, or the command must say so
> (`sh ${PLUGIN_ROOT}/scripts/check`). A non-executable script fails the
> handler and follows the declared effect's fail-open/fail-closed rule — a
> `deny` hook that cannot run denies.

## Handler contract

A handler reads the hook context from its environment and answers with its
exit code. It never parses a harness payload and never writes harness JSON.

| Variable | Meaning |
|---|---|
| `HOOK_HARNESS` | the delivering harness's id (`claude`, `codex`, `antigravity`, `opencode`) |
| `HOOK_EVENT` | `pre_tool_use` \| `post_tool_use` \| `stop` \| `session_start` |
| `HOOK_SOURCE` | `session_start` only: `startup`, `resume` or `clear`, when the harness reports it; empty otherwise |
| `HOOK_TOOL` | the portable alias that matched; empty for a tool the vocabulary does not bind, and on `stop` and `session_start`, which carry no tool |
| `HOOK_TOOL_NATIVE` | the harness's own tool name, as its hook system reports it (`Bash`, `run_command`, `shell`) |
| `HOOK_CWD` | the workspace directory; may be empty |
| `HOOK_INPUT` | the tool input, as JSON, for anything the alias does not name |
| `PLUGIN_ROOT` | the package root the handler was delivered from |
| `HOOK_<FIELD>` | one per portable field of the matched alias — see the vocabulary below |

| Exit code | Meaning |
|---|---|
| `0` | allow — the next handler of the group runs |
| `3` | deny — the reason is read from stderr, and no later handler runs |
| anything else, a failure to start, or a timeout | a handler failure, resolved by the group's effect |

Only the first 4096 bytes of a handler's stderr become the reason; a handler
that writes megabytes is still a decision, not a document the harness has to
parse.

### Rewriting a call

A handler in a `transform` group answers the same way, plus one thing: on
exit `0` it may write the call's **complete** input to stdout, as one JSON
object in the harness's own shape: what it read from `HOOK_INPUT`, changed.
Nothing on stdout leaves the input as it was. The handlers of the group run
in order, and each one reads the rewrite before it as its `HOOK_INPUT` (and
its portable fields); the last rewrite is what the tool runs. Stdout that is
not one JSON object, or that is longer than 64 KiB, is a handler failure.
The input is the harness's own because a rewrite in portable terms could not
be mapped back: a portable `command` is `command` on one harness and
`CommandLine` on another.

```sh
#!/bin/sh
# Every `rm -rf` the model asks for becomes a dry run.
printf '%s' "$HOOK_INPUT" | jq -c 'walk(if type == "string" then sub("rm -rf "; "echo would remove ") else . end)'
```

A handler failure is **fail-open** for `observe`/`allow` (the tool proceeds
and the failure is reported) and **fail-closed** for `deny`/`ask`/`transform`
(the tool is denied and the reason names the failure). A safety hook that
cannot be evaluated is never weakened into a no-op — and neither is a rewrite
that never happened.

The same rule covers everything that can go wrong before a handler is even
reached: a payload the wrapper cannot parse and a package root that is gone
are both failures resolved by the group's effect. Neither is ever treated as
"nothing matched": an unreadable payload would leave every `HOOK_*` variable
empty, and a missing root would run the handlers from whatever directory the
harness happened to be in — the user's own checkout, whose same-named script
is not the author's.

**Each handler is bounded by its own declared `timeout`.** Past it the
handler is stopped — `TERM`, then `KILL` a second later — along with every
process it started, and the group's effect decides, exactly as for any other
failure (`handler timed out after Ns: …`). There is no portable `timeout(1)`
(macOS ships none) and no job control in a script, so the wrapper does this
with a cancellable sleeper and a `ps` read, and it collects the handler's
stderr in a file under `$TMPDIR` rather than through a pipe — anything the
handler started inherits a pipe, and one that outlived the deadline would
hold the harness there long past it. A handler that exits `124` of its own
accord reads as a timeout, the same ambiguity `timeout(1)` carries.

```sh
#!/bin/sh
# The whole contract: read a variable, choose an exit code.
case "$HOOK_COMMAND" in
  *.env*|*id_rsa*)
    echo "blocked: $HOOK_COMMAND touches a secret file" >&2
    exit 3 ;;
esac
exit 0
```

Handlers of one group run **sequentially in manifest order** and the first
denial stops the rest — on every harness, whatever order the harness itself
would have used.

## Portable tool vocabulary

Each alias names the portable fields it guarantees; each harness names the
tool it matches and the native input field every portable field is read
from. This one table drives the matchers, the generated wrappers and the
compatibility verdicts.

| Alias | Fields | Claude Code | Codex | Antigravity CLI | OpenCode |
|---|---|---|---|---|---|
| `shell` | `HOOK_COMMAND` | `Bash` / `command` | `Bash` / `command` | `run_command` / `CommandLine` | `shell` / `command` |
| `file.read` | `HOOK_PATH` | `Read` / `file_path` | — | `view_file` / `AbsolutePath` | `read` / `path` |
| `file.write` | `HOOK_PATH` | `Write` / `file_path` | — ¹ | `write_to_file` / `TargetFile` | `write` / `path` |
| `file.edit` | `HOOK_PATH` | `Edit` / `file_path` | — ¹ | `replace_file_content` / `TargetFile` | `edit` / `path` |
| `search.files` | `HOOK_QUERY` | `Grep` / `pattern` ² | — | — | `grep` / `pattern` |
| `search.web` | `HOOK_QUERY` | `WebSearch` / `query` | — | `search_web` / `query` | `websearch` / `query` |
| `agent.spawn` | — | `Agent` | `collaborationspawn_agent` | — ³ | `subagent` |
| `agent.message` | — | — | `collaborationsend_message` | `send_message` | — |

Every name is measured, never recalled: it is the name and the input
field a real call reached a hook with, recorded by the Conformance Lab's
census (`conformance/evidence/tools/<harness>.json`, Claude Code 2.1.290,
codex-cli 0.160.1, Antigravity 1.2.17, OpenCode 2.0.23), and
`cargo test` fails on a table entry a later census contradicts. A
`native:<tool>` matcher bypasses the table entirely: the handler receives
`HOOK_TOOL_NATIVE` and `HOOK_INPUT`, with `HOOK_TOOL` empty. A group whose
matchers name only aliases a harness has no tool for is reported
unsupported there and not delivered.

1. Codex writes and edits files through `apply_patch`, which reaches a hook
   with the whole patch in `command` and no path field, so neither alias
   can carry `HOOK_PATH`; a guard on patches matches `native:apply_patch`.
2. Claude Code's native build searches through `Bash` and offers `Grep` to
   the main model only on opt-in (`--allowedTools`/`--tools`) or to a
   subagent whose tools name it.
3. Antigravity offers `invoke_subagent` only to an agent whose definition
   lists it.

## Delivery per harness

| Harness | Delivered artifact | Route |
|---|---|---|
| Claude Code | one merged entry per group in `~/.claude/settings.json`, `command` = the generated `hooks/exec` with the group's arguments (exec form: no shell parsing) | native |
| Codex | one merged entry per group in `~/.codex/hooks.json`, one quoted shell line invoking the same wrapper | native; held back until the person trusts it — Codex runs a hook from `hooks.json` only once its review recorded a hash for it (a TUI session asks; `codex exec` skips it in silence), and asks again after a change. `uze status`, `uze inspect`, `uze doctor` and the install report name every hook waiting on that review, read from Codex's own record and never written to it |
| Antigravity CLI | one named entry per group merged into the shared `~/.gemini/config/hooks.json` (the document root *is* the named-hook map), keyed `<package>:<group-id>` — grouped (`matcher` + `hooks`) for the tool events, a flat handler list for `Stop` and `SessionStart` — whose command is the shared `hooks/exec` wrapper by absolute path | native (shared vendor file); execution needs a signed-in session — an API-key session runs no hook at all (#893). The Lab measures the vendor's execution gate (`hooks > vendor`) every run, and every claimed cell through the hooks contract |
| OpenCode | one generated plugin module (the definition as its default export, no import), `<config root>/plugins/hooks-<package>.ts`, auto-discovered — the plugin *is* the wrapper, with the package's groups as data | adapted: every event and effect rides OpenCode's own plugin API (tool hooks, `permission.evaluate`, the `session.created` and `session.execution.succeeded` bus events); `SessionStart` reports `startup` only |

The `sh` wrapper is one file per harness, byte-identical for every package,
and depends on `sh` and `jq`. Claude, Codex and Antigravity each keep one
copy under `$UZE_HOME/runtime/attachments/<harness>/hooks/exec` — a shared
vendor config file has no plugin root to resolve against, so every entry
names the wrapper by absolute path.

A native entry runs it as:

```
<wrapper> <plugin-root> <event> <effect> <seconds>:<handler> [<seconds>:<handler>…]
```

One argument per handler, carrying its author's deadline beside its command
— so what will run, and for how long, is readable in the harness's own
configuration. The entry's own `timeout` key is the *harness's* backstop and
is sized (`sum(handler + 1) + 1`) so it can never be the bound that fires
first — which is also why a manifest whose group needs more than 300s is
refused rather than clamped: a hook the harness kills is read as
non-blocking, so a clamped backstop would turn a `deny` group into an
allowance.

**Nothing else implements this contract.** There is no UZE binary on the
execution path and no second route: a platform the `sh` template does not
cover delivers no hook at all and says so (see
[Known limitations](#known-limitations)). The wrapper's answer for every
fixture — decision document, exit status, reason — is recorded per harness
in `crates/uze-integrations/tests/goldens/hooks/`, so a change to what a
harness is told is a reviewed diff.

Compatibility is semantic, per event and effect. An event a harness does not
fire is never represented by one it does: a `Stop` hook is never a tool
callback, and a `SessionStart` hook is never a per-turn callback. The group
is reported Unsupported on that harness with the reason stated (in `uze
doctor`, one row per group and harness), and it is not attached; the
package's other groups are delivered there as usual, and the manifest is
not refused for it. A `SessionStart` group that waits only for a start the
harness never announces (a resume or a clear, on a harness that announces
only a new session) is reported Unsupported the same way.

On OpenCode V2 the decisions ride two halves of its plugin API: the tool
hook sees the input but cannot refuse, and `permission.evaluate` can refuse
or ask but carries no input, so the bridge keeps the input by call id
between the two. Its permission prompt shows the call and not the request's
message, so a handler's reason for asking is not on screen there (the
reason for a denial reaches the model as on every harness).

## Compatibility matrix

Legend: **native** = the harness's own mechanism with the canonical
semantics preserved · **adapted** = delivered, semantics degraded (reason
stated) · **—** = not expressible.

| | Claude Code | Codex | Antigravity CLI | OpenCode V2 |
|---|---|---|---|---|
| wrapper runtime | `sh` + `jq` | `sh` + `jq` | `sh` + `jq` | Bun (embedded) |
| plugin root | absolute path | absolute path | absolute path (cwd is the `hooks.json` directory) | `import.meta.url` |
| exec form (no shell parsing) | yes (`command` + `args`) | no (shell line) | no (shell line) | n/a |
| matcher | native, regex on the tool name | native | native, regex (`"*"` matches all) | in-plugin |
| a group's handlers | run **in parallel** natively → sequential inside `exec` | sequential inside `exec` | sequential inside `exec` | sequential inside the plugin |
| `PreToolUse` observe/allow | native | native | native (signed-in session) | native (`execute.before`) |
| `PreToolUse` deny | native (JSON + exit 2) | native (JSON + exit 2) | native (`decision: deny`, signed-in session) | native (`permission.evaluate` → `deny`, input kept from `execute.before`) |
| `PreToolUse` ask | native (`permissionDecision: ask`; the prompt shows the reason) | — (0.160.1 rejects `permissionDecision: ask` in `PreToolUse`, and its `PermissionRequest` hook takes `allow`/`deny` only: codex-rs `hooks/src/engine/output_parser.rs`) | native (`decision: ask`, signed-in session) | native (`permission.evaluate` → `ask`; the prompt does not show the reason) |
| `PreToolUse` transform | native (`allow` + `updatedInput`) | native (`allow` + `updatedInput`, `hookEventName` required; only `command` is read for a shell call) | native (`allow` + `overwrite`, a hook result field the docs do not list, measured every run) | native (`execute.before`'s input reassigned, as OpenCode's own input repair does; a failed rewrite is refused at `permission.evaluate`) |
| `PostToolUse` | native, observe only | native, observe only | native (`{}`), signed-in session | native (`execute.after`; a denial reaches the model as synthetic input) |
| `Stop` | native (exit 2 prevents the stop) | native (must print `{}`) | native, signed-in session | native (`session.execution.succeeded`; a denial continues the session with synthetic input, as OpenCode's own plan plugin does) |
| `SessionStart` (observe) | native, matcher on the source | native, matcher on the source (a hook is reviewed before it runs) | native through the undocumented `SessionStart` key (flat; once per new conversation, at its first model call; `startup` only — a `--continue` announces nothing), measured every run | native (`session.created` of a top-level session; `startup` only — a resumed session announces nothing) |
| a denial's exit status | 2 (the documented block signal) | 2 | **0** — the decision is the stdout document, and any non-zero exit is logged as a *failed* hook | n/a |
| fail-closed on handler failure | via `exec` (natively, exit ≠ 2 **runs the tool**) | via `exec` | via `exec` | via the plugin (`permission.evaluate`) |
| handler context | `HOOK_*` environment | `HOOK_*` environment | `HOOK_*` environment | `HOOK_*` environment |

### Session start

A `SessionStart` group observes a session opening. Its handler gets
`HOOK_HARNESS`, `HOOK_EVENT=session_start`, `HOOK_CWD`, `PLUGIN_ROOT` and
`HOOK_SOURCE`, and no tool fields. Nothing it answers can keep the session
from starting: a failure, a timeout or the deny exit code is reported on
stderr and the session opens. Without a matcher the delivered entry names
the three portable sources explicitly, so a harness's own further source (a
compaction) never runs a handler promised one of these.

Write the handler to be fast and idempotent: Claude Code runs a group's
entries in parallel, and a session can start several times in a day
(`resume`, `clear`). A handler that starts a service should check it is not
already running.

The route per harness is measured in the Lab
(`experiments/<vendor>/session-start`): on Claude Code and Codex the handler
runs exactly once for a new headless session, with `HOOK_SOURCE=startup`.

## Lifecycle safety

- Every projection is receipt-owned: config entries by exact content
  identity, the generated wrapper by comparison against the template UZE
  writes, the OpenCode plugin as a whole owned file. Inspection compares
  the exact managed content, removal refuses drift, and foreign hooks,
  plugins, files, entries and ordering are never changed.
- All generated artifacts are derived: safe to delete and regenerate from
  the Store (`uze install -m` / `uze update -m` rebuilds them). The
  shared wrapper is removed with the last entry that needs it.
- Existing installs are re-projected on the next install/update: nothing is
  migrated in place, and a receipt-owned entry from a previous release is
  replaced, never duplicated.
- `uze inspect <plugin>` lists hooks with their per-harness delivery;
  `uze doctor` reports attachment health, the route each hook took, and a
  delivered wrapper whose `jq` is missing; the TUI harness matrix shows the
  per-harness verdict.

## Known limitations

- **A rewrite is in the harness's own shape.** A `transform` handler that
  must run on several harnesses reads `HOOK_TOOL_NATIVE` to know which shape
  it is answering in; the portable fields are inputs only.
- **`jq` is the shell wrapper's dependency.** It is not declarable by a
  package yet (plugin `requirements` is its own change); `uze doctor`
  reports it missing, and until it is installed a `deny` group denies while
  an `observe` group proceeds and reports.
- **Windows has no wrapper template, so Windows has no hooks.** A PowerShell
  wrapper is future work; until it exists a hook there is reported
  Unsupported with that reason and nothing is attached. There is no second
  route to fall back to, by design: an entry running something other than
  the wrapper would be a second implementation of the contract, and the
  first thing two implementations do is disagree.
- **Antigravity ran delivered hooks only in a signed-in session through
  1.1.24.** Its hook entries load and list correctly in either mode
  (`hooks_manager: loaded N named hooks`), but the executor reads
  `enable_json_hooks` — field 17 of the model backend's
  `CustomizationConfig`, switched server-side by the `json-hooks-enabled`
  feature flag. Through 1.1.24 that config reached the CLI only over the
  CloudCode backend it speaks when signed in to a Google account; a session
  running on `GEMINI_API_KEY` never received it and ran no hook at all —
  not UZE's, not the vendor's own format at the vendor's own path. That was
  the vendor's own bug,
  [antigravity-cli#893](https://github.com/google-antigravity/antigravity-cli/issues/893),
  with #78 recording that the API-key path is unsupported. On 1.1.25 the
  Lab measured the same vendor-format deny hook firing under the API key.
  **If your hooks are silent on an older build, check how the CLI is
  authenticated before suspecting delivery.** The Lab's Antigravity
  vertical runs signed in (a synthetic identity against its own CloudCode
  plane) and proves the gate open there, and asserts the same control hook
  on the API-key mode every run, so a regression shows as a red check.
- **Antigravity does not load a plugin's `hooks.json` (1.1.24)**, which is
  why UZE delivers its hooks into the shared `~/.gemini/config/hooks.json`
  instead. The harness reads `hooks.json` from its shared customization
  roots but never from a plugin directory: `agy plugin validate` counts a
  plugin's hooks, the plugin is listed with a `hooks` component and enabled
  in `config.json`, and the session still reports `loaded 0 named hooks from
  0 hooks.json file(s)` — the file is never opened. The vendor's own shipped
  plugin guide documents the opposite ("Hooks defined in
  `plugins/<name>/hooks.json` are registered and run during the agent's
  lifecycle"). The Lab measures it live each run (`hooks > delivery`), so if
  a later build starts reading plugin hooks the delivery can move back.
- **OpenCode V2 cannot block.** Its tool hooks carry the input but no block
  signal; `deny`/`ask` are diagnosed before attach.
- **Antigravity's two entry shapes are not interchangeable.** Its docs
  give the tool events a `matcher` and a `hooks` group, and `Stop` a flat
  list of handler objects. A `Stop` written in the grouped form is parsed
  as invalid and dropped in silence — `agy plugin validate` reports
  nothing, and only `--log-file` names the reason
  ([antigravity-cli#925](https://github.com/google-antigravity/antigravity-cli/issues/925),
  1.1.24). UZE emits each event in its own shape.
- **Codex requires the `[features].hooks` flag** in `~/.codex/config.toml`
  (verified against codex-cli 0.150.0).
- **`tool.execute.before` does not cover subagent-issued tool calls** on
  OpenCode ([sst/opencode#5894](https://github.com/sst/opencode/issues/5894)).
