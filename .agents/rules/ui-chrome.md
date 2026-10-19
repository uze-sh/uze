---
trigger: glob
globs: "**/src/ui/**, **/src/ui.rs"
---

Chrome comes from `src/ui/widget/` and colour from `theme`; neither is
assembled at the call site.

- `src/ui/widget/` is the chrome vocabulary: `Surface`, `Rule`, `Button`,
  `Status`, `Field`, `Toast`, `tabs`, `drawer`, `dialog`, `modal`, `menu`,
  `grid`, `message` and the rest. Each module's own doc says what it is
  and why it joined; read the directory rather than trusting a list.
  Nothing in it names anything but `theme` and its own siblings. Where
  `theme` settles what a drawn thing may *look* like, this settles what it
  is *made of*.
- **Nothing outside `src/ui/widget/` builds chrome from ratatui's
  primitives.** `chrome_is_built_from_the_widget_vocabulary` in
  `tests/architecture/layering.rs` fails the build over a
  `Block::default()` anywhere else. Name what it is (`Surface::floating()`,
  `Surface::card()`, `Rule::new(Edge::Right)`) and let the widget decide the
  hairline, the ground and the inset; add a constructor when none of the
  existing ones says what yours means, and never rebuild one at the call
  site.
- A widget owns no hit-testing (a caller passes its `Hit` in and registers
  the rects it gets back), no state (ratatui is immediate mode) and no
  layout (it fills the rect it is handed).
  `a_widget_knows_no_model_and_reaches_nothing` fails the build on a widget
  naming `TuiModel`, `WorkspaceModel`, the filesystem, the environment or a
  process.
- The vocabulary lives here rather than in a crate because `ratatui`
  appears in exactly one manifest, the root's.
- Extract into `widget/` on the second *file*, not the second call: a
  helper three callers in one screen share is that screen's. A primitive a
  screen exports is the signal (`clip_line` lived in `management.rs` and
  four other screens reached across for it).
- Colour is a `uze_theme::Token` and a glyph a `Symbol`, adapted in
  `src/ui/theme.rs`; a key is a `uze_keys::Action`, adapted in
  `src/ui/keys.rs` (the `theme-and-keys` rule). Colour that comes from
  content (a pane's output, syntax highlighting) goes through
  `theme::content`.
- `Status` lives in `chip.rs`; `src/ui/widget.rs` lists what the
  vocabulary exports.
