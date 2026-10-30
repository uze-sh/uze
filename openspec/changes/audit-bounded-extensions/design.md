## Context

See proposal.md for the findings. Three places spend extension work:
the binder (`uze-extensions/src/declared/bind.rs`), which runs on the
drawing thread and rebuilds on every resize; the Rhai engine
(`uze-script`), on the extension pool's threads, a resident session's
thread, or the CLI's main thread for `uze agent plugin check --run`; and
the KDL pre-scan (`uze-core/src/capability/extension/bounded.rs`), which
stands before a parser that is exponential on nested slashdashed blocks.

## Goals / Non-Goals

**Goals:** each bound limits work spent, not only result size; no input
aborts, overflows or stalls UZE's process or drawing thread; every
regression test fails on the audited commit.

**Non-Goals:** moving the bind off the drawing thread (the budget makes it
a few milliseconds at worst, see D1); reaping the orphaned parse thread a
timed-out parse leaves (EXT-2's backstop, unchanged); upgrading Rhai.

## Decisions

### EXT-1: a work budget in the binder, and a static bound in the check

The binder counts a step for every `use` it follows, every node instance it
considers (drawn, hidden by a condition, or past the node cap) and every row
a list's `filter` looks at. At `EXPANSION_MAX` (20 000, in `uze-contract`
so the compiler and the binder share it) it sets `spent`, every loop breaks
at once, and the root carries `Folded: this view expands to more than 20000
nodes`. After the budget is spent the walk unwinds in as many steps as it is
deep, so the build is O(budget) whatever the view or the data.

The compiler adds `refuse_fan_out`: `CompiledView::expansion()` counts the
entries a bind walks with every `each` repeating once (memoised per
template, saturating; an acyclic `use` graph makes it exact), and the check
refuses past `EXPANSION_MAX`. A view that passes can still exceed the
budget through its data; that is what the binder's fold is for.

- The trace's `reached` counter is per entry, so it could not be the budget;
  the build keeps its own `steps`, and `reached` now saturates instead of
  overflowing `u32`.
- 20 000 is ten times the 2 000-node cap: a view that draws its cap with
  hidden alternatives and templates still fits; measured at about 1 ms per
  20 000 steps in a debug build of the test suite.
- Rejected: counting only `use`s (an `each` over its own parent's list with
  hidden nodes is n² with no `use` at all, see the second regression test);
  bounding fan-out per template (a chain of small fan-outs multiplies).
- Rejected: moving the bind off the drawing thread. The budget makes the
  worst build cheap, and the memo, hit-testing and acts all assume a
  synchronous build; it would be a large change for no remaining risk.

### SCRIPT-2: shadow every function whose result outgrows its input

`primitives::register_sized` shadows, as `pad` already was: `replace` of a
string or a character by a string (length counted with `matches` before the
new string is built), and `map` and `zip` (results counted as they are
collected, the way Rhai's own check adds a value up, and refused the moment
a bound is passed). The errors are Rhai's own `ErrorDataTooLarge`, so they
read as the string, array or map bound. `map` and `zip` call the function
pointer with the same arity fallback Rhai's do (`this`, then the item, then
item and index), checked by a semantics test.

Every function the registered packages give was checked against one
question: can its result be more than a small multiple of its inputs, built
before Rhai's check sees it?

