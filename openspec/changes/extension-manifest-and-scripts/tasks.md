## 1. Manifest and check

- [x] 1.1 `uze-contract`: `SourceDecl { id, kind: Script | Run | Fixture, input, every, refresh_on }`, `ActionDecl { id, label, on, key, confirm, does: Script | Refresh | Clipboard | OpenPath | Open }`, `Grant`, `EventName`, and `Manifest.wants`/`actions`; `timeline` in `RESERVED_IDS` and the 32-character bound on ids, with tests (parent 3.1, 3.4)
- [x] 1.2 `uze-core` decoder: the full manifest with the existing refusals (unknown node, repeated property, wrong case, type annotation); `preferences` refused naming change 6 (parent 4.2)
- [x] 1.3 The check: relative paths resolving inside the extension; only `read:checkout`, `clipboard` and `open-path` accepted; `run`, `exec:` and later grants refused with the change that brings them; `write:checkout` refused as nonexistent; a `confirm` action with a bare-letter key refused; `every` under 5 s refused; an action or `refresh` naming an unknown source refused (parent 4.3, 4.4)
- [x] 1.4 `uze agent plugin check` reports the new diagnostics; the scaffold template and the `gsd` fixture plugin gain a script source

## 2. Consent record

- [x] 2.1 `state/extensions.json` shape 1 in `uze-workspace`, named in `UzeHome`, `uze_document::Shaped`, reading the current switch file as shape 0: per identity `accepted_grants`, `accepted_digest`, `accepted_at`, `enabled_in`, and the change-5 fields (parent 4.5)
- [x] 2.2 The digest over the loaded directory (bounded read into memory, SHA-256 over sorted paths and bytes), compiled from those bytes (parent 4.8)
- [x] 2.3 The update rule by grant set; a change of plugin or marketplace is a new identity; `uze remove` removes the records (parent 4.6)
- [x] 2.4 The read model in `uze-application`: per extension, its state (fixtures only, needs consent, needs enabling here, enabled, held with the new grants) for a project; `consent`, `enable(project)`, `disable(project)` (parent 4.7)
- [x] 2.5 Refuse consent and enabling when `UZE_AGENT` is set or `launched_by` names a pane, as `uze workspace allow` does (parent 4.9)
- [x] 2.6 The checkout for extensions: a worktree root through `uze-git`, canonical, refusing `/`, a volume root, `$HOME` and ancestors, `$UZE_HOME` and anything above or below, tmp and bare repositories (parent 4.10)

## 3. Keymap

- [x] 3.1 `Action::Extension(ExtensionActionId)` interned in `uze_keys::extension`, named `ext.<plugin>@<marketplace>/<id>.<action>`, label from the manifest, sanitized; `ALL_ACTIONS` unchanged (parent 8.1)
- [x] 3.2 The per-extension table: surface actions before `Scope::Declared`, a suggestion bound only on `Fallthrough`, the operator's file first, a conflict dropping one binding with a `Problem`, an absent extension's name a warning, collisions in `uze doctor` (parent 8.2)
- [x] 3.3 Footer, action index and item menus show extension actions with `chord_for`'s answer or by pointer; disabling drops the table; `press` in `.test.kdl` reaches an extension action (parent 8.3)

## 4. Scripts

- [x] 4.1 Dependency review of `rhai` 1.27 (`default-features = false`, `std` + `sync`; `cargo tree`, `cargo deny`, its `unsafe` sites in the PR) and the crate `uze-script` on `uze-contract` and `uze-platform` only; a layering test keeps `rhai` out of every other crate (parent 24.1)
- [x] 4.2 The engine: `Engine::new_raw`, chosen packages, no modules, no `eval`, strict variables, `print`/`debug` to a trace, the size and depth limits and `on_progress` as the deadline (parent 24.2)
- [x] 4.3 Host functions as grants: `read_checkout`, `list_checkout` (beneath the root, no link followed, `..` refused) and `clock`; the native primitives `lines`, `glob`, `regex`, `json`, `front_matter`, `kdl`; the check refuses a script naming a function its grants do not register (parent 24.3, 25.4)
- [x] 4.4 Compile on first use, cached by digest; `answer(input, context)` returning the envelope, validated (parent 24.4, 25.3)
- [x] 4.5 `Host::answer(Question { identity, declaration, input }) -> Answer` in `uze-extensions`, implemented in `src/ui/extension_host.rs` through the `ExtensionRunner` of `uze-application`, which resolves the declaration from the consented compile, checks the checkout and answers scripts and fixtures; the session cache by (identity, source, checkout, input digest); an architecture test that only `declared/` calls `answer` (parent 6.1, 25.1)
- [x] 4.6 Extension jobs on a pool of two threads with a serial lane per extension (parent 25.2)

## 5. Sources and actions in the declared engine

- [x] 5.1 Sources: inputs resolved, jobs emitted, answers absorbed with staleness by question; a failed or stopped script said on the surface (parent 6.3)
- [x] 5.2 Actions: buttons and item menus run the declared action; the footer; confirmation with the resolved arguments, attributed; `refresh`, `open`, `clipboard` (toast shows the text; over one line or 256 bytes asks) and `open-path` inside the checkout, the last two from a gesture only; the envelope's `data`, `refresh`, `toast`, `notice` applied (parent 6.4)
- [x] 5.3 The attribution strip `id · plugin@marketplace` on every third-party `view` and `section`, uncoverable (parent 6.8)
- [x] 5.4 Engine tests from fixture manifests and canned answers: refresh, confirm, stale answer dropped, failure state, an undeclared action recorded (parent 6.7)

## 6. Events

- [x] 6.1 `every` with a 5-second floor, only while a surface showing the source is visible (parent 15.1)
- [x] 6.2 `paths.changed` and per-checkout `checkout.changed` from the one-second watch in `orchestrator/reads.rs`, with a bounded digest of the declared paths' modification times, coalesced in 250 ms (parent 15.2)

## 7. Operator surfaces

- [ ] 7.1 The Extensions screen: built-in and declared extensions with plugin, grants and state for this project; Consent and Enable showing the grants; Disable; the reason a held or refused extension does not run (parent 9.1, 9.2)
- [ ] 7.2 The toast → dialog on first open of a project that declares a plugin whose extension is not enabled there (parent 9.1)
- [ ] 7.3 Declared `view` surfaces in the tab strip and the action index; confirmation dialogs and toasts from an extension attributed (parent 9.3, 9.4)
- [ ] 7.4 `uze status` and `uze inspect <plugin>` report extensions and their state; `uze install` asks on a terminal outside a pane and reports otherwise (parent 10.1, 10.2)
- [ ] 7.5 `uze agent plugin check --run` executes scripts in the engine against a checkout (parent 10.3)

## 8. Docs and proof

- [ ] 8.1 Amend ADR-041 and `.agents/rules/extensions.md` with the texts of parent 11.7; refresh the stale doc comments of `uze-extensions/src/lib.rs` and `registry.rs` (parent 11.6)
- [ ] 8.2 `invariants.md` entries of parent 11.4, each naming its test
- [ ] 8.3 `uze-script` in `crate-layering.mmd` and `containers.mmd`; `make artifacts`
- [ ] 8.4 A minimal `uze:extension` Skill (manifest, a script, grants, the check loop) and the hand-off from `uze:author` (parent 11.1, 11.2)
- [ ] 8.5 A journey: install a plugin from a non-linked marketplace, consent, enable, open, act by key and pointer
- [ ] 8.6 The proof by hand for the operator: the change-4 plugin and `gsd-band`'s board from `board.rhai` with `paths.changed` and `every` (parent 12.2)
