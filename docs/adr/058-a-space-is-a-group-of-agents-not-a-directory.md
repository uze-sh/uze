# A space is a group of agents, not a directory

Status: Accepted

## Context

[053](053-isolation-belongs-to-the-agent-not-to-the-space.md) moved
isolation from the space to the agent, but kept the space a directory: one
root meant one space, and every agent created in it started from that root.
The domain had already stopped agreeing. The task store binds work to a
project, never to a space, and the git badge and the code, spec and
architect surfaces followed the agent in front rather than the root. Only
the workspace client still read `space.root` as "the project of this
space".

What that cost showed up in daily use. Work spanning several repositories
(a product's `front`, `back` and `landing`) could not sit together, and an
agent resumed or isolated from another space was moved to a space rooted at
its project.

## Decision

**A space is a named group of agents, bound to no directory.** Where it
works is read from its agents' directories.

- The runtime's space has no root. It is identified by an id that persists
  across a restart, and the persisted workspace keeps `next_space_id` so
  the id of a closed space is never reused.
- Creating a space asks for its name only. A new agent chooses its
  directory, with the harness as a field of the same popup.
- A space works in a project when a tab launched with an agent's
  environment stands in that project. A shell never counts, wherever it
  `cd`s. A launch takes a shell in the project only as a second choice, so
  `uze` started there twice does not open two spaces.
- Two spaces may work in one project, and one space may hold agents of
  several projects. Each agent stays in the space it was started in, and
  `CreateTab` names its space instead of following the client's selection.
- A project's actions belong to the agent in front: the strip's status,
  the surfaces and the overview read that agent's directory, and a space's
  own shells offer none of them.

The rejected alternative was a settable default directory per space. Every
use of it is answered by the tabs, and a stored default is a second record
that can disagree with them.

## Consequences

Work across repositories now sits in one space, and moving between spaces
no longer moves an agent's work. A space's caption counts its agents
rather than naming a directory. In a space of several projects, each row
names its project.

The persisted workspace climbed to shape 3 on
[052](052-what-uze-persists-is-tiered-by-what-deleting-it-costs.md)'s
ladder, and that step is a one-way door: a release older than this change
cannot read shape 3 and restores no tabs. It still leaves an agent's
checkout and its work alone. The client protocol moved to version 22, and a
server of an earlier protocol is replaced on attach, as for every protocol
bump.

Source change: openspec/changes/archive/2026-10-20-spaces-as-logical-groups/
