## ADDED Requirements

### Requirement: The client shows a checkout only through the agent holding it
The workspace client SHALL show a subagent's checkout under the agent it
belongs to, and SHALL show the checkout an agent holds only as an
attribute of that agent, as `recent-work` describes. The client SHALL NOT
list checkouts no agent holds, and SHALL NOT offer to open a space in a
checkout, to adopt one, to remove one, or to clean up the operator's
checkouts. The only destructive action the client offers on an agent's
checkout is discarding that agent's work, on the operator's explicit
confirmation. Reading a checkout's facts SHALL never hold up drawing.

#### Scenario: A foreign checkout is left alone
- **WHEN** a project holds a harness's own worktree and one the operator added by hand
- **THEN** the client lists neither and changes neither

#### Scenario: An agent's checkout is an attribute of the agent
- **WHEN** an agent with no live pane holds a checkout
- **THEN** the client shows that checkout only as the disk figure on the agent's selected row

#### Scenario: Discarding is the only removal offered
- **WHEN** the operator acts on an agent's kept work from the client
- **THEN** discarding it is the only action that removes its checkout, and it happens only on confirmation

## REMOVED Requirements

### Requirement: The operator sees every checkout and acts on it explicitly
**Reason**: The per-project checkout view and its actions duplicated what
the system already does on its own (adoption at startup, collection of
free slots and integrated branches on every pass) or acted on checkouts
that are not agents' work. Kept work is now reached through the agents
holding it, in the sidebar's recent mode.
**Migration**: Agents' work is resumed, joined, delivered or discarded
from recent. The operator's own checkouts and a harness's isolation are
managed with Git (`git worktree remove`), as they always could be; UZE's
slots need no action.
