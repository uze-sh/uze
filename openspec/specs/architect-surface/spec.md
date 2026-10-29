# architect-surface Specification

## Purpose
Draws a project's declared architecture artifacts — Mermaid flowcharts, C4
views and sequence diagrams — inside the workspace TUI, in terminal cells
alone, so the shape of a system can be read and walked down to its code
without leaving the terminal.

## Requirements

### Requirement: A project declares where its architecture artifacts live
A project SHALL declare where its artifacts live as `workspace.artifacts`
in `agents.yaml`: one directory, or a list of them, each relative to the
project root. The declaration SHALL name places only, never kinds: what a
file is, it says by its own content, so the surface reads the Mermaid
files under every declared directory and ignores the rest. The declaration
SHALL be optional.

#### Scenario: The project declares a directory
- **WHEN** `agents.yaml` holds `workspace: { artifacts: docs }`
- **THEN** the artifacts are read from `docs` under the project root, as
  deep as it goes

#### Scenario: The project declares several directories
- **WHEN** `workspace.artifacts` is `[docs, design]`
- **THEN** the diagrams of both are listed together

#### Scenario: A declared directory holds files of other kinds
- **WHEN** a declared directory holds Markdown, images and Mermaid files
- **THEN** only the Mermaid files are listed, and nothing is reported about
  the others

#### Scenario: The project declares nothing
- **WHEN** the surface is opened in a project whose `agents.yaml` has no
  `workspace.artifacts`
- **THEN** the surface SHALL say the project declares no artifacts yet
- **AND THEN** it SHALL show how to declare them, and SHALL NOT present
  this as an error

#### Scenario: The declared directory holds no diagram
- **WHEN** every declared directory exists and none holds a Mermaid file
- **THEN** the surface SHALL name the declared paths and say they hold no
  Mermaid files yet

#### Scenario: An unknown key under artifacts
- **WHEN** `workspace.artifacts` is a mapping, such as `{ path: docs }`
- **THEN** the manifest SHALL be rejected, saying `workspace.artifacts`
  takes a directory or a list of them

### Requirement: A declared path outside the project is refused
A path in `workspace.artifacts` that is absolute, or that leaves the
project, SHALL be refused rather than followed, and the refusal SHALL name
that entry; nothing SHALL be read until the declaration is fixed.

#### Scenario: The declared path leaves the project
- **WHEN** `workspace.artifacts` is `[docs, ../elsewhere]`
- **THEN** nothing SHALL be read, and the surface SHALL say that the
  `../elsewhere` entry of `workspace.artifacts` needs fixing

### Requirement: An artifact is a Mermaid file, described by itself
Every file ending in `.mmd` or `.mermaid` under the declared directory,
to a bounded depth and skipping hidden entries, SHALL be one artifact. No
manifest, index or registration SHALL be required beside the files. An
artifact's **area** SHALL be derived from the diagram's first word: the C4
family is one area, `sequenceDiagram` another, `flowchart` and `graph` a
third, and any other Mermaid diagram a fourth that is listed rather than
hidden.

#### Scenario: A file is added to the directory
- **WHEN** a `.mmd` file beginning with `C4Container` is added under the
  declared directory and the surface is opened
- **THEN** it SHALL be listed under the C4 area with no other file edited

#### Scenario: A comment before the diagram's first word
- **WHEN** a file's first lines are blank or `%%` comments
- **THEN** the area SHALL be derived from the first line that is neither

#### Scenario: A Mermaid diagram this surface does not draw
- **WHEN** a file begins with a diagram type outside the three drawn ones
- **THEN** it SHALL still be listed, under its own area
- **AND THEN** opening it SHALL say it could not be drawn rather than
  showing an empty board

### Requirement: An artifact is named by its own title
An artifact's **name** SHALL be its Mermaid front-matter `title`, else the
diagram's own `title` statement, else its file name made readable.

#### Scenario: A title in the front matter
- **WHEN** a file opens with a front matter whose `title` is `Containers`
- **THEN** the artifact SHALL be listed as `Containers`, and the front
  matter SHALL NOT be drawn as part of the diagram

#### Scenario: A file with no title
- **WHEN** `install-pipeline.mmd` carries no title of either kind
- **THEN** it SHALL be listed by a name derived from `install-pipeline`

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

### Requirement: The menu is one row of two selectors
The surface SHALL show, on a single row above the board, a selector for
the area and a selector for the artifact within it, each naming what is
currently open. Opening a selector SHALL list its choices over the board;
the area list SHALL say how many artifacts each area holds.

#### Scenario: Choosing another area
- **WHEN** the area selector is opened and `Sequence` is activated
- **THEN** the first artifact of the Sequence area SHALL open
- **AND THEN** both selectors SHALL name the new area and artifact

