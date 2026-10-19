---
trigger: glob
globs: "**/crates/uze-platform/**"
---

`uze-platform` answers what UZE asks the operating system, once per
platform. One concept per module, one API per concept, and one
implementation per platform selected by `cfg` at that module's boundary
(`mod unix;`/`mod windows;` behind `use … as imp`): nothing that uses it
branches on the operating system, and a `cfg!` at a call site anywhere else
in the workspace is a missing concept here. Distinct per-platform data gets
distinct types, never optional fields on a shared one.

It is a leaf naming no domain, no UZE path and no harness, because
`uze-core`, `uze-git`, the terminal runtime and the binary all need it and
none may depend on another for it.
