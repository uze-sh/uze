## MODIFIED Requirements

### Requirement: The surface is reachable the way the code surface is
The surface SHALL open by a `toggle-architect` action bound in the
workspace and in the code surface, and by a chip in the tab strip; the
same action SHALL close it. While it is open its scope SHALL seal the
keyboard from the pane underneath. From it, the review's door SHALL open
the review and the files entry point SHALL open the code surface directly.
Every key the surface answers to SHALL be a named, rebindable action listed
in the action index, and the surface's footer SHALL advertise keys read
from the keymap.

#### Scenario: Opening from the tab strip
- **WHEN** the architect chip in the tab strip is clicked
- **THEN** the surface SHALL open over the workspace

#### Scenario: A key typed while the surface is open
- **WHEN** a letter bound to nothing in the architect scope is typed
- **THEN** it SHALL NOT reach the pane underneath

#### Scenario: A rebound key
- **WHEN** the user's keymap binds `choose-artifact` to another chord
- **THEN** the footer SHALL show that chord

#### Scenario: From the architect to the review
- **WHEN** the review's door is pressed while the architect is open
- **THEN** the review SHALL open on the checkout in front
