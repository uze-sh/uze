## Why

A space is a directory today: it is created by choosing a root, one root
is one space, and every agent created in it starts from that root. Work
that spans several repositories (a product's `front`, `back` and `landing`)
cannot sit together: each repository is its own space, and an agent of one
project resumed or isolated from another space is moved to a space rooted
at its project.

The domain already says the opposite. ADR-053 bound isolation to the agent,
and the task store binds work to a *project*, never to a space; the only
thing still tying a space to one directory is the workspace client, which
reads `space.root` as "the project of this space". The git badge, the
changes overlay and the code, spec and architect surfaces already follow
the agent in front rather than the root.

## What Changes

- **A space is a named group of agents, bound to no directory.** Where it
  works is read from its tabs' directories: two spaces MAY start agents in
  the same directory, and one space MAY hold agents of several projects.
  **BREAKING** for the rule "one root, one space".
- **Creating a space asks for its name, not its root.** Its first shell
  opens in the operator's home directory.
- **A new agent chooses its directory.** The new-agent flow asks for a
  directory before the harness, offering the existing directories and
  prefilled with the project of the tab in front, so the common case
  costs one confirmation.
- **An agent stays in the space it was started in.** A new, isolated,
  switched or resumed-in-place agent opens in its own space; work resumed
  from recent opens in the space already holding an agent of its project,
  or in the space the operator is looking at, and never opens a space of
  its own. **BREAKING** for "a resumed agent returns to its own project's
  space".
- **A project's actions belong to agents.** The strip's work status, the
  spec, arch and code surfaces and the overview are offered only while an
  agent's context is in front (the agent or a shell opened beside it), and
  read that agent's directory. A space's own shells are free to go
  anywhere and offer none of them. A space holding several projects names
  each agent's project.
- **Launching `uze` in a directory** selects the space already working in
  that project, and creates one only when none is.
- **The runtime's space loses its root** and keeps its id across a
  restart; the persisted workspace and the client layout climb the
  `uze-document` ladder, and the client protocol's version is bumped. **BREAKING** for a
  client of an earlier protocol, which the handshake already refuses.

## Capabilities

### New Capabilities
- `space-groups`: a space as a named group of agents across projects: the
  directory step of a new agent, where an agent lands, and what a space
  holding several projects shows.

### Modified Capabilities
- `terminal-runtime`: a space is identified by its id and has no
  directory; creating a space asks for a name;
  a launch, nested or not, finds the space working in its project.
- `agent-isolation`: a space has no root to start agents in; an agent
  starts in the directory chosen for it; resumed work no longer opens a
  space rooted at its project.
- `recent-work`: resuming lands in the space holding the project, or the
  one in front, and the selected row no longer says that resuming opens a
  space.

## Impact

- `crates/uze-terminal`: `Space.root` is removed, `SpaceSeat` and
  `open_space` match by the project a space's tabs stand in,
  `CreateSpace` takes a label, `SpaceSeed` keeps the space's id, the
  persisted workspace's shape 3 and `PROTOCOL_VERSION`.
- `crates/uze-workspace`: `client_layout` keys folded spaces by id.
- `src/ui/orchestrator`: the new-agent flow (`session/agents.rs`,
  `pickers.rs`, `root_picker.rs`), `land_agent_in_its_own_space`,
  `space_cwd` and its callers, the space overview, prompt history,
  `recent.rs`, the occupancy sweep and the sidebar caption.
- `src/main.rs`: the launch and the nested launch.
- Tests in `src/ui/orchestrator/tests/` and `crates/uze-terminal`, and the
  journeys in `journeys/suites/04-workspace/` that create spaces.
- The open `space-overview` change, rewritten in place: the overview is
  the agent's (an `agent` button, a header naming the agent, its own
  prompts only, no count of other agents).
- `crates/uze-workspace/src/prompt_history.rs`: clearing one agent's
  prompts.
