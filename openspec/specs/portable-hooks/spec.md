# portable-hooks Specification

## Purpose
A package declares its hooks once, in a package-root `hooks.json`, against a
vendor-neutral manifest and command ABI (ADR-033). UZE validates the manifest,
assesses each group against what every harness can preserve, and delivers it
through that harness's own hook mechanism as a receipt-owned artifact
(ADR-040), never touching the operator's own hooks or plugins. Created by
syncing the archived change `add-portable-hooks`.

## Requirements

### Requirement: UZE discovers a canonical Hook manifest
The system SHALL discover a package-root `hooks.json` as portable Hook
resources without changing its bytes in the Store. The manifest SHALL accept
only the documented portable command-hook schema and SHALL report malformed,
duplicate, or unsafe declarations before attachment. Each group SHALL become
one Hook resource named by its `id`, or by `<event>-<index>` when it declares
none, and SHALL keep its manifest position as its order.

#### Scenario: One authored manifest creates independent hook resources
- **WHEN** a package contains one valid `hooks.json` with two declared hook groups
- **THEN** the Engine composes two Hook resources, one per group, each named by the group's `id` or `<event>-<index>`
- **AND** each resource identity remains stable across a reinstall

#### Scenario: Invalid declaration is not silently projected
- **WHEN** a manifest contains an unknown event, an unknown field, a malformed or unknown matcher, an unsupported handler type, a duplicate or invalid group id, a group with no handlers or an empty command, or a timeout outside the allowed range
- **THEN** UZE reports a validation error and creates no Hook attachment receipt

### Requirement: UZE calculates Hook compatibility semantically
The system SHALL calculate compatibility from event, effect, matcher,
input transformation, and handler ordering, not merely from matching vendor
event names. It SHALL report a route of `native`, `adaptable` (UZE's own
generated runner carries the hook), `degraded`, or `unsupported`, with a
reason when the target cannot preserve the group.

#### Scenario: Stop has no false OpenCode equivalence
- **WHEN** a portable Stop hook is assessed for OpenCode
- **THEN** the result is unsupported with the reason that the target has no `stop` semantic event
- **AND** UZE SHALL NOT claim that a session or tool callback is a native Stop hook

### Requirement: A group the target cannot preserve is not attached
A target that lacks the event, and a `deny` or `ask` group whose semantics
the target cannot preserve, SHALL be `unsupported`; any other loss SHALL be
`degraded`. A `degraded` or `unsupported` group SHALL NOT be attached.

#### Scenario: A deny group on a target that cannot block
- **WHEN** a `deny` group is assessed for a target that cannot block the intercepted operation
- **THEN** the route is unsupported with the reason, and the group is not attached

#### Scenario: A lesser loss is degraded
- **WHEN** a group would lose part of its semantics on a target, other than a missing event or a `deny` or `ask` it cannot preserve
- **THEN** the route is degraded with the reason, and the group is not attached

#### Scenario: Input transformation is reported, not attached
- **WHEN** a group declares input transformation
- **THEN** UZE reports that the target cannot preserve it, since no target does today, and does not attach the group

### Requirement: Command hooks use one portable ABI
The system SHALL hand a command handler the normalized hook context as
`HOOK_*` environment and SHALL read its decision from its exit code: `0`
allows, the canonical deny exit denies with the reason on stderr, and any
other status is a handler failure resolved by the group's effect. The
reason read back SHALL be bounded, and each handler SHALL be bounded by its
declared timeout.

#### Scenario: First deny wins in deterministic order
- **WHEN** one group declares several pre-tool command handlers
- **THEN** they run sequentially in manifest order
- **AND** a deny stops the later handlers and the intercepted operation where the target supports blocking

### Requirement: A group is attached only with the effect the target preserves
Command hooks SHALL support observation, allow, deny, ask and input
transformation where the target supports them, and the system SHALL report
a group whose effect the target cannot preserve instead of attaching it.

#### Scenario: A target that cannot preserve the group's effect
- **WHEN** a group declares an effect the target has no way to honour
- **THEN** the group is reported and not attached

### Requirement: A deny group fails closed where the target can block on failure
Where the target offers a native way to block a call whose hook cannot be
started or exceeds the target's own timeout, a deny, ask or transform group
SHALL be delivered with it, so the group fails closed even when the
delivered handler itself never runs; an observational group SHALL NOT be.

#### Scenario: A deny group whose delivered handler is gone
- **WHEN** a deny group is delivered to a target that offers native block-on-failure and the delivered handler file is then deleted
- **THEN** the intercepted call is blocked, not allowed

#### Scenario: An observational group whose handler is gone
- **WHEN** an observational group's delivered handler cannot be started
- **THEN** the intercepted call proceeds

### Requirement: Managed Hook delivery preserves user configuration
The system SHALL add only receipt-owned Hook artifacts and configuration
entries. Reconciliation SHALL inspect the exact managed content and removal
SHALL refuse drift/conflicts and preserve unrelated user hooks/plugins.

#### Scenario: Existing OpenCode plugins survive bridge delivery
- **WHEN** OpenCode's global plugin directory already contains a foreign plugin file
- **THEN** UZE delivers its hooks as its own generated bridge file in that directory, `hooks-<package>.ts`, without touching the foreign file or adding a `plugin` entry to `opencode.json`
- **AND** removal deletes only the matching UZE bridge file, and only when its content still matches what UZE generated

### Requirement: Session start is a portable observational event

