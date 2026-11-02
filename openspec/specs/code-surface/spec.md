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
navigating. A checkout's changes are the review's, which SHALL NOT offer
the map, by chip or by key: a map of the whole checkout answers a question
nobody reviewing a change is asking.

#### Scenario: Beside the tree, and not beside the diff
- **WHEN** the review is showing the changes of a measured checkout
- **THEN** the map SHALL NOT be among the ways of showing it
- **AND THEN** the key that toggles it in the code surface SHALL do nothing in the review

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
select that file and leave the map for the file tree. The map is another
way of asking about the same path, so the selection SHALL survive the
switch.

#### Scenario: Down to the file
- **WHEN** the tile of `src/main.rs` is selected and activated
- **THEN** `src/main.rs` SHALL be the surface's selection
- **AND THEN** the file tree SHALL be on show again

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

### Requirement: An image is drawn where a file's text would be
When the selected file in the files half is a PNG, JPEG, GIF, WebP, BMP or
ICO image, the content column SHALL draw the image under the file's path,
fitted to the column with its proportions kept and centred in it. An
animated image SHALL be drawn as its first frame. Anything else that is
not text SHALL keep saying it is not a text file.

#### Scenario: A screenshot in the tree
- **WHEN** the viewer selects `docs/screenshot.png`
- **THEN** the content column SHALL show the image under
  `docs/screenshot.png` rather than a message

#### Scenario: Each common format
- **WHEN** the viewer selects a `.jpg`, `.jpeg`, `.gif`, `.webp`, `.bmp` or
  `.ico` file holding a valid image
- **THEN** the image SHALL be drawn

#### Scenario: An animated GIF
- **WHEN** the selected file is a GIF with several frames
- **THEN** its first frame SHALL be drawn, still

#### Scenario: A file that only claims to be an image
- **WHEN** `logo.png` holds bytes that are not a PNG
- **THEN** the column SHALL say `logo.png` is not a text file, in the
  muted tone of a state rather than in red

#### Scenario: A format the surface does not draw
- **WHEN** the selected file is an `.svg` or an `.avif`
- **THEN** the column SHALL say it is not a text file

### Requirement: An image reaches every terminal, and a capable one at its own resolution
An image SHALL be drawn on every terminal with true colour, at two pixels
per cell. Where the terminal reports when the workspace opens that it
draws Sixel and how many pixels its cells hold, the image SHALL also be
sent as Sixel at that resolution, over the cells that stand in for it. A
terminal that answers neither, or not in time, SHALL get the cells alone.

#### Scenario: A terminal without graphics
- **WHEN** the terminal does not list Sixel among its device attributes
- **THEN** the image SHALL be drawn in cells and nothing else SHALL be sent

#### Scenario: A terminal that speaks Sixel
- **WHEN** the terminal lists Sixel and reports cells of 10 by 20 pixels,
  and an image is selected in a content column 120 cells wide
- **THEN** the image SHALL be sent as Sixel up to 1200 pixels wide,
  within the cells the image occupies

#### Scenario: Opening the workspace
- **WHEN** the terminal answers none of the questions
- **THEN** opening the workspace SHALL NOT wait more than the probe's
  bound for an answer

### Requirement: Pixels are sent again only when what covers them changes
The Sixel image SHALL be sent when it first appears and again only when
its file, its place, its size or the theme changes, or when something drawn
over it goes away. While a menu, dialog or notice covers any of its cells,
the image SHALL NOT be sent over it. A frame that changes nothing about
the image SHALL send nothing.

#### Scenario: Scrolling the tree beside the image
- **WHEN** the viewer moves the pointer over the tree while an image is
  shown
- **THEN** no image data SHALL be written to the terminal for those frames

#### Scenario: A menu opened over the image
- **WHEN** a row menu opens over part of the image and is then dismissed
- **THEN** the image SHALL NOT be drawn over the open menu, and SHALL be
  drawn whole again once it is dismissed

### Requirement: An image is shown as sharp as the room allows
An image SHALL be resampled once, from the decoded file, to the size it is
drawn at. It SHALL NOT be enlarged past its own pixel size. Transparent
pixels SHALL be laid over the theme's ground, so a transparent image sits
on the surface rather than on a colour nobody chose.

#### Scenario: A small icon
- **WHEN** a 32 by 32 icon is shown on a terminal drawing Sixel
- **THEN** it SHALL be sent at 32 by 32 pixels, not stretched to the column

#### Scenario: A transparent logo
- **WHEN** a PNG with a transparent background is shown on a dark theme
- **THEN** its background SHALL be the theme's ground colour

### Requirement: An image is never opened for editing
A file drawn as an image SHALL remain a file that is not text: the
surface SHALL NOT offer to edit it, and no save SHALL be possible from
it, since a buffer holding no text written over an image destroys it.

#### Scenario: Pressing edit on an image
- **WHEN** an image is shown and the viewer presses the edit key
- **THEN** no editor SHALL open and the file SHALL be unchanged on disk

