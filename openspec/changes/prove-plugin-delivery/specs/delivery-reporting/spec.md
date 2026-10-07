## ADDED Requirements

### Requirement: Context a harness will not load is reported

When a harness will not load the project context that UZE reconciled into the current project, the system SHALL report that context as not reaching that harness in `uze status`, with the reason and the operator's action. Examples are a project the harness has not been told to trust, a setting that turns the context file off, and a file over the size the harness reads. It SHALL NOT report the context as delivered to that harness.

#### Scenario: Codex in an untrusted project
- **WHEN** `uze status` runs in a project whose `AGENTS.md` is reconciled and Codex has not trusted that project
- **THEN** the report says Codex will not read the project context until the project is trusted, and how to trust it

#### Scenario: A context file over the harness's limit
- **WHEN** the reconciled `AGENTS.md` exceeds the size a harness reads
- **THEN** the report names that harness, the limit, and that the remainder is not read