The portable hook manifest SHALL accept a `SessionStart` event. Its groups
SHALL accept only the `observe` effect; a manifest declaring any other effect
for `SessionStart` SHALL be refused at check and install with the reason. A
handler failure SHALL be reported and SHALL NOT prevent the session from
starting.

#### Scenario: A deny effect on session start is refused

- **WHEN** a manifest declares a `SessionStart` group with effect `deny`
- **THEN** `uze agent plugin check` reports the group and the allowed effect

#### Scenario: A failing session-start handler does not stop the session

- **WHEN** a `SessionStart` handler exits with a failure
- **THEN** the failure is reported and the session starts

### Requirement: A session-start handler receives the session, not a tool

Session-start handlers SHALL receive `HOOK_HARNESS`,
`HOOK_EVENT=session_start`, `HOOK_CWD`, `PLUGIN_ROOT` and, when the harness
reports it, `HOOK_SOURCE` (one of `startup`, `resume`, `clear`), and no tool:
`HOOK_TOOL` and every alias field are empty. A group MAY declare a matcher
listing sources; without one it fires for every source.

#### Scenario: A session-start handler runs on a new session

- **WHEN** a package declares a `SessionStart` group running
  `${PLUGIN_ROOT}/hooks/ensure-ui.sh` and a new Claude Code session starts
- **THEN** the handler runs once with `HOOK_EVENT=session_start` and
  `HOOK_SOURCE=startup`
- **AND** `HOOK_TOOL` is empty

#### Scenario: A matcher narrows the sources

- **WHEN** a `SessionStart` group's matcher lists only `resume` and a new
  session starts
- **THEN** the group's handlers do not run

### Requirement: An event a harness lacks does not block the package

When a harness offers no equivalent of a declared portable event, the system
SHALL deliver the package's other hooks and capabilities to that harness and
SHALL report the missing event as Unsupported for that harness with the
reason. The manifest SHALL NOT be refused for it.

#### Scenario: Antigravity and a session-start hook

- **WHEN** a package with `SessionStart` and `PreToolUse` groups is installed
  with Antigravity CLI detected
- **THEN** the `PreToolUse` group is delivered to Antigravity CLI
- **AND** the report lists `SessionStart` as Unsupported for Antigravity CLI
  with the reason

### Requirement: A handler may name a program and its words instead of a shell line
A command handler MAY give `args`, a list of words, beside `command`. With `args` present, `command` SHALL be the path of a script or executable inside the package, relative to its root, and each word of `args` SHALL reach the handler intact, whatever it contains.

#### Scenario: Words with spaces and quotes arrive intact
- **WHEN** the package root contains a space and an argument is `it's "$HOME"`
- **THEN** the handler receives the script path and that argument as single, literal words

### Requirement: The system chooses the program that runs an exec-form script on each platform
The system SHALL choose the program that runs an exec-form script for the platform it delivers to: on POSIX a file with an execute bit runs itself; otherwise the file's extension selects an interpreter from a fixed table, and an `interpreter` list given by the author replaces the table. A handler the system cannot place on a platform SHALL be treated there like a handler with no spelling for it.

#### Scenario: A Python guard written once runs on both platforms
- **WHEN** a deny group's handler is `{"type": "command", "command": "hooks/guard.py", "args": ["--strict"]}`
- **THEN** on Linux the wrapper runs `python3` with the script's absolute path and `--strict`
- **AND** on Windows it runs the Python the machine answers with (`py -3`, `python` or `python3`) with the same words

#### Scenario: A shebang decides on POSIX
- **WHEN** `hooks/guard` is executable and starts with `#!/usr/bin/env -S uv run --script`
- **THEN** the wrapper runs the file itself and no interpreter is added to the package's requirements

#### Scenario: A script nothing can run is not delivered
- **WHEN** a deny group's exec-form handler is `hooks/guard.sh` and the platform is Windows
- **THEN** the package is refused there exactly as for a deny group with no `windows` spelling

### Requirement: A Windows Python alias that only opens the Store is not a Python
When choosing Python on Windows, the system SHALL run each candidate and SHALL treat a candidate that does not answer with a Python 3 version as absent.

#### Scenario: Only the Store alias is present
- **WHEN** `python3` on `PATH` is the App Execution Alias and no Python is installed
- **THEN** the launcher reports Python missing as a requirement of the hook, with the command that installs it
- **AND** the delivered line names `python`, so it runs once Python is installed

### Requirement: The harness's own runtime runs JavaScript where it carries one
On a harness that runs plugins in an embedded JavaScript runtime, the system SHALL run a `.js`, `.mjs`, `.cjs` or `.ts` exec-form handler in that runtime and SHALL NOT require `node` for it.

#### Scenario: A JavaScript guard on OpenCode
- **WHEN** a handler is `{"command": "hooks/guard.js", "args": []}` and the harness is OpenCode
- **THEN** the handler runs in OpenCode's own runtime and the package's requirements gain no `node` for it

### Requirement: An exec-form handler's interpreter is a requirement of the package
The executable an exec-form handler's launcher needs SHALL join the package's effective requirements, attributed to the hook. An interpreter every supported machine of that platform carries SHALL add nothing. When the hook fails closed, the report SHALL state that the calls it matches are denied until the executable is present.

#### Scenario: Missing Python under a deny group
- **WHEN** a deny group matching `shell` uses `hooks/guard.py` and `python3` is absent
- **THEN** the package installs, `python3` is reported unmet and attributed to that hook
- **AND** the report states every matching call is denied until `python3` is installed
