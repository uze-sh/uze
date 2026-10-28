## Purpose

Lets a declared extension compose free-form areas (a direction, wrapping,
sizes, padding, a ground and a border) from theme roles only, so an author
can build visuals the fixed nodes cannot draw.

## ADDED Requirements

### Requirement: A Box lays out its children as a flex container
A `Box` SHALL place its children in its `direction`, `column` by default or
`row`, `gap` cells apart, moving a `row`'s children onto a next line when
`wrap` is set and they do not fit. A child's `width` SHALL be a count of
cells or a percentage of the Box's inner width, and `grow` SHALL share what
is left of a line. `padding`, `padding-x` and `padding-y` SHALL keep cells
free inside its edge, and `height` SHALL fix its rows.

#### Scenario: Five day cards share a row
- **WHEN** a `Box direction="row" wrap=#true` holds seven children of `width="20%"`
- **THEN** five are drawn on the first line, each a fifth of the inner width, and two on the next

### Requirement: A Box takes a ground and a border in roles only
A `Box` SHALL take its `ground` and its `border-role` as roles, never as a
colour value, and `tone="soft"` SHALL draw the ground as the role's hue
lightened toward the surface. Text drawn inside a ground SHALL use an ink
the host chooses to read on it. A `border` SHALL be `line`, `round` or
`heavy`, drawn by the host inside the Box's area.

#### Scenario: A colour value is refused
- **WHEN** a view writes `Box ground="#5b5bf7"`
- **THEN** the check refuses it, naming the roles a ground may be

#### Scenario: Text stays readable on a ground
- **WHEN** a `Box ground="info"` holds a `Text`
- **THEN** the text is drawn in the ink that contrasts with the info hue of the theme in use

### Requirement: Progress can be drawn as a line
`Progress style="line"` SHALL draw a one-row line whose filled part is in
its role and whose track is in a quiet role, so a full bar still reads as a
measure.

#### Scenario: A full line still reads as a bar
- **WHEN** `Progress ratio=1 style="line"` is drawn
- **THEN** the row is a thin line in the role, not a solid block
