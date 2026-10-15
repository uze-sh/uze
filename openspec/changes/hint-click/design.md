## Context

`src/ui/widget/hint.rs` builds a `Line` from `uze_keys::Action`s, resolving
each chord from the live keymap, through four entry points (`line`,
`within`, `named_within`, `entries_within`). It returns no geometry, so a
caller that wanted a target had to compute one: the marketplace header
(`src/ui/view/plugins.rs`) registers its whole one-entry line as
`Hit::OfferedAction` and repaints the spans on `hovered_offer`, and the
management header (`src/ui/management.rs`) does the same for its close with
a danger colour of its own. Callers do not know where their line lands
either: `widget::footer::render` places it after the trailers and clips it,
`widget::screen_header::inline` pins it right, `recent.rs` puts a lead span
in front.

There are two hit worlds and both already have one door for an action:
the management modal's `Hit::OfferedAction(a)` → `TuiModel::act`
(`src/ui/input.rs:107`, via `src/ui/hit.rs:269`), and the workspace's
`WorkspaceHit::QuickAction(a)` → `Session::act`
(`src/ui/orchestrator/session/keys.rs:184`, via
`src/ui/orchestrator/session/mouse.rs:1173`), which is also where a key
lands and which holds the `offers_action` gate, the gesture span and the
routing to the code, spec and architect surfaces.

## Goals / Non-Goals

**Goals:**
- One way to draw a hint, and it is pointable unless it is drawn inside
  scrolled text.
- No caller computes where a key landed or paints a hover.
- A click and a keypress share one path in each world.

**Non-Goals:**
- `dialog::border_hint` (keys a dialog writes into its own border): a
  separate renderer with its own styling, left as it is.
- Any change to `uze-extensions`, the keymap, or what an action does.

## Decisions

**The widget draws and answers with rects, like `button_row`.** A `Hints`
value is built from scopes and entries (`key`, `named`, `run`), told which
action is `hovered`, and rendered into a rect, returning
`Vec<(Rect, Action)>` for the keys it actually drew. Rejected: returning
relative offsets beside a `Line`, which leaves each caller to add an
origin it often does not know (clipped footer, right-pinned trailer, lead
span), and keeping the old entry points as wrappers that drop the
positions, which would leave the non-pointable form one autocomplete away
for the next surface. The widget returns `Action`, not a generic hit type:
`Action` is its own input vocabulary, names no model and keeps
`a_widget_knows_no_model_and_reaches_nothing` true; each caller wraps it in
its own hit.

**`footer::render` and `screen_header::inline` take `Hints`.** They place
the line, so they are the ones that know its origin; they render it and
pass the rects back up. This is what removes the offset arithmetic from
callers rather than moving it.

**One text-only form, named for what it cannot do.** `Hints::line()`
returns a `Line` for hints written inside scrolled text (release notes'
body), documented as not pointable. It is the only way to get a hint
without its targets.

**The widget owns the hover look.** A hovered key's label is drawn in the
accent token (the key is already bold accent). The marketplace offer's
repaint and the management close's danger repaint go; the close now lights
like every other hint. A line that is a question rather than a list (the
prompt-history clearing note in `agent_support.rs`) keeps its warning
colour as a tone the widget is given, not as a repaint of its spans.

**A hint of one key is one target, its name included.** The first cut
made only the key a target, and a click on the name beside it did
nothing, which read as a broken control: the pointer aims at the words as
often as at the key.

**A run is no target.** `↓↑ move`, `→← expand` and `1–5 section` are the
keyboard's way through a list; the pointer walks the same list by clicking
the row, the tab or the screen it wants. A first cut offered each end of a
run as a one-column target, which read as noise rather than as a control.

**Width by display width.** Placement uses `Span::width()`, not
`chars().count()`, so an arrow glyph that is wider than one cell cannot
shift every target after it.

**The workspace gets its own variant, sharing `QuickAction`'s arm.**
`WorkspaceHit::HintAction(Action)` dispatches through the same
`self.act(action, viewport)` arm. It is a separate variant only so that
hovering the sidebar's quick strip does not light the same action in a
footer, and vice versa. A click never goes through `command_for` /
`tell_the_code_surface`, which would skip `act`'s gate and the surfaces'
door toggles.

**A press on a hint key is the host's wherever it is drawn.** The open
surface, the agent drawer and recent each take every press inside their
area before the generic click arm sees it, so a footer key there would
light under the pointer and do nothing when clicked. The press handler
reads `HintAction` first: it is excluded from what counts as the surface,
and the drawer's and recent's own arms perform it through `act`.

**The extension footer's rects come out through `Rendered`.**
`extension_view::render` collects only `ViewHit`s; the footer returns its
`(Rect, Action)` pairs into a new field of `Rendered`, and
`orchestrator/render.rs::render_extension`, the one place extension hits
are already translated into `WorkspaceHit`, wraps them. Hover is fed down
the same way `NavigatorFrame.sliding` is fed from `model.hovered`.

**Hints beneath an open dialog are not registered.** With an overlay open,
`src/ui/hit.rs:118` reads any `OfferedAction` as the dialog's answer, so a
surface skips registering its hint targets while a dialog is open.

## Risks / Trade-offs

- [Every caller migrates in one change] → The order in tasks.md keeps the
  build green per step: the widget lands with `footer`/`screen_header`,
  then each world's callers.
- [The management close loses its danger hover] → Intended: closing is not
  destructive, and one look for every hint is the requirement.
- [A one-column target (`↓`) is small] → It is the key itself, the same
  size the eye already reads; the label stays inert rather than guessing
  which half was meant.
