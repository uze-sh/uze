## 1. Contract and check

- [x] 1.1 `uze-contract`: `NodeName::{Cells, Image, Sprite}` with their props; `SpriteDecl { id, sheet, frame, columns, states, fallback }`; `SourceKind::Resident(file)`; `Grant::Resident`; tests
- [x] 1.2 `uze-core` decoder and check: cells and regions bound to declared actions; an image path inside the extension or `scratch:`; a sprite's sheet inside the extension, frames dividing it, states naming frames that exist; `resident` under its grant and only for scripts

## 2. Media in the host

- [x] 2.1 Move the decoders from `code/picture.rs` to `shared/picture.rs`; the code surface unchanged
- [x] 2.2 `Cells` rendered from roles, regions as hits; `Image` and `Sprite` laid out in every slot
- [x] 2.3 The painter holds one placement per slot (R-A8); decodes on the pool, one at a time per extension, under R-S12's bounds; the block fallback
- [x] 2.4 Sprites play on the host's clock from their state field

## 3. Resident tier

- [x] 3.1 `uze-script`: a resident session on a thread of its own: `start`, `event`, `input`, `render`, `stop`, scope kept, bounds per call; `timer(ms)`
- [x] 3.2 The engine and runner: a resident source opens and closes with its surfaces' visibility; renders asked at most once per frame and 30 a second; a breach stops it naming the bound; a late render keeps the last frame
- [x] 3.3 Raw keys and pointer to a focused resident surface, except `esc`, F1 and F8; the extensions rule amended (parent 18.4)

## 4. Gallery

- [ ] 4.1 Fixtures `pets` (band, sprite by agent state, store), `screen` (section, image on `every` from a program's scratch, `open` to an overlay), `snake` (view, resident, cells, raw keys, timer, store), each with tests

## 5. Proof and docs

- [ ] 5.1 The extensions rule, invariants, diagrams, the `uze:extension` skill
- [ ] 5.2 A journey: `snake` played with raw keys, and its score kept
- [ ] 5.3 `make check`, `openspec validate --all --strict`, and the hand proof script
