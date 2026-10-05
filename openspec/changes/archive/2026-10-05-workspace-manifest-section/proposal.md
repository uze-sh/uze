## Why

`agents.yaml` reads as one tool when it serves two. Its root carries
`worktrees:` and `artifacts:` beside `marketplaces:`, so a project that
only installs plugins gets a file half full of workspace vocabulary — the
scaffold writes ten commented `worktrees` keys and an `artifacts` block
into every first install. And the workspace's own keys name mechanisms
rather than choices: `worktrees.default: in-place | isolated` pairs an
adverb with an adjective, puts the switch that decides whether a worktree
exists inside the block that describes worktrees, and does not match the
action the workspace offers (now "To worktree"). The two modules were
split in the code (`module-boundary`); the file they share should say so.

## What Changes

- **BREAKING** The root keys `worktrees:` and `artifacts:` move into one
  flat `workspace:` section. `marketplaces:` stays at the root: it is the
  package manager's, and a project that only uses plugins writes nothing
  else.
- **BREAKING** `worktrees.default: in-place | isolated` becomes
  `workspace.worktree: manual | always`. The question it answers is *when
  a worktree is made*: `always` at every launch, `manual` only when the
  operator moves an agent with **To worktree**. Nothing names the primary
  checkout as a value.
- **BREAKING** `worktrees.completion` becomes `workspace.delivery`, same
  values (`handoff | merge | pr`).
- The rest move unchanged and flat: `target`, `branch`, `link`, `setup`,
  `gate`, `slots`, `spare`, `idle_days`.
- **BREAKING** `artifacts: { path: <dir> }` becomes
  `workspace.artifacts: <dir> | [<dir>, ...]`. It names *places* only,
  never kinds: each file says what it is by its own content, so a kind
  the workspace learns later needs no new key.
- The scaffold stops writing the commented workspace block. A
  package-manager-only project's `agents.yaml` holds `marketplaces:` and
  nothing else; the keys are documented in the project-files reference.
- No compatibility for the old shape: there are no users yet, so a file
  still carrying root `worktrees:` or `artifacts:` meets the ordinary
  unknown-key error and is recreated. Nothing reads, migrates or names it.
- The isolation actions' labels are "To worktree" / "To worktree, no
  changes" (already renamed in the keymap); the specs that name them are
  brought in step.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `module-boundary`: the created `agents.yaml` carries no workspace text
  at all, not even commented; choosing a policy writes a `workspace:`
  section.
- `agent-isolation`: the project-level default is `workspace.worktree:
  always | manual`; the isolation action is named "To worktree".
- `worktree-policy`: delivery reads `workspace.delivery`; the popup
  writes it there.
- `agent-work-naming`: the branch vocabulary is `workspace.branch`.
- `architect-surface`: artifacts are declared as `workspace.artifacts`,
  one directory or a list.

## Impact

- `crates/uze-core/src/project/manifest.rs`: `ProjectManifest` root keys,
  `Section`, the scaffold.
- `crates/uze-workspace/src/declaration.rs` and `worktree.rs`:
  `WorktreePolicy` field names, `AgentPlacementDefault` values,
  `DeclaredArtifacts` accepting one or many, every malformed-manifest
  message that names a key (`worktrees.slots` and the like).
- The AGENTS.md region's text and the CLI refusals that name
  `worktrees.branch` (`src/cli/agent.rs`).
- The architect surface's catalog (several roots) and `uze agent
  artifacts check`.
- This repository's own `agents.yaml`, test fixtures, journeys,
  `web/content/docs/reference/project-files.mdx`.
- Overlaps the open `project-agent-environment` change, which defines
  the current shape; that change's ADR-017 text on the file layout is
  amended in place rather than superseded.
