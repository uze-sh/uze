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

## D10 — A picture is asked of the host by identity and name, never by path

- status:   pending
- task:     2.2
- commit:   2890ddc8
- trigger:  M2 says where an image comes from, not how the engine reaches the extension's directory without being handed a path.
- decided:  `Host::media(identity, name)`, asked only by the declared engine's media jobs: the runner finds `name` in the installed extension's loaded files, or `scratch:<file>` in what its programs' runs kept, and reads no further than `IMAGE_FILE_MAX` and a byte. The engine refuses a name that leaves the extension (absolute, `..`, a backslash) or its scratch (a `/`), and any but a PNG, JPEG or GIF, before asking.
- rejected: The extension's directory in the compiled extension or the `Host`: a path an extension holds is a path it can name anything beneath.
- blast:    `uze_extensions::Host`, `declared/media.rs`, `ExtensionRunner::media`, `EngineHost`.

## D11 — A run's scratch keeps its pictures, copied out as it ends

- status:   pending
- task:     2.2
- commit:   2890ddc8
- trigger:  The runner removed a run's scratch directory with everything in it, so `scratch:` had nothing to read.
- decided:  As a program's run ends, up to 16 plain PNG, JPEG or GIF files at the top of its scratch, none past 8 MiB and no link, are copied (whole, through a `.partial` and a rename) to `runtime/extensions/pictures/<plugin@marketplace>/<id>/`, each replacing the one of its name; generated, since the next run writes them again. Every answer of a source marks the `scratch:` pictures held to be read again, drawn as they were until the new read lands. A check (`--run`) keeps none.
- rejected: Keeping the whole scratch until the next run: a program's temporary files are not its views' to read.
- blast:    `UzeHome::extension_pictures_dir`, `extensions/program.rs`.

## D12 — The 32 MiB are the decoded bytes one extension holds at once

- status:   pending
- task:     2.3
- commit:   2890ddc8
- trigger:  R-S12 names 32 MiB "of pixels" per extension, not whether per image or in all, nor in pixels or bytes.
- decided:  RGBA bytes held by one extension's pictures together: a read gets what is left once what no build names any more is let go, and the decoder refuses a header asking past it before allocating ("its pictures would hold over 32 MiB"). The bound reaches every decoder through a thread-local set for one decode (`shared::picture::decode_within`), so the BMP and ICO readers carry no new argument. One read is out per extension at a time.
- rejected: A bound per image: a view of a hundred 30 MiB images would hold three gigabytes.
- blast:    `shared/picture.rs`, `declared/media.rs`.

## D13 — One placement per slot, collected while the frame is drawn

- status:   pending
- task:     2.3
- commit:   2890ddc8
- trigger:  R-A8: several images a frame, keyed by slot; the tree painter that draws them knows no slot, and its callers (six slots, four section sites) return different shapes.
- decided:  Each slot's drawing runs inside `graphics::in_slot(slot id, …)`; drawing a picture `place`s it for that slot, the first one a slot draws winning; the loop takes what the frame placed, keeps what is still uncovered, and the `Painter` keeps what it sent apart per slot, its one encoder taking the latest request of each slot. A second picture in one slot, and one cut at a window's edge, is drawn in half blocks only. The code surface's picture goes the same way, and `Rendered::picture` is gone.
- rejected: A placements list threaded through every render function's return: four shapes changed for one value only the loop reads.
- blast:    `src/ui/graphics.rs`, `extension_view/{picture,tree}.rs`, the slot renderers, `orchestrator.rs`.

## D14 — A sprite's state begins when a build first sees it, and moves at the host's beat

- status:   pending
- task:     2.4
- commit:   2890ddc8
- trigger:  "frame = elapsed × fps" needs when the state began, and the host draws every frame from a tree it does not rebuild.
- decided:  The binder stamps a state with the host's clock at its last pass (`Pass::now`) the first build it plays, and keeps the stamp while later builds play the same state at the same place; the host picks `elapsed × fps` (wrapped, or held on the last frame) from its own clock, and a sprite with a frame still to show asks for the next frame as `Activity` does. Frames advance on the activity beat (120 ms), so a state faster than about 8 fps skips frames rather than speeding the clock. A sprite's few pixels grow in whole pixels (nearest) in Sixel; a picture never past its own size.
- rejected: The engine choosing the frame: it is asked only when its data changes.
- blast:    `view::{sprite_frame, sprite_moves}`, `declared/bind.rs`, `extension_view/tree.rs`, `graphics::Placement::fit_crisp`.

