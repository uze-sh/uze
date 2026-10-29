## MODIFIED Requirements

### Requirement: Artifacts are listed by area, and C4 from the outside in
Artifacts SHALL be ordered by area, and within an area by name, except
the C4 area, which SHALL be ordered as its zoom tree is walked depth
first: each view before the views that zoom into it, and among views with
one parent the levels first (landscape, context, container, component),
then dynamic, then deployment, then by name.

#### Scenario: Three C4 levels whose names sort the other way
- **WHEN** the C4 area holds `Containers` zooming into the system
  `System context` draws, and `Core components` zooming into a container
  `Containers` draws
- **THEN** they SHALL be listed as `System context`, `Containers`, `Core
  components`

#### Scenario: Two component views of one container
- **WHEN** the C4 area holds a context view of `uze`, a container view
  scoped to `uze` drawing container `cli`, and two component views each
  scoped to `cli`
- **THEN** they SHALL be listed context, container, then the two
  component views by name

#### Scenario: Stepping to the next artifact
- **WHEN** the next-artifact action is triggered on the last artifact of
  an area
- **THEN** the first artifact of the following area SHALL open
- **AND THEN** on the last artifact of all, the first of all SHALL open

### Requirement: The subset of Mermaid that architecture is written in is drawn
The surface SHALL draw flowcharts (nested subgraphs, node shapes, solid,
dotted and thick edges, edge labels), the C4 family (persons, systems,
containers, components, their database, queue and external variants,
boundaries, deployment nodes, relations) and sequence diagrams.

#### Scenario: A flowchart with nested subgraphs
- **WHEN** a flowchart nests one subgraph inside another
- **THEN** each SHALL be drawn as a titled frame around the boxes it
  holds, the inner inside the outer

#### Scenario: Two edges meet
- **WHEN** two edges share a stretch of line or cross
- **THEN** the cell where they meet SHALL be drawn as a junction or a
  crossing, not as whichever edge was drawn last

#### Scenario: An external C4 element
- **WHEN** a C4 view holds a `System_Ext`
- **THEN** it SHALL be drawn visibly fainter than the system's own
  elements
- **AND THEN** its type SHALL read `External Software System`

#### Scenario: A label with nowhere to go
- **WHEN** an edge's label fits neither on its line nor beside it
- **THEN** a numbered marker SHALL stand for it on or beside the line
- **AND THEN** the key SHALL list the marker with the label and the two
  ends

#### Scenario: An edge that cannot be routed
- **WHEN** no path exists for an edge
- **THEN** the rest SHALL still be drawn, and the footer SHALL say how
  many edges were not routed

### Requirement: C4 views are levels that can be walked, down to the code
A container view SHALL zoom into the system its outermost `System_Boundary`
names, a component view into the container its outermost
`Container_Boundary` names, and the parent SHALL be the view one level up
drawing an internal element of that kind and alias. A `$link` to another
catalogued `.mmd` SHALL declare the zoom. A `$link` or `click … href` to
any other project path SHALL lead to the code.

#### Scenario: Going inside a container
- **WHEN** the container view draws container `cli`, a component view
  holds `Container_Boundary(cli, …)`, and `cli` is selected and activated
- **THEN** the component view SHALL open
- **AND THEN** the menu SHALL show the path from the context view to it

#### Scenario: A system box does not skip the container level
- **WHEN** the context view draws system `uze` and a component view has
  a boundary also called `uze`
- **THEN** `uze` SHALL lead into the container view scoped to `uze`, not
  into the component view

#### Scenario: An explicit zoom
- **WHEN** an element carries `$link="docs/architecture/payments.mmd"`
  and that file is a catalogued C4 view
- **THEN** activating it SHALL open that view, and the menu SHALL show the
  linking view as its parent

#### Scenario: Down to the code
- **WHEN** a component carries `$link="crates/uze-core/src/store.rs"` and
  is activated
- **THEN** the code surface SHALL open showing that file's contents

#### Scenario: A flowchart that reuses a container's name
- **WHEN** a flowchart holds a subgraph named `core`
- **THEN** the C4 box `core` SHALL NOT lead into that flowchart

#### Scenario: A box that leads nowhere
- **WHEN** a box with no view below it and no link is activated
- **THEN** nothing SHALL happen, and the box SHALL carry no mark

