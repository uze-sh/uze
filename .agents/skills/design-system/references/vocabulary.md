# The vocabulary that implements the design system

What already exists, so a change reaches for it instead of rebuilding it.
Each piece's module doc is the authority on details. This file is the map,
and the design-system skill keeps it current.

## TUI

### Chrome: `src/ui/widget/`

Re-exported in `src/ui/widget.rs`. Nothing outside it builds chrome from
ratatui primitives (`chrome_is_built_from_the_widget_vocabulary`).

| need | reach for |
|---|---|
| a bordered box | `Surface::floating()`, `Surface::card()`, `Surface::raised()` (beside live content, `.held(bool)` lights its edge), `Surface::selectable(RowState)` |
| a ground | `fill`, `root` |
| a hairline | `Rule::new(Edge)`, `Rule::draggable(edge, dragging)` |
| buttons | `Button::new(label, hue)`, `button_row(...)`, which returns rects to register |
| a work state in a header | `Status` (`StatusKind::Report` / `Action{hovered}`), from `chip` |
| a control's skin, resting, hovered or pressed | `chip::ChipState` |
| a text input | `Field::new(text, placeholder).focused()` |
| an outcome | `Toast::new(kind, title, detail)`, `toast::stack` |
| what an empty surface says | `message::render_message`, `message_lines` |
| a grid of cards | `grid::Shape::fitting(...).lay(...)` |
| tabs | `Tab`, `tab_bar` |
| segmented | `Segment`, `segmented`: a few views of one place as spans for a row, the one in front in the accent, the rest lit under the pointer |
| a detail column | `drawer::render`, `drawer::foot` |
| a drawer docked beside a live pane | `drawer::render_docked`, `drawer::docked_width` |
| a question or notice | `dialog::Dialog` (its `table` for what a consent asks for), `render_choice`, `shell` |
| a form: a key and the control that sets its value, a group's heading and air, a list's check mark | `widget::form`: `Layout`, `key` + `Control` (`Stepper`, `Choice`, `Boxed`, `Combo`, `Typing`, `Add`) answering `Part`s, `label`/`indent` for a key of prose, `check`, `heading`, `GROUP_AIR`/`KEY_AIR` |
| names with what each says beside them (a panel's fields, a drawer's grants, a consent's grants) | `definitions::lines` with `Column::Fixed(view::FIELD_KEY_WIDTH)` for a panel's keys or `Column::Fit` for a table of names, one list of lines per entry |
| a list to choose from, answered once | `dialog::shell` with `Scope::Choose` (`orchestrator/extension_offer.rs`) |
| a titled card over a scrim | `modal`, `footer::render` (returns `Drawn{trailers, hints}`), `scrim` |
| what to check, worst first | `alerts` |
| a problem with content still drawn under it | `banner::render` |
| actions on a thing | `menu::render` over `menu::Entry` (`opens` draws the submenu arrow) |
| summary figures | `stat::cards` |
| a list line | `row::RowState::of`, `row::fill`, `push_trailing(_marquee)`, `marquee`, `title_row` |
| text that fits | `text::elide`, `elide_head`, `clip`, `fold`, `columns` |
| a disclosure chevron | `mark::disclosure` |
| a close mark's hue | `mark::close_tone(ChipState)` |
| keys that act here | `hint::Hints`: `.key`, `.named`, `.run`, `.hovered`, `render` (`#[must_use]`) |
| a screen title with an offer | `screen_header::inline`, which returns `offers` to register |
| the action index (F1) | `action_index` |
| a scrollbar | `Scrollbar::render`, `render_on_draggable` |
| spacing | `TRAILING_PAD`, `POPUP_H_PAD`, `POPUP_V_PAD` |

A declared extension's view (`Content::Tree`, `Section::tree`) is drawn by
`src/ui/extension_view/tree.rs`, one renderer per catalog node from the
pieces above: tabs are `render_chips`, list rows take `RowState`'s ground,
a button is `Button`. The host calls `tree::render_attribution` for the
strip naming a declared view, and reserves a declared section's rows with
`tree_section_height`; a section too tall for them folds to whole nodes and
a `MarkMore` row instead of cropping.

A widget holds no state, does no layout and does no hit-testing. The
caller passes `hovered`/`pressed` in and registers the rects it gets back
(`a_widget_knows_no_model_and_reaches_nothing`).

### Hits

The workspace hit type is `WorkspaceHit` (`src/ui/orchestrator.rs`):

- Hints map through `hint_targets` to `HintAction(action)`.
- A click on one goes `session/mouse.rs::click`, then `session/keys.rs::act`,
  the same entry point a key uses.
- The guard for what is being typed in the overview is in
  `session/mouse.rs::press`: a hint, like the form's own targets, leaves the
  edit to the key it performs; any other click runs `finish_editing` first,
  which ends a search keeping its words, drops an empty or picked field,
  keeps a typed one, and saves an instruction outright.
- `model.hovered_hint()` lights the hint under the pointer.
- `model.pointer_on(hit)` answers a `ChipState` for any other control.

The manage hit type is `Hit` (`src/ui/hit.rs`):

- `TuiModel::hint_targets` maps to `Hit::OfferedAction`.
- `model.hovered_offer` lights it.

Overlays put their hits at the front, so they win over what they cover.

### Keys

`crates/uze-keys` defines:

- `Action`, with its label and name.
- `Scope`, the place where keys are live.
- `Keymap`, through `uze_keys::active()`, with `chord_for` and `resolve`.

`src/ui/keys.rs` is the one file that names a `KeyCode`. Never write a chord
as text; a hint reads it from the live keymap. Every action says where its
pointer lands (`tests/architecture/affordances.rs`).

### Colour and glyphs

`crates/uze-theme` defines `Token` (`Surface*`, `Text*`, `Border*`,
`Accent`/`AccentMuted`, `State*` with `StateDiff*` among them, `Heat0`…`Heat4`
(`Token::HEAT`, a sequential ramp of one hue: how much, never what kind),
`Command*`, `Ansi*` for a pane's own output) and `Symbol`.

`src/ui/theme.rs` adapts them for drawing:

- Colour helpers: `fg`, `fg_bold`, `bg`, `on`, `muted`, `lifted`, `softened`,
  `tinted`, `faded`, `scrimmed`.
- Glyphs: `glyph`.
- Content colour: `content`.
- One surface in another bundled theme, the rest of the frame unchanged:
  `drawn_in(name, || ...)` (the inspector's theme tool).

Only the two adapters name a colour value. Only `theme.rs` writes a chrome
glyph.

A declared view's marks have symbols of their own: `ProgressFill` (whose
frames are the steps of a cell filling, not an animation) over
`ProgressTrack`, `PipFilled`/`PipEmpty`, `MarkFacet` for one standing among
several on a row, and `ActivityBusy`/`ActivityIdle`, which never borrow the
agents' `StatusWorking`.

### Tests

| what | where |
|---|---|
| widgets | `src/ui/widget/tests.rs`, `drawn(w, h, ...)` |
| manage | `src/ui/tests/`, `drawn(&model)`, `drawn_at(&model, w, h)`, which answers `(terminal, hits)`: find the rect in `hits` and `model.click(x, y)` it |
| workspace | `src/ui/orchestrator/tests/`, `driven(model, &home)` |

To draw a surface in another theme, wrap the draw in
`uze_theme::with_active(theme, || ...)`: it holds on the test's thread
alone, where `set_active` would repaint every neighbour drawing at the same
moment. `src/ui/extension_view/tree/tests.rs` runs each check in `uze` and
`light` that way.

`Driven` has `.frame()`, `.press(x, y)`, `.mouse(x, y, kind)`,
`.press_key(key_event(chord))` and `.on_a_roomy_terminal()`; the last
frame's targets are `driven.attach.model.hits`.

The pattern for a clickable thing is to drive one model by click and an
identical one by key, then assert the same state. Examples:

- `clicking_a_surface_footer_hint_is_pressing_its_key` (`tests/surfaces.rs`)
- `clicking_an_overview_foot_hint_is_pressing_its_key` (`tests/overview.rs`)

## CLI: `src/progress.rs`

Every style resolves a theme token. The helpers fall into these groups:

| group | helpers |
|---|---|
| text | `title`, `label` (muted), `accent`, `success_text`, `warning_text`, `error_text`, `command_name(text, CommandGroup)` |
| layout | `aligned_rows`, `aligned_rows_wrapped`, `aligned_groups`, `terminal_width` |
| reports | `report_section`, `report_title`, `key_value`, `path`, `count`, `count_packages` |
| changes (bun-style) | `change(Change::{Added, Removed, Updated, Attention, Failed}, subject, detail)` |
| whole report | `change_report(verb, lines, outcome)` |
| marks | `success`, `warn`, `error(msg, hint)`, `next_step`, `success_icon`, `warning_icon`, `error_icon` |
| progress | `spinner`, `follow_steps`, `settle_steps`, and `cli/output.rs::with_spinner` |
| format | `cli/output.rs::emit` (text or JSON) |
| scope line | `cli/install.rs::report_scope` |

Help lives in `src/cli/help.rs`: `print_root_help`, `print_command_help`,
`ROOT_COMMANDS`, `COMMAND_EXAMPLES`.

## Decisions behind it

ADRs:

| ADR | decision |
|---|---|
| 042 | architecture seams are enforced by tests |
| 045 | actions are named once, and the screen only draws them |
| 046 | the design vocabulary is a leaf crate |
| 054 | scope is reported |

OpenSpec specs:

- `action-discovery` ("a hint is the action it names")
- `input-keymap`
- `ui-theme`
- `overview`
- `command-grammar`
