# code-surface Specification

## Purpose
The code surface's map of the active checkout: a treemap measured from Git,
with nothing to write or declare, that shows which files are large and which
are hot, is walked one directory at a time, and hands a chosen file back to
the surface as its selection.
## Requirements
### Requirement: The checkout is drawn as a map nobody has to write
The code surface SHALL offer, beside its file tree, a map of the active
checkout: a treemap in which a file's area is proportional to its line
count, and a directory with a single directory in it is drawn as one tile
named for both. The map SHALL cover the text files Git tracks and the
untracked ones it does not ignore, and SHALL leave out binary files. It
SHALL require no file, no manifest key and no declared artifact. Where
the checkout has not been measured, the map SHALL NOT be offered.

#### Scenario: An ignored directory
- **WHEN** the repository ignores `target/`
- **THEN** no tile SHALL stand for anything under `target/`

#### Scenario: A chain of single directories
- **WHEN** `crates/` holds only `core/`, which holds only `src/`
- **THEN** one frame titled `crates/core/src` SHALL be drawn, not three

#### Scenario: Not a repository
- **WHEN** the surface is opened outside a Git repository
- **THEN** no `Code` area SHALL be listed

#### Scenario: Before the measurement arrives
- **WHEN** the surface is opened and the checkout's measurement is still
  outstanding
- **THEN** the map SHALL NOT be offered until it arrives

### Requirement: The map is offered beside the file tree, never beside the changes
The map SHALL be offered only where the file tree is what the surface is
navigating. The changes half SHALL NOT offer it, by chip or by key: a
map of the whole checkout answers a question nobody reviewing a change is
asking.

#### Scenario: Beside the tree, and not beside the diff
- **WHEN** the surface is showing the changes of a measured checkout
- **THEN** the map SHALL NOT be among the ways of showing it
- **AND THEN** the key that toggles it SHALL leave the changes on show

### Requirement: The map fits the view, and is entered rather than moved
The map SHALL take the whole frame while it is on show — it is a picture
of the whole checkout, and the tree it replaces is what leaving it goes
back to. It SHALL be laid out for the space it has, and laid out again
when that space changes; it SHALL NOT be panned. Tiles SHALL be laid out
so they stay close to square as seen, a cell being about twice as tall as
it is wide. Activating a selected directory, or a folded tile, SHALL enter
it: the map is laid out again for that directory alone.

#### Scenario: The terminal is resized
- **WHEN** the view becomes narrower while the map is shown
- **THEN** the map SHALL be laid out again to fill exactly the new space

#### Scenario: Entering a directory
- **WHEN** the directory `crates` is selected and activated
- **THEN** the map SHALL show the contents of `crates` alone, filling the
  view
- **AND THEN** the trail SHALL end in `crates`

#### Scenario: Entering a folded tile
- **WHEN** a folded tile is activated
- **THEN** the map SHALL show only the files it stood for

#### Scenario: The arrows
- **WHEN** a pan action is triggered on the code map
- **THEN** the selection SHALL move to the nearest tile in that direction

### Requirement: A level draws its own children, each with room for its name
A level SHALL draw its own children and nothing below them, no directory
drawn around its contents, so the tile count never depends on the
repository's size. Every tile SHALL be large
enough to carry its own name; a split leaving one too small for it SHALL
be made again one tile smaller until every tile kept can. What is left
out SHALL be folded into one tile that says how many files it stands for.
Borders SHALL be drawn as one grid, two tiles side by side sharing a line.

#### Scenario: Many small files
- **WHEN** a directory holds three large files and forty one-line files
- **THEN** the three SHALL be drawn and named
- **AND THEN** one tile SHALL read that it stands for the other forty

#### Scenario: A tile too thin for its name
- **WHEN** a child's share of the area would be drawn as a sliver
- **THEN** it SHALL be folded rather than drawn without a name

