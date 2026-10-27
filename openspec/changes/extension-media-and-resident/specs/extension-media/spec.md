## Purpose

What a declared extension may draw beyond text: a grid of cells, an image
and a sprite, all decoded, fitted and written by the host.

## ADDED Requirements

### Requirement: Cells draw a grid in the host's roles, with regions bound to actions
A `Cells` node SHALL draw rows of `{ch, role}` where `role` is one of the
host's roles, SHALL refuse a colour value from a declared extension, and
SHALL let `regions` bind rectangles of the grid to declared actions with an
`item`, as a button binds.

#### Scenario: A region runs its action
- **WHEN** the operator clicks a cell inside a region bound to `eat` with item `{x: 3}`
- **THEN** the `eat` action runs with that item

### Requirement: An image comes from the extension's files or its program's scratch, bounded
An `Image` node SHALL read only a file inside the extension's directory or
one its program wrote into its run's scratch, SHALL refuse a file over 8 MiB
or a decode past 32 MiB of pixels per extension naming the bound, SHALL
decode off the drawing thread one at a time per extension, and SHALL draw
blocks where the terminal shows no images.

#### Scenario: An oversized image is refused
- **WHEN** an extension names a 20 MiB PNG
- **THEN** the surface says the image is over 8 MiB, and nothing is decoded

### Requirement: Several slots may each hold an image in one frame
The host SHALL keep one image placement per slot, so a band, a section and
an overlay may each show an image at once, each fitted to its slot.

#### Scenario: Two images at once
- **WHEN** `screen`'s section and `pets`' band each show an image
- **THEN** both are drawn in the same frame, each inside its own slot

### Requirement: A sprite plays a state chosen by a field
A manifest SHALL declare a sprite's sheet, frame size, columns and states
(frames, rate, loop) with a fallback state, checked against the sheet; a
`Sprite` node SHALL play the state its field names, or the fallback, on the
host's clock with no run behind it.

#### Scenario: An agent's state picks the animation
- **WHEN** `pets`' band binds its sprite to the agent's `state` and the agent starts working
- **THEN** the `working` frames play, and the `idle` ones once it stops
