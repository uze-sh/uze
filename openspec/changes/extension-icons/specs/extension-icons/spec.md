## Purpose

Lets an extension show an icon that reads in whatever glyph set the
operator chose, by naming it, and keeps glyphs a terminal may not draw out
of what an author writes.

## ADDED Requirements

### Requirement: An extension names an icon and the host draws it in the operator's glyph set
An `Icon` node SHALL name one of the host's icons or one its manifest
declares, and the host SHALL draw it in the glyph set in force, in the role
it gives. A name neither holds SHALL draw nothing; one written as a literal
SHALL be refused by the check.

#### Scenario: The same icon in three glyph sets
- **WHEN** a view declares `Icon "clock" role="info"`
- **THEN** it is drawn `◷` by default, the Nerd Font clock under `nerd`, and `t` under `ascii`

### Requirement: A declared icon is spelled for every glyph set
A manifest's `icon` SHALL give `nerd` (one character of a Nerd Font's
private use area), `unicode` (one character drawn as text, not emoji) and
`ascii` (one or two printable ASCII characters). A missing or ill-formed
one, a name of the host's, or a name declared twice SHALL be refused.

#### Scenario: An icon the host lacks
- **WHEN** a manifest declares `icon "kennel" nerd="\u{f0a43}" unicode="⌂" ascii="K"`
- **THEN** `Icon "kennel"` draws `K` for an operator who chose ASCII

### Requirement: A glyph written loose is refused
The check SHALL refuse a view's text or a lookup table entry holding a
character of a Nerd Font's private use areas, or of the Miscellaneous
Technical, Miscellaneous Symbols, Dingbats, Miscellaneous Symbols and
Arrows or emoji blocks, naming the character and suggesting `Icon`.
Arrows, box drawing, geometric marks and text SHALL pass, and data a
source answers SHALL NOT be checked.

#### Scenario: A stopwatch typed into a bar item
- **WHEN** a view declares `Text "⏱ {total}"`
- **THEN** the check refuses it naming U+23F1 and suggests `Icon "clock"`
