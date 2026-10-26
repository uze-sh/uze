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

## D5 — A built-in's section keeps its own place and behaviour

- status:   decided by the operator
- task:     3.2
- commit:   e9abb401, c67dbb4e
- trigger:  The spec summary is not a generic section: it sits at the sidebar's foot on the timeline, takes up to half the column (a plugin's a third), its header is a handle on the timeline, its fold is the client's, and it is recounted every five seconds for the checkout in front against the branch the host evaluated.
- decided:  The operator asked that it keep exactly what it does today, seen and used. Its slot is hosted, but the sidebar draws it in its own place with its own budget, fold and handle, as before; the engine is told the checkout in front and its target on every pass (`Extension::focus`, `Checkout::target`) and recounts on the same cadence; a row sends the view to its change (`Outcome::Show`), which wins over the place last left; the row of the change on show is the extension's to say (`Section::lit`). The view's read asks the host for the delivery target, which needs the home (`Host::delivery_target`, answered by `EngineHost`). The code timeline (task 4) follows the same rule.
- rejected: One rule for every section (budget and handle), which would change how either the spec or a plugin's section looks.
- blast:    The redraw after a count is no longer skipped when it found nothing new: one frame every five seconds while an agent's checkout is in front.

## D6 — The code surface is entered, and its timeline asks the host for a commit

- status:   pending
- task:     4.1–4.2
- commit:   969c813a, 9a15e328, d906f727
- trigger:  Moving `code` showed what the host still did for it by hand: opening it through one of two doors, or onto a path another surface pointed at; sealing the keys while a file or a new name takes text, and pasting into it; keeping its file writes in order; saying a write that failed after the surface closed; holding a press for a drag only where something drags; and opening a commit's account beside the timeline row it was clicked on. The timeline was read with the tab strip's changes badge.
- decided:  `Extension` gained `enter(surface, Entry)` (`Entry::Door` or `Entry::Path`), `typing`, `paste` and `drags`; `absorb` answers an `Outcome`, and `orphaned` went, since the extension knows whether the surface it read for is still open; `Outcome` gained `Failed` (a toast) and `OpenCommit` (the timeline's popup, which stays the host's chrome, as `OpenPath` leads to the code surface). The order of the files half is kept by the extension, which runs its jobs in parallel and holds a read or write until the one before it answers. The timeline is the extension's section, read for the checkout in front as spec's summary is (D5); the badge is the host's own read of the summary alone.
- rejected: A `Hosted` method only the code engine answers (the doors, the commit): it would keep a built-in's shape in the host. A status-segment slot for the badge: the parent change's, not this one's.
- blast:    Two behaviours moved: a letter typed while a file is being renamed in the tree now reaches the name, where it was dropped before; and the wheel or a drag past the edge stops at the last line on every view, not only on code. The badge and the timeline are two reads now, each on its own cadence, where the timeline rode on every fourth badge read. The `tui.code_*` spans are `tui.engine_jobs`.

