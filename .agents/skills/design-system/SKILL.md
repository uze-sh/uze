---
name: design-system
description: The design system for everything UZE draws or prints - the workspace TUI (src/ui/), the CLI's output and help (src/progress.rs, src/cli/), and the words in both. Use before changing anything a person sees - a screen, a widget, a hint bar, a dialog, a toast, a drawer, an empty state, a command's report, help text, an error message, a label - and when reviewing such a change, when the operator reports something visually off, inconsistent, misaligned, not clickable or too verbose, or says "padrão", "inconsistente", "desalinhado", "já pedi isso". Covers the operator's standing rules (each one asked for more than once), the vocabulary that already implements them, the gestures every surface must answer, and the check before handing back. Also says how to keep this skill current.
---

# UZE's design system

Every rule here was asked for by the operator, most of them more than once.
A change that breaks one comes back as a correction, so check against the
rules **before** handing back, not after.

The rules are in two places, and they do not overlap:

- **The code enforces the structure.** Chrome comes from `src/ui/widget/`,
  colour from `theme` tokens, glyphs from `Symbol`s, keys from the keymap.
  `tests/architecture/layering.rs` and `affordances.rs` fail the build when
  that is broken. [vocabulary.md](references/vocabulary.md) lists what exists and
  where it lives.
- **This file holds the judgement.** It covers what the tests cannot see:
  how a surface should behave, how it should look and what it should say.

## Before you touch a screen or an output

1. Find the nearest surface that already does what you need: another
   overview section, another drawer, another manage screen, another
   command's report. Copy its structure. "No padrão das outras views" is the
   operator's most common correction.
2. If the vocabulary has no widget or constructor for your case, add one to
   `src/ui/widget/` (or a helper to `src/progress.rs`). Never build it at the
   call site.
3. Read the sections below that apply to your change. When the rule you need
   isn't here, ask the operator once, then write the answer down here.

## Interaction (TUI)

**A hint is the action it names.** Every key a hint bar, footer, header or
question shows is clickable and does exactly what the key does, in the scope
that is live at that moment. That includes while typing or filtering: a click
on "cancel" must not save first.

- `Hints::render` is `#[must_use]`, so register every target it returns as
  a hit. In the workspace that is `WorkspaceHit::HintAction(action)`,
  pushed directly or through `hint_targets` in `orchestrator.rs`; in manage
  it is `TuiModel::hint_targets`. `let _ =` is
  right only when a target already registered covers the same rect (the
  manage modal's close mark, Recents' show-all toggle), and the comment
  above it says which.
- `Hints::line()` (text with no target) is only for hints inside scrolling
  prose.
- A `run` (↑↓, 1–9) is read, never clicked.
- A dialog's bottom border is a hint bar too: draw it with
  `dialog::BorderHint` and register its `targets` along the popup.
- Prove a new hint bar with a click == key test. See
  `clicking_an_overview_foot_hint_is_pressing_its_key` and
  `clicking_a_surface_footer_hint_is_pressing_its_key`.

**Clicks:**
- The first click acts. Opening or closing a popover, menu or Recents must
  never swallow the next click.
- The whole row, chip, card or button is the target, not just the arrow or
  one word. The one exception is a list of choices (rows marked `◉`/`○`):
  a click on the row marks it, and only a click on its mark switches it,
  as space does from the keyboard.
- Never leave a dead region, and never open anything on a double click:
  Enter opens.
- A click that does nothing must not *look* like it acted. For example, a
  drawer must not close as if it ran something.
- Nor may a button offered as available answer a click with nothing. When
  what it would do cannot happen here, it is offered as unavailable and the
  place it is drawn says why and what to do instead. For example, an
  extension's Enable with no project in front says "Needs a project" and
  never quietly flips a switch that is already on.