#### Scenario: A selector is clicked
- **WHEN** a selector is clicked, and then one of its choices
- **THEN** the click on the choice SHALL open it, and SHALL NOT also reach
  the board underneath

### Requirement: An open selector list takes the keyboard
While a list is open it SHALL take the keyboard: up and down move the
highlight, left and right step between the two lists, activate opens the
highlighted choice, and dismiss closes the list without leaving the
surface. A list longer than the board SHALL keep the highlight in view and
say where it is in the whole.

#### Scenario: A list longer than the board
- **WHEN** the artifact list holds more entries than the board has rows
  and the highlight is moved past the last visible one
- **THEN** the list SHALL scroll so the highlight stays visible
- **AND THEN** it SHALL show the highlight's position out of the total

#### Scenario: Dismiss with a list open
- **WHEN** a list is open and dismiss is triggered
- **THEN** the list SHALL close and the surface SHALL stay open

### Requirement: Diagrams are drawn in terminal cells alone
A diagram SHALL be drawn with characters in cells, colored by the active
theme's roles, and SHALL NOT depend on any terminal graphics protocol or
on a font the terminal was not already using. The surface SHALL offer the
same drawing in box-drawing characters and in plain ASCII, and the
artifact's own Mermaid source, switched by one action.

#### Scenario: A terminal with no graphics protocol
- **WHEN** the surface is opened over SSH inside a multiplexer
- **THEN** the diagram SHALL be drawn the same as in a local terminal

#### Scenario: The ASCII rendering
- **WHEN** the next-rendering action is triggered from the default
  rendering
- **THEN** every line, corner, junction and arrowhead SHALL be drawn with
  ASCII characters only

#### Scenario: The source rendering
- **WHEN** the next-rendering action is triggered again
- **THEN** the artifact's Mermaid text SHALL be shown in place of the
  drawing

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

### Requirement: What cannot be drawn fails one artifact, never the surface
Styling statements, and a C4 macro the surface does not know, SHALL be
ignored without failing the rest of the diagram. A statement that cannot
be read SHALL fail that one artifact, with the reason shown in its place,
and never the surface.

#### Scenario: A styling statement
- **WHEN** a diagram holds `classDef`, `style`, `linkStyle` or
  `UpdateLayoutConfig`
- **THEN** the statement SHALL be ignored and the diagram still drawn

#### Scenario: A statement that cannot be read
- **WHEN** a C4 relation names only one end
- **THEN** the surface SHALL say the artifact could not be drawn and quote
  the statement
- **AND THEN** every other artifact SHALL still open

### Requirement: The board moves freely, by pointer and by key
The drawing SHALL sit on a board larger than itself. The board SHALL be
moved by dragging it with the pointer, by the wheel in both axes, by the
four pan actions, and by page up and page down. It SHALL be free to move
until the drawing's edge reaches the middle of the view, so any box can
reach the middle. An artifact SHALL open centred when it fits
the view. The board's empty space SHALL be marked by a faint dot grid that
moves with the drawing and never shows through a box or a label.

#### Scenario: Dragging the board
- **WHEN** the pointer is pressed on the board, moved by ten columns and
  released
- **THEN** the drawing SHALL have moved by ten columns with it
- **AND THEN** the release SHALL NOT be treated as a click on a box

#### Scenario: Bringing a corner box to the middle
- **WHEN** the board is moved as far as it goes toward a corner of the
  drawing
- **THEN** that corner of the drawing SHALL be at the middle of the view

#### Scenario: A small diagram
- **WHEN** an artifact smaller than the view is opened
- **THEN** it SHALL be drawn centred in the view

#### Scenario: The pointer is released outside the surface's content
- **WHEN** a drag ends
- **THEN** no part of the gesture SHALL reach the pane under the surface

### Requirement: A minimap of a fixed size says where the view is
The surface SHALL show a minimap of the whole drawing in a corner of the
board, with the part currently in view marked on it. The minimap SHALL be
the same size for every artifact. A click on it SHALL move the view to the
place clicked. A view too small to spare the room, and the source
rendering, SHALL show none.

#### Scenario: Two artifacts of very different sizes
- **WHEN** a small and a large artifact are each opened in the same view
- **THEN** their minimaps SHALL occupy the same number of cells

#### Scenario: A click on the minimap
- **WHEN** the far end of the minimap is clicked
- **THEN** the view SHALL move so the matching part of the drawing is at
  its middle

#### Scenario: A view too small for it
- **WHEN** the surface is drawn in a view smaller than the minimap needs
- **THEN** no minimap SHALL be shown, and the board SHALL still move

