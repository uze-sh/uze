---
name: architect
description: Writing and changing the Mermaid diagrams a project keeps under `workspace.artifacts` — choosing which diagram and which level a change belongs in, naming it so it reads where a reader expects, and proving it still draws with `uze agent artifacts check`, which needs no workspace. Use when adding or editing a `.mmd`/`.mermaid` file, when asked to diagram or document an architecture, when a diagram does not draw, or when setting up where a project keeps its architecture.
---

# UZE — a project's architecture, as files somebody wrote

A project tells UZE where its architecture lives:

```yaml
# agents.yaml
workspace:
  artifacts: docs/architecture   # or a list: [docs, design]
```

Every `.mmd` or `.mermaid` file under those directories is one **artifact**;
anything else there is ignored, so diagrams can live beside other docs. The
same places are where the workspace's spec surface finds decision records
(ADRs), by their shape, so a decision a diagram depends on can sit beside it
in `docs/adr/` without anything else declared.
`uze agent artifacts check` draws each one the way the workspace would and
fails on one that does not draw, with or without the workspace running;
the workspace's architect surface, when it is used, draws them in terminal
cells to read and walk. Nothing
lists the files and nothing registers them: adding a file is the whole act
of adding a diagram, and an index beside them would be the one document
that is wrong the moment somebody adds the next one.

That is also the boundary of what belongs here. These diagrams are what
somebody **wrote down**. What a project *is* — where its lines are, which
files changed, how large each one is — is measured rather than written and
is already answered elsewhere; a diagram that restates it is a second
answer to go stale.

## Before you add a file

Ask whether the change belongs in a diagram that already exists. Most do.
A project's architecture is a handful of views that stay accurate, not a
directory that grows one file per conversation — and a view nobody updates
is worse than a view nobody drew, because it still looks current.

Add one only when the project has no view at the altitude your change is
about, and say in the diagram's title what it is a view *of*.

## Choose the altitude before the syntax

The single most common mistake is drawing at the wrong level, and nothing
catches it: a diagram full of components under `C4Context` draws perfectly
and tells the reader something false about the system's boundary.

- **`C4Context`** — the system as one box, and the people and software
  systems directly connected to it. Nothing internal appears here. If you
  are naming a crate, you are in the wrong view.
- **`C4Container`** — the applications and data stores *inside* the
  system, and the people and systems directly connected to them.
- **`C4Component`** — what one container is made of, with the other
  containers, people and systems its components touch around it.
- **`sequenceDiagram`** — an interaction over time. Use it when the claim
  is about **order**; a structure drawn as a sequence hides that it has no
  order.
- **`flowchart` / `graph`** — everything whose claim is about
  relationships that are neither containment nor time: a dependency
  direction, a pipeline, a lifecycle.

The first word of the file decides which area the surface lists it under,
so the choice above is also the choice of that first word. The C4 views
are listed from the outside in — context, then container, then component —
which is the order a reader meets them. `C4Dynamic` (one feature or use
case at runtime, steps numbered) and `C4Deployment` (where containers run
in one environment) are supplementary views, not levels: draw one only when
the static views cannot show an interaction that keeps recurring, or a
deployment that differs from the obvious one.

### What each C4 box is

- **Person** — a human: a user, a role, a persona. A program acting for
  somebody is not a person; an agent's actions are performed by the
  process running it, so that process (or the system it belongs to) is
  the box.
- **Software system** — what one team builds, owns and releases together.
  One binary with one release is one system, however many bounded
  contexts it has inside; a context is a container or a component, never
  a second system.
- **Container** — a runtime boundary: a process (a CLI, a server, a
  daemon, a helper the binary re-executes as) or a data store (a
  database, a directory of files). Ask *does it run, or hold data, on its
  own?* A crate, a library, a package or a module never is one — it
  organises code inside a container. Two commands of one binary that run
  in one process are one container; the same binary started as a separate
  long-lived process is another.
