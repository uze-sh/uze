## Context

See proposal.md, Why. The constraints the approach is shaped by:

- **An extension never draws, and reaches nothing it was not handed**
  (ADR-041). Everything here goes through `Host`: `list_dir`, `read_file`
  and `git`. Those three are enough, so `Host` does not grow.
- **The `view` contract grows only when a second extension needs it.**
  The spec surface is a list with a document beside it, which is
  `Layout::Sidebar`, `Navigator` groups, `View::subjects` and `View::modes`
  as they stand. Nothing is added to `view.rs`.
- **Nothing the client draws waits on a repository.** Walking the dialect,
  reading every artifact and the Git reads that find the checkout's own
  change run on a thread, as a `spawn_*`/`absorb_*` pair.
- **Presentation never names the domain.** The architect surface needed a
  read model in `uze-application` because it reads `agents.yaml`. This
  surface reads only the checkout, so it needs none. The one fact the
  host adds, the branch the slot delivers to, is one the orchestrator
  already reads (`delivery_policy(..).target`, for the work badge).
- **Markdown is already rendered** by `code/markdown.rs`
  (`pulldown-cmark`, already a dependency).

## Goals / Non-Goals

**Goals:**

- A spec surface for OpenSpec that is useful on this repository on day one.
- A catalog shape that a second tool fits as data, proven by a test and
  not only by a table in this document.
- The link between lenses: whoever opens the spec surface in an agent's
  checkout lands on the change that agent is working on.

**Non-Goals:**

- Shipping Spec Kit, Kiro or Superpowers. They are shaped for (see
  *Decisions*) and not shipped.
- Writing. No checkbox is ticked from this surface, no change is created
  or archived. Editing goes through the code surface, which already has
  the write grant and ADR-048's rule for it.
- Running a tool's CLI (`openspec list --json`, `openspec validate`).
  `Host` grants no process but Git, and reading files works for every
  tool the same way.
- Comparing a spec delta against the living spec it modifies. It is the
  obvious next step and a second rendering problem; the delta is shown
  as written.
- Planning that lives outside the checkout (an OpenSpec *store*). The
  local `openspec/` is what is read.
- Reloading while open. The artifacts are read once per opening, as the
  architect surface does.

## Decisions

### One extension, with the tools in a catalog

Tools differ in where files go and what they are called. They do not
differ in what a reader does with them. One surface per tool would be four
copies of the same navigator, renderer and progress count, and a person
working across projects would learn four surfaces for one question.

So the extension knows **roles** and **subjects**, and a catalog entry maps
a tool's layout onto them. An entry is `const` data:

```rust
struct Dialect {
    name: &'static str,                  // "OpenSpec"
    marker: &'static str,                // "openspec", a directory at the checkout root
    collections: &'static [Collection],
    roles: &'static [(Pattern, Role)],   // first match wins, else Role::Other
}

struct Collection {
    subject: Subject,                    // Changes | Specs | Archive
    path: &'static str,                  // "openspec/changes"
    skip: &'static [&'static str],       // &["archive"]
    order: Order,                        // Ascending | Descending (dated names)
}

enum Role { Why, How, Steps, Contract, Other }
```

Every directory directly inside a collection's path is one unit. A role
pattern is a path relative to the unit where `*` stands for one segment and
`**` for one or more (`specs/**/spec.md`), and what the wildcards stood for
names the file: a contract is called by its capability path. That is all
globbing the current and planned entries need, so there is no glob crate.

*Alternative considered:* one extension per tool. Rejected for the reason
above, and because the code extension already set the precedent: when
the selection has to survive a switch, the switch belongs inside one
extension.

*Alternative considered:* a trait per dialect. It would let a dialect run
arbitrary code, which is what makes the next one expensive to review.
Data keeps a new tool to a table entry and a fixture.

### The catalog shape, checked against the tools not shipped

| | marker | changes | specs | archive | roles |
|---|---|---|---|---|---|
| OpenSpec | `openspec/` | `openspec/changes/*/` | `openspec/specs/**/spec.md` | `openspec/changes/archive/*/` | proposal=why, design=how, tasks=steps, `specs/**/spec.md`=contract |
| Spec Kit | `.specify/` | `specs/*/` | `.specify/memory/constitution.md` | none | spec=why, plan=how, tasks=steps |
| Kiro | `.kiro/specs/` | `.kiro/specs/*/` | none | none | requirements=why, design=how, tasks=steps |
| Superpowers | its docs directory | file units, one per design and one per plan | none | none | by collection: designs=how, plans=steps |

Three of the four fit the shape as written. Superpowers does not quite:
its design and its plan for one piece of work are two files in two
directories, joined only by a shared date-and-topic stem. The shape
would need units that are files, joined by stem. That is the one stretch known
today. It is recorded here and not built, and Superpowers' current paths
are confirmed against the release in use when its entry is written, since
they have moved between versions.

The proof that the shape is not OpenSpec-shaped is a **test-only Kiro
entry** with a fixture tree, read through the same code path as the
shipped one. Kiro was picked because it is the one farthest from OpenSpec
that still fits: no specs subject, no archive, different file names.

