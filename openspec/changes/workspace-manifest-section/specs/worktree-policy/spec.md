## MODIFIED Requirements

### Requirement: Changing the policy from the popup is an explicit, versioned edit
The popup SHALL let the completion behavior be changed directly, and that
change SHALL write the project's manifest. Because the manifest is a
tracked file whose policy is projected into `AGENTS.md`, the popup SHALL
say so before writing — naming the file it will change and the
reconciliation the change still needs — rather than editing a tracked
file silently.

#### Scenario: Changing the behavior writes the manifest
- **WHEN** a completion behavior is chosen in the popup
- **THEN** `agents.yaml` records it as `workspace.delivery`, created first if the project had none

#### Scenario: The consequence is stated before the write
- **WHEN** a completion behavior is chosen
- **THEN** the popup names the file being changed and that `AGENTS.md`
  needs reconciliation to carry the new text

#### Scenario: The projected text is not silently left stale
- **WHEN** the policy has changed and `AGENTS.md` still carries the
  previous projection
- **THEN** the workspace client reports the projection as out of date

### Requirement: The workspace client shows the policy in effect and where it came from
The agent context popup SHALL name the completion behavior, the delivery
target and the gate command in force for the repository, and SHALL state
whether they come from the project's manifest or from the built-in
default. A policy that has never been declared SHALL read as the default,
not as absent.

#### Scenario: An undeclared policy reads as the default, attributed
- **WHEN** the popup opens in a repository whose manifest declares no
  policy
- **THEN** it shows the default completion behavior, attributed to the
  default rather than to the project

#### Scenario: A declared policy is attributed to the manifest
- **WHEN** the popup opens in a repository declaring `workspace.delivery: pr`
- **THEN** it shows `pr`, attributed to `agents.yaml`
