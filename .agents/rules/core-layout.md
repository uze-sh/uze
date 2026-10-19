---
trigger: glob
globs: "**/crates/uze-core/**"
---

`uze-core` is organized into five concerns, each a module whose own doc
says what belongs in it: `package/` (where a package's bytes come from and
live), `capability/` (what a plugin declares, portably), `delivery/` (how a
capability reaches a harness), `project/` (what a project declares and what
UZE writes into it) and `machine/` (the local environment outside UZE's own
state). Read the concern before the module: `hook` is a *capability*, a
different question from where its file sits.

Public paths stay flat (`uze_core::store`, not `uze_core::package::store`)
via re-exports at the crate root, which is also where a reader sees which
concern each module belongs to. The crate names no harness and never
depends on `uze-workspace`.
