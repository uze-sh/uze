## Why

The bounds UZE puts on extension work limit the size of a result, not the
work spent reaching it, nor what accumulates between calls:

- **EXT-1**: a view's template fan-out walks ten to the twelfth `use`s on
  the drawing thread, because the node cap counts only nodes drawn.
- **SCRIPT-2**: Rhai's `replace` (and, found while fixing it, `map` and
  `zip`) builds its whole result before the size check sees it, so one
  call asks for gigabytes and can abort the process.
- **SCRIPT-1**: a resident script nests `this` a few levels per call until
  a recursive clone, drop or comparison overflows its thread's stack.
- **EXT-2**: an escline (`\` + newline) hides a slashdashed block from the
  KDL pre-scan, so the exponential parse runs until its timeout.

The property this change establishes: every bound on extension work limits
the work spent, and no extension input can abort, overflow or stall UZE's
own process or drawing thread.

## What Changes

- The binder spends a fixed budget of entries per build, counting every
  node and `use` it walks, drawn or not, and folds with a notice when it is
  spent; the check refuses a view whose templates alone expand past it.
- The script engine shadows every package function whose result can be many
  times its input (`replace` of a string, `map`, `zip`) with one that counts
  the result before allocating it.
- Every script runs on a thread with an explicit, documented stack; a
  resident session stops, with a `nesting depth` error, when `this` nests
  deeper than a value may cross.
- The KDL pre-scan understands escline, so the existing refusal of a
  slashdashed block inside another catches the bypass.

## Capabilities

### New Capabilities
- `extension-scripts`: adds the requirement that a script's work is bounded
  before it allocates and never on a stack it can overflow.
- `extension-resident`: adds the requirement that what a resident script
  keeps between calls stays within the depth a value may cross.

### Modified Capabilities
- `extension-views`: adds that binding is bounded in work, not only in
  nodes drawn, and that the nesting scan sees through escline.

## Impact

`uze-contract` (`EXPANSION_MAX`, `CompiledView::expansion`), `uze-core`'s
view compiler and KDL scanner, `uze-extensions`' binder, and `uze-script`'s
engine, primitives, resident session and answer path. No new dependency; a
view whose templates expand past 20 000 entries no longer passes the check.
