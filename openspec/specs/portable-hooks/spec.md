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
- **THEN** UZE delivers its hooks as a document of the package's groups, read by its one generated runtime plugin in that directory, without touching the foreign file or adding a `plugin` entry to `opencode.json`
- **AND** removal deletes only that package's document, and only when its content still matches what UZE generated

#### Scenario: A foreign file where the runtime plugin goes
- **WHEN** the file at the runtime plugin's path does not carry UZE's generated header
- **THEN** UZE does not write over it, attaches no OpenCode hook, and reports the hooks route blocked by that file

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

### Requirement: Delivered hooks run without the packager
The system SHALL deliver a package's portable hooks as a self-contained artifact in each harness's own hook form: the harness invokes a wrapper vendored inside the delivered artifact, never the `uze` binary, and the artifact SHALL contain no reference to the packager. A delivered hook SHALL keep working after the `uze` binary is removed from the machine, as long as the delivered artifact and the harness remain.

#### Scenario: Hook keeps working after UZE is uninstalled
- **WHEN** a package with a `deny` pre-tool hook was installed for Claude Code and the `uze` binary is subsequently removed from `PATH`
- **THEN** the harness still invokes the delivered wrapper for a matching tool call
- **AND** the handler's denial is relayed to the harness in its own contract and the tool does not run

#### Scenario: Native entry names only the delivered artifact
- **WHEN** the native hook configuration for a package is inspected on Claude Code, Codex or Antigravity
- **THEN** each entry's command resolves to a file inside the delivered artifact (`hooks/exec`) with the group's effect and the handler paths as arguments
- **AND** no entry contains the path of the `uze` executable

#### Scenario: OpenCode receives the runtime as a plugin
- **WHEN** a package with hooks is installed for OpenCode
- **THEN** the delivered artifact is a document of the package's groups, read by the one generated runtime plugin that carries the same contract
- **AND** the runtime runs the author's handlers with the same context and decision contract as the wrapper on the other harnesses

### Requirement: Handlers receive the hook context as environment and answer with exit codes
Command handlers SHALL receive the hook context as environment variables and SHALL NOT be required to parse any harness payload or emit any harness JSON. The wrapper SHALL set `HOOK_HARNESS`, `HOOK_EVENT`, `HOOK_TOOL` (the portable alias, or empty), `HOOK_TOOL_NATIVE`, `HOOK_CWD`, `HOOK_INPUT` (the tool input as JSON) and `PLUGIN_ROOT`, plus the portable fields of the matched alias. A handler's exit code SHALL be the decision: `0` allows, `3` denies with the reason read from stderr.

#### Scenario: A shell handler reads the portable command
- **WHEN** a `shell`-matched pre-tool hook fires on any harness for a command line
- **THEN** the handler observes `HOOK_TOOL=shell`, `HOOK_COMMAND` equal to that command line, `HOOK_TOOL_NATIVE` equal to the harness's own tool name and `HOOK_CWD` equal to the workspace directory
- **AND** the same handler, unchanged, observes the same variables on every other harness that delivers the hook

#### Scenario: Denial by exit code is relayed natively
- **WHEN** a handler of a `deny` group exits with code 3 and writes a reason to stderr
- **THEN** the harness receives its own blocking decision carrying that reason and the tool does not run
- **AND** the remaining handlers of the group are not started

### Requirement: A handler failure follows the group's effect
Any exit code other than `0` or `3`, a failure to start, or a timeout SHALL be a handler failure that follows the group's effect: fail-closed (a denial) for `deny` and `ask`, fail-open (the tool proceeds, the failure is reported) for `observe` and `allow`.

#### Scenario: A crashing deny handler still denies
- **WHEN** a handler of a `deny` group cannot start, exits with a code other than 0 or 3, or exceeds its timeout
- **THEN** the intercepted tool is denied and the reason names the failure
- **AND** a handler of an `observe` group failing the same way is reported and the tool proceeds

#### Scenario: Missing wrapper dependency follows the group's effect
- **WHEN** the wrapper's own dependency is not available on the machine (for the `sh` wrapper, `jq`)
- **THEN** a `deny` or `ask` group denies the intercepted tool with a reason naming the missing dependency
- **AND** an `observe` or `allow` group lets the tool proceed and reports the missing dependency

### Requirement: Handlers of one group run in order and the first denial stops the rest
Handlers of one group SHALL run sequentially in manifest order and the first denial SHALL stop the remaining handlers, regardless of how the harness schedules its own hooks.

