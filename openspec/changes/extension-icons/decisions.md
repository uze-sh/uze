# Decisions — extension-icons

## D1 — The vocabulary is the theme's `icon.*` symbols

- status:   pending
- task:     1.2
- commit:
- trigger:  The host already resolves every glyph it draws from the active theme, and the glyph sets are layers over it.
- decided:  49 `Symbol`s named `icon.<name>`, each with a glyph in the default (Unicode text), `nerd` (a Font Awesome or Octicons codepoint of Nerd Fonts) and `ascii` (one or two characters) files. `uze_contract::icons::ICONS` lists the names an extension may use; `the_ascii_set_declares_every_symbol_and_every_one_is_ascii` and `every_nerd_glyph_is_an_icon_a_patched_font_supplies` hold the two sets.
- rejected: A table inside `uze-extensions`: a second place glyphs are chosen, which a theme could not redraw.
- blast:    `uze-theme/src/symbol.rs`, `themes/{dark,nerd,ascii}.json`, `uze-contract/src/icons.rs`.

## D2 — A declared icon carries all three spellings; the theme says which set it is

- status:   pending
- task:     1.2, 2.2
- commit:
- trigger:  A declared icon is the author's, not the theme's, so the host must pick its spelling without a symbol to resolve.
- decided:  `icon "<name>" nerd= unicode= ascii=`, all required. A theme file gains `glyphs` (`nerd` or `ascii`; the bundled sets say it), resolved top layer down into `Theme::glyphs`, default `unicode`. The host draws that spelling; ASCII takes its length in cells, Nerd one cell.
- rejected: Only `nerd` with a fallback: an ASCII operator would see a host-chosen stand-in for the author's idea.
- blast:    `uze-theme/src/file.rs::GlyphSet`, `load.rs`, `manifest/icon.rs`, `src/ui/theme.rs::icon`.

## D3 — A glyph written loose is refused, not warned

- status:   pending
- task:     2.3
- commit:
- trigger:  The operator asked to refuse it: a warning an author can ignore still ships a `⏱` an ASCII terminal cannot draw.
- decided:  The compiler refuses a literal in a view's text or a lookup entry holding a character of U+E000–F8FF, the supplementary private use planes, U+2300–23FF, U+2600–27BF, U+2B00–2BFF or U+1F000–1FAFF, naming the character and suggesting `Icon`. Arrows, box drawing, block elements and geometric shapes (`▶ ● ━ ▁`) are text and pass: the bundled default draws the same.
- rejected: Refusing every non-ASCII character: `·`, `…`, `→` and accented words are text an author needs.
- blast:    `view.rs::loose_glyph`, `uze_contract::icons::is_icon_glyph`.

## D4 — The default set's icons are text, never emoji

- status:   pending
- task:     1.2
- commit:
- trigger:  `no_bundled_glyph_is_an_emoji` failed on the first draft (`⚠`, `🔒`, `🚀`).
- decided:  Each default glyph is one cell of text: `▲` for warning, `⊠` for lock, `↟` for rocket. A declared icon's `unicode` is held to the same rule.
- rejected: Exempting icons from the test: an emoji's width differs by terminal and breaks the layout.
- blast:    `themes/dark.json`, `icons.rs::unicode_glyph_problem`.

## D5 — Only what the author wrote is checked

- status:   pending
- task:     2.3
- commit:
- trigger:  A program's answer, a file's content or an agent's output may hold any character.
- decided:  The check reads the view's literals and lookup entries; data bound at run time is drawn as it comes.
- rejected: Scrubbing data: it would change what a commit message or a file name says.
- blast:    none beyond the compiler.

## D6 — A name nothing knows draws nothing

- status:   pending
- task:     3.1
- commit:
- trigger:  `Icon "{state}"` names an icon by data, which the check cannot see.
- decided:  A literal name is refused when neither the host nor the manifest holds it; a name the data gives that neither holds draws no cell (the row keeps its gap, as for an empty `Text`).
- rejected: A placeholder glyph: it would draw something the author never chose.
- blast:    `bind.rs::icon`, `IconGlyph::Unknown`.