## D15 — What a media node takes when it says nothing, and what a grid holds

- status:   pending
- task:     2.2
- commit:   2890ddc8
- trigger:  The catalog leaves `height` optional and bounds neither a grid nor its regions.
- decided:  An `Image` takes 8 rows without `height`, a `Sprite` half its frame's height in rows (1 to 8), neither more than 64; both keep their rows while read, refused or drawn, so nothing moves when the picture lands (its first row says the `alt`, or why it was refused, in the warning hue). A grid draws at most 200 rows of 400 cells and binds 1,000 regions, its rows counting against the node cap; a control character or one not one column wide is a blank, and a role it does not know, a colour value among them, is the default. A region under the pointer brightens to the accent.
- rejected: An image as tall as its picture fitted to the width: the row count would change when it is read.
- blast:    `declared/bind.rs`, `extension_view/tree.rs`.

## D20 — What a resident script keeps, it keeps on `this`

- status:   pending
- task:     3.1
- commit:   688b49be
- trigger:  M4 says the scope is kept between calls; a Rhai function sees no variable outside itself, so a kept `Scope` alone holds nothing an entry point can read.
- decided:  The session runs the script's top level once, at `start`, in a `Scope` it keeps, and calls every entry point with `this` bound to one map it keeps (`this.score += 1`). `this` is where a resident script's state lives; the top level's constants are what the scope keeps.
- rejected: Passing the state in and out as an argument and a member of the answer: every call would copy the whole game across the boundary twice.
- blast:    `uze-script::resident`.

## D21 — Only `render(size)` is required; the slot rides in `start`'s context

- status:   pending
- task:     3.1
- commit:   688b49be
- trigger:  D19 names `render(slot, size)`; the task names `render(size)`; and a script that only draws has nothing to start, hear or stop.
- decided:  `render(size)` (`#{ width, height }`) is the one entry point a resident script must define. `start(context)` (with `slot`, `locale`, `size`, `checkout`, `extension`, `source`), `event(event)`, `input(input)` and `stop()` are optional: a missing `start` answers a redraw, a missing `event` or `input` none.
- rejected: `render(slot, size)`: a session renders for one surface, whose slot does not change while it runs.
- blast:    `uze-script::resident`, the runner's start context.

## D22 — `start`, `event` and `input` answer a bool, or a map with `redraw` beside the envelope's members

- status:   pending
- task:     3.1
- commit:   688b49be
- trigger:  "answer whether to redraw" leaves a resident script no way to keep a score in its store, toast or notify, which `snake` (4.1) needs.
- decided:  `()` or a bool says whether to redraw; a map says it as `redraw` beside `store`, `toast`, `notice`, `notify`, `sound` and `refresh`, applied as a source's envelope is (the store patch by the runner, the rest by the engine). A map holding `data` is refused: only `render` draws.
- rejected: A host function per effect (`toast()`, `keep()`): a second way to say what the envelope already says.
- blast:    `uze-script::resident::redraw_of`, the engine's `absorb_resident`.

## D23 — `timer(ms)` asks for one tick, held to 34 ms, and rides back on the envelope

- status:   pending
- task:     3.1
- commit:   688b49be
- trigger:  "asks for a tick event at most 30/s" does not say whether a timer repeats, nor how the engine learns of it.
- decided:  `timer(ms)` asks for one tick, `ms` after the call that asked, never sooner than `TIMER_FLOOR` (34 ms: under 30 a second); the last `timer` of a call wins, and a call that asks none leaves no tick. The delay is set by the session on `Envelope::timer` (a field the contract carries, never a member a script may return), and the engine queues `event(#{ name: "timer" })` when it is due. `timer` is a host function under the `resident` grant, so the check refuses it elsewhere.
- rejected: A repeating timer: a script that forgets to stop one keeps the host ticking for nothing; one shot asked again from `event` is one line.
- blast:    `uze-contract::Envelope::timer`, `uze-script::{resident, reach::HOST_FUNCTIONS}`, the engine.

