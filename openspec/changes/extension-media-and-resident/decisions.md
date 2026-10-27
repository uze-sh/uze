<!-- Decisions taken while implementing, one entry each, judged at archive. -->

## D1 — The media nodes are leaves with their data in props; a sprite is declared in the manifest

- status:   pending
- task:     1.1
- commit:
- trigger:  M1 names the nodes, not their shapes.
- decided:  `Cells { rows, regions }` (rows of `{ch, role}` or plain strings; regions `{x, y, w, h, action, item}`), `Image "<path>" alt= height=`, `Sprite "<id>" state= height=`. A sprite is a manifest declaration, `sprite "<id>" sheet= frame="WxH" columns= fallback= { state "<name>" frames="0-3,6" fps= loop= }`, so its sheet is a digested file the check can measure, and a view only names it.
- rejected: A sprite's sheet and states in the view: a view is bound to data, and a sheet is a file the extension ships.
- blast:    `uze-contract::{catalog, manifest}`.

## D2 — The check measures a sprite's sheet from its PNG header

- status:   pending
- task:     1.2
- commit:
- trigger:  "frames dividing it, states naming frames that exist" needs the sheet's size, and the check reads at most a file's head (256 bytes) besides its text.
- decided:  `png_size` reads the IHDR from the head the check already has: the frame size must divide the sheet, `columns` fit across it, and every state's frames lie inside `columns × rows`. A sheet that is not a PNG is refused. `fps` is 1–30, a state at most 256 frames, a frame side 1–1024 px.
- rejected: Decoding the sheet in the check: a decoder in `uze-core` for a size.
- blast:    `manifest/sprite.rs`, `extension.rs::sheet_holds_its_frames`.

## D3 — A resident source is a script kind of its own, never answered per question

- status:   pending
- task:     1.2
- commit:
- trigger:  M4: a resident script keeps its scope; `SourceDecl::script()` feeds the per-question runner.
- decided:  `SourceKind::Resident(file)` under `resident`; `script()` returns `None` for it and `resident()` names its file, so nothing that runs a script per question can run it by mistake; its file is checked and digested as a script.
- rejected: `script "x.rhai" resident=#true`: one flag away from running a resident script per question.
- blast:    `uze-contract::SourceKind`, the decoder, the runner.
