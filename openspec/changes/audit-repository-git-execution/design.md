## Context

See proposal.md - Why, and findings.md for the attacks. `uze-git` is the one
transport for Git (the `git-transport` rule). Its reads already ran with no
hook, no fsmonitor, no external diff or textconv. Its writes forced only
`core.fsmonitor=false`, on the stated ground that a write is UZE acting for
the operator and their `pre-push` or `post-checkout` is part of the action.
That ground does not hold: the repository's common Git directory, and with
it `hooks/` and `config`, is writable by every agent that can commit.

Investigation while fixing found the same class beyond hooks: a clean filter
runs on a plain `git status` (a read), a smudge filter on `worktree add`, a
merge driver on `rebase`, a `credential.helper` or `core.askPass` on any
fetch that asks for a credential, and `remote.<name>.uploadpack` on a fetch
from a local remote. All are named by the repository's own configuration.

## Goals / Non-Goals

**Goals:** one hardening set in `uze-git`, applied to every invocation,
read or write, so no caller can forget it; the operator's own configuration
keeps working; ids read back from Git or a harness never reach argv as an
option.

**Non-Goals:** what an agent's harness runs in its own checkout (its
commits and hooks are the harness's, inside its sandbox); hardening the
remote end of a push (a server's hooks are the server's); the acquisition
clone of untrusted marketplaces (`acquisition::git`, already stripped).

## Decisions

### D1. Hooks never run in UZE's own Git operations

`core.hooksPath` is the platform's null device on every invocation. The
brief offered two options: fold the hooks into ADR-055's approval digest, or
do not run them. Folding was rejected: a hook is a file in a directory any
agent writes, so an approval would have to snapshot its bytes into a
directory UZE owns and point Git there, and a hook digest would be asked for
again every time Git LFS or a hook manager rewrote one. The approved place
for "run this before delivering" already exists: `workspace.gate`. This
amends no ADR's decision; it reverses the reasoning in `uze-git`'s
`EVERY_INVOCATION` comment, which is rewritten.

### D2. A fixed set for every invocation, a scoped override where a program can run

Static settings on every invocation: `core.fsmonitor=false`,
`core.hooksPath=<null>`, `protocol.ext.allow=never` (a per-protocol setting
beats the repository's `protocol.allow`), `submodule.recurse=false`,
`fetch.recurseSubmodules=false`, `push.recurseSubmodules=no` (a submodule's
own configuration is out of reach of the override below), and
`log.showSignature=false` (a log showing signatures runs the signing
program).

Keys that name a program cannot be switched off as a class: a filter or a
merge driver is named by an attribute and defined under a name the
repository picks. So for an operation that can reach one, `uze-git` first
asks Git which of those keys are set and in which scope
(`git config -z --show-scope --get-regexp`, which follows includes and
conditional includes and reports the including file's scope). Keys set in
the `local` or `worktree` scope are overridden on the command line, where a
`-c` wins: with the operator's value for the same key when their system,
global or environment configuration has one, otherwise with an inert value
(an empty filter with `required=false`; a merge driver of `false`, which
fails the merge closed into a conflict rather than silently keeping one
side; `ssh` for `core.sshCommand`; an empty askpass; an empty alternate
refs command; `gpg`/`gpgsm`/`ssh-keygen` for the signing programs).
Credential helpers are reset with an empty `credential.helper` and the
operator's helpers re-added in order. Keys with a fixed name (SSH, askpass,
alternate refs, credential helpers, upload/receive pack, signing programs)
are given on every command that can reach them whether or not the probe saw
the repository set them; the filter and merge-driver names the repository
picks are answered from the probe. Two keys are first-wins in Git, so a
`-c` cannot beat them: `remote.<name>.uploadpack`/`receivepack` are
answered with `--upload-pack`/`--receive-pack` after the subcommand, and
`core.gitProxy` by turning the `git://` transport off for UZE's own
transport commands.

Rejected: dropping the repository's configuration altogether (Git needs its
`core.*`, remotes and extensions); parsing the configuration files in UZE (a
second parser that disagrees with Git's is the bypass); a cache keyed on
file times (conditional includes and `onbranch:` make the key incomplete).

### D3. Only the operator's scopes name programs, including SSH

A repository-local `core.sshCommand` (a per-repository deploy key) no longer
takes effect inside UZE's own operations: the local scope is the agent's to
write. The operator's place for it is global configuration, scoped to the
repository with `[includeIf "gitdir:…"]`, which Git reports as global scope.

### D4. The probe runs only where a program can be reached

The scoped probe costs one Git process. It runs for every write and every
network subcommand, and for reads only of the subcommands that hash work
tree content (`status`, `diff`, `diff-files`, `diff-index`, `ls-files`,
`hash-object`, `blame`, or any read given `--filters` or `--textconv`). Pure plumbing reads (`rev-parse`,
`for-each-ref`, `cat-file`, `log`, `show`, …) and the plumbing writes known
to run no program (`update-ref`, `symbolic-ref`, `config`, `branch`,
`worktree prune|lock|unlock|list|repair`, `init`) skip it. A probe
that fails refuses the operation: an unknown configuration is not a safe one.
A key the probe reports whose name holds `=` or a newline cannot be written
as a `-c` without changing its meaning and refuses the operation too.

### D5. Session ids are a validated type, tightened per harness

`SessionId::new` is replaced by `SessionId::parse`, which accepts 1–128
ASCII letters, digits, `-` and `_`, starting with a letter or digit; it is
also the serde path, so a stored record cannot smuggle one either. Each
integration requires its harness's shape on the ids it reads back: a UUID
for Claude Code, Codex and Antigravity (observed in their stores), `ses_`
followed by letters and digits for OpenCode. No end-of-options marker is
added before the id: Codex's `resume <id>` is followed by the operator's
overrides, which a `--` would turn into positionals; Antigravity's Go flags
consume the next word as the value; and the journeys and the Lab read the
two-word `--resume <id>` form. The type is what makes the marker
unnecessary.

### D6. WKS-3: `--` before the URL

`ls-remote --heads -- <url> <patterns>`. A sweep of `uze-workspace` and
`uze-git` for other values read from Git and re-passed found the rest
already behind `--` or `--end-of-options`, or validated (branch names,
object ids).

## Risks / Trade-offs

- [Git LFS users lose LFS's `pre-push` upload inside UZE's delivery push]
  → their objects are pushed by `git lfs push` or by the LFS hook of a push
  they run themselves; the smudge filter, configured globally by `git lfs
  install`, still runs. Recorded as a residual.
- [A repository-local filter the operator configured on purpose is
  skipped] → configure it globally.
- [One extra Git process per write, network call and content-hashing read]
  → bounded by D4; the process budgets are re-measured.
- [Findings on this surface left unfixed by this change] → tracked
  privately in GHSA-97f7-95q3-wcjw until fixed.

## Migration Plan

None: no persisted shape changes. A conversation record holding an id the
new type refuses reads as no record, and the agent starts a new
conversation.
