## 1. Make room: Markdown moves to `shared/`

- [ ] 1.1 Move `code/markdown.rs` and `code/highlight.rs` to
      `shared/markdown.rs` and `shared/highlight.rs`, update `shared.rs`'s
      doc with why they joined it, and keep every code-surface test green
      with no behaviour change.

## 2. The catalog: dialects as data

- [ ] 2.1 `spec/dialect.rs`: `Dialect`, `Collection`, `Unit`, `Order`,
      `Subject`, `Role` and the one-wildcard `Pattern`, with pattern
      matching unit-tested (single segment, `**`, no match, first match
      wins, fallback to `Other`).
- [ ] 2.2 The OpenSpec entry: marker `openspec/`, changes (directories,
      skipping `archive`), specs (`**/spec.md` by capability path),
      archive (newest first), roles proposal/design/tasks/`specs/**/spec.md`.
- [ ] 2.3 `spec/catalog.rs`: detect every dialect whose marker exists at a
      checkout root through `Host::list_dir`, walk its collections into
      units and artifacts with roles, order artifacts by role, list
      unnamed files as `Other`, bound the walk's depth, and skip hidden
      entries.
- [ ] 2.4 A test-only Kiro entry and fixture tree, read through the same
      path as OpenSpec, proving a second dialect is data: changes subject
      only, no specs, no archive, its own file names.
- [ ] 2.5 Fixture tests for OpenSpec: a change with all four roles, a
      change with an unnamed file, a change from a schema that names none
      of the files, `changes/` holding only `archive/`, and no marker at
      all.

## 3. Progress and ownership

- [ ] 3.1 `spec/progress.rs`: count `- [ ]`/`- [x]` (and `*`, `X`) at any
      indentation outside fenced code; no count when there are no
      checkboxes; complete when every one is checked. Counted for changes
      in flight only.
- [ ] 3.2 `spec/ownership.rs`: map the paths from `git status --porcelain`
      and, when a target is given, `git diff --name-only
      $(merge-base HEAD <target>)..HEAD` onto units by the dialect's
      collection prefixes. Tested against a real temporary repository:
      uncommitted file, committed since base, operator checkout with no
      target.

## 4. The surface

- [ ] 4.1 `spec.rs`: `CATALOG` entry, `SpecView` state, `SpecAnswer`
      (checkout, units, ownership) and `read_spec(host, checkout, target)`;
      register it in `ExtensionRegistry::builtin` and add
      `ExtensionHit::Spec`.
- [ ] 4.2 `view()`: `Layout::Sidebar`, subjects Changes / Specs / Archive
      (a subject no dialect fills is not offered), one navigator group per
      unit with its progress as a trailing marker and its own-checkout
      mark, rendered Markdown or source by mode, the checkout caption from
      `shared::checkout`, and the three empty states (reading, no layout
      found naming the known ones, nothing in flight).
- [ ] 4.3 `handle_command` / `handle_mouse` / `handle_scroll` over the
      existing `Command` and `ViewHit` vocabulary, and `SpecOutcome`
      (`Stay`, `Close`, `OpenPath`) for activation.
- [ ] 4.4 `SpecPlace` by unit and artifact name, `place()` and
      `resuming()`, with a test that an archived change is not restored.
- [ ] 4.5 Selection on first open: the first own-checkout unit, else the
      first unit; own-checkout units listed first.

## 5. Host wiring

- [ ] 5.1 `crates/uze-theme`: `Symbol::Spec` in the three shipped glyph
      sets; regenerate the theme schema.
- [ ] 5.2 `crates/uze-keys`: `Scope::Spec`, `toggle-spec`, bindings in the
      workspace, code, architect and spec scopes on a free chord that
      passes the keymap's conflict test; affordances in
      `tests/architecture`.
- [ ] 5.3 Orchestrator: `open_spec` / `close_spec` (closing the other
      lenses the way they close each other), `spawn_spec_read` /
      `absorb_spec` on a thread with the checkout carried in the answer
      and a stale answer dropped, the target passed from
      `delivery_policy(..).target`, and `spec_places` remembered per
      checkout beside `architect_places`.
- [ ] 5.4 Tab strip: the spec button in the extension group after `code`,
      lit while open; `SpecOutcome::OpenPath` opens the code surface at
      the path, as `ArchitectOutcome::OpenPath` does.
- [ ] 5.5 Orchestrator tests: the button opens and lights the surface, a
      key typed while open does not reach the pane, a late answer after
      close is dropped, reopening restores the place.

## 6. Docs and gate

- [ ] 6.1 `docs/architecture/containers.mmd`: the TUI container's
      description names the three lenses; run `cargo test -p
      uze-extensions` and `uze agent artifacts check`.
- [ ] 6.2 `crates/uze-extensions/src/lib.rs` crate doc: three extensions,
      and what the spec one is.
- [ ] 6.3 User docs: a spec section in
      `web/content/docs/workspace/extensions.mdx` beside the architect
      one, saying which layouts are read; the new key in
      `configuration/keys.mdx`; the term in `reference/glossary.mdx`.
- [ ] 6.4 `make check` green (fmt, clippy `--all-targets`, workspace tests,
      deny, artifacts).
- [ ] 6.5 Hand validation on this repository: open the surface in an
      agent worktree that is editing a change, and in the operator's
      checkout.
