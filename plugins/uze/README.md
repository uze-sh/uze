# UZE, the official UZE plugin

This is a normal Agent Plugins 1.0 package. It carries no special treatment in
the Store, the router, or any integration: it is installed, discovered and
delivered exactly the way any other Skill-only package is.

It ships six Skills:

- `skills/init/SKILL.md`: an agentic orchestrator that calls UZE's own
  deterministic `uze agent context inspect|plan|reconcile` CLI to make a project's
  instructions context portable.
- `skills/worktree/SKILL.md`: coordinates isolated worktrees for concurrent
  agent work and safe integration, honouring the `workspace:` policy the
  project declares in `agents.yaml`.
- `skills/rules/SKILL.md`: writing a project's rules under `.agents/rules/`,
  the instructions that arrive only where they are relevant, proven with
  `uze agent rules check`.
- `skills/architect/SKILL.md`: writing the Mermaid diagrams a project keeps
  under `workspace.artifacts`: which view a change belongs in, how the architect
  surface lists it, and `uze agent artifacts check` as the proof it draws.
- `skills/author/SKILL.md`: creating a plugin end to end, from choosing or
  creating its marketplace to checking, installing and iterating on it;
  `references/extension.md` beside it covers writing a declared extension
  the plugin carries: its manifest, a script or a program, its grants, its
  tests, and the consent and enabling it needs before it runs.
- `skills/try/SKILL.md`: making a project to try the marketplace's example
  plugins in, with `demo.py` beside it writing the project (a Git history
  and a milestone in `.planning/`), and installing the examples there.

The plugin format (skills, agents, MCP servers, hooks, and the
`${PLUGIN_ROOT}` placeholder) and what each harness receives are documented
at [uze.sh/docs/reference/plugin-format](https://uze.sh/docs/reference/plugin-format)
and [uze.sh/docs/reference/harnesses](https://uze.sh/docs/reference/harnesses).

See [`docs/capabilities/context-manager.md`](https://github.com/uze-sh/uze/blob/main/docs/capabilities/context-manager.md)
for the architecture these sit on top of, and
[`docs/capabilities/uze-skill.md`](https://github.com/uze-sh/uze/blob/main/docs/capabilities/uze-skill.md) for how
they are invoked per harness, the no-special-treatment proof, and what is not
tested.
