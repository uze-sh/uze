## D1 — Bound the bind with one work budget, shared with the check

- status:   pending
- task:     1.2
- commit:   squashed on delivery
- trigger:  the node cap counted only nodes drawn; the brief proposed reusing the trace's `reached` counter.
- decided:  the binder keeps its own `steps` (per build, not per trace entry), counting every `use`, node instance and filtered list row, capped at `uze_contract::EXPANSION_MAX` = 20 000; once spent every loop breaks, so the build is O(budget). The compiler refuses a view whose template expansion alone passes the same constant.
- rejected: `reached` as the budget (it is per entry and was `u32`, overflowing at 4e9); counting only `use`s (an `each` over its parent's list is n² with no `use`); moving the bind off the drawing thread (the budget makes the worst build ~1 ms; the memo and hit-testing assume a synchronous build).
- blast:    reverting re-opens the hang; a view expanding past 20 000 entries through templates is now refused by the check, which no repository plugin approaches.

## D2 — A template fan-out still installs, marked "does not compile"

- status:   pending
- task:     1.3
- commit:   squashed on delivery
- trigger:  the brief asked that `uze agent plugin check` reject the bomb before install; `uze install` installs any extension whose view does not compile and reports it so.
- decided:  kept that behaviour: the bomb installs, `uze status` shows `× does not compile` with the fan-out message, and nothing binds it.
- rejected: making install refuse a plugin over one extension's compile error (a behaviour change to install beyond this audit, and other compile errors would need the same).
- blast:    none; the check refuses it and the workspace never draws it.

## D3 — Shadow map and zip too, not only replace

- status:   pending
- task:     2.2
- commit:   squashed on delivery
- trigger:  auditing every package function (design.md's table) found `map` and `zip` collect fresh callback results with no check until the end: 10 000 × 512 KiB held 5.2 GB for 7.7 s in the audited binary.
- decided:  shadowed both with a collector that counts each result as Rhai's check would and refuses at the first bound passed, calling the function pointer with Rhai's own arity fallback (a semantics test covers `this`, one and two arguments).
- rejected: shadowing the linear-bounded functions (`split`, `to_chars`: ≤ one element per byte, +14 MB measured); patching Rhai.
- blast:    reverting re-opens multi-gigabyte allocations from one call; the shadows must be re-checked on a Rhai upgrade.

## D4 — Run every answer on a 64 MiB scoped thread, not only the pool

- status:   pending
- task:     3.2
- commit:   squashed on delivery
- trigger:  the brief named the resident and pool threads; the pool lives in `src/ui` (which may not name `uze-script`), and the CLI's `check --run` answers on the main thread (1 MiB on Windows). One answer nests ~1 000 levels before its deadline, enough to overflow a 2 MiB debug thread on `==` (measured).
- decided:  `Script::call` runs the answer on a scoped thread with `engine::STACK` (64 MiB, reserved not committed); the resident thread gets the same. Measured: a release build compares, prints and drops the structural ceiling (~20 000 levels) in 64 MiB; debug ~5 000.
- rejected: a constant re-exported through `uze-application` into `src/ui/orchestrator/lanes.rs` (covers the pool but not the CLI or tests); bigger stacks for debug (≥200 MiB).
- blast:    one thread spawn per answer (tens of µs against a 0.19 ms engine build).

## D5 — Bound `this` by depth only, at the crossing depth

- status:   pending
- task:     3.2
- commit:   squashed on delivery
- trigger:  the brief asked for a depth/size check on `this` between calls.
- decided:  after every call, `this` nesting more than `value::MAX_DEPTH + 1` containers (what could still cross as JSON) stops the session with the new `Limit::Nesting`. No size check: Rhai already checks the whole `this` on every property set (growing it 512 KiB a call failed on the second call).
- rejected: a lower bound (refuses nothing a render could use at 128); a size walk (redundant with Rhai's).
- blast:    a resident script keeping deeper data now stops; such data could never have been rendered.

## D6 — Escline as a flag cleared by the next line break

- status:   pending
- task:     4.2
- commit:   squashed on delivery
- trigger:  KDL's escline allows whitespace, Unicode spaces and comments between `\` and the break.
- decided:  a `\` outside strings and comments sets `continued`; the next break (`\r\n` as one) clears it without ending the slashdash. What comes between passes through the normal loop.
- rejected: parsing escline's exact grammar after the `\` (any whitespace form left out would reopen the bypass).
- blast:    none; a stray `\` only makes the scan stricter on input KDL rejects anyway.
