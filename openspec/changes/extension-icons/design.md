## Context

See proposal.md. Symbols are named glyphs resolved from the active theme;
the glyph sets `nerd` and `ascii` are layers over the default that redraw
them. An extension's text reached the screen as written.

## Goals / Non-Goals

**Goals:** an extension names an icon and never its glyph; every icon reads
in every glyph set; the check stops a glyph that would not.

**Non-Goals:** icons in data (a program's answer is not refused, D5); an
extension choosing a glyph set; colour icons or images (that is `Image`).

## Decisions

- **The vocabulary is the theme's** (D1): `icon.<name>` symbols, so the
  three sets carry every icon and a theme may redraw one.
- **A declared icon carries all three spellings** (D2), and the host draws
  the one of the set in force, read from the theme (`Theme::glyphs`).
- **Loose glyphs are refused, not warned** (D3), by Unicode block.
- **The default set's spelling is text, never emoji** (D4), held by the
  test that holds every bundled glyph.
- **Only what the author wrote is checked** (D5).
- **A name nothing knows draws nothing** (D6).