## D24 — The first failed call stops the session, whatever failed

- status:   pending
- task:     3.1
- commit:   688b49be
- trigger:  The spec stops a script past a bound; a script that throws or answers a wrong shape leaves its state as unknown as one stopped mid-loop.
- decided:  Any failed call (a bound, a thrown error, a wrong answer) ends the session's thread; every later call answers that same error, the runner drops the session, and the engine draws the reason (`game.rhai: the script crossed its operations bound at 3:5`) in place of the frame. The surface showing it again starts a new session.
- rejected: Carrying on after a runtime error: the next frame would draw from state the failed call left half-changed.
- blast:    `uze-script::resident`, the runner, `declared/resident.rs`.

## D25 — The session lives in the runner, reached by the source's id through `Host::answer`

- status:   pending
- task:     3.2
- commit:   424f526e, 49449eda
- trigger:  The engine keeps no thread and names no `rhai`; the session needs both and the consent `Host::answer` already judges.
- decided:  `Asking::Resident(ResidentCall)` (`Start`, `Event`, `Input`, `Render`, `Stop`) on the existing `Question`, the call's argument as its input. The runner opens a session on `Start` after the same consent and enabling as a script, keyed by (identity, source), and answers every later call from it without reading the extension again: up to thirty a second must not re-read and re-digest its files. `forget` (consent changed) drops an extension's sessions, and the engine's `reconsider` and `replace` close and reopen them. Every call runs as a job on the extension's serial lane, so they arrive in order and none on the drawing thread.
- rejected: A `Host::resident` method beside `answer`: a second door the architecture suite would have to guard. The session in `uze-extensions`: it would name `rhai` and spawn a thread.
- blast:    `uze-contract::{Asking, ResidentCall}`, `uze-application::extensions::runner`, `uze-extensions::declared::resident`.

## D26 — A drawn frame is the engine drawing a surface; the first render waits for one too

- status:   pending
- task:     3.2
- commit:   49449eda
- trigger:  "at most once per drawn frame" needs the engine to know a frame was drawn, which only the host knows.
- decided:  Every `view` and `section` the host asks of the engine counts a frame. A render is asked only when a call answered a redraw, a frame was counted since the last render was asked (the first included, so it knows its size), and a thirtieth of a second passed on the host's clock; at the size the surface was last drawn at. Raw input waiting for a busy session is held up to 64 calls; what arrives past them is dropped.
- rejected: A frame counter in the contract (`Extension::drawn`): one more method every extension carries for the one that needs it.
- blast:    `uze-extensions::declared::{resident, view, section}`.

## D27 — One session per source, on the checkout its first surface in view is about