**Moving between agents:**
- A surface belongs to the agent it was opened on: its own tab and the
  shells beside it are one place, as the sidebar marks the agent whichever
  of them is in front. Going to another agent, however it got there (a
  click, a key, another space's header, an extension asking for an agent),
  puts it away for that agent, and the agent coming back, on whichever of
  its tabs, has it standing again *as it was left*: the same half, the
  same selection, the same scroll, whatever the extension. A return is not
  a door; a door still decides the half on a fresh opening.
- The space overview opened over a surface puts it away the same way, and
  putting the overview away gives it back. A reattach does too.
- Only choosing the tab or space already in front, or another terminal of
  the agent in front (its tab from its shell, a shell from its tab), takes
  the pane back from it, and forgets it: an agent never keeps a surface
  waiting on one of its tabs while showing another. A shell is a terminal
  wherever it is chosen, the overview put away first. A shell the
  overview's own "+" opens is not chosen: the overview follows the agent
  to it, and putting it away gives back what it put away.
  `each_agent_keeps_its_own_surface`,
  `an_agent_and_its_shell_never_keep_two_surfaces`,
  `clicking_the_agents_shell_under_the_overview_shows_the_shell`,
  `moving_to_the_agents_shell_under_the_overview_keeps_the_surface` and
  `returning_to_an_agent_shows_code_on_the_half_it_was_left_on` hold it.

**Hover:**
- Everything clickable has a hover, and nothing that isn't clickable does.
- Hover is a change of colour only (a "realce"): never underline, an arrow
  or a background overlay.
- Hover keeps the word's weight. A word that is bold at rest stays bold; a
  word that is not never turns bold under the pointer or while pressed.
  Manage's version and health are the reference.
- Text-only affordances start muted and brighten.
- A plugin's surface has at most one button in the accent, the one it
  means to be pressed next (`Button primary=#true`), as UZE's own buttons
  are for what builds; every other button stays neutral.
- What a plugin draws follows the same rule, without the plugin asking:
  every element of the catalog that answers a click lifts under the
  pointer in its own hue (`theme::lifted_ink`, `widget::lift`), and a
  whole region that answers one, such as a bar item that opens a drawer,
  rests held back like the bar's own doors (`widget::hold_back`). A
  coloured mark never turns accent on hover: it is a stronger version of
  itself.
- Every close `x` is faint at rest and turns to the danger hue under the
  pointer and while pressed: use `mark::close_tone(state)`, never a tone
  of your own.
- Anything that reveals itself on hover needs a hint saying what it means.

**Wheel:**
- The wheel scrolls the view, never the selection, and never toggles
  anything.
- The keyboard keeps its own selection model.

**Scrollbars:**
- Every list that can overflow has the standard `Scrollbar`, which can be
  dragged.
- It sits **on** the separator or divider, not in a column of its own.
- A click on the bar never falls through to the list.

**Resize:**
- Every split and drawer is resizable, in the same way everywhere.
- The handle lights while it is dragged.
- Drawers on manage's screens share one width.
- Stacked sections push and pull each other.

**Popovers:**
- A popover opens anchored to its trigger, whatever opened it (click,
  shortcut, resume).
- A context menu opens at the cursor.
- A menu entry that opens a further menu ends in an arrow against the
  menu's right edge (`Symbol::Submenu`, `menu::Entry { opens: true }`) and
  carries no ellipsis: `Rename ›`, `Continue in ›`. An ellipsis marks only
  an entry that asks before acting (`Discard…`). Two entries for one verb
  side by side (`Rename`, `Rename project`) become one that opens both.
- A popover that holds more than one view of what it asks (the new
  agent's `new agent · overrides`) names them at the start of its first
  row (`widget::segmented`), standing for a title the popover then does
  not carry, with the row's field (`‹ Claude Code ›`, unlabelled) against
  its end; never on the border, where they read as part of what is
  behind. The views are parted by the separator glyph alone, the one in
  front in the accent and the rest held back and lit under the pointer:
  colour alone tells them apart, never a ground, a weight or a button's
  look. A click on one is the key that reaches it, through a target of
  its own (`WorkspaceHit::ViewAction`), so pointing at a view lights that
  view and never the foot's hint for the same key. The row is drawn
  alike on every view so nothing shifts when they switch, and its field
  is changeable on every view (what a view holds follows it); a rule
  closes it, then the view in front, every view in the same rows, so
  switching never resizes it. What a view holds that the other should
  know is one muted line at the body's foot, with a row of air over and
  under it so it rests against neither the list nor the keys (`3 flags ·
  2 vars`), said only once there is something to say: with none, the
  rows stay air. What the row marked or the word typed is (its summary,
  what it gives up, why it was refused) takes that same line while there
  is one, by the keys, never a row of its own in the middle of the list's
  room.
- A click on a popover's line typed into puts the keys back on it:
  nothing stays marked in the list under it.
- Every line typed into in one popover looks the same: the typed text
  bright, the accent caret, a faint placeholder and a plain rule under
  it. A view's line never wears a lit underline the other's does not.
- A popover is narrow and tall rather than wide: as wide as a
  directory's name and what is known about it need (52 columns for the
  new agent), with the room for its list given in height.
- Dropdowns float over content and never push it down.
- A breadcrumb names levels, short and generic (`Context › Containers ›
  Components ▾`), the step on show included. An item's own long name goes
  in the list that step opens and, muted and in its own case, on the row
  of the content's modes (`View::label`), never in the step itself.
- A modal keeps its height while you filter inside it.
- A popover with steps fixes its height when it opens, as the largest any
  of its steps needs: going into a step and back never resizes it.
- A popover's rows stand on its own ground: no raised band under the line
  typed into or under its list. The row Enter takes is the one marked.
- A list under a line typed into opens with nothing marked, so Enter acts
  on the line; `↓` marks the first row, and `↑` on it unmarks again.
  Hovering a row only lights it: the pointer resting there never changes
  what Enter does.
- A choice that rarely changes is a field prefilled with the last answer
  (`‹ Claude Code ›`, moved with `←` `→`), never a step of its own:
  the common case is the one question and Enter. Its arrows are its keys.
- A field whose wrong value is unsafe, or invisible once the thing has
  started (a new agent's launch overrides), is never prefilled from last
  time: it opens empty, and the last values head a history under it, to
  be taken back on purpose.
- That history is a list of choices, one row per value, never per
  bundle of them: the overrides list holds each flag or variable once
  (`◉ --model opus ✕`), the ones added first, then the ones used lately,
  `◉` when this launch carries it and `○` when not. Unchecking never
  takes a row off the list; only its close mark against the end does,
  leaving it out and forgetting one used before. Space
  switches the row marked (and is typed when none is), Delete is the
  close mark, Enter still answers the whole question (`new`), and the
  foot names space and Delete only while a row is marked (`space toggle
  · delete forget`, `remove` for one only added).
- A popover's foot names its keys and nothing else: the keys that act on
  what is in front at its start, two of them where there are three, and
  one against its end, the way out of it: `esc close`, or, in a popover
  with views, the way to the other view (`enter new · ←→ harness` …
  `alt+→ overrides`). What stands against the end gives way, whole, when
  the keys at the start need its room.
- A popover's views are walked with `alt` and the sideways arrows
  (`alt+→`, `alt+←`), as `alt+↑`/`alt+↓` walk the agents: the bare arrows
  keep their job in the view, and a key drawn with a modifier spells its
  arrow as a glyph (`alt+→`). A key whose job
  changes with the state is named by what it does now, from the same
  answer the key acts on (the overrides step's Enter: `new` on an empty
  line, `take` with a completion marked, `add` otherwise).
- `Tab` in a line typed into completes, as it does in a shell; it never
  takes another job there. What Enter acts on is already on screen, in the
  field and the row marked; a sentence repeating it crowds the keys out.
- A popover is as wide as its rows need and no wider.
- A listing row says what is known about its entry against its end
  (`space · main`, `git`), never in a column of its own.
- An agent's caption row leads with its project, by the operator's label
  for it when there is one, in every space, in the
  column's quiet hue, and ends with its harness in the
  trailing column. Only the harness takes the row's lit caption token
  (`caption_color`), never a fixed colour and never the whole line.
  The harness is drawn only in the space in front; a space behind says
  where its agents work and nothing more.
- A name too long for its row is cut at its end (`uze-audit-20…`), on
  every row of a list, the selected one included; a path is cut at its head
  (`…/projects/uze`). Never in the middle.
- A click anywhere inside a popover keeps it open; only a click outside it
  dismisses it.

**Closing:**
- Esc closes everything, and a modal or a drawer says so nowhere: its
  closing is a bare `x` at the top right, with no label, no footer "close"
  button and no "esc" hint.
- A step opened inside a dialog (everything a row asks for, in the list's
  place) goes back before it closes: its heading starts with `‹`
  (`Symbol::StepPrevious`) and is a target, and the bottom border names
  esc "back". Esc on the first step closes.
- A surface's own foot (the overview's) is different: it is the keys that
  act there, so it names Dismiss by what it does in that state, "back",
  "cancel", "clear" or "close", and that word is a click target like any
  other.
- A drawer closes only on a click outside it. The exception is a plugin's
  `drawer` slot beside a live pane, which is not modal: a click outside
  gives the pane its keys back and leaves it open; its bar item, its key
  or Esc closes it. Esc never leaks to the agent behind a drawer it
  belongs to: a floating drawer (over the pane) closes on Esc wherever the
  keys are, and a docked one closes on Esc while it holds them. Only
  beside a docked drawer the pane holds the keys of is Esc the agent's,
  which interrupts its turn with it.

**Shortcuts:**
- Keep them few: group actions behind a context menu (`alt+.`) rather than
  adding specific keys.
- One modifier per family: extensions all use `alt`. But a global chord an
  extension suggests competes with the system, GPU overlays (NVIDIA takes
  `alt+z`, `alt+r`, `alt+f1`, `alt+f9`, `alt+f10` before a terminal sees
  them) and the harness in the pane (Claude Code pastes an image on
  `alt+v`): an example opens what it shows from a control the person can
  see, and leaves the global key for the person to bind on the Keys
  screen.
- Keyboard users must never lose a path.

**What is UZE's and what is a plugin's:**
- The tab strip holds UZE's own surfaces only. Everything a plugin adds
  (the doors to its views and drawers, the items it draws) stands in the
  bar under the agent area, the tab strip's mirror at the content column's
  foot, which takes its rows only while a plugin puts something there.
- A plugin's list in a sidebar section reads as the sidebar's own rows: no
  ground on hover or selection, its text held back at rest and lit in its
  own hues under the pointer or once chosen, as the tasks section is.
- A plugin's section in the sidebar is drawn flat, its header its own
  title, as the sidebar's own sections are: no strip naming its extension
  and plugin above it. A band has no mark naming it either: a muted name
  at its end reads as a button. Whose it is belongs to the Extensions screen and the
  inspector; a clash between extensions is resolved by id, never shown. A
  view, a drawer and an overlay are headed by the title their extension
  gave them, muted, on no strip of its own: never the extension's id, its
  plugin or its marketplace, which belong to the Extensions screen, a
  consent and a question about what it would do to an agent.
- A plugin has no standing line to say it has nothing to say: no "no
  notice yet" row at the top of the sidebar. What it notifies is drawn in
  the sidebar's notice, where a new release is, one notice at a time with
  its mark to put it away, ahead of the release's; a status row that has
  nothing to show takes no row.
- A surface whose extension is withheld (it needs consent, enabling here,
  or its update is held) says why in one row inset as a list's rows are,
  with `review` at its far end, and the whole row opens the Extensions
  screen on that extension. A state the person can act on is never a
  bare warning they have to go find the answer to.
- Marketplace says what a package brings; Extensions says what runs.
  Each links to the other and neither repeats the other's actions. A
  package's tree lists the extensions it declares as a kind like any
  other (`extensions 1`, its rows carrying no status, since whether one
  runs is the Extensions screen's to say), and `⊘` stands only under a
  package that brings nothing at all. Enter on one, or its panel's `open
  Extensions`, opens the Extensions screen on it; a declared extension's
  drawer names its plugin on its `from` line, a link that opens the
  Marketplace on that plugin unfolded on the extension. A way across is a
  link and an index entry (`Show plugin`, `Show extension`), never a
  drawer button beside the actions.
- A plugin never writes an icon glyph either: it names one with `Icon`
  and the host draws it in the operator's glyph set, as UZE's own chrome
  names a `Symbol`. The check refuses a Nerd Font codepoint or a
  pictograph written loose in a view (`extension-icons`).

**Confirmation:**
- A destructive action is confirmed in a centred dialog with a scrim and
  Confirm/Cancel buttons, chosen with the arrows and Enter. Never an
  inline question in a foot or a row.
- In the space overview the dialog is its one question
  (`overview_question.rs`): add a case to `Asked` rather than a dialog of
  your own.
- Never ask for Y/N.
- Enter is the primary action.
- Several things waiting on the same answer are asked in one dialog, a
  list to choose from (`Scope::Choose`), never a queue of dialogs; one
  toast says what was done (`3 extensions enabled`), never one each.
- A question is 60 columns at most. A list to choose from takes what its
  rows need, up to 96, its rows take the height left (4 to 14, then it
  scrolls), and the marked row's detail keeps a fixed height, so moving
  the cursor never resizes the dialog.
- Putting a question away answers nothing unless the question says so: an
  offer put away is made again the next time, never refused for good.

**Skip choices with one answer:**
- With one harness, launch it directly.
- Open setup automatically when nothing is set up.

**Defaults and state:**
- Lists start on a sensible selection, such as the first file or the parent.
- When a child is selected, its parent keeps the accent.
- UI state (collapsed sections, navigation, widths) survives navigation and
  restarts.

**Forms:**
- Inputs share one look: an underline that lights on focus, and a visible
  caret so the field reads as an input before anyone types.
- Numbers use `- value +` steppers sized for two digits.
- A fixed set of choices uses a combobox or radio buttons.
- Files and branches come from a picker of real options.
- Settings autosave, and the save shows no toast.
- An extension's preferences open as a dialog (`dialog::shell`) titled
  `Focus preferences`, never a bare menu of `label  value`: one row per
  preference, its label on the left and its value as the control that
  changes it in one column against the right edge (`widget::control`:
  `− 25 +`, `‹ dark ›`, `◉ on`, a field), then under a rule a detail of
  fixed height saying the selected row's kind, range and default (`A
  number from 1 to 90 · default 25`), or where a secret is kept. ←→ step
  and save at once; Enter types or flips.

## Feedback, state and timing

**The header says what is happening; a toast says what happened.**

The header:
- Shows work in flight only: one line, with a spinner and no clock.
- Shows *what* the work is doing, not only a spinner.
- Updates the moment state changes.

Toasts:
- Sit top-right, opaque, above everything, closable with `x`.
- Need a title **and** a detail: short, never truncated, never wider than
  their maximum.
- Appear only for something that happened. Never raise a toast for a
  no-op, a save, or an error flood at startup.
- Never for what the person already sees happen: a prompt typed into the
  agent in front stands in its pane, so "sent to agent" is said only of
  one typed into an agent out of sight.
- What a plugin's action says while it waits ("Start a session…") is gone
  the moment its extension answers, on a view, a drawer or an overlay
  alike: it never stays on a foot after the action ran.
- A gesture with nothing to act on runs nothing and says so in a toast
  that names what is missing (`Copy hash did not run` · `select a row
  first`). It never acts on emptiness: a copy of empty text never reaches
  the clipboard.

**Problems with what a surface draws:**
- What is wrong with an extension's compile or save is `widget::banner`:
  a danger rule down its left, a `⚠` title that counts the errors (`the
  last save has 2 errors`), each error under it in a secondary tone. Never
  loose lines of red text.
- Over a plugin's own surface it is cut to fit and takes at most half the
  room. On a screen that exists to say what is wrong (an extension's
  detail) it is `render_folded`: every error whole, folded across rows,
  in all the room its other content leaves.

**Timing:**
- The first frame is right. Nothing visibly settles, flickers, or shows one
  status and then corrects it a second later.
- Slow work runs off the render thread and survives navigation.
- Agent status never flaps.

**Notifications:**
- One discreet beep on completion, and it can be configured.

## Visual (TUI)

**Colour:**
- Every colour is a `Token`; content colour goes through `theme::content`.
- Colour carries meaning. Don't colour names for decoration, and don't
  spray the accent everywhere.
- A quantity is drawn in the heat ramp (`Heat0`…`Heat4`, `heat-0`…`heat-4`
  for an extension): one hue, brighter for more. Never stand the state hues
  in for amounts: info, success and warning in a row read as three kinds,
  not as less and more.
- The default theme (`dark`) is monochrome: a near-black ground, off-white
  text, a white accent, and hue spent only where it means something (the
  `State*` tokens, of which the warning gold is the workspace's own).
  Markdown is "clean, not a rainbow".
- Ground overlays are neutral grey, **never green-tinted**.
- What weakens safety or switches something off (a flag that skips
  permissions, one that turns delivery off) is drawn in
  `Token::StateWarning` on every row that carries it, its words in that
  hue and no glyph beside them: where a row has only two states, the
  default and the warning, the hue says it alone. Its detail says what it
  gives up in the catalog's own words. Those words are what is lost, a
  noun phrase the detail's verb completes (`-c turns off uze's own value
  for this key`), never a sentence of their own.
- A focused or selected container gets a subtle, slightly darker overlay.
- The sidebar is transparent: only the space in front is lifted onto the
  panel, every other space stands on the column itself, and the row in
  front wears `SurfaceSelected` like every other list, never a trace of
  the accent.
  A drawer or card must stand out from the base ground.
- Light mode and adaptive themes must colour everything, extensions and
  harness panes included.

**Emphasis:**
- Show emphasis by colour (accent or light text): no bold, underline,
  arrows or vertical bars unless asked.
- At most one item in the sidebar is white and bold.

**Alignment:**
- Content's left edge lines up with its title.
- A selection container is inset one column on each side and centred on
  its parent.
- Never double an indent. Columns line up across rows.
- Check every edge before handing back: "desalinhado" is the most frequent
  visual correction.

**Spacing.** One cell of air is the norm: nothing cramped ("colado"),
nothing loose. The settled specifics:
- No empty line at the top of a surface.
- A rule under a section or menu title, then one blank line before its
  items.
- A blank line between column headers and rows, and above a drawer's
  buttons.
- Lists are tight. Label and value are never squeezed together.
- In a form whose keys have a blank line between them (Settings), a
  group heading has two above it, so the groups read apart.
- Add space only where it was asked for. Padding the whole drawer when the
  top was meant is a regression.

Whether options get blank lines between them has gone both ways. Show the
operator rather than assume.

**Chrome:**
- Dividers are thin, dim and one colour. Use few separators: inside a line
  separate with ` · `, not more bars.
- No heavy frames. No small caps, save a tab's badge (`text::small_caps`),
  the one place the vocabulary keeps them: never for a title, a label or a
  hint.
- Glyphs say something and are all the same size. Nerd-only icons appear
  only under the nerd scheme.
- The spinner is `Symbol::StatusWorking`, whose frames the theme declares
  (a small star pulsing, in the default theme); the CLI's is
  `progress::spinner`. Never hand-pick frames.
- The animated star is the agents' alone. A shell running a program wears
  `Symbol::StatusRunning` (`▸`, still) in `state.success`, selected or
  not: the runtime knows a shell runs something, not that it is producing
  output, so nothing about it pulses. When that program ends in a tab the
  operator is not on, the tab wears `Symbol::StatusCompleted` in
  `state.success` until it is on screen, as an agent's finished turn does.

**Drawers.**
- A drawer's title is the name the list shows (`Linked files`), never
  the key it is stored under; the kind line above it is one word
  (`setting`, `memory`).
- A setting's drawer teaches: what it decides, each option with what it
  does (the default marked `· default` on its option, not a row of its
  own), and an `example` of how a project writes it.
- A path beside a label stays on one line, cut at its head.
- A section's status line is short and in a quiet tone
  (`worktree copy · applies once delivered`); the accent hues are for
  state that is changing.

**Declutter.**
- Nothing redundant: a hint the footer already shows, a branch the
  timeline already shows, a label repeated in three places.
- Details go in the drawer, on selection.
- Counts are minimal (`1/5`). There are no raw logs in the UI.

**Empty states:**
- A list inside a popover that has nothing to show (no directory
  matches, no overrides used before) says so centred in the list's own
  room, across and down (`message::render_message`), never on its first
  row.
- The same holds while a line typed into wants a free value and so has
  nothing to list: what is said about it (the flag's summary, and under
  it, held back, that other users can read flag values) stands centred in
  the list's room, never floating on a detail row with empty rows around
  it.
- Minimal: one centred short title, with an optional muted line.
- No hint bar, no stray esc, no empty sidebar slot.
- The drawer goes away.
- The same shape in every extension and section.
- A part that cannot be read empties its own section, never the screen:
  the section bar stays, and each section is gated on what it needs alone.
- A section that cannot apply where the agent stands is not on the bar,
  rather than on it and marked: outside a Git repository the overview is
  the agent's prompts and memory alone.

**Narrow widths:**
- Nothing clips the header, the footer or the first line.
- Text that is too long marquees (slowly, after a pause) or elides. It
  never cuts mid-word in a hint. A plugin's cut line (`Text truncate`)
  slides only while the pointer rests on it, never on its own; at rest it
  ends in an ellipsis. Inside something the pointer presses (a list's
  row), the press wins and the line stays cut. `marquee=#false` keeps a
  line cut for good.
- Drop unimportant columns first and keep the drawer.
- Footers stay on one line, with the two key actions, the rest behind F1.

**Layout:**
- Reuse the shared shapes: a list with a drawer beside it, the integrations
  grid, the slotted top bar.
- A surface's header shows only its capitalised name, muted; paths and
  branches go in the bottom decoration.

## CLI output

**Minimal and structured, like bun, with one pattern for every command.**
Use the `src/progress.rs` helpers and never hand-format:
- `change_report`
- `change`
- `aligned_rows` / `aligned_groups`
- `key_value`
- `report_title`
- `success` / `warn` / `error`
- `next_step`

**Framing:**
- One blank line after the command and one at the end, never two.
- A notice gets a left rule with air above and below.

**Lists:**
- Anything that grows with the number of harnesses, skills or plugins is
  listed **vertically and aligned**, never in columns that overflow.
- Harness names are highlighted the same way in every command.
- A table of figures (sizes, percentages, counts) right-aligns its numbers
  under their headings, and every block of one report shares the same
  columns: `progress::figure_groups`. Two blocks aligned apart, numbers
  flush left, read as lists rather than a table (`uze usage`).

**Scope** is always reported (ADR 054).

**Errors:**
- `error:` plus a message a person understands, plus `→` a next step.
- No repeated warnings, and no internal vocabulary such as "shadowed" or
  "installed, but not delivered".

**Exit and progress:**
- Commands exit when they finish; nothing hangs waiting for ctrl+c.
- A long step shows what it is doing.

**Help:**
- Flat, grouped only by spacing and the `Command*` colours. Never add
  category titles.
- Docs URL at the end. `uze` alone prints help.
- Examples show the command and its output, with room between them.

## Words

- Every UI and CLI message is in English.
- Write short and direct, not robotic, not literal, not poetic.
- Name the thing, not the mechanism. User-facing status is never technical.
- A project resource that reaches a harness says only who delivers it:
  `Native` (the harness reads it) or `Delivered` (uze brings it, through
  the shim, the rules engine or its plugin), never the mechanism ("Inside
  the workspace", "Rules engine", "Plugin") nor the brand ("Via uze"). The
  drawer's own groups already say where it is. One that does not reach it
  says why in one word a person can act on: `Run uze setup`, or `Blocked`
  when a file of the operator's holds the path uze would use (`uze doctor`
  names the file).
- A status shows the state, not the history that produced it: provisioning
  reads `Verified`, never `Verified (Update)`, which reads as an update
  waiting beside a version.
- Use the present tense for actions ("send", "install"), lowercase labels,
  and punctuate status words the same way everywhere. A plugin's clickable
  controls follow the core: its buttons, tabs, switches, a form's submit and
  the doors to its surfaces are drawn lowercase by the host, whatever the
  plugin wrote. Everything else it draws (text, headings, badges, a
  choice's value, its data) keeps the case the plugin gave it.
- A package that carries only workspace extensions is an extension, not a
  plugin, wherever a report names what it changed: `1 extension installed
  on this machine only`, `1 plugin and 1 extension updated`
  (`progress::count_packages`). Only a package a harness receives is a
  plugin.
- "uze" is lowercase, and a sentence shouldn't start with it.
- Don't claim more than the product does, and don't repeat a claim
  everywhere.
- Prompts UZE writes into a project are generic and never mention uze.
- Don't show the plumbing: no briefing paths or migration chatter.

## Before handing back

Answer each of these. A "no" is a fix, not a note in the report:

1. Does every hint, button, chip and row I drew answer a click exactly as
   its key does, in every state, typing included? Is there a click == key
   test for it?
2. Does everything clickable have a hover? Does the hover change only the
   colour, keeping the word's weight?
3. Does it match the nearest sibling surface's structure, spacing and
   alignment? Did I check edges against the title?
4. Is every colour a token, every glyph a `Symbol`, every piece of chrome a
   widget?
5. Did I check it at a narrow width, in an empty state, in light mode, and
   with long text?
6. Does each toast have a title and a detail, and is it about something
   that actually happened?
7. CLI: does it use the progress helpers, list vertically, frame with
   single blank lines, and report scope?
8. Is every word short, in English, and not technical?

`TestBackend` tests (`src/ui/**/tests`) prove what a screen says. Exercising
a change by hand in the real TUI (`make install`, then `uze workspace`) is the
operator's job before they validate it. Don't claim it works from tests
alone: "faça um teste real na TUI, teste input por input". When a change
touches a recorded flow, the GIFs need recording again (`tui:record`).

## Keeping this skill current

This skill is only useful if it holds every rule the operator has settled.

- **When the operator corrects anything visual, UX, DX or wording,** add the
  rule here in the same change as the fix. Write it as a directive, in the
  section it belongs to, and replace the old rule if it contradicts one.
  Don't log it as an anecdote.
- **When a rule becomes enforceable,** move it into code: a widget, a
  `#[must_use]`, a layering rule, an affordance test. Leave a one-line
  pointer here. A rule the build enforces cannot regress.
- **When you add a widget, token, symbol or progress helper,** update
  [vocabulary.md](references/vocabulary.md).
- Don't leave a rule here that the code no longer agrees with. Fix one or
  the other.