### Requirement: Reading an image never blocks the workspace
Reading and decoding an image, and encoding it for the terminal, SHALL
happen off the thread that draws. A file larger than the image read limit
SHALL be reported as too large to open rather than read.

#### Scenario: A large photograph
- **WHEN** the viewer selects a 20 MB JPEG
- **THEN** the surface SHALL say it is reading while the decode runs, and
  SHALL keep accepting input

### Requirement: Returning to an agent shows the code surface as it was left
When an agent comes back to the front with the code surface kept for it, the
surface SHALL show the mode it was left on, the selection, the opened
directories and the scroll. A fresh open through its door SHALL open on
the file tree.

#### Scenario: Files left open on an agent
- **WHEN** the code surface shows a file in Files on agent A, and the operator selects agent B, then agent A
- **THEN** the code surface stands on A in Files, on that file

### Requirement: A click on a file's line starts typing there
A click on a line of a text file in the files half SHALL place the caret
where the click landed and start typing, with no key first. The line under
the pointer SHALL be highlighted across its row while a click there would
type, and the caret's line SHALL be highlighted while typing. A diff's
lines SHALL NOT be highlighted under the pointer.

#### Scenario: Clicking into a file
- **WHEN** a file is shown and the viewer clicks its second line
- **THEN** typing starts with the caret on that line, where the click landed

### Requirement: Unsaved work is the text that differs from the file, and is kept
A buffer SHALL count as unsaved only while its text differs from what the
file last held. Moving to another file SHALL keep an unsaved buffer and
SHALL NOT refuse the move. Returning to the file SHALL show the buffer.
Closing the surface SHALL ask once while any buffer is unsaved. A surface
put away without being asked, as when another agent comes to the front,
SHALL keep its unsaved work and give it back when opened again on its
checkout.

#### Scenario: Typing undone
- **WHEN** the viewer erases a character and types it back
- **THEN** the file is not unsaved, and closing does not ask

#### Scenario: Another agent comes to the front
- **WHEN** a buffer is unsaved and the viewer switches to another agent and back
- **THEN** the code surface opens with the buffer and its tab as they were

#### Scenario: Moving away from unsaved work
- **WHEN** a buffer is unsaved and the viewer selects another file
- **THEN** the other file opens, the tree marks the first as unsaved, and selecting it again shows the typing

### Requirement: Files open in tabs
Each file opened SHALL have a tab over the content. A file only passed over
SHALL take the place of the previous such tab. A changed file SHALL keep
its tab and mark it unsaved. Closing a tab with unsaved changes SHALL ask
once, and asking again SHALL discard them without writing.

#### Scenario: Closing a changed file's tab
- **WHEN** the viewer closes the tab of a file with unsaved changes, then closes it again
- **THEN** the first close only says so, and the second removes the tab and leaves the file on disk unchanged

### Requirement: The search tab searches every file
The search SHALL be a tab with a field over its results, and search every file
of the checkout that Git does not ignore as the text is typed, ignoring case,
off the drawing thread. An answer to text the field no longer holds SHALL
be dropped. Results SHALL be grouped by file with a count, and each match
SHALL be set apart. Opening a result SHALL show its file with the caret on the
match. The file tree SHALL carry no field.

#### Scenario: Searching and opening a result
- **WHEN** the viewer types `fog` into the field and presses Enter
- **THEN** the files with `fog` are listed with their lines, and the first line opens with the caret on the match

### Requirement: A search is kept until it is cleared
Changing tab or agent SHALL keep the search's text and cursor, and coming back
to it SHALL ask again what the files hold now. Esc SHALL go from the field to
the results and from the results to the tree, emptying nothing; only the
field's clear mark SHALL empty it, leaving the keys in the field.

#### Scenario: Going to the tree and back
- **WHEN** the viewer searches for `fog`, moves to a result, goes to the files tab and returns to the search tab
- **THEN** the field still holds `fog`, the results say what the files hold now, and the cursor is on the same result while it is found

#### Scenario: Esc a level at a time
- **WHEN** the viewer presses Esc in a field holding `fog`, then again on the results
- **THEN** the keys go to the results and then to the tree, opened down to the selection, and the field still holds `fog`

### Requirement: The code surface navigates and edits, and does not review
The code surface SHALL offer files, search and map. It SHALL NOT offer a
changes mode or a diff. A path another surface hands it SHALL open in files,
at the line given when one is.

#### Scenario: The modes on offer
- **WHEN** the operator opens the code surface
- **THEN** its modes are `files`, `search` and `map`, and no `changes`

#### Scenario: Opened from the review
- **WHEN** the review hands it `naming.rs` at line 53
- **THEN** the code surface shows `naming.rs`'s contents with line 53 in view

#### Scenario: A pane path with a line
- **WHEN** the operator ctrl+clicks `src/main.rs:42` in an agent's pane
- **THEN** the code surface shows `src/main.rs` with line 42 in view
