---
trigger: glob
globs: "**/crates/uze-application/**"
---

`uze-application` is the product-facing facade. `src/application.rs` is
`UzeApplication` and its service accessors; `src/application/lifecycle/`
holds one module per operation (install, update, remove, attach, and
`effective`, what is effectively installed); `src/application/services/`
holds the workspace's services (tasks, checkouts, work). A module becomes a
directory once it outgrows one file.

It is what `src/` consumes: a read model, a service method, or a re-export
of vocabulary a read model is made of. It names no harness.
