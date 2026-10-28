## 1. Vocabulary

- [x] 1.1 `uze_contract::icons`: the names, the loose-glyph test, each set's rule for a declared glyph
- [x] 1.2 `icon.*` symbols in the default, `nerd` and `ascii` sets; `GlyphSet` and `Theme::glyphs`; the theme schema

## 2. Contract

- [x] 2.1 `NodeName::Icon` with `role`; `IconDecl` and `Manifest::icons`
- [x] 2.2 The manifest decodes `icon`, refusing a missing or ill-formed spelling, a host name and a repeat
- [x] 2.3 The view compiler refuses a loose glyph in text and in a lookup table; a literal `Icon` name must be known

## 3. Drawing

- [x] 3.1 The binder resolves a name to the host's or a declared icon; the tree draws it in the set in force, in its role

## 4. Proof and documentation

- [x] 4.1 Tests: declaration rules, loose glyphs, unknown names, one icon in each set
- [x] 4.2 The gsd fixture's branch line uses `Icon "git-branch"`
- [x] 4.3 `docs/extensions/` (an Icons section), the `uze:author` extension reference
