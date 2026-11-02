---
trigger: model_decision
description: Read before writing code that copes with anything an earlier UZE release left on a machine - an older record shape, a path it wrote, a file it delivered into a harness or a project, an older wire protocol or command spelling - or before deleting such code.
---

Support for what an earlier release left is **declared, never inline**, so
that it can be found and retired by changing one value.

- **One floor.** `uze_document::earlier::FLOOR` is the oldest release a
  machine can come from and still be carried across.
- **Every piece is an `Earlier` entry**, in the home of its kind, naming how
  long it `lasts`:

  | kind | home |
  |---|---|
  | a record's older shape | a rung of its `Shaped::ladder()`, listed by its module's `rungs()` |
  | a path UZE's home once had | a `retired(..)` entry in `machine/home/owned.rs` |
  | a file an integration once delivered | that integration's `earlier.rs` (`shared/earlier.rs` for what verticals share) |
  | what UZE left in a project or a checkout | `project/earlier.rs`, `checkout/earlier.rs` |
  | an older wire or command spelling | `uze-terminal`'s `runtime/earlier.rs`, `src/cli/earlier.rs` |

  The code that carries it out runs where it always ran; it reads the
  names it handles from its entry (`spelled`), and nothing else spells them.
- **`Lasts::Until(r)`** only when every run since `r` carries it out (a
  disposable path swept on every bootstrap). Anything carried out only
  when something next happens — a record rewritten when next changed, a
  leftover removed when its package is next attached — is
  **`Lasts::Unbounded`**: the floor never passes it. `r` is the last release
  whose run could leave it: `git tag --contains <commit that stopped
  writing it> --sort=v:refname | head -1`, then the tag before that in
  version order; unreleased means the newest tag.
- **Retiring a release** is raising `FLOOR` and deleting every entry
  `tests/architecture/earlier.rs` names, with its tests. Nothing else
  changes; if something else has to, that something was support written
  inline.
- A field a record gains for an older build sharing the machine to ignore is
  **additive**: `#[serde(default)]` with `Additive` in its doc, no new
  shape. A field an earlier shape spelled differently is a rung, never a
  `serde(alias)`.
- The suite's budgets for inline support are exact and only go down. A
  failure's `Fix` is the instruction to follow; never raise a budget.
