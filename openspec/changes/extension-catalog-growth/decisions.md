## D1 — Draw a Rule with the theme's existing divider

- status:   pending
- task:     3.2
- commit:
- trigger:  G3 named a `Symbol::Rule`; the theme already has `tree.divider`, "a horizontal rule", which every theme sets.
- decided:  `Rule` draws `Symbol::TreeDivider` in the faint role, its label in its own role between two runs of it. Only `chart.level` is new.
- rejected: A second symbol that every theme would set to the same glyph.
- blast:    One symbol and every bundled theme, if a rule should ever look unlike a divider.

## D2 — Gauges stacked together share their label column

- status:   pending
- task:     3.2
- commit:
- trigger:  Three gauges labelled Opus, Sonnet and Haiku started their bars at three different columns, which reads as three unrelated meters.
- decided:  The gauges of one `Stack` draw their labels in a column as wide as the widest of them, so their bars start together; a gauge alone keeps its own width.
- rejected: A fixed label width (cuts long labels, wastes room for short ones); asking authors for `min-width` on each (the common case would need it every time).
- blast:    The tree painter's `gauge_labels`; a gauge's snapshot moves if it is regrouped.

## D3 — Lay Code out unbounded, then cut it, and drop a block's trailing blank row

- status:   pending
- task:     3.4
- commit:
- trigger:  G2 said `Code` is drawn through the code surface's Markdown at the node's width; that rendering wraps long lines and leaves the blank row it puts after every block.
- decided:  `Code` is laid out at a measure of 1024 columns and each line cut at the node's width; both `Markdown` and `Code` drop trailing blank rows. Folded blocks are cached per text, syntax theme and width (64 entries, dropped whole when full).
- rejected: Wrapping code (a wrapped line reads as two); a separate highlighter path (two renderings of one language to keep alike).
- blast:    `tree/display.rs`'s `folded`.
