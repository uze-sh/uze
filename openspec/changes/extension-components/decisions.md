# Decisions — extension-components

## D1 — Blocks are tags alone on their lines

- status:   pending
- task:     1.1
- commit:
- trigger:  A block must be found without parsing either language, and neither KDL nor Rhai has a line that is exactly `<view>`.
- decided:  `<view>`, `</view>`, `<script>`, `</script>`, each alone on its line, unindented, no attributes. Each block at most once, in either order. Outside them only blank lines and `//` comments; anything else is refused at its line. An unclosed block is refused at its opening tag.
- rejected: `--- view ---` separators: a block's end would be the next separator or the file's, so a stray separator in a script would split it. Attributes such as `lang=`: there is one language per block.
- blast:    `uze-core/src/capability/extension/component.rs`.

## D2 — Each block is read in place, its offset kept

- status:   pending
- task:     1.2
- commit:
- trigger:  An author reads `panel.uze:12:5`, not "line 3 of the view block".
- decided:  The reader hands each compile the block's text and where it starts; the view compile reports through a `Report` over the whole file, shifted by the block's start, and a script's compile and runtime errors add the block's first line. Rhai positions are lines and columns, so the shift is in lines.
- rejected: Blanking everything outside the block and parsing the whole file: it works for KDL, but Rhai would read `<script>` as code.
- blast:    `document.rs::Report`, `uze-script/src/script.rs`.

## D3 — One way to write it: `.kdl` views and `.rhai` scripts are refused

- status:   pending
- task:     2.1, 4.1
- commit:
- trigger:  The platform is unreleased (PR #309); two ways to write a view would be two ways to document, check and highlight forever.
- decided:  The manifest refuses a `view` ending in `.kdl` and a `script` or `resident` ending in `.rhai`, suggesting the `.uze` name. Every fixture, journey and example moves in this change.
- rejected: Accepting both during a transition nobody needs: no extension outside this repository exists yet.
- blast:    `manifest.rs`, `manifest/source.rs`, `manifest/action.rs`, every gallery fixture.

## D4 — `call` runs a function of the surface's own script

- status:   pending
- task:     2.2, 2.3
- commit:
- trigger:  The timesheet's month buttons were two files of three lines; a button's logic belongs beside the view that draws it.
- decided:  `action "<id>" on="<surface>" { call "<function>"; }` runs `<function>(input, context)` of the `<script>` in that surface's `.uze` file, as an action `script` runs `answer`. The check refuses a `call` whose file has no `<script>` or no function of that name taking two parameters. What the action may reach is still its manifest's grants; `call` adds none.
- rejected: Naming the file in the call (`call "panel.uze#prev"`): the surface already names it.
- blast:    `uze-contract::Does::Call`, `manifest/action.rs`, `uze-script::Script::call`, the runner.

## D5 — A script with no view is a `.uze` with only `<script>`

- status:   pending
- task:     2.1
- commit:
- trigger:  A source that feeds two surfaces, or a resident game, has logic and no view of its own.
- decided:  Such a file holds only `<script>`. A source may also name the `.uze` of a surface whose script answers it, as the timesheet's `sheet` names `panel.uze`.
- rejected: Keeping `.rhai` for script-only files: two suffixes for scripts again.
- blast:    none beyond D3.

## D6 — The manifest and the tests stay KDL

- status:   pending
- task:     2.1
- commit:
- trigger:  The operator: `extension.kdl` needs no new wrapper; it is one language already.
- decided:  `extension.kdl` and `tests/*.test.kdl` are unchanged.
- rejected: An `<extension>` block in `.uze`: a wrapper around a file that holds one language.
- blast:    none.

## D7 — Highlighting is UZE's own

- status:   pending
- task:     3.1
- commit:
- trigger:  The operator: only UZE's own highlighting matters now; grammars for other editors can come later as official extensions.
- decided:  UZE's highlighter splits a `.uze` with the block reader and highlights each block with its language's grammar (KDL, and the Rhai grammar UZE ships). No TextMate, tree-sitter or editor package is written.
- rejected: A combined syntect grammar with embeds: a second reader of the block rules, which could disagree with the first.
- blast:    `uze-extensions/src/shared/highlight.rs`.
