---
trigger: glob
globs: "**/web/content/**"
---

The documentation site is written for the person using uze, in English.

- "uze" is lowercase, and no sentence or paragraph opens on it: a page
  starting "uze puts…" reads as a typo. Reword so another word leads
  ("Installing a plugin puts…", "It is not…").
- Short, plain sentences in the second person, with no marketing. Keys,
  flags, slots and variables go in tables.
- A page states only what the current code does: a flag, key, default or
  interval is checked against the source, never copied from a commit
  message or a proposal, which drift.
- A new page is listed in its folder's `meta.json`, links to the pages it
  leans on, and ends with `<Cards>` as its siblings do.
- A block marked as generated (the harness matrix in
  `reference/harnesses.mdx`) is regenerated, never edited by hand.
- What the site says about security describes defences, never an attack.
- `make web-check` builds the site as CI does; `make web` serves it with
  live reload.
