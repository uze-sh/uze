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
reason when the target cannot preserve the group. A target that lacks the
event, and a `deny` or `ask` group whose semantics the target cannot
preserve, SHALL be `unsupported`; any other loss SHALL be `degraded`. A
`degraded` or `unsupported` group SHALL NOT be attached.

#### Scenario: Stop has no false OpenCode equivalence
- **WHEN** a portable Stop hook is assessed for OpenCode
- **THEN** the result is unsupported with the reason that the target has no `stop` semantic event
- **AND** UZE SHALL NOT claim that a session or tool callback is a native Stop hook

### Requirement: Command hooks use one portable ABI
The system SHALL hand a command handler the normalized hook context as
`HOOK_*` environment and SHALL read its decision from its exit code: `0`
allows, the canonical deny exit denies with the reason on stderr, and any
other status is a handler failure resolved by the group's effect. The reason
read back SHALL be bounded, and each handler SHALL be bounded by its declared
timeout. It SHALL support observation, allow, deny, and ask where the target
supports them, and SHALL report a group whose effect the target cannot
preserve (including input transformation, which no target preserves today)
instead of attaching it.

#### Scenario: First deny wins in deterministic order
- **WHEN** one group declares several pre-tool command handlers
- **THEN** they run sequentially in manifest order
- **AND** a deny stops the later handlers and the intercepted operation where the target supports blocking

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
for `SessionStart` SHALL be refused at check and install with the reason.
Handlers SHALL receive `HOOK_HARNESS`, `HOOK_EVENT=session_start`, `HOOK_CWD`,
`PLUGIN_ROOT` and, when the harness reports it, `HOOK_SOURCE` (one of
`startup`, `resume`, `clear`), and no tool: `HOOK_TOOL` and every alias field
are empty. A handler failure SHALL be
reported and SHALL NOT prevent the session from starting. A group MAY declare
a matcher listing sources; without one it fires for every source.

#### Scenario: A session-start handler runs on a new session

- **WHEN** a package declares a `SessionStart` group running
  `${PLUGIN_ROOT}/hooks/ensure-ui.sh` and a new Claude Code session starts
- **THEN** the handler runs once with `HOOK_EVENT=session_start` and
  `HOOK_SOURCE=startup`

#### Scenario: A deny effect on session start is refused

- **WHEN** a manifest declares a `SessionStart` group with effect `deny`
- **THEN** `uze agent plugin check` reports the group and the allowed effect

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

