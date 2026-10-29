## ADDED Requirements

### Requirement: A project's own agents reach the harness, in that project alone
Every Markdown file in a project's `.agents/agents` SHALL be offered to
each supported harness as a subagent named by its frontmatter `name` or
file stem, in sessions of that project only, read by the harness itself or
projected by UZE with a package agent's portable subset, and never by
writing into the checkout.

#### Scenario: Offered in its own project
- **WHEN** a project holds `.agents/agents/reviewer.md` and a session starts in that project on any supported harness
- **THEN** the harness offers `reviewer` to the model

#### Scenario: Dispatched on OpenCode
- **WHEN** an OpenCode session dispatches a project agent
- **THEN** the subagent's own request carries the file's body as its system prompt

#### Scenario: A linked checkout reads its own branch
- **WHEN** an OpenCode session starts in a linked checkout whose branch changed `.agents/agents`
- **THEN** that checkout's agents are offered, not the main checkout's

#### Scenario: Two projects on one OpenCode service
- **WHEN** projects `a` and `b` each hold their own agent and OpenCode serves both from its shared background service
- **THEN** a session in `a` is offered `a`'s agent and not `b`'s, and a session in `b` the reverse

#### Scenario: A session below the checkout root
- **WHEN** an OpenCode session starts in a subdirectory of a checkout whose root holds `.agents/agents`
- **THEN** the root's agents are offered

#### Scenario: An agent written while OpenCode runs
- **WHEN** a file is added to `.agents/agents` while OpenCode's service is serving that project
- **THEN** the next session turn in that project is offered it without a restart

#### Scenario: A built-in agent's name
- **WHEN** a project agent's name is one of OpenCode's built-in agents
- **THEN** the built-in agent is kept and the project's is not registered

#### Scenario: A configured agent's name
- **WHEN** a project agent's name is one OpenCode's own configuration also defines
- **THEN** the configured fields apply over the project's, as OpenCode's precedence orders them

#### Scenario: OpenCode's route
- **WHEN** support is reported for project agents on OpenCode
- **THEN** the harness matrix reads Adapted, and the workspace and `uze doctor` read delivered while UZE's runtime plugin is in place

#### Scenario: The runtime plugin is not in place
- **WHEN** UZE's runtime plugin is missing from OpenCode's plugin directory
- **THEN** the workspace and `uze doctor` say to run `uze setup` for project agents, not that they are unsupported

#### Scenario: A foreign file holds the plugin's path
- **WHEN** a file UZE did not generate sits where the runtime plugin goes
- **THEN** project agents are reported blocked, naming that file, and setup does not write over it