- **Component** — a grouping of code inside one container, sharing its
  process: a module, a crate, a service behind an interface. Not
  separately deployable. A container too big for one component view is
  drawn as several, each filtered to one part (one crate's modules, say)
  and saying so in its title.

### The notation, every time

- The title states the type and the scope: `System context diagram for
  shop`, `Container diagram for shop`, `Component diagram for shop API`.
  A view showing only part of its scope says so in the title.
- Every element is the macro of its type, with a short description of its
  responsibility; every container and component carries a real technology
  (`Rust, clap`, `PostgreSQL`, `Filesystem, JSON`) — never a crate name or a
  file path.
- Every relationship is one-way, with a label that says what happens in
  that direction — never `Uses`, `Calls` or `Runs` alone. Between
  containers, and from a container to a system, name the protocol in
  `Rel`'s fourth argument: `Rel(api, db, "Reads orders from", "SQL/TCP")`.
  No `BiRel`: draw the two directions as two relationships.
- A technology is a property of an edge, not a box: Git, HTTPS or a socket
  is how two things talk, not an external system of its own.

### One model, many views: aliases are join keys

Every C4 view is a projection of the same model, so a person or system
appears with the **same alias, name and description** in every view that
shows it, and an alias names **one element** across every file — never a
system here and a container or boundary there. The surface joins levels by
alias:

- a `C4Container` view's scope is its outermost `System_Boundary`; it is
  the inside of the `System` with that alias in the context view;
- a `C4Component` view's scope is its outermost `Container_Boundary`; it
  is the inside of the `Container` (or `ContainerDb`, `ContainerQueue`)
  with that alias in the container view. Several component views of one
  container are siblings;
- nested `Boundary` blocks inside a scope group elements and join nothing.

Where an alias cannot say it, declare the zoom: `$link` on an element to
another catalogued `.mmd` (`$link="docs/architecture/api-components.mmd"`)
opens that file as its inside. Prefer the alias; it is what other Mermaid
readers understand too.

## Name it for the reader, not for the filesystem

The surface lists an artifact by its Mermaid front-matter `title`, else a
`title` statement in the diagram, else the file name made readable. Give
it a title:

```
---
title: Container diagram for shop
---
C4Container
  ...
```

A name is what somebody scanning a list of a dozen diagrams reads. Name
the view (`Container diagram for shop`, `Install pipeline`, `Agent
lifecycle`), not the occasion for drawing it. A title holding a colon is
YAML, so quote it: `title: "Component diagram for shop API: billing"`.

## Join a box to the code it stands for

The code is the level below the last diagram, and it is not drawn: it is
linked. In C4, `$link` names the file an element stands for:

```
Component(delivery, "delivery", "Rust module", "Routes capabilities", $link="crates/core/src/delivery.rs")
```

Link every component, and every container that has no component view of
its own, to its entry or module file; a data store may link to the code
that names its paths. A container that *has* a component view is entered,
not linked. In a flowchart, a node does the same with `click`:

```
flowchart LR
  core[uze-core] --> integrations[uze-integrations]
  click core href "crates/uze-core/src/lib.rs"
```

The surface opens it. This is what turns a diagram from a picture into a
way into the code — use it wherever a box stands for something a reader
would then want to go and read. Paths are relative to the project.

## Prove it draws — this is not optional

```bash
uze agent artifacts check          # the whole project
uze agent artifacts check --format json
```

It reads every declared file and **draws** it the way the surface would,
then reports each one. It exits non-zero when any of them fails, so it is
a gate and not a report to skim.

Run it after every change to a diagram. Four things it catches that
reading the file does not:

1. **A diagram type that is listed but not drawn.** The surface draws the
   C4 family, sequences and flowcharts. Valid Mermaid outside those
   appears in the list and opens onto a sentence saying it cannot be
   drawn — which is not what you want a reader to find.
2. **A statement that cannot be read.** The check quotes it back.
3. **Relations with nowhere to go.** A diagram can draw with edges
   *missing* — every box present, the board looking finished, a
   relationship simply absent. This is the only failure here that nobody
   catches by looking, and it usually means the view is carrying more
   relationships than it can show. Split it, or raise its altitude.
4. **A link that opens nothing.** A box's `$link` or `click … href` is
   opened as a file: a module's directory, a file that moved or a path
   that leaves the project looks like a working link on the board and
   opens nothing. Link a Rust module by its file (`src/delivery.rs`), not
   its directory.

**Never describe the accepted syntax from memory, including from this
page.** The parser is the only authority on what draws, it moves, and a
remembered grammar diverges from it silently. Write the diagram, run the
check, read what it says. If a statement is refused, the answer is the
refusal — not a guess at what the parser meant.

## When a diagram does not appear at all

- The file is not under a directory `workspace.artifacts` declares, or does not end in
  `.mmd` / `.mermaid`.
- `agents.yaml` declares no `workspace.artifacts` at all, or declares a path that
  leaves the project — both of which the check names.
- The directory is read only a few levels deep, so a diagram buried far
  below the declared root is not found.
