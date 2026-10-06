## Context

See proposal.md for why. This builds on `project-rules`, still in flight:
- the canonical rule format and the validator;
- `.agents/rules/` read from the checkout by Antigravity natively and by
  the generated engine for Claude Code, Codex and OpenCode;
- the guard that keeps a package's `rules/` out of every machine-wide
  delivery.

It also builds on `project-agent-environment`: `agents.yaml` declares, and
`agents.lock` pins a revision and an `integrity` hash over the ingested
bytes.

The constraint that shapes everything is what Antigravity can read. It
reads rules from the workspace and from machine-wide locations only (its
global rules directories, and plugins installed with `agy plugin install`).
The only place a rule can be native on Antigravity *and* scoped to the
projects that declared it is the checkout's own `.agents/rules/`.

## Goals / Non-Goals

**Goals:**
- One delivery path for all four harnesses: plugin rules become checkout
  files, and `project-rules` delivers them unchanged.
- A rule change from an update is reviewed like code.
- Ownership and integrity that a fresh clone can judge.

**Non-Goals:**
- Disabling or overriding one rule of a plugin per project. A local edit is
  drift today. A declared exclusion in `agents.yaml` is a follow-up if
  asked for.
- Rules in machine-scope (`uze plugin install`) plugins.
- Merging a plugin's rule with a project rule. They are separate files,
  both delivered.

## Decisions

### D1: Copy into the checkout instead of delivering from the Store
Plugin rules are copied into `.agents/rules/<namespace>/` and committed.

Alternatives considered:
- **Deliver from the Store at run time.** The engine would read the
  project's lock to find the plugin's rules in the Store. Antigravity
  would get nothing natively, because it reads no Store and its
  machine-wide plugin leaks. The engine would need to parse the lock and
  locate the Store without `uze`. A worktree, a CI checkout or a clone
  without UZE would have no rules.
- **Copy, but ignore the copies in git.** Every clone and every worktree
  needs `uze install` before rules apply, and a rule change arrives in
  nobody's review.

The decision was made with the operator on 2026-10-06: the copies are
committed, as `agents.lock` is.

### D2: Ownership comes from the namespace and the lock, not from receipts
Receipts live in `$UZE_HOME`, but the copies travel through git to
machines that never wrote them. So a copy's owner is the plugin its
namespace names, and its expected bytes are the plugin's rule at the locked
revision. The locked revision's `integrity` already authenticates those
bytes. On a machine without the plugin in its Store, status acquires it the
way `uze install` does: integrity-checked, read-only. A receipt is still
written where UZE performs the copy, for the inspect-before-detach path,
but the judgement never depends on one.

### D3: The namespace's form waits for one measurement
The preferred form is a directory per plugin: `.agents/rules/<plugin>@<market>/`.
It is readable, removable in one operation, and collision-free across
marketplaces. That needs Antigravity to read rules in subdirectories of
`.agents/rules/`, which is unmeasured (task 1.1). If it does not, the form
falls back to a flat prefix: `.agents/rules/<plugin>@<market>--<rule>.md`.

Either way, the `project-rules` engine reads the same set Antigravity
reads. Its "directly under `.agents/rules/`" requirement widens to include
the namespace form, and only that form, so a project's own subdirectory
never becomes a rule by accident on one harness only.

### D4: No marker inside the copied bytes
The copy is byte-identical to the package's rule. A `source:` field would
make the copy differ from the authenticated bytes and would need
Antigravity to tolerate an unknown field (unmeasured). The namespace already
names the source.

### D5: Drift blocks, absence restores
This follows the receipt discipline UZE applies everywhere (drift blocks
destructive mutation, inspect before detach):
- a drifted copy is never overwritten or removed;
- a missing copy is restored, since restoring destroys nothing;
- an extra file in the namespace is reported and left alone.

The person resolves drift by reverting the file or by taking ownership of
the change, moving it out of the namespace into the project's own rules.

## Candidate ADRs

- **Plugin rules are copied into the checkout and committed.** This is the
  first capability that writes package bytes into a project as committed
  files, which changes what `uze install` may write and what a reviewer
  sees, and is hard to reverse once projects commit them.

## Risks / Trade-offs

- **Committed copies can be edited by anyone.** → That is drift, reported
  by status and by every install or update. It can also be checked in CI
  with `uze status`.
- **A large rules plugin adds files to every consuming repository.** →
  Rules are small Markdown. The namespace keeps them out of the way, and a
  plugin with no rules writes nothing.
- **Status on a fresh clone may need to acquire the plugin to judge the
  copies.** → Acquisition is cached and integrity-checked. Without network,
  status reports the copies as unverified rather than failing.
- **Antigravity may not read subdirectories.** → D3's flat fallback, decided
  by task 1.1 before any code.
