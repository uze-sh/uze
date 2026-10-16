## ADDED Requirements

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

### Requirement: A changed image is drawn in the changes half
When the selected change is a binary file that is one of the drawn image
formats and exists in the working tree, the changes half SHALL draw it as
it stands in the working tree instead of saying Git has no lines to
compare. A deleted image, or one that cannot be decoded, SHALL keep that
message.

#### Scenario: An updated screenshot
- **WHEN** `web/public/poster.png` is modified and selected in the changes
  half
- **THEN** the working tree's version of the image SHALL be drawn under
  its path

#### Scenario: A deleted image
- **WHEN** a tracked `logo.png` is deleted and selected in the changes half
- **THEN** the half SHALL say Git records that it changed, with no image

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
