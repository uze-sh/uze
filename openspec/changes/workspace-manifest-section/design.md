## Context

`ProjectManifest` (`uze-core::project::manifest`) owns the file and
`deny_unknown_fields` at its root. Its root keys are `worktrees`,
`marketplaces` and `artifacts`, and the first and last are carried as raw
YAML values that only `uze-workspace::declaration` parses. That shape is
what keeps `uze-core` free of the workspace (`module-boundary`), and the
new one must keep it. The scaffold (`manifest::SCAFFOLD`) documents every
workspace key as a comment. Edits go through `manifest::edit`, the
lossless round-trip that keeps a person's comments and order.

## Goals / Non-Goals

**Goals:**
- One `workspace:` section, flat, owned and parsed by `uze-workspace`
  alone. `uze-core` still sees one opaque value.
- Values that answer the question their key asks (`worktree: always |
  manual`, `delivery: handoff | merge | pr`).

**Non-Goals:**
- Reading, migrating or naming the old shape. There are no users yet, so
  this is the schema from here on: a file in the old shape meets the
  ordinary unknown-field error and is recreated by hand or by
  `uze install`. The file is authored, so it has no ladder either (see the persisted-tier rule in AGENTS.md).
- New artifact kinds. Places become a list here; what is found in them
  is the business of each surface (see the `spec-decisions` change).
- Renaming the internal vocabulary the code already uses well
  ("primary checkout", `Placement`): only names a person reads change.

## Decisions

**One opaque `workspace` value in `uze-core`, not two.** `ProjectManifest`
trades `worktrees` and `artifacts` for one `workspace: Option<Value>`, and
`Section` gets one variant for it. `uze-workspace` deserializes it into a
`WorkspaceDeclaration` with `deny_unknown_fields` and splits it into the
`WorktreePolicy` and the artifact roots it already returns. *Alternative:*
let `uze-core` type the section. Rejected: that is the dependency
`module-boundary` exists to forbid.

**`always | manual`, not a name for the primary checkout.** The key is
the mechanism the neighbouring keys already describe (`link`, `setup`,
`slots` are all about worktrees), so the value says *when* one is made.
`manual` is true even when nothing is declared, because "To worktree" is
always on offer; a `none` or `in-place` value would have to name the
primary checkout and would be slightly false. The serialized enum
becomes `Always | Manual`; `AgentPlacementDefault` keeps its name.

**`completion` becomes `delivery`.** The popup, the AGENTS.md region and
the spec already call the act *delivery*; only the key disagreed. The
enum `CompletionBehavior` and its `abi_name` keep their values, so the
`uze agent` ABI is unchanged.

**`artifacts` takes one directory or a list.** An untagged
string-or-sequence deserializes into `Vec<PathBuf>`. Each entry is
validated on its own, and the first that leaves the project refuses the
declaration by name: the file has to be fixed either way, and a surface
drawing half of what was declared would need a place to say the other
half is missing. The architect catalog walks every root; with more than
one, an artifact's origin carries its root, so two `overview.mmd` stay
two.

**The scaffold carries only the package manager.** It keeps its header
and the commented `marketplaces:` example. The workspace keys are
documented in `project-files.mdx`, and the workspace writes the section
when a choice is made, as it does today for `completion`.

## Candidate ADRs

- `agents.yaml` keeps the package manager at its root and the workspace
  under one section: decides the file's layout for every future key.
  Amends ADR-017 in place, since that change is still open.

## Risks / Trade-offs

- [Every existing `agents.yaml` with `worktrees:` stops reading] → there
  are no users yet; this repository's own file, the fixtures and the
  journeys move in the same change, and anyone else deletes and recreates
  the file. No compatibility code is carried for it, which is also what
  keeps reading the manifest as cheap as it was.
- [An AGENTS.md region already projected names the old keys] → the
  region's text is regenerated on the next reconcile; nothing in it is
  parsed back.
- [A workspace that writes `workspace.delivery` into a file still in the
  old shape] → it reads first and fails, so it never writes into a file it
  could not read.
