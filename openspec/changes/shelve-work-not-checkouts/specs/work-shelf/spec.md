## Purpose

Keeps the work an isolated agent leaves behind in Git rather than in the
checkout it ended in, so that a checkout is only capacity: what an agent's
uncommitted work becomes when it ends, how it is put back, and how it is
found, collected and discarded.

## ADDED Requirements

### Requirement: An agent that ends holding uncommitted work has it shelved
When an isolated agent ends and its checkout holds uncommitted work, the
system SHALL record that work as a shelf for the agent's task before the
checkout is freed. Uncommitted work SHALL be every change the repository
would report against the branch tip: tracked files changed, added or
deleted, and files it does not track and does not ignore; and commits on a
detached `HEAD` that no branch reaches, which the shelf keeps reachable.

#### Scenario: A modified file and a new file are shelved
- **WHEN** an agent ends with one tracked file modified and one new file the repository does not ignore, and no pane carries it
- **THEN** the task holds a shelf containing both changes
- **AND** the task's branch points where it pointed before
- **AND** the repository's stash list is unchanged

#### Scenario: Shelving fails
- **WHEN** the shelf cannot be written
- **THEN** the checkout is not freed, nothing in it is changed, the task still holds it, and the operator is told why

#### Scenario: Commits no branch reaches
- **WHEN** an agent ends on a detached `HEAD` carrying commits no branch reaches, with a clean tree
- **THEN** those commits are reachable from the task's shelf, and the checkout is free

### Requirement: Derived, ignored and uncapturable work is not shelved
Content UZE derives, as `checkout-accounting` defines it, SHALL NOT be
shelved. Files the repository ignores SHALL NOT be shelved and SHALL stay
in the checkout. Work that cannot be captured in a shelf (a submodule with
changes or commits of its own, or a repository nested inside the checkout)
SHALL NOT be shelved, and the checkout SHALL be pinned with that reason
(`worktree-policy`).

#### Scenario: Ignored output stays where it was built
- **WHEN** an agent ends with only ignored build output in its checkout
- **THEN** no shelf is made and the checkout is free with that output in place

#### Scenario: Derived content alone is not shelved
- **WHEN** an agent ends with only a lock file change that moves no pin
- **THEN** no shelf is made and the checkout is free

#### Scenario: A submodule with changes of its own pins the checkout
- **WHEN** an agent ends with a submodule in its checkout holding changes or commits of its own
- **THEN** that work is not shelved and the checkout is pinned with that reason

### Requirement: Shelving leaves the branch, the stash and other checkouts alone
Shelving SHALL NOT move the task's branch, SHALL NOT create a commit on it,
SHALL NOT use the repository's stash, and SHALL NOT touch any other
checkout. A shelf SHALL stay on this machine: nothing is pushed.

#### Scenario: Shelving reaches nothing beyond its own checkout
- **WHEN** an agent's work is shelved while another checkout of the project holds changes of its own
- **THEN** the other checkout is unchanged, the task's branch has no new commit, and nothing is pushed to any remote

### Requirement: A task holds one shelf, and a shelf is never replaced
A shelf SHALL name the task, its label and its branch. Whether a change was
staged SHALL NOT be recorded. A task SHALL have at most one shelf, and a
shelf SHALL never be replaced: work shelved for a task that already holds
one SHALL keep the earlier shelf reachable from the new one.

#### Scenario: A task shelved twice
- **WHEN** a task that already holds a shelf ends again holding uncommitted work
- **THEN** the task holds one shelf, and the earlier shelf is reachable from it

### Requirement: A shelved task frees its checkout
Once a task's uncommitted work is shelved, and whatever commits its branch
holds that the target lacks, the task's checkout SHALL be free for the next
agent and the task SHALL no longer hold it, unless a pin `worktree-policy`
names holds it.

#### Scenario: A week of agents
- **WHEN** over many sessions at most four agents work at once, and some of them end with uncommitted work and some with commits the target lacks
- **THEN** the project never has more than four checkouts at a time
- **AND** every task that ended holding work is listed as shelved

#### Scenario: Local commits alone
- **WHEN** an agent ends with commits the target lacks and a clean tree
- **THEN** no shelf is made, the branch keeps the commits, the task is shelved and its checkout is free

### Requirement: Whether anybody works in a checkout is asked of the machine
Whether anybody is working in a task's checkout SHALL be read from the
processes on the machine at the moment of shelving, not only from what one client
knows: a process working in it, an agent another client launched, or an
agent whose work was delivered but whose tab is still open SHALL leave the
checkout and its agent untouched.

