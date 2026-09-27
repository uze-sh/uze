# Scope is reported, not positional

Status: Accepted
Supersedes in part: [019](019-explicit-project-machine-boundary-in-cli-command-grammar.md) (§1–§3)

## Context

ADR-019 made scope structural: root verbs were the project, and the machine
lived under `market`, `plugin` and `harness`. It was written against a real
defect, `uze remove flow` falling back from project to machine depending on
whether a lock happened to mention `flow`, so the command's target depended
on state nobody could see.

It left two spellings for one operation. `uze plugin install git@ai` and
`uze git@ai` did the same thing except for whether the project's files
recorded it, and the directory the command ran in almost always already said
which one was meant. Underneath, project root resolution had no "none":
any directory became a project, so running from `$HOME` would have written
`~/agents.yaml`.

## Decision

**A command's scope is where it runs, or `-m` / `--machine`, and the command
always says which scopes it touched.** The `plugin` namespace is gone and its
operations are root verbs. In a project they act on the machine and maintain
the project's files; outside one they act on the machine and say nothing was
declared. `-m` is the same word on every verb with two scopes, because a
script cannot state intent by choosing a directory. What ADR-019 actually
guarded, that scope is never silent and never inferred from ambient state,
is kept by reporting it instead of by the command's position.

A project is the nearest ancestor with `agents.yaml`, else the repository
root, else the nearest ancestor with `AGENTS.md`, else **none**.

Rejected: keeping the namespace (two spellings for one operation, forever);
a `--no-save` on install alone (one verb's special case instead of one word
on all of them); inferring who else declares a package before a machine
removal, because nothing on the machine knows every project that declares
one and a rule built on what it does know would delete bytes another project
needs.

## Consequences

One spelling per operation, and a scope that is either obvious from where
you stand or stated with one flag. `uze remove <p>` still only undeclares;
taking bytes off the machine is `-m`, asked for by name.

The reported signal is weaker than a positional one: a person who does not
read the output can be surprised. The surprise is bounded, since the machine
half always happens and the only variable is whether a project file moved,
which `git status` shows. Running `uze <p>@<m>` in a cloned repository
creates `agents.yaml` there, visible and untracked.

`inspect` and `update` return to the root, reversing that part of ADR-019;
its machine/project boundary itself stands.

Source change: openspec/changes/archive/2026-09-27-flatten-the-command-grammar/