A subject none of the detected dialects fill is not offered: Kiro would
show changes alone.

### Detected at the checkout root, never declared

ADR-050 made `artifacts.path` declared because a diagram directory is the
project's choice and `agents.yaml` is where a project states its choices.
Here the location is the **tool's** choice: OpenSpec decides where
`openspec/` lives, and its own config is where a project says anything
more. Declaring it again in `agents.yaml` would be a second place to
disagree, which is what ADR-050 refused.

The read starts at the checkout's repository root (`Host::repository_root`
of the tab's directory), not the project root, because the files travel
with the branch: an agent's worktree has its own `openspec/changes/`, and
that is the one worth reading there. More than one marker found means
every matching dialect is read, and each unit carries its dialect's name,
which the caption shows only when there is more than one.

### Finding the checkout's own change

A unit is this checkout's when a path under it appears in either of:

1. `git status --porcelain` (the working tree against `HEAD`), which is
   the same read the code surface's changes make;
2. `git diff --name-only <merge-base>..HEAD`, where the merge base is
   taken between `HEAD` and the target the host reports.

The host reports the target it already reads for the work badge
(`delivery_policy(..).target`), as a field of the read request. In the
operator's checkout there is none, and only the first read is made:
work on the target branch has no base to be ahead of.

Mapping a path to a unit is prefix matching on the collection paths the
dialect already declares, so the catalog is the only place that says
where units are.

*Alternative considered:* matching the branch name to the change name
(`feat/spec-lens` ↔ `add-the-spec-surface`). Names are chosen by
different people at different moments and rarely agree. What the branch
touched is a fact.

### Rendered by the code surface's Markdown, moved to `shared/`

`code/markdown.rs` renders into `ContentLine`s and knows nothing about
the code surface except the syntax-highlighting module it colours fences
with. It moves to `shared/markdown.rs` with `highlight` alongside it, per
the crate's rule that a module moves to `shared/` the day a second
extension reaches for it. The two modes are *Rendered* and *Source*, as
in the architect surface.

### Opening an artifact means opening it in the code surface

Activating an artifact returns an outcome naming the path, and the host
opens the code surface there, which is the same move
`ArchitectOutcome::OpenPath` makes from a diagram box. The code surface
already has the write grant, the editor and ADR-048's rule about what it
may write. Giving this surface a second editor would be a second place to
keep that rule.

### Progress is counted from the steps artifact alone

`- [ ]` and `- [x]` (and `*` bullets, and `X`) at any indentation, outside
fenced code, are counted. Nothing else is interpreted: no section
structure, no task numbering. OpenSpec's own `tasks.md` and the other
three tools' task lists all use GitHub checkboxes, and a count that tried
to understand more would be the first thing to disagree with the tool.

### Place in the tab strip and on the keyboard

The button leads the tab strip's extension group, in the order a change is
read: spec, arch, code (what it intends, how it was described, what it
is). The architect button's label shortens to `arch` to pay for the third
button's room: at 80 columns the strip gives the tabs' room up first.
`toggle-spec` is bound to `alt+x` in the workspace, code, architect and
spec scopes: a door is reached with the thumb already on Alt, so it sits
in the row beside it. A new `spec` scope seals the keyboard the way
`architect` does and borrows the code surface's keys for the moves the two
share. `alt+c`, `alt+d`, `alt+f` and `alt+b` were passed over on purpose:
a shell in the pane capitalises, deletes and moves by word on them, and
the workspace scope takes a chord before the pane sees it. `alt+r` was the
first choice and did not survive a Windows machine: AMD's overlay claims it
system-wide, as NVIDIA's does `alt+z`, so the terminal never receives it.

The remembered place is by checkout, like `architect_places`, and names
the unit and the artifact (never indexes), so a change that was archived
since is simply not restored.

## Candidate ADRs

- **Spec-driven tools are a catalog of layouts behind one surface**: the
  roles (why, how, steps, contract) are the vocabulary, a tool is data,
  and a dialect is detected by its own marker rather than declared. Hard
  to reverse once there are several entries and people rely on the roles.

## Risks / Trade-offs

- [OpenSpec schemas vary: this repository's `adr-driven` schema and any
  custom one may write files the entry does not name] → unknown files
  are listed as *other*, never dropped, and the scenario is in the spec.
- [A tool changes its layout between releases] → a layout that no
  longer matches reads as "no spec layout found" with the list of known
  layouts, which is a visible and cheap failure; the entry is one table.
- [The merge base read costs a Git call per opening] → it runs on the
  read thread with the rest, and only when the host reports a target.
- [A large archive makes the read slow] → everything is read once per
  opening on the read thread, which this repository (333 documents, about
  2.6 MB on disk) does in well under a second; nothing is fetched while
  somebody moves through the list, and progress is counted only for
  changes in flight. Reading an archived unit only when it is opened is the
  step to take if a repository outgrows that.
- [Two dialects in one checkout (a project migrating tools)] → both are
  read and shown; nothing is merged across them.

## Migration Plan

None. The surface is new, reads only, and persists only a remembered
place in the tier `architect_places` already sits in.