| Package / function | Result vs. inputs | Verdict |
|---|---|---|
| String `replace(str, str)`, `replace(char, str)` | up to input × substitute (4 GiB in the PoC) | **shadowed** |
| String `replace(str, char)`, `replace(char, char)` | ≤ 4 × input | bounded, kept |
| String `pad(len, str)` | Rhai loops forever on `""` | already shadowed (`pad`) |
| String `pad(len, char)` | `len` checked before; ≤ 4 bytes a char | bounded, kept |
| String `+`, `+=`, `append` (string, char, any value) | ≤ sum of inputs; a value printed is ≤ ~8 × its size, itself bounded | bounded, kept |
| String `to_upper`, `to_lower`, `make_*` | ≤ 3 × input | bounded, kept |
| String `sub_string`, `crop`, `trim`, `remove`, `truncate`, `pop`, `get`, `set`, `index_of`, `contains`, `starts_with`, `ends_with`, `min`, `max`, `len`, `bytes` | ≤ input | bounded, kept |
| String `split*`, `split_rev*`, `to_chars` | ≤ one element per input byte; measured +14 MB for 1 MiB, then refused | bounded, kept |
| String `to_blob`, `+ Blob` | ≤ input (no blob constructor is registered) | bounded, kept |
| Array `map(f)` | n fresh callback results, checked only at the end (5 GiB held in 3 s, measured) | **shadowed** |
| Array `zip(other, f)` | same as `map` | **shadowed** |
| Array `pad(len, item)` | Rhai checks `item × pad` before resizing | bounded, kept |
| Array `push`, `insert`, `append`, `+`, `concat`, `splice*` | ≤ sum of inputs; checked after each call | bounded, kept |
| Array `filter`, `retain*`, `drain*`, `extract*`, `split_at`, `chop`, `truncate`, `reverse`, `sort*`, `order*`, `dedup*` | ≤ input | bounded, kept |
| Array `reduce*`, `for_each`, `find*`, `some`, `all`, `index_of*`, `contains`, `==` | one value, each callback result checked | bounded, kept (recursion: see SCRIPT-1) |
| Map `keys`, `values`, `mixin`, `+`, `fill_with`, `remove`, `get`, `set`, `==` | ≤ sum of inputs | bounded, kept |
| Iterator ranges, `chars`, `bits` | lazy; each step costs an operation | bounded, kept |
| Arithmetic, logic, maths | scalars | n/a |
| Our primitives: `lines`, `regex` (matches capped), `json_parse` (depth 128), `json_stringify`, `glob*` (lengths capped), `front_matter` | ≤ a small multiple of input | bounded, kept |

Rejected: lowering `string_size` (the bug is the order of allocate and
check, not the size); the `unchecked`-free alternative of patching Rhai
(not ours to ship; the shadows can go when upstream checks first).

### SCRIPT-1: a sized stack everywhere, and a depth bound on `this`

Rhai clones, drops, compares and prints a value recursively, and its size
bounds allow a value nested some 20 000 levels (arrays and maps
alternating, each bound 10 000). Measured on rhai 1.27: on a 2 MiB thread a
debug build overflows comparing a value about 1 000 deep; a release build
compares, prints and drops one 20 000 deep within 64 MiB, a debug build
about 5 000, while one answer nests at most ~1 000 levels before its 100 ms
deadline. So:

- `engine::STACK` (64 MiB, reserved not committed) is the resident thread's
  stack, and `Script::call` runs every answer on a scoped thread with it,
  whichever thread asked (pool, CLI main thread, which is 1 MiB on Windows,
  or a test). Spawning costs some tens of microseconds against the 0.19 ms
  engine build an answer already pays.
- After each resident call, `value::deeper_than(this, MAX_DEPTH + 1)` (a walk
  no deeper than the bound) stops the session with the new
  `Limit::Nesting` when `this` could no longer cross as JSON. 128 is the
  depth a value crossing to the host already has (`value::MAX_DEPTH`), so
  nothing a session could render is refused.
- Rhai already bounds `this`'s total size on every property set (checked:
  growing it by 512 KiB a call fails on the second call), so no size check
  was added.

Rejected: a bigger stack alone (accumulation across calls would still grow
without bound, and only the release build covers the structural ceiling);
the depth check alone (one call still nests ~1 000 levels, enough to
overflow a 2 MiB debug thread on `==`); shadowing `==` and printing with
iterative versions (many entry points, and clone and drop would remain).

### EXT-2: escline in the scanner

A `\` outside a string, comment or raw string sets `continued`; the next
line break (`\r\n` taken as one) clears it instead of ending the node a
`/-` applies to. Whitespace, Unicode spaces and comments between the `\`
and the break pass through the normal loop and change nothing. A `\` that
KDL would refuse only makes the scan more conservative.

Rejected: skipping exactly KDL's escline grammar after the `\` (any
whitespace form the scanner did not list would reopen the bypass).

## Risks / Trade-offs

- [A legitimate view expands past 20 000 entries through templates] → the
  check says so with a fix; none of the repository's plugins comes close.
- [Rhai adds a new amplifying function in an upgrade] → the table above is
  the list to re-check on a Rhai bump; the `pad` shadow already carries the
  same note.
- [A debug build still overflows on `==` of a value nested past ~5 000
  levels] → one call cannot build that before its deadline on this hardware;
  release covers the structural ceiling. Residual, recorded in evidence.md.
- [Each answer starts a thread] → measured cost is far below the answer's
  own; a failure to spawn is an answer error, never a panic.

No existing ADR is changed. ADR-041 (an extension reaches nothing it was not
handed) is unaffected.
