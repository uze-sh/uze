## MODIFIED Requirements

### Requirement: Authoring verbs are deterministic, agent-facing, and unaliased for people
The system SHALL expose plugin authoring only as deterministic verbs under `uze agent …`: each verb takes its full input on the command line (or its defaults), performs exactly one artifact-producing step, prints a machine-readable answer, and never prompts interactively. The authoring verbs SHALL be hidden from `uze --help` like the rest of the `agent` grammar, documented in a region the package manager projects into the `AGENTS.md` of every project UZE manages, whether or not the project declares a workspace policy, and SHALL NOT gain person-facing aliases: the person's CLI surface carries only what a person performs by hand, and browsing marketplaces and their plugins remains the `uze market` surface's answer. None of the authoring verbs SHALL require an agent the workspace launched.

#### Scenario: An agent drives the full authoring loop without a person's input
- **WHEN** an agent runs `uze agent market create`, then `uze agent plugin create`, then `uze agent plugin check`, providing all arguments up front
- **THEN** each command performs its one step and exits zero with a summary an agent can parse, and no command ever waits for interactive input

#### Scenario: Authoring verbs stay off the person's help surface
- **WHEN** a person runs `uze --help` or `uze agent --help`
- **THEN** the authoring verbs are not listed; they are reachable only by following the `AGENTS.md` region, and no person-facing alias of them exists — browsing what a marketplace offers is `uze market`'s answer, not a second authoring spelling

#### Scenario: A project without a workspace policy documents authoring
- **WHEN** a project that declares no workspace policy reconciles its context
- **THEN** its `AGENTS.md` carries the package manager's region documenting the authoring verbs

#### Scenario: An agent started by hand authors a plugin
- **WHEN** an agent a person started by hand follows that region
- **THEN** every verb it names runs without asking for a launched agent
