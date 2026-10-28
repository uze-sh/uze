# Decisions — extension-components

## D1 — Blocks are tags alone on their lines, ruled in `uze-contract`

- status:   pending
- task:     1.1
- commit:
- trigger:  A block must be found without parsing either language. The core, the highlighter (in `uze-extensions`), `uze-script`'s fixture suites and the language server all need the same rule, and the first two may reach only `uze-contract`.
- decided:  `<view>`, `</view>`, `<script>`, `</script>`, each alone on its line and unindented; trailing whitespace, a CR before the newline and a BOM at the file's start are accepted. Each block at most once, in either order, and at least one. Outside them only blank lines and `//` comments; a `/* */` comment is refused with the reason. A line inside a block that is exactly a closing tag closes it, even inside a multi-line string: such a string must not hold that line. The rules are plain data in `uze_contract::component`, the one reader every crate uses.
- rejected: `--- view ---` separators: a stray separator in a script would split it. Attributes such as `lang=`: one language per block. A reader in `uze-core`: the highlighter and `uze-script` cannot reach it.
- blast:    `uze-contract/src/component.rs`.

## D2 — Each block is read from the file with everything else blanked

- status:   pending
- task:     1.2, 1.3
- commit:
- trigger:  View spans are stored and placed later against the file's whole text (`checked.rs::place`, the inspector's trace), and Rhai positions come from compile, runtime and nested errors alike; shifting each by the block's start would miss some.
- decided:  Each compile reads a copy of the whole file in which every byte outside its block, tag lines included, is a space and every newline is kept. Every offset, node span, `locate` answer, Rhai position and scan position is then the file's own, with no shifting anywhere. A `.uze` file is bounded as a script is (256 KiB); its `<view>` block's own bytes as a view is (64 KiB).
- rejected: Shifting `Report` by the block's start: stored spans and Rhai's nested positions would stay block-relative and point at the wrong line.
- blast:    `bounded.rs` (the bound counts the block), `view.rs`, `uze-script/src/script.rs`.

## D3 — One way to write it: `.kdl` views and `.rhai` scripts are refused