#### Scenario: Somebody is still at work
- **WHEN** a client no longer shows an agent's tab but a process is still working in its checkout
- **THEN** nothing is shelved or reset there, and the agent is not ended

### Requirement: A task holds work while its branch or its shelf holds what the target lacks
A task SHALL hold work while its branch holds commits its target lacks or
its shelf holds changes its target lacks; such a task SHALL be shelved,
SHALL NOT be settled as integrated, and its branch SHALL NOT be pruned. A
task holding neither SHALL be closed, or integrated when what it held
reached the target.

#### Scenario: A merged branch with work on the shelf
- **WHEN** a shelved task's branch reaches the target while its shelf holds changes the target lacks
- **THEN** the task stays shelved, and neither its branch nor its shelf is removed

#### Scenario: A task that holds nothing is settled
- **WHEN** an agent ends with a clean tree and no commits the target lacks
- **THEN** its task is closed, or integrated when what it held reached the target

### Requirement: Shelved work is resumed in any free checkout
Resuming a shelved task SHALL place its branch in the checkout the task
last held when that one is free, otherwise in any free checkout, or in a
new one when none is free and the cap allows it, and SHALL put the shelf
back as uncommitted changes, all of them unstaged, then remove the shelf.
A branch that no longer exists SHALL be recreated where the shelf was cut.
The resumed task SHALL be live again.

#### Scenario: Resumed somewhere else
- **WHEN** a task is shelved, its old checkout is taken by another agent, and the operator resumes it
- **THEN** the task works in a different checkout, on its branch, with every shelved change present and unstaged

#### Scenario: Back where it was
- **WHEN** a shelved task is resumed while the checkout it last held is free
- **THEN** it resumes in that checkout

### Requirement: A shelf is put back on top of where its branch stands
When the branch moved since the shelf was made, the shelf SHALL be applied
as a change on top of where the branch stands; when that conflicts, the
resume SHALL be refused naming the conflicting files, the shelf kept, and
the checkout it tried left free.

#### Scenario: The branch moved since the shelf was made
- **WHEN** a shelved task's branch gained commits after its shelf was made, touching other files, and the operator resumes it
- **THEN** the task's checkout stands on the branch's current tip with every shelved change on top, unstaged

#### Scenario: Nothing is lost on a conflict
- **WHEN** a shelf cannot be applied cleanly onto where its branch stands
- **THEN** the resume is refused naming the conflicting files, the shelf still exists, the task stays shelved holding no checkout, and no shelved change is lost

### Requirement: A resumed conversation follows where its harness can find it
A conversation its harness can only find from the directory it was held in
SHALL be resumed in that directory, and started anew with a note when the
task resumes elsewhere.

#### Scenario: Resumed away from the directory its conversation is bound to
- **WHEN** a shelved task whose harness finds its conversation only from the directory it was held in is resumed in a different checkout
- **THEN** the agent starts a new conversation, with a note saying why

### Requirement: A shelf is never orphaned
A shelf SHALL describe itself well enough to be listed without the task
record that made it. When reconciliation finds a shelf no recorded task
holds, it SHALL record a shelved task for it from what the shelf names,
under the task identity the shelf names, so it is adopted once.

#### Scenario: The task store was lost
- **WHEN** a project's task records are deleted while a shelf exists
- **THEN** after the next reconciliation the shelf's work is listed as a shelved task under its label and branch

#### Scenario: The branch was deleted by hand
- **WHEN** a shelved task's branch is deleted outside the system
- **THEN** its shelf remains, and the task is still listed as shelved

### Requirement: A shelf is removed only when the target has it or its task is discarded
A shelf whose every change the target already carries, path by path,
deletions and renames included, SHALL be collected with no operator action.
Any other shelf SHALL be removed only when the operator discards its task;
discarding a task SHALL remove its branch and its shelf.

#### Scenario: A path that only looks alike
- **WHEN** a shelf changes a path whose name reads as a pattern, and the target carries the same content at a path that pattern would match
- **THEN** the shelf is kept

#### Scenario: The target already has it
- **WHEN** every change in a shelf is already in the target
- **THEN** the shelf is removed without an operator action

#### Scenario: Discard removes everything the task held
- **WHEN** the operator discards a shelved task
- **THEN** its branch and its shelf are gone, and no checkout is affected
