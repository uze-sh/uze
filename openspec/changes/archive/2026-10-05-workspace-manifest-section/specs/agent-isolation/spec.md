## MODIFIED Requirements

### Requirement: Isolation is an action on one agent
The system SHALL offer, for a single agent, an action named **To worktree**
that gives it a checkout of its own: a slot is acquired as the project's policy says, the
agent's record gains its isolation — the checkout, the branch and what the
branch was cut from — and the agent is relaunched there. The agent's
identity SHALL NOT change, and its conversation record SHALL stay that
agent's.

Whether the *harness's* conversation follows is the harness's answer, not
UZE's: one that binds a conversation to the directory it ran in cannot
resume it in the new checkout and starts a fresh one there. UZE SHALL
resume where the harness can and start a new conversation where it
cannot, never refusing the isolation over it.

The action SHALL be offered only where it can be honoured: inside a Git
working tree with a commit to branch from. Where a slot cannot be
acquired, the agent SHALL be left exactly where it is and the reason
stated.

#### Scenario: An agent is isolated
- **WHEN** the operator isolates an agent running in the space's root
- **THEN** a checkout is created for it, its record carries that isolation, and it is relaunched there
- **AND THEN** it is the same agent: the same identity, and the same conversation record

#### Scenario: A harness that binds its conversation to a directory
- **WHEN** an agent whose harness stores its conversation under the directory it ran in is isolated
- **THEN** the agent is relaunched in the checkout with a conversation of its own, and the isolation is not refused

#### Scenario: Isolation is not offered where it cannot be honoured
- **WHEN** the space's root is not a Git working tree, or has no commit to branch from
- **THEN** the action is not offered

#### Scenario: A slot cannot be acquired
- **WHEN** isolating an agent and the checkout cap is reached, or Git refuses
- **THEN** the agent keeps running where it was, and the operator is told why

Isolating SHALL take a copy of the root's uncommitted changes into the
checkout. At the moment an agent is moved, what the tree holds is
usually what that agent was doing, and a checkout without it is one
where the file it was mid-edit on has gone back to its last commit.

Cutting from the last commit instead SHALL be offered as a second
answer, named **To worktree, no changes**, and only where the tree is known to hold uncommitted work. The
knowledge is the evaluation's, never a Git read taken while the surface
draws, so it MAY be up to a refresh old — which is why it gates this
answer and not the carrying one: a stale *clean* reading costs the
operator nothing, where the same staleness on the carrying answer would
take away the very thing they had just edited. Neither answer takes
anything from the operator's own tree.

#### Scenario: The work follows the agent
- **WHEN** an agent is isolated while the space's root has uncommitted changes
- **THEN** a copy of them is in the checkout — edits to tracked files and new files the repository does not ignore alike
- **AND THEN** the root's own working tree is left exactly as it was, whichever answer is given
- **AND THEN** nothing is discarded

#### Scenario: A clean tree asks nothing
- **WHEN** an agent is isolated while the space's root has no uncommitted changes
- **THEN** the action carries the one answer, and neither tree gains or loses anything

#### Scenario: The work is not this agent's to take
- **WHEN** the root is known to hold uncommitted changes and the operator cuts from the last commit
- **THEN** the checkout holds the commit alone, and the changes stay in the root they were made in

#### Scenario: The actions say where the agent goes and what stays
- **WHEN** the menu of an agent in the space's root is opened while the root holds uncommitted changes
- **THEN** it offers "To worktree" and "To worktree, no changes", and no label speaks of isolating or cleaning

### Requirement: A project may declare that its agents start isolated
The system SHALL let a project declare, as `workspace.worktree` in its
manifest, when an agent launched in it gets a worktree: `always`, at
launch, or `manual`, only when the operator moves it with **To worktree**.
Where it declares `always`, an agent SHALL be placed in a slot at launch,
and the isolation action SHALL have nothing to offer. Where it declares
`manual`, or nothing, an agent SHALL start in the root. No other value
SHALL be accepted, and a refusal SHALL name both.

#### Scenario: A project that isolates by default
- **WHEN** a project's manifest declares `workspace.worktree: always` and an agent is launched
- **THEN** the agent is placed in a checkout of its own at launch

#### Scenario: A project that declares nothing
- **WHEN** a project's manifest declares no `workspace.worktree` and an agent is launched
- **THEN** the agent starts in the space's root

#### Scenario: A manual project moves agents on request
- **WHEN** a project declares `workspace.worktree: manual` and the operator chooses "To worktree" on an agent
- **THEN** that agent alone is moved to a checkout of its own

#### Scenario: A value that is not a choice
- **WHEN** `workspace.worktree` holds `isolated` or `in-place`
- **THEN** the workspace refuses the declaration, naming `always` and `manual`