### Requirement: A box that leads somewhere says where
A box that leads somewhere SHALL carry a mark in its border for each way
it leads, inside and to the code, and the footer SHALL say what
activating it does. A box that leads both ways SHALL go inside when
activated and to the code when its code mark is clicked.

#### Scenario: A box that leads inside is selected
- **WHEN** a box with a view below it is selected
- **THEN** its border SHALL carry the mark for leading inside
- **AND THEN** the footer SHALL say that activating it goes inside

#### Scenario: A box that leads both ways
- **WHEN** a container with a component view also carries a `$link` to a
  file, and its code mark is clicked
- **THEN** the code surface SHALL open on that file

### Requirement: Going inside a C4 view leaves a trail back out
The menu SHALL show the path of ancestors from the root to the C4 view on
show, whichever way it was reached; the view on show SHALL open a list of
the views sharing its parent. Going up, or clicking an ancestor, SHALL
open that view with the box zoomed into selected and in view.

#### Scenario: Coming back up
- **WHEN** the level-up action is triggered on a component view of `cli`
- **THEN** the container view SHALL open with `cli` selected and in view

#### Scenario: A view opened from the menu
- **WHEN** a component view is opened with the next-artifact action
- **THEN** the menu SHALL show the context and container views before it

#### Scenario: Choosing an artifact from the menu while inside
- **WHEN** an artifact is opened from a selector while a path is shown
- **THEN** the path SHALL be that artifact's own ancestors, not the way
  the viewer came

#### Scenario: The step on show names its level
- **WHEN** a component view titled `uze CLI: uze-core` is on show
- **THEN** its step SHALL read `Components`
- **AND THEN** the title SHALL be shown on the row of the drawing's
  modes

#### Scenario: Views that share a parent
- **WHEN** two component views zoom into `cli` and one is on show
- **THEN** its step SHALL open a list naming both, and choosing the other
  SHALL open it under the same path

#### Scenario: A supplementary view
- **WHEN** a dynamic view's outermost boundary is `System_Boundary(uze)`
- **THEN** it SHALL be listed beside the container view of `uze`, never
  as a level below it

## ADDED Requirements

### Requirement: Every diagram carries a key
Under every drawn C4 view the surface SHALL draw a key listing the
notation it uses: relationships, element shapes, the external style,
boundaries, the marks for leading inside and to the code, and every
relation whose label became a marker. Under any other graph the key SHALL
list only shapes, marks and markers it uses. Unused notation SHALL NOT be
listed.

#### Scenario: A view with a database and an external system
- **WHEN** a container view holds a `ContainerDb` and a `System_Ext`
- **THEN** the key SHALL explain the database shape and the external
  style
- **AND THEN** it SHALL NOT mention a queue

### Requirement: C4 macros are read as Mermaid specifies them
Each C4 macro's arguments SHALL be matched to its Mermaid signature,
positionally then by `$name=`. Typed boundaries SHALL take tags, not a
type, as their third argument. In a dynamic view every relation SHALL be
numbered by statement order. `BiRel` SHALL draw a head at each end and
`Rel_Back` SHALL point at its first end.

#### Scenario: A named technology
- **WHEN** a container is written `Container(api, "API", $techn="Rust")`
- **THEN** its type SHALL read `Container: Rust`

#### Scenario: A dynamic view
- **WHEN** a `C4Dynamic` view holds `RelIndex(7, a, b, "x")` then
  `Rel(b, c, "y")`
- **THEN** the two SHALL read `1. x` and `2. y`

### Requirement: A box's link names a path in the project
A `$link` or `click … href` SHALL be followed only when it names a path
inside the project: an absolute path, a `..` or a URL SHALL lead nowhere.
A link to a directory SHALL open the code surface on it, and a `#`
fragment SHALL be dropped before the path is resolved.

#### Scenario: A URL
- **WHEN** a component carries `$link="https://example.com/x"`
- **THEN** it SHALL carry no mark and the check SHALL report it leaving
  the project

### Requirement: The check notes departures from C4 notation
Beside each drawn C4 artifact the check SHALL list notes for: an element
without a description, a container or component without a technology, a
relation without a label, a container or component view whose scope no
view above draws, and an alias naming a different kind of element in
another view. Notes SHALL NOT fail the check.

#### Scenario: A container with no technology
- **WHEN** a container view writes `Container(api, "API")`
- **THEN** the check SHALL note that `api` names no technology
- **AND THEN** the command SHALL still exit zero
