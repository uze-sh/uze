## D1 — Hosted engines are one per slot, which the built-ins cannot be

- status:   pending
- task:     1.1
- commit:   d8179d9a
- trigger:  Making jobs real showed the slot map builds one engine per surface: a declared extension with a view and a section holds two engines, each asking for its own reads. That is harmless for a declared engine, which keeps no state between its surfaces, but `code` shares its selection between its view and its timeline section, and `spec` its change between its document and its summary.
- decided:  Jobs run per slot as the map stands (task 1.1). Before `spec` (task 3), the slot map must hold one engine per extension, shared by its slots; that is a step of its own, recorded here rather than folded into 1.1.
- rejected: Sharing engines in this step: it changes how declared extensions keep UI state per surface, which deserves its own green commit.
- blast:    `schedule_engine_jobs` iterates slots; with shared engines it iterates engines.

## D2 — Stop the unattended run before architect moves

- status:   pending
- task:     2.0
- commit:
- trigger:  Mapping `architect` showed its move is a rewrite of the slot path, not a port: it borrows the code surface's frame, its text selection reads the rows that frame drew, and it answers hover, a grabbed diagram's drag, a sideways pan, its scrollbar and a jump into the code surface, none of which the slot path carries.
- decided:  The overnight run ends after group 1 (jobs, one engine per extension). Group 2 starts in a session where the operator can check each gesture by hand in the TUI, as the design system asks of every gesture change.
- rejected: Moving it unattended: the orchestrator tests cover the drawing and most gestures, but not how a drag or a hover feels, and the operator was about to validate this build.
- blast:    None: nothing of group 2 was written.
