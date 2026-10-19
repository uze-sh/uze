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

Neither Claude Code's nor Codex's plugin format can carry a rule. Claude
Code's is answered by its plugins reference (task 1.3). Codex's was measured on
0.163.0 (`experiments/codex/plugin-rules`, plain `codex`, no UZE): a plugin
installed with `codex plugin add` whose `.codex-plugin/plugin.json`
declares `skills` and an undeclared `"rules": "./rules/"` reached the model
with its Skill, the control, and with none of `rules/always.md`
(Antigravity's format), `rules/AGENTS.md` or a root `AGENTS.md`. Codex's own
`rules` are execpolicy `.rules` files, which the binary loads from the user
and project configuration: the plugin's `rules/lab.rules` forbidding a command
left it running, while the same rule in `~/.codex/rules/` refused it. Codex
stages the whole plugin tree in its cache, `rules/` included, but reads
nothing from it, so a package's `rules/` is inert there rather than a leak.

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
Plugin rules are copied into `.agents/rules/<plugin>@<market>/`, the folder registered in `.agents/rules.json` (D3), and committed.

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

It was confirmed on 2026-10-19 against the rule in
`keep-agents-md-the-operators` D1. UZE writes into a project only the
declaration and agnostic artifacts. A copy here qualifies: Antigravity's
documented rule format, a namespace UZE owns, and readable by a harness without UZE.
What the rule forbids is writing into a file the operator authors, which a
copy never does.

### D2: Ownership comes from the namespace and the lock, not from receipts
Receipts live in `$UZE_HOME`, but the copies travel through git to
machines that never wrote them. So a copy's owner is the plugin its
namespace names, and its expected bytes are the plugin's rule at the locked
revision. The locked revision's `integrity` already authenticates those
bytes. On a machine without the plugin in its Store, status acquires it the
way `uze install` does: integrity-checked, read-only. A receipt is still
written where UZE performs the copy, for the inspect-before-detach path,
but the judgement never depends on one.

### D3: The namespace is a folder, registered in `.agents/rules.json`
A plugin's rules are copied as `.agents/rules/<plugin>@<market>/<rule>.md`,
and the folder is registered in `.agents/rules.json` with one entry
`{"path": ".agents/rules/<plugin>@<market>"}`. Decided with the operator on
2026-10-19, after the measurement below: a folder per plugin is the layout a
person expects, removable in one operation and owned by its name.

Antigravity reads no subdirectory of `.agents/rules/` on its own (measured,
`experiments/antigravity/rules`, tasks 1.1 and 1.2); the manifest entry is
what makes it read the folder. The first measurement chose a flat prefix,
`<plugin>@<market>--<rule>.md`, for that reason; the prefix never shipped.

The manifest is a file the operator may also write, so UZE owns only its
own entries (D7). The `project-rules` engine and `uze-core` read the same
manifest, so the four harnesses receive the same set.

Measured: the `.agents/rules.json` manifest. Antigravity's docs say a
nested rule is read once registered in `.agents/rules.json`. On `agy` 1.4.2
(`experiments/antigravity/rules-manifest`, plain `agy`, no UZE), an entry
`{"path": ".agents/rules/acme@corp"}` made the folder's `always_on` rule
land in `systemInstruction` and its two `glob` rules fire on the first
matching read and on a write, the same as flat files; the absolute path
did too. `path` is relative to the workspace root, not to `rules.json`:
`rules/acme@corp` and `./rules/acme@corp` registered nothing, and an
operator's `team-rules` folder at the root was read beside two plugin
folders and a flat rule. A directory entry reads its immediate `*.md` only
(`sub/inner.md` stayed out); an entry may also name one file.
`include_only` and `exclude` match a path relative to the entry: `a.md`
selected or dropped `a.md` and not `sub/a.md`, `include_only: ["sub/c.md"]`
reached a file the entry alone does not, and `exclude: ["sub/"]` changed
nothing. An entry for `.agents/rules`, or for a flat rule itself, still
delivered that rule once. The registered rule held in the parent's request,
an `invoke_subagent` subagent's and the request after `agy -c`. A
`rules.json` that does not parse is ignored: the flat rules still applied
and nothing it registered did. An `@` in the folder name costs nothing
(`acme-corp/` behaved the same).

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

### D6: `always_on` replaces the package instructions
A plugin's `AGENTS.md` resource used to become a region of the project's
`AGENTS.md`, which the rule D1 cites forbids. An `always_on` rule says the same
thing to every agent of a declaring project, through the same route as the
plugin's other rules. Nothing about it is a second mechanism. The retirement
itself (flag, resource, region cleanup) belongs to
`keep-agents-md-the-operators`, ordered after this change.

### D7: UZE owns its entries of `.agents/rules.json`, never the file
- UZE adds, keeps and removes only entries whose `path` is a plugin folder
  it declares, `.agents/rules/<plugin>@<market>`. Every other entry, and
  every other key, is the operator's and is written back unchanged.
- A manifest that does not parse is never written: install, update and
  remove report it and copy nothing that needs it. A plugin folder already
  registered by the operator is left as they wrote it.
- A missing manifest is created holding UZE's entries alone; a manifest left
  with no entry and no other key once the last plugin goes is removed.
- `inherits` is not followed by UZE's readers, which is reported when a
  manifest uses it: Antigravity reads further manifests UZE does not.

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
- **Antigravity reads no subdirectory of `.agents/rules/`** (measured on
  1.4.2). → D3 registers the folder in `.agents/rules.json`, which it does
  read (measured, `rules-manifest`). A later Antigravity that starts
  reading subdirectories on its own reads each copy once.
