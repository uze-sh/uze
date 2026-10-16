## MODIFIED Requirements

### Requirement: An isolated agent is the subject of delivery
The system SHALL treat an isolated agent exactly as a task is treated
today: its work is delivered by the project's completion behaviour when the
project declares one, it can be named, and its checkout is a slot that is
reused when the agent ends. An agent that is not isolated SHALL be none of
those things.

Readiness is not among them. Where the work in a checkout stands is read
for every agent, isolated or not — what an unisolated one lacks is a
branch UZE cut, and therefore a delivery UZE may run.

#### Scenario: Delivery is offered for an isolated agent
- **WHEN** an isolated agent has commits its target does not have and the project declares a completion behavior
- **THEN** the same delivery the project's policy describes is offered for it

#### Scenario: An agent in the root is never delivered
- **WHEN** an agent that is not isolated has been working
- **THEN** no delivery is offered for it: its commits are already on the operator's branch
