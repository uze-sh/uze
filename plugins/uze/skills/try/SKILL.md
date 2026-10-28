---
name: try
description: Sets up a project to try uze's example plugins in — milestone, radar, focus, pulse, worklog — so the person sees what extensions do in the workspace without wiring anything of their own. Use when the person asks to try uze, to see example extensions or plugins, to set up a demo or playground project, or invokes /uze:try or $uze:try directly.
invoke:
  model: true
  user: true
---

# UZE — trying the example plugins

uze's own marketplace carries, besides the `uze` plugin, a handful of
examples (category `example`), built into uze as the `uze-official`
marketplace. Each one needs a project with something in it to show:
`pulse` draws a repository's commit history, `milestone` a plan in
`.planning/`. This Skill makes that project and installs the examples into
it. The person then opens the workspace.

| Example | Where it shows | What it does |
|---|---|---|
| `milestone` | bar, drawer | a milestone in phases; plans or executes the next one through the agent |
| `radar` | sidebar, `alt+e` | who is working and for how long; a sound or a notification when a turn ends |
| `focus` | bar, drawer | a pomodoro, with the last seven days |
| `pulse` | bar, view | a heatmap of twelve weeks of commits; click a day for its commits |
| `worklog` | bar, drawer | hours logged per day, read from an MCP server the agents can call too |
| `pingpong` | the harness | one of every capability a plugin delivers; its MCP server is built from a uze checkout |

## Steps

1. **uze is there.** `uze --version`. Without it, stop and point the person
   to <https://uze.sh>: installing uze is theirs to do.
2. **The examples are there.** They come built in, as `uze-official`. A
   uze older than the examples answers `milestone@uze-official` with an
   unknown package: then run `uze market add uze-sh/uze` and name them
   `@uze` instead in step 5.
3. **Where.** The person's directory if they named one, else `~/uze-try`.
   It must be new or empty: never reuse a directory with something in it.
4. **Make the project.** Run the `demo.py` beside this file with Python 3:
   `python3 <this skill's directory>/demo.py <directory>`. It makes a Git
   repository with twelve weeks of commits and a milestone in
   `.planning/`, touches nothing outside the directory, and prints its
   path.
5. **Install the examples into it**, one command each, from inside the
   directory: `uze install milestone@uze-official`, then `radar`, `focus`,
   `pulse` and `worklog`, each `@uze-official`. `worklog`'s MCP server runs
   `python3`; if an install refuses an executable capability, say which and
   ask the person before adding `--trust` to that one command. Leave
   `pingpong` out unless they ask: its server is not in a release.
6. **Hand over.** The workspace is a terminal UI the person runs, never
   you. Tell them, in this order:
   - `cd <directory> && uze workspace`;
   - the workspace offers the project's extensions the first time it opens
     there: review what each one may reach and enable them;
   - where each one shows, from the table above.

Answer with the directory and the command to run, nothing else.
