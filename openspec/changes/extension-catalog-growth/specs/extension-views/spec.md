## MODIFIED Requirements

### Requirement: Views are drawn from the catalog in the active theme
A view SHALL be built only from catalog nodes: `Stack`, `Row`, `Text`,
`Badge`, `Marks`/`Mark`, `Pips`, `Progress`/`Segment`, `List`, `Table`,
`Tabs`/`Tab`, `Toggle`, `Button`, `Activity`, `Elapsed`, `Log`, `Rule`,
`Details`/`Entry`, `Markdown`, `Code`, `Gauge`, `Chart`, `Stat`, `Kbd` and
`Grid`. Colour SHALL be named only as a semantic role, and the host SHALL
animate `Activity` and tick `Elapsed` without rebuilding the view.

#### Scenario: A progress bar is segmented
- **WHEN** a `Progress` holds six `Segment`s, each with a weight, a ratio and a role
- **THEN** the bar has one segment per entry, widths proportional to the weights, each filled to its ratio in its role's colour

#### Scenario: A colour value is refused
- **WHEN** a view writes `role="#ff0000"`
- **THEN** the check fails listing the roles a value may name

## ADDED Requirements

### Requirement: Measures draw numbers a fixture holds
A `Gauge` SHALL draw its `ratio` as a meter with its label and percentage,
in the `warning` or `danger` role once the ratio reaches the threshold of
that name. A `Chart` SHALL draw a list of numbers as one `sparkline` row
or as `bar`s `height` rows tall, scaled to the largest. A `Stat` SHALL
draw a figure with its label and, when given, a delta in its role.

#### Scenario: A gauge turns at its threshold
- **WHEN** a `Gauge` has `ratio="{used}"`, `warning=0.7` and `danger=0.9`, and `used` is 0.93
- **THEN** the meter is drawn in the `danger` role with `93%` beside it

#### Scenario: A sparkline scales to its largest value
- **WHEN** a `Chart kind="sparkline"` is bound to `[1, 4, 8]`
- **THEN** one row is drawn whose last cell is the tallest level and whose first is the lowest above empty

### Requirement: Text blocks draw Markdown and highlighted code
A `Markdown` node SHALL draw its text as the code surface draws Markdown:
headings, lists, quotes, emphasis and fenced code, wrapped to its width. A
`Code` node SHALL draw its text highlighted for its `language` in the
active syntax theme, one row per line, cut rather than wrapped.

#### Scenario: A plan reads as prose
- **WHEN** a `Markdown` is bound to `"## Plan\n- read the loop\n- write the tests"`
- **THEN** the heading and two bullets are drawn, each bullet's continuation hung under its text

### Requirement: Structure nodes lay out pairs, tiles and dividers
`Details` SHALL draw its `Entry` children as key/value pairs with the keys
in one aligned column. `Grid` SHALL lay its children out as tiles, in as
many columns as fit `min-width` with `gap` between them, row by row. `Rule`
SHALL draw a hairline across its width, with its label when given. `Kbd`
SHALL draw a key as the workspace draws the keys of its own hints.

#### Scenario: Tiles reflow with the width
- **WHEN** a `Grid min-width=20` holds four tiles and is drawn 44 columns wide, then 90
- **THEN** it draws two rows of two tiles, then one row of four
