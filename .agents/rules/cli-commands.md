---
trigger: glob
globs: "**/src/cli/**, **/src/main.rs, **/src/command_performance.rs"
---

- **Every CLI leaf command is classified** in `src/command_performance.rs`
  as `Budgeted` (low-millisecond, cache-backed via
  `UzeApplication::detect_cached`) or `JustifiedSlow` with a stated reason;
  `every_cli_command_is_classified` fails on an unclassified one.
- **Machine and project scope are independent, and always reported**
  (ADR-054). `uze setup`, `doctor`, `config …`, `upgrade` and `market …`
  act on the machine (`~/.uze`). `uze <plugin>@<market>`, `install`
  (`i`), `update`, `remove`, `status` and `inspect` act where they run: in
  a project on the machine plus `agents.yaml`, `agents.lock` and
  `AGENTS.md`, outside one on the machine only; `-m`/`--machine` states
  machine scope on every verb that has two. Neither scope is inferred from
  the other's state.
- **`uze agent …` is an audience, not a category.** Its reader is an agent
  UZE launched, so it is hidden from `uze --help` and documented in the
  launch instructions and the official plugin's Skills. Anything a person
  needs to know belongs in `uze status`, never in a verb of their own.
  `uze agent work name <type>/<subject>` gives work its label and, in a
  checkout of its own, its branch (never the operator's branch), judged
  against `workspace.branch`; unnamed work is named once from its first
  commit's subject. `uze agent work split|join|list` gives subagents
  checkouts UZE records as the agent's children.
- Help text and reports follow the `design-system` skill.