#### Scenario: A directory holding another directory
- **WHEN** `src/` holds `ui/`, which holds seven files
- **THEN** `src/` SHALL be one tile, with nothing of `ui/` drawn inside it

### Requirement: The trail shows the way into the map and back out
Once a level is entered, the trail SHALL show the way in, beginning with
the checkout. A click on a step of the trail SHALL go back to that level,
with the directory that was entered selected. The key that closes SHALL
let go of the selection, then leave the level, and only with neither left
SHALL it leave the map.

#### Scenario: Coming back
- **WHEN** `level-up` is triggered inside `crates`
- **THEN** the whole map SHALL be shown with `crates` selected

#### Scenario: Closing from inside a level
- **WHEN** the key that closes is pressed inside `crates` with a tile
  selected
- **THEN** the selection SHALL be let go and `crates` SHALL stay on show
- **AND THEN** pressing it again SHALL leave `crates`, and only a press
  with neither left SHALL leave the map

### Requirement: The map says which files are large and which are hot
A file's tile SHALL be coloured by how many commits touched it in the last
year, ranked against the other files of the map in four steps from none to
hottest, using semantic roles only. A directory drawn as one tile SHALL
take the step weighted by its files' lines. A file with uncommitted
changes SHALL be marked, and that mark SHALL survive a name too long to
fit.

#### Scenario: A large file nobody touches and a small one everybody does
- **WHEN** the map holds a 5,000-line file with no commit this year and a
  200-line file with the most commits of any
- **THEN** the first SHALL be the larger tile in the coldest step
- **AND THEN** the second SHALL be the smaller tile in the hottest step

### Requirement: A tile and the heading say what they stand for
A tile SHALL say its own name and what it holds — its lines, and the
files under it or the commits that touched it — as far down as there is
room. Selecting a tile SHALL name its path, its lines, its commits and
whether it is changed in the heading; with nothing selected the heading
SHALL give the level's files and lines.

#### Scenario: A tile is selected
- **WHEN** the tile of `src/main.rs` is clicked
- **THEN** the footer SHALL read its path, its line count and its commits

### Requirement: The map is offered in three renderings
The map SHALL be offered in three renderings on one control — the map,
the same map in ASCII, and the ranking itself: every file by lines,
largest first, with its commits.

#### Scenario: The ranking
- **WHEN** the `Ranking` rendering is chosen
- **THEN** the files SHALL be listed by lines, largest first

#### Scenario: The ASCII rendering
- **WHEN** the map is shown in the ASCII rendering
- **THEN** every glyph it draws SHALL be an ASCII character

### Requirement: A file on the map is the surface's selection
Activating a selected file's tile, or clicking it a second time, SHALL
select that file and leave the map for whichever half the map was opened
over. The map is a fourth way of asking about the same path, so the
selection SHALL survive the switch the way it does between the other
three.

#### Scenario: Down to the file
- **WHEN** the tile of `src/main.rs` is selected and activated
- **THEN** `src/main.rs` SHALL be the surface's selection
- **AND THEN** the half the map was opened over SHALL be on show again

### Requirement: Measuring the checkout never blocks the workspace
The measurement SHALL be made off the thread that draws, and asked for
once per checkout the surface is opened on — a `git grep` over every file
is not a read to repeat, and the answer outside a repository is nothing,
which SHALL NOT be asked for again. Its answer SHALL carry the checkout
it was asked for, and one that arrives after the surface was closed or
reopened elsewhere SHALL be dropped. A measurement arriving SHALL NOT
change what is on show: the map is a mode somebody asks for.

#### Scenario: The measurement arrives while the diff is being read
- **WHEN** the surface is showing a diff and the measurement arrives
- **THEN** the diff SHALL stay on show
- **AND THEN** the map SHALL be on offer from the file tree

#### Scenario: The surface was closed before the answer
- **WHEN** the surface is closed while the measurement is outstanding
- **THEN** the late answer SHALL be dropped

