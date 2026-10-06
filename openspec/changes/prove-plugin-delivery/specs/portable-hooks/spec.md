## ADDED Requirements

### Requirement: A portable alias fires on the harness's current tool

Each portable tool alias SHALL be bound, for each harness, to the native tools of that kind that the harness's current release declares, and to the input field names those tools carry. A group matching an alias SHALL run when the harness calls any of those tools. An alias that a harness has no tool for SHALL be unbound there, and a group that matches only unbound aliases SHALL be reported unsupported for that harness. It SHALL NOT be delivered with a matcher that can never fire.

#### Scenario: Subagent dispatch on Claude Code
- **WHEN** a group matching `agent.spawn` is delivered to Claude Code and the session dispatches a subagent
- **THEN** the group's handlers run with `HOOK_TOOL=agent.spawn`

#### Scenario: Shell on OpenCode V2
- **WHEN** a deny group matching `shell` is delivered to OpenCode and the session runs a shell command
- **THEN** the handler runs with `HOOK_COMMAND` set to the command the model asked for

### Requirement: A hook the harness holds back is reported as pending

When a harness will not run a delivered hook until the operator acts (a review, a trust decision, a feature switch), the system SHALL report that hook as pending, together with the action the operator must take in the harness, in the install report, `uze status`, `uze inspect` and `uze doctor`. It SHALL NOT report the hook as delivered. Changing a delivered hook in a way that makes the harness ask again (its content identity changes) SHALL be reported as pending again.

#### Scenario: Codex hook review
- **WHEN** a package's hooks are installed for Codex and Codex has not recorded trust for them
- **THEN** `uze status` names each hook as pending review in Codex and says how to review it
- **AND** once the operator trusts them in Codex, the same command reports them delivered

#### Scenario: An update changes the wrapper
- **WHEN** an update changes the content of a hook's delivered command in a way that makes Codex's recorded trust no longer match it
- **THEN** the update report names that hook as pending review again
