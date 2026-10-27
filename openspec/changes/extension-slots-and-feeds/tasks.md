## 1. Manifest and check

- [x] 1.1 `uze-contract`: `Slot::{Status, Band, Overlay}` with a band's `rows` (1–6) and `place`; `Output::View`; `Envelope::{notify, sound}`; `Grant::Notify`; `PreferenceDecl { name, label, kind: Text | Number { min, max } | Toggle | Choice { values }, default }`; `Deliver::{Latest, Each}` on a `refresh-on`; the `events` source kind; tests
- [x] 1.2 `uze-core` decoder and check: the slots and their properties, `preferences { … }` (refused until now as "arrives in change 6"), `output "view"`, `deliver`, `events <n>`, `notify` accepted; an `open` action naming an overlay; a band's rows out of range refused; `{pref.<name>}` in a view checked against the declared names
- [x] 1.3 The binding scope `pref` in `uze-contract::binding` and the engine (parent 14.11)

## 2. Preferences

- [x] 2.1 `state/extensions/preferences/<plugin@marketplace>/<id>.json` in `UzeHome` (record), read leniently against the declaration, written atomically
- [x] 2.2 `Extensions::{preferences, set_preference}` in `uze-application`; `input.preferences` for a source declaring it; `{pref.*}` handed to the engine with the data
- [x] 2.3 The Extensions drawer: a "preferences" section, Enter and click edit (field dialog for text and number, flip, cycle), with TestBackend tests that click equals key

## 3. Compose

- [x] 3.1 The tree decoder in `uze-extensions::declared`: JSON → the engine's built `View`, against the catalog's `NodeSpec`s and the node cap, every string cleaned, failures by path (parent 13.3)
- [x] 3.2 Interactive composed nodes bound to declared actions with their `item`; the 300 ms re-layout guard; memoized per (answer digest, size)
- [x] 3.3 `output "view"` in the runner for scripts and programs, and in the check (`--run` validates the tree)

## 4. Slots

- [x] 4.1 `status`: rows in the sidebar's header from the slot path, folded; the order in the client layout; Move up and Move down on the Extensions screen
- [x] 4.2 `band`: rows taken in `orchestrator/render.rs::compute_layout` before the PTY size; drawn per agent pane of an enabled project; mouse and selection offsets; a test that no resize follows an answer
- [x] 4.3 `overlay`: opened by an `open` action or the index, centered, attributed, Esc closes, focus restored
- [x] 4.4 Every new slot through the generic slot path (no per-slot field in the orchestrator beyond geometry), and the consent screen naming a band's rows

## 5. Notify and sound

- [x] 5.1 `uze_platform::notify`: desktop tool by path, OSC 9 by `TERM_PROGRAM`, the bell; one implementation per platform; unit tests over the choice (parent 14.10)
- [x] 5.2 The envelope's `notify` and `sound` applied from the frame loop under `notify`, attributed, rate-limited (1 per 10 s, 30 per hour per extension), the excess counted in the notice

## 6. Events delivered

- [x] 6.1 `input.event` for a source asked by an event; `deliver "each"` with its window bound and `dropped`
- [x] 6.2 The `events` source: the last N events of the extension, in memory
- [x] 6.3 A TestBackend test that `Activity`, `Elapsed` and segmented `Progress` move while no run happens (parent 13.5)

## 7. Proof and docs

- [x] 7.1 Fixtures: `gpu-meter` with a `status` surface and a `warn-at` preference; `gsd-band` with a 2-row `band` composed by its script; `notes` with an `overlay` picker; the probe asking for `notify` and `sound`; their `.test.kdl` and snapshots
- [x] 7.2 The extensions rule, invariants (bands never resize a pane; compose validates before drawing; notifications are attributed and rate-limited), diagrams, the `uze:extension` skill
- [ ] 7.3 A journey: a band and a status row appear once enabled, and a preference changes what they draw
- [ ] 7.4 `make check`, `openspec validate --all --strict`, and the hand proof script