#### Scenario: A later handler never runs after a denial
- **WHEN** a group lists two handlers and the first exits with code 3
- **THEN** the second handler is not started, on every harness that delivers the hook

### Requirement: The portable tool vocabulary defines fields
The system SHALL maintain one vocabulary of portable tool aliases in which each alias names the portable fields it guarantees to handlers and, per harness, the native tool name and the native field each portable field is read from. Matchers, generated wrappers and compatibility assessment SHALL derive from this vocabulary alone. A tool matched through `native:<name>` SHALL yield only `HOOK_TOOL_NATIVE` and `HOOK_INPUT`, with `HOOK_TOOL` empty.

#### Scenario: Alias fields are the same on every harness
- **WHEN** the vocabulary defines `shell` with the portable field `command` and `file.write` with `path`
- **THEN** a handler matched on `shell` receives `HOOK_COMMAND` on every harness and a handler matched on `file.write` receives `HOOK_PATH` on every harness, each read from that harness's native field name

#### Scenario: Native escape hatch exposes raw input only
- **WHEN** a group matches `native:Write` on Claude Code
- **THEN** the handler receives `HOOK_TOOL_NATIVE=Write`, `HOOK_INPUT` with the raw tool input and an empty `HOOK_TOOL`
- **AND** no portable field variable is set

#### Scenario: Conformance proves each vocabulary row
- **WHEN** the conformance Lab runs a harness vertical
- **THEN** for every alias the harness delivers, a real tool call of that alias reaches a handler that asserts the guaranteed `HOOK_*` values
- **AND** an alias the harness cannot deliver is declared with a reason, never omitted

### Requirement: The wrapper is the only implementation of the contract
The system SHALL carry no hook runtime of its own: the generated wrapper (and, on a harness without command hooks, the generated plugin that is the same runtime) SHALL be the only thing that runs an authored handler, and no packager binary SHALL appear on any delivered hook's execution path. Where no wrapper template applies to the target platform or harness, the hook SHALL be reported Unsupported with that reason and nothing SHALL be attached.

#### Scenario: Platform without a wrapper template delivers nothing
- **WHEN** a hook is delivered on a platform or harness for which no wrapper template exists
- **THEN** no native hook entry is written
- **AND** the hook is reported Unsupported with the reason that no wrapper template covers this platform

### Requirement: The wrapper's answers are held as goldens per harness
The answer the wrapper gives for each recorded fixture (the native decision document, the exit status and the reason) SHALL be held as a golden per harness, so a changed answer is a reviewed change of contract.

#### Scenario: Every recorded fixture answers as recorded
- **WHEN** the generated wrapper is run against a recorded fixture payload for a harness
- **THEN** its native decision document, exit status and reason are the recorded ones

### Requirement: Each handler is bounded by the deadline its author declared
The generated wrapper SHALL run each handler under the `timeout` its author declared in `hooks.json`, and SHALL stop a handler that exceeds it — together with the processes that handler started — before continuing. A handler stopped that way SHALL be a handler failure, resolved by the group's declared effect and reported with a reason naming the deadline. The native entry's own group timeout SHALL remain the harness's backstop and SHALL be sized so it is never the bound that fires first.

#### Scenario: A hanging handler in a deny group blocks at its deadline
- **WHEN** a `deny` group's handler declares a 1s timeout and does not answer
- **THEN** the intercepted tool is denied about a second later, with a reason naming the timeout
- **AND** the same handler in an `observe` group lets the tool proceed and the timeout is reported

### Requirement: OpenCode runs every package's hooks in one pass
On OpenCode, the system SHALL run the groups of every installed package for
an event in one pass, in a deterministic order, so a denial by any package
stops the handlers after it and reaches the call exactly once.

#### Scenario: Two packages guard the same tool
- **WHEN** package `a` delivers a deny group and package `b` an observe group, both matching `shell`, and `a`'s handler denies a call
- **THEN** the call does not run and the denial reason reaches the model
- **AND** `b`'s observer sees the call under its own tool name or does not run, never under a substituted one

#### Scenario: A package installed while the service runs
- **WHEN** a package with hooks is installed while OpenCode's background service is running
- **THEN** the next matching tool call runs its groups without the service being restarted

### Requirement: An unreadable hook document fails closed
When a package's hook document cannot be read or parsed on OpenCode, the
system SHALL refuse every tool call, naming the document, until it is
generated again: which calls the package guarded is unknown without it.

#### Scenario: A corrupted document
- **WHEN** a package's hook document holds invalid JSON
- **THEN** a tool call is refused with a reason naming that document
- **AND** other capabilities the runtime carries keep working
