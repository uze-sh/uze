# uze-application

The product-facing facade. `UzeApplication` orchestrates Core plus the
integrations into the lifecycle operations a user asks for (install, update,
remove, attach, context) and the workspace's services.

- `src/application.rs` — the facade and its service accessors
- `src/application/lifecycle/` — one module per operation (install, update,
  remove, attach)
- `src/application/services/` — the workspace's services (tasks, checkouts,
  work), each a directory once it outgrows one file

This is also the only surface presentation may consume: `src/` must not name
`uze_core::` or `uze_integrations`. Something missing? Add a read model or a
method here.

```bash
cargo test -p uze-application
```