- status:   pending
- task:     3.2
- commit:   49449eda
- trigger:  A source shown by a view and a section at once, or on two checkouts, would otherwise run two games.
- decided:  The engine opens one session per resident source, on the checkout `shown_sources` picks (the open view's first), for the first surface in view showing it; a different checkout stops it and starts a new one. A session started again drops the last one's failure and keeps its last frame until its first render.
- rejected: A session per surface: two states for one source, which every other source never has.
- blast:    `uze-extensions::declared::resident`.

## D28 — Esc lets go of a resident surface, as it does a docked drawer; a press takes it back

- status:   pending
- task:     3.3
- commit:   33b23f47
- trigger:  M5: "esc unfocuses it"; a view or an overlay already closes on Esc, and the surface has no other way to be focused again.
- decided:  A resident view in the pane or a resident overlay reads raw input while nothing stands over it (the index, a question, a picker, a menu, the drawer holding the keys). Esc (with any modifier) lets go of it, and the next Esc reaches the host and puts it away, as the docked drawer's two Escs do; F1 and F8 (with any modifier) always reach the host. A left press inside its content takes it back and is handed over relative to the content's top left; opening it again focuses it. Keys are spelled `{key, mods}` from the `Chord` (`src/ui/keys.rs` alone names a `KeyCode`).
- rejected: Esc closing it at once: a game left by accident loses its state.
- blast:    `src/ui/orchestrator/{declared/raw.rs, session/keys.rs, session/mouse.rs}`, `extension_view::Rendered::content_area`.

## D29 — A resident script's `store()` is the store as its session started

- status:   pending
- task:     3.2
- commit:   424f526e
- trigger:  The session's reach is built once, at `Start`, and the store is read into it then.
- decided:  `store()` answers what the store held when the session started; what its calls keep is written at once, and read by `store()` from the next session on. A resident script keeps what it is playing on `this`.
- rejected: Reading the store on every call: a file read per call at thirty a second.
- blast:    `uze-application::extensions::runner::start_resident`.

## D40 — The gallery is one plugin, `media`, with three extensions

- status:   pending
- task:     4.1
- commit:
- trigger:  The task leaves one plugin or three to the implementer.
- decided:  `tests/_fixtures/canonical/media` carries `pets`, `screen` and `snake`, as `bridge` carries the bridge's: one plugin the fixture gate (`every_fixture_plugins_extensions_check_clean`) checks, one marketplace entry a test stages. The journey stages only `snake`'s files, inline, since a container mounts only `journeys/` and a PNG cannot be written as YAML text.
- rejected: Three plugins: three manifests and three gate entries for no difference a reader of the gallery sees.
- blast:    `tests/_fixtures/canonical/media/`, `tests/packages/authoring.rs`.

## D41 — `screen`'s program is real, writes its PNG with the shell's own `printf`, and is proven confined

- status:   pending
- task:     4.1
- commit:
- trigger:  "a packaged `sh` program writing a PNG, or a script-side fixture if programs can't run in tests".
- decided:  `bin/shot` is a `#!/bin/sh` program (`exec:sh`) standing in for `adb exec-out screencap -p`: it writes a 36×64 home screen, embedded as octal escapes, into `$TMPDIR/shot.png` with the `printf` builtin alone, so nothing else needs a grant or the confinement's leave, and answers `{device, screen}`. The lifecycle suite installs `media`, consents, enables and asks the source through the confining runner, then reads `scratch:shot.png` back through `ExtensionRunner::media` (`the_screen_program_leaves_a_picture_its_views_read_from_scratch`, skipped where the machine does not confine, as the GPU meter's is). Its `.test.kdl` gives the source a fixture; a test reads no picture, so the image draws its `alt`.
- rejected: A script-side fixture: it would prove nothing of a run's scratch being kept for its views (D11).
- blast:    `media/extensions/screen/`, `tests/lifecycle/extension_programs.rs`.

## D42 — `snake` plays the same game every time, and its test draws the frame its script answers

- status:   pending
- task:     4.1
- commit:
- trigger:  A plugin check runs no script and keeps no session, and a journey has to score without guessing where the food is.
- decided:  The board is 20×10, two columns a square; the snake waits for an arrow, its first meal sits three squares above its head, so the first `up` scores, and the next come from a fixed linear congruential sequence. A tick is 150 ms (`timer`, asked again from `event` while it plays); a new best is written to the store from the `event` that made it (D22), and `start` reads the last best (D29). `crates/uze-script/tests/media_fixtures.rs` plays it through `Resident` (the first frame, a score, the wall, a replay, 200 ticks of turns), and holds `fixtures/start.json`, the tree the `.test.kdl` gives its source, equal to what the script's first `render` answers.
- rejected: Food from `clock()`: a journey could not know where to steer, and a test could not hold the fixture to the script.
- blast:    `media/extensions/snake/`, `crates/uze-script/tests/media_fixtures.rs`.

## D43 — `pets`' band draws every agent's pet, each playing its own agent's state

- status:   pending
- task:     4.1
- commit:
- trigger:  "a band over each agent's pane, a `Sprite` whose state follows the agent's state": a band is one tree drawn over every pane, and nothing binds it to the agent of the pane it stands over.
- decided:  The band lists the `agents` source with `each`, a `Sprite "pet" state="{state}"` beside each agent's label, so every pane shows every agent's pet; the sheet's states are the agents' (`ready`, `working`, `done`, the fallback `ready`). The store keeps the pet's name, how often it was fed and when (a `feed` action's script), and a `kennel` section shows it `content` for ten minutes after a meal and `hungry` after, two more states of the same sheet. Its 64×32 sheet holds eight 16-pixel frames (291 bytes).
- rejected: A per-pane binding (`context.agent` in a band): new engine behaviour the gallery would invent for itself; it is a change of the band's, if the gallery shows it is wanted.
- blast:    `media/extensions/pets/`.
