## Context

C4 (c4model.com) is a hierarchy of zooms. A container diagram's scope is
"a single software system"; a component diagram's is "a single
container"; the code level is left to the IDE, which here is a `$link` the
code surface opens. System landscape, dynamic and deployment diagrams are
supplementary views. The model's FAQ endorses splitting one level into
several simpler diagrams. Notation asks for a title naming type and scope,
a key, an explicit type, a description and a technology on every element,
and a label on every unidirectional line.

The surface took a level from the diagram keyword (`C4Context` = 1 … ) and
treated an area whose views were all on distinct levels as a ladder, any
other as a set. A box led inside whenever any other C4 file drew a
boundary with its alias, whatever its kind.

## Goals / Non-Goals

**Goals:** navigation that is the C4 tree, with nothing project-specific
in it; every rule of C4 notation the engine can honour, honoured; a check
that tells an author where a file departs from the notation.

**Non-Goals:** drawing the code level; a C4 model shared across files
(Mermaid aliases are per file, so identity across files stays a
convention, the alias); line anchors in a `$link` reaching the editor
(the code surface's `Entry::Path` carries no line; the fragment is
dropped, not followed).

## Decisions

### The tree is read from boundaries, matched by kind

A view's *scope* is the alias of its outermost boundary of the kind its
level expands: `System_Boundary` (or `Boundary` typed `System`) for a
container view, `Container_Boundary` (or `Boundary` typed `Container`)
for a component view. A context view's scope is its one internal system.
Its *parent* is the first view one level up that draws an internal element
of the matching kind (a system for a container view; a container,
database or queue container for a component view) under that alias.

An element `$link` naming another catalogued `.mmd` file overrides the
inference: that file is the element's inside, and its parent is the file
that links it. This is the one unambiguous join Mermaid offers, so it is
honoured first.

Alternatives: matching by label (labels are prose and change), by file
naming (project-specific), or by any alias regardless of kind (the old
rule, which let a system box skip the container level).

### Supplementary views hang beside the views of their scope

A dynamic view is scoped by its outermost system or container boundary
and gets the parent a container or component view with that scope would
get; a deployment view, a landscape (a context view with no single
internal system, or with an `Enterprise_Boundary`) and any view whose
parent cannot be found are roots. Siblings are listed with the levels
first (landscape, context, container, component), then dynamic and
deployment, then by name.

### The trail is the path of ancestors, and its last step opens the siblings

The surface keeps no list of entered boxes: the path from the root to the
view on show is computed, so a view opened from anywhere shows the same
path. Every step is named by its level (`Context`, `Containers`,
`Components`, `Landscape`, `Dynamic`, `Deployment`), the view on show
included: a title names type and scope and is a sentence, so it is pinned
to the board's top-left corner over the drawing instead, where it stays
whatever the board is moved to. The step on show opens the views that
share its parent, by their titles, through
`TrailStep::choices` (a new field, empty by default, so the code map's
trail is untouched). Going up opens the parent with the box whose alias is
the child's scope selected and centred. A box with several children enters
the first; the others are its siblings.

### The key is part of the drawing

C4 asks every diagram for a key, and Mermaid cannot draw one. The engine
paints it on the board under the drawing, so it moves with it and never
covers a box. It lists only what the drawing uses: element shapes, the
external style, boundary kinds, dotted lines, the two marks, and the
relations whose labels went to a marker.

### A label with no room becomes a marker

The old rule left a label off rather than write it over a line. The rule
stays, and what is left off now leaves a numbered marker (`[3]`) inline or
beside the line, with `3  from → to: label` in the key; a relation whose
marker also finds no room is still listed there. A dynamic step's number
is the marker.

### Read the macros the way Mermaid does

Arguments are matched to each macro's documented signature, positional
then `$name=`; `$tags`, `$sprite` and styling are read and ignored. The
three typed boundaries take tags, not a type, as their third argument.
In `C4Dynamic` every relation is numbered by statement order, which is
what Mermaid draws (it ignores `RelIndex`'s own index). `BiRel` draws two
heads; `Rel_Back` is drawn from its second end to its first, as
C4-PlantUML defines it (Mermaid does not document it).

### Types read as C4 names them

`[Software System]`, `[External Software System]`, `[Container: Rust]`,
`[External Container: …]`, `[Person]`, `[External Person]`. Shape still
tells a database (a lid) and now a queue (a second wall on the right)
apart from a person (rounded corners).

### The check notes departures from the notation

Beside each drawn C4 artifact, `uze agent artifacts check` lists notes: an
element with no description, a container or component with no technology,
a relation with no label, a container or component view whose scope no
view above draws, and an alias that names a different kind of element in
another view. Notes do not fail the check: C4 is guidance other projects
may follow loosely. The repository holds its own diagrams to zero notes in
a test.

## Risks / Trade-offs

- Alias identity across files is a convention → the check's note on an
  alias that names two kinds of element catches the common mistake.
- A view the old rule joined (any boundary alias) may now be a root →
  the check names it ("zooms into nothing"), and an explicit `$link`
  joins it.
- The key costs rows under the drawing → it is drawn once, compactly, and
  only what is used.
