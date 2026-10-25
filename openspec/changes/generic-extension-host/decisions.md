## D1 — Hosted engines are one per slot, which the built-ins cannot be

- status:   pending
- task:     1.1
- commit:   d8179d9a
- trigger:  Making jobs real showed the slot map builds one engine per surface: a declared extension with a view and a section holds two engines, each asking for its own reads. That is harmless for a declared engine, which keeps no state between its surfaces, but `code` shares its selection between its view and its timeline section, and `spec` its change between its document and its summary.
- decided:  Jobs run per slot as the map stands (task 1.1). Before `spec` (task 3), the slot map must hold one engine per extension, shared by its slots; that is a step of its own, recorded here rather than folded into 1.1.
- rejected: Sharing engines in this step: it changes how declared extensions keep UI state per surface, which deserves its own green commit.
- blast:    `schedule_engine_jobs` iterates slots; with shared engines it iterates engines.

## D2 — Stop the unattended run before architect moves

- status:   resolved (the operator asked to continue, then checked the moved architect by hand in the TUI and found it unchanged)
- task:     2.0
- commit:
- trigger:  Mapping `architect` showed its move is a rewrite of the slot path, not a port: it borrows the code surface's frame, its text selection reads the rows that frame drew, and it answers hover, a grabbed diagram's drag, a sideways pan, its scrollbar and a jump into the code surface, none of which the slot path carries.
- decided:  The overnight run ends after group 1 (jobs, one engine per extension). Group 2 starts in a session where the operator can check each gesture by hand in the TUI, as the design system asks of every gesture change.
- rejected: Moving it unattended: the orchestrator tests cover the drawing and most gestures, but not how a drag or a hover feels, and the operator was about to validate this build.
- blast:    None: nothing of group 2 was written.

## D3 — A slot is owned by a built-in or a declared extension

- status:   pending
- task:     2.2
- commit:   3952f63a
- trigger:  Hosting `architect` in the slot map needed a key for an extension no plugin carries: the map was keyed by a plugin's `ExtensionIdentity`, and a discovery drops every identity it no longer finds.
- decided:  An `Owner` enum (`Builtin(&'static str)` or `Declared(ExtensionIdentity)`) keys slots, engines and places. The built-ins are hosted when the map is made and never dropped by a discovery; their switch key is their catalog id. `Hosted`'s compile, data and inspection calls do nothing by default, so a built-in implements it empty. The tab strip keeps a built-in's own door; only declared views are listed as a plugin's.
- rejected: A synthetic `ExtensionIdentity` for each built-in: discovery, attribution and the inspector would each need to tell it apart anyway, by a string.
- blast:    `declared_open`, `declared_view` and `DeclaredExtensions` now hold built-ins too; their names say less than they do until 5.1 renames them.

## D4 — The contract grew beyond H4 to carry architect whole

- status:   pending
- task:     2.0–2.2
- commit:   3e38b6b5, 82d0626f, 3952f63a
- trigger:  Moving `architect` showed four more things the host did for it by hand: telling it which checkout it opens on, giving it a board's room rather than a list's, shutting its open list on a press that hit nothing, and letting its drawings go on close; and its read needs where the project declares its artifacts.
- decided:  `Extension` gained `open(surface, Checkout)`, `layout(surface)`, `missed(surface, space)` and `close(surface)`, each doing nothing by default; `scroll` takes the room the surface is drawn in, as every other gesture does; `Host` gained `artifact_source`, which the workspace answers from `workspace.artifacts`. A declared engine answers `scroll_to` itself, so the host no longer walks a held scrollbar a row at a time.
- rejected: `hit(surface, Option<ViewHit>)` for the missed press: it would change every engine's signature for one case.
- blast:    One behaviour moved: architect's content scrollbar now follows a held drag, as every other surface's does, where before a press jumped once and the drag was dropped.