- status:   pending
- task:     2.1, 4.1–4.4
- commit:
- trigger:  The platform is unreleased (PR #309); two ways to write a view would be two ways to document, check and highlight forever.
- decided:  The manifest refuses a `view` ending in `.kdl` and a `script` or `resident` ending in `.rhai`, suggesting the `.uze` name. Every fixture, journey, test, the scaffold, the generated reference and the sandbox move in this change. Seven of the gallery's fifteen extensions have no script, so their views become files of one block: the tags are ceremony there, accepted for one uniform shape.
- rejected: Accepting both during a transition nobody needs: no extension outside this repository exists yet.
- blast:    `manifest.rs`, `manifest/source.rs`, `manifest/action.rs`, the gallery, journeys 15–29.

## D4 — `call` runs a named function, needs consent, and never enters a resident

- status:   pending
- task:     2.2, 2.3
- commit:
- trigger:  The timesheet's month buttons were two files of three lines. A review found that `needs_consent` matches `Script | Run` with `matches!`, so a new variant would run code without consent and still compile; and that a call into a resident's script would run without the session's `this`.
- decided:  `action "<id>" on="<surface>" { call "<fn>" in="<file>.uze"; }` runs `<fn>(input, context)` of that file's `<script>`; without `in=`, of the file of the surface the action is `on`, so a function beside a shared source is reachable too. It applies its envelope as a script's answer is applied. `Does::Call { function, file }` is handled in `needs_consent`, `then`, the confirm text, the declared engine's action paths and the runner (D10 made it the only script action). A `call` into a script some source runs `resident` is refused, and so are `answer` and the resident entry names. What the action may reach is still the manifest's grants.
- rejected: Naming the file always (`call "panel.uze#prev"`): the surface already names it in the common case. Running a call inside a resident session: a second way to feed a session that its own `input` already is.
- blast:    `uze-contract::Does::Call`, `consent.rs::needs_consent`, `manifest/action.rs`, `uze-extensions` `declared/action.rs`, the runner.

## D5 — Script-only components; a script's role decides what it must define

- status:   pending
- task:     2.4
- commit:
- trigger:  `Script::compile` refuses a script without `answer`, so a `<script>` holding only `prev` and `next` could never run. A source that feeds two surfaces has logic and no view of its own.
- decided:  A script is compiled without an entry requirement; what it must define is checked by its role: `answer(input, context)` for a source, the resident entries for a `resident`, the named function for a `call`. A `.uze` may hold only `<script>`. A source may name the `.uze` of a surface whose script answers it. A resident surface still names a view, so a game's file holds its placeholder `<view>` and its `<script>`.
- rejected: Keeping `.rhai` for script-only files: two suffixes for scripts again.
- blast:    `uze-script/src/script.rs`, the runner's script cache.

## D6 — The manifest and the tests stay KDL

- status:   pending
- task:     2.1
- commit:
- trigger:  The operator: `extension.kdl` needs no new wrapper; it is one language already.
- decided:  `extension.kdl` and `tests/*.test.kdl` are unchanged.
- rejected: An `<extension>` block in `.uze`: a wrapper around a file that holds one language.
- blast:    none.

## D7 — Highlighting treats tag lines as fences

- status:   pending
- task:     3.2
- commit:
- trigger:  The operator: only UZE's own highlighting matters now. The editor highlights line by line as a stream, and the highlighter already switches grammar at a Markdown fence.
- decided:  For `.uze`, the highlighter uses `uze_contract::component`'s tag-line rule as a fence: lines after `<view>` in KDL, after `<script>` in UZE's Rhai grammar, the tag lines themselves as markup. No TextMate, tree-sitter or editor package is written.
- rejected: A combined syntect grammar with embeds: a second reader of the block rules.
- blast:    `uze-extensions/src/shared/highlight.rs`.

## D8 — The check reaches every script and scans only its block

- status:   pending
- task:     3.1
- commit:
- trigger:  The grant scan lists only `SourceKind::Script` and `Does::Script`, misses residents, and given a whole `.uze` would read `Text "store"` in a view as a call needing a grant.
- decided:  The check (in `uze-application`, which reaches `uze-script`) compiles every script a source, an action, a `call` or a resident reaches, reporting Rhai syntax errors and a missing function, which is new: a check used to compile no script. It scans only the blanked `<script>` for host functions. A `<script>` nothing reaches is warned about.
- rejected: Checking the called function in `uze-core`: it does not depend on `uze-script`.
- blast:    `uze-application/src/application/extensions/checked.rs`.

## D9 — The language server knows `.uze`

- status:   pending
- task:     3.3, 4.4
- commit:
- trigger:  `FileKind::of` matches `.kdl` names only, and editors hand the server files by their language.
- decided:  A `.uze` file is a component: inside `<view>` it completes and hovers as a view, inside `<script>` it answers nothing yet, and the check's diagnostics are reported on it. The author reference shows how Helix, Neovim and VS Code associate `.uze` with the server; no editor package is written (D7).
- rejected: Rhai completion inside `<script>`: a separate piece of work.
- blast:    `src/cli/lsp/complete.rs`, `plugins/uze/skills/author/references/extension.md`.

## D10 — `answer` stays a source's entry; an action only `call`s

- status:   pending
- task:     2.2, 2.3
- commit:
- trigger:  The operator asked whether `answer` was a fixed name and what other tools call it. Two independent reviews followed: one from the author's seat, one from the code. Both found `script` and `call` actions interchangeable in the code (one run path, one input, one `then`), no gallery extension or journey with a `script` action, and a `script` action able to run a resident's script outside its session, which the check refused only for `call`. They split on renaming `answer` to `load`.
- decided:  `answer(input, context)` stays the function a source's script defines, named once as `uze_contract::SOURCE_ENTRY`. `answer` is the word the whole protocol uses for what a source returns — a program answers on stdout, `mcp()` answers the tool's result, `then` reads `answer.*` — and SvelteKit's `load` would promise a run-once function without side effects, where this one is asked again and returns store patches, toasts and refreshes. An action runs code only with `call "<fn>"`; `script` in an action is refused, suggesting `call`, and a source's answer is asked again with `refresh`. An envelope a call returns wrongly names the function that returned it.
- rejected: `load` (rename): the wrong lifecycle and a second word for one idea. A source naming its function (`fn=`): two sources sharing a component tell themselves apart by their `input` today; added when an extension needs it. Keeping `script` actions: a second way to run the same thing, which named every action's function `answer`.
- blast:    `uze-contract` (`Does`, `SOURCE_ENTRY`), `manifest/action.rs`, `consent.rs`, `checked.rs`, `runner.rs`, `declared/action.rs`, `envelope.rs`, docs, the LSP.