### Requirement: A selected box lights what it connects to
Selecting a box SHALL draw it, every edge that touches it and those
edges' labels in the accent role, and the footer SHALL name the box and
say how many edges come into it and go out of it. A box SHALL be selected
by a click, and by the four directional selection actions, which move to
the nearest box in that direction and bring it into view. With nothing
selected the footer SHALL say how many boxes and edges the drawing holds.

#### Scenario: A box is clicked
- **WHEN** a box with two incoming and three outgoing edges is clicked
- **THEN** the box and those five edges SHALL be lit
- **AND THEN** the footer SHALL read its title, `2 in` and `3 out`

#### Scenario: Selecting toward a direction
- **WHEN** a box is selected and the select-right action is triggered
- **THEN** the nearest box to its right SHALL become the selection
- **AND THEN** the board SHALL move if that box was out of view

#### Scenario: Clicking the selection again
- **WHEN** a selected box that leads nowhere is clicked again
- **THEN** the selection SHALL be cleared

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

### Requirement: The surface is reachable the way the code surface is
The surface SHALL open by a `toggle-architect` action bound in the
workspace and in the code surface, and by a chip in the tab strip; the
same action SHALL close it. While it is open its scope SHALL seal the
keyboard from the pane underneath. From it, the changes and files entry
points SHALL open the code surface directly. Every key the surface
answers to SHALL be a named, rebindable action listed in the action index,
and the surface's footer SHALL advertise keys read from the keymap.

#### Scenario: Opening from the tab strip
- **WHEN** the architect chip in the tab strip is clicked
- **THEN** the surface SHALL open over the workspace

#### Scenario: A key typed while the surface is open
- **WHEN** a letter bound to nothing in the architect scope is typed
- **THEN** it SHALL NOT reach the pane underneath

#### Scenario: A rebound key
- **WHEN** the user's keymap binds `choose-artifact` to another chord
- **THEN** the footer SHALL show that chord

### Requirement: Reading the artifacts never blocks the workspace
Resolving the project's declaration and reading its artifact files SHALL
happen off the thread that draws. While it is outstanding the surface
SHALL be open and say it is reading. An answer SHALL carry the project it
was asked for, and one that arrives after the surface was closed or
reopened for another project SHALL be dropped.

#### Scenario: A slow filesystem
- **WHEN** the surface is opened and the artifact directory takes seconds
  to read
- **THEN** the workspace SHALL keep drawing and taking input meanwhile
- **AND THEN** the surface SHALL say it is reading the project's artifacts

#### Scenario: The surface was closed before the answer
- **WHEN** the surface is closed while the read is outstanding
- **THEN** the late answer SHALL be dropped and nothing SHALL reopen

### Requirement: A project can be told whether its artifacts draw
UZE SHALL offer a command that reads every artifact the project declares
and draws it, reporting for each one whether it drew and exiting non-zero
when any did not. It SHALL use the same catalog, parser and layout the
surface uses, and SHALL NOT restate the accepted syntax anywhere: a reason
it reports SHALL be the one the surface would show. The reader is the
agent that wrote the file, so the command SHALL sit in the `agent`
namespace and SHALL offer both text and JSON.

#### Scenario: A diagram type the surface does not draw
- **WHEN** the declared directory holds a file beginning with `gantt`
- **THEN** the check SHALL report it not drawn, with the surface's own
  reason
- **AND THEN** the command SHALL exit non-zero

#### Scenario: A statement that cannot be read
- **WHEN** a C4 relation names only one end
- **THEN** the check SHALL quote the statement back
- **AND THEN** every other artifact SHALL still be reported

### Requirement: The check reports each artifact in one of three states
An artifact SHALL be reported in one of three states: drawn; drawn with a
stated number of relations that found no path; or not drawn, with the
reason quoted. The second SHALL fail the check like the third, because a
diagram missing a relation still draws every box.

#### Scenario: A diagram whose relations have nowhere to go
- **WHEN** a flowchart joins every node to every other, and the layout
  leaves edges unrouted
- **THEN** the check SHALL report how many found no path
- **AND THEN** the command SHALL exit non-zero, as for one that does not
  draw at all

### Requirement: The check judges a declaration apart from what its directory holds
A project's *declaration* SHALL be judged apart from what its directory
holds. Declaring no artifacts, and declaring a directory that holds none,
SHALL both pass. A declaration the host will not follow, and a declared
directory that cannot be read, SHALL fail.

#### Scenario: A project with nothing declared
- **WHEN** `agents.yaml` declares no `artifacts:`
- **THEN** the check SHALL say so and SHALL exit zero

#### Scenario: A directory declared before anything is drawn in it
- **WHEN** the declared directory holds no Mermaid file
- **THEN** the check SHALL report nothing checked and SHALL exit zero

#### Scenario: A declaration the host will not follow
- **WHEN** `artifacts.path` leaves the project
- **THEN** the check SHALL report the refusal and SHALL exit non-zero

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
