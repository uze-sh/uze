## Context

ADR-016 separated declaring a plugin (`uze flow@ai`) from reproducing a
project (`uze install`). The flatten change (ADR-054) folded `uze plugin
install` into `uze install <spec>`, so `install` became both, and the
code said so: "the same operation with two spellings".

## Goals / Non-Goals

**Goals:** one meaning per verb (`add` declares, `install` reproduces,
`update` moves pins, `remove` undeclares); a named, listed verb for adding.

**Non-goals:** changing what either operation does; a `--locked` mode for
`install` (whether `install` re-resolves a drifted manifest is a separate
question); a transition alias.

## Decisions

- **`add`, not the shorthand alone.** Keeping only `uze x@y` would honour
  "one spelling per operation" literally, but leaves the operation without
  a row in the help and without a word to say in docs. `uze x@y` stays as
  `add` without its verb: the root help lists `add` and shows the shorthand
  as an example, never as a row of its own.
- **`install x@y` is refused, not aliased.** An alias keeps the two-meaning
  verb alive. The refusal names `uze add x@y`, exits 2 like any usage
  error, and the flags that belong to one plugin's add are unknown to
  `install`, so clap refuses them itself.
- **`AddArgs` is shared** by `Command::Add` and `ShorthandArgs`
  (`#[command(flatten)]`), so the two spellings cannot drift.
- **`market add` stays.** It is a subcommand of `market` with its own
  object (a source), as `cargo add` and `cargo registry add` coexist.

## Risks / Trade-offs

- Callers that discard output (`uze install $p@lab -m >/dev/null`) would
  fail silently; the Conformance Lab's scenarios and experiments move in
  this change for that reason.
- Agents trained on `claude plugin install name@marketplace` will type
  `uze install x@y`; the refusal's next step costs them one round trip.
