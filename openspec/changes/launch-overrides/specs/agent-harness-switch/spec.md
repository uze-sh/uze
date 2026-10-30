## ADDED Requirements

### Requirement: Overrides do not cross harnesses
The system SHALL launch an agent continued on another harness with the
overrides recorded for it on that harness, never the ones it carried on
the harness it leaves, since a flag belongs to one harness, and SHALL say
on its tab that those were left behind. The restart panel SHALL show the
incoming harness's recorded set before the operator confirms.

#### Scenario: Continuing an agent launched with a model flag
- **WHEN** an agent launched on one harness with `--model opus` is continued in another harness it never ran on
- **THEN** the new harness starts with no overrides and the tab says the earlier ones were left behind

#### Scenario: Switching back
- **WHEN** that agent is later continued in its first harness again
- **THEN** the panel holds `--model opus` checked, and the agent starts with it

## MODIFIED Requirements

### Requirement: An agent can be continued on another harness
The system SHALL offer, in the menu of a running agent UZE launched, one
`Restart in` entry opening, beside the menu, the restart panel: the
harness field on the agent's current harness, switchable to any other
detected harness, and the overrides for the harness it names. Confirming
the panel on another harness SHALL relaunch the same agent on that
harness in the same checkout, on the same branch and under the same
label, replacing the tab. Harnesses that are not detected SHALL NOT be
offered.

#### Scenario: Continuing a Claude agent in Codex
- **WHEN** the operator opens `Restart in`, moves the harness field to another detected harness and confirms
- **THEN** a tab running the chosen harness replaces the agent's tab, standing in the same checkout on the same branch with the same label

#### Scenario: Only detected harnesses are offered
- **WHEN** the operator walks the restart panel's harness field
- **THEN** it offers the agent's current harness first and every other detected harness, and no harness that is not detected

#### Scenario: An exited agent offers no switch
- **WHEN** the agent in a tab has exited
- **THEN** the tab's menu offers no `Restart in`

### Requirement: An agent's menu opens from the keyboard
The system SHALL open the selected agent's menu, the one its row opens with
the pointer, from a key, and SHALL let every entry, every entry of a group
it opens and every field of a panel it opens be reached and taken from the
keyboard alone.

#### Scenario: Continuing an agent without the pointer
- **WHEN** the operator presses the menu key on a selected agent, moves to `Restart in`, opens it, moves the harness field and presses Enter
- **THEN** the agent continues on that harness exactly as if the panel had been used with the pointer

#### Scenario: Backing out of a group
- **WHEN** the operator opens `Restart in` and then goes back or presses Escape
- **THEN** the panel closes and the menu stays open on the entry that opened it

### Requirement: Switching a busy agent asks first
The system SHALL ask the operator to confirm a switch or a restart of an
agent that is mid-turn, saying that its current work in progress will be
interrupted, and SHALL switch or restart an idle agent without asking.

#### Scenario: A busy agent
- **WHEN** the operator switches or restarts an agent that is mid-turn
- **THEN** it waits for confirmation and does nothing if it is declined

#### Scenario: Switching to a harness without continuity
- **WHEN** the chosen harness declares no continuity
- **THEN** the operator is told that its conversation will not be resumed on a later launch
