## D1 — Hosted engines are one per slot, which the built-ins cannot be

- status:   pending
- task:     1.1
- commit:
- trigger:  Making jobs real showed the slot map builds one engine per surface: a declared extension with a view and a section holds two engines, each asking for its own reads. That is harmless for a declared engine, which keeps no state between its surfaces, but `code` shares its selection between its view and its timeline section, and `spec` its change between its document and its summary.
- decided:  Jobs run per slot as the map stands (task 1.1). Before `spec` (task 3), the slot map must hold one engine per extension, shared by its slots; that is a step of its own, recorded here rather than folded into 1.1.
- rejected: Sharing engines in this step: it changes how declared extensions keep UI state per surface, which deserves its own green commit.
- blast:    `schedule_engine_jobs` iterates slots; with shared engines it iterates engines.
