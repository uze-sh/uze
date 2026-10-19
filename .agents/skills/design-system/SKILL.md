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
- Prove a new hint bar with a click == key test. See
  `clicking_an_overview_foot_hint_is_pressing_its_key` and
  `clicking_a_surface_footer_hint_is_pressing_its_key`.

**Clicks:**
- The first click acts. Opening or closing a popover, menu or Recents must
  never swallow the next click.
- The whole row, chip, card or button is the target, not just the arrow or
  one word.
- Never leave a dead region, and never open anything on a double click:
  Enter opens.
- A click that does nothing must not *look* like it acted. For example, a
  drawer must not close as if it ran something.

**Hover:**
- Everything clickable has a hover, and nothing that isn't clickable does.
- Hover is a change of colour only (a "realce"): never underline, an arrow
  or a background overlay.
- Hover keeps the word's weight. A word that is bold at rest stays bold; a
  word that is not never turns bold under the pointer or while pressed.
  Manage's version and health are the reference.
- Text-only affordances start muted and brighten.
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
- Dropdowns float over content and never push it down.
- A modal keeps its height while you filter inside it.

**Closing:**
- Esc closes everything, and a modal or a drawer says so nowhere: its
  closing is a bare `x` at the top right, with no label, no footer "close"
  button and no "esc" hint.
- A surface's own foot (the overview's) is different: it is the keys that
  act there, so it names Dismiss by what it does in that state, "back",
  "cancel", "clear" or "close", and that word is a click target like any
  other.
- A drawer closes only on a click outside it.

**Shortcuts:**
- Keep them few: group actions behind a context menu (`alt+.`) rather than
  adding specific keys.
- One modifier per family: extensions all use `alt`.
- Keyboard users must never lose a path.

**Confirmation:**
- A destructive action is confirmed in a centred dialog with a scrim and
  Confirm/Cancel buttons, chosen with the arrows and Enter. Never an
  inline question in a foot or a row.
- In the space overview the dialog is its one question
  (`overview_question.rs`): add a case to `Asked` rather than a dialog of
  your own.
- Never ask for Y/N.
- Enter is the primary action.

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
- The default theme (`dark`) is monochrome: a near-black ground, off-white
  text, a white accent, and hue spent only where it means something (the
  `State*` tokens, of which the warning gold is the workspace's own).
  Markdown is "clean, not a rainbow".
- Ground overlays are neutral grey, **never green-tinted**.
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

**Declutter.**
- Nothing redundant: a hint the footer already shows, a branch the
  timeline already shows, a label repeated in three places.
- Details go in the drawer, on selection.
- Counts are minimal (`1/5`). There are no raw logs in the UI.

**Empty states:**
- Minimal: one centred short title, with an optional muted line.
- No hint bar, no stray esc, no empty sidebar slot.
- The drawer goes away.
- The same shape in every extension and section.

**Narrow widths:**
- Nothing clips the header, the footer or the first line.
- Text that is too long marquees (slowly, after a pause) or elides. It
  never cuts mid-word in a hint.
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
- Use the present tense for actions ("send", "install"), lowercase labels,
  and punctuate status words the same way everywhere.
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
