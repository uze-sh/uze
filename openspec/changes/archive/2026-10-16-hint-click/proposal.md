## Why

Every surface ends in a row of hints (`↓ next · d delete · m map · esc
close`) that names what can be done here, and nearly all of it is
decoration: the pointer passes over a key it could have pressed for you.
One hint, the marketplace header's `a add`, already answers the pointer,
painted and registered by hand at its call site, which is the shape a
second one would copy and the reason no second one exists.

## What Changes

- Every hint a surface draws becomes a pointer target: hovering a key
  lights it, clicking it performs the action exactly as pressing it does.
- A hint of one key is a target over its key and its name. A run of keys
  written as one hint (`↓↑ move`, `1–5 section`) is not a target: the
  pointer walks those lists by pointing at what it wants.
- Hints drawn beneath an open dialog are not targets, so a click there can
  never be read as the dialog's answer.
- The hint widget draws into the rect it is given and hands back where each
  key landed, the way a row of buttons does. Its four line-building entry
  points go; one text-only form stays for hints drawn inside scrolled text,
  and says that it cannot be pointed at.
- The hover look of a hint is the widget's, not the caller's: the two
  hand-painted hovers (the marketplace header's offer, the management
  header's close) are removed, and every hint lights the same way.
- Out of scope: the keys a dialog writes into its own border.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `action-discovery`: a new requirement, *a hint is the action it names*,
  extending "every chord has a pointer affordance" to the hint line itself.

## Impact

- `src/ui/widget/hint.rs` (the widget), `src/ui/widget/footer.rs` and
  `src/ui/widget/screen_header.rs` (take hints rather than a line).
- Management modal callers: `src/ui/management.rs`, `src/ui/view/plugins.rs`,
  `src/ui/release_notes.rs`; dispatch through the existing
  `Hit::OfferedAction`.
- Workspace: a hit variant for a hint's action in `src/ui/orchestrator.rs`,
  the extension footer (`src/ui/extension_view/footer.rs`,
  `src/ui/extension_view.rs`), its translation in
  `src/ui/orchestrator/render.rs`, `src/ui/orchestrator/render/recent.rs`,
  `src/ui/agent_support.rs`, and the click arm in
  `src/ui/orchestrator/session/mouse.rs`.
- `uze-extensions`, the keymap and every domain crate are unchanged.
