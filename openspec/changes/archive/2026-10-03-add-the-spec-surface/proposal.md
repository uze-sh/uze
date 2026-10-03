## Why

The workspace has two lenses on a checkout: `code` shows what *is* (measured
from Git and the files), `architect` shows what somebody *described* (the
diagrams a project keeps). The third thing a reviewer of agent work reads is
what was *intended*: the proposal, design, tasks and spec deltas an agent
worked from. Projects that practise spec-driven development already write
these as files, and today reading them means leaving the terminal or
opening each Markdown file by hand in the code surface, with no sense of
which change is in flight, how far its tasks got, or which change the
checkout in front of you is working on.

The tools that produce these files differ in layout, not in concept.
OpenSpec, Spec Kit, Kiro and Superpowers all converge on a unit of intent
(a change, a feature, a plan) holding a *why*, a *how*, a list of *steps*
and, in some, a *contract* that outlives the change. So this is one surface
with a catalog of layouts, not one extension per tool. OpenSpec ships
first because this repository is written in it and the surface can be
proven against its own history.

## What Changes

- **A new built-in extension, `spec`**, the third lens beside `architect`
  and `code`: a project's spec-driven-development artifacts, listed by
  unit and rendered as documents. It opens from the workspace, the code
  surface and the architect surface, by a key and by a chip in the tab
  strip's button group, the way the other two do.
- **A dialect catalog.** What a tool's layout means is data: where its
  root is, what a unit is, and which role each file plays (why, how,
  steps, contract, other). The surface knows roles, never tools. OpenSpec
  shipped first; Spec Kit (#142), Superpowers and GSD (#175) followed as
  catalog entries on the same surface, which is the proof that adding a
  tool is a catalog entry and not a new surface. This change's spec
  describes the catalog as it now stands.
- **Detected, not declared.** A dialect is found by the marker its own
  tool defines (`openspec/`, `.specify/`, `docs/superpowers/` or
  `.planning/` at the checkout root). Nothing is added to
  `agents.yaml`: the tool already decides where its files live, and a
  second declaration would be a second place to disagree (the argument of
  ADR-050, applied the other way round, because here the location is the
  tool's and not the project's).
- **Read from the checkout, not the project.** Specs travel with the
  branch, so an agent's worktree shows the changes as that branch has
  them.
- **The change this checkout is working on is marked.** A change whose
  files this checkout has touched, uncommitted or committed since its
  base, is marked and opened first. This is the link between the lenses:
  intent (spec), what it did (code), where it lands (architect).
- **Steps carry progress.** A unit's checkbox tasks are counted and shown
  beside it (`7/12`), complete or not; for GSD a plan counts as done once
  its summary is written.
- **Three subjects**: in-flight changes, the living specs, and the
  archive. Artifacts render as Markdown by default, with the source one
  mode away. Activating an artifact opens it in the code surface, where it
  can be edited: the spec surface itself writes nothing.
- **A `tasks` section in the workspace sidebar** totals the steps of the
  changes the checkout in front is working on, and opens the surface on
  one of them.
- **Markdown rendering moves to `shared/`**, because a second extension now
  reaches for it.
- **`Symbol::Spec`**, a `spec` key scope and a `toggle-spec` action with
  its default binding.

No breaking change. No new external dependency. The `view` contract is
used as it stands (`Layout::Sidebar`, subjects, modes, navigator groups);
nothing is added to it.

## Capabilities

### New Capabilities

- `spec-surface`: the surface that lists and renders a checkout's
  spec-driven-development artifacts, how a dialect is detected and what
  its catalog entry must say, how units, roles and task progress are read,
  how the checkout's own change is found, and the rule that none of it
  waits on the filesystem or on Git.

### Modified Capabilities

None. The architect surface's reachability requirement names the changes
and files entry points it opens directly; the spec surface adds its own
entry points without changing that requirement.

## Impact

- `crates/uze-extensions`: `spec.rs` and `spec/` (`dialect`, `catalog`,
  `progress`, `ownership`, `summary`); `code/markdown.rs` and
  `code/highlight.rs` move to `shared/`;
  `ExtensionHit::Spec`; one `ExtensionRegistry::builtin` entry.
- `crates/uze-keys`: `Scope::Spec`, and `toggle-spec` bound to `alt+x` in
  the surfaces scope.
- `crates/uze-theme`: `Symbol::Spec` in the three shipped glyph sets,
  regenerated schema.
- `src/ui`: the orchestrator gains `spawn_spec_read` / `absorb_spec` and
  `spawn_spec_summary` / `absorb_spec_summary` (`orchestrator/reads.rs`,
  `orchestrator/answers.rs`), the button in the tab strip's group, the
  sidebar section, and a remembered place per checkout;
  `extension_view.rs` needs nothing new.
- `tests/architecture`: affordances for the new action.
- `docs/architecture/containers.mmd`: the TUI container's description names
  the three lenses. No diagram changes structure: none draws extensions
  one by one.
- No change to `uze-core`, `uze-application` or `agents.yaml`.

## Not in this change

- A Kiro catalog entry, test-only or shipped: the second-dialect proof the
  design planned with it was made by shipping Spec Kit instead. A follow-up
  if Kiro is wanted.
