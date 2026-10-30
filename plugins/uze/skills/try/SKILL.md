---
name: try
description: Sets up a project to try uze's example plugins in — gsd, focus, contributions — so the person sees what extensions do in the workspace without wiring anything of their own. Use when the person asks to try uze, to see example extensions or plugins, to set up a demo or playground project, or invokes /uze:try or $uze:try directly.
invoke:
  model: true
  user: true
---

# UZE — trying the example plugins

uze's own marketplace, `uze-sh/uze`, carries besides the `uze` plugin a
handful of examples (category `example`). Each one needs a project with
something in it to show:
`contributions` draws a repository's commit history, `gsd` a GSD plan in
`.planning/`. This Skill makes that project and installs the examples into
it. The person then opens the workspace.

| Example | Where it shows | What it does |
|---|---|---|
| `gsd` | bar, drawer, band | a GSD milestone in phases; asks the agent for the next step, and takes a checkpoint's answer over its pane. Its steps need GSD itself, installed by the person |
| `focus` | bar, drawer | a pomodoro, with the last seven days |
| `contributions` | bar, drawer | a map of twenty weeks of commits; click a day for its commits |
| `pingpong` | the harness | one of every capability a plugin delivers; its MCP server is built from a uze checkout |

## Steps

1. **uze is there.** `uze --version`. Without it, stop and point the person
   to <https://uze.sh>: installing uze is theirs to do.
2. **The marketplace is there.** `uze market list`; with no marketplace
   named `uze`, run `uze market add uze-sh/uze`. Not the built-in
   `uze`, which carries the same examples: a project cannot
   declare a plugin from it, and the workspace offers a project only the
   extensions it declares.
3. **Where.** The person's directory if they named one, else `~/uze-try`.
   It must be new or empty: never reuse a directory with something in it.
4. **Make the project.** Run the `demo.py` beside this file with Python 3:
   `python3 <this skill's directory>/demo.py <directory>`. It makes a Git
   repository with twenty weeks of commits, a GSD milestone in `.planning/`
   and an empty `agents.yaml`, touches nothing outside the directory, and
   prints its path.
5. **Install the examples into it**, one command each, from inside the
   directory: `uze install gsd@uze`, then `focus@uze` and
   `contributions@uze`; each is declared in its `agents.yaml`. Leave
   `pingpong` out unless they ask: its server is not in a release, and its
   hooks and MCP server ask the person to trust them.
6. **Hand over.** The workspace is a terminal UI the person runs, never
   you. Tell them, in this order:
   - `cd <directory> && uze workspace`;
   - the workspace offers the project's extensions the first time it opens
     there: review what each one may reach and enable them;
   - where each one shows, from the table above.

Answer with the directory and the command to run, nothing else.
