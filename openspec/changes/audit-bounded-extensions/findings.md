Audited commit: 98d1210f · Date: 2026-10-30 · Skill: `security-audit`

These findings share one root cause. The bounds UZE puts on extension work
limit the *visible* size of results: drawn nodes, a string's final length,
one value's width. They do not limit the work spent getting there, nor what
accumulates across calls. A small input therefore buys unbounded CPU,
memory or stack, and since the work runs on the drawing thread or in UZE's
own process, the whole workspace goes down.

## EXT-1 - Template fan-out in a view hangs the drawing thread, with no consent needed

- **Severity**: High (availability). Installing the plugin is enough. A
  fixture-only extension needs no consent and freezes the workspace client
  for every agent.
- **Status**: CONFIRMED through `uze install` and `uze agent plugin check`,
  which call the same `bind::build`. The TUI freeze itself was read from the
  code, not run.
- **Threat actor**: a malicious plugin author (1), or a repository whose
  extension gets installed (2).
- **Location**:
  - `crates/uze-extensions/src/declared/bind.rs:333-356` (`use_template`)
    walks every `use`, while `NODE_CAP` (`:26`) counts only nodes appended
    in `instance` (`:401`). `TEMPLATE_DEPTH` (`:41`) bounds depth, not
    fan-out.
  - The compiler (`crates/uze-core/src/capability/extension/view.rs`)
    refuses only cycles.
  - The bind runs on the draw path: `render/bar.rs:85` → `declared.rs:1197`
    `view()` → `with_built` (`:964`). The terminal size is part of the memo
    key, so every resize rebuilds.
  - `crates/uze-application/src/application/extensions/consent.rs:144`
    decides that a fixture-only extension needs no consent.
- **Attack**: `t0` holds one `Text`, each `tK` holds ten `use "t{K-1}"`, and
  the root uses `t12`. That is 1,869 bytes, and it compiles clean.
- **Evidence**: bind time grows tenfold per level:

  | Levels | Bind time |
  |---|---|
  | 3 | 0.11 s |
  | 5 | 0.51 s |
  | 6 | 3.9 s |
  | 7 | 38.2 s |
  | 12 | never returns (timeout at 20 s of pure CPU) |

  `uze status` reports the plugin as "runs nothing".

## SCRIPT-2 - Rhai `replace` makes UZE attempt an unbounded allocation and abort

- **Severity**: Medium/High (availability). Any consented script source is
  enough, with no grant. The whole uze process aborts. Sources run on
  workspace open and on `every`.
- **Status**: CONFIRMED with the real binary, and reproduced independently.
- **Threat actor**: a malicious plugin author (1), once the extension is
  consented.
- **Location**: `crates/uze-script/src/engine.rs:79-90` keeps
  `MoreStringPackage`. Its `replace` builds the whole result before Rhai's
  `max_string_size` check, and the 100 ms deadline is not consulted inside
  native calls. `primitives.rs:30` already shadows `pad` for the same class
  of bug.
- **Attack**: `s` is 512 KiB of `a` and `big` is 8 KiB, then
  `s.replace("a", big)`, which asks for 4 GiB.
- **Evidence**: `memory allocation of 4294967296 bytes failed` and
  `Command terminated by signal 6`.

## SCRIPT-1 - A resident script nests data across calls until the resident thread's stack overflows

- **Severity**: Medium. A plugin granted only `resident` aborts the TUI
  (SIGABRT), and it does so again every time its surface opens.
- **Status**: CONFIRMED at library level, through the public `uze-script`
  API, which spawns the same `uze-resident` thread.
- **Location**:
  - `crates/uze-script/src/resident.rs:136-139` spawns the thread with the
    default stack, and each call gets a fresh budget.
  - `this` persists between calls. Rhai's limits are per value (array and
    map size) with no nesting depth, and its clone, drop and size checks are
    recursive. `value.rs`'s depth limit of 128 covers only the JSON
    crossing.
- **Attack**: each timer event runs `this.a=[this.a]` (or `#{x:this.a}`) 8
  times.
- **Evidence**: in a release build, depth reaches 6408 at call 800 (3.3 s),
  then `thread 'uze-resident' has overflowed its stack` and SIGABRT. A debug
  build aborts at a depth of about 1150.

## EXT-2 - The KDL scanner's slashdash-nesting guard is bypassed with escline

- **Severity**: Low. The 2 s parse timeout backstops it, but each file
  leaves an orphaned worker burning a core.
- **Status**: CONFIRMED.
- **Location**: `crates/uze-core/src/capability/extension/bounded.rs`.
  `Scanner::open_block` (~`:286`) refuses `/-` blocks nested in one another,
  but it treats `\` + newline as ordinary bytes.
- **Attack**: `Row {` + (`/-n \`⏎`{ `)×N + `}`×N.
- **Evidence**: depth 8 parses in 0.21 s, depth 12 in 1.61 s, and depth 14
  takes 2.11 s and hits "took longer than the 2 s". Memory stays flat.

## Defenses that held

- The sanitizer strips ESC, CSI and OSC (including OSC 52 and OSC 8), C1,
  bidi overrides and isolates, and zero-width and tag characters. It caps
  combining marks and bounds width.
- `settle_controls` runs on every frame.
- KDL input is capped at 64 KiB, the pre-scan at depth 32, the parse thread
  at an 8 MiB stack, and the parse at 2 s.
- `read_checkout` and `list_checkout` refuse traversal and links. The
  per-question deadline of 100 ms and the operation budget stop single-call
  nesting, and a store crossing is capped at depth 128.
- `uze-document`'s ladder refuses a newer shape and never overwrites it.
- The program runner refuses exec paths under the project, `UZE_HOME` or
  tmp, allowlists the environment, and refuses an argv `{field}` with a
  leading `-`. Linux confinement requires both Landlock and seccomp, or
  nothing runs.
- Architect links reject absolute paths, `..` and drive letters. Picture
  decode bounds its pixels before allocating.
