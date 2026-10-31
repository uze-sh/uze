## ADDED Requirements

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

### Requirement: The files list searches every file
The files list SHALL keep a search field over its tree and search every file of
the checkout that Git does not ignore as the text is typed, ignoring case,
off the drawing thread. An answer to text the field no longer holds SHALL
be dropped. Results SHALL be grouped by file with a count, and each match
SHALL be set apart, in the tree's place while the field holds text. Opening a
result SHALL show its file with the caret on the match. Letting go of the
search SHALL bring the tree back.

#### Scenario: Searching and opening a result
- **WHEN** the viewer types `fog` into the field and presses Enter
- **THEN** the files with `fog` are listed with their lines, and the first line opens with the caret on the match
