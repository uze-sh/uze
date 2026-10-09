# A project's commands run only once the operator approved them

Status: Accepted

## Context

`agents.yaml` declares two lists of shell lines the workspace runs on the
operator's machine: `workspace.setup`, in every checkout it prepares, and
`workspace.gate`, before it delivers work. They run with the operator's own
permissions, outside any harness's sandbox, and before any harness has asked
whether the folder is trusted. The file arrives with the repository, so
cloning a project and opening the workspace on it ran a stranger's command
with nobody having read it. An agent working in the operator's checkout
could do the same by editing the file. `workspace.link` is not among them: it
links ignored files and runs nothing.

Package trust (an MCP `command` from a remote marketplace) already crosses a
consent boundary. A project's own commands crossed none.

## Decision

**A project's commands run only once a person has read the exact lines and
said yes**, the way `direnv allow` treats an `.envrc`.

- The approval is a record under the project's own directory in `state/`
  (`approved-commands.json`), naming the canonical project root and a SHA-256
  of the lines this platform's shell would run, each framed by its step and
  length. Any edit, addition, removal or move between `setup` and `gate`
  changes it, and the commands wait again. A line with no spelling for this
  platform is not part of it: it never runs here.
- Unapproved, a checkout is still placed, without its `setup`, and says why;
  delivery is refused before anything moves. An unreadable record, one a
  newer build wrote, or one naming another root approves nothing.
- The code that runs the commands (`checkout::materialize`,
  `landing::deliver`) takes an `Approved` value only the consent check can
  make, so a caller that forgot to ask has nothing to hand it.
- A person answers in two places: the workspace's dialog, opened from a
  toast when it first sees the project or when a placement or delivery went
  without the commands, and `uze workspace allow`. Both show every line with
  its controls written out. `uze workspace revoke` withdraws it. The verb is
  not under `uze agent`, refuses inside an agent UZE launched, and needs a
  terminal: no flag stands in for the answer.
- What is approved is what was shown, never what the file says when the
  answer arrives.

Rejected: trusting a project because the operator opened it (opening is not
reading); a `--yes` flag (an agent would pass it); keying on the file's bytes
(a comment edit would ask again, while what runs did not change); approving
per command rather than per list (the list is what runs, in order).

## Consequences

A freshly cloned project asks once before its first setup or delivery, and
again whenever its commands change, on every machine it is opened on. That
is the cost and the point. A project with no `setup` and no `gate` asks
nothing.

It is a consent boundary, not a sandbox: an approved line runs with the
operator's permissions, and anything that can write the operator's `state/`
can forge an approval. It stops an unread command from running, and an agent
from approving through the surfaces UZE offers it.
